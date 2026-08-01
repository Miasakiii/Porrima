import { ListPlus, Plus } from "lucide-react";
import {
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu";
import { usePlaylistStore } from "@/stores/playlistStore";

/**
 * 曲目菜单里的「添加到播放列表 ▸」子菜单：列出已有列表 + 新建。
 * 悬停/聚焦时懒加载播放列表；点击已有列表直接加入，点击“新建”打开全局新建对话框并携带曲目。
 */
export function AddToPlaylistSub({ trackIds }: { trackIds: string[] }) {
  const playlists = usePlaylistStore((s) => s.playlists);
  const ensureLoaded = usePlaylistStore((s) => s.ensureLoaded);
  const addTracks = usePlaylistStore((s) => s.addTracks);
  const openCreateDialog = usePlaylistStore((s) => s.openCreateDialog);

  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger
        onPointerEnter={ensureLoaded}
        onFocus={ensureLoaded}
      >
        <ListPlus className="size-4" />
        添加到播放列表
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="max-h-72 min-w-40 overflow-y-auto">
        <DropdownMenuItem onClick={() => openCreateDialog(trackIds)}>
          <Plus className="size-4" />
          新建播放列表…
        </DropdownMenuItem>
        {playlists.length > 0 && <DropdownMenuSeparator />}
        {playlists.map((pl) => (
          <DropdownMenuItem
            key={pl.id}
            onClick={() => void addTracks(pl.id, trackIds)}
          >
            <span className="truncate">{pl.name}</span>
          </DropdownMenuItem>
        ))}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  );
}
