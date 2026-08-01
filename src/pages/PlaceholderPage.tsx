import { Construction } from "lucide-react";

/** Phase 1 占位页：播放队列 / 统计（后续 Phase 实现）。 */
export function PlaceholderPage({ title }: { title: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground">
      <Construction className="size-10 opacity-40" />
      <p className="text-base font-medium text-foreground">{title}</p>
      <p className="text-sm">即将推出</p>
    </div>
  );
}
