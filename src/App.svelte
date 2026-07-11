<script>
  import { onMount } from "svelte";
  import TitleBar from "./lib/components/TitleBar.svelte";
  import Toolbar from "./lib/components/Toolbar.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import EditorPane from "./lib/components/EditorPane.svelte";
  import StatusBar from "./lib/components/StatusBar.svelte";
  import Toast from "./lib/components/Toast.svelte";
  import NewFileDialog from "./lib/components/NewFileDialog.svelte";
  import { tabs, activeTab, createTab } from "./lib/store";
  import { openFileByPath, openFileDialog, saveTab, saveTabAs } from "./lib/fileActions";
  import { setActiveTabExtensionAsDefault } from "./lib/fileAssocActions";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { get } from "svelte/store";

  let sidebarVisible = true;
  let editorPaneRef;
  let newFileDialogOpen = false;

  onMount(async () => {
    // 处理程序启动时通过"打开方式"传入的文件路径
    try {
      const launchFile = await invoke("get_launch_file");
      if (launchFile) {
        await openFileByPath(launchFile);
      } else {
        // 启动时的初始空白标签不弹窗，直接给一个默认名字，
        // 弹窗只在用户主动点"新建"/按 Ctrl+N 时出现。
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

  function openNewFileDialog() {
    newFileDialogOpen = true;
  }

  function handleNewFileConfirm(e) {
    const { name, encoding } = e.detail;
    createTab({ name, encoding: encoding || "UTF-8" });
    newFileDialogOpen = false;
  }

  function handleNewFileCancel() {
    newFileDialogOpen = false;
  }

  // 注意：查找 (Ctrl+F)、替换 (Ctrl+H)、撤销重做等快捷键
  // 现在由 CodeMirror 自己的 keymap 处理（只要编辑器有焦点），
  // 这里的全局快捷键只处理 CodeMirror 管不到的应用级操作：
  // 新建/打开/保存文件、以及切换侧边栏。
  function handleKeydown(e) {
    const ctrl = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();

    if (ctrl && key === "n") {
      e.preventDefault();
      openNewFileDialog();
    } else if (ctrl && key === "o") {
      e.preventDefault();
      openFileDialog();
    } else if (ctrl && key === "s" && e.shiftKey) {
      e.preventDefault();
      saveTabAs(get(activeTab));
    } else if (ctrl && key === "s") {
      e.preventDefault();
      saveTab(get(activeTab));
    }
  }
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="app-shell">
  <TitleBar activeTab={$activeTab} />
  <Toolbar
    bind:sidebarVisible
    activeTab={$activeTab}
    on:new-file={openNewFileDialog}
    on:open-search={() => editorPaneRef && editorPaneRef.focusSearch()}
    on:set-default={() => setActiveTabExtensionAsDefault($activeTab)}
  />
  <div class="body-wrap">
    {#if sidebarVisible}
      <Sidebar on:new-file={openNewFileDialog} />
    {/if}
    <div class="editor-area">
      <EditorPane bind:this={editorPaneRef} />
    </div>
  </div>
  <StatusBar />
</div>

{#if newFileDialogOpen}
  <NewFileDialog
    defaultName={`未命名-${$tabs.length + 1}.txt`}
    on:confirm={handleNewFileConfirm}
    on:cancel={handleNewFileCancel}
  />
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
