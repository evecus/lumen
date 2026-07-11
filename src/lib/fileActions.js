import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  tabs,
  createTab,
  markSaved,
  findTabByPath,
  activeTabId,
} from "./store";
import { get } from "svelte/store";
import { showToast } from "./toastStore";

function basename(path) {
  const parts = path.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] || path;
}

/**
 * 打开一个磁盘文件路径：若已打开则切换到该标签，否则读取内容并新建标签。
 */
export async function openFileByPath(path) {
  const existing = findTabByPath(get(tabs), path);
  if (existing) {
    activeTabId.set(existing.id);
    return existing;
  }

  try {
    const result = await invoke("read_text_file", { path });
    if (result.is_binary) {
      showToast(`“${basename(path)}” 看起来是二进制文件，无法以文本方式打开`, "error");
      return null;
    }
    const tab = createTab({
      name: basename(path),
      path,
      content: result.content,
      savedContent: result.content,
      encoding: result.encoding,
      eol: result.eol,
      dirty: false,
    });
    return tab;
  } catch (e) {
    showToast(`打开文件失败：${e}`, "error");
    return null;
  }
}

/**
 * 弹出系统打开对话框，选择一个或多个文件打开
 */
export async function openFileDialog() {
  const selected = await open({
    multiple: true,
    title: "打开文件",
  });
  if (!selected) return;
  const paths = Array.isArray(selected) ? selected : [selected];
  for (const p of paths) {
    await openFileByPath(p);
  }
}

/**
 * 保存指定标签：如果已有磁盘路径直接写入，否则走"另存为"流程
 */
export async function saveTab(tab) {
  if (!tab) return;
  if (tab.path) {
    try {
      const result = await invoke("write_text_file", {
        path: tab.path,
        content: tab.content,
        eol: tab.eol || "LF",
        encoding: tab.encoding || "UTF-8",
      });
      markSaved(tab.id, tab.path, tab.name);
      if (result && result.had_unmappable_chars) {
        showToast(
          `已保存 “${tab.name}”，但部分字符无法用 ${tab.encoding} 表示，已替换为 ?`,
          "info"
        );
      } else {
        showToast(`已保存 “${tab.name}”`, "success");
      }
    } catch (e) {
      showToast(`保存失败：${e}`, "error");
    }
  } else {
    await saveTabAs(tab);
  }
}

export async function saveTabAs(tab) {
  if (!tab) return;
  const targetPath = await save({
    title: "另存为",
    defaultPath: tab.name,
  });
  if (!targetPath) return;
  try {
    const result = await invoke("write_text_file", {
      path: targetPath,
      content: tab.content,
      eol: tab.eol || "LF",
      encoding: tab.encoding || "UTF-8",
    });
    markSaved(tab.id, targetPath, basename(targetPath));
    if (result && result.had_unmappable_chars) {
      showToast(
        `已保存 “${basename(targetPath)}”，但部分字符无法用 ${tab.encoding} 表示，已替换为 ?`,
        "info"
      );
    } else {
      showToast(`已保存 “${basename(targetPath)}”`, "success");
    }
  } catch (e) {
    showToast(`保存失败：${e}`, "error");
  }
}
