<script>
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { showToast } from "../toastStore";

  const dispatch = createEventDispatcher();

  // 常见的文本/代码文件扩展名，分组展示，用户按需勾选
  const extensionGroups = [
    {
      label: "纯文本",
      exts: ["txt", "log", "md"],
    },
    {
      label: "Web 前端",
      exts: ["html", "htm", "css", "js", "jsx", "ts", "tsx", "json"],
    },
    {
      label: "后端 / 系统语言",
      exts: ["py", "java", "c", "cpp", "h", "cs", "go", "rs", "php", "rb"],
    },
    {
      label: "配置 / 数据",
      exts: ["ini", "conf", "yml", "yaml", "xml", "toml", "env"],
    },
    {
      label: "脚本",
      exts: ["sh", "bat", "ps1"],
    },
  ];

  let status = {}; // { ext: boolean }
  let loading = true;
  let platformSupported = true;

  onMount(async () => {
    const allExts = extensionGroups.flatMap((g) => g.exts);
    try {
      for (const ext of allExts) {
        status[ext] = await invoke("is_extension_registered", { ext });
      }
      status = { ...status };
    } catch (e) {
      platformSupported = false;
    }
    loading = false;
  });

  async function toggle(ext) {
    const willRegister = !status[ext];
    try {
      if (willRegister) {
        await invoke("register_file_extension", { ext });
        showToast(`已将 .${ext} 加入 Lumen 的可选打开方式`, "success");
      } else {
        await invoke("unregister_file_extension", { ext });
        showToast(`已取消 .${ext} 的关联`, "info");
      }
      status[ext] = willRegister;
      status = { ...status };
    } catch (e) {
      showToast(`操作失败：${e}`, "error");
    }
  }

  async function setDefault(ext) {
    try {
      await invoke("set_default_file_extension", { ext });
      status[ext] = true;
      status = { ...status };
      showToast(`已将 Lumen 设为 .${ext} 的默认打开程序`, "success");
    } catch (e) {
      showToast(`设置失败：${e}`, "error");
    }
  }

  function close() {
    dispatch("close");
  }
</script>

<div class="overlay" on:click|self={close}>
  <div class="modal">
    <div class="modal-head">
      <h2>设置</h2>
      <div class="modal-close" on:click={close}>✕</div>
    </div>
    <div class="modal-body">
      {#if !platformSupported}
        <div class="notice warn">
          文件关联功能目前仅支持 Windows。当前平台无法设置默认打开方式。
        </div>
      {:else}
        <div class="notice">
          勾选的扩展名会把 Lumen 加入该类型文件的“打开方式”列表，不会强制替换你已有的默认程序。
          点击“设为默认”可以直接将 Lumen 设为该扩展名的默认打开程序。部分改动可能需要重新登录或重启资源管理器后才会在右键菜单中完全生效。
        </div>

        {#if loading}
          <div class="loading">正在读取当前关联状态…</div>
        {:else}
          {#each extensionGroups as group}
            <div class="group">
              <div class="group-label">{group.label}</div>
              <div class="ext-grid">
                {#each group.exts as ext}
                  <div class="ext-row">
                    <label class="ext-check">
                      <input
                        type="checkbox"
                        checked={status[ext]}
                        on:change={() => toggle(ext)}
                      />
                      <span class="ext-name">.{ext}</span>
                    </label>
                    <button class="default-btn" on:click={() => setDefault(ext)}>
                      设为默认
                    </button>
                  </div>
                {/each}
              </div>
            </div>
          {/each}
        {/if}
      {/if}
    </div>
    <div class="modal-foot">
      <button class="btn primary" on:click={close}>完成</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(20, 24, 33, 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal {
    width: 600px;
    max-height: 82vh;
    background: #fff;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .modal-head {
    padding: 18px 22px 14px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
  }
  .modal-head h2 {
    font-size: 16px;
    font-weight: 700;
    margin: 0;
    color: var(--ink);
  }
  .modal-close {
    margin-left: auto;
    width: 26px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--ink-dim);
    cursor: pointer;
    border-radius: var(--radius-sm);
  }
  .modal-close:hover {
    background: var(--bg-hover);
  }
  .modal-body {
    padding: 18px 22px;
    overflow-y: auto;
    flex: 1;
  }
  .notice {
    background: var(--bg-accent-soft);
    border: 1px solid #cfe1fb;
    color: #2c5aa8;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    font-size: 12px;
    line-height: 1.6;
    margin-bottom: 16px;
  }
  .notice.warn {
    background: #fff7ea;
    border-color: #f2dca8;
    color: #8a6416;
  }
  .loading {
    color: var(--ink-dim);
    font-size: 12.5px;
    padding: 20px 0;
    text-align: center;
  }
  .group {
    margin-bottom: 16px;
  }
  .group-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--ink-faint);
    font-weight: 700;
    margin-bottom: 8px;
  }
  .ext-grid {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ext-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
  }
  .ext-row:hover {
    background: var(--bg-panel);
  }
  .ext-check {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    flex: 1;
  }
  .ext-check input {
    accent-color: var(--accent);
    width: 15px;
    height: 15px;
  }
  .ext-name {
    font-family: var(--mono);
    font-size: 12.5px;
    color: var(--ink);
  }
  .default-btn {
    padding: 4px 10px;
    background: transparent;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    color: var(--ink-dim);
    font-size: 11px;
    cursor: pointer;
  }
  .default-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .modal-foot {
    padding: 14px 22px;
    border-top: 1px solid var(--border);
    display: flex;
    justify-content: flex-end;
  }
  .btn {
    padding: 8px 18px;
    border-radius: var(--radius-sm);
    font-size: 12.5px;
    cursor: pointer;
    border: 1px solid var(--border);
    background: var(--bg-panel);
    color: var(--ink);
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .btn.primary:hover {
    background: var(--accent-hover);
  }
</style>
