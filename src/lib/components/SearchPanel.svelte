<script>
  import { tick } from "svelte";
  import {
    searchPanelOpen,
    searchPanelMode,
    searchOptions,
    searchStatus,
    findAllMatches,
  } from "../searchStore";
  import { activeTab, updateTabContent } from "../store";
  import { showToast } from "../toastStore";

  let findInput;
  let matches = [];

  $: mode = $searchPanelMode;
  $: opts = $searchOptions;

  // 每当查找条件或文本内容变化时重新计算匹配
  $: if ($activeTab) {
    matches = findAllMatches($activeTab.content, opts);
    searchStatus.update((s) => ({
      ...s,
      totalMatches: matches.length,
      currentIndex: matches.length
        ? Math.min(s.currentIndex, matches.length - 1)
        : -1,
    }));
  }

  function getEditorTextarea() {
    return document.querySelector(".editor-inner textarea");
  }

  function jumpToMatch(index) {
    if (!matches.length) return;
    const m = matches[index];
    const ta = getEditorTextarea();
    if (!ta) return;
    ta.focus();
    ta.setSelectionRange(m.start, m.end);
    const upTo = ta.value.slice(0, m.start);
    const line = upTo.split("\n").length;
    ta.scrollTop = Math.max(0, (line - 6) * 20);
    searchStatus.update((s) => ({ ...s, currentIndex: index }));
  }

  function findNext() {
    if (!matches.length) {
      showToast("未找到匹配项", "info");
      return;
    }
    const backward = opts.backward;
    let idx = $searchStatus.currentIndex;
    idx = backward ? idx - 1 : idx + 1;
    if (idx < 0) {
      if (!opts.wrapAround) {
        showToast("已到达文档开头", "info");
        return;
      }
      idx = matches.length - 1;
    } else if (idx >= matches.length) {
      if (!opts.wrapAround) {
        showToast("已到达文档末尾", "info");
        return;
      }
      idx = 0;
    }
    jumpToMatch(idx);
  }

  function findPrev() {
    searchOptions.update((o) => ({ ...o, backward: true }));
    findNext();
    searchOptions.update((o) => ({ ...o, backward: false }));
  }

  function countMatches() {
    showToast(`共找到 ${matches.length} 处匹配`, "info");
  }

  function replaceOne() {
    if (!$activeTab) return;
    const idx = $searchStatus.currentIndex;
    if (idx < 0 || !matches.length) {
      showToast("请先查找到匹配项", "info");
      return;
    }
    const m = matches[idx];
    const content = $activeTab.content;
    const newContent =
      content.slice(0, m.start) + opts.replaceText + content.slice(m.end);
    updateTabContent($activeTab.id, newContent);
    tick().then(() => {
      const ta = getEditorTextarea();
      if (ta) {
        const newPos = m.start + opts.replaceText.length;
        ta.setSelectionRange(newPos, newPos);
      }
      findNext();
    });
  }

  function replaceAll() {
    if (!$activeTab) return;
    if (!matches.length) {
      showToast("未找到匹配项", "info");
      return;
    }
    let content = $activeTab.content;
    let offset = 0;
    const count = matches.length;
    for (const m of matches) {
      const start = m.start + offset;
      const end = m.end + offset;
      content = content.slice(0, start) + opts.replaceText + content.slice(end);
      offset += opts.replaceText.length - (m.end - m.start);
    }
    updateTabContent($activeTab.id, content);
    showToast(`已替换 ${count} 处`, "success");
  }

  function replaceInSelection() {
    if (!$activeTab) return;
    const ta = getEditorTextarea();
    if (!ta) return;
    const selStart = ta.selectionStart;
    const selEnd = ta.selectionEnd;
    if (selStart === selEnd) {
      showToast("请先在编辑区选中要替换的范围", "info");
      return;
    }
    const inRange = matches.filter((m) => m.start >= selStart && m.end <= selEnd);
    if (!inRange.length) {
      showToast("选区内没有匹配项", "info");
      return;
    }
    let content = $activeTab.content;
    let offset = 0;
    for (const m of inRange) {
      const start = m.start + offset;
      const end = m.end + offset;
      content = content.slice(0, start) + opts.replaceText + content.slice(end);
      offset += opts.replaceText.length - (m.end - m.start);
    }
    updateTabContent($activeTab.id, content);
    showToast(`已在选区内替换 ${inRange.length} 处`, "success");
  }

  function close() {
    searchPanelOpen.set(false);
  }

  function handleKeydown(e) {
    if (e.key === "Escape") close();
    if (e.key === "Enter") {
      e.preventDefault();
      findNext();
    }
  }

  function setMode(m) {
    searchPanelMode.set(m);
  }

  $: if ($searchPanelOpen) {
    tick().then(() => findInput && findInput.focus());
  }
</script>

<div class="search-panel">
  <div class="sp-close" on:click={close} title="关闭 (Esc)">✕</div>

  <div class="sp-tabs">
    <button class:active={mode === "find"} on:click={() => setMode("find")}>查找</button>
    <button class:active={mode === "replace"} on:click={() => setMode("replace")}>替换</button>
  </div>

  <div class="sp-body">
    <div class="sp-field">
      <label for="sp-find">查找目标：</label>
      <input
        id="sp-find"
        bind:this={findInput}
        bind:value={$searchOptions.findText}
        on:keydown={handleKeydown}
        placeholder="输入要查找的内容…"
      />
    </div>

    {#if mode === "replace"}
      <div class="sp-field">
        <label for="sp-replace">替换为：</label>
        <input
          id="sp-replace"
          bind:value={$searchOptions.replaceText}
          on:keydown={handleKeydown}
          placeholder="输入替换内容…"
        />
      </div>
    {/if}

    <div class="sp-checks">
      <label><input type="checkbox" bind:checked={$searchOptions.backward} /> 反向查找</label>
      <label><input type="checkbox" bind:checked={$searchOptions.wholeWord} /> 全词匹配</label>
      <label><input type="checkbox" bind:checked={$searchOptions.matchCase} /> 匹配大小写</label>
      <label><input type="checkbox" bind:checked={$searchOptions.wrapAround} /> 循环查找</label>
    </div>

    <div class="sp-mode-group">
      <div class="sp-mode-label">查找模式</div>
      <label>
        <input type="radio" bind:group={$searchOptions.mode} value="normal" /> 普通
      </label>
      <label>
        <input type="radio" bind:group={$searchOptions.mode} value="extended" />
        扩展 (\n,\r,\t,\0,\x…)
      </label>
      <label>
        <input type="radio" bind:group={$searchOptions.mode} value="regex" /> 正则表达式
      </label>
      {#if opts.mode === "regex"}
        <label class="newline-check">
          <input type="checkbox" bind:checked={$searchOptions.matchNewline} /> . 匹配新行
        </label>
      {/if}
    </div>

    <div class="sp-status">
      {#if matches.length}
        第 {$searchStatus.currentIndex + 1} / {matches.length} 处匹配
      {:else if opts.findText}
        未找到匹配项
      {/if}
    </div>

    <div class="sp-actions">
      <button class="sp-btn" on:click={findPrev}>↑ 查找上一个 (F4)</button>
      <button class="sp-btn" on:click={findNext}>↓ 查找下一个 (F3)</button>
      <button class="sp-btn" on:click={countMatches}>计数</button>
    </div>

    {#if mode === "replace"}
      <div class="sp-actions">
        <button class="sp-btn" on:click={replaceOne}>替换</button>
        <button class="sp-btn" on:click={replaceInSelection}>在选区替换</button>
        <button class="sp-btn primary" on:click={replaceAll}>全部替换</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .search-panel {
    position: absolute;
    top: 10px;
    right: 10px;
    width: 320px;
    background: var(--bg-raised);
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
  .sp-field {
    margin-bottom: 10px;
  }
  .sp-field label {
    display: block;
    font-size: 11.5px;
    color: var(--ink-dim);
    margin-bottom: 4px;
  }
  .sp-field input[type="text"],
  .sp-field input:not([type]) {
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
  .sp-field input:focus {
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
  .newline-check {
    margin-left: 18px;
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
