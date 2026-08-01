import { useEffect, useState } from "react";
import { ArrowLeft, Disc3, Play } from "lucide-react";
import { formatDuration } from "@/lib/format";
import { getAlbumTracks, listAlbums } from "@/lib/commands";
import { usePlayerStore } from "@/stores/playerStore";
import { AlbumArt } from "@/components/AlbumArt";
import { DetailTrackList } from "@/components/TrackList/DetailTrackList";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import type { AlbumSummary, Track } from "@/lib/types";

/**
 * 专辑页（Phase 2）：master 网格 + detail 详情。
 * master 一次性拉取全部专辑摘要（list_albums），封面走 AlbumArt 懒加载。
 * 点击进入详情：拉取该专辑曲目（get_album_tracks），支持播放全部 / 双击起播。
 */
export function AlbumsPage() {
  const [albums, setAlbums] = useState<AlbumSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<AlbumSummary | null>(null);
  const playFromList = usePlayerStore((s) => s.playFromList);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listAlbums()
      .then((list) => {
        if (!cancelled) setAlbums(list);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // 播放整张专辑：先拉取曲目再以其为队列起播。
  const playAlbum = async (album: AlbumSummary) => {
    try {
      const tracks = await getAlbumTracks(album.name, album.albumArtist);
      if (tracks.length > 0) await playFromList(tracks.map((t) => t.id), 0);
    } catch {
      /* 失败静默：playFromList 内部已有 toast */
    }
  };

  if (selected) {
    return <AlbumDetail album={selected} onBack={() => setSelected(null)} />;
  }

  if (loading) {
    return <AlbumsGridSkeleton />;
  }

  if (albums.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground">
        <Disc3 className="size-10 opacity-40" />
        <p className="text-base font-medium text-foreground">专辑</p>
        <p className="text-sm">曲库还没有专辑 — 先在设置中添加目录并扫描</p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex shrink-0 items-baseline gap-3 px-6 pt-5 pb-3">
        <h1 className="text-xl font-semibold tracking-tight">专辑</h1>
        <span className="tnum text-xs text-muted-foreground">{albums.length} 张</span>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-4">
        <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-x-4 gap-y-5">
          {albums.map((album) => (
            <AlbumCard
              key={album.id}
              album={album}
              onOpen={() => setSelected(album)}
              onPlay={() => void playAlbum(album)}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function AlbumCard({
  album,
  onOpen,
  onPlay,
}: {
  album: AlbumSummary;
  onOpen: () => void;
  onPlay: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onOpen}
      className="group flex flex-col gap-2 text-left"
    >
      <div className="relative">
        <AlbumArt
          trackId={album.coverTrackId}
          className="aspect-square w-full rounded-lg shadow-sm transition-shadow duration-200 group-hover:shadow-md"
        />
        <span
          role="button"
          aria-label="播放专辑"
          onClick={(e) => {
            e.stopPropagation();
            onPlay();
          }}
          className="absolute right-2 bottom-2 flex size-9 translate-y-1 items-center justify-center rounded-full bg-accent text-accent-foreground opacity-0 shadow-lg transition-all duration-200 hover:scale-105 group-hover:translate-y-0 group-hover:opacity-100"
        >
          <Play className="size-4 fill-current" />
        </span>
      </div>
      <div className="min-w-0">
        <p className="truncate text-[13px] font-medium" title={album.name ?? "未知专辑"}>
          {album.name ?? "未知专辑"}
        </p>
        <p
          className="truncate text-xs text-muted-foreground"
          title={album.albumArtist ?? "未知艺术家"}
        >
          {album.albumArtist ?? "未知艺术家"}
        </p>
      </div>
    </button>
  );
}

function AlbumDetail({
  album,
  onBack,
}: {
  album: AlbumSummary;
  onBack: () => void;
}) {
  const [tracks, setTracks] = useState<Track[]>([]);
  const playFromList = usePlayerStore((s) => s.playFromList);

  useEffect(() => {
    let cancelled = false;
    getAlbumTracks(album.name, album.albumArtist)
      .then((list) => {
        if (!cancelled) setTracks(list);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [album.name, album.albumArtist]);

  const playAll = () => {
    if (tracks.length > 0) void playFromList(tracks.map((t) => t.id), 0);
  };

  const meta = [
    album.year ? String(album.year) : null,
    `${album.trackCount} 首`,
    formatDuration(album.totalDurationMs),
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* 详情页头：返回 + 封面 + 元信息 + 播放全部 */}
      <header className="shrink-0 px-6 pt-4 pb-3">
        <button
          type="button"
          onClick={onBack}
          className="mb-4 flex items-center gap-1 text-sm text-muted-foreground transition-colors hover:text-foreground"
        >
          <ArrowLeft className="size-4" />
          专辑
        </button>
        <div className="flex gap-5">
          <AlbumArt
            trackId={album.coverTrackId}
            lazy={false}
            className="size-36 shrink-0 rounded-lg shadow-md"
          />
          <div className="flex min-w-0 flex-col justify-end gap-2 pb-1">
            <h1 className="truncate text-2xl font-semibold tracking-tight">
              {album.name ?? "未知专辑"}
            </h1>
            <p className="truncate text-sm text-muted-foreground">
              {album.albumArtist ?? "未知艺术家"}
            </p>
            <p className="tnum text-xs text-muted-foreground">{meta}</p>
            <div className="mt-1">
              <Button size="sm" onClick={playAll} disabled={tracks.length === 0}>
                <Play className="size-4 fill-current" />
                播放全部
              </Button>
            </div>
          </div>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-2">
        <DetailTrackList
          tracks={tracks}
          onPlayIndex={(i) => void playFromList(tracks.map((t) => t.id), i)}
        />
      </div>
    </div>
  );
}

function AlbumsGridSkeleton() {
  return (
    <div className="flex h-full flex-col">
      <div className="px-6 pt-5 pb-3">
        <Skeleton className="h-6 w-24" />
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-x-4 gap-y-5 px-6">
        {Array.from({ length: 12 }).map((_, i) => (
          <div key={i} className="flex flex-col gap-2">
            <Skeleton className="aspect-square w-full rounded-lg" />
            <Skeleton className="h-3.5 w-3/4" />
            <Skeleton className="h-3 w-1/2" />
          </div>
        ))}
      </div>
    </div>
  );
}
