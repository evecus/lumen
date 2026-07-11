<script>
  import { createEventDispatcher, tick } from "svelte";

  export let defaultName = "未命名.txt";

  const dispatch = createEventDispatcher();

  let filename = defaultName;
  let inputEl;
  let errorMsg = "";

  // 弹窗打开时自动聚焦，并且只选中"文件名主体"部分（不含扩展名），
  // 这样用户直接打字就能替换文件名，同时保留默认扩展名，体验更顺手。
  async function focusAndSelect() {
    await tick();
    if (!inputEl) return;
    inputEl.focus();
    const dotIndex = filename.lastIndexOf(".");
    if (dotIndex > 0) {
      inputEl.setSelectionRange(0, dotIndex);
    } else {
      inputEl.select();
    }
  }

  focusAndSelect();

  function validate(name) {
    const trimmed = name.trim();
    if (!trimmed) return "文件名不能为空";
    if (/[\\/:*?"<>|]/.test(trimmed)) return '文件名不能包含 \\ / : * ? " < > |';
    if (!trimmed.includes(".") || trimmed.endsWith(".")) {
      return "请包含文件扩展名，例如 123.json";
    }
    return "";
  }

  function handleConfirm() {
    const err = validate(filename);
    if (err) {
      errorMsg = err;
      return;
    }
    dispatch("confirm", { name: filename.trim() });
  }

  function handleCancel() {
    dispatch("cancel");
  }

  function handleKeydown(e) {
    if (e.key === "Enter") {
      e.preventDefault();
      handleConfirm();
    } else if (e.key === "Escape") {
      e.preventDefault();
      handleCancel();
    }
  }

  function handleInput() {
    if (errorMsg) errorMsg = "";
  }
</script>

<div class="overlay" on:click|self={handleCancel}>
  <div class="dialog">
    <div class="dialog-head">
      <h2>新建文件</h2>
    </div>
    <div class="dialog-body">
      <label class="field-label" for="new-file-name">文件名（含扩展名）</label>
      <input
        id="new-file-name"
        type="text"
        bind:this={inputEl}
        bind:value={filename}
        on:keydown={handleKeydown}
        on:input={handleInput}
        placeholder="例如 123.json、notes.md、script.py"
        class:has-error={!!errorMsg}
      />
      {#if errorMsg}
        <div class="error-text">{errorMsg}</div>
      {:else}
        <div class="hint-text">支持任意扩展名，例如 .txt .md .json .py .js 等</div>
      {/if}
    </div>
    <div class="dialog-foot">
      <button class="btn" on:click={handleCancel}>取消</button>
      <button class="btn primary" on:click={handleConfirm}>确定</button>
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
  .dialog {
    width: 400px;
    background: #fff;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
  }
  .dialog-head {
    padding: 18px 20px 12px;
  }
  .dialog-head h2 {
    font-size: 15px;
    font-weight: 700;
    margin: 0;
    color: var(--ink);
  }
  .dialog-body {
    padding: 4px 20px 18px;
  }
  .field-label {
    display: block;
    font-size: 11.5px;
    color: var(--ink-dim);
    margin-bottom: 6px;
  }
  input {
    width: 100%;
    padding: 9px 11px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 13px;
    outline: none;
    background: #fff;
    color: var(--ink);
  }
  input:focus {
    border-color: var(--accent);
  }
  input.has-error {
    border-color: var(--danger);
  }
  .hint-text {
    font-size: 11px;
    color: var(--ink-faint);
    margin-top: 6px;
  }
  .error-text {
    font-size: 11px;
    color: var(--danger);
    margin-top: 6px;
  }
  .dialog-foot {
    padding: 12px 20px 18px;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
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
  .btn:hover {
    background: var(--bg-hover);
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
