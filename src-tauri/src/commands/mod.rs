//! Tauri Commands（IPC 接口层）。
//!
//! command 只做参数校验与转发，业务逻辑放在 services 层。

pub mod browse;
pub mod library;
pub mod media_controls;
pub mod player;
pub mod playlists;
pub mod settings;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use tauri::ipc::Channel;

use crate::db::store::Store;
use crate::error::AppResult;
use crate::services::player::PlayerCore;

use player::{CueWindow, WatchPayload};

/// 全局应用状态（`tauri::State<AppState>`）。
pub struct AppState {
    /// SQLite 访问层。rusqlite 连接非线程安全，统一走互斥锁；
    /// 查询都是毫秒级，锁竞争可忽略。扫描在独立线程持有同一把锁批量写入。
    pub store: Mutex<Store>,
    /// 扫描取消标志（下一次文件边界生效）。
    pub scan_cancel: Arc<AtomicBool>,
    /// 是否有扫描正在进行（防止并发扫描）。
    pub scan_running: Arc<AtomicBool>,
    /// 播放状态机（队列/模式/进度）。
    pub player: Mutex<PlayerCore>,
    /// 当前曲目的 CUE 时间窗口（整轨内绝对区间）；非 CUE 曲目为 None。
    /// Load 时由 dispatch 更新，引擎事件/seek 的时间平移依赖它。
    pub cue_window: Mutex<Option<CueWindow>>,
    /// `watch_player` 注册的推送 Channel（重复注册替换旧值）。
    pub watch: Mutex<Option<Channel<WatchPayload>>>,
    /// progress 节流闸门：上次推送时刻。
    pub progress_gate: Mutex<Option<Instant>>,
    /// 前端引擎适配器是否已就绪（engine_ready 已调用）。
    pub engine_ready: AtomicBool,
    /// 启动时/适配器就绪前挂起的待播放文件（命令行参数）。
    pub pending_open: Mutex<Vec<String>>,
    /// 封面主题色的会话内缓存（track id → 提取结果；None 表示无封面/解码失败）。
    /// 提取廉价且结果可再生，用内存缓存避免会话内重复解码，无需落库。
    pub cover_colors: Mutex<HashMap<String, Option<crate::services::cover::CoverColor>>>,
    /// 系统媒体控制句柄（SMTC/MPRIS/Now Playing）；init 失败或不支持时为 None。
    pub media: Mutex<Option<souvlaki::MediaControls>>,
    /// 已同步给媒体控制的曲目 id，避免重复写 metadata/封面。
    pub media_last_track: Mutex<Option<String>>,
    /// 当前写给媒体控制的封面临时文件（切歌时清理旧文件）。
    pub media_cover_path: Mutex<Option<PathBuf>>,
    /// 媒体控制封面临时目录（app cache dir），setup 时确定。
    pub media_cover_dir: PathBuf,
}

impl AppState {
    pub fn new(store: Store, media_cover_dir: PathBuf) -> Self {
        AppState {
            store: Mutex::new(store),
            scan_cancel: Arc::new(AtomicBool::new(false)),
            scan_running: Arc::new(AtomicBool::new(false)),
            player: Mutex::new(PlayerCore::new()),
            cue_window: Mutex::new(None),
            watch: Mutex::new(None),
            progress_gate: Mutex::new(None),
            engine_ready: AtomicBool::new(false),
            pending_open: Mutex::new(Vec::new()),
            cover_colors: Mutex::new(HashMap::new()),
            media: Mutex::new(None),
            media_last_track: Mutex::new(None),
            media_cover_path: Mutex::new(None),
            media_cover_dir,
        }
    }
}

/// `ping` 命令的响应，用于验证 IPC 双向打通。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Pong {
    pub message: String,
    pub version: String,
}

/// IPC 连通性自检：前端 `invoke("ping")` 应拿到 `{ message: "pong", version }`。
#[tauri::command]
pub fn ping() -> AppResult<Pong> {
    tracing::debug!("ping received");
    Ok(Pong {
        message: "pong".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_returns_pong_with_version() {
        let pong = ping().unwrap();
        assert_eq!(pong.message, "pong");
        assert_eq!(pong.version, env!("CARGO_PKG_VERSION"));
        let v = serde_json::to_value(&pong).unwrap();
        assert!(v.get("message").is_some() && v.get("version").is_some());
    }
}
