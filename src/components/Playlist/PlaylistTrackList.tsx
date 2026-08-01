import { useRef, useState } from "react";
import {
  GripVertical,
  ListEnd,
  ListPlus,
  MoreHorizontal,
  Play,
  Volume2,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { formatDuration } from "@/lib/format";
import { usePlayerStore } from "@/stores/playerStore";
import { FormatBadge } from "@/components/TrackList/TrackList";
import { AddToPlaylistSub } from "@/components/Playlist/AddToPlaylistSub";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { Track } from "@/lib/types";

const GRID = "grid-cols-[16px_28px_minmax(0,2fr)_minmax(0,1.2fr)_64px_110px_28px]";

/**
 * 播放列表详情曲目列表：拖拽重排 + 移除 + 双击/悬停起播 + 行尾菜单（队列/加入其它列表）。
 * 曲目数有界，普通渲染；起播以整个列表为队列（play_queue）。
 */
export function PlaylistTrackList({
  tracks,
  onPlayIndex,
  onRemove,
  onMove,
}: {
  tracks: Track[];
  onPlayIndex: (index: number) => void;
  onRemove: (index: number) => void;
  onMove: (from: number, to: number) => void;
}) {
  const currentTrackId = usePlayerStore((s) => s.currentTrackId);
  const status = usePlayerStore((s) => s.status);
  const dragFromRef = useRef<number | null>(null);
  const [dropTarget, setDropTarget] = useState<number | null>(null);

  return (
    <div className="py-1">
      {tracks.map((track, index) => {
        const isCurrent = track.id === currentTrackId;
        const isPlaying = isCurrent && status === "playing";
        return (
          <div
            key={`${track.id}-${index}`}
            draggable
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
              if (from !== null && from !== index) onMove(from, index);
            }}
            onDragEnd={() => {
              dragFromRef.current = null;
              setDropTarget(null);
            }}
            onDoubleClick={() => onPlayIndex(index)}
            className={cn(
              "group grid h-11 cursor-default items-center gap-2 rounded-md px-2 text-[13px] transition-colors duration-150",
              GRID,
              isCurrent ? "bg-accent/12" : "hover:bg-muted/60",
              dropTarget === index && "border-t-2 border-accent",
            )}
          >
            <span className="cursor-grab text-muted-foreground/50 opacity-0 transition-opacity duration-150 group-hover:opacity-100 active:cursor-grabbing">
              <GripVertical className="size-3.5" />
            </span>

            <span className="flex items-center justify-center">
              {isPlaying ? (
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
                      onPlayIndex(index);
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
              title={track.title}
            >
              {track.title}
            </span>
            <span className="truncate text-muted-foreground" title={track.artist ?? ""}>
              {track.artist ?? "未知艺术家"}
            </span>
            <span className="tnum text-right text-xs text-muted-foreground">
              {formatDuration(track.durationMs)}
            </span>
            <span className="flex justify-end">
              <FormatBadge track={track} />
            </span>

            <RowMenu track={track} index={index} onRemove={onRemove} />
          </div>
        );
      })}
    </div>
  );
}

function RowMenu({
  track,
  index,
  onRemove,
}: {
  track: Track;
  index: number;
  onRemove: (index: number) => void;
}) {
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
        <AddToPlaylistSub trackIds={[track.id]} />
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" onClick={() => onRemove(index)}>
          <X className="size-4" />
          从播放列表移除
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
