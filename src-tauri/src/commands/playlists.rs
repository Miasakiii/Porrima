//! 播放列表命令：create / list / rename / delete / get_tracks / add / remove / move。
//!
//! 与其他命令一致：只做参数转发，业务逻辑（校验/排序/事务）在 Store。

use tauri::State;

use crate::db::store::PlaylistSummary;
use crate::error::{AppError, AppResult};
use crate::models::track::Track;

use super::AppState;

fn poisoned<T>(_: std::sync::PoisonError<T>) -> AppError {
    AppError::Other("state lock poisoned".into())
}

/// 新建播放列表（契约 `create_playlist`）。空名报 invalid_argument。
#[tauri::command]
pub fn create_playlist(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
) -> AppResult<PlaylistSummary> {
    let store = state.store.lock().map_err(poisoned)?;
    store.create_playlist(&name, description.as_deref())
}

/// 全部播放列表摘要（契约 `list_playlists`）：按最近更新倒序。
#[tauri::command]
pub fn list_playlists(state: State<'_, AppState>) -> AppResult<Vec<PlaylistSummary>> {
    let store = state.store.lock().map_err(poisoned)?;
    store.list_playlists()
}

/// 重命名 / 改描述（契约 `rename_playlist`）。空名报 invalid_argument。
#[tauri::command]
pub fn rename_playlist(
    state: State<'_, AppState>,
    id: String,
    name: String,
    description: Option<String>,
) -> AppResult<PlaylistSummary> {
    let store = state.store.lock().map_err(poisoned)?;
    store.rename_playlist(&id, &name, description.as_deref())
}

/// 删除播放列表（契约 `delete_playlist`）。曲目关联由级联清理。
#[tauri::command]
pub fn delete_playlist(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let store = state.store.lock().map_err(poisoned)?;
    store.delete_playlist(&id)
}

/// 某播放列表的全部曲目（契约 `get_playlist_tracks`）：按 position 排序。
#[tauri::command]
pub fn get_playlist_tracks(state: State<'_, AppState>, id: String) -> AppResult<Vec<Track>> {
    let store = state.store.lock().map_err(poisoned)?;
    store.playlist_tracks(&id)
}

/// 追加曲目到播放列表末尾（契约 `add_to_playlist`）：允许重复，库中不存在的 id 静默过滤。
#[tauri::command]
pub fn add_to_playlist(
    state: State<'_, AppState>,
    id: String,
    track_ids: Vec<String>,
) -> AppResult<()> {
    let store = state.store.lock().map_err(poisoned)?;
    store.add_to_playlist(&id, &track_ids)
}

/// 按展示顺序下标移除一项（契约 `remove_from_playlist`）：越界无动作。
#[tauri::command]
pub fn remove_from_playlist(state: State<'_, AppState>, id: String, index: usize) -> AppResult<()> {
    let store = state.store.lock().map_err(poisoned)?;
    store.remove_from_playlist(&id, index)
}

/// 拖拽重排（契约 `move_in_playlist`）：把下标 from 移到 to；越界/同位无动作。
#[tauri::command]
pub fn move_in_playlist(
    state: State<'_, AppState>,
    id: String,
    from: usize,
    to: usize,
) -> AppResult<()> {
    let store = state.store.lock().map_err(poisoned)?;
    store.move_in_playlist(&id, from, to)
}
