// ffmpeg command construction, process guarding and post conversion.
// Mirrors main.py:1157-1204 (command template), 420-491 (guard) and 219-251 (convert).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

/// Bundled ffmpeg binary name: Windows ships `ffmpeg.exe`, Unices use `ffmpeg`.
pub const FFMPEG_BIN: &str = if cfg!(windows) {
    "ffmpeg.exe"
} else {
    "ffmpeg"
};

/// Where a resolved ffmpeg binary came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfmpegSource {
    Builtin,
    Custom,
    Installed,
    Path,
}

impl FfmpegSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Custom => "custom",
            Self::Installed => "installed",
            Self::Path => "path",
        }
    }
}

/// Directory for an in-app installed ffmpeg: app data, no admin rights needed.
pub fn install_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("ffmpeg"))
        .map_err(|e| e.to_string())
}

/// The ffmpeg shipped with the app (dev `binaries/` or the bundled resource).
pub fn builtin_ffmpeg(app: Option<&tauri::AppHandle>) -> Option<PathBuf> {
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(FFMPEG_BIN);
    if dev.is_file() {
        return Some(dev);
    }
    let app = app?;
    use tauri::Manager;
    let candidate = app
        .path()
        .resource_dir()
        .ok()?
        .join("binaries")
        .join(FFMPEG_BIN);
    candidate.is_file().then_some(candidate)
}

/// An ffmpeg previously installed by the in-app installer.
pub fn managed_ffmpeg(app: &tauri::AppHandle) -> Option<PathBuf> {
    let candidate = install_dir(app).ok()?.join(FFMPEG_BIN);
    candidate.is_file().then_some(candidate)
}

/// Resolve ffmpeg: explicit path → bundled → in-app install → system PATH.
pub fn resolve_ffmpeg(
    app: Option<&tauri::AppHandle>,
    custom: Option<&str>,
) -> Result<(PathBuf, FfmpegSource), String> {
    if let Some(raw) = custom.map(str::trim).filter(|s| !s.is_empty()) {
        let candidate = PathBuf::from(raw);
        if candidate.is_file() {
            return Ok((candidate, FfmpegSource::Custom));
        }
    }
    if let Some(path) = builtin_ffmpeg(app) {
        return Ok((path, FfmpegSource::Builtin));
    }
    if let Some(app) = app {
        if let Some(path) = managed_ffmpeg(app) {
            return Ok((path, FfmpegSource::Installed));
        }
    }
    if which::exists_checked(FFMPEG_BIN) {
        return Ok((PathBuf::from(FFMPEG_BIN), FfmpegSource::Path));
    }
    Err(format!(
        "未找到 ffmpeg。请在设置中安装 FFmpeg，或指定 {FFMPEG_BIN} 的路径。"
    ))
}

/// Convenience wrapper when only the executable path is needed.
pub fn find_ffmpeg(
    app: Option<&tauri::AppHandle>,
    custom: Option<&str>,
) -> Result<PathBuf, String> {
    resolve_ffmpeg(app, custom).map(|(path, _)| path)
}

/// First line of `ffmpeg -version`, shown in the settings page.
pub async fn ffmpeg_version(path: &Path) -> Option<String> {
    let mut cmd = Command::new(path);
    cmd.arg("-version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    hide_console(&mut cmd);
    let output = cmd.output().await.ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
}

pub fn install_supported() -> bool {
    cfg!(any(target_os = "windows", target_os = "macos"))
}

#[cfg(target_os = "windows")]
fn install_url() -> Result<&'static str, String> {
    // BtbN publishes static LGPL builds (no GPL-only codecs). Pin to the 8.1
    // release branch instead of the rolling master build: master churn broke
    // recordings once already (strict input/output option validation).
    Ok(if cfg!(target_arch = "aarch64") {
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n8.1-latest-winarm64-lgpl-8.1.zip"
    } else {
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n8.1-latest-win64-lgpl-8.1.zip"
    })
}

#[cfg(target_os = "macos")]
fn install_url() -> Result<&'static str, String> {
    Ok(if cfg!(target_arch = "aarch64") {
        "https://ffmpeg.martin-riedl.de/redirect/latest/macos/arm64/release/ffmpeg.zip"
    } else {
        "https://ffmpeg.martin-riedl.de/redirect/latest/macos/amd64/release/ffmpeg.zip"
    })
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn install_url() -> Result<&'static str, String> {
    Err("当前平台不支持自动安装 FFmpeg，请使用系统包管理器安装或在设置中指定路径".into())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallProgress {
    pub stage: &'static str,
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Download a static ffmpeg build and install it under the app data directory.
pub async fn install_ffmpeg(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let url = install_url()?;
    let dir = install_dir(app)?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;

    let archive = dir.join("ffmpeg-download.zip");
    let target = dir.join(FFMPEG_BIN);

    let client = reqwest::Client::builder()
        .user_agent("BananaRecorder")
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载 FFmpeg 失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载 FFmpeg 失败: {e}"))?;
    let total = resp.content_length();

    let mut file = tokio::fs::File::create(&archive)
        .await
        .map_err(|e| e.to_string())?;
    let mut downloaded = 0u64;
    let mut last_emit = 0u64;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("下载 FFmpeg 失败: {e}"))?
    {
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if downloaded - last_emit >= 512 * 1024 || Some(downloaded) == total {
            last_emit = downloaded;
            let _ = app.emit(
                "ffmpeg://install-progress",
                InstallProgress {
                    stage: "downloading",
                    downloaded,
                    total,
                },
            );
        }
    }
    file.flush().await.ok();
    drop(file);

    let _ = app.emit(
        "ffmpeg://install-progress",
        InstallProgress {
            stage: "extracting",
            downloaded,
            total,
        },
    );

    let archive_path = archive.clone();
    let target_path = target.clone();
    let extracted =
        tauri::async_runtime::spawn_blocking(move || extract_ffmpeg(&archive_path, &target_path))
            .await
            .map_err(|e| format!("解压 FFmpeg 失败: {e}"))?;
    let _ = tokio::fs::remove_file(&archive).await;
    extracted?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
    }

    let _ = app.emit(
        "ffmpeg://install-progress",
        InstallProgress {
            stage: "done",
            downloaded,
            total,
        },
    );
    Ok(target)
}

/// Extract the first entry named exactly `ffmpeg` / `ffmpeg.exe` from a zip.
/// Entry paths are ignored, so a malicious archive cannot escape `target`.
fn extract_ffmpeg(archive: &Path, target: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("安装包格式错误: {e}"))?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        if !entry.is_file() {
            continue;
        }
        let name = entry.name().rsplit('/').next().unwrap_or_default();
        if name != FFMPEG_BIN {
            continue;
        }
        let temp = target.with_file_name(format!(".{FFMPEG_BIN}.part"));
        {
            let mut out = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&temp, target).map_err(|e| e.to_string())?;
        return Ok(());
    }
    Err(format!("安装包中未找到 {FFMPEG_BIN}"))
}

/// Minimal PATH probe (std has no `which`). Uses the platform path separator
/// so it works on Unix too (`;` only splits PATH on Windows).
mod which {
    pub fn exists_checked(name: &str) -> bool {
        let Some(path) = std::env::var_os("PATH") else {
            return false;
        };
        std::env::split_paths(&path).any(|dir| dir.join(name).is_file())
    }
}

pub struct RecordSpec<'a> {
    pub stream_url: &'a str,
    pub output_path: &'a Path,
    pub format: &'a str, // ts mkv flv mp4
    pub split_enabled: bool,
    pub split_seconds: u64,
    pub proxy: Option<&'a str>,
    pub extra_headers: &'a str, // "key:value\r\nkey:value"
    pub overseas: bool,
}

const BASE_UA: &str = "Mozilla/5.0 (Linux; Android 11; SAMSUNG SM-G973U) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/87.0.4280.141 Mobile Safari/537.36";

/// The app is a GUI-subsystem binary: without this flag every ffmpeg child
/// allocates its own visible console window on Windows.
#[cfg(target_os = "windows")]
pub fn hide_console(cmd: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
pub fn hide_console(_cmd: &mut Command) {}

/// Build the ffmpeg argument list (mirrors main.py build record command).
pub fn build_args(spec: &RecordSpec) -> Vec<String> {
    let (rw_timeout, analyze, probe, bufsize, mux_queue) = if spec.overseas {
        (50_000_000u64, 40_000_000u64, 20_000_000u64, "15000k", "2048")
    } else {
        (15_000_000, 20_000_000, 10_000_000, "8000k", "1024")
    };

    let ext = spec.format;
    let output: Vec<String> = match (ext, spec.split_enabled) {
        // ts
        ("ts", false) => vec![
            "-c:v".into(), "copy".into(),
            "-c:a".into(), "copy".into(),
            "-map".into(), "0".into(),
            "-f".into(), "mpegts".into(),
        ],
        ("ts", true) => vec![
            "-c:v".into(), "copy".into(),
            "-c:a".into(), "copy".into(),
            "-map".into(), "0".into(),
            "-f".into(), "segment".into(),
            "-segment_time".into(), spec.split_seconds.to_string(),
            "-segment_format".into(), "mpegts".into(),
            "-reset_timestamps".into(), "1".into(),
        ],
        // flv
        ("flv", false) => vec![
            "-map".into(), "0".into(),
            "-c:v".into(), "copy".into(),
            "-c:a".into(), "copy".into(),
            "-bsf:a".into(), "aac_adtstoasc".into(),
            "-f".into(), "flv".into(),
        ],
        ("flv", true) => vec![
            "-c:v".into(), "copy".into(),
            "-c:a".into(), "copy".into(),
            "-map".into(), "0".into(),
            "-f".into(), "segment".into(),
            "-segment_time".into(), spec.split_seconds.to_string(),
            "-segment_format".into(), "flv".into(),
            "-reset_timestamps".into(), "1".into(),
        ],
        // mkv
        ("mkv", false) => vec![
            "-flags".into(), "global_header".into(),
            "-map".into(), "0".into(),
            "-c:v".into(), "copy".into(),
            "-c:a".into(), "copy".into(),
            "-f".into(), "matroska".into(),
        ],
        ("mkv", true) => vec![
            "-flags".into(), "global_header".into(),
            "-c:v".into(), "copy".into(),
            "-c:a".into(), "aac".into(),
            "-map".into(), "0".into(),
            "-f".into(), "segment".into(),
            "-segment_time".into(), spec.split_seconds.to_string(),
            "-segment_format".into(), "matroska".into(),
            "-reset_timestamps".into(), "1".into(),
        ],
        // mp4 (segmented mp4 needs re-encoded audio + fragmentation)
        ("mp4", false) => vec![
            "-map".into(), "0".into(),
            "-c:v".into(), "copy".into(),
            "-c:a".into(), "copy".into(),
            "-f".into(), "mp4".into(),
        ],
        ("mp4", true) => vec![
            "-c:v".into(), "copy".into(),
            "-c:a".into(), "aac".into(),
            "-map".into(), "0".into(),
            "-f".into(), "segment".into(),
            "-segment_time".into(), spec.split_seconds.to_string(),
            "-segment_format".into(), "mp4".into(),
            "-reset_timestamps".into(), "1".into(),
            "-movflags".into(), "+frag_keyframe+empty_moov".into(),
        ],
        _ => vec!["-f".into(), format!("{ext}").into()],
    };

    let mut args: Vec<String> = vec![
        // ---- input options (ffmpeg >= 8 rejects input options given after -i) ----
        "-y".into(),
        "-v".into(), "verbose".into(),
        "-rw_timeout".into(), rw_timeout.to_string(),
        "-loglevel".into(), "error".into(),
        "-hide_banner".into(),
        "-user_agent".into(), BASE_UA.into(),
        "-protocol_whitelist".into(),
        "rtmp,crypto,file,http,https,tcp,tls,udp,rtp,httpproxy".into(),
        "-analyzeduration".into(), analyze.to_string(),
        "-probesize".into(), probe.to_string(),
        "-fflags".into(), "+discardcorrupt".into(),
        // http reconnection belongs to the input protocol (older ffmpeg silently
        // dropped these when given on the output side, so reconnection never worked).
        // explicit values: the CLI cannot infer protocol bool flags on its own
        "-reconnect_delay_max".into(), "60".into(),
        "-reconnect_streamed".into(), "1".into(),
        "-reconnect_at_eof".into(), "1".into(),
        "-correct_ts_overflow".into(), "1".into(),
        "-re".into(),
        "-i".into(), spec.stream_url.into(),
        // ---- output options ----
        "-thread_queue_size".into(), "1024".into(),
        "-bufsize".into(), bufsize.into(),
        "-sn".into(),
        "-dn".into(),
        "-max_muxing_queue_size".into(), mux_queue.into(),
        "-avoid_negative_ts".into(), "1".into(),
    ];

    if !spec.extra_headers.is_empty() {
        let headers = format!("{}\r\n", spec.extra_headers);
        // anchor on the protocol whitelist so the option order above can shift freely
        let pos = args
            .iter()
            .position(|a| a == "-protocol_whitelist")
            .unwrap_or(args.len());
        args.insert(pos, "-headers".into());
        args.insert(pos + 1, headers);
    }
    if let Some(proxy) = spec.proxy {
        args.insert(1, "-http_proxy".into());
        args.insert(2, proxy.into());
    }

    args.extend(output);

    // output file (segment template appends _%03d)
    if spec.split_enabled {
        let mut name = spec.output_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        name.push_str("_%03d.");
        name.push_str(ext);
        args.push(spec.output_path.with_file_name(name).to_string_lossy().to_string());
    } else {
        args.push(spec.output_path.to_string_lossy().to_string());
    }
    args
}

/// A running ffmpeg recording with cooperative graceful stop.
pub struct RecordingProcess {
    child: Child,
    stopped: Arc<AtomicBool>,
    stderr_tail: Arc<std::sync::Mutex<String>>,
}

/// Size cap for the captured stderr tail, so verbose ffmpeg output cannot grow
/// the buffer unboundedly over a long recording.
const STDERR_TAIL_BYTES: usize = 8 * 1024;

impl RecordingProcess {
    pub fn spawn(ffmpeg: &Path, args: &[String]) -> Result<Self, String> {
        let mut cmd = Command::new(ffmpeg);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            // ffmpeg writes progress/error info to stderr; keep it for debugging
            .stderr(std::process::Stdio::piped());
        hide_console(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("启动 ffmpeg 失败: {e}"))?;

        // drain stderr in the background: unread pipe data eventually blocks
        // the child, and the tail is the only way to see why a recording died
        let stderr_tail = Arc::new(std::sync::Mutex::new(String::new()));
        if let Some(mut stderr) = child.stderr.take() {
            let tail = Arc::clone(&stderr_tail);
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut chunk = [0u8; 4096];
                loop {
                    match stderr.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut buf = tail.lock().expect("stderr tail lock");
                            buf.push_str(&String::from_utf8_lossy(&chunk[..n]));
                            let excess = buf.len().saturating_sub(STDERR_TAIL_BYTES);
                            if excess > 0 {
                                let drop_to = (excess + 512).min(buf.len());
                                buf.drain(..drop_to);
                            }
                        }
                    }
                }
            });
        }

        Ok(Self {
            child,
            stopped: Arc::new(AtomicBool::new(false)),
            stderr_tail,
        })
    }

    /// The last few KB of ffmpeg stderr, for error reporting when a
    /// recording exits abnormally.
    pub fn stderr_tail(&self) -> String {
        self.stderr_tail.lock().expect("stderr tail lock").clone()
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// Non-blocking exit check: returns Some(exit_code) once ffmpeg exited.
    pub fn try_wait(&mut self) -> Option<i32> {
        self.child.try_wait().ok().flatten().and_then(|s| s.code())
    }

    /// Graceful stop: send "q" to stdin so ffmpeg finalizes the container.
    pub async fn stop_gracefully(&mut self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(stdin) = self.child.stdin.as_mut() {
            let _ = stdin.write_all(b"q").await;
            let _ = stdin.flush().await;
        }
        // drop stdin to unblock ffmpeg even if it ignores "q"
        self.child.stdin = None;
    }

    pub async fn wait(&mut self) -> Option<i32> {
        self.child.wait().await.ok().and_then(|s| s.code())
    }

}

/// Convert a ts/mkv/flv file to mp4 without re-encoding (main.py converts_mp4).
pub async fn convert_to_mp4(
    ffmpeg: &Path,
    source: &Path,
    delete_origin: bool,
) -> Result<PathBuf, String> {
    let target = source.with_extension("mp4");
    let mut cmd = Command::new(ffmpeg);
    cmd.args([
        "-y",
        "-i", &source.to_string_lossy(),
        "-c:v", "copy",
        "-c:a", "copy",
        "-f", "mp4", &target.to_string_lossy(),
    ])
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
    hide_console(&mut cmd);
    let status = cmd
        .status()
        .await
        .map_err(|e| format!("启动 ffmpeg 转码失败: {e}"))?;

    if !status.success() {
        return Err(format!("转码失败: {}", source.display()));
    }
    if delete_origin {
        let _ = std::fs::remove_file(source);
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_non_split_command_snapshot() {
        let spec = RecordSpec {
            stream_url: "https://example.com/live.m3u8",
            output_path: Path::new("D:/videos/anchor_2025-01-01_10-00-00.ts"),
            format: "ts",
            split_enabled: false,
            split_seconds: 1800,
            proxy: None,
            extra_headers: "",
            overseas: false,
        };
        let args = build_args(&spec);
        assert_eq!(args.last().unwrap(), "D:/videos/anchor_2025-01-01_10-00-00.ts");
        assert!(args.windows(2).any(|w| w[0] == "-f" && w[1] == "mpegts"));
        assert!(args.contains(&"-reconnect_streamed".to_string()));
    }

    #[test]
    fn ts_split_command_uses_segment_template() {
        let spec = RecordSpec {
            stream_url: "https://example.com/live.flv",
            output_path: Path::new("D:/videos/anchor.ts"),
            format: "ts",
            split_enabled: true,
            split_seconds: 1800,
            proxy: None,
            extra_headers: "",
            overseas: false,
        };
        let args = build_args(&spec);
        assert!(args.contains(&"1800".to_string()));
        assert!(args.last().unwrap().contains("_%03d.ts"));
    }

    #[test]
    fn proxy_is_inserted_right_after_y() {
        let spec = RecordSpec {
            stream_url: "https://example.com/live.flv",
            output_path: Path::new("D:/videos/a.ts"),
            format: "ts",
            split_enabled: false,
            split_seconds: 0,
            proxy: Some("http://127.0.0.1:7890"),
            extra_headers: "",
            overseas: false,
        };
        let args = build_args(&spec);
        assert_eq!(&args[1], "-http_proxy");
        assert_eq!(&args[2], "http://127.0.0.1:7890");
    }

    #[test]
    fn extract_ffmpeg_ignores_entry_paths() {
        let dir = std::env::temp_dir().join(format!(
            "banana-ffmpeg-extract-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let archive = dir.join("build.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            writer
                .start_file(format!("ffmpeg-build/bin/{FFMPEG_BIN}"), options)
                .unwrap();
            std::io::Write::write_all(&mut writer, b"fake-ffmpeg").unwrap();
            writer.finish().unwrap();
        }

        let target = dir.join(FFMPEG_BIN);
        extract_ffmpeg(&archive, &target).expect("extract");
        assert_eq!(std::fs::read(&target).unwrap(), b"fake-ffmpeg");

        // a zip without an ffmpeg entry is rejected
        let bad = dir.join("bad.zip");
        {
            let file = std::fs::File::create(&bad).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("readme.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut writer, b"nope").unwrap();
            writer.finish().unwrap();
        }
        assert!(extract_ffmpeg(&bad, &dir.join("out")).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
