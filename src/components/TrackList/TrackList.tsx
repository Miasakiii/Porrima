import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, ListEnd, ListPlus, MoreHorizontal, Play, Volume2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { formatDuration, formatSampleRate } from "@/lib/format";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlayerStore } from "@/stores/playerStore";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { AddToPlaylistSub } from "@/components/Playlist/AddToPlaylistSub";
import type { SortBy, Track } from "@/lib/types";

const ROW_HEIGHT = 36;
/** 滚动到距底部该数量行以内时触发下一页加载 */
const LOAD_MORE_THRESHOLD = 50;

/** 网格列：悬停钮 / 标题 / 艺术家 / 专辑 / 时长 / 格式徽标 / 更多（设计规范 4.5） */
const GRID_COLS =
  "grid-cols-[28px_minmax(0,2fr)_minmax(0,1.2fr)_minmax(0,1.2fr)_64px_120px_28px]";

interface Column {
  key: SortBy | null;
  label: string;
}

const COLUMNS: Column[] = [
  { key: null, label: "" },
  { key: "title", label: "标题" },
  { key: "artist", label: "艺术家" },
  { key: "album", label: "专辑" },
  { key: "durationMs", label: "时长" },
  { key: null, label: "格式" },
  { key: null, label: "" },
];

export function TrackList() {
  const tracks = useLibraryStore((s) => s.tracks);
  const loading = useLibraryStore((s) => s.loading);
  const loadingMore = useLibraryStore((s) => s.loadingMore);
  const sortBy = useLibraryStore((s) => s.sortBy);
  const sortDir = useLibraryStore((s) => s.sortDir);
  const setSort = useLibraryStore((s) => s.setSort);
  const loadMore = useLibraryStore((s) => s.loadMore);

  const currentTrackId = usePlayerStore((s) => s.currentTrackId);
  const status = usePlayerStore((s) => s.status);
  const playFromList = usePlayerStore((s) => s.playFromList);

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: tracks.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });

  const items = virtualizer.getVirtualItems();
  const lastIndex = items.length > 0 ? items[items.length - 1].index : 0;

  // 接近底部时分页追加
  useEffect(() => {
    if (tracks.length > 0 && lastIndex >= tracks.length - LOAD_MORE_THRESHOLD) {
      void loadMore();
    }
  }, [lastIndex, tracks.length, loadMore]);

  if (loading) {
    return <TrackListSkeleton />;
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* 表头 */}
      <div
        className={cn(
          "grid h-9 shrink-0 items-center gap-2 border-b border-border px-3 text-xs text-muted-foreground",
          GRID_COLS,
        )}
      >
        {COLUMNS.map((col, i) => (
          <button
            key={i}
            type="button"
            disabled={!col.key}
            onClick={() => col.key && setSort(col.key)}
            className={cn(
              "flex items-center gap-1 truncate text-left select-none",
              col.key
                ? "cursor-pointer hover:text-foreground"
                : "cursor-default",
              col.key === sortBy && "text-foreground",
            )}
          >
            <span className="truncate">{col.label}</span>
            {col.key === sortBy &&
              (sortDir === "asc" ? (
                <ArrowUp className="size-3 shrink-0 text-accent" />
              ) : (
                <ArrowDown className="size-3 shrink-0 text-accent" />
              ))}
          </button>
        ))}
      </div>

      {/* 虚拟滚动区 */}
      <div ref={parentRef} className="min-h-0 flex-1 overflow-y-auto">
        <div
          className="relative w-full"
          style={{ height: virtualizer.getTotalSize() }}
        >
          {items.map((row) => {
            const track = tracks[row.index];
            if (!track) return null;
            return (
              <div
                key={track.id}
                className="absolute top-0 left-0 w-full"
                style={{
                  height: ROW_HEIGHT,
                  transform: `translateY(${row.start}px)`,
                }}
              >
                <TrackRow
                  track={track}
                  selected={track.id === selectedId}
                  isCurrent={track.id === currentTrackId}
                  isPlaying={track.id === currentTrackId && status === "playing"}
                  onSelect={() => setSelectedId(track.id)}
                  onPlay={() =>
                    void playFromList(
                      tracks.map((t) => t.id),
                      row.index,
                    )
                  }
                />
              </div>
            );
          })}
        </div>
        {loadingMore && (
          <div className="px-3 py-2 text-center text-xs text-muted-foreground">
            加载中…
          </div>
        )}
      </div>
    </div>
  );
}

function TrackRow({
  track,
  selected,
  isCurrent,
  isPlaying,
  onSelect,
  onPlay,
}: {
  track: Track;
  selected: boolean;
  isCurrent: boolean;
  isPlaying: boolean;
  onSelect: () => void;
  onPlay: () => void;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onDoubleClick={onPlay}
      onKeyDown={(e) => {
        if (e.key === "Enter") onPlay();
      }}
      className={cn(
        "group grid h-full cursor-default items-center gap-2 px-3 text-[13px] transition-colors duration-150",
        GRID_COLS,
        isCurrent
          ? "bg-accent/12"
          : selected
            ? "bg-muted"
            : "hover:bg-muted/60",
      )}
    >
      {/* 当前播放行：左侧 2px accent 指示条 */}
      {isCurrent && (
        <span className="absolute top-0 left-0 h-full w-0.5 bg-accent" />
      )}

      {/* 悬停播放钮 / 当前播放指示 */}
      <span className="flex items-center justify-center">
        {isCurrent && isPlaying ? (
          <Volume2 className="size-3.5 text-accent" />
        ) : (
          <button
            type="button"
            aria-label="播放"
            onClick={(e) => {
              e.stopPropagation();
              onPlay();
            }}
            className="text-muted-foreground opacity-0 transition-opacity duration-150 group-hover:opacity-100 hover:text-foreground"
          >
            <Play className="size-3.5 fill-current" />
          </button>
        )}
      </span>

      <span
        className={cn(
          "truncate",
          isCurrent ? "font-medium text-accent" : "text-foreground",
        )}
        title={track.title}
      >
        {track.title}
      </span>
      <span className="truncate text-muted-foreground" title={track.artist ?? ""}>
        {track.artist ?? "未知艺术家"}
      </span>
      <span className="truncate text-muted-foreground" title={track.album ?? ""}>
        {track.album ?? "未知专辑"}
      </span>
      <span className="tnum text-right text-xs text-muted-foreground">
        {formatDuration(track.durationMs)}
      </span>
      <span className="flex justify-end">
        <FormatBadge track={track} />
      </span>

      {/* 更多操作：加入队列 */}
      <TrackRowMenu track={track} />
    </div>
  );
}

/** 行尾悬停菜单：下一首播放 / 添加到队列末尾。 */
function TrackRowMenu({ track }: { track: Track }) {
  const addToQueue = usePlayerStore((s) => s.addToQueue);
  const [open, setOpen] = useState(false);

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label="更多操作"
          onClick={(e) => e.stopPropagation()}
          onDoubleClick={(e) => e.stopPropagation()}
          className={cn(
            "flex size-6 items-center justify-center rounded-md text-muted-foreground transition-opacity duration-150 hover:bg-muted hover:text-foreground",
            open ? "opacity-100" : "opacity-0 group-hover:opacity-100",
          )}
        >
          <MoreHorizontal className="size-3.5" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
        <DropdownMenuItem onClick={() => void addToQueue([track.id], true)}>
          <ListPlus className="size-4" />
          下一首播放
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => void addToQueue([track.id], false)}>
          <ListEnd className="size-4" />
          添加到播放队列
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <AddToPlaylistSub trackIds={[track.id]} />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** 格式徽标：无损/有损配色区分（设计规范 4.5：FLAC·24bit/96kHz 等）。 */
export function FormatBadge({ track }: { track: Track }) {
  const fmt = track.format.toUpperCase();
  let detail = "";
  if (track.isLossless) {
    if (track.bitDepth) detail += ` ${track.bitDepth}bit`;
    if (track.sampleRate) detail += `/${formatSampleRate(track.sampleRate)}`;
  } else if (track.bitrate) {
    detail = ` ${track.bitrate}k`;
  }
  return (
    <Badge
      variant="outline"
      className={cn(
        "tnum h-5 px-1.5 text-[10px] font-medium",
        track.isLossless
          ? "border-lossless/40 text-lossless"
          : "border-lossy/40 text-lossy",
      )}
    >
      {fmt}
      {detail}
    </Badge>
  );
}

/** 加载骨架行（设计规范 4.5）。 */
function TrackListSkeleton() {
  return (
    <div className="flex h-full flex-col">
      <div
        className={cn(
          "grid h-9 shrink-0 items-center gap-2 border-b border-border px-3",
          GRID_COLS,
        )}
      >
        {COLUMNS.map((col, i) => (
          <span key={i} className="truncate text-xs text-muted-foreground">
            {col.label}
          </span>
        ))}
      </div>
      <div className="flex-1 space-y-1 p-2">
        {Array.from({ length: 20 }).map((_, i) => (
          <div key={i} className={cn("grid h-9 items-center gap-2 px-1", GRID_COLS)}>
            <span />
            <Skeleton className="h-3.5 w-3/4" />
            <Skeleton className="h-3.5 w-1/2" />
            <Skeleton className="h-3.5 w-1/2" />
            <Skeleton className="h-3.5 w-10 justify-self-end" />
            <Skeleton className="h-4 w-14 justify-self-end" />
            <span />
          </div>
        ))}
      </div>
    </div>
  );
}
