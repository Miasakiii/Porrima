import { useEffect } from "react";
import { getCoverColor } from "@/lib/commands";
import { usePlayerStore } from "@/stores/playerStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useResolvedTheme } from "@/lib/theme";
import { applyAccent, clearAccent } from "@/lib/accent";

/**
 * 当前曲目封面主题色 → 全局强调色（--accent）。
 *
 * 依赖当前曲目与解析后的明暗主题：切歌或切换主题时重新取色并按主题裁剪重应用；
 * 停止播放（无当前曲目）或该曲目无封面时恢复样式表默认强调色。
 * 后端会话缓存使重复取色近乎零成本；乱序响应由每次 effect 的 cancelled 守卫处理。
 */
export function useDynamicAccent(): void {
  const currentTrackId = usePlayerStore((s) => s.currentTrackId);
  const theme = useSettingsStore((s) => s.theme);
  const resolved = useResolvedTheme(theme);

  useEffect(() => {
    if (!currentTrackId) {
      clearAccent();
      return;
    }
    let cancelled = false;
    getCoverColor(currentTrackId)
      .then((color) => {
        if (cancelled) return;
        if (color) applyAccent(color, resolved);
        else clearAccent();
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [currentTrackId, resolved]);
}
