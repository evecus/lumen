<script>
  import { tick, createEventDispatcher } from "svelte";
  import { showToast } from "../toastStore";

  export let editorPaneRef; // EditorPane 组件实例，暴露了 applySearchQuery/goToNextMatch 等方法

  const dispatch = createEventDispatcher();

  let mode = "find"; // 'find' | 'replace'
  let findText = "";
  let replaceText = "";

  let backward = false; // 反向查找：勾选后"查找下一个"实际走 findPrevious
  let matchCase = false;
  let wholeWord = false;
  let wrapAround = true; // CodeMirror 的查找本身就是循环的，这里保留选项只为贴近 Notepad++ 习惯，暂不单独控制
  let searchMode = "normal"; // 'normal' | 'extended' | 'regex'

  let findInputEl;
  let panelEl;

  function expandEscapes(str) {
    return str.replace(/\\(n|r|t|0|x[0-9a-fA-F]{2})/g, (_, code) => {
      switch (code[0]) {
        case "n":
          return "\n";
        case "r":
          return "\r";
        case "t":
          return "\t";
        case "0":
          return "\0";
        default:
          if (code.startsWith("x")) {
            return String.fromCharCode(parseInt(code.slice(1), 16));
          }
          return code;
      }
    });
  }

  /**
   * 根据当前三种模式（普通 / 扩展转义 / 正则）把用户输入转换成
   * 最终要交给 CodeMirror SearchQuery 的 search 字符串和 regexp 标志。
   * 普通/扩展模式下 CodeMirror 走字面量匹配（regexp: false），
   * 扩展模式只是在字面量匹配前，先把 \n \t 等转义序列替换成真实字符。
   */
  function buildQueryParams() {
    if (searchMode === "regex") {
      return { search: findText, regexp: true };
    }
    if (searchMode === "extended") {
      return { search: expandEscapes(findText), regexp: false };
    }
    return { search: findText, regexp: false };
  }

  let matchCount = 0;

  function syncQuery() {
    if (!editorPaneRef) return;
    const { search, regexp } = buildQueryParams();
    editorPaneRef.applySearchQuery({
      search,
      replace: replaceText,
      caseSensitive: matchCase,
      regexp,
      wholeWord,
    });
    matchCount = findText ? editorPaneRef.countMatches() : 0;
  }

  // 查找条件的任一部分变化时，都重新同步一次 CodeMirror 的查询状态并刷新计数/高亮
  $: findText, replaceText, matchCase, wholeWord, searchMode, syncQuery();

  function handleNext() {
    if (!editorPaneRef || !findText) return;
    syncQuery();
    if (backward) {
      editorPaneRef.goToPreviousMatch();
    } else {
      editorPaneRef.goToNextMatch();
    }
  }

  function handlePrevious() {
    if (!editorPaneRef || !findText) return;
    syncQuery();
    if (backward) {
      editorPaneRef.goToNextMatch();
    } else {
      editorPaneRef.goToPreviousMatch();
    }
  }

  function handleCount() {
    if (!findText) {
      showToast("请输入查找内容", "info");
      return;
    }
    syncQuery();
    showToast(`共找到 ${matchCount} 处匹配`, "info");
  }

  function handleReplace() {
    if (!editorPaneRef || !findText) return;
    syncQuery();
    editorPaneRef.replaceCurrentMatch();
    syncQuery();
  }

  function handleReplaceAll() {
    if (!editorPaneRef || !findText) {
      showToast("请输入查找内容", "info");
      return;
    }
    syncQuery();
    const before = matchCount;
    editorPaneRef.replaceAllMatches();
    syncQuery();
    showToast(`已替换 ${before} 处`, "success");
  }

  export function open(withReplace) {
    mode = withReplace ? "replace" : "find";
    tick().then(() => {
      if (findInputEl) {
        findInputEl.focus();
        findInputEl.select();
      }
      syncQuery();
    });
  }

  function close() {
    dispatch("close");
  }

  function handleKeydown(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      if (e.shiftKey) {
        handlePrevious();
      } else {
        handleNext();
      }
    } else if (e.key === "F3") {
      e.preventDefault();
      e.stopPropagation();
      if (e.shiftKey) handlePrevious();
      else handleNext();
    }
  }
</script>

<div class="search-panel" bind:this={panelEl} on:keydown={handleKeydown}>
  <div class="sp-close" on:click={close} title="关闭 (Esc)">✕</div>

  <div class="sp-tabs">
    <button class:active={mode === "find"} on:click={() => (mode = "find")}>查找</button>
    <button class:active={mode === "replace"} on:click={() => (mode = "replace")}>替换</button>
  </div>

  <div class="sp-row">
    <label class="sp-label" for="sp-find-input">查找目标：</label>
    <input
      id="sp-find-input"
      type="text"
      bind:this={findInputEl}
      bind:value={findText}
      placeholder="输入要查找的内容…"
    />
  </div>

  {#if mode === "replace"}
    <div class="sp-row">
      <label class="sp-label" for="sp-replace-input">替换为：</label>
      <input id="sp-replace-input" type="text" bind:value={replaceText} placeholder="输入替换内容…" />
    </div>
  {/if}

  <div class="sp-checks">
    <label><input type="checkbox" bind:checked={backward} /> 反向查找</label>
    <label><input type="checkbox" bind:checked={wholeWord} /> 全词匹配</label>
    <label><input type="checkbox" bind:checked={matchCase} /> 匹配大小写</label>
    <label><input type="checkbox" bind:checked={wrapAround} /> 循环查找</label>
  </div>

  <div class="sp-mode-group">
    <div class="sp-mode-label">查找模式</div>
    <label><input type="radio" bind:group={searchMode} value="normal" /> 普通</label>
    <label>
      <input type="radio" bind:group={searchMode} value="extended" />
      扩展 (\n,\r,\t,\0,\x…)
    </label>
    <label><input type="radio" bind:group={searchMode} value="regex" /> 正则表达式</label>
  </div>

  <div class="sp-status">
    {#if findText}
      {matchCount} 处匹配
    {/if}
  </div>

  <div class="sp-actions">
    <button class="sp-btn" on:click={handlePrevious}>↑ 查找上一个</button>
    <button class="sp-btn" on:click={handleNext}>↓ 查找下一个</button>
    <button class="sp-btn" on:click={handleCount}>计数</button>
  </div>

  {#if mode === "replace"}
    <div class="sp-actions">
      <button class="sp-btn" on:click={handleReplace}>替换</button>
      <button class="sp-btn primary" on:click={handleReplaceAll}>全部替换</button>
    </div>
  {/if}
</div>

<style>
  .search-panel {
    position: absolute;
    top: 10px;
    right: 10px;
    width: 330px;
    background: var(--bg-raised, #fff);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    padding: 14px;
    z-index: 40;
  }
  .sp-close {
    position: absolute;
    top: 10px;
    right: 10px;
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--ink-faint);
    cursor: pointer;
    border-radius: var(--radius-sm);
    font-size: 12px;
  }
  .sp-close:hover {
    background: var(--bg-hover);
    color: var(--ink);
  }
  .sp-tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 12px;
    border-bottom: 1px solid var(--border);
    padding-bottom: 8px;
  }
  .sp-tabs button {
    background: transparent;
    border: none;
    padding: 5px 12px;
    border-radius: var(--radius-sm);
    font-size: 12.5px;
    color: var(--ink-dim);
    cursor: pointer;
  }
  .sp-tabs button.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .sp-row {
    margin-bottom: 10px;
  }
  .sp-label {
    display: block;
    font-size: 11.5px;
    color: var(--ink-dim);
    margin-bottom: 4px;
  }
  .sp-row input[type="text"] {
    width: 100%;
    padding: 7px 9px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 12.5px;
    outline: none;
    background: #fff;
    color: var(--ink);
  }
  .sp-row input[type="text"]:focus {
    border-color: var(--accent);
  }
  .sp-checks {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 10px;
  }
  .sp-checks label,
  .sp-mode-group label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--ink);
    cursor: pointer;
  }
  .sp-checks input,
  .sp-mode-group input {
    accent-color: var(--accent);
  }
  .sp-mode-group {
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 10px;
  }
  .sp-mode-label {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--ink-faint);
    font-weight: 700;
    margin-bottom: 2px;
  }
  .sp-status {
    font-size: 11px;
    color: var(--ink-dim);
    min-height: 14px;
    margin-bottom: 8px;
  }
  .sp-actions {
    display: flex;
    gap: 6px;
    margin-bottom: 8px;
  }
  .sp-btn {
    flex: 1;
    padding: 7px 6px;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--ink);
    font-size: 11px;
    cursor: pointer;
  }
  .sp-btn:hover {
    background: var(--bg-hover);
    border-color: var(--border-strong);
  }
  .sp-btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .sp-btn.primary:hover {
    background: var(--accent-hover);
  }
</style>
