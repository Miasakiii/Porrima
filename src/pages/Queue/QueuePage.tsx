import { useEffect, useMemo, useRef, useState } from "react";
import { GripVertical, ListMusic, Play, Trash2, Volume2, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { formatDuration } from "@/lib/format";
import { getTracks } from "@/lib/commands";
import { usePlayerStore } from "@/stores/playerStore";
import type { Track } from "@/lib/types";

/**
 * 播放队列页（Phase 2）：当前队列展示 + 双击跳播 + 拖拽排序 + 移除/清空。
 * 队列以 playerStore 的 queue（id 列表）为准，元数据经 get_tracks 批量拉取。
 */
export function QueuePage() {
  const queue = usePlayerStore((s) => s.queue);
  const queueIndex = usePlayerStore((s) => s.queueIndex);
  const currentTrackId = usePlayerStore((s) => s.currentTrackId);
  const status = usePlayerStore((s) => s.status);
  const playTrack = usePlayerStore((s) => s.playTrack);
  const removeFromQueue = usePlayerStore((s) => s.removeFromQueue);
  const moveInQueue = usePlayerStore((s) => s.moveInQueue);
  const clearQueue = usePlayerStore((s) => s.clearQueue);

  // id → Track 元数据缓存（队列变化时增量补拉）
  const [trackMap, setTrackMap] = useState<Map<string, Track>>(new Map());
  useEffect(() => {
    const missing = [...new Set(queue)].filter((id) => !trackMap.has(id));
    if (missing.length === 0) return;
    let cancelled = false;
    getTracks(missing)
      .then((tracks) => {
        if (cancelled || tracks.length === 0) return;
        setTrackMap((prev) => {
          const next = new Map(prev);
          for (const t of tracks) next.set(t.id, t);
          return next;
        });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [queue, trackMap]);

  const totalMs = useMemo(
    () =>
      queue.reduce((sum, id) => sum + (trackMap.get(id)?.durationMs ?? 0), 0),
    [queue, trackMap],
  );

  // 拖拽状态：源索引 + 当前悬停目标索引（视觉指示）
  const dragFromRef = useRef<number | null>(null);
  const [dropTarget, setDropTarget] = useState<number | null>(null);

  if (queue.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground">
        <ListMusic className="size-10 opacity-40" />
        <p className="text-base font-medium text-foreground">播放队列</p>
        <p className="text-sm">队列为空 — 在曲库双击播放，或右键加入队列</p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* 页头：标题 + 统计 + 清空 */}
      <div className="flex shrink-0 items-center justify-between border-b border-border px-4 py-3">
        <div className="flex items-baseline gap-3">
          <h1 className="text-base font-semibold">播放队列</h1>
          <span className="tnum text-xs text-muted-foreground">
            {queue.length} 首 · {formatDuration(totalMs)}
          </span>
        </div>
        <button
          type="button"
          onClick={() => void clearQueue()}
          className="flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-muted-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground"
        >
          <Trash2 className="size-3.5" />
          清空队列
        </button>
      </div>

      {/* 队列行（队列规模有限，不做虚拟滚动；超长队列后续再优化） */}
      <div className="min-h-0 flex-1 overflow-y-auto py-1">
        {queue.map((id, index) => {
          const track = trackMap.get(id);
          const isCurrent = index === queueIndex && id === currentTrackId;
          return (
            <QueueRow
              key={`${id}-${index}`}
              index={index}
              track={track}
              trackId={id}
              isCurrent={isCurrent}
              isPlaying={isCurrent && status === "playing"}
              isDropTarget={dropTarget === index}
              onPlay={() => void playTrack(id)}
              onRemove={() => void removeFromQueue(index)}
              onDragStart={() => {
                dragFromRef.current = index;
              }}
              onDragOver={(e) => {
                e.preventDefault();
                if (dragFromRef.current !== null && dragFromRef.current !== index) {
                  setDropTarget(index);
                }
              }}
              onDrop={() => {
                const from = dragFromRef.current;
                dragFromRef.current = null;
                setDropTarget(null);
                if (from !== null && from !== index) {
                  void moveInQueue(from, index);
                }
              }}
              onDragEnd={() => {
                dragFromRef.current = null;
                setDropTarget(null);
              }}
            />
          );
        })}
      </div>
    </div>
  );
}

function QueueRow({
  index,
  track,
  trackId,
  isCurrent,
  isPlaying,
  isDropTarget,
  onPlay,
  onRemove,
  onDragStart,
  onDragOver,
  onDrop,
  onDragEnd,
}: {
  index: number;
  track: Track | undefined;
  trackId: string;
  isCurrent: boolean;
  isPlaying: boolean;
  isDropTarget: boolean;
  onPlay: () => void;
  onRemove: () => void;
  onDragStart: () => void;
  onDragOver: (e: React.DragEvent) => void;
  onDrop: () => void;
  onDragEnd: () => void;
}) {
  return (
    <div
      draggable
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDrop={onDrop}
      onDragEnd={onDragEnd}
      onDoubleClick={onPlay}
      className={cn(
        "group relative mx-1 grid h-11 cursor-default items-center gap-2 rounded-md px-2 text-[13px] transition-colors duration-150",
        "grid-cols-[16px_28px_minmax(0,2fr)_minmax(0,1.2fr)_56px_28px]",
        isCurrent ? "bg-accent/12" : "hover:bg-muted/60",
        isDropTarget && "border-t-2 border-accent",
      )}
    >
      {/* 拖拽把手 */}
      <span className="cursor-grab text-muted-foreground/50 opacity-0 transition-opacity duration-150 group-hover:opacity-100 active:cursor-grabbing">
        <GripVertical className="size-3.5" />
      </span>

      {/* 序号 / 播放中指示 / 悬停播放钮 */}
      <span className="flex items-center justify-center">
        {isCurrent && isPlaying ? (
          <Volume2 className="size-3.5 text-accent" />
        ) : (
          <>
            <span className="tnum text-xs text-muted-foreground group-hover:hidden">
              {index + 1}
            </span>
            <button
              type="button"
              aria-label="播放"
              onClick={(e) => {
                e.stopPropagation();
                onPlay();
              }}
              className="hidden text-muted-foreground group-hover:inline-flex hover:text-foreground"
            >
              <Play className="size-3.5 fill-current" />
            </button>
          </>
        )}
      </span>

      <span
        className={cn(
          "truncate",
          isCurrent ? "font-medium text-accent" : "text-foreground",
        )}
        title={track?.title ?? trackId}
      >
        {track?.title ?? "（曲目已不在库中）"}
      </span>
      <span
        className="truncate text-muted-foreground"
        title={track?.artist ?? ""}
      >
        {track?.artist ?? "—"}
      </span>
      <span className="tnum text-right text-xs text-muted-foreground">
        {track ? formatDuration(track.durationMs) : "--:--"}
      </span>

      {/* 移出队列 */}
      <button
        type="button"
        aria-label="移出队列"
        onClick={(e) => {
          e.stopPropagation();
          onRemove();
        }}
        className="flex items-center justify-center text-muted-foreground opacity-0 transition-opacity duration-150 group-hover:opacity-100 hover:text-foreground"
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}
