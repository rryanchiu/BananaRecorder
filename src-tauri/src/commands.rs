// All #[tauri::command] exposed to the frontend.

use crate::config::{save_settings, AppSettings};
use crate::rooms::{normalize_url, room_id, save_rooms, Room};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::State;

#[derive(Debug, Clone, Serialize)]
pub struct StatusResponse {
    pub recorder: serde_json::Value,
    pub rooms: serde_json::Value,
    pub recording_files: Vec<crate::recorder::ActiveRecording>,
    pub save_path: String,
    pub disk_free_gb: Option<f64>,
}

/// Free bytes on the volume holding `path` (win32 GetDiskFreeSpaceExW).
#[cfg(target_os = "windows")]
fn volume_free(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut u64,
            lpTotalNumberOfBytes: *mut u64,
            lpTotalNumberOfFreeBytes: *mut u64,
        ) -> i32;
    }
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, &mut total_free) };
    if ok != 0 {
        Some(total_free)
    } else {
        None
    }
}

/// Free bytes on the volume holding `path` (POSIX statvfs).
#[cfg(unix)]
fn volume_free(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc == 0 {
        Some(stat.f_bavail as u64 * stat.f_frsize as u64)
    } else {
        None
    }
}

#[cfg(not(any(target_os = "windows", unix)))]
fn volume_free(_path: &Path) -> Option<u64> {
    None
}

/// Free bytes for `path`; falls back to the nearest existing ancestor so a
/// not-yet-created save folder still reports its volume.
fn disk_free_bytes(path: &Path) -> Option<u64> {
    let mut existing = path;
    while !existing.exists() {
        existing = existing.parent()?;
    }
    volume_free(existing)
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<StatusResponse, String> {
    let settings = state.settings.read().await.clone();
    let save_root = settings.save_dir(&state.downloads_dir);
    let recording_files = state.recorder.active_recordings().await;
    let rooms = state.rooms.read().await;
    let disk_free = disk_free_bytes(&save_root).map(|bytes| (bytes as f64 / 1024.0 / 1024.0 / 1024.0 * 100.0).round() / 100.0);

    Ok(StatusResponse {
        recorder: serde_json::json!({
            "running": state.recorder.is_running(),
            "uptime": state.recorder.uptime(),
        }),
        rooms: serde_json::json!({
            "total": rooms.len(),
            "enabled": rooms.iter().filter(|r| r.enabled).count(),
        }),
        recording_files: recording_files
            .into_iter()
            .map(|mut r| {
                r.folder = Path::new(&r.folder)
                    .strip_prefix(&save_root)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or(r.folder);
                r
            })
            .collect(),
        save_path: save_root.to_string_lossy().to_string(),
        disk_free_gb: disk_free,
    })
}

#[tauri::command]
pub async fn control_recorder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    action: String,
) -> Result<serde_json::Value, String> {
    match action.as_str() {
        "start" => state.recorder.start(app).await?,
        "stop" => state.recorder.stop(app).await,
        _ => return Err("不支持的操作".into()),
    }
    Ok(serde_json::json!({ "ok": true, "running": state.recorder.is_running() }))
}

#[tauri::command]
pub async fn list_rooms(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let rooms = state.rooms.read().await;
    Ok(serde_json::json!({
        "rooms": rooms.iter().cloned().collect::<Vec<_>>(),
        "recorder_running": state.recorder.is_running(),
    }))
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomInfo {
    pub platform: String,
    pub anchor_name: String,
    pub is_live: bool,
    pub title: String,
}

/// Resolve a room link far enough to show the anchor name in the add dialog.
#[tauri::command]
pub async fn resolve_room_info(
    state: State<'_, AppState>,
    url: String,
) -> Result<RoomInfo, String> {
    let url = normalize_url(&url)?;
    let platform = crate::resolver::Platform::detect(&url)
        .ok_or_else(|| "暂不支持该平台，仅支持抖音、B站、虎牙、快手".to_string())?;
    let settings = state.settings.read().await.clone();
    let room = crate::resolver::resolve(
        platform,
        &url,
        &settings.quality,
        settings.cookie_for(&url),
        settings.proxy_url(),
    )
    .await?;
    Ok(RoomInfo {
        platform: platform.display_name().to_string(),
        anchor_name: room.anchor_name,
        is_live: room.is_live,
        title: room.title,
    })
}

#[derive(Debug, Deserialize)]
pub struct RoomInput {
    pub url: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub quality: String,
}

#[tauri::command]
pub async fn create_room(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    room: RoomInput,
) -> Result<serde_json::Value, String> {
    let url = normalize_url(&room.url)?;
    let id = room_id(&url);
    {
        let rooms = state.rooms.read().await;
        if rooms.iter().any(|r| r.url == url) {
            return Err("该直播间已存在".into());
        }
    }
    let new_room = Room {
        id,
        enabled: true,
        quality: room.quality,
        url,
        name: room.name.trim().to_string(),
    };
    {
        let mut rooms = state.rooms.write().await;
        rooms.push(new_room);
        save_rooms(&state.rooms_file, &rooms)?;
    }
    state.recorder.sync_rooms(&app).await;
    Ok(serde_json::json!({ "ok": true }))
}

#[derive(Debug, Deserialize)]
pub struct RoomPatch {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Stop the in-flight recording of a room: cancel its task (which sends the
/// graceful stdin-"q" stop to ffmpeg) and disable the room so it does not
/// restart on the next cycle. Re-enable via the room switch.
#[tauri::command]
pub async fn stop_recording(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    room_id: String,
) -> Result<serde_json::Value, String> {
    {
        let mut rooms = state.rooms.write().await;
        let room = rooms
            .iter_mut()
            .find(|r| r.id == room_id)
            .ok_or("直播间不存在")?;
        room.enabled = false;
        save_rooms(&state.rooms_file, &rooms)?;
    }
    state.recorder.stop_task_for(&app, &room_id).await;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn update_room(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    patch: RoomPatch,
) -> Result<serde_json::Value, String> {
    let mut url_changed: Option<String> = None;
    {
        let mut rooms = state.rooms.write().await;
        if let Some(room) = rooms.iter_mut().find(|r| r.id == id) {
            if let Some(name) = &patch.name {
                room.name = name.trim().to_string();
            }
            if let Some(quality) = &patch.quality {
                room.quality = quality.clone();
            }
            if let Some(enabled) = patch.enabled {
                room.enabled = enabled;
            }
        }
        if let Some(url) = &patch.url {
            let new_url = normalize_url(url)?;
            let new_id = room_id(&new_url);
            let duplicated = rooms.iter().any(|r| r.url == new_url && r.id != id);
            if duplicated {
                return Err("该直播间已存在".into());
            }
            let room = rooms
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or("直播间不存在")?;
            room.url = new_url.clone();
            room.id = new_id.clone();
            url_changed = Some(new_id);
        }
        save_rooms(&state.rooms_file, &rooms)?;
    }
    // url edits create a new id: ensure the old task (if any) gets cancelled
    if let Some(new_id) = url_changed {
        if new_id != id {
            state.recorder.stop_task_for(&app, &id).await;
        }
    }
    state.recorder.sync_rooms(&app).await;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn delete_room(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<serde_json::Value, String> {
    {
        let mut rooms = state.rooms.write().await;
        let before = rooms.len();
        rooms.retain(|r| r.id != id);
        if rooms.len() == before {
            return Err("直播间不存在".into());
        }
        save_rooms(&state.rooms_file, &rooms)?;
    }
    state.recorder.stop_task_for(&app, &id).await;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
pub async fn update_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    {
        let mut current = state.settings.write().await;
        *current = settings;
        save_settings(&state.settings_file, &current)?;
    }
    // save path / interval changes take effect on the next resolve cycle
    state.recorder.sync_rooms(&app).await;
    Ok(state.settings.read().await.clone())
}

// ---------------------------------------------------------------------------
// ffmpeg runtime
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct FfmpegStatus {
    pub path: Option<String>,
    pub version: Option<String>,
    /// builtin | custom | installed | path | none
    pub source: String,
    pub builtin_available: bool,
    pub install_supported: bool,
}

async fn resolve_status(app: &tauri::AppHandle, custom: &str) -> FfmpegStatus {
    use crate::recorder::ffmpeg;
    let builtin_available = ffmpeg::builtin_ffmpeg(Some(app)).is_some();
    let (path, version, source) = match ffmpeg::resolve_ffmpeg(Some(app), Some(custom)) {
        Ok((path, source)) => {
            let version = ffmpeg::ffmpeg_version(&path).await;
            // an explicit path pointing at our own install dir reads as "installed"
            let source = if ffmpeg::managed_ffmpeg(app).as_deref() == Some(path.as_path()) {
                "installed"
            } else {
                source.as_str()
            };
            (
                Some(path.to_string_lossy().to_string()),
                version,
                source,
            )
        }
        Err(_) => (None, None, "none"),
    };
    FfmpegStatus {
        path,
        version,
        source: source.to_string(),
        builtin_available,
        install_supported: ffmpeg::install_supported(),
    }
}

#[tauri::command]
pub async fn get_ffmpeg_status(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<FfmpegStatus, String> {
    let custom = state.settings.read().await.ffmpeg_path.clone();
    Ok(resolve_status(&app, &custom).await)
}

/// Empty path = auto (builtin → installed → PATH); otherwise an explicit binary.
#[tauri::command]
pub async fn set_ffmpeg_path(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<FfmpegStatus, String> {
    let path = path.unwrap_or_default().trim().to_string();
    if !path.is_empty() && !Path::new(&path).is_file() {
        return Err("所选文件不存在".into());
    }
    {
        let mut settings = state.settings.write().await;
        settings.ffmpeg_path = path.clone();
        save_settings(&state.settings_file, &settings)?;
    }
    Ok(resolve_status(&app, &path).await)
}

#[tauri::command]
pub async fn install_ffmpeg(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<FfmpegStatus, String> {
    let installed = crate::recorder::ffmpeg::install_ffmpeg(&app).await?;
    let path = installed.to_string_lossy().to_string();
    {
        let mut settings = state.settings.write().await;
        settings.ffmpeg_path = path.clone();
        save_settings(&state.settings_file, &settings)?;
    }
    Ok(resolve_status(&app, &path).await)
}

// ---------------------------------------------------------------------------
// updates (GitHub Releases)
// ---------------------------------------------------------------------------

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/rryanchiu/BananaRecorder/releases/latest";

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub name: String,
    pub notes: String,
    pub html_url: String,
    pub published_at: String,
    pub download_url: Option<String>,
}

/// Numeric version comparison (`v` prefix and pre-release suffixes ignored).
fn version_gt(candidate: &str, current: &str) -> bool {
    fn parts(value: &str) -> Vec<u64> {
        let value = value.trim();
        let value = value
            .strip_prefix('v')
            .or_else(|| value.strip_prefix('V'))
            .unwrap_or(value);
        value
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap_or(0))
            .collect()
    }
    let candidate = parts(candidate);
    let current = parts(current);
    for i in 0..candidate.len().max(current.len()) {
        let a = *candidate.get(i).unwrap_or(&0);
        let b = *current.get(i).unwrap_or(&0);
        if a != b {
            return a > b;
        }
    }
    false
}

/// Latest GitHub release, compared with the running app version.
#[tauri::command]
pub async fn check_update(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<UpdateInfo, String> {
    let current_version = app.package_info().version.to_string();
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(format!("BananaRecorder/{current_version}"));
    if let Some(proxy) = state.settings.read().await.proxy_url() {
        builder = builder.proxy(reqwest::Proxy::all(&proxy).map_err(|e| e.to_string())?);
    }
    let client = builder.build().map_err(|e| e.to_string())?;

    let response = client
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("检查更新失败: {e}"))?;

    // no published release yet — not an error
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(UpdateInfo {
            current_version: current_version.clone(),
            latest_version: current_version,
            available: false,
            name: String::new(),
            notes: String::new(),
            html_url: String::new(),
            published_at: String::new(),
            download_url: None,
        });
    }
    if !response.status().is_success() {
        return Err(format!("检查更新失败: HTTP {}", response.status()));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let tag = json
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let latest_version = tag
        .strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag)
        .to_string();
    let available = !latest_version.is_empty() && version_gt(&latest_version, &current_version);
    // only offer an installer for this OS; the release also contains the other
    // platform's artifacts
    let installer_exts: &[&str] = if cfg!(target_os = "windows") {
        &[".exe", ".msi"]
    } else if cfg!(target_os = "macos") {
        &[".dmg", ".app.tar.gz"]
    } else {
        &[".appimage", ".deb", ".rpm"]
    };
    let download_url = json
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .and_then(|assets| {
            let matched = |asset: &serde_json::Value| -> Option<String> {
                let name = asset.get("name")?.as_str()?.to_ascii_lowercase();
                if !installer_exts.iter().any(|ext| name.ends_with(ext)) {
                    return None;
                }
                asset
                    .get("browser_download_url")?
                    .as_str()
                    .map(str::to_string)
            };
            // prefer the full build if a lite installer is also attached
            assets
                .iter()
                .filter(|asset| {
                    asset
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .map(|name| !name.to_ascii_lowercase().contains("lite"))
                        .unwrap_or(false)
                })
                .find_map(|asset| matched(asset))
                .or_else(|| assets.iter().find_map(|asset| matched(asset)))
        });

    Ok(UpdateInfo {
        current_version,
        latest_version,
        available,
        name: json
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        notes: json
            .get("body")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        html_url: json
            .get("html_url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        published_at: json
            .get("published_at")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        download_url,
    })
}

// ---------------------------------------------------------------------------
// save path migration
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SavePathCheck {
    /// true when the effective folder would change
    pub changed: bool,
    pub old_root: String,
    pub new_root: String,
    pub file_count: u64,
    pub total_size: u64,
}

fn count_video_files(dir: &Path) -> (u64, u64) {
    let mut count = 0u64;
    let mut size = 0u64;
    for file in walk_simple(dir) {
        if video_ext(&file).is_some() {
            count += 1;
            if let Ok(meta) = file.metadata() {
                size += meta.len();
            }
        }
    }
    (count, size)
}

#[tauri::command]
pub async fn check_save_path_change(
    state: State<'_, AppState>,
    new_path: String,
) -> Result<SavePathCheck, String> {
    let settings = state.settings.read().await.clone();
    let old_root = settings.save_dir(&state.downloads_dir);
    let mut candidate = settings.clone();
    candidate.save_path = new_path;
    let new_root = candidate.save_dir(&state.downloads_dir);

    let changed = old_root != new_root;
    let (file_count, total_size) = if changed && old_root.exists() {
        count_video_files(&old_root)
    } else {
        (0, 0)
    };

    Ok(SavePathCheck {
        changed,
        old_root: old_root.to_string_lossy().to_string(),
        new_root: new_root.to_string_lossy().to_string(),
        file_count,
        total_size,
    })
}

#[derive(Debug, Serialize)]
pub struct MigrateResult {
    pub moved: u64,
    pub skipped: u64,
    pub failed: u64,
}

/// Move the whole save tree from the current root to `new_path`.
/// Same-drive moves are atomic renames; cross-drive falls back to copy+delete.
/// Files that already exist at the destination are skipped (reported).
fn migrate_tree(src: &Path, dst: &Path) -> Result<MigrateResult, String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let mut result = MigrateResult {
        moved: 0,
        skipped: 0,
        failed: 0,
    };
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            let sub = migrate_tree(&from, &to)?;
            result.moved += sub.moved;
            result.skipped += sub.skipped;
            result.failed += sub.failed;
        } else if to.exists() {
            result.skipped += 1;
        } else if std::fs::rename(&from, &to).is_ok() {
            result.moved += 1;
        } else {
            // cross-drive fallback
            match std::fs::copy(&from, &to) {
                Ok(_) => {
                    let _ = std::fs::remove_file(&from);
                    result.moved += 1;
                }
                Err(_) => result.failed += 1,
            }
        }
    }
    Ok(result)
}

/// Best-effort removal of now-empty directories under `dir` (inclusive).
fn remove_empty_dirs(dir: &Path) -> bool {
    let mut empty = true;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if !remove_empty_dirs(&path) {
                    empty = false;
                }
            } else {
                empty = false;
            }
        }
    }
    if empty {
        let _ = std::fs::remove_dir(dir);
    }
    empty
}

#[tauri::command]
pub async fn migrate_save_path(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    new_path: String,
) -> Result<MigrateResult, String> {
    let old_root = state.settings.read().await.save_dir(&state.downloads_dir);
    let mut candidate = state.settings.read().await.clone();
    candidate.save_path = new_path;
    let new_root = candidate.save_dir(&state.downloads_dir);

    if old_root == new_root || !old_root.exists() {
        return Ok(MigrateResult {
            moved: 0,
            skipped: 0,
            failed: 0,
        });
    }

    let result = migrate_tree(&old_root, &new_root)?;
    remove_empty_dirs(&old_root);
    crate::events::app_log(
        &app,
        &state.log_buffer,
        "INFO",
        &format!(
            "视频目录迁移完成: {} → {} (移动 {} 个, 跳过 {} 个, 失败 {} 个)",
            old_root.display(),
            new_root.display(),
            result.moved,
            result.skipped,
            result.failed
        ),
    );
    Ok(result)
}

#[tauri::command]
pub async fn get_logs(state: State<'_, AppState>, source: String, lines: usize) -> Result<serde_json::Value, String> {
    match source.as_str() {
        "app" => Ok(serde_json::json!({
            "source": source,
            "lines": state.log_buffer.tail(lines.clamp(1, 500)),
        })),
        _ => Err("未知日志来源".into()),
    }
}

// ---------------------------------------------------------------------------
// videos
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct VideoDir {
    pub name: String,
    pub path: String,
    pub file_count: u64,
    pub total_size: u64,
}

#[derive(Debug, Serialize)]
pub struct VideoFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub mtime: f64,
    pub ext: String,
}

#[derive(Debug, Serialize)]
pub struct VideoListResponse {
    pub root: String,
    pub path: String,
    pub parent: String,
    pub dirs: Vec<VideoDir>,
    pub files: Vec<VideoFile>,
    pub total_size: u64,
}

fn video_ext(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    match ext.as_str() {
        "ts" | "mp4" | "mkv" | "flv" | "mp3" | "m4a" | "mov" | "webm" | "aac" => Some(ext),
        _ => None,
    }
}

fn resolve_under_root(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.replace('\\', "/");
    let rel = rel.trim_start_matches('/');
    let target = root.join(rel);
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical_target = target.canonicalize().unwrap_or_else(|_| target.clone());
    if canonical_target != canonical_root && !canonical_target.starts_with(&canonical_root) {
        return Err("非法路径".into());
    }
    Ok(target)
}

#[tauri::command]
pub async fn list_videos(state: State<'_, AppState>, path: String) -> Result<VideoListResponse, String> {
    let settings = state.settings.read().await.clone();
    let root = settings.save_dir(&state.downloads_dir);
    if !root.exists() {
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    }
    let target = resolve_under_root(&root, &path)?;
    if !target.exists() {
        return Err("路径不存在".into());
    }

    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let mut total_size = 0u64;

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&target)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default());

    for entry in entries {
        let name = entry
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if entry.is_dir() {
            let mut file_count = 0u64;
            let mut dir_size = 0u64;
            for inner in walk_simple(&entry) {
                if video_ext(&inner).is_some() {
                    file_count += 1;
                }
                if let Ok(meta) = inner.metadata() {
                    dir_size += meta.len();
                }
            }
            dirs.push(VideoDir {
                name,
                path: relative_to(&root, &entry),
                file_count,
                total_size: dir_size,
            });
        } else if let Some(ext) = video_ext(&entry) {
            if let Ok(meta) = entry.metadata() {
                total_size += meta.len();
                files.push(VideoFile {
                    name,
                    path: entry.to_string_lossy().to_string(),
                    size: meta.len(),
                    mtime: meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0),
                    ext: format!(".{ext}"),
                });
            }
        }
    }

    Ok(VideoListResponse {
        root: root.to_string_lossy().to_string(),
        path: if target == root {
            String::new()
        } else {
            relative_to(&root, &target)
        },
        parent: if target == root {
            String::new()
        } else {
            target
                .parent()
                .map(|p| relative_to(&root, p))
                .unwrap_or_default()
        },
        dirs,
        files,
        total_size,
    })
}

/// shallow recursive walk with a depth cap (keeps folder summaries cheap)
fn walk_simple(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        if depth > 5 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, depth + 1));
            } else {
                out.push(path);
            }
        }
    }
    out
}

fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"))
}

#[tauri::command]
pub async fn delete_video(state: State<'_, AppState>, path: String) -> Result<serde_json::Value, String> {
    let settings = state.settings.read().await.clone();
    let root = settings.save_dir(&state.downloads_dir);
    let target = resolve_under_root(&root, &path)?;
    if target == root {
        return Err("不能删除保存根目录".into());
    }
    if target.is_file() {
        std::fs::remove_file(&target).map_err(|e| e.to_string())?;
    } else if target.is_dir() {
        std::fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    } else {
        return Err("文件不存在".into());
    }
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn save_video_as(
    state: State<'_, AppState>,
    path: String,
    dest_path: String,
) -> Result<serde_json::Value, String> {
    let settings = state.settings.read().await.clone();
    let root = settings.save_dir(&state.downloads_dir);
    let source = resolve_under_root(&root, &path)?;
    std::fs::copy(&source, &dest_path).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn open_video_folder(state: State<'_, AppState>, path: String) -> Result<serde_json::Value, String> {
    let settings = state.settings.read().await.clone();
    let root = settings.save_dir(&state.downloads_dir);
    let target = resolve_under_root(&root, &path)?;
    let folder = if target.is_dir() { target } else { target.parent().unwrap_or(&root).to_path_buf() };
    tauri_plugin_opener::open_path(folder, None::<&str>).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// batch operations on video files
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn delete_videos(state: State<'_, AppState>, paths: Vec<String>) -> Result<serde_json::Value, String> {
    let settings = state.settings.read().await.clone();
    let root = settings.save_dir(&state.downloads_dir);
    let mut deleted = 0u64;
    let mut failed = 0u64;
    for path in &paths {
        match resolve_under_root(&root, path) {
            Ok(target) if target != root => {
                let ok = if target.is_file() {
                    std::fs::remove_file(&target).is_ok()
                } else if target.is_dir() {
                    std::fs::remove_dir_all(&target).is_ok()
                } else {
                    false
                };
                if ok {
                    deleted += 1;
                } else {
                    failed += 1;
                }
            }
            _ => failed += 1,
        }
    }
    Ok(serde_json::json!({ "deleted": deleted, "failed": failed }))
}

#[tauri::command]
pub async fn merge_videos(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<serde_json::Value, String> {
    if paths.len() < 2 {
        return Err("至少选择两个视频才能合并".into());
    }
    let settings = state.settings.read().await.clone();
    let root = settings.save_dir(&state.downloads_dir);

    // resolve all inputs; every file must share one container format
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    let mut ext: Option<String> = None;
    for path in &paths {
        let target = resolve_under_root(&root, path)?;
        if !target.is_file() {
            return Err(format!("文件不存在: {}", target.display()));
        }
        let file_ext = video_ext(&target).ok_or("选择中包含不支持的文件类型")?;
        match &ext {
            Some(prev) if *prev != file_ext => {
                return Err("合并的视频格式不一致，请先批量转码为相同格式（如 mp4）".into())
            }
            _ => ext = Some(file_ext),
        }
        let mtime = target
            .metadata()
            .and_then(|m| m.modified())
            .map_err(|e| e.to_string())?;
        files.push((target, mtime));
    }
    let ext = ext.unwrap_or_default();

    // recording order = oldest modification time first
    files.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

    // ffmpeg concat demuxer with a temporary file list
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let first_dir = files[0]
        .0
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.clone());
    let list_path = first_dir.join(format!("concat_{stamp}.txt"));
    let mut list = String::new();
    for (file, _) in &files {
        // single quotes in paths need ffmpeg's escaping
        let escaped = file.to_string_lossy().replace('\'', "'\\''");
        list.push_str(&format!("file '{escaped}'\n"));
    }
    std::fs::write(&list_path, &list).map_err(|e| e.to_string())?;

    let output_path = first_dir.join(format!("merged_{stamp}.{ext}"));
    let ffmpeg = crate::recorder::ffmpeg::find_ffmpeg(Some(&app), Some(&settings.ffmpeg_path))?;
    let mut cmd = tokio::process::Command::new(&ffmpeg);
    cmd.args([
        "-y",
        "-f", "concat",
        "-safe", "0",
        "-i", &list_path.to_string_lossy(),
        "-c", "copy",
        &output_path.to_string_lossy(),
    ])
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
    crate::recorder::ffmpeg::hide_console(&mut cmd);
    let status = cmd
        .status()
        .await
        .map_err(|e| format!("启动 ffmpeg 合并失败: {e}"))?;
    let _ = std::fs::remove_file(&list_path);

    if !status.success() {
        let _ = std::fs::remove_file(&output_path);
        return Err("合并失败：所选视频的编码参数可能不一致，请先批量转码后再合并".into());
    }

    crate::events::app_log(
        &app,
        &state.log_buffer,
        "INFO",
        &format!(
            "视频合并完成: {} 个文件 → {}",
            files.len(),
            output_path.display()
        ),
    );
    Ok(serde_json::json!({
        "ok": true,
        "output": output_path.to_string_lossy(),
        "count": files.len(),
    }))
}

#[tauri::command]
pub async fn transcode_videos(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<serde_json::Value, String> {
    let settings = state.settings.read().await.clone();
    let root = settings.save_dir(&state.downloads_dir);
    let delete_origin = settings.delete_origin;
    let ffmpeg = crate::recorder::ffmpeg::find_ffmpeg(Some(&app), Some(&settings.ffmpeg_path))?;

    let mut converted = 0u64;
    let mut skipped = 0u64;
    let mut failed = 0u64;
    for path in &paths {
        let target = match resolve_under_root(&root, path) {
            Ok(t) => t,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        if video_ext(&target).as_deref() == Some("mp4") {
            skipped += 1;
            continue;
        }
        if target.is_file() {
            match crate::recorder::ffmpeg::convert_to_mp4(&ffmpeg, &target, delete_origin).await {
                Ok(_) => converted += 1,
                Err(_) => failed += 1,
            }
        } else {
            failed += 1;
        }
    }

    crate::events::app_log(
        &app,
        &state.log_buffer,
        "INFO",
        &format!(
            "批量转码完成: 转码 {converted} 个, 跳过 {skipped} 个 (已是 mp4), 失败 {failed} 个"
        ),
    );
    Ok(serde_json::json!({ "converted": converted, "skipped": skipped, "failed": failed }))
}

#[cfg(test)]
mod disk_tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn reports_free_space_on_unix() {
        let free = disk_free_bytes(Path::new("/")).expect("statvfs should succeed");
        assert!(free > 0);
    }

    #[test]
    fn missing_paths_do_not_panic() {
        let _ = disk_free_bytes(Path::new("/definitely/not/a/real/path"));
    }

    #[test]
    fn version_gt_compares_numerically() {
        assert!(version_gt("0.2.0", "0.1.9"));
        assert!(version_gt("1.0.0", "0.9.9"));
        assert!(version_gt("v0.10.0", "v0.9.0"));
        assert!(!version_gt("0.1.0", "0.1.0"));
        assert!(!version_gt("0.1.0", "0.2.0"));
    }
}
