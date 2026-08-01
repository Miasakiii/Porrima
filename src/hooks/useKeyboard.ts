import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { usePlayerStore } from "@/stores/playerStore";
import { takeScreenshot } from "@/lib/engine";

/** 判断事件是否发生在输入控件内（此时不拦截快捷键）。 */
function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return (
    tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable
  );
}

/**
 * 全局快捷键（设计规范 4.8）：
 * Space 播放暂停 / Ctrl+←→ 上下首 / Ctrl+↑↓ 音量 / Ctrl+M 静音 / F11 全屏。
 */
export function useKeyboard(): void {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (isEditableTarget(e.target)) return;
      const player = usePlayerStore.getState();

      // F11 全屏切换
      if (e.key === "F11") {
        e.preventDefault();
        void toggleFullscreen();
        return;
      }

      // F12 截图
      if (e.key === "F12") {
        e.preventDefault();
        void takeScreenshot();
        return;
      }

      if (e.code === "Space" && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        void player.toggle();
        return;
      }

      if (!(e.ctrlKey || e.metaKey)) return;

      switch (e.key) {
        case "ArrowLeft":
          e.preventDefault();
          void player.previous();
          break;
        case "ArrowRight":
          e.preventDefault();
          void player.next();
          break;
        case "ArrowUp":
          e.preventDefault();
          void player.changeVolume(player.volume + 5);
          break;
        case "ArrowDown":
          e.preventDefault();
          void player.changeVolume(player.volume - 5);
          break;
        case "m":
        case "M":
          e.preventDefault();
          void player.toggleMute();
          break;
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
