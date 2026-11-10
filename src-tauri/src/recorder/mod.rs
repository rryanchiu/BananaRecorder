// Recorder engine: one tokio task per room, ffmpeg guarded per recording.
// Mirrors main.py start_record state machine (main.py:545-1645).

pub mod ffmpeg;

use ffmpeg::{build_args, convert_to_mp4, find_ffmpeg, RecordSpec, RecordingProcess};
use crate::config::AppSettings;
use crate::events::LogBuffer;
use crate::resolver::{self, Platform};
use crate::rooms::Room;
use tauri::{AppHandle, Manager};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

/// Snapshot of one in-flight recording, for status reporting.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ActiveRecording {
    pub room_id: String,
    pub anchor: String,
    pub folder: String,
    pub file: String,
    pub size: u64,
    #[serde(skip)]
    pub stream_url: String,
}

#[derive(Clone)]
struct RoomSignals {
    cancel: CancellationToken,
}

pub struct RecorderManager {
    pub settings: Arc<RwLock<AppSettings>>,
    pub rooms: Arc<RwLock<Vec<Room>>>,
    pub log_buffer: Arc<LogBuffer>,
    engine_running: AtomicBool,
    engine_started_at: Mutex<Option<std::time::Instant>>,
    tasks: Mutex<HashMap<String, RoomSignals>>,
    recordings: Arc<Mutex<HashMap<String, ActiveRecording>>>,
}

impl RecorderManager {
    pub fn new(
        settings: Arc<RwLock<AppSettings>>,
        rooms: Arc<RwLock<Vec<Room>>>,
        log_buffer: Arc<LogBuffer>,
    ) -> Self {
        Self {
            settings,
            rooms,
            log_buffer,
            engine_running: AtomicBool::new(false),
            engine_started_at: Mutex::new(None),
            tasks: Mutex::new(HashMap::new()),
            recordings: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn is_running(&self) -> bool {
        self.engine_running.load(Ordering::SeqCst)
    }

    pub fn uptime(&self) -> u64 {
        match self.engine_started_at.try_lock() {
            Ok(guard) => match *guard {
                Some(started) => started.elapsed().as_secs(),
                None => 0,
            },
            Err(_) => 0,
        }
    }

    pub async fn active_recordings(&self) -> Vec<ActiveRecording> {
        self.recordings.lock().await.values().cloned().collect()
    }

    fn log(&self, app: &tauri::AppHandle, level: &str, message: &str) {
        crate::events::app_log(app, &self.log_buffer, level, message);
    }

    /// Start the engine (idempotent).
    pub async fn start(&self, app: AppHandle) -> Result<(), String> {
        if self.is_running() {
            return Ok(());
        }
        // fail fast when ffmpeg is missing
        let custom = app
            .state::<crate::AppState>()
            .settings
            .read()
            .await
            .ffmpeg_path
            .clone();
        find_ffmpeg(Some(&app), Some(&custom))?;
        self.engine_running.store(true, Ordering::SeqCst);
        *self.engine_started_at.lock().await = Some(std::time::Instant::now());
        self.log(&app, "INFO", "录制引擎已启动");
        self.sync_rooms(&app).await;
        Ok(())
    }

    /// Stop everything: cancel room tasks; they gracefully stop their ffmpeg.
    pub async fn stop(&self, app: AppHandle) {
        if !self.is_running() {
            return;
        }
        self.engine_running.store(false, Ordering::SeqCst);
        *self.engine_started_at.lock().await = None;
        let mut tasks = self.tasks.lock().await;
        for (_, signals) in tasks.drain() {
            signals.cancel.cancel();
        }
        drop(tasks);
        self.log(&app, "INFO", "录制引擎已停止");
    }

    /// Cancel one room task immediately (used on room delete / url change).
    pub async fn stop_task_for(&self, _app: &AppHandle, room_id: &str) {
        if let Some(signals) = self.tasks.lock().await.remove(room_id) {
            signals.cancel.cancel();
        }
    }

    /// Align running room tasks with the room registry: add new enabled rooms,
    /// cancel removed or disabled ones. Called after any room mutation.
    pub async fn sync_rooms(&self, app: &AppHandle) {
        if !self.is_running() {
            return;
        }
        let rooms = self.rooms.read().await.clone();
        let mut tasks = self.tasks.lock().await;

        for room in &rooms {
            let should_run = room.enabled && !tasks.contains_key(&room.id);
            if should_run {
                let cancel = CancellationToken::new();
                tasks.insert(room.id.clone(), RoomSignals { cancel: cancel.clone() });
                tauri::async_runtime::spawn(room_task(
                    app.clone(),
                    self.handle(),
                    room.clone(),
                    cancel,
                ));
                self.log(app, "INFO", &format!("新增监控: {}", display_name(room)));
            }
        }

        let enabled_ids: std::collections::HashSet<&String> = rooms
            .iter()
            .filter(|r| r.enabled)
            .map(|r| &r.id)
            .collect();
        for (id, signals) in tasks.iter() {
            if !enabled_ids.contains(id) {
                signals.cancel.cancel();
            }
        }
        tasks.retain(|id, _| enabled_ids.contains(id));
    }

    fn handle(&self) -> Arc<ManagerHandle> {
        Arc::new(ManagerHandle {
            settings: self.settings.clone(),
            log_buffer: self.log_buffer.clone(),
            recordings: self.recordings.clone(),
        })
    }
}

/// Cheap clonable handle handed to room tasks.
struct ManagerHandle {
    settings: Arc<RwLock<AppSettings>>,
    log_buffer: Arc<LogBuffer>,
    recordings: Arc<Mutex<HashMap<String, ActiveRecording>>>,
}

impl ManagerHandle {
    fn log(&self, app: &tauri::AppHandle, level: &str, message: &str) {
        crate::events::app_log(app, &self.log_buffer, level, message);
    }

    async fn set_recording(&self, info: ActiveRecording) {
        self.recordings.lock().await.insert(info.room_id.clone(), info);
    }

    async fn clear_recording(&self, room_id: &str) {
        self.recordings.lock().await.remove(room_id);
    }

    async fn current_settings(&self) -> AppSettings {
        self.settings.read().await.clone()
    }
}

/// Per-room loop: resolve → record → wait → schedule.
async fn room_task(
    app: AppHandle,
    handle: Arc<ManagerHandle>,
    room: Room,
    cancel: CancellationToken,
) {
    let platform = match Platform::detect(&room.url) {
        Some(p) => p,
        None => {
            handle.log(&app, "ERROR", &format!("无法识别平台: {}", room.url));
            return;
        }
    };

    loop {
        if sleep_or_cancel(1, &cancel).await {
            break;
        }

        let settings = handle.current_settings().await;
        let proxy = settings.proxy_url();
        let cookie = settings.cookie_for(platform.key()).map(|s| s.to_string());
        let quality = if room.quality.is_empty() {
            settings.quality.clone()
        } else {
            room.quality.clone()
        };

        let state = resolver::resolve(
            platform,
            &room.url,
            &quality,
            cookie.as_deref(),
            proxy.clone(),
        )
        .await;

        let state = match state {
            Ok(state) => state,
            Err(err) => {
                handle.log(
                    &app,
                    "ERROR",
                    &format!("[{}] {} 解析失败: {err}", platform.display_name(), display_name(&room)),
                );
                if sleep_or_cancel(settings.delay_seconds, &cancel).await {
                    break;
                }
                continue;
            }
        };

        let anchor = if room.name.is_empty() {
            state.anchor_name.clone()
        } else {
            room.name.clone()
        };

        if !state.is_live {
            // waiting for the streamer to go live
            if sleep_or_cancel(settings.delay_seconds, &cancel).await {
                break;
            }
            continue;
        }

        // ---- live: start a recording ----
        let stream_url = state.flv_url.clone().or(state.m3u8_url.clone()).unwrap_or_default();
        if stream_url.is_empty() {
            if sleep_or_cancel(settings.delay_seconds, &cancel).await {
                break;
            }
            continue;
        }

        let anchor_clean = crate::naming::clean_name(&anchor, true);
        let (dir, filename_base) = crate::naming::build_output(
            &settings.save_dir(&app_data_dir(&app)),
            platform.display_name(),
            &anchor_clean,
            settings.folder_by_author,
            settings.folder_by_time,
        );
        if let Err(err) = std::fs::create_dir_all(&dir) {
            handle.log(&app, "ERROR", &format!("创建目录失败: {err}"));
            if sleep_or_cancel(30, &cancel).await {
                break;
            }
            continue;
        }
        let ext = settings.save_format.clone();
        let output_path = dir.join(format!("{filename_base}.{ext}"));

        let spec = RecordSpec {
            stream_url: &stream_url,
            output_path: &output_path,
            format: &ext,
            split_enabled: settings.split_enabled,
            split_seconds: settings.split_seconds,
            proxy: proxy.as_deref(),
            extra_headers: "",
            overseas: false,
        };
        let args = build_args(&spec);

        let ffmpeg_path = match find_ffmpeg(Some(&app), Some(&settings.ffmpeg_path)) {
            Ok(path) => path,
            Err(err) => {
                handle.log(&app, "ERROR", &err);
                if sleep_or_cancel(settings.delay_seconds, &cancel).await {
                    break;
                }
                continue;
            }
        };

        let mut process = match RecordingProcess::spawn(&ffmpeg_path, &args) {
            Ok(p) => p,
            Err(err) => {
                handle.log(&app, "ERROR", &err);
                if sleep_or_cancel(settings.delay_seconds, &cancel).await {
                    break;
                }
                continue;
            }
        };

        handle.log(
            &app,
            "INFO",
            &format!(
                "[{}] {} 开播，开始录制 ({})",
                platform.display_name(),
                anchor_clean,
                output_path.display()
            ),
        );

        let mut active = ActiveRecording {
            room_id: room.id.clone(),
            anchor: anchor_clean.clone(),
            folder: dir.to_string_lossy().to_string(),
            file: output_path.to_string_lossy().to_string(),
            size: 0,
            stream_url: stream_url.clone(),
        };
        handle.set_recording(active.clone()).await;

        // ---- guard loop: poll ffmpeg once per second ----
        let started = std::time::Instant::now();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    process.stop_gracefully().await;
                    break;
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                    active.size = current_output_size(&dir, &filename_base, &ext);
                    active.file = active_file(&dir, &filename_base, &ext).to_string_lossy().into_owned();
                    handle.set_recording(active.clone()).await;
                    if process.try_wait().is_some() {
                        break;
                    }
                }
            }
        }

        let exit_code = process.wait().await;
        handle.clear_recording(&room.id).await;
        let recorded_secs = started.elapsed().as_secs();

        // post conversion (main.py: ts → mp4; flv → mp4)
        if settings.convert_to_mp4 && (ext == "ts" || ext == "flv") {
            let final_file = active_file(&dir, &filename_base, &ext);
            if final_file.exists() {
                let ffmpeg_for_convert = ffmpeg_path.clone();
                handle.log(
                    &app,
                    "INFO",
                    &format!("录制结束，正在转码 mp4: {}", final_file.display()),
                );
                match convert_to_mp4(&ffmpeg_for_convert, &final_file, settings.delete_origin).await {
                    Ok(mp4) => handle.log(&app, "INFO", &format!("转码完成: {}", mp4.display())),
                    Err(err) => handle.log(&app, "ERROR", &err),
                }
            }
        }

        handle.log(
            &app,
            "INFO",
            &format!(
                "[{}] {} 本段录制完成（{recorded_secs}秒, 退出码 {exit_code:?}），继续监控",
                platform.display_name(),
                anchor_clean
            ),
        );

        // schedule next check: quick 30s recheck if this recording was short
        let delay = if recorded_secs < 60 { 30 } else { settings.delay_seconds };
        if sleep_or_cancel(delay, &cancel).await {
            break;
        }
    }

    handle.clear_recording(&room.id).await;
}

fn display_name(room: &Room) -> String {
    if room.name.is_empty() {
        room.url.clone()
    } else {
        room.name.clone()
    }
}

/// Sleep for n seconds, waking immediately on cancellation.
/// Returns true when cancellation was requested.
async fn sleep_or_cancel(seconds: u64, cancel: &CancellationToken) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => true,
        _ = tokio::time::sleep(std::time::Duration::from_secs(seconds)) => false,
    }
}

/// Total size of the files currently written for this recording base name
/// (handles both single files and _%03d segment templates).
fn current_output_size(dir: &PathBuf, base: &str, ext: &str) -> u64 {
    let mut total = 0u64;
    let prefix = format!("{base}_");
    let exact = format!("{base}.{ext}");
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let matches = (name.starts_with(&prefix) && name.ends_with(ext))
                || name == exact;
            if matches {
                // Windows directory enumeration can cache size until the writer
                // closes its handle. Query the open file instead.
                if let Ok(meta) = std::fs::File::open(entry.path()).and_then(|file| file.metadata()) {
                    total += meta.len();
                }
            }
        }
    }
    total
}

/// The file currently being written for this recording (newest match).
fn active_file(dir: &PathBuf, base: &str, ext: &str) -> PathBuf {
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    let prefix = format!("{base}_");
    let exact = format!("{base}.{ext}");
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let matches = (name.starts_with(&prefix) && name.ends_with(ext))
                || name == exact;
            if matches {
                if let Ok(meta) = std::fs::File::open(entry.path()).and_then(|file| file.metadata()) {
                    if let Ok(modified) = meta.modified() {
                        if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                            newest = Some((modified, entry.path()));
                        }
                    }
                }
            }
        }
    }
    newest.map(|(_, p)| p).unwrap_or_else(|| dir.join(format!("{base}.{ext}")))
}

fn app_data_dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod output_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_growth_while_writer_is_open_and_sums_segments() {
        let dir = std::env::temp_dir().join(format!("banana-size-test-{:032x}", rand::random::<u128>()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut first = std::fs::File::create(dir.join("record_000.ts")).unwrap();
        first.write_all(&[1; 100]).unwrap();
        first.flush().unwrap();
        assert_eq!(current_output_size(&dir, "record", "ts"), 100);
        first.write_all(&[2; 75]).unwrap();
        first.flush().unwrap();
        assert_eq!(current_output_size(&dir, "record", "ts"), 175);
        let mut second = std::fs::File::create(dir.join("record_001.ts")).unwrap();
        second.write_all(&[3; 50]).unwrap();
        second.flush().unwrap();
        std::fs::write(dir.join("other_000.ts"), [0; 30]).unwrap();
        assert_eq!(current_output_size(&dir, "record", "ts"), 225);
        std::fs::write(dir.join("single.mp4"), [0; 20]).unwrap();
        assert_eq!(current_output_size(&dir, "single", "mp4"), 20);
        drop(first);
        drop(second);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
