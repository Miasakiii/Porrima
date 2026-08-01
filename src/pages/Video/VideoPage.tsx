import { useCallback, useEffect, useRef, useState } from "react";
import {
  Camera,
  FolderOpen,
  Maximize,
  PanelLeftClose,
  PanelLeftOpen,
  Pause,
  Play,
  SlidersHorizontal,
  SkipBack,
  SkipForward,
  Subtitles,
  Volume2,
  VolumeX,
} from "lucide-react";
import { command as mpvCommand, setProperty } from "tauri-plugin-libmpv-api";
import { invoke } from "@tauri-apps/api/core";
import { cn } from "@/lib/utils";
import { usePlayerStore } from "@/stores/playerStore";
import { toggleFullscreen } from "@/hooks/useKeyboard";
import { formatDuration } from "@/lib/format";
import { takeScreenshot } from "@/lib/engine";
import { FileBrowser } from "@/components/VideoBrowser/FileBrowser";
import { TrackPanel } from "@/components/VideoBrowser/TrackPanel";
import { VideoAdjustPanel } from "@/components/VideoBrowser/VideoAdjustPanel";

/** 侧边栏宽度约束。 */
const SIDEBAR_MIN = 200;
const SIDEBAR_MAX = 500;
const SIDEBAR_DEFAULT = 280;

/**
 * 视频播放页（Phase 4）。
 *
 * 视频由 mpv 渲染在 Tauri 窗口原生表面（--wid），
 * 本页面通过透明背景区域让视频"透出"，UI 控件叠加在上方。
 */
export function VideoPage() {
  const { status, positionMs, durationMs, volume, muted } = usePlayerStore();
  const toggle = usePlayerStore((s) => s.toggle);
  const next = usePlayerStore((s) => s.next);
  const previous = usePlayerStore((s) => s.previous);
  const seekTo = usePlayerStore((s) => s.seekTo);
  const changeVolume = usePlayerStore((s) => s.changeVolume);
  const toggleMute = usePlayerStore((s) => s.toggleMute);

  const [controlsVisible, setControlsVisible] = useState(true);
  const [browserOpen, setBrowserOpen] = useState(true);
  const [trackPanelOpen, setTrackPanelOpen] = useState(false);
  const [adjustPanelOpen, setAdjustPanelOpen] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(SIDEBAR_DEFAULT);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const dragging = useRef(false);

  // 鼠标移动时显示控件，3 秒无操作自动隐藏
  const showControls = useCallback(() => {
    setControlsVisible(true);
    if (hideTimer.current) clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => {
      if (status === "playing") setControlsVisible(false);
    }, 3000);
  }, [status]);

  useEffect(() => {
    return () => {
      if (hideTimer.current) clearTimeout(hideTimer.current);
    };
  }, []);

  // 侧边栏拖动调整宽度
  const onDragStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    const startX = e.clientX;
    const startW = sidebarWidth;
    const onMove = (ev: MouseEvent) => {
      if (!dragging.current) return;
      const delta = ev.clientX - startX;
      setSidebarWidth(Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, startW + delta)));
    };
    const onUp = () => {
      dragging.current = false;
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }, [sidebarWidth]);

  // 监听外部打开视频事件（文件关联/命令行）
  const playFileRef = useRef<(path: string) => void>(() => {});
  useEffect(() => {
    const handler = (e: Event) => {
      const path = (e as CustomEvent<string>).detail;
      if (path) playFileRef.current(path);
    };
    window.addEventListener("porrima:play-video", handler);
    return () => window.removeEventListener("porrima:play-video", handler);
  }, []);

  const handleFullscreen = () => {
    void toggleFullscreen();
  };

  /** 播放视频文件：直接通过 mpv loadfile 加载（不经后端队列），支持续播。 */
  const handlePlayFile = async (path: string) => {
    try {
      await setProperty("start", "none");
      await setProperty("end", "none");
      await mpvCommand("loadfile", [path]);
      await setProperty("pause", false);
      // 续播：查询上次位置并 seek
      const savedPos = await invoke<number>("get_video_position", { path });
      if (savedPos > 5000) {
        // 延迟 seek，等待 file-loaded
        setTimeout(() => {
          void mpvCommand("seek", [savedPos / 1000, "absolute"]);
        }, 300);
      }
      // 播放视频时自动收起浏览器
      setBrowserOpen(false);
      setCurrentVideoPath(path);
    } catch (err) {
      console.warn("[video] 播放失败:", err);
    }
  };
  playFileRef.current = (path) => void handlePlayFile(path);

  const [currentVideoPath, setCurrentVideoPath] = useState<string | null>(null);

  // 定期保存视频播放位置（每 10 秒）
  useEffect(() => {
    if (!currentVideoPath || status !== "playing") return;
    const interval = setInterval(() => {
      const pos = usePlayerStore.getState().positionMs;
      if (pos > 0) {
        void invoke("save_video_position", {
          path: currentVideoPath,
          positionMs: pos,
        });
      }
    }, 10_000);
    return () => clearInterval(interval);
  }, [currentVideoPath, status]);

  const progress = durationMs > 0 ? (positionMs / durationMs) * 100 : 0;

  return (
    <div className="flex h-full overflow-hidden">
      {/* 文件浏览器侧栏 */}
      {browserOpen && (
        <div
          className="shrink-0 overflow-hidden border-r border-border"
          style={{ width: sidebarWidth }}
        >
          <FileBrowser onPlayFile={(p) => void handlePlayFile(p)} />
        </div>
      )}

      {/* 拖动手柄 */}
      {browserOpen && (
        <div
          onMouseDown={onDragStart}
          className="w-1 shrink-0 cursor-col-resize bg-transparent transition-colors hover:bg-accent/40 active:bg-accent/60"
        />
      )}

      {/* 视频区域 */}
      <div
        className="relative flex min-w-0 flex-1 flex-col"
        onMouseMove={showControls}
        onMouseLeave={() => status === "playing" && setControlsVisible(false)}
      >
        {/* 视频渲染表面：透明背景让 mpv 视频从窗口原生层透出 */}
        <div
          className="relative flex-1 bg-transparent"
          onDoubleClick={handleFullscreen}
        >
          {/* 无视频时的占位提示 */}
          {status === "stopped" && (
            <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-background">
              <FolderOpen className="size-12 text-muted-foreground/40" />
              <p className="text-sm text-muted-foreground">
                从左侧浏览器选择视频文件播放
              </p>
              <p className="text-xs text-muted-foreground/60">
                支持 MP4 / MKV / AVI / MOV / WebM / FLV / TS 等格式
              </p>
            </div>
          )}
        </div>

        {/* 视频控制栏：悬浮在底部 */}
        <div
          className={cn(
            "absolute right-0 bottom-0 left-0 transition-opacity duration-300",
            controlsVisible ? "opacity-100" : "pointer-events-none opacity-0",
          )}
        >
          {/* 进度条 */}
          <div className="group px-4">
            <input
              type="range"
              min={0}
              max={durationMs || 1}
              value={positionMs}
              onChange={(e) => void seekTo(Number(e.target.value))}
              className="h-1 w-full cursor-pointer appearance-none rounded-full bg-white/20 accent-[var(--accent)] [&::-webkit-slider-thumb]:size-3 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-white"
              style={{
                background: `linear-gradient(to right, var(--accent) ${progress}%, rgba(255,255,255,0.2) ${progress}%)`,
              }}
            />
          </div>

          {/* 控制按钮 */}
          <div className="flex items-center gap-3 bg-gradient-to-t from-black/80 to-transparent px-4 pt-1 pb-3">
            {/* 浏览器开关 */}
            <button
              type="button"
              onClick={() => setBrowserOpen(!browserOpen)}
              className="rounded p-1.5 text-white/80 transition-colors hover:text-white"
              title={browserOpen ? "收起浏览器" : "展开浏览器"}
            >
              {browserOpen ? (
                <PanelLeftClose className="size-4" />
              ) : (
                <PanelLeftOpen className="size-4" />
              )}
            </button>

            <button
              type="button"
              onClick={() => void previous()}
              className="rounded p-1.5 text-white/80 transition-colors hover:text-white"
            >
              <SkipBack className="size-5" />
            </button>

            <button
              type="button"
              onClick={() => void toggle()}
              className="rounded-full bg-white/15 p-2.5 text-white transition-colors hover:bg-white/25"
            >
              {status === "playing" ? (
                <Pause className="size-5" />
              ) : (
                <Play className="size-5" />
              )}
            </button>

            <button
              type="button"
              onClick={() => void next()}
              className="rounded p-1.5 text-white/80 transition-colors hover:text-white"
            >
              <SkipForward className="size-5" />
            </button>

            {/* 时间显示 */}
            <span className="ml-2 text-xs tabular-nums text-white/70">
              {formatDuration(positionMs)} / {formatDuration(durationMs)}
            </span>

            <div className="flex-1" />

            {/* 字幕/音轨 */}
            <button
              type="button"
              onClick={() => setTrackPanelOpen(!trackPanelOpen)}
              className={cn(
                "rounded p-1.5 transition-colors",
                trackPanelOpen ? "text-accent" : "text-white/80 hover:text-white",
              )}
              title="字幕/音轨"
            >
              <Subtitles className="size-4" />
            </button>

            {/* 画面调节 */}
            <button
              type="button"
              onClick={() => setAdjustPanelOpen(!adjustPanelOpen)}
              className={cn(
                "rounded p-1.5 transition-colors",
                adjustPanelOpen ? "text-accent" : "text-white/80 hover:text-white",
              )}
              title="画面调节"
            >
              <SlidersHorizontal className="size-4" />
            </button>

            {/* 截图 */}
            <button
              type="button"
              onClick={() => void takeScreenshot()}
              className="rounded p-1.5 text-white/80 transition-colors hover:text-white"
              title="截图 (F12)"
            >
              <Camera className="size-4" />
            </button>

            {/* 音量 */}
            <button
              type="button"
              onClick={() => void toggleMute()}
              className="rounded p-1.5 text-white/80 transition-colors hover:text-white"
            >
              {muted || volume === 0 ? (
                <VolumeX className="size-4" />
              ) : (
                <Volume2 className="size-4" />
              )}
            </button>
            <input
              type="range"
              min={0}
              max={100}
              value={muted ? 0 : volume}
              onChange={(e) => void changeVolume(Number(e.target.value))}
              className="h-1 w-20 cursor-pointer appearance-none rounded-full bg-white/20 accent-white [&::-webkit-slider-thumb]:size-2.5 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-white"
            />

            {/* 全屏 */}
            <button
              type="button"
              onClick={handleFullscreen}
              className="rounded p-1.5 text-white/80 transition-colors hover:text-white"
            >
              <Maximize className="size-4" />
            </button>
          </div>
        </div>

        {/* 字幕/音轨选择面板 */}
        <TrackPanel open={trackPanelOpen} onClose={() => setTrackPanelOpen(false)} />

        {/* 画面调节面板 */}
        <VideoAdjustPanel open={adjustPanelOpen} onClose={() => setAdjustPanelOpen(false)} />
      </div>
    </div>
  );
}
