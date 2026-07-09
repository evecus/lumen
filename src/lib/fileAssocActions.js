import { invoke } from "@tauri-apps/api/core";
import { showToast } from "./toastStore";

function getExtension(tab) {
  if (!tab) return null;
  const name = tab.name || "";
  const idx = name.lastIndexOf(".");
  if (idx === -1 || idx === name.length - 1) return null;
  return name.slice(idx + 1).toLowerCase();
}

/**
 * 把当前活动标签对应的文件扩展名设为 Lumen 的默认打开程序。
 * 未保存过的新建文件（比如"未命名-1.txt"）也能取到扩展名，
 * 但只有真正落盘的文件类型被设为默认才有实际意义——这里不做额外限制，
 * 用户对着一个 .txt 新文件点"设为默认"，效果上就是把 .txt 全局设为 Lumen 打开，
 * 这和图形界面里"这个格式以后都用这个程序打开"的直觉是一致的。
 */
export async function setActiveTabExtensionAsDefault(tab) {
  const ext = getExtension(tab);
  if (!ext) {
    showToast("当前文件没有可识别的扩展名", "info");
    return;
  }
  try {
    await invoke("set_default_file_extension", { ext });
    showToast(`已将 Lumen 设为 .${ext} 的默认打开程序`, "success");
  } catch (e) {
    showToast(`设置失败：${e}`, "error");
  }
}
