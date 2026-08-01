//! 媒体库命令：scan / cancel / list / get / stats。
//!
//! 扫描在独立线程执行，线程内打开同一 SQLite 文件的第二个连接
//! （WAL 模式，已设 busy_timeout），不占用主连接的互斥锁。
//! 进度经 `library:scan-progress` 事件推送（契约见 docs/ipc-contract.md）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use base64::Engine as _;
use tauri::{AppHandle, Emitter, State};

use crate::db::store::{AlbumSummary, ArtistSummary, LibraryStats, ListQuery, StatsSummary, Store, TrackPage};
use crate::error::{AppError, AppResult};
use crate::models::track::Track;
use crate::services::library::{self, ScanEvent};
use crate::services::lyrics::LyricsSource;
use crate::services::{cover, lyrics};

use super::AppState;

/// 契约事件名。
const SCAN_PROGRESS_EVENT: &str = "library:scan-progress";

/// 启动对 `Settings.scanDirs` 的后台扫描，立即返回。
#[tauri::command]
pub fn scan_library(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    if state.scan_running.swap(true, Ordering::SeqCst) {
        return Err(AppError::InvalidArgument("scan already running".into()));
    }
    state.scan_cancel.store(false, Ordering::SeqCst);

    let (dirs, db_path) = {
        let store = state.store.lock().map_err(poisoned)?;
        let dirs = store.get_settings()?.scan_dirs;
        let path = store
            .path()
            .ok_or_else(|| AppError::Other("database path unavailable".into()))?;
        (dirs, path)
    };
    if dirs.is_empty() {
        state.scan_running.store(false, Ordering::SeqCst);
        return Err(AppError::InvalidArgument(
            "no scan directories configured".into(),
        ));
    }

    let cancel = state.scan_cancel.clone();
    let running = state.scan_running.clone();

    std::thread::spawn(move || {
        // RAII：无论扫描结果如何（包括 panic），退出时复位 running 标志。
        struct ResetOnDrop(Arc<AtomicBool>);
        impl Drop for ResetOnDrop {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _guard = ResetOnDrop(running);

        let result = Store::open(&db_path).and_then(|store| {
            let on_progress = |e: ScanEvent| {
                if let Err(err) = app.emit(SCAN_PROGRESS_EVENT, &e) {
                    tracing::warn!(error = %err, "failed to emit scan progress");
                }
            };
            library::scan(&dirs, &store, &cancel, &on_progress)
        });
        match result {
            Ok(outcome) => tracing::info!(?outcome, "library scan finished"),
            Err(e) => tracing::error!(error = %e, "library scan failed"),
        }
    });

    Ok(())
}

/// 取消进行中的扫描（在下一个文件边界生效）。
#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) -> AppResult<()> {
    state.scan_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// 分页/搜索/排序列表（契约 `list_tracks`）。
#[tauri::command]
pub fn list_tracks(
    state: State<'_, AppState>,
    offset: u32,
    limit: u32,
    sort_by: Option<String>,
    sort_dir: Option<String>,
    search: Option<String>,
) -> AppResult<TrackPage> {
    let store = state.store.lock().map_err(poisoned)?;
    store.list_tracks(&ListQuery {
        offset,
        limit,
        sort_by,
        sort_dir,
        search,
    })
}

#[tauri::command]
pub fn get_track(state: State<'_, AppState>, id: String) -> AppResult<Track> {
    let store = state.store.lock().map_err(poisoned)?;
    store.get_track(&id)
}

/// 批量取曲目（队列页渲染用）：保持入参顺序，不存在的 id 静默跳过。
#[tauri::command]
pub fn get_tracks(state: State<'_, AppState>, ids: Vec<String>) -> AppResult<Vec<Track>> {
    let store = state.store.lock().map_err(poisoned)?;
    Ok(ids
        .iter()
        .filter_map(|id| store.get_track(id).ok())
        .collect())
}

#[tauri::command]
pub fn get_library_stats(state: State<'_, AppState>) -> AppResult<LibraryStats> {
    let store = state.store.lock().map_err(poisoned)?;
    store.stats()
}

/// 统计页概览（契约 `get_stats_summary`）：总数/时长/无损、总播放/已播放数、格式分布。
#[tauri::command]
pub fn get_stats_summary(state: State<'_, AppState>) -> AppResult<StatsSummary> {
    let store = state.store.lock().map_err(poisoned)?;
    store.stats_summary()
}

/// 最近播放（契约 `list_recently_played`）：按 last_played 倒序。
#[tauri::command]
pub fn list_recently_played(state: State<'_, AppState>, limit: u32) -> AppResult<Vec<Track>> {
    let store = state.store.lock().map_err(poisoned)?;
    store.recently_played(limit)
}

/// 常听排行（契约 `list_most_played`）：按 play_count 倒序。
#[tauri::command]
pub fn list_most_played(state: State<'_, AppState>, limit: u32) -> AppResult<Vec<Track>> {
    let store = state.store.lock().map_err(poisoned)?;
    store.most_played(limit)
}

/// 全部专辑摘要（契约 `list_albums`）：按专辑艺术家/专辑名升序。
#[tauri::command]
pub fn list_albums(state: State<'_, AppState>) -> AppResult<Vec<AlbumSummary>> {
    let store = state.store.lock().map_err(poisoned)?;
    store.albums()
}

/// 全部艺术家摘要（契约 `list_artists`）：按名称升序。
#[tauri::command]
pub fn list_artists(state: State<'_, AppState>) -> AppResult<Vec<ArtistSummary>> {
    let store = state.store.lock().map_err(poisoned)?;
    store.artists()
}

/// 某专辑的全部曲目（契约 `get_album_tracks`）：按碟号/轨号排序；未知专辑传 null。
#[tauri::command]
pub fn get_album_tracks(
    state: State<'_, AppState>,
    album: Option<String>,
    album_artist: Option<String>,
) -> AppResult<Vec<Track>> {
    let store = state.store.lock().map_err(poisoned)?;
    store.album_tracks(album.as_deref(), album_artist.as_deref())
}

/// 某艺术家的全部曲目（契约 `get_artist_tracks`）：按专辑/轨号排序；未知艺术家传 null。
#[tauri::command]
pub fn get_artist_tracks(
    state: State<'_, AppState>,
    artist: Option<String>,
) -> AppResult<Vec<Track>> {
    let store = state.store.lock().map_err(poisoned)?;
    store.artist_tracks(artist.as_deref())
}

/// `get_cover` 返回 payload：base64 编码的图片数据。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverPayload {
    pub mime_type: String,
    pub data_base64: String,
}

/// 曲目封面（内嵌优先，回退同目录 cover/folder/front）；无封面返回 null。
#[tauri::command]
pub fn get_cover(state: State<'_, AppState>, id: String) -> AppResult<Option<CoverPayload>> {
    let path = {
        let store = state.store.lock().map_err(poisoned)?;
        store.get_track(&id)?.path
    };
    // 文件 IO / 标签解析在锁外执行，避免大封面阻塞其他查询
    Ok(cover::read_cover(&path)?.map(|c| CoverPayload {
        mime_type: c.mime_type,
        data_base64: base64::engine::general_purpose::STANDARD.encode(c.data),
    }))
}

/// `get_lyrics` 返回 payload：原始歌词文本（LRC 解析在前端）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsPayload {
    pub source: LyricsSource,
    pub text: String,
}

/// 曲目歌词（同目录 .lrc 优先，回退内嵌标签）；无歌词返回 null。
#[tauri::command]
pub fn get_lyrics(state: State<'_, AppState>, id: String) -> AppResult<Option<LyricsPayload>> {
    let path = {
        let store = state.store.lock().map_err(poisoned)?;
        store.get_track(&id)?.path
    };
    Ok(lyrics::read_lyrics(&path)?.map(|l| LyricsPayload {
        source: l.source,
        text: l.text,
    }))
}

/// 曲目封面主题色（契约 `get_cover_color`）：会话内缓存，无封面/解码失败返回 null。
/// 解码在锁外执行，避免阻塞其他查询。
#[tauri::command]
pub fn get_cover_color(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Option<cover::CoverColor>> {
    // 命中会话缓存直接返回（CoverColor 为 Copy）
    if let Some(cached) = state.cover_colors.lock().map_err(poisoned)?.get(&id) {
        return Ok(*cached);
    }
    let path = {
        let store = state.store.lock().map_err(poisoned)?;
        store.get_track(&id)?.path
    };
    let color = cover::cover_color(&path)?;
    state
        .cover_colors
        .lock()
        .map_err(poisoned)?
        .insert(id, color);
    Ok(color)
}

fn poisoned<T>(_: std::sync::PoisonError<T>) -> AppError {
    AppError::Other("state lock poisoned".into())
}

// ---------- 在线歌词搜索（Phase 5） ----------

/// 在线搜索歌词（lrclib.net API）。
/// 传入曲目标题/艺术家/专辑，返回同步歌词（LRC）和/或纯文本歌词。
#[tauri::command]
pub async fn search_lyrics_online(
    title: String,
    artist: Option<String>,
    album: Option<String>,
) -> AppResult<crate::services::online::OnlineLyrics> {
    crate::services::online::search_lyrics(
        &title,
        artist.as_deref(),
        album.as_deref(),
    )
    .await
}

/// 保存在线歌词到本地 .lrc 文件（与音频文件同目录）。
#[tauri::command]
pub fn save_lyrics_file(
    state: State<'_, AppState>,
    track_id: String,
    lyrics_text: String,
) -> AppResult<()> {
    let track = state.store.lock().map_err(poisoned)?.get_track(&track_id)?;
    let lrc_path = track.path.with_extension("lrc");
    std::fs::write(&lrc_path, &lyrics_text)
        .map_err(|e| AppError::Other(format!("failed to write .lrc: {e}")))?;
    tracing::info!(path = %lrc_path.display(), "lyrics saved");
    Ok(())
}

/// 在线搜索封面（MusicBrainz + Cover Art Archive）。
/// 传入艺术家 + 专辑名，返回 Base64 编码的封面图片。
#[tauri::command]
pub async fn search_cover_online(
    artist: String,
    album: String,
) -> AppResult<crate::services::online::OnlineCover> {
    crate::services::online::search_cover(&artist, &album).await
}
