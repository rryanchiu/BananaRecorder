//! Local rolling HLS preview of an active recording's stream.
use std::{collections::HashMap, sync::{Arc, Mutex}, time::{Duration, Instant}};
use tauri::{Manager, State};
use tokio_util::sync::CancellationToken;

struct Session { cancel: CancellationToken, touched: Mutex<Instant> }
#[derive(Default)]
pub struct Previews(Mutex<HashMap<String, Arc<Session>>>);
impl Drop for Previews {
    fn drop(&mut self) {
        if let Ok(sessions) = self.0.lock() {
            for session in sessions.values() { session.cancel.cancel(); }
        }
    }
}
#[derive(serde::Serialize)]
pub struct Preview { id: String, playlist: String }

#[tauri::command]
pub async fn start_preview(app: tauri::AppHandle, state: State<'_, crate::AppState>, room_id: String) -> Result<Preview, String> {
    let recording = state.recorder.active_recordings().await.into_iter()
        .find(|r| r.room_id == room_id).ok_or("该直播间已停止录制")?;
    let settings = state.settings.read().await.clone();
    let ffmpeg = crate::recorder::ffmpeg::find_ffmpeg(Some(&app), Some(&settings.ffmpeg_path))?;
    let id = format!("{:032x}", rand::random::<u128>());
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("live-preview").join(&id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let playlist = dir.join("live.m3u8");
    let mut command = tokio::process::Command::new(ffmpeg);
    command.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-rw_timeout", "15000000"]);
    if let Some(proxy) = settings.proxy_url() { command.args(["-http_proxy", &proxy]); }
    command.arg("-i").arg(recording.stream_url)
        .args(["-map", "0:v:0", "-map", "0:a:0?", "-c:v", "copy", "-c:a", "aac",
            "-f", "hls", "-hls_time", "2", "-hls_list_size", "6", "-hls_flags", "delete_segments+temp_file",
            "-hls_segment_filename"])
        .arg(dir.join("part-%06d.ts")).arg(&playlist)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null()).kill_on_drop(true);
    crate::recorder::ffmpeg::hide_console(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => { let _ = std::fs::remove_dir_all(&dir); return Err(format!("无法启动预览：{err}")); }
    };
    let session = Arc::new(Session { cancel: CancellationToken::new(), touched: Mutex::new(Instant::now()) });
    app.state::<Previews>().0.lock().unwrap().insert(id.clone(), session.clone());
    let worker_app = app.clone();
    let worker_id = id.clone();
    let worker_session = session.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = worker_session.cancel.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    if child.try_wait().ok().flatten().is_some()
                        || worker_session.touched.lock().unwrap().elapsed() > Duration::from_secs(45) { break; }
                }
            }
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
        worker_session.cancel.cancel();
        worker_app.state::<Previews>().0.lock().unwrap().remove(&worker_id);
        let _ = tokio::fs::remove_dir_all(dir).await;
    });
    // Do not hand the player an empty playlist while FFmpeg waits for a keyframe.
    for _ in 0..120 {
        if session.cancel.is_cancelled() { return Err("直播预览失败：直播源已断开或无法读取，请重试".into()); }
        if tokio::fs::read_to_string(&playlist).await.map(|s| s.contains("#EXTINF:")).unwrap_or(false) {
            return Ok(Preview { id, playlist: playlist.to_string_lossy().into_owned() });
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    session.cancel.cancel();
    Err("等待直播画面超时，请稍后重试".into())
}

#[tauri::command]
pub fn touch_preview(state: State<'_, Previews>, id: String) -> Result<(), String> {
    let sessions = state.0.lock().unwrap();
    let session = sessions.get(&id).ok_or("直播预览已结束，请重试")?;
    *session.touched.lock().unwrap() = Instant::now();
    Ok(())
}
#[tauri::command]
pub fn stop_preview(state: State<'_, Previews>, id: String) {
    if let Some(session) = state.0.lock().unwrap().remove(&id) { session.cancel.cancel(); }
}
