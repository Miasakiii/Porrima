//! 播放控制命令（契约 docs/ipc-contract.md「播放」）+ 引擎适配内部接口。
//!
//! 架构（Phase 0 最终决策，见 docs/phase0-findings.md）：
//! - mpv 控制面只在前端（tauri-plugin-libmpv JS API）；
//! - Rust `PlayerCore` 拥有队列/模式/状态机，产出 `EngineCmd`，
//!   经 webview 事件 `engine:cmd` 交给前端适配器执行；
//! - mpv 事件由前端适配器经 `engine_event` 命令原样转发回来；
//! - 状态/进度经 `watch_player` 注册的 Channel 推送（progress ≤4Hz）。

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{AppError, AppResult};
use crate::models::player_state::{PlayMode, PlayerState};
use crate::services::library;
use crate::services::player::{EngineCmd, PlayerCore};

use super::AppState;

/// 引擎指令事件名（后端 → 前端适配器）。
pub const ENGINE_CMD_EVENT: &str = "engine:cmd";

/// progress 推送最小间隔（≤4Hz）。
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

// ---------- payload 类型 ----------

/// 当前曲目的 CUE 时间窗口（整轨文件内的绝对区间）。
///
/// 存在时：mpv 上报的绝对时间平移为轨内相对时间后再进状态机，
/// seek 反向平移；PlayerCore 始终只看到轨内时间，保持纯净。
#[derive(Debug, Clone, Copy)]
pub struct CueWindow {
    pub start_ms: u64,
    pub end_ms: Option<u64>,
}

/// `watch_player` Channel 推送的 payload（契约「事件与 Channel」）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WatchPayload {
    #[serde(rename_all = "camelCase")]
    Progress { position_ms: u64, duration_ms: u64 },
    State { state: PlayerState },
}

/// `engine:cmd` 事件 payload（后端 → 前端适配器）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EngineCmdPayload {
    #[serde(rename_all = "camelCase")]
    Load {
        path: String,
        /// CUE 虚拟曲目：起播位置（整轨文件内绝对时间）；普通曲目为 null。
        start_ms: Option<u64>,
        /// CUE 虚拟曲目：结束位置；末轨/普通曲目为 null（播到文件尾）。
        end_ms: Option<u64>,
    },
    Pause,
    Resume,
    Stop,
    #[serde(rename_all = "camelCase")]
    Seek { position_ms: u64 },
    SetVolume { volume: u32 },
    /// 音频输出选项变更（运行时切换 WASAPI/ReplayGain/gapless）。
    #[serde(rename_all = "camelCase")]
    SetAudioOptions {
        /// mpv audio-exclusive 值。
        exclusive: bool,
        /// mpv audio-device 值（"auto" 为系统默认）。
        device: String,
        /// gapless-audio。
        gapless: bool,
        /// replaygain 模式（"no" / "track" / "album"）。
        replay_gain: String,
        /// 无 RG 标签时 loudnorm 滤镜。
        loudnorm_fallback: bool,
    },
}

/// 引擎事件应触发的推送类别。
#[derive(Debug, PartialEq, Eq)]
enum EnginePush {
    None,
    /// 位置/时长更新（time-pos 由调用方节流）。
    Progress,
    /// 全量状态推送。
    State,
}

// ---------- 播放命令（契约） ----------

/// 播放指定曲目。已在当前队列 → 跳转到该索引；不在 → 队列替换为该曲目。
/// （列表上下文由前端用 `play_queue` 表达。）
#[tauri::command]
pub fn play_track(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    // 先校验曲目存在，避免状态机已切换而引擎加载失败的不一致
    let _ = state.store.lock().map_err(poisoned)?.get_track(&id)?;
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = apply_play_track(&mut player, id);
        (cmd, player.state().clone())
    };
    finish(&app, &state, cmd, Some(snapshot))
}

/// 显式设置队列并从 startIndex 播放。
#[tauri::command]
pub fn play_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    start_index: usize,
) -> AppResult<()> {
    if ids.is_empty() || start_index >= ids.len() {
        return Err(AppError::InvalidArgument(
            "empty queue or startIndex out of range".into(),
        ));
    }
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.play_queue(ids, start_index);
        (cmd, player.state().clone())
    };
    finish(&app, &state, cmd, Some(snapshot))
}

#[tauri::command]
pub fn toggle_play(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.toggle();
        (cmd, player.state().clone())
    };
    finish(&app, &state, cmd, Some(snapshot))
}

#[tauri::command]
pub fn stop(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.stop();
        (cmd, player.state().clone())
    };
    finish(&app, &state, Some(cmd), Some(snapshot))
}

#[tauri::command]
pub fn next_track(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.next();
        (cmd, player.state().clone())
    };
    finish(&app, &state, cmd, Some(snapshot))
}

#[tauri::command]
pub fn previous_track(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.previous();
        (cmd, player.state().clone())
    };
    finish(&app, &state, cmd, Some(snapshot))
}

#[tauri::command]
pub fn seek(app: AppHandle, state: State<'_, AppState>, position_ms: u64) -> AppResult<()> {
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.seek(position_ms);
        (cmd, player.state().clone())
    };
    finish(&app, &state, Some(cmd), Some(snapshot))
}

#[tauri::command]
pub fn set_volume(app: AppHandle, state: State<'_, AppState>, volume: u32) -> AppResult<()> {
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.set_volume(volume);
        (cmd, player.state().clone())
    };
    finish(&app, &state, Some(cmd), Some(snapshot))
}

#[tauri::command]
pub fn set_muted(app: AppHandle, state: State<'_, AppState>, muted: bool) -> AppResult<()> {
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.set_muted(muted);
        (cmd, player.state().clone())
    };
    finish(&app, &state, Some(cmd), Some(snapshot))
}

#[tauri::command]
pub fn set_play_mode(app: AppHandle, state: State<'_, AppState>, mode: PlayMode) -> AppResult<()> {
    let snapshot = {
        let mut player = state.player.lock().map_err(poisoned)?;
        player.set_mode(mode);
        player.state().clone()
    };
    finish(&app, &state, None, Some(snapshot))
}

#[tauri::command]
pub fn get_player_state(state: State<'_, AppState>) -> AppResult<PlayerState> {
    Ok(state.player.lock().map_err(poisoned)?.state().clone())
}

// ---------- 队列编辑（Phase 2） ----------

/// 追加曲目到队列；`next=true` 插到当前曲目之后。不存在的 id 静默过滤。
#[tauri::command]
pub fn queue_add(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    next: bool,
) -> AppResult<()> {
    let valid: Vec<String> = {
        let store = state.store.lock().map_err(poisoned)?;
        ids.into_iter()
            .filter(|id| store.get_track(id).is_ok())
            .collect()
    };
    if valid.is_empty() {
        return Err(AppError::InvalidArgument("no valid track ids".into()));
    }
    let snapshot = {
        let mut player = state.player.lock().map_err(poisoned)?;
        player.queue_add(valid, next);
        player.state().clone()
    };
    finish(&app, &state, None, Some(snapshot))
}

/// 移除队列指定位置（移除当前曲目时自动切歌/停止）。
#[tauri::command]
pub fn queue_remove(app: AppHandle, state: State<'_, AppState>, index: usize) -> AppResult<()> {
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.queue_remove(index);
        (cmd, player.state().clone())
    };
    finish(&app, &state, cmd, Some(snapshot))
}

/// 队内移动（拖拽排序）。
#[tauri::command]
pub fn queue_move(
    app: AppHandle,
    state: State<'_, AppState>,
    from: usize,
    to: usize,
) -> AppResult<()> {
    let snapshot = {
        let mut player = state.player.lock().map_err(poisoned)?;
        player.queue_move(from, to);
        player.state().clone()
    };
    finish(&app, &state, None, Some(snapshot))
}

/// 清空队列（保留正在播放的当前曲目）。
#[tauri::command]
pub fn queue_clear(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let snapshot = {
        let mut player = state.player.lock().map_err(poisoned)?;
        player.queue_clear();
        player.state().clone()
    };
    finish(&app, &state, None, Some(snapshot))
}

/// 注册播放状态 Channel（重复注册幂等：替换旧 channel），随即推一次全量 state。
#[tauri::command]
pub fn watch_player(
    state: State<'_, AppState>,
    channel: Channel<WatchPayload>,
) -> AppResult<()> {
    let snapshot = state.player.lock().map_err(poisoned)?.state().clone();
    *state.watch.lock().map_err(poisoned)? = Some(channel);
    push_state(&state, snapshot);
    Ok(())
}

// ---------- 引擎适配内部接口 ----------

/// 前端适配器转发的 mpv 事件。时间值单位为秒（mpv 原生），此处换算为 ms。
#[tauri::command]
pub fn engine_event(
    app: AppHandle,
    state: State<'_, AppState>,
    event: String,
    value: Option<serde_json::Value>,
) -> AppResult<()> {
    let window = *state.cue_window.lock().map_err(poisoned)?;
    let (cmd, push, snapshot, stats_id) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let (cmd, push) = apply_engine_event(&mut player, &event, value.as_ref(), window.as_ref());
        let stats_id = player.take_stats_pending();
        (cmd, push, player.state().clone(), stats_id)
    };

    // 播放达阈值（≥ 30s / ≥ 50%）：记一次播放（play_count +1, last_played=now）。
    if let Some(id) = stats_id {
        if let Ok(store) = state.store.lock() {
            if let Err(e) = store.record_play(&id) {
                tracing::warn!(error = %e, track = %id, "record_play failed");
            }
        }
    }

    if let Some(cmd) = cmd {
        dispatch(&app, &state, cmd)?;
    }
    match push {
        EnginePush::Progress => {
            // time-pos ~15Hz，节流到 ≤4Hz 再推
            let throttled = event == "time-pos" && !progress_gate_open(&state);
            if !throttled {
                push_progress(&state, snapshot.position_ms, snapshot.duration_ms);
                // 同步系统媒体控制的进度（曲目未变，仅刷新播放态/位置）
                super::media_controls::sync(&state, &snapshot);
            }
        }
        EnginePush::State => push_state(&state, snapshot),
        EnginePush::None => {}
    }
    Ok(())
}

/// 前端适配器初始化完成：同步音量与音频输出配置到引擎，处理启动时挂起的打开文件请求。
#[tauri::command]
pub fn engine_ready(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    state
        .engine_ready
        .store(true, std::sync::atomic::Ordering::SeqCst);

    // mpv 默认音量与后端状态可能不一致，主动同步一次（静音 → 0）
    let (volume, muted) = {
        let player = state.player.lock().map_err(poisoned)?;
        (player.state().volume, player.state().muted)
    };
    dispatch_payload(
        &app,
        EngineCmdPayload::SetVolume {
            volume: if muted { 0 } else { volume },
        },
    )?;

    // 同步音频输出配置（gapless / WASAPI / ReplayGain）
    if let Ok(store) = state.store.lock() {
        if let Ok(settings) = store.get_settings() {
            let _ = emit_audio_options(&app, &settings.audio_output);
        }
    }

    // 恢复上次播放状态（队列/位置/音量/模式，不自动播放）
    {
        let saved = state
            .store
            .lock()
            .ok()
            .and_then(|store| store.load_player_state());
        if let Some(snapshot) = saved {
            // 校验当前曲目仍在库中，否则丢弃恢复
            let valid = snapshot.current_track_id.as_ref().map_or(true, |id| {
                state.store.lock().ok().map_or(false, |s| s.get_track(id).is_ok())
            });
            if valid {
                let mut player = state.player.lock().map_err(poisoned)?;
                player.restore(snapshot);
                tracing::info!("player state restored from previous session");
                // 推送恢复后的状态给前端
                push_state(&state, player.state().clone());
            }
        }
    }

    let pending: Vec<String> = state.pending_open.lock().map_err(poisoned)?.drain(..).collect();
    if !pending.is_empty() {
        open_files(&app, pending)?;
    }
    Ok(())
}

/// 视频文件扩展名（与 browse.rs 保持一致）。
const VIDEO_EXTS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "ts", "wmv", "rmvb", "m4v", "mpg", "mpeg", "3gp",
];

fn is_video_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// 打开外部文件（命令行参数 / 二次实例转发）：
/// - 视频文件 → emit `video:open` 事件，前端路由到视频模式直接播放
/// - 音频文件 → 入库后整批设为队列播放
pub fn open_files(app: &AppHandle, files: Vec<String>) -> AppResult<()> {
    let state = app.state::<AppState>();

    // 分离视频/音频文件
    let (videos, audios): (Vec<_>, Vec<_>) = files.into_iter().partition(|f| is_video_path(f));

    // 视频文件：通知前端进入视频模式播放
    for path in &videos {
        let _ = app.emit("video:open", path);
    }

    // 音频文件：入库 + 队列播放
    if !audios.is_empty() {
        let tracks = {
            let store = state.store.lock().map_err(poisoned)?;
            library::import_files(&store, &audios)?
        };
        let ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
        if !ids.is_empty() {
            let (cmd, snapshot) = {
                let mut player = state.player.lock().map_err(poisoned)?;
                let cmd = player.play_queue(ids, 0);
                (cmd, player.state().clone())
            };
            if let Some(cmd) = cmd {
                dispatch(app, &state, cmd)?;
            }
            push_state(&state, snapshot);
        }
    }

    if videos.is_empty() && audios.is_empty() {
        tracing::info!("open_files: no playable files");
    }
    Ok(())
}

/// 二次实例启动（tauri-plugin-single-instance 回调）：聚焦主窗口并处理文件参数。
pub fn handle_second_instance(app: &AppHandle, argv: Vec<String>) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
    let files: Vec<String> = argv
        .into_iter()
        .skip(1) // argv[0] 是可执行文件路径
        .filter(|a| !a.starts_with('-'))
        .collect();
    if files.is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    if state
        .engine_ready
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        if let Err(e) = open_files(app, files) {
            tracing::error!(error = %e, "failed to open files from second instance");
        }
    } else if let Ok(mut pending) = state.pending_open.lock() {
        pending.extend(files);
    }
}

/// 拖放文件播放：前端监听 `tauri://drag-drop` 事件后将文件路径列表传入。
/// 音频入库播放，视频进入视频模式。
#[tauri::command]
pub fn open_dropped_files(app: AppHandle, paths: Vec<String>) -> AppResult<()> {
    if paths.is_empty() {
        return Ok(());
    }
    open_files(&app, paths)
}

// ---------- 内部逻辑（纯函数，可单测） ----------

/// `play_track` 语义：命中当前队列则原地跳转，否则替换队列。
fn apply_play_track(player: &mut PlayerCore, id: String) -> Option<EngineCmd> {
    let idx = player.state().queue.iter().position(|t| *t == id);
    match idx {
        Some(i) => player.play_at(i),
        None => player.play_queue(vec![id], 0),
    }
}

/// 引擎事件 → 状态机更新，返回（需执行的引擎动作，需推送的类别）。
/// `window` 非空时把 mpv 的整轨绝对时间平移为轨内相对时间。
fn apply_engine_event(
    player: &mut PlayerCore,
    event: &str,
    value: Option<&serde_json::Value>,
    window: Option<&CueWindow>,
) -> (Option<EngineCmd>, EnginePush) {
    match event {
        "time-pos" => {
            if let Some(secs) = value.and_then(|v| v.as_f64()) {
                let abs_ms = secs_to_ms(secs);
                let rel_ms = match window {
                    Some(w) => abs_ms.saturating_sub(w.start_ms),
                    None => abs_ms,
                };
                let duration = player.state().duration_ms;
                player.on_progress(rel_ms, duration);
                return (None, EnginePush::Progress);
            }
            (None, EnginePush::None)
        }
        "duration" => {
            if let Some(secs) = value.and_then(|v| v.as_f64()) {
                let position = player.state().position_ms;
                player.on_progress(position, track_duration(secs_to_ms(secs), window));
                return (None, EnginePush::Progress);
            }
            (None, EnginePush::None)
        }
        "pause" => {
            if let Some(paused) = value.and_then(|v| v.as_bool()) {
                player.on_pause_changed(paused);
                return (None, EnginePush::State);
            }
            (None, EnginePush::None)
        }
        "end-file" => (player.on_end_file(), EnginePush::State),
        "file-loaded" => {
            let full_ms = value.and_then(|v| v.as_f64()).map(secs_to_ms).unwrap_or(0);
            player.on_file_loaded(track_duration(full_ms, window));
            (None, EnginePush::State)
        }
        other => {
            tracing::debug!(event = other, "ignoring unknown engine event");
            (None, EnginePush::None)
        }
    }
}

/// 整轨文件总时长 → 当前轨时长（CUE 窗口裁剪；非 CUE 原样返回）。
fn track_duration(full_ms: u64, window: Option<&CueWindow>) -> u64 {
    match window {
        Some(w) => w.end_ms.unwrap_or(full_ms).saturating_sub(w.start_ms),
        None => full_ms,
    }
}

fn secs_to_ms(secs: f64) -> u64 {
    (secs.max(0.0) * 1000.0).round() as u64
}

// ---------- 推送与分发 ----------

/// 命令收尾：先分发引擎动作，再推全量状态。
fn finish(
    app: &AppHandle,
    state: &AppState,
    cmd: Option<EngineCmd>,
    snapshot: Option<PlayerState>,
) -> AppResult<()> {
    if let Some(cmd) = cmd {
        dispatch(app, state, cmd)?;
    }
    if let Some(snapshot) = snapshot {
        push_state(state, snapshot);
    }
    Ok(())
}

/// 把状态机产出的 `EngineCmd` 翻译成 `engine:cmd` 事件发给前端适配器。
/// Load 时同步更新 CUE 窗口；Seek 把轨内相对时间平移回整轨绝对时间。
fn dispatch(app: &AppHandle, state: &AppState, cmd: EngineCmd) -> AppResult<()> {
    let payload = match cmd {
        EngineCmd::Load { track_id } => {
            let track = state.store.lock().map_err(poisoned)?.get_track(&track_id)?;
            let window = track.cue_source.as_ref().map(|c| CueWindow {
                start_ms: c.start_ms,
                end_ms: c.end_ms,
            });
            *state.cue_window.lock().map_err(poisoned)? = window;
            EngineCmdPayload::Load {
                path: track.path.to_string_lossy().into_owned(),
                start_ms: window.map(|w| w.start_ms),
                end_ms: window.and_then(|w| w.end_ms),
            }
        }
        EngineCmd::Pause => EngineCmdPayload::Pause,
        EngineCmd::Resume => EngineCmdPayload::Resume,
        EngineCmd::Stop => {
            *state.cue_window.lock().map_err(poisoned)? = None;
            EngineCmdPayload::Stop
        }
        EngineCmd::Seek { position_ms } => {
            let offset = state
                .cue_window
                .lock()
                .map_err(poisoned)?
                .map(|w| w.start_ms)
                .unwrap_or(0);
            EngineCmdPayload::Seek {
                position_ms: offset + position_ms,
            }
        }
        EngineCmd::SetVolume { volume } => EngineCmdPayload::SetVolume { volume },
    };
    dispatch_payload(app, payload)
}

fn dispatch_payload(app: &AppHandle, payload: EngineCmdPayload) -> AppResult<()> {
    app.emit(ENGINE_CMD_EVENT, &payload)
        .map_err(|e| AppError::Player(format!("failed to emit engine command: {e}")))
}

fn push_state(state: &AppState, snapshot: PlayerState) {
    // 切歌/暂停/停止等全量状态变化时，同步系统媒体控制（元数据 + 播放态）
    super::media_controls::sync(state, &snapshot);
    push_payload(state, WatchPayload::State { state: snapshot });
}

fn push_progress(state: &AppState, position_ms: u64, duration_ms: u64) {
    push_payload(
        state,
        WatchPayload::Progress {
            position_ms,
            duration_ms,
        },
    );
}

fn push_payload(state: &AppState, payload: WatchPayload) {
    let Ok(guard) = state.watch.lock() else {
        return;
    };
    if let Some(channel) = guard.as_ref() {
        if let Err(e) = channel.send(payload) {
            tracing::warn!(error = %e, "failed to push watch payload");
        }
    }
}

/// progress 节流闸门：距上次推送 ≥ PROGRESS_INTERVAL 才放行（并刷新时间戳）。
fn progress_gate_open(state: &AppState) -> bool {
    let Ok(mut last) = state.progress_gate.lock() else {
        return true;
    };
    let now = Instant::now();
    match *last {
        Some(prev) if now.duration_since(prev) < PROGRESS_INTERVAL => false,
        _ => {
            *last = Some(now);
            true
        }
    }
}

fn poisoned<T>(_: std::sync::PoisonError<T>) -> AppError {
    AppError::Other("state lock poisoned".into())
}

/// 根据音频输出配置构建 payload 并下发给前端适配器。
/// 由 `engine_ready`（启动同步）和 `update_settings`（运行时切换）调用。
pub fn emit_audio_options(app: &AppHandle, cfg: &crate::models::settings::AudioOutputConfig) -> AppResult<()> {
    use crate::models::settings::{AudioBackend, ReplayGainMode};
    let exclusive = cfg.backend == AudioBackend::WasapiExclusive;
    let device = match &cfg.device {
        Some(d) if !d.is_empty() => d.clone(),
        _ => "auto".to_string(),
    };
    let replay_gain = match cfg.replay_gain {
        ReplayGainMode::Off => "no".to_string(),
        ReplayGainMode::Track => "track".to_string(),
        ReplayGainMode::Album => "album".to_string(),
    };
    dispatch_payload(
        app,
        EngineCmdPayload::SetAudioOptions {
            exclusive,
            device,
            gapless: cfg.gapless,
            replay_gain,
            loudnorm_fallback: cfg.loudnorm_fallback,
        },
    )
}

// ---------- 托盘菜单控制（无 State 抽取，由 lib.rs 传入） ----------

/// 托盘“播放/暂停”。
pub fn tray_toggle(app: &AppHandle, state: &AppState) -> AppResult<()> {
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.toggle();
        (cmd, player.state().clone())
    };
    finish(app, state, cmd, Some(snapshot))
}

/// 托盘“上一首”。
pub fn tray_previous(app: &AppHandle, state: &AppState) -> AppResult<()> {
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.previous();
        (cmd, player.state().clone())
    };
    finish(app, state, cmd, Some(snapshot))
}

/// 托盘“下一首”。
pub fn tray_next(app: &AppHandle, state: &AppState) -> AppResult<()> {
    let (cmd, snapshot) = {
        let mut player = state.player.lock().map_err(poisoned)?;
        let cmd = player.next();
        (cmd, player.state().clone())
    };
    finish(app, state, cmd, Some(snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::player_state::PlayStatus;

    fn core_with_queue(ids: &[&str], start: usize) -> PlayerCore {
        let mut core = PlayerCore::new();
        core.play_queue(ids.iter().map(|s| s.to_string()).collect(), start);
        core
    }

    #[test]
    fn play_track_jumps_in_queue_or_replaces() {
        // 命中队列：跳转，队列不变
        let mut core = core_with_queue(&["a", "b", "c"], 0);
        let cmd = apply_play_track(&mut core, "c".into());
        assert_eq!(cmd, Some(EngineCmd::Load { track_id: "c".into() }));
        assert_eq!(core.state().queue, vec!["a", "b", "c"]);
        assert_eq!(core.state().queue_index, 2);

        // 不在队列：整体替换
        let cmd = apply_play_track(&mut core, "x".into());
        assert_eq!(cmd, Some(EngineCmd::Load { track_id: "x".into() }));
        assert_eq!(core.state().queue, vec!["x"]);
        assert_eq!(core.state().queue_index, 0);
    }

    #[test]
    fn engine_events_advance_state_machine() {
        let mut core = core_with_queue(&["a", "b"], 0);

        // file-loaded（秒 → ms）
        let v = serde_json::json!(180.5);
        let (cmd, push) = apply_engine_event(&mut core, "file-loaded", Some(&v), None);
        assert!(cmd.is_none());
        assert_eq!(push, EnginePush::State);
        assert_eq!(core.state().duration_ms, 180_500);

        // time-pos 更新位置，duration 不被 0 覆盖
        let v = serde_json::json!(12.34);
        let (_, push) = apply_engine_event(&mut core, "time-pos", Some(&v), None);
        assert_eq!(push, EnginePush::Progress);
        assert_eq!(core.state().position_ms, 12_340);
        assert_eq!(core.state().duration_ms, 180_500);

        // pause 事件（外部暂停）
        let v = serde_json::json!(true);
        let (_, push) = apply_engine_event(&mut core, "pause", Some(&v), None);
        assert_eq!(push, EnginePush::State);
        assert_eq!(core.state().status, PlayStatus::Paused);

        // end-file：顺序模式推进到下一首
        let (cmd, push) = apply_engine_event(&mut core, "end-file", None, None);
        assert_eq!(cmd, Some(EngineCmd::Load { track_id: "b".into() }));
        assert_eq!(push, EnginePush::State);

        // 队尾 end-file：停止，无引擎动作
        let (cmd, _) = apply_engine_event(&mut core, "end-file", None, None);
        assert!(cmd.is_none());
        assert_eq!(core.state().status, PlayStatus::Stopped);

        // 未知事件与缺失 value 均无副作用
        let (cmd, push) = apply_engine_event(&mut core, "whatever", None, None);
        assert!(cmd.is_none());
        assert_eq!(push, EnginePush::None);
        let (_, push) = apply_engine_event(&mut core, "time-pos", None, None);
        assert_eq!(push, EnginePush::None);
    }

    #[test]
    fn cue_window_translates_time_and_duration() {
        let mut core = core_with_queue(&["a"], 0);
        let window = CueWindow {
            start_ms: 60_000,
            end_ms: Some(200_000),
        };

        // file-loaded：整轨 3600s → 轨时长被窗口裁剪为 140s
        let v = serde_json::json!(3600.0);
        apply_engine_event(&mut core, "file-loaded", Some(&v), Some(&window));
        assert_eq!(core.state().duration_ms, 140_000);

        // time-pos：绝对 61.5s → 轨内 1.5s；窗口前的时间钳位到 0
        let v = serde_json::json!(61.5);
        apply_engine_event(&mut core, "time-pos", Some(&v), Some(&window));
        assert_eq!(core.state().position_ms, 1_500);
        let v = serde_json::json!(10.0);
        apply_engine_event(&mut core, "time-pos", Some(&v), Some(&window));
        assert_eq!(core.state().position_ms, 0);

        // 末轨（end 缺失）：时长 = 文件尾 - start
        let last = CueWindow {
            start_ms: 60_000,
            end_ms: None,
        };
        let v = serde_json::json!(100.0);
        apply_engine_event(&mut core, "duration", Some(&v), Some(&last));
        assert_eq!(core.state().duration_ms, 40_000);
    }

    #[test]
    fn watch_payload_serializes_contract_shape() {
        let p = WatchPayload::Progress {
            position_ms: 1500,
            duration_ms: 60000,
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({ "kind": "progress", "positionMs": 1500, "durationMs": 60000 })
        );

        let s = WatchPayload::State {
            state: PlayerState::default(),
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["kind"], "state");
        assert!(v["state"].get("playMode").is_some());
    }

    #[test]
    fn engine_cmd_payload_serializes_kinds() {
        let cases = [
            (
                EngineCmdPayload::Load {
                    path: "C:/a.flac".into(),
                    start_ms: None,
                    end_ms: None,
                },
                serde_json::json!({ "kind": "load", "path": "C:/a.flac", "startMs": null, "endMs": null }),
            ),
            (
                EngineCmdPayload::Load {
                    path: "C:/整轨.ape".into(),
                    start_ms: Some(60_000),
                    end_ms: Some(200_000),
                },
                serde_json::json!({ "kind": "load", "path": "C:/整轨.ape", "startMs": 60_000, "endMs": 200_000 }),
            ),
            (EngineCmdPayload::Pause, serde_json::json!({ "kind": "pause" })),
            (EngineCmdPayload::Resume, serde_json::json!({ "kind": "resume" })),
            (EngineCmdPayload::Stop, serde_json::json!({ "kind": "stop" })),
            (
                EngineCmdPayload::Seek { position_ms: 90000 },
                serde_json::json!({ "kind": "seek", "positionMs": 90000 }),
            ),
            (
                EngineCmdPayload::SetVolume { volume: 75 },
                serde_json::json!({ "kind": "setVolume", "volume": 75 }),
            ),
        ];
        for (payload, expected) in cases {
            assert_eq!(serde_json::to_value(&payload).unwrap(), expected);
        }
    }

    #[test]
    fn secs_to_ms_rounds_and_clamps() {
        assert_eq!(secs_to_ms(1.2345), 1235);
        assert_eq!(secs_to_ms(0.0), 0);
        assert_eq!(secs_to_ms(-3.0), 0);
    }
}
