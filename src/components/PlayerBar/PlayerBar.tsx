import { useEffect, useRef, useState } from "react";
import {
  Globe,
  ListOrdered,
  MicVocal,
  Music2,
  Pause,
  Play,
  Repeat,
  Repeat1,
  Shuffle,
  SkipBack,
  SkipForward,
  SlidersVertical,
  Volume,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { formatDuration } from "@/lib/format";
import { getCover, getTrack, searchCoverOnline } from "@/lib/commands";
import { PLAY_MODE_LABEL, usePlayerStore } from "@/stores/playerStore";
import { Slider } from "@/components/ui/slider";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { PlayMode, Track } from "@/lib/types";
import { EqualizerPanel } from "@/components/PlayerBar/EqualizerPanel";

const PLAY_MODE_ICON: Record<PlayMode, React.ComponentType<{ className?: string }>> = {
  sequential: ListOrdered,
  "repeat-all": Repeat,
  "repeat-one": Repeat1,
  shuffle: Shuffle,
};

/**
 * 全局 PlayerBar（设计规范 4.6，72px 三段式）：
 * 左 240px 封面+曲名 / 中段 控制钮+进度条 / 右 200px 音量。
 * 无播放任务时整体置灰。
 */
export function PlayerBar() {
  const currentTrackId = usePlayerStore((s) => s.currentTrackId);
  const hasTrack = currentTrackId != null;
  const [eqOpen, setEqOpen] = useState(false);

  return (
    <footer className="relative flex h-[72px] shrink-0 items-center gap-4 border-t border-border bg-card px-4">
      <TrackInfo />
      <TransportControls disabled={!hasTrack} />
      <div className="flex items-center gap-1">
        {/* 均衡器按钮 */}
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              onClick={() => setEqOpen(!eqOpen)}
              className={cn(
                "flex size-7 items-center justify-center rounded-md transition-colors duration-150",
                eqOpen
                  ? "bg-accent/15 text-accent"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )}
            >
              <SlidersVertical className="size-4" />
            </button>
          </TooltipTrigger>
          <TooltipContent side="top">均衡器</TooltipContent>
        </Tooltip>
        <VolumeControl disabled={!hasTrack} />
      </div>
      <EqualizerPanel open={eqOpen} onClose={() => setEqOpen(false)} />
    </footer>
  );
}

/** 左段：48px 封面（get_cover，无封面回退图标）+ 标题/艺术家两行截断（定宽 240px）。 */
function TrackInfo() {
  const currentTrackId = usePlayerStore((s) => s.currentTrackId);
  const [current, setCurrent] = useState<Track | null>(null);
  const [coverUrl, setCoverUrl] = useState<string | null>(null);

  // 曲目信息：切歌时拉取一次（SQLite 单行查询 O(1)，不遍历前端数组）
  useEffect(() => {
    setCurrent(null);
    if (!currentTrackId) return;
    let cancelled = false;
    getTrack(currentTrackId)
      .then((t) => !cancelled && setCurrent(t))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [currentTrackId]);

  // 封面：随曲目切换拉取，base64 直接作 data URI（无封面/失败保持占位图标）
  useEffect(() => {
    setCoverUrl(null);
    if (!currentTrackId) return;
    let cancelled = false;
    getCover(currentTrackId)
      .then((c) => {
        if (!cancelled && c) {
          setCoverUrl(`data:${c.mimeType};base64,${c.dataBase64}`);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [currentTrackId]);

  /** 在线搜索封面（需要 artist + album）。 */
  const handleSearchCover = async () => {
    if (!current?.artist || !current?.album) {
      toast.error("无法搜索", { description: "当前曲目缺少艺术家或专辑信息" });
      return;
    }
    try {
      const cover = await searchCoverOnline(current.artist, current.album);
      setCoverUrl(`data:${cover.mimeType};base64,${cover.dataBase64}`);
      toast.success("封面已找到");
    } catch (err) {
      toast.error("未找到封面", { description: String(err) });
    }
  };

  return (
    <div className="flex w-60 shrink-0 items-center gap-3">
      <div className="group relative flex size-12 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted">
        {coverUrl ? (
          <img
            src={coverUrl}
            alt="封面"
            className="size-full object-cover"
            draggable={false}
          />
        ) : (
          <>
            <Music2 className="size-5 text-muted-foreground" />
            {/* 无封面时悬停显示在线搜索按钮 */}
            {current?.artist && current?.album && (
              <button
                type="button"
                onClick={() => void handleSearchCover()}
                className="absolute inset-0 hidden items-center justify-center bg-black/50 group-hover:flex"
                title="在线搜索封面"
              >
                <Globe className="size-4 text-white" />
              </button>
            )}
          </>
        )}
      </div>
      <div className="min-w-0">
        <p
          className={cn(
            "truncate text-[13px] font-medium",
            !current && "text-muted-foreground",
          )}
          title={current?.title}
        >
          {current?.title ?? "未在播放"}
        </p>
        <p
          className="truncate text-xs text-muted-foreground"
          title={current?.artist ?? ""}
        >
          {current?.artist ?? "—"}
        </p>
      </div>
    </div>
  );
}

/** 中段：上排控制钮（播放键 32px accent 圆钮），下排进度条 + 两端 mm:ss。 */
function TransportControls({ disabled }: { disabled: boolean }) {
  const status = usePlayerStore((s) => s.status);
  const positionMs = usePlayerStore((s) => s.positionMs);
  const durationMs = usePlayerStore((s) => s.durationMs);
  const playMode = usePlayerStore((s) => s.playMode);
  const toggle = usePlayerStore((s) => s.toggle);
  const next = usePlayerStore((s) => s.next);
  const previous = usePlayerStore((s) => s.previous);
  const seekTo = usePlayerStore((s) => s.seekTo);
  const cyclePlayMode = usePlayerStore((s) => s.cyclePlayMode);

  const [dragValue, setDragValue] = useState<number | null>(null);
  const lyricsOpen = usePlayerStore((s) => s.lyricsOpen);
  const toggleLyrics = usePlayerStore((s) => s.toggleLyrics);
  const sliderMax = Math.max(durationMs, 1);
  const shownMs = dragValue ?? Math.min(positionMs, sliderMax);

  const ModeIcon = PLAY_MODE_ICON[playMode];
  const playing = status === "playing";

  return (
    <div
      className={cn(
        "flex min-w-0 flex-1 flex-col items-center justify-center gap-1",
        disabled && "pointer-events-none opacity-40",
      )}
    >
      {/* 控制钮排 */}
      <div className="flex items-center gap-2">
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              aria-label={PLAY_MODE_LABEL[playMode]}
              onClick={() => void cyclePlayMode()}
              className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground"
            >
              <ModeIcon className="size-4" />
            </button>
          </TooltipTrigger>
          <TooltipContent>{PLAY_MODE_LABEL[playMode]}</TooltipContent>
        </Tooltip>

        <button
          type="button"
          aria-label="上一首"
          onClick={() => void previous()}
          className="flex size-7 items-center justify-center rounded-md text-foreground/80 transition-colors duration-150 hover:bg-muted hover:text-foreground"
        >
          <SkipBack className="size-4 fill-current" />
        </button>

        <button
          type="button"
          aria-label={playing ? "暂停" : "播放"}
          onClick={() => void toggle()}
          className="flex size-8 items-center justify-center rounded-full bg-accent text-accent-foreground shadow-sm transition-all duration-150 hover:bg-accent/90 active:scale-95"
        >
          {playing ? (
            <Pause className="size-4 fill-current" />
          ) : (
            <Play className="ml-0.5 size-4 fill-current" />
          )}
        </button>

        <button
          type="button"
          aria-label="下一首"
          onClick={() => void next()}
          className="flex size-7 items-center justify-center rounded-md text-foreground/80 transition-colors duration-150 hover:bg-muted hover:text-foreground"
        >
          <SkipForward className="size-4 fill-current" />
        </button>

        {/* 与左侧模式钮对称：歌词面板开关 */}
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              aria-label="歌词"
              aria-pressed={lyricsOpen}
              onClick={toggleLyrics}
              className={cn(
                "flex size-7 items-center justify-center rounded-md transition-colors duration-150 hover:bg-muted hover:text-foreground",
                lyricsOpen ? "text-accent" : "text-muted-foreground",
              )}
            >
              <MicVocal className="size-4" />
            </button>
          </TooltipTrigger>
          <TooltipContent>歌词</TooltipContent>
        </Tooltip>
      </div>

      {/* 进度条排：细轨 4px，悬停 6px 出拖拽柄 */}
      <div className="flex w-full max-w-[560px] items-center gap-2">
        <span className="tnum w-10 shrink-0 text-right text-xs text-muted-foreground">
          {formatDuration(shownMs)}
        </span>
        <Slider
          value={[shownMs]}
          min={0}
          max={sliderMax}
          step={500}
          disabled={disabled}
          onValueChange={([v]) => setDragValue(v)}
          onValueCommit={([v]) => {
            setDragValue(null);
            void seekTo(v);
          }}
          className={cn(
            "group/progress h-4 cursor-pointer",
            "[&_[data-slot=slider-track]]:h-1 [&_[data-slot=slider-track]]:transition-[height] [&_[data-slot=slider-track]]:duration-150",
            "[&_[data-slot=slider-track]]:group-hover/progress:h-1.5",
            "[&_[data-slot=slider-range]]:bg-accent",
            "[&_[data-slot=slider-thumb]]:size-3 [&_[data-slot=slider-thumb]]:border-accent [&_[data-slot=slider-thumb]]:bg-accent",
            "[&_[data-slot=slider-thumb]]:opacity-0 [&_[data-slot=slider-thumb]]:transition-opacity",
            "[&_[data-slot=slider-thumb]]:group-hover/progress:opacity-100",
          )}
        />
        <span className="tnum w-10 shrink-0 text-xs text-muted-foreground">
          {formatDuration(durationMs)}
        </span>
      </div>
    </div>
  );
}

/** 右段：音量图标 + 悬停展开滑条 + 滚轮调节（定宽 200px）。 */
function VolumeControl({ disabled }: { disabled: boolean }) {
  const volume = usePlayerStore((s) => s.volume);
  const muted = usePlayerStore((s) => s.muted);
  const changeVolume = usePlayerStore((s) => s.changeVolume);
  const toggleMute = usePlayerStore((s) => s.toggleMute);

  const containerRef = useRef<HTMLDivElement>(null);

  // 滚轮调节音量（非 passive，以便 preventDefault 阻止页面滚动）
  useEffect(() => {
    const el = containerRef.current;
    if (!el || disabled) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const { volume: v, changeVolume: cv } = usePlayerStore.getState();
      void cv(v + (e.deltaY < 0 ? 5 : -5));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [disabled]);

  const effective = muted ? 0 : volume;
  const VolumeIcon =
    muted || effective === 0
      ? VolumeX
      : effective < 34
        ? Volume
        : effective < 67
          ? Volume1
          : Volume2;

  return (
    <div
      ref={containerRef}
      className={cn(
        "group/volume flex w-50 shrink-0 items-center justify-end gap-1",
        disabled && "pointer-events-none opacity-40",
      )}
    >
      <button
        type="button"
        aria-label={muted ? "取消静音" : "静音"}
        onClick={() => void toggleMute()}
        className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground"
      >
        <VolumeIcon className="size-4" />
      </button>
      <div className="w-0 overflow-hidden opacity-0 transition-all duration-200 group-hover/volume:w-24 group-hover/volume:opacity-100">
        <Slider
          value={[effective]}
          min={0}
          max={100}
          step={1}
          onValueChange={([v]) => void changeVolume(v)}
          className={cn(
            "w-24 cursor-pointer",
            "[&_[data-slot=slider-range]]:bg-accent",
            "[&_[data-slot=slider-thumb]]:size-3 [&_[data-slot=slider-thumb]]:border-accent [&_[data-slot=slider-thumb]]:bg-accent",
          )}
        />
      </div>
      <span className="tnum w-8 text-right text-xs text-muted-foreground">
        {effective}
      </span>
    </div>
  );
}
