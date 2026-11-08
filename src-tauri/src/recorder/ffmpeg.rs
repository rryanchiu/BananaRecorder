// ffmpeg command construction, process guarding and post conversion.
// Mirrors main.py:1157-1204 (command template), 420-491 (guard) and 219-251 (convert).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

/// Resolve the ffmpeg executable: PATH → dev resource → portable layout →
/// installed resource dir.
pub fn find_ffmpeg(app: Option<&tauri::AppHandle>) -> Result<PathBuf, String> {
    // 1. PATH
    if which::exists_checked("ffmpeg") {
        return Ok(PathBuf::from("ffmpeg"));
    }
    // 2. dev: src-tauri/binaries/ffmpeg.exe (resolved at compile time)
    let dev_candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries/ffmpeg.exe");
    if dev_candidate.exists() {
        return Ok(dev_candidate);
    }
    // 3. next to the app executable (portable layout)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("ffmpeg.exe");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    // 4. bundled resource directory
    if let Some(app) = app {
        use tauri::Manager;
        if let Ok(resource_dir) = app.path().resource_dir() {
            let candidate = resource_dir.join("binaries/ffmpeg.exe");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    Err("未找到 ffmpeg。请将 ffmpeg.exe 放在应用目录下，或加入系统 PATH。".into())
}

/// Minimal PATH probe (std has no `which`).
mod which {
    pub fn exists_checked(name: &str) -> bool {
        let path = std::env::var("PATH").unwrap_or_default();
        for dir in path.split(';') {
            if dir.is_empty() {
                continue;
            }
            for ext in ["", ".exe"] {
                let candidate = std::path::Path::new(dir).join(format!("{name}{ext}"));
                if candidate.is_file() {
                    return true;
                }
            }
        }
        false
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
        "-y".into(),
        "-v".into(), "verbose".into(),
        "-rw_timeout".into(), rw_timeout.to_string(),
        "-loglevel".into(), "error".into(),
        "-hide_banner".into(),
        "-user_agent".into(), BASE_UA.into(),
        "-protocol_whitelist".into(),
        "rtmp,crypto,file,http,https,tcp,tls,udp,rtp,httpproxy".into(),
        "-thread_queue_size".into(), "1024".into(),
        "-analyzeduration".into(), analyze.to_string(),
        "-probesize".into(), probe.to_string(),
        "-fflags".into(), "+discardcorrupt".into(),
        "-re".into(),
        "-i".into(), spec.stream_url.into(),
        "-bufsize".into(), bufsize.into(),
        "-sn".into(),
        "-dn".into(),
        "-reconnect_delay_max".into(), "60".into(),
        "-reconnect_streamed".into(),
        "-reconnect_at_eof".into(),
        "-max_muxing_queue_size".into(), mux_queue.into(),
        "-correct_ts_overflow".into(), "1".into(),
        "-avoid_negative_ts".into(), "1".into(),
    ];

    if !spec.extra_headers.is_empty() {
        let headers = format!("{}\r\n", spec.extra_headers);
        args.insert(11, "-headers".into());
        args.insert(12, headers);
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
}

impl RecordingProcess {
    pub fn spawn(ffmpeg: &Path, args: &[String]) -> Result<Self, String> {
        let child = Command::new(ffmpeg)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            // ffmpeg writes progress/error info to stderr; keep it for debugging
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("启动 ffmpeg 失败: {e}"))?;
        Ok(Self {
            child,
            stopped: Arc::new(AtomicBool::new(false)),
        })
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
    let status = Command::new(ffmpeg)
        .args([
            "-y",
            "-i", &source.to_string_lossy(),
            "-c:v", "copy",
            "-c:a", "copy",
            "-f", "mp4", &target.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
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
}
