import { useState } from "react";
import { ListEnd, ListPlus, MoreHorizontal, Play, Volume2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { formatDuration } from "@/lib/format";
import { usePlayerStore } from "@/stores/playerStore";
import { FormatBadge } from "@/components/TrackList/TrackList";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { AddToPlaylistSub } from "@/components/Playlist/AddToPlaylistSub";
import type { Track } from "@/lib/types";

/**
 * 详情页曲目列表（专辑/艺术家详情复用）。
 *
 * 与曲库主列表（虚拟滚动）不同，这里曲目数有界（单专辑/单艺术家），用普通渲染。
 * onPlayIndex 由调用方以整个 tracks 为队列起播（play_queue）。
 * showAlbum=true 时展示专辑列（艺术家详情用），否则用轨号占位。
 */
export function DetailTrackList({
  tracks,
  onPlayIndex,
  showAlbum = false,
}: {
  tracks: Track[];
  onPlayIndex: (index: number) => void;
  showAlbum?: boolean;
}) {
  const currentTrackId = usePlayerStore((s) => s.currentTrackId);
  const status = usePlayerStore((s) => s.status);

  const gridCols = showAlbum
    ? "grid-cols-[28px_minmax(0,2fr)_minmax(0,1.4fr)_64px_120px_28px]"
    : "grid-cols-[28px_minmax(0,3fr)_64px_120px_28px]";

  return (
    <div className="py-1">
      {tracks.map((track, index) => {
        const isCurrent = track.id === currentTrackId;
        const isPlaying = isCurrent && status === "playing";
        return (
          <div
            key={track.id}
            role="button"
            tabIndex={0}
            onDoubleClick={() => onPlayIndex(index)}
            onKeyDown={(e) => {
              if (e.key === "Enter") onPlayIndex(index);
            }}
            className={cn(
              "group grid h-10 cursor-default items-center gap-2 rounded-md px-2 text-[13px] transition-colors duration-150",
              gridCols,
              isCurrent ? "bg-accent/12" : "hover:bg-muted/60",
            )}
          >
            {/* 轨号 / 悬停播放钮 / 当前播放指示 */}
            <span className="flex items-center justify-center">
              {isPlaying ? (
                <Volume2 className="size-3.5 text-accent" />
              ) : (
                <>
                  <span className="tnum text-xs text-muted-foreground group-hover:hidden">
                    {track.trackNumber ?? index + 1}
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

            {showAlbum && (
              <span
                className="truncate text-muted-foreground"
                title={track.album ?? ""}
              >
                {track.album ?? "未知专辑"}
              </span>
            )}

            <span className="tnum text-right text-xs text-muted-foreground">
              {formatDuration(track.durationMs)}
            </span>
            <span className="flex justify-end">
              <FormatBadge track={track} />
            </span>

            <RowMenu track={track} />
          </div>
        );
      })}
    </div>
  );
}

/** 行尾悬停菜单：下一首播放 / 添加到队列末尾。 */
function RowMenu({ track }: { track: Track }) {
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
