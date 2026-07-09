<script>
  export let activeTab = null;

  $: title = activeTab
    ? `${activeTab.dirty ? "● " : ""}${activeTab.name}${
        activeTab.path ? " — " + activeTab.path : ""
      }`
    : "Lumen 文本编辑器";
</script>

<div class="titlebar" data-tauri-drag-region>
  <div class="logo"></div>
  <div class="name">Lumen</div>
  <div class="sep">·</div>
  <div class="filetitle">{title}</div>
</div>

<style>
  .titlebar {
    height: 36px;
    background: var(--bg-panel);
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    padding: 0 12px;
    flex-shrink: 0;
    -webkit-user-select: none;
    user-select: none;
  }
  .logo {
    width: 15px;
    height: 15px;
    border-radius: 4px;
    background: linear-gradient(135deg, #63a4ff, var(--accent));
    margin-right: 8px;
    flex-shrink: 0;
  }
  .name {
    font-size: 12.5px;
    font-weight: 700;
    color: var(--ink);
    letter-spacing: 0.2px;
  }
  .sep {
    margin: 0 8px;
    color: var(--ink-faint);
  }
  .filetitle {
    font-size: 12px;
    color: var(--ink-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
