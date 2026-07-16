<script>
  import { onDestroy } from "svelte";
  import { EditorState } from "@codemirror/state";
  import {
    EditorView,
    keymap,
    lineNumbers,
    highlightActiveLine,
    highlightActiveLineGutter,
  } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
  import {
    search,
    setSearchQuery,
    SearchQuery,
    findNext,
    findPrevious,
    replaceNext,
    replaceAll as cmReplaceAll,
    getSearchQuery,
  } from "@codemirror/search";
  import { activeTab, updateTabContent } from "../store";

  // ===== 为什么换成 CodeMirror 6 =====
  // 原生 <textarea> 没法在软换行（长段落自动折行）时把行号栏和视觉行精确对齐，
  // 也没有真正的视口虚拟化能力。CodeMirror 6 把这些都做成了内置能力：
  //   - lineWrapping 打开后，行号只出现在每个逻辑行的第一屏视觉行，折行部分自动留空
  //   - 内部本身就是虚拟滚动的编辑器内核，大文件不需要我们自己维护可见区间
  //   - 查找替换的底层引擎（正则匹配、增量高亮、大小写/全词选项）用官方
  //     @codemirror/search 模块，但 UI 完全是我们自己写的 SearchPanel.svelte
  //     浮层组件，不使用它自带的面板样式（见下面 createExtensions 里的注释）

  let editorContainer;
  let view = null;
  let currentTabId = null;
  let suppressSync = false;

  function createExtensions() {
    return [
      lineNumbers(),
      highlightActiveLine(),
      highlightActiveLineGutter(),
      history(),
      // 关键点：CodeMirror 的"匹配高亮"视觉效果（.cm-searchMatch）内部实现
      // 依赖 search() 扩展存在且有一个 panel 被创建，如果完全不加载 search()，
      // setSearchQuery/findNext 虽然仍能正确定位/选中匹配项，但不会画出高亮背景。
      // 所以这里必须保留 search() 扩展本身，但通过 createPanel 提供一个
      // "什么都不显示、不占用任何布局空间"的空面板，实际的搜索 UI
      // 完全由我们自己的 SearchPanel.svelte 组件（浮层，不挤压编辑器区域）驱动。
      search({
        createPanel: () => {
          const dom = document.createElement("div");
          dom.style.display = "none";
          return { dom, top: true };
        },
      }),
      // 自动换行永久开启：长段落超出编辑区宽度时自动折行，避免内容显示不全。
      // 软换行时行号栏只在每个逻辑行的第一屏视觉行显示数字，折行部分自动留空，
      // 这是原生 <textarea> 做不到、必须用 CodeMirror 才能正确实现的效果。
      EditorView.lineWrapping,
      keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
      EditorView.updateListener.of((update) => {
        if (update.docChanged && $activeTab) {
          suppressSync = true;
          updateTabContent($activeTab.id, update.state.doc.toString());
          // 微任务后再解除抑制，保证本次 store 更新触发的响应式回写不会
          // 又把同一份内容重新 dispatch 回编辑器造成无意义的循环。
          Promise.resolve().then(() => {
            suppressSync = false;
          });
        }
      }),
      EditorView.theme({
        "&": {
          height: "100%",
          fontSize: "13px",
          backgroundColor: "var(--bg-editor)",
        },
        ".cm-content": {
          fontFamily: "var(--mono)",
          caretColor: "var(--accent)",
          padding: "12px 20px 12px 14px",
        },
        ".cm-content ::selection": {
          backgroundColor: "rgba(47, 111, 237, 0.25)",
        },
        ".cm-line ::selection": {
          backgroundColor: "rgba(47, 111, 237, 0.25)",
        },
        ".cm-gutters": {
          backgroundColor: "var(--bg-editor)",
          color: "var(--ink-faint)",
          border: "none",
          borderRight: "1px solid var(--border)",
        },
        ".cm-activeLineGutter": {
          backgroundColor: "var(--bg-active)",
          color: "var(--accent)",
          fontWeight: "600",
        },
        ".cm-activeLine": {
          backgroundColor: "var(--bg-active)",
        },
        ".cm-scroller": {
          fontFamily: "var(--mono)",
          overflow: "auto",
        },
        ".cm-searchMatch": {
          backgroundColor: "rgba(224, 164, 88, 0.35)",
        },
        ".cm-searchMatch.cm-searchMatch-selected": {
          backgroundColor: "rgba(47, 111, 237, 0.35)",
        },
      }),
    ];
  }

  function mountEditor(content) {
    if (view) {
      view.destroy();
      view = null;
    }
    const state = EditorState.create({
      doc: content,
      extensions: createExtensions(),
    });
    view = new EditorView({
      state,
      parent: editorContainer,
    });
  }

  // 切换标签：重新挂载文档内容（保留各标签独立的撤销历史，这里做简化处理，
  // 每次切换标签都是一次新的 EditorState，避免撤销栈在标签间串台）
  $: if (editorContainer && $activeTab && $activeTab.id !== currentTabId) {
    currentTabId = $activeTab.id;
    mountEditor($activeTab.content);
  } else if (editorContainer && !$activeTab && view) {
    view.destroy();
    view = null;
    currentTabId = null;
  }

  // 外部内容变化（例如以后如果有"重新从磁盘加载"、查找替换等由其他地方触发的
  // 内容更新）时才强制同步回编辑器；如果是编辑器自己刚刚触发的变更（上面的
  // updateListener 设置了 suppressSync），这一轮就跳过，避免无意义的循环 dispatch。
  $: if (
    view &&
    !suppressSync &&
    $activeTab &&
    $activeTab.id === currentTabId &&
    $activeTab.content !== view.state.doc.toString()
  ) {
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: $activeTab.content },
    });
  }

  // ===== 暴露给外部 SearchPanel 组件使用的搜索接口 =====
  // 这里只做"转发"：真正的查找/替换逻辑仍然是 CodeMirror 官方的
  // @codemirror/search 模块实现，我们只是不用它自带的 UI 面板。

  export function getView() {
    return view;
  }

  /**
   * 根据自定义面板收集到的选项构建一个 CodeMirror SearchQuery，并把它设置为
   * 当前编辑器的搜索状态（这样匹配高亮、后续的 findNext/findPrevious 才能生效）。
   */
  export function applySearchQuery({
    search: searchText,
    replace,
    caseSensitive,
    regexp,
    wholeWord,
  }) {
    if (!view) return null;
    const query = new SearchQuery({
      search: searchText,
      replace: replace ?? "",
      caseSensitive: !!caseSensitive,
      regexp: !!regexp,
      wholeWord: !!wholeWord,
    });
    view.dispatch({ effects: setSearchQuery.of(query) });
    return query;
  }

  export function goToNextMatch() {
    if (view) findNext(view);
  }

  export function goToPreviousMatch() {
    if (view) findPrevious(view);
  }

  export function replaceCurrentMatch() {
    if (view) replaceNext(view);
  }

  export function replaceAllMatches() {
    if (view) cmReplaceAll(view);
  }

  /**
   * 统计当前查询在整份文档里一共有多少处匹配。
   * CodeMirror 的 SearchCursor 是惰性遍历的，这里手动跑一遍数出总数，
   * 用于面板里的"计数"按钮和"第 X / 共 Y 处"提示。
   */
  export function countMatches() {
    if (!view) return 0;
    const query = getSearchQuery(view.state);
    if (!query || !query.search) return 0;
    const cursor = query.getCursor(view.state.doc);
    let count = 0;
    let guard = 0;
    while (!cursor.next().done && guard < 200000) {
      count++;
      guard++;
    }
    return count;
  }

  export function focusEditor() {
    if (view) view.focus();
  }

  export function getEditorValue() {
    return view ? view.state.doc.toString() : "";
  }

  onDestroy(() => {
    if (view) view.destroy();
  });
</script>

<div class="editor-container">
  {#if !$activeTab}
    <div class="empty-state">
      <div class="es-icon">📄</div>
      <div class="es-text">没有打开的文件</div>
      <div class="es-kbd">Ctrl+N 新建 · Ctrl+O 打开</div>
    </div>
  {:else}
    <div class="cm-host" bind:this={editorContainer}></div>
  {/if}
</div>

<style>
  .editor-container {
    flex: 1;
    display: flex;
    overflow: hidden;
    background: var(--bg-editor);
    position: relative;
    min-height: 0;
  }
  .cm-host {
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }
  :global(.cm-editor) {
    height: 100%;
  }
  .empty-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    color: var(--ink-faint);
    gap: 10px;
  }
  .es-icon {
    font-size: 34px;
    opacity: 0.4;
  }
  .es-text {
    font-size: 13px;
    color: var(--ink-dim);
  }
  .es-kbd {
    font-size: 11px;
    font-family: var(--mono);
  }
</style>
