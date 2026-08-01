import { useEffect, useRef, useState } from "react";
import { Music2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { getCover } from "@/lib/commands";

/**
 * 封面缩略图：按 trackId 懒加载 get_cover（内嵌/同目录封面），无封面显示占位图标。
 *
 * 默认开启 IntersectionObserver 懒加载（rootMargin 200px 预取），
 * 避免专辑网格一次性拉取上百张 base64 封面。
 */
export function AlbumArt({
  trackId,
  className,
  lazy = true,
}: {
  /** 代表曲目 id；null 时只显示占位 */
  trackId: string | null;
  className?: string;
  lazy?: boolean;
}) {
  const [url, setUrl] = useState<string | null>(null);
  const [inView, setInView] = useState(!lazy);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!lazy) return;
    const el = ref.current;
    if (!el || inView) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) {
          setInView(true);
          io.disconnect();
        }
      },
      { rootMargin: "200px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [lazy, inView]);

  useEffect(() => {
    setUrl(null);
    if (!inView || !trackId) return;
    let cancelled = false;
    getCover(trackId)
      .then((c) => {
        if (!cancelled && c) setUrl(`data:${c.mimeType};base64,${c.dataBase64}`);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [inView, trackId]);

  return (
    <div
      ref={ref}
      className={cn(
        "relative flex items-center justify-center overflow-hidden bg-muted",
        className,
      )}
    >
      {url ? (
        <img src={url} alt="" className="size-full object-cover" draggable={false} />
      ) : (
        <Music2 className="size-1/3 text-muted-foreground/40" />
      )}
    </div>
  );
}