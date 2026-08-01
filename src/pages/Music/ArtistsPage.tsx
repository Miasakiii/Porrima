import { useEffect, useState } from "react";
import { ArrowLeft, ChevronRight, Play, User, Users } from "lucide-react";
import { formatDuration } from "@/lib/format";
import { getArtistTracks, listArtists } from "@/lib/commands";
import { usePlayerStore } from "@/stores/playerStore";
import { DetailTrackList } from "@/components/TrackList/DetailTrackList";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import type { ArtistSummary, Track } from "@/lib/types";

/**
 * 艺术家页（Phase 2）：master 列表 + detail 详情。
 * master 拉取全部艺术家摘要（list_artists）。
 * 点击进入详情：拉取该艺术家全部曲目（get_artist_tracks，跨专辑按专辑/轨号排序），
 * 详情列表展示专辑列，支持播放全部 / 双击起播。
 */
export function ArtistsPage() {
  const [artists, setArtists] = useState<ArtistSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<ArtistSummary | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listArtists()
      .then((list) => {
        if (!cancelled) setArtists(list);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (selected) {
    return <ArtistDetail artist={selected} onBack={() => setSelected(null)} />;
  }

  if (loading) {
    return <ArtistsListSkeleton />;
  }

  if (artists.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground">
        <Users className="size-10 opacity-40" />
        <p className="text-base font-medium text-foreground">艺术家</p>
        <p className="text-sm">曲库还没有艺术家 — 先在设置中添加目录并扫描</p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex shrink-0 items-baseline gap-3 px-6 pt-5 pb-3">
        <h1 className="text-xl font-semibold tracking-tight">艺术家</h1>
        <span className="tnum text-xs text-muted-foreground">{artists.length} 位</span>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-3">
        {artists.map((artist) => (
          <ArtistRow
            key={artist.name ?? "\u0000unknown"}
            artist={artist}
            onOpen={() => setSelected(artist)}
          />
        ))}
      </div>
    </div>
  );
}

function ArtistRow({
  artist,
  onOpen,
}: {
  artist: ArtistSummary;
  onOpen: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onOpen}
      className="group flex w-full items-center gap-3 rounded-md px-3 py-2 text-left transition-colors duration-150 hover:bg-muted/60"
    >
      <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground">
        <User className="size-5" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[13px] font-medium">
          {artist.name ?? "未知艺术家"}
        </span>
        <span className="tnum block truncate text-xs text-muted-foreground">
          {artist.albumCount} 张专辑 · {artist.trackCount} 首
        </span>
      </span>
      <ChevronRight className="size-4 shrink-0 text-muted-foreground/50 transition-transform duration-150 group-hover:translate-x-0.5" />
    </button>
  );
}

function ArtistDetail({
  artist,
  onBack,
}: {
  artist: ArtistSummary;
  onBack: () => void;
}) {
  const [tracks, setTracks] = useState<Track[]>([]);
  const playFromList = usePlayerStore((s) => s.playFromList);

  useEffect(() => {
    let cancelled = false;
    getArtistTracks(artist.name)
      .then((list) => {
        if (!cancelled) setTracks(list);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [artist.name]);

  const playAll = () => {
    if (tracks.length > 0) void playFromList(tracks.map((t) => t.id), 0);
  };

  const meta = `${artist.albumCount} 张专辑 · ${artist.trackCount} 首 · ${formatDuration(
    artist.totalDurationMs,
  )}`;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="shrink-0 px-6 pt-4 pb-3">
        <button
          type="button"
          onClick={onBack}
          className="mb-4 flex items-center gap-1 text-sm text-muted-foreground transition-colors hover:text-foreground"
        >
          <ArrowLeft className="size-4" />
          艺术家
        </button>
        <div className="flex items-end gap-5">
          <span className="flex size-28 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground shadow-md">
            <User className="size-12" />
          </span>
          <div className="flex min-w-0 flex-col gap-2 pb-1">
            <h1 className="truncate text-2xl font-semibold tracking-tight">
              {artist.name ?? "未知艺术家"}
            </h1>
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
          showAlbum
          onPlayIndex={(i) => void playFromList(tracks.map((t) => t.id), i)}
        />
      </div>
    </div>
  );
}

function ArtistsListSkeleton() {
  return (
    <div className="flex h-full flex-col">
      <div className="px-6 pt-5 pb-3">
        <Skeleton className="h-6 w-24" />
      </div>
      <div className="space-y-1 px-3">
        {Array.from({ length: 10 }).map((_, i) => (
          <div key={i} className="flex items-center gap-3 px-3 py-2">
            <Skeleton className="size-10 shrink-0 rounded-full" />
            <div className="flex-1 space-y-1.5">
              <Skeleton className="h-3.5 w-1/3" />
              <Skeleton className="h-3 w-1/4" />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
