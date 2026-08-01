import { useEffect, useState } from "react";
import { FolderSearch, FolderPlus, Music4, RefreshCw, Search, X } from "lucide-react";
import { useLibraryStore } from "@/stores/libraryStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { TrackList } from "@/components/TrackList/TrackList";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { NavPage } from "@/lib/nav";

/**
 * 曲库页（设计规范 4.5）：
 * 页头（标题 + 曲目总数 + 300ms 防抖搜索框 + 扫描按钮，扫描中显示进度条）
 * + TrackList（虚拟滚动）+ 空库态 / 无结果态。
 */
export function LibraryPage({
  onNavigate,
}: {
  onNavigate: (page: NavPage) => void;
}) {
  const total = useLibraryStore((s) => s.total);
  const trackCount = useLibraryStore((s) => s.tracks.length);
  const loading = useLibraryStore((s) => s.loading);
  const search = useLibraryStore((s) => s.search);
  const setSearch = useLibraryStore((s) => s.setSearch);
  const scanning = useLibraryStore((s) => s.scanning);
  const scannedFiles = useLibraryStore((s) => s.scanScannedFiles);
  const scanTotalFiles = useLibraryStore((s) => s.scanTotalFiles);
  const startScan = useLibraryStore((s) => s.startScan);
  const cancelScan = useLibraryStore((s) => s.cancelScan);
  const stats = useLibraryStore((s) => s.stats);

  const scanDirs = useSettingsStore((s) => s.scanDirs);

  const [inputValue, setInputValue] = useState(search);

  // 搜索防抖 300ms（设计规范 4.5，走后端 trigram 搜索）
  useEffect(() => {
    const timer = setTimeout(() => setSearch(inputValue.trim()), 300);
    return () => clearTimeout(timer);
  }, [inputValue, setSearch]);

  const displayCount = stats?.trackCount ?? total;
  const isEmpty = !loading && trackCount === 0;
  const showEmptyLibrary = isEmpty && !search;
  const showNoResult = isEmpty && !!search;
  const scanPercent =
    scanTotalFiles && scanTotalFiles > 0
      ? Math.min(100, Math.round((scannedFiles / scanTotalFiles) * 100))
      : null;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* 页头 */}
      <header className="shrink-0 space-y-3 px-6 pt-5 pb-3">
        <div className="flex items-center justify-between gap-4">
          <div className="flex min-w-0 items-baseline gap-3">
            <h1 className="text-xl font-semibold tracking-tight">媒体库</h1>
            <span className="tnum shrink-0 text-xs text-muted-foreground">
              {displayCount > 0 ? `${displayCount} 首曲目` : "暂无曲目"}
            </span>
          </div>

          <div className="flex shrink-0 items-center gap-2">
            <div className="relative">
              <Search className="absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={inputValue}
                onChange={(e) => setInputValue(e.target.value)}
                placeholder="搜索标题 / 艺术家 / 专辑"
                className="h-8 w-56 pl-8 text-[13px]"
              />
              {inputValue && (
                <button
                  type="button"
                  aria-label="清空搜索"
                  onClick={() => setInputValue("")}
                  className="absolute top-1/2 right-2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                >
                  <X className="size-3.5" />
                </button>
              )}
            </div>
            {scanning ? (
              <Button variant="outline" size="sm" onClick={() => void cancelScan()}>
                取消扫描
              </Button>
            ) : (
              <Button
                variant="outline"
                size="sm"
                onClick={() => void startScan()}
                disabled={scanDirs.length === 0}
                title={scanDirs.length === 0 ? "请先在设置中添加扫描目录" : undefined}
              >
                <RefreshCw className="size-3.5" />
                扫描目录
              </Button>
            )}
          </div>
        </div>

        {/* 扫描中态：页头进度条 + 已扫描数（设计规范 4.5） */}
        {scanning && (
          <div className="space-y-1.5">
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span className="flex items-center gap-1.5">
                <FolderSearch className="size-3.5 animate-pulse text-accent" />
                正在扫描媒体库…
              </span>
              <span className="tnum">
                已扫描 {scannedFiles}
                {scanTotalFiles != null ? ` / ${scanTotalFiles}` : ""} 个文件
                {scanPercent != null ? `（${scanPercent}%）` : ""}
              </span>
            </div>
            <div className="h-1 w-full overflow-hidden rounded-full bg-muted">
              <div
                className={
                  scanPercent != null
                    ? "h-full bg-accent transition-[width] duration-300"
                    : "h-full w-1/3 animate-pulse bg-accent"
                }
                style={scanPercent != null ? { width: `${scanPercent}%` } : undefined}
              />
            </div>
          </div>
        )}
      </header>

      {/* 内容区 */}
      <div className="min-h-0 flex-1 px-3 pb-2">
        {showEmptyLibrary ? (
          <EmptyLibrary onAddScanDir={() => onNavigate("settings")} />
        ) : showNoResult ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground">
            <Search className="size-8 opacity-40" />
            <p className="text-sm">未找到与「{search}」匹配的曲目</p>
          </div>
        ) : (
          <TrackList />
        )}
      </div>
    </div>
  );
}

/** 空库态：引导添加扫描目录（设计规范 4.5）。 */
function EmptyLibrary({ onAddScanDir }: { onAddScanDir: () => void }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4">
      <div className="flex size-20 items-center justify-center rounded-2xl bg-muted">
        <Music4 className="size-9 text-muted-foreground" />
      </div>
      <div className="space-y-1 text-center">
        <p className="text-base font-medium">媒体库还是空的</p>
        <p className="text-sm text-muted-foreground">
          添加包含音频文件的目录，Porrima 会自动扫描并入库
        </p>
      </div>
      <Button onClick={onAddScanDir}>
        <FolderPlus className="size-4" />
        添加扫描目录
      </Button>
    </div>
  );
}
