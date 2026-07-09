<script>
  import { activeTab } from "../store";

  $: content = $activeTab ? $activeTab.content : "";
  $: charCount = content.length;
  $: lineCount = content.length ? content.split("\n").length : 0;
</script>

<div class="statusbar">
  {#if $activeTab}
    <div class="stat">{$activeTab.encoding || "UTF-8"}</div>
    <div class="stat">{$activeTab.eol || "LF"}</div>
    <div class="stat">{charCount.toLocaleString()} 字符 · {lineCount.toLocaleString()} 行</div>
    <div class="spacer"></div>
    <div class="stat">{$activeTab.path ? $activeTab.path : "未保存到磁盘"}</div>
  {:else}
    <div class="stat">没有打开的文件</div>
  {/if}
</div>

<style>
  .statusbar {
    height: 26px;
    background: var(--accent);
    color: #fff;
    display: flex;
    align-items: center;
    padding: 0 14px;
    font-size: 11px;
    gap: 18px;
    flex-shrink: 0;
  }
  .stat {
    opacity: 0.95;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .spacer {
    flex: 1;
  }
</style>
