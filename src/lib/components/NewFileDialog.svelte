<script>
  import { createEventDispatcher, tick } from "svelte";

  export let defaultName = "未命名.txt";

  const dispatch = createEventDispatcher();

  const ENCODING_OPTIONS = [
    { value: "UTF-8", label: "UTF-8（推荐，适合绝大多数文件）" },
    { value: "GBK", label: "GBK（简体中文，适合 .bat 等需要兼容旧版 Windows 的文件）" },
    { value: "GB18030", label: "GB18030（GBK 的超集，兼容性更广）" },
    { value: "BIG5", label: "Big5（繁体中文）" },
    { value: "UTF-16LE", label: "UTF-16 LE" },
    { value: "UTF-16BE", label: "UTF-16 BE" },
    { value: "SHIFT-JIS", label: "Shift-JIS（日文）" },
  ];

  let filename = defaultName;
  let encoding = "UTF-8";
  let inputEl;
  let errorMsg = "";

  // 弹窗打开时自动聚焦，并且只选中"文件名主体"部分（不含扩展名，如果有的话），
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

  // 文件名不强制要求带扩展名：像 Makefile、Dockerfile、LICENSE 这类
  // 约定俗成没有扩展名的文件也是合法的，只校验文件名本身不含非法字符。
  function validate(name) {
    const trimmed = name.trim();
    if (!trimmed) return "文件名不能为空";
    if (/[\\/:*?"<>|]/.test(trimmed)) return '文件名不能包含 \\ / : * ? " < > |';
    return "";
  }

  function handleConfirm() {
    const err = validate(filename);
    if (err) {
      errorMsg = err;
      return;
    }
    dispatch("confirm", { name: filename.trim(), encoding });
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
      <label class="field-label" for="new-file-name">文件名</label>
      <input
        id="new-file-name"
        type="text"
        bind:this={inputEl}
        bind:value={filename}
        on:keydown={handleKeydown}
        on:input={handleInput}
        placeholder="例如 123.json、notes.md、Dockerfile"
        class:has-error={!!errorMsg}
      />
      {#if errorMsg}
        <div class="error-text">{errorMsg}</div>
      {:else}
        <div class="hint-text">
          可以带扩展名（如 .txt .py），也可以不带（如 Makefile、LICENSE）
        </div>
      {/if}

      <label class="field-label encoding-label" for="new-file-encoding">保存编码</label>
      <select id="new-file-encoding" bind:value={encoding} on:keydown={handleKeydown}>
        {#each ENCODING_OPTIONS as opt}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
      <div class="hint-text">
        大部分情况用 UTF-8 即可；如果要新建 .bat/.cmd 等在简体中文 Windows
        上运行、且包含中文的批处理脚本，选 GBK 可以避免中文显示乱码。
      </div>
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
    width: 420px;
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
  .encoding-label {
    margin-top: 16px;
  }
  input,
  select {
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
  select {
    font-family: var(--ui);
    font-size: 12.5px;
    cursor: pointer;
  }
  input:focus,
  select:focus {
    border-color: var(--accent);
  }
  input.has-error {
    border-color: var(--danger);
  }
  .hint-text {
    font-size: 11px;
    color: var(--ink-faint);
    margin-top: 6px;
    line-height: 1.5;
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
