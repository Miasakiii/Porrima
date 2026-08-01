import { invoke } from "@tauri-apps/api/core";

/**
 * 与 Rust 端 `error::AppError` 的序列化结构对应：`{ "kind", "message" }`。
 * command 返回 `Err(AppError)` 时，`invoke` 的 reject 值就是该结构。
 */
export interface AppError {
  kind: string;
  message: string;
}

/** 把 invoke 的 reject 值归一化为 AppError（防御非 AppError 的异常）。 */
export function normalizeError(err: unknown): AppError {
  if (
    typeof err === "object" &&
    err !== null &&
    "kind" in err &&
    "message" in err
  ) {
    return err as AppError;
  }
  return { kind: "unknown", message: String(err) };
}

/**
 * 类型化 invoke 封装：前端所有 command 调用统一走这里。
 *
 * ```ts
 * const track = await invokeCmd<Track>("get_track", { id: 42 });
 * ```
 */
export async function invokeCmd<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    throw normalizeError(err);
  }
}

/** `ping` 响应，对应 Rust 端 `commands::Pong`。 */
export interface Pong {
  message: string;
  version: string;
}

/** IPC 连通性自检：`invoke("ping")` 应返回 `{ message: "pong", version }`。 */
export function ping(): Promise<Pong> {
  return invokeCmd<Pong>("ping");
}
