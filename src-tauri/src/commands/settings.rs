//! 设置命令：get / update（全量替换语义，契约见 docs/ipc-contract.md）。

use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::models::settings::Settings;

use super::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    let store = state
        .store
        .lock()
        .map_err(|_| crate::error::AppError::Other("state lock poisoned".into()))?;
    store.get_settings()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> AppResult<Settings> {
    let settings = crate::services::settings::sanitize(settings);
    let audio_changed = {
        let store = state
            .store
            .lock()
            .map_err(|_| crate::error::AppError::Other("state lock poisoned".into()))?;
        let old = store.get_settings()?;
        let changed = old.audio_output != settings.audio_output;
        store.save_settings(&settings)?;
        changed
    };
    // 音频输出配置变更时实时下发给引擎适配器（无需重启）
    if audio_changed {
        let _ = super::player::emit_audio_options(&app, &settings.audio_output);
    }
    Ok(settings)
}
