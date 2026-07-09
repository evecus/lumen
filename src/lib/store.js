import { writable, derived } from "svelte/store";

let idCounter = 0;
function nextId() {
  idCounter += 1;
  return `tab_${idCounter}_${Date.now().toString(36)}`;
}

function makeTab(overrides = {}) {
  return {
    id: nextId(),
    name: "未命名",
    path: null, // null = 未保存到磁盘
    content: "",
    savedContent: "",
    dirty: false,
    encoding: "UTF-8",
    eol: "LF",
    ...overrides,
  };
}

export const tabs = writable([]);
export const activeTabId = writable(null);

export const activeTab = derived(
  [tabs, activeTabId],
  ([$tabs, $activeTabId]) => $tabs.find((t) => t.id === $activeTabId) || null
);

export function createTab(overrides = {}) {
  const tab = makeTab(overrides);
  tabs.update((list) => [...list, tab]);
  activeTabId.set(tab.id);
  return tab;
}

export function closeTab(id) {
  tabs.update((list) => {
    const idx = list.findIndex((t) => t.id === id);
    const next = list.filter((t) => t.id !== id);
    activeTabId.update((current) => {
      if (current !== id) return current;
      if (!next.length) return null;
      const fallbackIdx = Math.max(0, idx - 1);
      return next[Math.min(fallbackIdx, next.length - 1)].id;
    });
    return next;
  });
}

export function setActiveTab(id) {
  activeTabId.set(id);
}

export function updateTabContent(id, content) {
  tabs.update((list) =>
    list.map((t) =>
      t.id === id ? { ...t, content, dirty: content !== t.savedContent } : t
    )
  );
}

export function markSaved(id, path, name) {
  tabs.update((list) =>
    list.map((t) =>
      t.id === id
        ? {
            ...t,
            path: path ?? t.path,
            name: name ?? t.name,
            savedContent: t.content,
            dirty: false,
          }
        : t
    )
  );
}

export function renameTab(id, name) {
  tabs.update((list) => list.map((t) => (t.id === id ? { ...t, name } : t)));
}

export function findTabByPath(list, path) {
  return list.find((t) => t.path === path);
}
