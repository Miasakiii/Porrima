import { X, Maximize } from "lucide-react";
import { exitPiP } from "@/lib/engine";

interface MiniPlayerProps {
  active: boolean;
  onRestore: () => void;
}

/**
 * 画中画浮窗：当 PiP 激活且用户不在视频页时显示。
 * 右下角透明区域让 mpv 视频透出，周围有边框和控件。
 */
export function MiniPlayer({ active, onRestore }: MiniPlayerProps) {
  if (!active) return null;

  const handleRestore = async () => {
    await exitPiP();
    onRestore();
  };

  return (
    <div className="pointer-events-none fixed inset-0 z-[100]">
      {/* 视频浮窗区域：右下角 */}
      <div
        className="pointer-events-auto absolute right-[1%] bottom-[13%] h-[27%] w-[27%] overflow-hidden rounded-lg border-2 border-accent/60 shadow-2xl shadow-black/50"
      >
        {/* 透明背景让视频透出 */}
        <div className="h-full w-full bg-transparent" />

        {/* 悬浮控件 */}
        <div className="absolute top-1 right-1 flex gap-1 opacity-0 transition-opacity duration-200 hover:opacity-100 [&:hover]:opacity-100">
          <button
            type="button"
            onClick={() => void handleRestore()}
            className="rounded bg-black/70 p-1 text-white/90 transition-colors hover:bg-black/90"
            title="恢复全屏"
          >
            <Maximize className="size-3.5" />
          </button>
          <button
            type="button"
            onClick={() => void handleRestore()}
            className="rounded bg-black/70 p-1 text-white/90 transition-colors hover:bg-red-600"
            title="关闭画中画"
          >
            <X className="size-3.5" />
          </button>
        </div>

        {/* 底部标签 */}
        <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/60 to-transparent px-2 py-1">
          <span className="text-[10px] text-white/80">画中画</span>
        </div>
      </div>
    </div>
  );
}
