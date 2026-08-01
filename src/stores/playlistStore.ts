import { create } from "zustand";
import { toast } from "sonner";
import * as cmd from "@/lib/commands";
import { normalizeError } from "@/lib/ipc";
import type { PlaylistSummary } from "@/lib/types";

/**
 * 播放列表状态：摘要列表 + 新建对话框状态。
 * 详情页的曲目增删改（remove/move）由页面本地管理并直接调命令，操作后 load() 刷新摘要计数。
 * “添加到播放列表”菜单与新建对话框共享这里的 playlists 与 pendingTrackIds。
 */
interface PlaylistStore {
  playlists: PlaylistSummary[];
  loaded: boolean;
  /** 新建对话框；pendingTrackIds 为“新建并加入”携带的曲目 */
  createDialogOpen: boolean;
  pendingTrackIds: string[];

  load: () => Promise<void>;
  ensureLoaded: () => void;
  openCreateDialog: (pendingTrackIds?: string[]) => void;
  closeCreateDialog: () => void;
  confirmCreate: (name: string) => Promise<void>;
  rename: (id: string, name: string, description?: string | null) => Promise<void>;
  remove: (id: string) => Promise<void>;
  addTracks: (id: string, trackIds: string[]) => Promise<void>;
}

function report(action: string, err: unknown): void {
  const e = normalizeError(err);
  console.warn(`[playlist] ${action} 失败:`, e);
  toast.error(`${action}失败`, { description: e.message });
}

export const usePlaylistStore = create<PlaylistStore>()((set, get) => ({
  playlists: [],
  loaded: false,
  createDialogOpen: false,
  pendingTrackIds: [],

  load: async () => {
    try {
      const playlists = await cmd.listPlaylists();
      set({ playlists, loaded: true });
    } catch (err) {
      report("加载播放列表", err);
    }
  },

  ensureLoaded: () => {
    if (!get().loaded) void get().load();
  },

  openCreateDialog: (pendingTrackIds = []) =>
    set({ createDialogOpen: true, pendingTrackIds }),
  closeCreateDialog: () => set({ createDialogOpen: false, pendingTrackIds: [] }),

  confirmCreate: async (name) => {
    const pending = get().pendingTrackIds;
    try {
      const pl = await cmd.createPlaylist(name);
      if (pending.length > 0) await cmd.addToPlaylist(pl.id, pending);
      await get().load();
      set({ createDialogOpen: false, pendingTrackIds: [] });
      toast.success(
        pending.length > 0
          ? `已创建「${pl.name}」并加入 ${pending.length} 首`
          : `已创建「${pl.name}」`,
      );
    } catch (err) {
      report("创建播放列表", err);
    }
  },

  rename: async (id, name, description) => {
    try {
      await cmd.renamePlaylist(id, name, description ?? null);
      await get().load();
      toast.success("已重命名");
    } catch (err) {
      report("重命名", err);
    }
  },

  remove: async (id) => {
    try {
      await cmd.deletePlaylist(id);
      set({ playlists: get().playlists.filter((p) => p.id !== id) });
      toast.success("已删除播放列表");
    } catch (err) {
      report("删除播放列表", err);
    }
  },

  addTracks: async (id, trackIds) => {
    if (trackIds.length === 0) return;
    try {
      await cmd.addToPlaylist(id, trackIds);
      await get().load();
      const name = get().playlists.find((p) => p.id === id)?.name ?? "播放列表";
      toast.success(`已加入「${name}」`);
    } catch (err) {
      report("加入播放列表", err);
    }
  },
}));
