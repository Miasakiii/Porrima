import { useSyncExternalStore } from "react";
import type { Theme } from "@/lib/types";

/**
 * 主题应用与解析。index.html 内联脚本会在首屏前读取同一 storage key
 * 做预应用，这里负责运行时的切换与持久化缓存。
 */
export const THEME_STORAGE_KEY = "porrima-theme";

const mediaQuery = "(prefers-color-scheme: dark)";

function systemPrefersDark(): boolean {
  return window.matchMedia(mediaQuery).matches;
}

export function resolveTheme(theme: Theme): "dark" | "light" {
  if (theme === "system") return systemPrefersDark() ? "dark" : "light";
  return theme;
}

/** 立即应用主题（切换 .dark class），并写入 localStorage 供下次启动防闪烁。 */
export function applyTheme(theme: Theme): void {
  const resolved = resolveTheme(theme);
  const root = document.documentElement;
  root.classList.toggle("dark", resolved === "dark");
  root.style.colorScheme = resolved;
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // localStorage 不可用时忽略，仅影响下次启动的预应用
  }
}

/** 读取本地缓存的主题（后端设置加载前的兜底）。 */
export function getCachedTheme(): Theme {
  try {
    const t = localStorage.getItem(THEME_STORAGE_KEY);
    if (t === "dark" || t === "light" || t === "system") return t;
  } catch {
    // ignore
  }
  return "dark";
}

function subscribeSystemTheme(callback: () => void): () => void {
  const mql = window.matchMedia(mediaQuery);
  mql.addEventListener("change", callback);
  return () => mql.removeEventListener("change", callback);
}

/**
 * 解析后的实际明暗主题（'dark' | 'light'），跟随系统时响应系统切换。
 * 传入当前设置的主题值。
 */
export function useResolvedTheme(theme: Theme): "dark" | "light" {
  const sysDark = useSyncExternalStore(
    subscribeSystemTheme,
    systemPrefersDark,
  );
  if (theme === "system") return sysDark ? "dark" : "light";
  return theme;
}

/** 跟随系统主题时，系统切换需重新应用（确保 class 同步）。 */
export function watchSystemTheme(onChange: () => void): () => void {
  return subscribeSystemTheme(onChange);
}
