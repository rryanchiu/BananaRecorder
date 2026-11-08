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

/// Win32 GetDiskFreeSpaceExW binding — free bytes of the drive holding `path`.
#[cfg(target_os = "windows")]
fn windows_disk_free(path: &Path) -> Option<u64> {
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

#[cfg(not(target_os = "windows"))]
fn windows_disk_free(_path: &Path) -> Option<u64> {
    None
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<StatusResponse, String> {
    let settings = state.settings.read().await.clone();
    let save_root = settings.save_dir(&state.downloads_dir);
    let recording_files = state.recorder.active_recordings().await;
    let rooms = state.rooms.read().await;
    let disk_free = windows_disk_free(&save_root).map(|bytes| (bytes as f64 / 1024.0 / 1024.0 / 1024.0 * 100.0).round() / 100.0);

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
