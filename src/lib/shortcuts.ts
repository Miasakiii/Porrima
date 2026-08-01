/**
 * 快捷键配置模块：默认绑定 + localStorage 持久化 + 运行时查询。
 *
 * 绑定格式示例："Space"、"Ctrl+ArrowLeft"、"Ctrl+Shift+M"、"F11"。
 * 修饰键统一用 Ctrl（macOS 上运行时自动映射为 Meta）。
 */

export interface ShortcutAction {
  id: string;
  label: string;
  /** 默认绑定 */
  defaultBinding: string;
}

/** 所有可配置的快捷键动作。 */
export const SHORTCUT_ACTIONS: ShortcutAction[] = [
  { id: "play-pause", label: "播放 / 暂停", defaultBinding: "Space" },
  { id: "previous", label: "上一首", defaultBinding: "Ctrl+ArrowLeft" },
  { id: "next", label: "下一首", defaultBinding: "Ctrl+ArrowRight" },
  { id: "volume-up", label: "音量 +5", defaultBinding: "Ctrl+ArrowUp" },
  { id: "volume-down", label: "音量 -5", defaultBinding: "Ctrl+ArrowDown" },
  { id: "mute", label: "静音", defaultBinding: "Ctrl+M" },
  { id: "fullscreen", label: "全屏切换", defaultBinding: "F11" },
  { id: "screenshot", label: "截图（视频模式）", defaultBinding: "F12" },
];

const STORAGE_KEY = "porrima-shortcuts";

/** 当前生效的绑定表（action id → binding string）。 */
let bindings: Record<string, string> = {};

/** 初始化：从 localStorage 加载，缺失项用默认值补全。 */
export function initShortcuts(): void {
  let saved: Record<string, string> = {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) saved = JSON.parse(raw);
  } catch {
    // ignore
  }
  bindings = {};
  for (const action of SHORTCUT_ACTIONS) {
    bindings[action.id] = saved[action.id] ?? action.defaultBinding;
  }
}

/** 获取当前绑定表。 */
export function getBindings(): Record<string, string> {
  if (Object.keys(bindings).length === 0) initShortcuts();
  return { ...bindings };
}

/** 获取指定动作的绑定。 */
export function getBinding(actionId: string): string {
  if (Object.keys(bindings).length === 0) initShortcuts();
  return bindings[actionId] ?? "";
}

/** 设置指定动作的绑定并持久化。 */
export function setBinding(actionId: string, binding: string): void {
  bindings[actionId] = binding;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(bindings));
  } catch {
    // ignore
  }
}

/** 重置所有快捷键为默认值。 */
export function resetShortcuts(): void {
  bindings = {};
  for (const action of SHORTCUT_ACTIONS) {
    bindings[action.id] = action.defaultBinding;
  }
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // ignore
  }
}

/**
 * 将 KeyboardEvent 序列化为绑定字符串（如 "Ctrl+Shift+M"）。
 * 用于录制快捷键时捕获用户按键。
 */
export function eventToBinding(e: KeyboardEvent): string | null {
  // 忽略纯修饰键按下
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return null;

  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("Ctrl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");

  // 规范化 key 名
  let key = e.key;
  if (key === " ") key = "Space";
  else if (key.length === 1) key = key.toUpperCase();

  parts.push(key);
  return parts.join("+");
}

/**
 * 判断 KeyboardEvent 是否匹配指定绑定字符串。
 */
export function matchBinding(e: KeyboardEvent, binding: string): boolean {
  if (!binding) return false;

  const parts = binding.split("+");
  const needCtrl = parts.includes("Ctrl");
  const needShift = parts.includes("Shift");
  const needAlt = parts.includes("Alt");
  const key = parts.filter((p) => !["Ctrl", "Shift", "Alt"].includes(p))[0] ?? "";

  const hasCtrl = e.ctrlKey || e.metaKey;
  if (hasCtrl !== needCtrl) return false;
  if (e.shiftKey !== needShift) return false;
  if (e.altKey !== needAlt) return false;

  // 比较 key（大小写不敏感）
  let eventKey = e.key;
  if (eventKey === " ") eventKey = "Space";
  return eventKey.toLowerCase() === key.toLowerCase();
}
