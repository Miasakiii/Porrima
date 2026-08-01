import { BarChart3, Disc3, Library, ListMusic, ListVideo, MonitorPlay, Music2, Settings, Users } from "lucide-react";
import { cn } from "@/lib/utils";
import type { NavPage } from "@/lib/nav";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

interface NavItem {
  page: NavPage;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
}

const MAIN_ITEMS: NavItem[] = [
  { page: "library", label: "媒体库", icon: Library },
  { page: "albums", label: "专辑", icon: Disc3 },
  { page: "artists", label: "艺术家", icon: Users },
  { page: "playlists", label: "播放列表", icon: ListVideo },
  { page: "queue", label: "播放队列", icon: ListMusic },
  { page: "video", label: "视频", icon: MonitorPlay },
  { page: "stats", label: "统计", icon: BarChart3 },
];

const SETTINGS_ITEM: NavItem = { page: "settings", label: "设置", icon: Settings };

/**
 * 侧边栏（设计规范 4.4）：固定 220px，窗口 <900px 折叠为图标栏。
 * 折叠用纯 CSS（max-[900px]）实现，无需 JS 监听。
 */
export function Sidebar({
  page,
  onNavigate,
}: {
  page: NavPage;
  onNavigate: (page: NavPage) => void;
}) {
  return (
    <aside className="flex w-[220px] shrink-0 flex-col border-r border-border bg-sidebar transition-[width] duration-200 max-[900px]:w-14">
      {/* 应用标识 */}
      <div className="flex h-14 items-center gap-2.5 px-4 max-[900px]:justify-center max-[900px]:px-0">
        <Music2 className="size-5 shrink-0 text-accent" />
        <span className="text-base font-semibold tracking-tight max-[900px]:hidden">
          Porrima
        </span>
      </div>

      <nav className="flex flex-1 flex-col gap-0.5 px-2 max-[900px]:items-center max-[900px]:px-1">
        {MAIN_ITEMS.map((item) => (
          <SidebarButton
            key={item.page}
            item={item}
            active={page === item.page}
            onClick={() => onNavigate(item.page)}
          />
        ))}
      </nav>

      <div className="border-t border-sidebar-border px-2 py-2 max-[900px]:px-1">
        <SidebarButton
          item={SETTINGS_ITEM}
          active={page === "settings"}
          onClick={() => onNavigate("settings")}
        />
      </div>
    </aside>
  );
}

function SidebarButton({
  item,
  active,
  onClick,
}: {
  item: NavItem;
  active: boolean;
  onClick: () => void;
}) {
  const Icon = item.icon;
  const button = (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      className={cn(
        "relative flex h-9 w-full items-center gap-3 rounded-md px-3 text-[13px] transition-colors duration-150 max-[900px]:w-10 max-[900px]:justify-center max-[900px]:px-0",
        active
          ? "bg-accent/12 font-medium text-foreground"
          : "text-muted-foreground hover:bg-muted hover:text-foreground",
      )}
    >
      {active && (
        <span className="absolute top-1/2 left-0 h-4 w-0.5 -translate-y-1/2 rounded-full bg-accent max-[900px]:hidden" />
      )}
      <Icon className={cn("size-4 shrink-0", active && "text-accent")} />
      <span className="truncate max-[900px]:hidden">{item.label}</span>
    </button>
  );

  // 折叠态下用 tooltip 补足标签（展开态通过 CSS 隐藏内容）
  return (
    <Tooltip>
      <TooltipTrigger asChild>{button}</TooltipTrigger>
      <TooltipContent side="right" className="hidden max-[900px]:block">
        {item.label}
      </TooltipContent>
    </Tooltip>
  );
}
