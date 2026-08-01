import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { usePlayerStore } from "@/stores/playerStore";
import { takeScreenshot } from "@/lib/engine";
import { getBinding, initShortcuts, matchBinding } from "@/lib/shortcuts";

/** 判断事件是否发生在输入控件内（此时不拦截快捷键）。 */
function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return (
    tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable
  );
}

/**
 * 全局快捷键（可配置，见 lib/shortcuts.ts）。
 * 默认：Space 播放暂停 / Ctrl+←→ 上下首 / Ctrl+↑↓ 音量 / Ctrl+M 静音 / F11 全屏 / F12 截图。
 */
export function useKeyboard(): void {
  useEffect(() => {
    initShortcuts();

    const onKeyDown = (e: KeyboardEvent) => {
      if (isEditableTarget(e.target)) return;
      const player = usePlayerStore.getState();

      if (matchBinding(e, getBinding("fullscreen"))) {
        e.preventDefault();
        void toggleFullscreen();
        return;
      }
      if (matchBinding(e, getBinding("screenshot"))) {
        e.preventDefault();
        void takeScreenshot();
        return;
      }
      if (matchBinding(e, getBinding("play-pause"))) {
        e.preventDefault();
        void player.toggle();
        return;
      }
      if (matchBinding(e, getBinding("previous"))) {
        e.preventDefault();
        void player.previous();
        return;
      }
      if (matchBinding(e, getBinding("next"))) {
        e.preventDefault();
        void player.next();
        return;
      }
      if (matchBinding(e, getBinding("volume-up"))) {
        e.preventDefault();
        void player.changeVolume(player.volume + 5);
        return;
      }
      if (matchBinding(e, getBinding("volume-down"))) {
        e.preventDefault();
        void player.changeVolume(player.volume - 5);
        return;
      }
      if (matchBinding(e, getBinding("mute"))) {
        e.preventDefault();
        void player.toggleMute();
        return;
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}

/** 切换窗口全屏状态。 */
export async function toggleFullscreen(): Promise<void> {
  try {
    const win = getCurrentWindow();
    const isFs = await win.isFullscreen();
    await win.setFullscreen(!isFs);
  } catch (err) {
    console.warn("[fullscreen] 切换失败:", err);
  }
}
