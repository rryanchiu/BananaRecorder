mod commands;
mod preview;
mod config;
mod events;
mod naming;
pub mod recorder;
pub mod resolver;
mod rooms;

use events::LogBuffer;
use recorder::RecorderManager;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};
#[cfg(not(target_os = "macos"))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tokio::sync::RwLock;

pub struct AppState {
    pub settings: Arc<RwLock<config::AppSettings>>,
    pub rooms: Arc<RwLock<Vec<rooms::Room>>>,
    pub recorder: RecorderManager,
    pub log_buffer: Arc<LogBuffer>,
    pub settings_file: PathBuf,
    pub rooms_file: PathBuf,
    pub downloads_dir: PathBuf,
}

/// Show (and focus) the main window, optionally navigating the frontend
/// to a route (tray menu → page).
fn show_main(app: &tauri::AppHandle, route: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("app://navigate", route);
    }
}

/// macOS: replace the menu-bar banana with the active recording count, and
/// restore the banana when nothing is recording.
#[cfg(target_os = "macos")]
fn update_tray_badge(app: &tauri::AppHandle, count: usize) {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return;
    };
    if count > 0 {
        // set the title before dropping the icon so the status item never
        // collapses to zero width between the two calls
        let _ = tray.set_title(Some(count.to_string()));
        let _ = tray.set_icon(None);
    } else {
        if let Some(icon) = app.default_window_icon() {
            let _ = tray.set_icon(Some(icon.clone()));
        }
        let _ = tray.set_title(None::<&str>);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(preview::Previews::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // second launch: surface the existing window on the dashboard
            show_main(app, "/");
        }))
        // closing the window hides it to the tray so monitoring keeps running;
        // real exit goes through the tray menu (graceful recording stop)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .expect("failed to resolve config dir");
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| config_dir.clone());
            std::fs::create_dir_all(&config_dir).ok();
            std::fs::create_dir_all(data_dir.join("downloads")).ok();

            let settings_file = config_dir.join("settings.json");
            let rooms_file = config_dir.join("rooms.json");

            let settings = Arc::new(RwLock::new(config::load_settings(&settings_file)));
            let rooms = Arc::new(RwLock::new(rooms::load_rooms(&rooms_file)));
            let log_buffer = Arc::new(LogBuffer::new(500));

            let state = AppState {
                settings: settings.clone(),
                rooms: rooms.clone(),
                recorder: RecorderManager::new(
                    settings.clone(),
                    rooms.clone(),
                    log_buffer.clone(),
                ),
                log_buffer: log_buffer.clone(),
                settings_file,
                rooms_file,
                downloads_dir: data_dir.clone(),
            };
            app.manage(state);

            // ----- macOS window corners -----
            // Frameless windows are square by default. Clip the content view to
            // a rounded rect and use CALayer's continuous curve, which is the
            // same "squircle" smoothing native macOS windows use.
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.with_webview(|webview| {
                    use objc2_app_kit::{NSColor, NSWindow};
                    use objc2_quartz_core::kCACornerCurveContinuous;

                    // SAFETY: `with_webview` runs on the main thread and the
                    // pointers refer to the live window/webview.
                    unsafe {
                        let ns_window: &NSWindow = &*webview.ns_window().cast();
                        let clear = NSColor::clearColor();
                        ns_window.setOpaque(false);
                        ns_window.setBackgroundColor(Some(&clear));

                        if let Some(content) = ns_window.contentView() {
                            content.setWantsLayer(true);
                            if let Some(layer) = content.layer() {
                                layer.setCornerRadius(16.0);
                                layer.setCornerCurve(kCACornerCurveContinuous);
                                layer.setMasksToBounds(true);
                            }
                        }
                        ns_window.invalidateShadow();
                    }
                });
            }

            // ----- system tray -----
            let open_dashboard =
                MenuItem::with_id(app, "open_dashboard", "打开仪表盘", true, None::<&str>)?;
            let open_videos =
                MenuItem::with_id(app, "open_videos", "打开视频库", true, None::<&str>)?;
            let open_rooms =
                MenuItem::with_id(app, "open_rooms", "直播间管理", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出应用", true, None::<&str>)?;
            let tray_menu = Menu::with_items(
                app,
                &[&open_dashboard, &open_videos, &open_rooms, &quit],
            )?;

            // A transparent, full-size banana stays legible at Windows tray sizes.
            // Keep the framed application icon for the window and other platforms.
            #[cfg(target_os = "windows")]
            let tray_icon = tauri::include_image!("../public/banana.png");
            #[cfg(not(target_os = "windows"))]
            let tray_icon = app.default_window_icon().expect("missing bundle icon").clone();
            let builder = TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .tooltip("香蕉录制 BananaRecorder")
                .menu(&tray_menu);
            // macOS convention: clicking the menu-bar item opens the menu.
            // Windows/Linux keep left-click → dashboard (menu on right click).
            #[cfg(target_os = "macos")]
            let builder = builder.show_menu_on_left_click(true);
            #[cfg(not(target_os = "macos"))]
            let builder = builder.show_menu_on_left_click(false);
            let tray = builder.build(app)?;
            tray.on_menu_event(|app, event| match event.id.as_ref() {
                "open_dashboard" => show_main(app, "/"),
                "open_videos" => show_main(app, "/videos"),
                "open_rooms" => show_main(app, "/rooms"),
                "quit" => {
                    // stop recordings gracefully (stdin-q → container finalize)
                    // before exiting the process
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<AppState>();
                        state.recorder.stop(app.clone()).await;
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        app.exit(0);
                    });
                }
                _ => {}
            });
            // Windows/Linux only: left click opens the dashboard.
            #[cfg(not(target_os = "macos"))]
            tray.on_tray_icon_event(|tray, event| {
                let clicked = matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    }
                );
                if clicked {
                    show_main(tray.app_handle(), "/");
                }
            });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // push aggregated status to the frontend every 2 seconds
                let mut last_recordings = 0usize;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let status = {
                        let state = handle.state::<AppState>();
                        commands::get_status(state).await
                    };
                    if let Ok(status) = status {
                        let count = status.recording_files.len();
                        if count != last_recordings {
                            last_recordings = count;
                            #[cfg(target_os = "macos")]
                            update_tray_badge(&handle, count);
                        }
                        let _ = handle.emit("recorder://status", status);
                    }
                }
            });

            // auto-start the engine when there are enabled rooms saved
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                let has_enabled = state.rooms.read().await.iter().any(|r| r.enabled);
                if has_enabled {
                    if let Err(err) = state.recorder.start(handle.clone()).await {
                        events::app_log(&handle, &state.log_buffer, "ERROR", &err);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            preview::start_preview,
            preview::touch_preview,
            preview::stop_preview,
            commands::get_status,
            commands::control_recorder,
            commands::list_rooms,
            commands::resolve_room_info,
            commands::create_room,
            commands::update_room,
            commands::delete_room,
            commands::stop_recording,
            commands::get_settings,
            commands::update_settings,
            commands::check_update,
            commands::get_ffmpeg_status,
            commands::set_ffmpeg_path,
            commands::install_ffmpeg,
            commands::check_save_path_change,
            commands::migrate_save_path,
            commands::get_logs,
            commands::list_videos,
            commands::delete_video,
            commands::delete_videos,
            commands::merge_videos,
            commands::transcode_videos,
            commands::save_video_as,
            commands::open_video_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
