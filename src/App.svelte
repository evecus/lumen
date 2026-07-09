<script>
  import { onMount } from "svelte";
  import TitleBar from "./lib/components/TitleBar.svelte";
  import Toolbar from "./lib/components/Toolbar.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import EditorPane from "./lib/components/EditorPane.svelte";
  import SearchPanel from "./lib/components/SearchPanel.svelte";
  import StatusBar from "./lib/components/StatusBar.svelte";
  import SettingsModal from "./lib/components/SettingsModal.svelte";
  import Toast from "./lib/components/Toast.svelte";
  import { tabs, activeTab, createTab } from "./lib/store";
  import { searchPanelOpen, searchPanelMode } from "./lib/searchStore";
  import { openFileByPath, openFileDialog, saveTab, saveTabAs } from "./lib/fileActions";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { get } from "svelte/store";

  let settingsOpen = false;
  let sidebarVisible = true;

  onMount(async () => {
    // 处理程序启动时通过"打开方式"传入的文件路径
    try {
      const launchFile = await invoke("get_launch_file");
      if (launchFile) {
        await openFileByPath(launchFile);
      } else {
        createTab({ name: "未命名-1.txt" });
      }
    } catch (e) {
      createTab({ name: "未命名-1.txt" });
    }

    // 监听已运行实例收到的新文件打开请求
    const unlisten = await listen("open-file-request", async (event) => {
      await openFileByPath(event.payload);
    });

    return () => unlisten();
  });

  function handleKeydown(e) {
    const ctrl = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();

    if (ctrl && key === "n") {
      e.preventDefault();
      createTab({ name: `未命名-${$tabs.length + 1}.txt` });
    } else if (ctrl && key === "o") {
      e.preventDefault();
      openFileDialog();
    } else if (ctrl && key === "s" && e.shiftKey) {
      e.preventDefault();
      saveTabAs(get(activeTab));
    } else if (ctrl && key === "s") {
      e.preventDefault();
      saveTab(get(activeTab));
    } else if (ctrl && key === "f" && !e.shiftKey) {
      e.preventDefault();
      searchPanelMode.set("find");
      searchPanelOpen.set(true);
    } else if (ctrl && key === "h") {
      e.preventDefault();
      searchPanelMode.set("replace");
      searchPanelOpen.set(true);
    } else if (key === "escape") {
      searchPanelOpen.set(false);
    } else if (key === "f3") {
      e.preventDefault();
      // 让查找面板处理，如果面板未打开则先打开
      if (!get(searchPanelOpen)) {
        searchPanelMode.set("find");
        searchPanelOpen.set(true);
      }
    }
  }
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="app-shell">
  <TitleBar activeTab={$activeTab} />
  <Toolbar
    bind:sidebarVisible
    on:open-settings={() => (settingsOpen = true)}
  />
  <div class="body-wrap">
    {#if sidebarVisible}
      <Sidebar />
    {/if}
    <div class="editor-area">
      <EditorPane />
      {#if $searchPanelOpen}
        <SearchPanel />
      {/if}
    </div>
  </div>
  <StatusBar />
</div>

{#if settingsOpen}
  <SettingsModal on:close={() => (settingsOpen = false)} />
{/if}

<Toast />

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }
  .body-wrap {
    flex: 1;
    display: flex;
    overflow: hidden;
    min-height: 0;
  }
  .editor-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    position: relative;
  }
</style>
