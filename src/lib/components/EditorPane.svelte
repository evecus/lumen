<script>
  import { onDestroy } from "svelte";
  import { EditorState, Compartment } from "@codemirror/state";
  import {
    EditorView,
    keymap,
    lineNumbers,
    highlightActiveLine,
    highlightActiveLineGutter,
    drawSelection,
  } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
  import { search, searchKeymap, openSearchPanel } from "@codemirror/search";
  import { activeTab, updateTabContent } from "../store";

  // ===== 为什么换成 CodeMirror 6 =====
  // 原生 <textarea> 没法在软换行（长段落自动折行）时把行号栏和视觉行精确对齐，
  // 也没有真正的视口虚拟化能力。CodeMirror 6 把这些都做成了内置能力：
  //   - lineWrapping 打开后，行号只出现在每个逻辑行的第一屏视觉行，折行部分自动留空
  //   - 内部本身就是虚拟滚动的编辑器内核，大文件不需要我们自己维护可见区间
  //   - 查找替换直接用官方 @codemirror/search 扩展和它自带的搜索面板，
  //     不用自己维护正则匹配、选区计算这些逻辑

  let editorContainer;
  let view = null;
  let currentTabId = null;

  // 用 Compartment 让"是否自动换行"可以在运行时动态切换，
  // 不用销毁重建整个 EditorView。
  const wrapCompartment = new Compartment();

  export let wrapEnabled = true;

  let suppressSync = false;

  function createExtensions(wrap) {
    return [
      lineNumbers(),
      highlightActiveLine(),
      highlightActiveLineGutter(),
      drawSelection(),
      history(),
      search({ top: true }),
      wrapCompartment.of(wrap ? EditorView.lineWrapping : []),
      keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, indentWithTab]),
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
        ".cm-selectionBackground": {
          backgroundColor: "rgba(47, 111, 237, 0.18) !important",
        },
        "&.cm-focused .cm-selectionBackground": {
          backgroundColor: "rgba(47, 111, 237, 0.22) !important",
        },
        ".cm-scroller": {
          fontFamily: "var(--mono)",
          overflow: "auto",
        },
        ".cm-panels": {
          backgroundColor: "var(--bg-panel)",
          color: "var(--ink)",
        },
        ".cm-panels.cm-panels-top": {
          borderBottom: "1px solid var(--border)",
        },
        ".cm-searchMatch": {
          backgroundColor: "rgba(224, 164, 88, 0.35)",
        },
        ".cm-searchMatch.cm-searchMatch-selected": {
          backgroundColor: "rgba(47, 111, 237, 0.35)",
        },
        ".cm-textfield": {
          backgroundColor: "#fff",
          color: "var(--ink)",
          border: "1px solid var(--border-strong)",
          borderRadius: "4px",
        },
        ".cm-button": {
          backgroundImage: "none",
          backgroundColor: "var(--bg-hover)",
          border: "1px solid var(--border)",
          borderRadius: "4px",
          color: "var(--ink)",
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
      extensions: createExtensions(wrapEnabled),
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

  // 自动换行开关：动态重新配置 Compartment，不需要重建整个编辑器
  $: if (view) {
    view.dispatch({
      effects: wrapCompartment.reconfigure(wrapEnabled ? EditorView.lineWrapping : []),
    });
  }

  export function focusSearch() {
    if (view) openSearchPanel(view);
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
