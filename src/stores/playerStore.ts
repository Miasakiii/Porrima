import { create } from "zustand";
import { toast } from "sonner";
import * as cmd from "@/lib/commands";
import { normalizeError } from "@/lib/ipc";
import type { PlayMode, PlayerState, WatchPlayerPayload } from "@/lib/types";

const PLAY_MODE_ORDER: PlayMode[] = [
  "sequential",
  "repeat-all",
  "repeat-one",
  "shuffle",
];

export const PLAY_MODE_LABEL: Record<PlayMode, string> = {
  sequential: "顺序播放",
  "repeat-all": "列表循环",
  "repeat-one": "单曲循环",
  shuffle: "随机播放",
};

const DEFAULT_STATE: PlayerState = {
  currentTrackId: null,
  status: "stopped",
  positionMs: 0,
  durationMs: 0,
  volume: 80,
  muted: false,
  playMode: "sequential",
  queue: [],
  queueIndex: -1,
};

interface PlayerStore extends PlayerState {
  /** 是否已完成首次全量拉取 + Channel 注册 */
  initialized: boolean;
  /** 后端是否可达（watch_player 注册成功） */
  backendReady: boolean;
  /** 歌词面板是否展开（纯 UI 状态，不落后端） */
  lyricsOpen: boolean;

  /** 启动时调用一次：拉全量状态并注册 watch_player Channel。 */
  init: () => Promise<void>;
  handleWatchPayload: (payload: WatchPlayerPayload) => void;
  toggleLyrics: () => void;

  playTrack: (id: string) => Promise<void>;
  /** 在给定列表上下文中播放：整列设为队列，从 startIndex 起播。 */
  playFromList: (ids: string[], startIndex: number) => Promise<void>;
  toggle: () => Promise<void>;
  next: () => Promise<void>;
  previous: () => Promise<void>;
  seekTo: (positionMs: number) => Promise<void>;
  changeVolume: (volume: number) => Promise<void>;
  toggleMute: () => Promise<void>;
  cyclePlayMode: () => Promise<void>;

  /** 队列编辑：后端 state 推送为准，move 做乐观排序保拖拽手感。 */
  addToQueue: (ids: string[], next: boolean) => Promise<void>;
  removeFromQueue: (index: number) => Promise<void>;
  moveInQueue: (from: number, to: number) => Promise<void>;
  clearQueue: () => Promise<void>;
}

/** 用户操作类命令失败：console + toast。 */
function reportActionError(action: string, err: unknown): void {
  const e = normalizeError(err);
  console.warn(`[player] ${action} 失败:`, e);
  toast.error(`${action}失败`, { description: e.message });
}

export const usePlayerStore = create<PlayerStore>()((set, get) => ({
  ...DEFAULT_STATE,
  initialized: false,
  backendReady: false,
  lyricsOpen: false,

  toggleLyrics: () => set({ lyricsOpen: !get().lyricsOpen }),

  init: async () => {
    if (get().initialized) return;
    set({ initialized: true });

    // 1. 全量拉取一次（后端未实现时容忍失败，保持默认停止态）
    try {
      const state = await cmd.getPlayerState();
      set({ ...state, backendReady: true });
    } catch (err) {
      console.warn("[player] get_player_state 失败（后端可能未就绪）:", normalizeError(err));
    }

    // 2. 注册进度/状态 Channel
    try {
      await cmd.watchPlayer((payload) => get().handleWatchPayload(payload));
      set({ backendReady: true });
    } catch (err) {
      console.warn("[player] watch_player 注册失败（后端可能未就绪）:", normalizeError(err));
    }
  },

  handleWatchPayload: (payload) => {
    if (payload.kind === "progress") {
      set({ positionMs: payload.positionMs, durationMs: payload.durationMs });
    } else {
      set({ ...payload.state });
    }
  },

  playTrack: async (id) => {
    try {
      await cmd.playTrack(id);
      // 乐观更新：等 state 推送校准
      set({ currentTrackId: id, status: "playing", positionMs: 0 });
    } catch (err) {
      reportActionError("播放", err);
    }
  },

  playFromList: async (ids, startIndex) => {
    if (startIndex < 0 || startIndex >= ids.length) return;
    try {
      await cmd.playQueue(ids, startIndex);
      // 乐观更新：等 state 推送校准
      set({
        currentTrackId: ids[startIndex],
        status: "playing",
        positionMs: 0,
        queue: ids,
        queueIndex: startIndex,
      });
    } catch (err) {
      reportActionError("播放", err);
    }
  },

  toggle: async () => {
    const { status } = get();
    if (status === "stopped") return;
    // 乐观切换，后端 state 推送会校准
    set({ status: status === "playing" ? "paused" : "playing" });
    try {
      await cmd.togglePlay();
    } catch (err) {
      set({ status }); // 回滚
      reportActionError("播放/暂停", err);
    }
  },

  next: async () => {
    try {
      await cmd.nextTrack();
    } catch (err) {
      reportActionError("下一首", err);
    }
  },

  previous: async () => {
    try {
      await cmd.previousTrack();
    } catch (err) {
      reportActionError("上一首", err);
    }
  },

  seekTo: async (positionMs) => {
    const prev = get().positionMs;
    set({ positionMs }); // 乐观，进度推送会校准
    try {
      await cmd.seek(Math.round(positionMs));
    } catch (err) {
      set({ positionMs: prev });
      reportActionError("跳转", err);
    }
  },

  changeVolume: async (volume) => {
    const v = Math.max(0, Math.min(100, Math.round(volume)));
    const prev = get().volume;
    set({ volume: v, muted: v === 0 ? get().muted : false });
    try {
      await cmd.setVolume(v);
    } catch (err) {
      set({ volume: prev });
      reportActionError("调节音量", err);
    }
  },

  toggleMute: async () => {
    const muted = !get().muted;
    set({ muted });
    try {
      await cmd.setMuted(muted);
    } catch (err) {
      set({ muted: !muted });
      reportActionError("静音", err);
    }
  },

  cyclePlayMode: async () => {
    const cur = get().playMode;
    const nextMode =
      PLAY_MODE_ORDER[(PLAY_MODE_ORDER.indexOf(cur) + 1) % PLAY_MODE_ORDER.length];
    set({ playMode: nextMode });
    try {
      await cmd.setPlayMode(nextMode);
      toast.success(`播放模式：${PLAY_MODE_LABEL[nextMode]}`);
    } catch (err) {
      set({ playMode: cur });
      reportActionError("切换播放模式", err);
    }
  },

  addToQueue: async (ids, next) => {
    if (ids.length === 0) return;
    try {
      await cmd.queueAdd(ids, next);
      toast.success(next ? "将在下一首播放" : "已加入播放队列");
    } catch (err) {
      reportActionError("加入队列", err);
    }
  },

  removeFromQueue: async (index) => {
    try {
      await cmd.queueRemove(index);
    } catch (err) {
      reportActionError("移出队列", err);
    }
  },

  moveInQueue: async (from, to) => {
    const { queue, queueIndex } = get();
    if (from === to || from < 0 || to < 0 || from >= queue.length || to >= queue.length)
      return;
    // 乐观排序（含 queueIndex 跟随），后端 state 推送校准
    const reordered = [...queue];
    const [item] = reordered.splice(from, 1);
    reordered.splice(to, 0, item);
    let idx = queueIndex;
    if (idx === from) idx = to;
    else if (from < idx && to >= idx) idx -= 1;
    else if (from > idx && to <= idx) idx += 1;
    set({ queue: reordered, queueIndex: idx });
    try {
      await cmd.queueMove(from, to);
    } catch (err) {
      set({ queue, queueIndex });
      reportActionError("调整队列顺序", err);
    }
  },

  clearQueue: async () => {
    try {
      await cmd.queueClear();
      toast.success("队列已清空");
    } catch (err) {
      reportActionError("清空队列", err);
    }
  },
}));
