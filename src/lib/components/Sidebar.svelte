<script>
  import { tabs, activeTabId, closeTab, setActiveTab, createTab } from "../store";
  import { saveTab } from "../fileActions";

  async function handleClose(e, tab) {
    e.stopPropagation();
    if (tab.dirty) {
      const ok = confirm(`“${tab.name}” 尚有未保存的更改，确定要关闭吗？`);
      if (!ok) return;
    }
    closeTab(tab.id);
  }
</script>

<div class="sidebar">
  <div class="sidebar-head">打开的文件</div>
  <div class="file-list">
    {#each $tabs as tab (tab.id)}
      <div
        class="file-item"
        class:active={tab.id === $activeTabId}
        on:click={() => setActiveTab(tab.id)}
      >
        <span class="ficon">{tab.path ? "📄" : "📝"}</span>
        <span class="fname" title={tab.path || tab.name}>{tab.name}</span>
        {#if tab.dirty}<span class="fdot"></span>{/if}
        <span class="fclose" on:click={(e) => handleClose(e, tab)}>✕</span>
      </div>
    {/each}
  </div>
  <div class="sidebar-foot">
    <button
      class="newfile-btn"
      on:click={() => createTab({ name: `未命名-${$tabs.length + 1}.txt` })}
    >
      + 新建文件
    </button>
  </div>
</div>

<style>
  .sidebar {
    width: 200px;
    background: var(--bg-panel);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }
  .sidebar-head {
    padding: 10px 12px 6px;
    font-size: 10.5px;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--ink-faint);
    font-weight: 700;
  }
  .file-list {
    flex: 1;
    overflow-y: auto;
    padding: 2px 6px;
  }
  .file-item {
    padding: 7px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 12px;
    color: var(--ink-dim);
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 1px;
    white-space: nowrap;
  }
  .file-item:hover {
    background: var(--bg-hover);
    color: var(--ink);
  }
  .file-item.active {
    background: var(--bg-active);
    color: var(--ink);
    font-weight: 600;
  }
  .ficon {
    flex-shrink: 0;
    font-size: 11px;
    opacity: 0.8;
  }
  .fname {
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
  }
  .fdot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warn);
    flex-shrink: 0;
  }
  .fclose {
    width: 16px;
    height: 16px;
    border-radius: 3px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--ink-faint);
    opacity: 0;
    font-size: 11px;
  }
  .file-item:hover .fclose {
    opacity: 1;
  }
  .fclose:hover {
    background: var(--danger);
    color: #fff;
  }
  .sidebar-foot {
    padding: 8px;
    border-top: 1px solid var(--border);
  }
  .newfile-btn {
    width: 100%;
    padding: 8px;
    background: transparent;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-sm);
    color: var(--ink-dim);
    font-size: 11.5px;
    cursor: pointer;
  }
  .newfile-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
