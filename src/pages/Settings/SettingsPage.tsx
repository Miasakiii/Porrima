import { useEffect, useState } from "react";
import { FolderMinus, FolderOpen, Monitor, Moon, Sun } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { cn } from "@/lib/utils";
import { useSettingsStore } from "@/stores/settingsStore";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  SHORTCUT_ACTIONS,
  eventToBinding,
  getBindings,
  resetShortcuts,
  setBinding,
} from "@/lib/shortcuts";
import type { AudioBackend, ReplayGainMode, Theme } from "@/lib/types";

const THEME_OPTIONS: { value: Theme; label: string; icon: React.ComponentType<{ className?: string }> }[] = [
  { value: "dark", label: "暗色", icon: Moon },
  { value: "light", label: "亮色", icon: Sun },
  { value: "system", label: "跟随系统", icon: Monitor },
];

/** 设置页：主题 + 扫描目录 + 音频输出 + 快捷键 + 关于。 */
export function SettingsPage() {
  return (
    <ScrollArea className="h-full">
      <div className="mx-auto max-w-xl space-y-8 px-6 py-6">
        <h1 className="text-xl font-semibold tracking-tight">设置</h1>

        <ThemeSection />
        <Separator />
        <ScanDirsSection />
        <Separator />
        <AudioOutputSection />
        <Separator />
        <ScreenshotSection />
        <Separator />
        <ShortcutsSection />
        <Separator />
        <AboutSection />
      </div>
    </ScrollArea>
  );
}

function ThemeSection() {
  const theme = useSettingsStore((s) => s.theme);
  const setTheme = useSettingsStore((s) => s.setTheme);

  return (
    <section className="space-y-3">
      <div>
        <h2 className="text-base font-medium">外观主题</h2>
        <p className="mt-0.5 text-xs text-muted-foreground">
          暗色为默认主题，设置会保存并在下次启动时生效
        </p>
      </div>
      <div className="flex gap-2">
        {THEME_OPTIONS.map(({ value, label, icon: Icon }) => (
          <button
            key={value}
            type="button"
            onClick={() => void setTheme(value)}
            aria-pressed={theme === value}
            className={cn(
              "flex h-16 w-28 flex-col items-center justify-center gap-1.5 rounded-lg border text-[13px] transition-colors duration-150",
              theme === value
                ? "border-accent bg-accent/12 text-foreground"
                : "border-border text-muted-foreground hover:bg-muted hover:text-foreground",
            )}
          >
            <Icon className={cn("size-5", theme === value && "text-accent")} />
            {label}
          </button>
        ))}
      </div>
    </section>
  );
}

function ScanDirsSection() {
  const scanDirs = useSettingsStore((s) => s.scanDirs);
  const addScanDir = useSettingsStore((s) => s.addScanDir);
  const removeScanDir = useSettingsStore((s) => s.removeScanDir);

  const [submitting, setSubmitting] = useState(false);

  const handlePickDir = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected) return;
    setSubmitting(true);
    try {
      await addScanDir(selected as string);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <section className="space-y-3">
      <div>
        <h2 className="text-base font-medium">扫描目录</h2>
        <p className="mt-0.5 text-xs text-muted-foreground">
          媒体库会递归扫描以下目录中的音频文件
        </p>
      </div>

      <Button
        variant="outline"
        size="sm"
        className="h-9"
        disabled={submitting}
        onClick={() => void handlePickDir()}
      >
        <FolderOpen className="size-4" />
        选择目录
      </Button>

      {scanDirs.length === 0 ? (
        <p className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-xs text-muted-foreground">
          尚未添加扫描目录
        </p>
      ) : (
        <ul className="divide-y divide-border rounded-lg border border-border">
          {scanDirs.map((dir) => (
            <li key={dir} className="flex items-center gap-3 px-3 py-2">
              <span className="min-w-0 flex-1 truncate text-[13px]" title={dir}>
                {dir}
              </span>
              <button
                type="button"
                aria-label={`移除 ${dir}`}
                onClick={() => void removeScanDir(dir)}
                className="shrink-0 rounded-md p-1.5 text-muted-foreground transition-colors duration-150 hover:bg-muted hover:text-destructive"
              >
                <FolderMinus className="size-4" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

const BACKEND_OPTIONS: { value: AudioBackend; label: string; desc: string }[] = [
  { value: "system", label: "系统默认", desc: "走系统混音器，兼容性最好" },
  { value: "wasapi-shared", label: "WASAPI 共享", desc: "低延迟，与其他应用共存" },
  { value: "wasapi-exclusive", label: "WASAPI 独占", desc: "绕过混音器，HiFi 直出" },
];

const RG_OPTIONS: { value: ReplayGainMode; label: string }[] = [
  { value: "off", label: "关闭" },
  { value: "track", label: "按曲目" },
  { value: "album", label: "按专辑" },
];

function AudioOutputSection() {
  const audioOutput = useSettingsStore((s) => s.audioOutput);
  const setAudioOutput = useSettingsStore((s) => s.setAudioOutput);

  return (
    <section className="space-y-4">
      <div>
        <h2 className="text-base font-medium">音频输出</h2>
        <p className="mt-0.5 text-xs text-muted-foreground">
          音频后端、无缝播放与响度归一化设置，修改后即时生效
        </p>
      </div>

      {/* 输出后端 */}
      <div className="space-y-2">
        <label className="text-[13px] font-medium">输出模式</label>
        <div className="grid grid-cols-3 gap-2">
          {BACKEND_OPTIONS.map(({ value, label, desc }) => (
            <button
              key={value}
              type="button"
              onClick={() => void setAudioOutput({ backend: value })}
              aria-pressed={audioOutput.backend === value}
              className={cn(
                "flex flex-col items-start gap-0.5 rounded-lg border px-3 py-2.5 text-left transition-colors duration-150",
                audioOutput.backend === value
                  ? "border-accent bg-accent/12"
                  : "border-border hover:bg-muted",
              )}
            >
              <span className={cn("text-[13px] font-medium", audioOutput.backend === value && "text-accent")}>
                {label}
              </span>
              <span className="text-[11px] text-muted-foreground">{desc}</span>
            </button>
          ))}
        </div>
      </div>

      {/* 无缝播放 + ReplayGain + loudnorm */}
      <div className="space-y-3 rounded-lg border border-border p-3">
        <label className="flex cursor-pointer items-center justify-between">
          <span className="text-[13px]">无缝播放 (Gapless)</span>
          <input
            type="checkbox"
            checked={audioOutput.gapless}
            onChange={(e) => void setAudioOutput({ gapless: e.target.checked })}
            className="size-4 accent-[var(--accent)]"
          />
        </label>

        <div className="flex items-center justify-between">
          <span className="text-[13px]">ReplayGain</span>
          <div className="flex gap-1">
            {RG_OPTIONS.map(({ value, label }) => (
              <button
                key={value}
                type="button"
                onClick={() => void setAudioOutput({ replayGain: value })}
                className={cn(
                  "rounded-md px-2.5 py-1 text-xs transition-colors",
                  audioOutput.replayGain === value
                    ? "bg-accent text-accent-foreground"
                    : "bg-muted text-muted-foreground hover:text-foreground",
                )}
              >
                {label}
              </button>
            ))}
          </div>
        </div>

        <label className="flex cursor-pointer items-center justify-between">
          <span className="text-[13px]">响度归一化降级 (loudnorm)</span>
          <input
            type="checkbox"
            checked={audioOutput.loudnormFallback}
            onChange={(e) => void setAudioOutput({ loudnormFallback: e.target.checked })}
            className="size-4 accent-[var(--accent)]"
          />
        </label>
        <p className="text-[11px] text-muted-foreground">
          无 ReplayGain 标签时启用 EBU R128 响度归一化，避免曲目间音量差异过大
        </p>
      </div>
    </section>
  );
}

function ScreenshotSection() {
  const [dir, setDir] = useState<string | null>(null);

  useEffect(() => {
    void invoke<string>("get_screenshot_dir").then(setDir).catch(() => setDir(null));
  }, []);

  return (
    <section className="space-y-3">
      <div>
        <h2 className="text-base font-medium">视频截图</h2>
        <p className="mt-0.5 text-xs text-muted-foreground">
          按 F12 或点击控制栏截图按钮保存当前帧
        </p>
      </div>
      <div className="rounded-lg border border-border px-4 py-3">
        <span className="text-[11px] text-muted-foreground">保存位置</span>
        <p className="mt-0.5 truncate text-[13px]" title={dir ?? ""}>
          {dir ?? "加载中…"}
        </p>
      </div>
    </section>
  );
}

function ShortcutsSection() {
  const [bindings, setBindings] = useState<Record<string, string>>(() => getBindings());
  const [recording, setRecording] = useState<string | null>(null);

  // 录制模式：监听下一次按键作为新绑定
  useEffect(() => {
    if (!recording) return;
    const handler = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecording(null);
        return;
      }
      const binding = eventToBinding(e);
      if (binding) {
        setBinding(recording, binding);
        setBindings(getBindings());
        setRecording(null);
      }
    };
    window.addEventListener("keydown", handler, { capture: true });
    return () => window.removeEventListener("keydown", handler, { capture: true });
  }, [recording]);

  const handleReset = () => {
    resetShortcuts();
    setBindings(getBindings());
  };

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-base font-medium">快捷键</h2>
          <p className="mt-0.5 text-xs text-muted-foreground">
            点击按键后按下新组合键即可重新绑定，Esc 取消
          </p>
        </div>
        <Button variant="ghost" size="sm" onClick={handleReset} className="h-7 text-xs">
          恢复默认
        </Button>
      </div>
      <div className="space-y-1">
        {SHORTCUT_ACTIONS.map(({ id, label }) => (
          <div key={id} className="flex items-center justify-between py-1">
            <span className="text-[13px] text-muted-foreground">{label}</span>
            <button
              type="button"
              onClick={() => setRecording(id)}
              className={cn(
                "min-w-[100px] rounded border px-2 py-1 text-center text-[11px] font-medium transition-colors",
                recording === id
                  ? "animate-pulse border-accent bg-accent/10 text-accent"
                  : "border-border bg-muted text-foreground hover:border-accent/50",
              )}
            >
              {recording === id ? "按下按键…" : bindings[id] || "未绑定"}
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}

function AboutSection() {
  return (
    <section className="space-y-3">
      <div>
        <h2 className="text-base font-medium">关于</h2>
      </div>
      <div className="rounded-lg border border-border p-4 text-[13px] leading-relaxed text-muted-foreground">
        <p className="font-medium text-foreground">Porrima v0.1.0</p>
        <p className="mt-1">
          轻量跨平台音视频播放器。基于 Tauri v2 + libmpv，
          用 mpv 的全格式解码能力做出精致的桌面 UI。
        </p>
        <p className="mt-2 text-xs">
          技术栈：Tauri v2 · React 19 · libmpv · rusqlite · lofty · souvlaki
        </p>
        <p className="mt-1 text-xs">
          libmpv 运行库以 LGPL 许可随包分发。
        </p>
      </div>
    </section>
  );
}
