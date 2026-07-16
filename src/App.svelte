<script>
  import { onMount, tick } from "svelte";
  import TitleBar from "./lib/components/TitleBar.svelte";
  import Toolbar from "./lib/components/Toolbar.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import EditorPane from "./lib/components/EditorPane.svelte";
  import StatusBar from "./lib/components/StatusBar.svelte";
  import Toast from "./lib/components/Toast.svelte";
  import NewFileDialog from "./lib/components/NewFileDialog.svelte";
  import SearchPanel from "./lib/components/SearchPanel.svelte";
  import { tabs, activeTab, createTab } from "./lib/store";
  import { openFileByPath, openFileDialog, saveTab, saveTabAs } from "./lib/fileActions";
  import { setActiveTabExtensionAsDefault } from "./lib/fileAssocActions";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { get } from "svelte/store";

  let sidebarVisible = true;
  let editorPaneRef;
  let searchPanelRef;
  let newFileDialogOpen = false;
  let searchPanelVisible = false;

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

  function openSearch(withReplace) {
    searchPanelVisible = true;
    tick().then(() => {
      if (searchPanelRef) searchPanelRef.open(withReplace);
    });
  }

  function closeSearch() {
    searchPanelVisible = false;
    if (editorPaneRef) editorPaneRef.focusEditor();
  }

  // 注意：撤销/重做等基础编辑快捷键由 CodeMirror 自己的 keymap 处理
  // （只要编辑器有焦点）。查找 (Ctrl+F) / 替换 (Ctrl+H) 现在由我们自己的
  // SearchPanel 组件接管（因为不再使用 CodeMirror 自带的搜索面板/keymap），
  // 所以在这里的全局快捷键里处理。
  //
  // "新建文件"弹窗打开时，不响应这些全局快捷键：用户在文件名输入框里
  // 打字时如果恰好按住 Ctrl 敲了 f/h/n/o/s 之类的字母（不算罕见的操作失误），
  // 不应该意外弹出查找面板或触发保存，弹窗本身的 Enter/Escape 由它自己处理。
  function handleKeydown(e) {
    if (newFileDialogOpen) return;

    const ctrl = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();

    // 查找面板打开时：
    //   - 放行 Ctrl+F / Ctrl+H（切换查找/替换 tab 或重新聚焦）、Escape（关闭面板）
    //   - 放行 Ctrl+S（保存是静默的后台操作，不抢焦点，不会打断查找）
    //   - 屏蔽 Ctrl+N / Ctrl+O（这两个会弹出新窗口/新建标签，抢走查找框焦点，
    //     很可能是用户在查找输入框里打字时手滑误触，不应该响应）
    if (searchPanelVisible) {
      if (ctrl && key === "f") {
        e.preventDefault();
        openSearch(false);
      } else if (ctrl && key === "h") {
        e.preventDefault();
        openSearch(true);
      } else if (key === "escape") {
        closeSearch();
      } else if (ctrl && key === "s" && e.shiftKey) {
        e.preventDefault();
        saveTabAs(get(activeTab));
      } else if (ctrl && key === "s") {
        e.preventDefault();
        saveTab(get(activeTab));
      }
      return;
    }

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
    } else if (ctrl && key === "f") {
      e.preventDefault();
      openSearch(false);
    } else if (ctrl && key === "h") {
      e.preventDefault();
      openSearch(true);
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
    on:open-search={() => openSearch(false)}
    on:set-default={() => setActiveTabExtensionAsDefault($activeTab)}
  />
  <div class="body-wrap">
    {#if sidebarVisible}
      <Sidebar on:new-file={openNewFileDialog} />
    {/if}
    <div class="editor-area">
      <EditorPane bind:this={editorPaneRef} />
      {#if searchPanelVisible}
        <SearchPanel bind:this={searchPanelRef} {editorPaneRef} on:close={closeSearch} />
      {/if}
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
