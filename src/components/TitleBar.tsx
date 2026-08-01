import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X, Copy } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * 自定义标题栏（设计规范 4.4：拖拽区 + 最小化/最大化/关闭）。
 *
 * TODO(集成环节)：当前窗口保留原生装饰（tauri.conf.json decorations: true），
 * 本组件尚未挂载。集成环节将 decorations 改为 false 后，在 App.tsx 顶部挂载
 * <TitleBar />（已留好 TODO 注释），并确认 data-tauri-drag-region 与
 * tauri allowlist 的 window 权限（minimize/toggle_maximize/close）已开启。
 */
export function TitleBar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    let disposed = false;
    void win
      .isMaximized()
      .then((m) => !disposed && setMaximized(m))
      .catch(() => {});
    const unlisten = win
      .onResized(() => {
        void win
          .isMaximized()
          .then((m) => !disposed && setMaximized(m))
          .catch(() => {});
      })
      .catch(() => {});
    return () => {
      disposed = true;
      void Promise.resolve(unlisten).then((fn) => {
        if (typeof fn === "function") fn();
      });
    };
  }, []);

  const win = getCurrentWindow();

  return (
    <div
      data-tauri-drag-region
      className="flex h-8 shrink-0 items-center justify-between border-b border-border bg-background select-none"
    >
      <div
        data-tauri-drag-region
        className="flex h-full flex-1 items-center px-3 text-xs text-muted-foreground"
      >
        Porrima
      </div>
      <div className="flex h-full">
        <TitleBarButton
          label="最小化"
          onClick={() => void win.minimize().catch(() => {})}
        >
          <Minus className="size-3.5" />
        </TitleBarButton>
        <TitleBarButton
          label={maximized ? "还原" : "最大化"}
          onClick={() => void win.toggleMaximize().catch(() => {})}
        >
          {maximized ? (
            <Copy className="size-3" />
          ) : (
            <Square className="size-3" />
          )}
        </TitleBarButton>
        <TitleBarButton
          label="关闭"
          danger
          onClick={() => void win.close().catch(() => {})}
        >
          <X className="size-4" />
        </TitleBarButton>
      </div>
    </div>
  );
}

function TitleBarButton({
  children,
  label,
  danger,
  onClick,
}: {
  children: React.ReactNode;
  label: string;
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn(
        "flex h-full w-11 items-center justify-center text-muted-foreground transition-colors duration-150",
        danger
          ? "hover:bg-destructive hover:text-white"
          : "hover:bg-muted hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}
