<script>
  import { createEventDispatcher } from "svelte";
  import { tabs, createTab } from "../store";
  import { openFileDialog, saveTab } from "../fileActions";

  export let sidebarVisible = true;
  export let activeTab = null;

  const dispatch = createEventDispatcher();

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
    on:click={() => saveTab(activeTab)}
  >
    <svg viewBox="0 0 24 24"
      ><path
        d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"
      /><path d="M17 21v-8H7v8M7 3v5h8" /></svg
    >
    保存
  </button>

  <div class="tb-sep"></div>

  <button
    class="tb-btn"
    title="查找 / 替换 (Ctrl+F / Ctrl+H，需先点击编辑区)"
    on:click={() => dispatch("open-search")}
  >
    <svg viewBox="0 0 24 24"><circle cx="11" cy="11" r="7" /><path d="M21 21l-4.3-4.3" /></svg>
    查找/替换
  </button>

  <div class="tb-sep"></div>

  <button
    class="tb-btn"
    title="把当前文件的格式设为 Lumen 默认打开程序"
    on:click={() => dispatch("set-default")}
  >
    <svg viewBox="0 0 24 24"
      ><path d="M20 6L9 17l-5-5" /></svg
    >
    设为默认
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
