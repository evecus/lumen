import { writable } from "svelte/store";

export const toast = writable({ message: "", type: "info", visible: false });

let hideTimer = null;

export function showToast(message, type = "info") {
  toast.set({ message, type, visible: true });
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => {
    toast.update((t) => ({ ...t, visible: false }));
  }, 2200);
}
