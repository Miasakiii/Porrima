import { useEffect, useRef, useState } from "react";
import { Globe, MicVocal } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { getLyrics, getTrack, saveLyricsFile, searchLyricsOnline } from "@/lib/commands";
import { findActiveLine, parseLrc, type ParsedLyrics } from "@/lib/lrc";
import { usePlayerStore } from "@/stores/playerStore";

/** 手动滚动后暂停自动跟随的时长 */
const MANUAL_SCROLL_HOLD_MS = 3000;

/**
 * 歌词面板（Phase 2）：覆盖内容区的浮层，PlayerBar 歌词钮开关。
 * LRC 有时间轴 → 当前行高亮 + 自动居中滚动；纯文本 → 静态居中列表。
 */
export function LyricsPanel() {
  const open = usePlayerStore((s) => s.lyricsOpen);
  const currentTrackId = usePlayerStore((s) => s.currentTrackId);

  const [lyrics, setLyrics] = useState<ParsedLyrics | null>(null);
  const [loading, setLoading] = useState(false);

  // 曲目切换或面板打开时拉取歌词
  useEffect(() => {
    setLyrics(null);
    if (!open || !currentTrackId) return;
    let cancelled = false;
    setLoading(true);
    getLyrics(currentTrackId)
      .then((payload) => {
        if (cancelled) return;
        setLyrics(payload ? parseLrc(payload.text) : null);
      })
      .catch(() => !cancelled && setLyrics(null))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [open, currentTrackId]);

  if (!open) return null;

  return (
    <div className="absolute inset-0 z-10 flex flex-col bg-background/95 backdrop-blur-sm">
      {!currentTrackId || loading || !lyrics || lyrics.lines.length === 0 ? (
        <EmptyState loading={loading} hasTrack={currentTrackId != null} />
      ) : lyrics.synced ? (
        <SyncedLyrics lyrics={lyrics} />
      ) : (
        <PlainLyrics lyrics={lyrics} />
      )}
    </div>
  );
}

function EmptyState({ loading, hasTrack }: { loading: boolean; hasTrack: boolean }) {
  const currentTrackId = usePlayerStore((s) => s.currentTrackId);
  const [searching, setSearching] = useState(false);

  const handleSearchOnline = async () => {
    if (!currentTrackId) return;
    setSearching(true);
    try {
      const track = await getTrack(currentTrackId);
      const result = await searchLyricsOnline(track.title, track.artist, track.album);
      const text = result.syncedLyrics ?? result.plainLyrics;
      if (text) {
        await saveLyricsFile(currentTrackId, text);
        toast.success("歌词已保存", { description: "已写入同目录 .lrc 文件" });
        // 触发重新加载（通过切换面板状态）
        const store = usePlayerStore.getState();
        store.toggleLyrics();
        setTimeout(() => usePlayerStore.getState().toggleLyrics(), 50);
      }
    } catch (err) {
      toast.error("未找到歌词", { description: String(err) });
    } finally {
      setSearching(false);
    }
  };

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 text-muted-foreground">
      <MicVocal className="size-8 opacity-50" />
      <p className="text-sm">
        {!hasTrack ? "未在播放" : loading ? "加载歌词中…" : "暂无歌词"}
      </p>
      {hasTrack && !loading && (
        <button
          type="button"
          onClick={() => void handleSearchOnline()}
          disabled={searching}
          className="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-xs text-foreground transition-colors hover:bg-muted disabled:opacity-50"
        >
          <Globe className="size-3.5" />
          {searching ? "搜索中…" : "在线搜索歌词"}
        </button>
      )}
    </div>
  );
}

/** 有时间轴：当前行高亮 + 自动居中滚动（手动滚动后暂停跟随 3s）。 */
function SyncedLyrics({ lyrics }: { lyrics: ParsedLyrics }) {
  const positionMs = usePlayerStore((s) => s.positionMs);
  const activeIndex = findActiveLine(lyrics.lines, positionMs);

  const containerRef = useRef<HTMLDivElement>(null);
  const activeRef = useRef<HTMLParagraphElement>(null);
  const manualUntilRef = useRef(0);

  // 自动滚动：活动行变化时居中（用户刚手动滚动过则跳过）
  useEffect(() => {
    if (Date.now() < manualUntilRef.current) return;
    activeRef.current?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [activeIndex]);

  return (
    <div
      ref={containerRef}
      className="flex-1 overflow-y-auto px-8 py-[35vh] text-center"
      onWheel={() => {
        manualUntilRef.current = Date.now() + MANUAL_SCROLL_HOLD_MS;
      }}
    >
      {lyrics.lines.map((line, i) => (
        <p
          key={`${line.timeMs}-${i}`}
          ref={i === activeIndex ? activeRef : undefined}
          className={cn(
            "py-1.5 leading-relaxed transition-all duration-300",
            i === activeIndex
              ? "scale-105 text-lg font-medium text-foreground"
              : "text-sm text-muted-foreground",
          )}
        >
          {line.text || "\u00A0"}
        </p>
      ))}
    </div>
  );
}

/** 无时间轴：纯文本居中展示。 */
function PlainLyrics({ lyrics }: { lyrics: ParsedLyrics }) {
  return (
    <div className="flex-1 overflow-y-auto px-8 py-10 text-center">
      {lyrics.lines.map((line, i) => (
        <p key={i} className="py-1.5 text-sm leading-relaxed text-foreground/80">
          {line.text}
        </p>
      ))}
    </div>
  );
}
