import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";

/**
 * 后端 → 前端事件名常量（见 docs/ipc-contract.md「事件与 Channel」）。
 * Rust 端 `app.emit(...)` / `window.emit(...)` 时必须使用相同的事件名。
 *
 * 注：播放进度/状态走 `watch_player` Channel（见 lib/commands.ts），
 * 高频推送不经过 event。
 */
export const AppEvents = {
  /** 媒体库扫描进度（payload: ScanProgress，每 100 文件或 500ms 至少一次） */
  LibraryScanProgress: "library:scan-progress",
  /** 引擎指令（后端 → 引擎适配器，payload: EngineCmdPayload，见 lib/engine.ts） */
  EngineCmd: "engine:cmd",
  /** 封面更新预留（Phase 2，payload 待定） */
  CoverUpdate: "cover:update",
  /** 视频文件打开（文件关联/命令行，payload: string 文件路径） */
  VideoOpen: "video:open",
} as const;

export type AppEventName = (typeof AppEvents)[keyof typeof AppEvents];

/**
 * 在组件内订阅 Tauri 事件，自动处理 listen / unlisten 生命周期。
 *
 * `listen` 返回 `Promise<UnlistenFn>`，cleanup 阶段通过
 * `unlisten.then((fn) => fn())` 取消订阅（组件卸载时若 Promise
 * 尚未 resolve，也会在 resolve 后立即 unlisten，不会泄漏）。
 *
 * ```ts
 * useTauriEvent<ScanProgress>(AppEvents.LibraryScanProgress, (p) => {
 *   libraryStore.handleScanProgress(p);
 * });
 * ```
 */
export function useTauriEvent<T>(
  event: AppEventName,
  handler: (payload: T) => void,
): void {
  // handler 用 ref 持有，避免闭包捕获旧值，同时不重复订阅。
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    const unlisten: Promise<UnlistenFn> = listen<T>(event, (e) => {
      handlerRef.current(e.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [event]);
}
