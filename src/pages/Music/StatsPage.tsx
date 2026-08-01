import { useEffect, useState } from "react";
import { BarChart3, Clock, Disc3, Music2, Play } from "lucide-react";
import { cn } from "@/lib/utils";
import { formatDuration } from "@/lib/format";
import {
  getStatsSummary,
  listMostPlayed,
  listRecentlyPlayed,
} from "@/lib/commands";
import { usePlayerStore } from "@/stores/playerStore";
import { DetailTrackList } from "@/components/TrackList/DetailTrackList";
import { Skeleton } from "@/components/ui/skeleton";
import type { StatsSummary, Track } from "@/lib/types";

const LIST_LIMIT = 30;

/**
 * 统计页（Phase 3）：概览卡片 + 格式分布 + 常听排行 + 最近播放。
 * 数据一次性拉取（get_stats_summary / list_most_played / list_recently_played）。
 * 播放计数由后端在播放达阈值（≥30s 或 ≥50%）时自动记录。
 */
export function StatsPage() {
  const [summary, setSummary] = useState<StatsSummary | null>(null);
  const [mostPlayed, setMostPlayed] = useState<Track[]>([]);
  const [recent, setRecent] = useState<Track[]>([]);
  const [loading, setLoading] = useState(true);
  const playFromList = usePlayerStore((s) => s.playFromList);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    Promise.all([
      getStatsSummary(),
      listMostPlayed(LIST_LIMIT),
      listRecentlyPlayed(LIST_LIMIT),
    ])
      .then(([s, top, rec]) => {
        if (cancelled) return;
        setSummary(s);
        setMostPlayed(top);
        setRecent(rec);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (loading) {
    return <StatsSkeleton />;
  }

  if (!summary || summary.trackCount === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground">
        <BarChart3 className="size-10 opacity-40" />
        <p className="text-base font-medium text-foreground">统计</p>
        <p className="text-sm">媒体库为空 — 先在设置中添加目录并扫描</p>
      </div>
    );
  }

  const losslessPct =
    summary.trackCount > 0
      ? Math.round((summary.losslessCount / summary.trackCount) * 100)
      : 0;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="shrink-0 px-6 pt-5 pb-3">
        <h1 className="text-xl font-semibold tracking-tight">统计</h1>
      </header>

      <div className="min-h-0 flex-1 space-y-6 overflow-y-auto px-6 pb-6">
        {/* 概览卡片 */}
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <StatCard icon={Music2} label="曲目总数" value={`${summary.trackCount}`} />
          <StatCard icon={Clock} label="总时长" value={formatDuration(summary.totalDurationMs)} />
          <StatCard
            icon={Disc3}
            label="无损占比"
            value={`${losslessPct}%`}
            sub={`${summary.losslessCount}/${summary.trackCount}`}
          />
          <StatCard
            icon={Play}
            label="总播放次数"
            value={`${summary.totalPlays}`}
            sub={`${summary.playedCount} 首听过`}
          />
        </div>

        {/* 格式分布 */}
        {summary.formats.length > 0 && (
          <section className="space-y-2">
            <h2 className="text-sm font-medium text-muted-foreground">格式分布</h2>
            <div className="space-y-1.5">
              {summary.formats.map((f) => (
                <div key={f.format} className="flex items-center gap-3">
                  <span className="w-14 shrink-0 text-xs font-medium uppercase">
                    {f.format}
                  </span>
                  <div className="h-2 flex-1 overflow-hidden rounded-full bg-muted">
                    <div
                      className="h-full rounded-full bg-accent"
                      style={{
                        width: `${Math.max(2, (f.count / summary.trackCount) * 100)}%`,
                      }}
                    />
                  </div>
                  <span className="tnum w-10 shrink-0 text-right text-xs text-muted-foreground">
                    {f.count}
                  </span>
                </div>
              ))}
            </div>
          </section>
        )}

        {/* 常听排行 */}
        <StatSection
          icon={BarChart3}
          title="常听排行"
          empty={mostPlayed.length === 0}
          emptyHint="还没有播放记录"
        >
          <DetailTrackList
            tracks={mostPlayed}
            showAlbum
            onPlayIndex={(i) => void playFromList(mostPlayed.map((t) => t.id), i)}
          />
        </StatSection>

        {/* 最近播放 */}
        <StatSection
          icon={Clock}
          title="最近播放"
          empty={recent.length === 0}
          emptyHint="还没有播放记录"
        >
          <DetailTrackList
            tracks={recent}
            showAlbum
            onPlayIndex={(i) => void playFromList(recent.map((t) => t.id), i)}
          />
        </StatSection>
      </div>
    </div>
  );
}

function StatCard({
  icon: Icon,
  label,
  value,
  sub,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  value: string;
  sub?: string;
}) {
  return (
    <div className="rounded-lg border border-border bg-card p-3">
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Icon className="size-3.5" />
        {label}
      </div>
      <div className="mt-1.5 truncate text-xl font-semibold tracking-tight tnum">
        {value}
      </div>
      {sub && <div className="tnum truncate text-xs text-muted-foreground">{sub}</div>}
    </div>
  );
}

function StatSection({
  icon: Icon,
  title,
  empty,
  emptyHint,
  children,
}: {
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  empty: boolean;
  emptyHint: string;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-2">
      <h2 className="flex items-center gap-1.5 text-sm font-medium text-muted-foreground">
        <Icon className="size-4" />
        {title}
      </h2>
      {empty ? (
        <p className={cn("px-2 py-4 text-sm text-muted-foreground")}>{emptyHint}</p>
      ) : (
        children
      )}
    </section>
  );
}

function StatsSkeleton() {
  return (
    <div className="flex h-full flex-col">
      <div className="px-6 pt-5 pb-3">
        <Skeleton className="h-6 w-16" />
      </div>
      <div className="space-y-6 px-6">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-16 rounded-lg" />
          ))}
        </div>
        <Skeleton className="h-24 w-full rounded-lg" />
        <Skeleton className="h-40 w-full rounded-lg" />
      </div>
    </div>
  );
}
