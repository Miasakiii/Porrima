import { create } from "zustand";
import { toast } from "sonner";
import * as cmd from "@/lib/commands";
import { normalizeError } from "@/lib/ipc";
import { applyTheme, getCachedTheme, watchSystemTheme } from "@/lib/theme";
import type { AudioOutputConfig, Settings, Theme } from "@/lib/types";

const DEFAULT_AUDIO_OUTPUT: AudioOutputConfig = {
  backend: "system",
  device: null,
  gapless: true,
  replayGain: "off",
  loudnormFallback: false,
};

interface SettingsStore extends Settings {
  loaded: boolean;
  /** 启动时调用一次：先用本地缓存主题，再拉后端设置校准。 */
  load: () => Promise<void>;
  setTheme: (theme: Theme) => Promise<void>;
  addScanDir: (dir: string) => Promise<boolean>;
  removeScanDir: (dir: string) => Promise<void>;
  setAudioOutput: (cfg: Partial<AudioOutputConfig>) => Promise<void>;
}

let systemWatchStop: (() => void) | null = null;

/** 应用主题；theme === 'system' 时同时挂系统主题监听。 */
function applyWithSystemWatch(theme: Theme): void {
  applyTheme(theme);
  systemWatchStop?.();
  systemWatchStop = null;
  if (theme === "system") {
    systemWatchStop = watchSystemTheme(() => applyTheme("system"));
  }
}

/** 全量持久化到后端；失败时 toast 并回滚到生效前的值。 */
async function persist(
  prev: Settings,
  next: Settings,
  rollback: (s: Settings) => void,
): Promise<boolean> {
  try {
    const applied = await cmd.updateSettings(next);
    rollback(applied);
    return true;
  } catch (err) {
    const e = normalizeError(err);
    console.warn("[settings] update_settings 失败:", e);
    toast.error("保存设置失败", { description: e.message });
    rollback(prev);
    return false;
  }
}

export const useSettingsStore = create<SettingsStore>()((set, get) => ({
  theme: getCachedTheme(),
  scanDirs: [],
  audioOutput: DEFAULT_AUDIO_OUTPUT,
  loaded: false,

  load: async () => {
    if (get().loaded) return;
    set({ loaded: true });
    // 本地缓存主题已由 index.html 内联脚本预应用，这里确保运行时一致
    applyWithSystemWatch(get().theme);
    try {
      const settings = await cmd.getSettings();
      set({
        theme: settings.theme,
        scanDirs: settings.scanDirs,
        audioOutput: settings.audioOutput ?? DEFAULT_AUDIO_OUTPUT,
      });
      applyWithSystemWatch(settings.theme);
    } catch (err) {
      console.warn("[settings] get_settings 失败（后端可能未就绪）:", normalizeError(err));
    }
  },

  setTheme: async (theme) => {
    const prev = getSnapshot(get);
    if (theme === prev.theme) return;
    set({ theme });
    applyWithSystemWatch(theme); // 立即应用，切换无闪烁
    await persist(prev, { ...prev, theme }, (s) => {
      set({ theme: s.theme });
      applyWithSystemWatch(s.theme);
    });
  },

  addScanDir: async (dir) => {
    const trimmed = dir.trim();
    if (!trimmed) {
      toast.error("目录路径不能为空");
      return false;
    }
    const prev = getSnapshot(get);
    if (prev.scanDirs.includes(trimmed)) {
      toast.info("该目录已在扫描列表中");
      return false;
    }
    const scanDirs = [...prev.scanDirs, trimmed];
    set({ scanDirs });
    const ok = await persist(prev, { ...prev, scanDirs }, (s) => set({ scanDirs: s.scanDirs }));
    if (ok) toast.success("已添加扫描目录", { description: trimmed });
    return ok;
  },

  removeScanDir: async (dir) => {
    const prev = getSnapshot(get);
    const scanDirs = prev.scanDirs.filter((d) => d !== dir);
    set({ scanDirs });
    await persist(prev, { ...prev, scanDirs }, (s) => set({ scanDirs: s.scanDirs }));
  },

  setAudioOutput: async (partial) => {
    const prev = getSnapshot(get);
    const audioOutput = { ...prev.audioOutput, ...partial };
    set({ audioOutput });
    await persist(
      prev,
      { ...prev, audioOutput },
      (s) => set({ audioOutput: s.audioOutput ?? DEFAULT_AUDIO_OUTPUT }),
    );
  },
}));

/** 取当前设置的纯数据快照（不含 store 方法）。 */
function getSnapshot(get: () => SettingsStore): Settings {
  const s = get();
  return { theme: s.theme, scanDirs: s.scanDirs, audioOutput: s.audioOutput };
}
