<script>
  import { createEventDispatcher } from "svelte";
  import { activeTab, tabs, createTab } from "../store";
  import { openFileDialog, saveTab } from "../fileActions";
  import { searchPanelOpen, searchPanelMode } from "../searchStore";

  export let sidebarVisible = true;

  const dispatch = createEventDispatcher();

  function openSearch(mode) {
    searchPanelMode.set(mode);
    searchPanelOpen.set(true);
  }

  function handleNew() {
    createTab({ name: `未命名-${$tabs.length + 1}.txt` });
  }
</script>

<div class="toolbar">
  <button class="tb-btn" title="新建 (Ctrl+N)" on:click={handleNew}>
    <svg viewBox="0 0 24 24"><path d="M12 5v14M5 12h14" /></svg>
    新建
  </button>
  <button class="tb-btn" title="打开文件 (Ctrl+O)" on:click={openFileDialog}>
    <svg viewBox="0 0 24 24"
      ><path d="M3 7l3-3h6l2 2h7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z" /></svg
    >
    打开
  </button>
  <button
    class="tb-btn"
    title="保存 (Ctrl+S)"
    on:click={() => saveTab($activeTab)}
  >
    <svg viewBox="0 0 24 24"
      ><path
        d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"
      /><path d="M17 21v-8H7v8M7 3v5h8" /></svg
    >
    保存
  </button>

  <div class="tb-sep"></div>

  <button class="tb-btn" title="查找 (Ctrl+F)" on:click={() => openSearch("find")}>
    <svg viewBox="0 0 24 24"><circle cx="11" cy="11" r="7" /><path d="M21 21l-4.3-4.3" /></svg>
    查找
  </button>
  <button class="tb-btn" title="替换 (Ctrl+H)" on:click={() => openSearch("replace")}>
    <svg viewBox="0 0 24 24"
      ><path d="M17 3l4 4-4 4M21 7H9M7 21l-4-4 4-4M3 17h12" /></svg
    >
    替换
  </button>

  <div class="spacer"></div>

  <button
    class="tb-btn icon-only"
    title="显示/隐藏侧边栏"
    on:click={() => (sidebarVisible = !sidebarVisible)}
  >
    <svg viewBox="0 0 24 24"
      ><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M9 4v16" /></svg
    >
  </button>
  <button
    class="tb-btn icon-only"
    title="设置"
    on:click={() => dispatch("open-settings")}
  >
    <svg viewBox="0 0 24 24"
      ><circle cx="12" cy="12" r="3" /><path
        d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"
      /></svg
    >
  </button>
</div>

<style>
  .toolbar {
    height: 44px;
    background: var(--bg-app);
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    padding: 0 10px;
    gap: 4px;
    flex-shrink: 0;
  }
  .tb-btn {
    height: 30px;
    padding: 0 12px;
    display: flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--ink-dim);
    cursor: pointer;
    font-size: 12.5px;
  }
  .tb-btn.icon-only {
    padding: 0 8px;
  }
  .tb-btn:hover {
    background: var(--bg-hover);
    color: var(--ink);
    border-color: var(--border);
  }
  .tb-btn svg {
    width: 15px;
    height: 15px;
    stroke: currentColor;
    fill: none;
    stroke-width: 1.8;
    flex-shrink: 0;
  }
  .tb-sep {
    width: 1px;
    height: 20px;
    background: var(--border);
    margin: 0 6px;
  }
  .spacer {
    flex: 1;
  }
</style>
