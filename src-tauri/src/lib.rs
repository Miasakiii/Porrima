//! Porrima 后端库入口：模块装配、日志初始化、Tauri 启动。

mod commands;
mod db;
mod error;
mod models;
mod services;

pub use error::{AppError, AppResult};

/// 初始化 tracing 日志。
///
/// 默认 `porrima_lib=debug`（debug 构建）/ `info`（release），
/// 可用 `RUST_LOG` 环境变量覆盖，例如 `RUST_LOG=porrima_lib=trace`。
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let default_directive = if cfg!(debug_assertions) {
        "porrima_lib=debug"
    } else {
        "info"
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_directive));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}

/// 系统托盘：右键菜单（播放/暂停、上/下一首、显示窗口、退出），双击显示窗口。
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri::Manager;

    let show = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
    let toggle = MenuItemBuilder::with_id("toggle", "播放/暂停").build(app)?;
    let prev = MenuItemBuilder::with_id("prev", "上一首").build(app)?;
    let next = MenuItemBuilder::with_id("next", "下一首").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&toggle)
        .item(&prev)
        .item(&next)
        .separator()
        .item(&quit)
        .build()?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().unwrap())
        .tooltip("Porrima")
        .menu(&menu)
        .on_menu_event(|app, event| {
            let state = app.state::<commands::AppState>();
            match event.id().as_ref() {
                "show" => {
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.unminimize();
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                }
                "toggle" => {
                    let _ = commands::player::tray_toggle(app, &state);
                }
                "prev" => {
                    let _ = commands::player::tray_previous(app, &state);
                }
                "next" => {
                    let _ = commands::player::tray_next(app, &state);
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            // 双击托盘图标显示窗口
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.unminimize();
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tracing::info!("starting Porrima v{}", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        // single-instance 必须最先注册：二次启动时聚焦已有窗口并转发文件参数
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            tracing::info!(?argv, "second instance launched, forwarding args");
            commands::player::handle_second_instance(app, argv);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_libmpv::init())
        .on_window_event(|window, event| {
            // 关闭窗口时最小化到托盘（而非退出）；用户通过托盘菜单“退出”真正退出。
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            use tauri::Manager;

            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("porrima.db");
            tracing::info!(path = %db_path.display(), "opening database");

            let store = db::store::Store::open(&db_path)?;
            // 系统媒体控制封面的临时目录（app cache dir，取不到则退回数据目录）。
            let media_cover_dir = app
                .path()
                .app_cache_dir()
                .unwrap_or_else(|_| data_dir.clone())
                .join("media-covers");
            let state = commands::AppState::new(store, media_cover_dir);

            // 首次启动的命令行文件参数：挂起到引擎适配器就绪（engine_ready）后播放
            let args: Vec<String> = std::env::args()
                .skip(1)
                .filter(|a| !a.starts_with('-'))
                .collect();
            if !args.is_empty() {
                tracing::info!(?args, "queueing files from command line");
                if let Ok(mut pending) = state.pending_open.lock() {
                    pending.extend(args);
                }
            }

            app.manage(state);

            // 系统媒体控制（SMTC/MPRIS/Now Playing）：在主线程（STA + 消息泵）初始化最稳妥。
            // 失败时静默禁用，不影响其余功能。
            if let Some(controls) = commands::media_controls::init(&app.handle().clone()) {
                if let Ok(mut guard) = app.state::<commands::AppState>().media.lock() {
                    *guard = Some(controls);
                }
            }

            // 系统托盘：右键菜单控制播放，双击显示窗口
            setup_tray(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::library::scan_library,
            commands::library::cancel_scan,
            commands::library::list_tracks,
            commands::library::get_track,
            commands::library::get_tracks,
            commands::library::get_library_stats,
            commands::library::get_stats_summary,
            commands::library::list_recently_played,
            commands::library::list_most_played,
            commands::library::list_albums,
            commands::library::list_artists,
            commands::library::get_album_tracks,
            commands::library::get_artist_tracks,
            commands::library::get_cover,
            commands::library::get_cover_color,
            commands::library::get_lyrics,
            commands::library::search_lyrics_online,
            commands::library::save_lyrics_file,
            commands::library::search_cover_online,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::playlists::create_playlist,
            commands::playlists::list_playlists,
            commands::playlists::rename_playlist,
            commands::playlists::delete_playlist,
            commands::playlists::get_playlist_tracks,
            commands::playlists::add_to_playlist,
            commands::playlists::remove_from_playlist,
            commands::playlists::move_in_playlist,
            commands::player::play_track,
            commands::player::play_queue,
            commands::player::toggle_play,
            commands::player::stop,
            commands::player::next_track,
            commands::player::previous_track,
            commands::player::seek,
            commands::player::set_volume,
            commands::player::set_muted,
            commands::player::set_play_mode,
            commands::player::get_player_state,
            commands::player::queue_add,
            commands::player::queue_remove,
            commands::player::queue_move,
            commands::player::queue_clear,
            commands::player::watch_player,
            commands::player::engine_event,
            commands::player::engine_ready,
            commands::player::open_dropped_files,
            commands::browse::browse_dir,
            commands::browse::play_video_file,
            commands::browse::save_video_position,
            commands::browse::get_video_position,
            commands::browse::get_screenshot_dir,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            use tauri::Manager;
            // 退出时持久化播放状态（队列/位置/音量/模式）
            if let tauri::RunEvent::Exit = event {
                let state = app_handle.state::<commands::AppState>();
                let snapshot = state.player.lock().ok().map(|p| p.state().clone());
                if let Some(snapshot) = snapshot {
                    if let Ok(store) = state.store.lock() {
                        if let Err(e) = store.save_player_state(&snapshot) {
                            tracing::warn!(error = %e, "failed to persist player state on exit");
                        } else {
                            tracing::debug!("player state persisted on exit");
                        }
                    }
                }
            }
        });
}
