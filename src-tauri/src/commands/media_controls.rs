//! 系统媒体控制集成（souvlaki）：SMTC(Win) / MPRIS(Linux) / Now Playing(Mac) + 媒体键。
//!
//! 归属 commands 层：本模块与平台/Tauri 深度耦合，且要调用同层 player 命令与读取 AppState，
//! 放在 services 会造成 services → commands 的分层反转，故置于 commands。
//!
//! 数据流：
//! - 出：`push_state`/进度节流点调用 [`sync`]，把当前曲目元数据 + 播放态写入系统浮窗；
//! - 入：`attach` 回调收到媒体键/浮窗按钮事件，转调同层 player 命令（复用状态机与 engine:cmd）。
//!
//! Windows 特有：MediaControls 需要主窗口 HWND，且在主线程（STA 且有消息泵）创建最稳妥，
//! 故 [`init`] 在 `setup()`（主线程）里调用。封面通过临时文件 + file:// URI 提供给 SMTC。

use std::path::Path;
use std::time::Duration;

use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};
use tauri::{AppHandle, Manager};

use crate::error::AppResult;
use crate::models::player_state::{PlayStatus, PlayerState};
use crate::services::cover;

use super::{player, AppState};

/// 媒体键 seek 步长（浮窗“快进/快退”按钮）。
const SEEK_STEP: Duration = Duration::from_secs(10);

/// 初始化系统媒体控制。失败（不支持/无窗口/权限）时返回 None，不影响其余功能。
pub fn init(app: &AppHandle) -> Option<MediaControls> {
    let hwnd = main_window_hwnd(app);
    #[cfg(target_os = "windows")]
    if hwnd.is_none() {
        tracing::warn!("media controls: main window hwnd unavailable, disabled");
        return None;
    }

    let config = PlatformConfig {
        dbus_name: "porrima",
        display_name: "Porrima",
        hwnd,
    };
    let mut controls = match MediaControls::new(config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = ?e, "media controls init failed");
            return None;
        }
    };

    let app_cb = app.clone();
    if let Err(e) = controls.attach(move |event| handle_event(&app_cb, event)) {
        tracing::warn!(error = ?e, "media controls attach failed");
        return None;
    }
    tracing::info!("system media controls ready");
    Some(controls)
}

#[cfg(target_os = "windows")]
fn main_window_hwnd(app: &AppHandle) -> Option<*mut std::ffi::c_void> {
    let win = app.get_webview_window("main")?;
    // tauri 的 HWND 来自其自带 windows crate 版本；取原始指针即可，与 souvlaki 无类型耦合。
    win.hwnd().ok().map(|h| h.0 as *mut std::ffi::c_void)
}

#[cfg(not(target_os = "windows"))]
fn main_window_hwnd(_app: &AppHandle) -> Option<*mut std::ffi::c_void> {
    None
}

// ---------- 出：状态 → 系统浮窗 ----------

/// 用当前播放状态刷新系统媒体控制。在 `push_state` 与进度节流点调用。
/// 未初始化时静默返回；曲目未变则只更新播放态/进度，切歌时才重建元数据 + 封面。
pub fn sync(state: &AppState, snapshot: &PlayerState) {
    let track_id = snapshot.current_track_id.clone();
    let stopped = snapshot.status == PlayStatus::Stopped || track_id.is_none();

    // 切歌判定（在媒体锁外完成，减少持锁时间）
    let track_changed = match state.media_last_track.lock() {
        Ok(mut last) if *last != track_id => {
            *last = track_id.clone();
            true
        }
        Ok(_) => false,
        Err(_) => return,
    };

    // 切歌且在播时，锁外读 store + 写封面临时文件
    let meta = if track_changed && !stopped {
        track_id.as_deref().and_then(|id| build_meta(state, id))
    } else {
        None
    };

    let Ok(mut guard) = state.media.lock() else {
        return;
    };
    let Some(controls) = guard.as_mut() else {
        return;
    };

    if let Some(m) = &meta {
        let _ = controls.set_metadata(MediaMetadata {
            title: Some(&m.title),
            artist: m.artist.as_deref(),
            album: m.album.as_deref(),
            cover_url: m.cover_uri.as_deref(),
            duration: m.duration,
        });
    }

    let playback = if stopped {
        MediaPlayback::Stopped
    } else {
        let progress = Some(MediaPosition(Duration::from_millis(snapshot.position_ms)));
        match snapshot.status {
            PlayStatus::Playing => MediaPlayback::Playing { progress },
            _ => MediaPlayback::Paused { progress },
        }
    };
    let _ = controls.set_playback(playback);
}

/// 供 SMTC 使用的曲目元数据（拥有所有权，set_metadata 借用其内部 &str）。
struct TrackMeta {
    title: String,
    artist: Option<String>,
    album: Option<String>,
    cover_uri: Option<String>,
    duration: Option<Duration>,
}

fn build_meta(state: &AppState, track_id: &str) -> Option<TrackMeta> {
    let track = state.store.lock().ok()?.get_track(track_id).ok()?;
    let duration = (track.duration_ms > 0).then(|| Duration::from_millis(track.duration_ms));
    let cover_uri = write_cover_temp(state, &track.path);
    Some(TrackMeta {
        title: track.title,
        artist: track.artist,
        album: track.album,
        cover_uri,
        duration,
    })
}

/// 把当前曲目封面写入临时文件，返回其 file:// URI（无封面/失败时 None）。
/// 用唯一文件名避免 WinRT 按 URI 缓存旧缩略图；清理上一张临时文件。
fn write_cover_temp(state: &AppState, track_path: &Path) -> Option<String> {
    let cover = cover::read_cover(track_path).ok().flatten()?;
    let ext = match cover.mime_type.as_str() {
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "jpg",
    };
    std::fs::create_dir_all(&state.media_cover_dir).ok()?;
    let file = state
        .media_cover_dir
        .join(format!("smtc-cover-{}.{ext}", uuid::Uuid::new_v4()));
    std::fs::write(&file, &cover.data).ok()?;

    if let Ok(mut prev) = state.media_cover_path.lock() {
        if let Some(old) = prev.replace(file.clone()) {
            let _ = std::fs::remove_file(old);
        }
    }
    Some(to_file_uri(&file))
}

/// 路径 → `file:///` URI，按 RFC3986 百分号编码（正确处理空格与中文文件名）。
fn to_file_uri(path: &Path) -> String {
    let mut out = String::from("file:///");
    for b in path.to_string_lossy().bytes() {
        match b {
            b'/' | b'\\' => out.push('/'),
            b':' => out.push(':'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---------- 入：系统媒体键/按钮 → 播放命令 ----------

fn handle_event(app: &AppHandle, event: MediaControlEvent) {
    let res: AppResult<()> = match event {
        MediaControlEvent::Play => resume_if_paused(app),
        MediaControlEvent::Pause => pause_if_playing(app),
        MediaControlEvent::Toggle => player::toggle_play(app.clone(), app.state()),
        MediaControlEvent::Next => player::next_track(app.clone(), app.state()),
        MediaControlEvent::Previous => player::previous_track(app.clone(), app.state()),
        MediaControlEvent::Stop => player::stop(app.clone(), app.state()),
        MediaControlEvent::Seek(dir) => seek_relative(app, dir, SEEK_STEP),
        MediaControlEvent::SeekBy(dir, dur) => seek_relative(app, dir, dur),
        MediaControlEvent::SetPosition(pos) => {
            player::seek(app.clone(), app.state(), pos.0.as_millis() as u64)
        }
        // SetVolume / OpenUri / Raise / Quit 暂不处理
        _ => Ok(()),
    };
    if let Err(e) = res {
        tracing::warn!(error = %e, "media control event handling failed");
    }
}

/// 读取当前 (状态, 位置ms, 时长ms)；锁在返回前释放，避免与后续命令重入死锁。
fn current_status(app: &AppHandle) -> Option<(PlayStatus, u64, u64)> {
    let state = app.state::<AppState>();
    let player = state.player.lock().ok()?;
    let s = player.state();
    Some((s.status, s.position_ms, s.duration_ms))
}

fn resume_if_paused(app: &AppHandle) -> AppResult<()> {
    if matches!(current_status(app), Some((PlayStatus::Paused, _, _))) {
        player::toggle_play(app.clone(), app.state())
    } else {
        Ok(())
    }
}

fn pause_if_playing(app: &AppHandle) -> AppResult<()> {
    if matches!(current_status(app), Some((PlayStatus::Playing, _, _))) {
        player::toggle_play(app.clone(), app.state())
    } else {
        Ok(())
    }
}

fn seek_relative(app: &AppHandle, dir: SeekDirection, step: Duration) -> AppResult<()> {
    let Some((_, pos, dur)) = current_status(app) else {
        return Ok(());
    };
    let step_ms = step.as_millis() as u64;
    let new_pos = match dir {
        // 时长未知（0）时不向前越过当前位置
        SeekDirection::Forward => pos.saturating_add(step_ms).min(dur.max(pos)),
        SeekDirection::Backward => pos.saturating_sub(step_ms),
    };
    player::seek(app.clone(), app.state(), new_pos)
}
