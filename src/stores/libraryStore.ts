import { create } from "zustand";
import { toast } from "sonner";
import * as cmd from "@/lib/commands";
import { normalizeError } from "@/lib/ipc";
import type {
  LibraryStats,
  ScanProgress,
  SortBy,
  SortDir,
  Track,
} from "@/lib/types";

const PAGE_SIZE = 200;

interface LibraryStore {
  tracks: Track[];
  total: number;
  /** 首屏/条件变更加载中 */
  loading: boolean;
  /** 分页追加加载中 */
  loadingMore: boolean;
  /** 后端不可达时为 true（静默降级，列表保持空） */
  unavailable: boolean;

  search: string;
  sortBy: SortBy;
  sortDir: SortDir;

  scanning: boolean;
  scanScannedFiles: number;
  scanTotalFiles: number | null;
  scanCurrentPath: string;

  stats: LibraryStats | null;

  /** 请求代数，防止旧查询结果覆盖新查询 */
  _generation: number;

  /** 重置并按当前 search/sort 拉第一页 + 统计。 */
  refresh: () => Promise<void>;
  loadMore: () => Promise<void>;
  setSearch: (search: string) => void;
  setSort: (sortBy: SortBy) => void;
  startScan: () => Promise<void>;
  cancelScan: () => Promise<void>;
  handleScanProgress: (p: ScanProgress) => void;
  loadStats: () => Promise<void>;
}

export const useLibraryStore = create<LibraryStore>()((set, get) => ({
  tracks: [],
  total: 0,
  loading: false,
  loadingMore: false,
  unavailable: false,

  search: "",
  sortBy: "dateAdded",
  sortDir: "desc",

  scanning: false,
  scanScannedFiles: 0,
  scanTotalFiles: null,
  scanCurrentPath: "",

  stats: null,

  _generation: 0,

  refresh: async () => {
    const gen = get()._generation + 1;
    set({ _generation: gen, loading: true, tracks: [], total: 0 });
    const { search, sortBy, sortDir } = get();
    try {
      const result = await cmd.listTracks({
        offset: 0,
        limit: PAGE_SIZE,
        sortBy,
        sortDir,
        search: search || undefined,
      });
      if (get()._generation !== gen) return; // 已有更新的查询
      set({ tracks: result.tracks, total: result.total, loading: false, unavailable: false });
      void get().loadStats();
    } catch (err) {
      if (get()._generation !== gen) return;
      console.warn("[library] list_tracks 失败（后端可能未就绪）:", normalizeError(err));
      set({ loading: false, unavailable: true });
    }
  },

  loadMore: async () => {
    const { tracks, total, loading, loadingMore, search, sortBy, sortDir, _generation } = get();
    if (loading || loadingMore || tracks.length >= total) return;
    set({ loadingMore: true });
    try {
      const result = await cmd.listTracks({
        offset: tracks.length,
        limit: PAGE_SIZE,
        sortBy,
        sortDir,
        search: search || undefined,
      });
      if (get()._generation !== _generation) return;
      // 以 id 去重，防御并发/扫描导致的重复行
      const seen = new Set(get().tracks.map((t) => t.id));
      const appended = result.tracks.filter((t) => !seen.has(t.id));
      set({
        tracks: [...get().tracks, ...appended],
        total: result.total,
        loadingMore: false,
      });
    } catch (err) {
      if (get()._generation !== _generation) return;
      console.warn("[library] list_tracks 分页失败:", normalizeError(err));
      set({ loadingMore: false });
    }
  },

  setSearch: (search) => {
    if (search === get().search) return;
    set({ search });
    void get().refresh();
  },

  setSort: (sortBy) => {
    const { sortBy: cur, sortDir } = get();
    if (sortBy === cur) {
      const nextDir: SortDir = sortDir === "asc" ? "desc" : "asc";
      set({ sortDir: nextDir });
    } else {
      set({ sortBy, sortDir: "asc" });
    }
    void get().refresh();
  },

  startScan: async () => {
    set({ scanning: true, scanScannedFiles: 0, scanTotalFiles: null, scanCurrentPath: "" });
    try {
      await cmd.scanLibrary();
    } catch (err) {
      const e = normalizeError(err);
      console.warn("[library] scan_library 失败:", e);
      set({ scanning: false });
      toast.error("启动扫描失败", { description: e.message });
    }
  },

  cancelScan: async () => {
    try {
      await cmd.cancelScan();
    } catch (err) {
      console.warn("[library] cancel_scan 失败:", normalizeError(err));
    }
  },

  handleScanProgress: (p) => {
    set({
      scanScannedFiles: p.scannedFiles,
      scanTotalFiles: p.totalFiles,
      scanCurrentPath: p.currentPath,
    });
    if (p.done) {
      set({ scanning: false });
      if (p.error) {
        toast.error("扫描失败", { description: p.error });
      } else {
        toast.success("扫描完成", {
          description: `共扫描 ${p.scannedFiles} 个文件`,
        });
      }
      // 扫描结束后刷新列表与统计
      void get().refresh();
    }
  },

  loadStats: async () => {
    try {
      const stats = await cmd.getLibraryStats();
      set({ stats });
    } catch (err) {
      console.warn("[library] get_library_stats 失败:", normalizeError(err));
    }
  },
}));
