import { useCallback, useEffect, useState } from "react";
import { ChevronRight, Film, Folder, HardDrive, Home } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { ScrollArea } from "@/components/ui/scroll-area";

/** 目录条目。 */
interface DirEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
}

/** 浏览结果。 */
interface BrowseResult {
  current: string;
  parent: string | null;
  dirs: DirEntry[];
  files: DirEntry[];
}

interface FileBrowserProps {
  onPlayFile: (path: string) => void;
}

/** 视频文件浏览器：目录导航 + 视频文件列表。 */
export function FileBrowser({ onPlayFile }: FileBrowserProps) {
  const [result, setResult] = useState<BrowseResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const browse = useCallback(async (path?: string) => {
    setLoading(true);
    setError(null);
    try {
      const res = await invoke<BrowseResult>("browse_dir", {
        path: path ?? null,
      });
      setResult(res);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  // 初始加载：显示盘符/根目录
  useEffect(() => {
    void browse();
  }, [browse]);

  const formatSize = (bytes: number): string => {
    if (bytes === 0) return "";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  };

  return (
    <div className="flex h-full flex-col bg-background">
      {/* 路径面包屑 */}
      <div className="flex items-center gap-1 border-b border-border px-3 py-2">
        <button
          type="button"
          onClick={() => void browse()}
          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          title="返回盘符列表"
        >
          <HardDrive className="size-3.5" />
        </button>
        {result?.current && (
          <>
            <ChevronRight className="size-3 text-muted-foreground/50" />
            <button
              type="button"
              onClick={() => void browse(result.current)}
              className="max-w-[300px] truncate rounded px-1.5 py-0.5 text-xs font-medium text-foreground transition-colors hover:bg-muted"
              title={result.current}
            >
              {result.current}
            </button>
          </>
        )}
        {result?.parent && (
          <>
            <div className="flex-1" />
            <button
              type="button"
              onClick={() => void browse(result.parent!)}
              className="flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            >
              <Home className="size-3" />
              上级
            </button>
          </>
        )}
      </div>

      {/* 内容区 */}
      <ScrollArea className="min-h-0 flex-1">
        {loading && (
          <div className="flex items-center justify-center py-12 text-sm text-muted-foreground">
            加载中…
          </div>
        )}
        {error && (
          <div className="px-4 py-8 text-center text-sm text-destructive">{error}</div>
        )}
        {!loading && !error && result && (
          <div className="p-1">
            {/* 子目录 */}
            {result.dirs.map((dir) => (
              <button
                key={dir.path}
                type="button"
                onClick={() => void browse(dir.path)}
                className="flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-[13px] transition-colors hover:bg-muted"
              >
                <Folder className="size-4 shrink-0 text-accent/70" />
                <span className="min-w-0 flex-1 truncate">{dir.name}</span>
              </button>
            ))}

            {/* 视频文件 */}
            {result.files.length > 0 && result.dirs.length > 0 && (
              <div className="mx-3 my-1 border-t border-border" />
            )}
            {result.files.map((file) => (
              <button
                key={file.path}
                type="button"
                onClick={() => onPlayFile(file.path)}
                className="flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-[13px] transition-colors hover:bg-accent/10"
              >
                <Film className="size-4 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate">{file.name}</span>
                <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground">
                  {formatSize(file.size)}
                </span>
              </button>
            ))}

            {/* 空目录提示 */}
            {result.dirs.length === 0 && result.files.length === 0 && (
              <div className="py-12 text-center text-xs text-muted-foreground">
                此目录下没有视频文件
              </div>
            )}
          </div>
        )}
      </ScrollArea>
    </div>
  );
}
