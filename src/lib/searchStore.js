import { writable } from "svelte/store";

export const searchPanelOpen = writable(false);
export const searchPanelMode = writable("find"); // 'find' | 'replace'

export const searchOptions = writable({
  findText: "",
  replaceText: "",
  backward: false, // 反向查找
  wholeWord: false, // 全词匹配
  matchCase: false, // 匹配大小写
  wrapAround: true, // 循环查找
  mode: "normal", // 'normal' | 'extended' | 'regex'
  matchNewline: false, // 正则模式下 . 匹配新行
});

export const searchStatus = writable({
  totalMatches: 0,
  currentIndex: -1, // 0-based
  lastActionMessage: "",
});

/**
 * 把"扩展"模式的转义序列 (\n \r \t \0 \xFF) 转换成真实字符
 */
export function expandEscapes(str) {
  return str.replace(/\\(n|r|t|0|x[0-9a-fA-F]{2})/g, (_, code) => {
    switch (code[0]) {
      case "n":
        return "\n";
      case "r":
        return "\r";
      case "t":
        return "\t";
      case "0":
        return "\0";
      default:
        if (code.startsWith("x")) {
          return String.fromCharCode(parseInt(code.slice(1), 16));
        }
        return code;
    }
  });
}

function escapeRegExp(str) {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * 根据当前查找选项构建正则表达式。normal/extended 模式下按字面量转义，
 * regex 模式下直接使用用户输入的正则。
 */
export function buildSearchRegex(opts) {
  const { findText, mode, wholeWord, matchCase, matchNewline } = opts;
  if (!findText) return null;

  let pattern;
  if (mode === "regex") {
    pattern = findText;
  } else if (mode === "extended") {
    pattern = escapeRegExp(expandEscapes(findText));
  } else {
    pattern = escapeRegExp(findText);
  }

  if (wholeWord) {
    pattern = `\\b(?:${pattern})\\b`;
  }

  let flags = "g";
  if (!matchCase) flags += "i";
  if (mode === "regex" && matchNewline) flags += "s";

  try {
    return new RegExp(pattern, flags);
  } catch (e) {
    return null;
  }
}

/**
 * 在给定文本中查找所有匹配项，返回 [{start, end}]
 */
export function findAllMatches(text, opts) {
  const re = buildSearchRegex(opts);
  if (!re) return [];
  const matches = [];
  let m;
  let guard = 0;
  while ((m = re.exec(text)) && guard < 100000) {
    const len = m[0].length;
    matches.push({ start: m.index, end: m.index + (len || 1) });
    if (len === 0) re.lastIndex += 1;
    guard += 1;
  }
  return matches;
}
