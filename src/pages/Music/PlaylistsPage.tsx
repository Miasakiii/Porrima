import { useEffect, useState } from "react";
import {
  ArrowLeft,
  ListVideo,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  Trash2,
} from "lucide-react";
import { formatDuration } from "@/lib/format";
import {
  getPlaylistTracks,
  moveInPlaylist,
  removeFromPlaylist,
} from "@/lib/commands";
import { usePlaylistStore } from "@/stores/playlistStore";
import { usePlayerStore } from "@/stores/playerStore";
import { PlaylistTrackList } from "@/components/Playlist/PlaylistTrackList";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { PlaylistSummary, Track } from "@/lib/types";

/**
 * 播放列表页（Phase 2）：master 列表 + detail 详情。
 * 列表数据走 playlistStore（与"添加到播放列表"菜单共享）；详情曲目按 id 拉取，
 * 支持拖拽重排 / 移除 / 播放全部。重命名、删除用页级对话框。
 */
export function PlaylistsPage() {
  const playlists = usePlaylistStore((s) => s.playlists);
  const load = usePlaylistStore((s) => s.load);
  const openCreateDialog = usePlaylistStore((s) => s.openCreateDialog);
  const playFromList = usePlayerStore((s) => s.playFromList);

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [renameTarget, setRenameTarget] = useState<PlaylistSummary | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<PlaylistSummary | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  const selected = selectedId
    ? playlists.find((p) => p.id === selectedId) ?? null
    : null;

  const playPlaylist = async (id: string) => {
    try {
      const tracks = await getPlaylistTracks(id);
      if (tracks.length > 0) await playFromList(tracks.map((t) => t.id), 0);
    } catch {
      /* playFromList 内部已有 toast */
    }
  };

  const content = selected ? (
    <PlaylistDetail
      playlist={selected}
      onBack={() => setSelectedId(null)}
      onRename={() => setRenameTarget(selected)}
      onDelete={() => setDeleteTarget(selected)}
    />
  ) : (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex shrink-0 items-center justify-between px-6 pt-5 pb-3">
        <div className="flex items-baseline gap-3">
          <h1 className="text-xl font-semibold tracking-tight">播放列表</h1>
          <span className="tnum text-xs text-muted-foreground">
            {playlists.length} 个
          </span>
        </div>
        <Button variant="outline" size="sm" onClick={() => openCreateDialog()}>
          <Plus className="size-3.5" />
          新建
        </Button>
      </header>

      {playlists.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 text-muted-foreground">
          <ListVideo className="size-10 opacity-40" />
          <p className="text-sm">还没有播放列表 — 点击右上角「新建」</p>
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-3">
          {playlists.map((pl) => (
            <PlaylistRow
              key={pl.id}
              playlist={pl}
              onOpen={() => setSelectedId(pl.id)}
              onPlay={() => void playPlaylist(pl.id)}
              onRename={() => setRenameTarget(pl)}
              onDelete={() => setDeleteTarget(pl)}
            />
          ))}
        </div>
      )}
    </div>
  );

  return (
    <>
      {content}
      <RenameDialog
        target={renameTarget}
        onClose={() => setRenameTarget(null)}
      />
      <DeleteDialog
        target={deleteTarget}
        onClose={() => setDeleteTarget(null)}
        onDeleted={(id) => {
          if (selectedId === id) setSelectedId(null);
        }}
      />
    </>
  );
}

function PlaylistRow({
  playlist,
  onOpen,
  onPlay,
  onRename,
  onDelete,
}: {
  playlist: PlaylistSummary;
  onOpen: () => void;
  onPlay: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter") onOpen();
      }}
      className="group flex h-14 cursor-default items-center gap-3 rounded-md px-3 transition-colors duration-150 hover:bg-muted/60"
    >
      <span className="relative flex size-10 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
        <ListVideo className="size-5" />
        <button
          type="button"
          aria-label="播放"
          onClick={(e) => {
            e.stopPropagation();
            onPlay();
          }}
          className="absolute inset-0 flex items-center justify-center rounded-md bg-accent/90 text-accent-foreground opacity-0 transition-opacity duration-150 group-hover:opacity-100"
        >
          <Play className="size-4 fill-current" />
        </button>
      </span>

      <span className="min-w-0 flex-1">
        <span className="block truncate text-[13px] font-medium">{playlist.name}</span>
        <span className="tnum block truncate text-xs text-muted-foreground">
          {playlist.trackCount} 首
          {playlist.description ? ` · ${playlist.description}` : ""}
        </span>
      </span>

      <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label="更多操作"
            onClick={(e) => e.stopPropagation()}
            className={cnMenuBtn(menuOpen)}
          >
            <MoreHorizontal className="size-4" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
          <DropdownMenuItem onClick={onRename}>
            <Pencil className="size-4" />
            重命名
          </DropdownMenuItem>
          <DropdownMenuItem variant="destructive" onClick={onDelete}>
            <Trash2 className="size-4" />
            删除
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

function cnMenuBtn(open: boolean): string {
  return `flex size-7 items-center justify-center rounded-md text-muted-foreground transition-opacity duration-150 hover:bg-muted hover:text-foreground ${
    open ? "opacity-100" : "opacity-0 group-hover:opacity-100"
  }`;
}

function PlaylistDetail({
  playlist,
  onBack,
  onRename,
  onDelete,
}: {
  playlist: PlaylistSummary;
  onBack: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  const [tracks, setTracks] = useState<Track[]>([]);
  const playFromList = usePlayerStore((s) => s.playFromList);
  const loadSummaries = usePlaylistStore((s) => s.load);

  // 拉取详情曲目；playlist.updatedAt 变化（增删改后 load 刷新）时重新拉取校准。
  useEffect(() => {
    let cancelled = false;
    getPlaylistTracks(playlist.id)
      .then((t) => {
        if (!cancelled) setTracks(t);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [playlist.id, playlist.updatedAt]);

  const ids = tracks.map((t) => t.id);
  const totalMs = tracks.reduce((sum, t) => sum + t.durationMs, 0);

  const remove = (index: number) => {
    setTracks((prev) => prev.filter((_, i) => i !== index)); // 乐观
    removeFromPlaylist(playlist.id, index)
      .then(() => loadSummaries())
      .catch(() => loadSummaries());
  };
  const move = (from: number, to: number) => {
    setTracks((prev) => {
      const a = [...prev];
      const [x] = a.splice(from, 1);
      a.splice(to, 0, x);
      return a;
    });
    moveInPlaylist(playlist.id, from, to)
      .then(() => loadSummaries())
      .catch(() => loadSummaries());
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="shrink-0 px-6 pt-4 pb-3">
        <button
          type="button"
          onClick={onBack}
          className="mb-4 flex items-center gap-1 text-sm text-muted-foreground transition-colors hover:text-foreground"
        >
          <ArrowLeft className="size-4" />
          播放列表
        </button>
        <div className="flex items-end gap-5">
          <span className="flex size-28 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground shadow-md">
            <ListVideo className="size-12" />
          </span>
          <div className="flex min-w-0 flex-col gap-2 pb-1">
            <h1 className="truncate text-2xl font-semibold tracking-tight">
              {playlist.name}
            </h1>
            {playlist.description && (
              <p className="truncate text-sm text-muted-foreground">
                {playlist.description}
              </p>
            )}
            <p className="tnum text-xs text-muted-foreground">
              {playlist.trackCount} 首 · {formatDuration(totalMs)}
            </p>
            <div className="mt-1 flex items-center gap-2">
              <Button
                size="sm"
                onClick={() => ids.length > 0 && void playFromList(ids, 0)}
                disabled={ids.length === 0}
              >
                <Play className="size-4 fill-current" />
                播放全部
              </Button>
              <Button variant="outline" size="sm" onClick={onRename}>
                <Pencil className="size-3.5" />
                重命名
              </Button>
              <Button variant="outline" size="sm" onClick={onDelete}>
                <Trash2 className="size-3.5" />
                删除
              </Button>
            </div>
          </div>
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-2">
        {tracks.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground">
            <ListVideo className="size-8 opacity-40" />
            <p className="text-sm">列表为空 — 在曲库右键「添加到播放列表」</p>
          </div>
        ) : (
          <PlaylistTrackList
            tracks={tracks}
            onPlayIndex={(i) => void playFromList(ids, i)}
            onRemove={remove}
            onMove={move}
          />
        )}
      </div>
    </div>
  );
}

function RenameDialog({
  target,
  onClose,
}: {
  target: PlaylistSummary | null;
  onClose: () => void;
}) {
  const rename = usePlaylistStore((s) => s.rename);
  const [name, setName] = useState("");

  useEffect(() => {
    if (target) setName(target.name);
  }, [target]);

  const submit = () => {
    const n = name.trim();
    if (target && n) {
      void rename(target.id, n, target.description);
      onClose();
    }
  };

  return (
    <Dialog open={!!target} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>重命名播放列表</DialogTitle>
        </DialogHeader>
        <Input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
          placeholder="播放列表名称"
          maxLength={100}
        />
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            取消
          </Button>
          <Button onClick={submit} disabled={!name.trim()}>
            保存
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function DeleteDialog({
  target,
  onClose,
  onDeleted,
}: {
  target: PlaylistSummary | null;
  onClose: () => void;
  onDeleted: (id: string) => void;
}) {
  const remove = usePlaylistStore((s) => s.remove);
  return (
    <Dialog open={!!target} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>删除播放列表</DialogTitle>
          <DialogDescription>
            确定删除「{target?.name}」吗？此操作不可撤销（曲目本身不受影响）。
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            取消
          </Button>
          <Button
            variant="destructive"
            onClick={() => {
              if (target) {
                void remove(target.id);
                onDeleted(target.id);
              }
              onClose();
            }}
          >
            删除
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

