<script>
  import { onMount, tick } from "svelte";
  import { activeTab, updateTabContent } from "../store";
  import { searchStatus } from "../searchStore";

  // ===== 大文件性能策略 =====
  // 原生 <textarea> 本身不支持"只渲染可见行"——它是单一文本节点，无法做真正的
  // 视口虚拟化编辑区域。这里实际采用的策略是：
  //   1. 行号栏（gutter）做虚拟滚动：只渲染视口内的行号 DOM 节点，避免大文件下
  //      生成几十万个 <div>，这是本组件里唯一能安全虚拟化的部分。
  //   2. textarea 内容仍是完整文本（原生输入框的限制），大文件模式下关闭自动换行、
  //      关闭拼写检查，并对行数组的重新计算做防抖，减少每次输入的开销。
  //   3. 超过阈值时给用户一个"大文件模式"提示，管理性能预期。
  // 如果之后需要支持 GB 级文件的真正虚拟化编辑，需要换用 CodeMirror 6 / Monaco
  // 这类内置了视口虚拟化的编辑器内核，而不是原生 textarea。

  const VIRTUALIZE_THRESHOLD = 5000; // 超过此行数进入"大文件模式"
  const LINE_HEIGHT = 20;
  const OVERSCAN = 30;

  let textareaEl;
  let gutterEl;
  let containerEl;

  let totalLines = 1;
  let isLargeFile = false;

  let scrollTop = 0;
  let viewportHeight = 600;
  let cursorLine = 1;
  let cursorCol = 1;
  let selectionLength = 0;

  let debounceTimer = null;
  let currentTabId = null;
  let editValue = "";

  // 切换标签时，把新标签的内容同步进本地可编辑变量；
  // 编辑时（下面的 handleInput）再把 editValue 写回 store。
  // 这样可以用 Svelte 惯用的 bind:value，避免手动操作 textarea DOM。
  // 同时也要处理"同一个标签的内容被外部改动"的情况（例如查找替换面板执行了
  // 全部替换/在选区替换），这时 store 里的 content 变了但不是通过这个组件的
  // 输入触发的，需要把新内容同步回 editValue，否则用户在编辑器里看不到替换结果。
  $: if ($activeTab && $activeTab.id !== currentTabId) {
    currentTabId = $activeTab.id;
    editValue = $activeTab.content;
  } else if (
    $activeTab &&
    $activeTab.content !== editValue &&
    typeof document !== "undefined" &&
    document.activeElement !== textareaEl
  ) {
    // 只有当 textarea 未聚焦时才从外部同步，避免用户正在输入时被覆盖打断
    editValue = $activeTab.content;
  } else if (!$activeTab) {
    currentTabId = null;
    editValue = "";
  }

  $: recomputeLineCount(editValue);

  function recomputeLineCount(text) {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      let count = 1;
      for (let i = 0; i < text.length; i++) if (text.charCodeAt(i) === 10) count++;
      totalLines = count;
      isLargeFile = totalLines > VIRTUALIZE_THRESHOLD;
    }, 60);
  }

  // 只虚拟化行号栏：用总行数 + 滚动位置计算需要渲染的行号范围，
  // 而不是真的把 textarea 内容切片渲染（textarea 做不到这一点）。
  $: startIndex = Math.max(0, Math.floor(scrollTop / LINE_HEIGHT) - OVERSCAN);
  $: visibleCount = Math.ceil(viewportHeight / LINE_HEIGHT) + OVERSCAN * 2;
  $: endIndex = Math.min(totalLines, startIndex + visibleCount);
  $: gutterVisibleRange = Array.from(
    { length: Math.max(0, endIndex - startIndex) },
    (_, i) => startIndex + i + 1
  );
  $: topSpacerHeight = startIndex * LINE_HEIGHT;
  $: bottomSpacerHeight = Math.max(0, (totalLines - endIndex) * LINE_HEIGHT);

  function handleInput() {
    if (!$activeTab) return;
    updateTabContent($activeTab.id, editValue);
  }

  function handleScroll() {
    if (!textareaEl) return;
    scrollTop = textareaEl.scrollTop;
    if (gutterEl) gutterEl.scrollTop = scrollTop;
  }

  function updateCursorInfo() {
    if (!textareaEl) return;
    const val = textareaEl.value;
    const pos = textareaEl.selectionStart;
    const upTo = val.slice(0, pos);
    cursorLine = upTo.split("\n").length;
    cursorCol = pos - upTo.lastIndexOf("\n");
    selectionLength = textareaEl.selectionEnd - textareaEl.selectionStart;
  }

  function handleKeydown(e) {
    if (e.key === "Tab") {
      e.preventDefault();
      const s = textareaEl.selectionStart;
      const en = textareaEl.selectionEnd;
      editValue = editValue.slice(0, s) + "    " + editValue.slice(en);
      handleInput();
      tick().then(() => {
        textareaEl.selectionStart = textareaEl.selectionEnd = s + 4;
      });
    }
  }

  onMount(() => {
    const resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        viewportHeight = entry.contentRect.height;
      }
    });
    if (containerEl) resizeObserver.observe(containerEl);
    return () => resizeObserver.disconnect();
  });

  // 供外部（查找面板）调用：跳转到指定字符偏移并滚动到视图内
  export function scrollToOffset(offset) {
    if (!textareaEl) return;
    textareaEl.focus();
    textareaEl.setSelectionRange(offset, offset);
    const upTo = textareaEl.value.slice(0, offset);
    const line = upTo.split("\n").length;
    textareaEl.scrollTop = Math.max(0, (line - 6) * LINE_HEIGHT);
    handleScroll();
    updateCursorInfo();
  }

  export function selectRange(start, end) {
    if (!textareaEl) return;
    textareaEl.focus();
    textareaEl.setSelectionRange(start, end);
    const upTo = textareaEl.value.slice(0, start);
    const line = upTo.split("\n").length;
    textareaEl.scrollTop = Math.max(0, (line - 6) * LINE_HEIGHT);
    handleScroll();
    updateCursorInfo();
  }

  export function getEditorValue() {
    return editValue;
  }

  export function setEditorValue(newValue, cursorPos) {
    if (!$activeTab) return;
    editValue = newValue;
    updateTabContent($activeTab.id, newValue);
    tick().then(() => {
      if (textareaEl && cursorPos != null) {
        textareaEl.setSelectionRange(cursorPos, cursorPos);
      }
    });
  }

  $: statusText = `行 ${cursorLine}，列 ${cursorCol}`;
</script>

<div class="editor-container" bind:this={containerEl}>
  {#if !$activeTab}
    <div class="empty-state">
      <div class="es-icon">📄</div>
      <div class="es-text">没有打开的文件</div>
      <div class="es-kbd">Ctrl+N 新建 · Ctrl+O 打开</div>
    </div>
  {:else}
    <div class="gutter" bind:this={gutterEl}>
      {#if isLargeFile}
        <div style="height: {topSpacerHeight}px;"></div>
        {#each gutterVisibleRange as lineNum (lineNum)}
          <div class="gline" class:current={lineNum === cursorLine}>{lineNum}</div>
        {/each}
        <div style="height: {bottomSpacerHeight}px;"></div>
      {:else}
        {#each Array(totalLines) as _, i (i)}
          <div class="gline" class:current={i + 1 === cursorLine}>{i + 1}</div>
        {/each}
      {/if}
    </div>
    <div class="editor-inner">
      <textarea
        bind:this={textareaEl}
        bind:value={editValue}
        on:input={handleInput}
        on:scroll={handleScroll}
        on:click={updateCursorInfo}
        on:keyup={updateCursorInfo}
        on:keydown={handleKeydown}
        spellcheck="false"
        class:nowrap={isLargeFile}
        placeholder="开始输入，或使用 Ctrl+O 打开本地文件…"
      ></textarea>
      {#if isLargeFile}
        <div class="large-file-badge" title="大文件模式：行号栏已启用虚拟渲染，并关闭了自动换行以提升性能">
          大文件模式 · {totalLines.toLocaleString()} 行
        </div>
      {/if}
    </div>
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
  .gutter {
    background: var(--bg-editor);
    color: var(--ink-faint);
    font-family: var(--mono);
    font-size: 13px;
    line-height: 20px;
    text-align: right;
    padding: 12px 10px 12px 0;
    user-select: none;
    white-space: pre;
    border-right: 1px solid var(--border);
    flex-shrink: 0;
    overflow: hidden;
    min-width: 48px;
  }
  .gline {
    height: 20px;
  }
  .gline.current {
    color: var(--accent);
    font-weight: 600;
  }
  .editor-inner {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  textarea {
    width: 100%;
    height: 100%;
    resize: none;
    border: none;
    outline: none;
    background: transparent;
    color: var(--ink);
    font-family: var(--mono);
    font-size: 13px;
    line-height: 20px;
    padding: 12px 20px 12px 14px;
    white-space: pre;
    overflow-wrap: normal;
    tab-size: 4;
    caret-color: var(--accent);
  }
  textarea::selection {
    background: rgba(47, 111, 237, 0.18);
  }
  textarea::placeholder {
    color: var(--ink-faint);
  }
  textarea.nowrap {
    white-space: pre;
    overflow-x: auto;
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
  .large-file-badge {
    position: absolute;
    top: 8px;
    right: 16px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    color: var(--ink-dim);
    padding: 3px 10px;
    border-radius: 20px;
    font-size: 10.5px;
    box-shadow: var(--shadow-sm);
  }
</style>
