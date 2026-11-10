// Full recording pipeline test against a real live stream:
// resolve → spawn ffmpeg → graceful stop → mp4 conversion.
// Run explicitly with: cargo test --test recording_live -- --ignored --nocapture

use liverecorder_lib::recorder::ffmpeg::{
    build_args, convert_to_mp4, find_ffmpeg, RecordSpec, RecordingProcess,
};
use liverecorder_lib::resolver;
use std::path::Path;

#[tokio::test]
#[ignore]
async fn record_bilibili_for_8_seconds() {
    let state = resolver::resolve(
        resolver::Platform::Bilibili,
        "https://live.bilibili.com/6",
        "原画",
        None,
        None,
    )
    .await
    .expect("resolve failed");
    assert!(state.is_live, "room 6 should be live for this test");
    let stream_url = state.flv_url.clone().or(state.m3u8_url).expect("no stream url");

    let dir = std::env::temp_dir().join("liverecorder-rec-test");
    std::fs::create_dir_all(&dir).unwrap();
    let output = dir.join("rec-test.ts");

    let spec = RecordSpec {
        stream_url: &stream_url,
        output_path: &output,
        format: "ts",
        split_enabled: false,
        split_seconds: 0,
        proxy: None,
        extra_headers: "",
        overseas: false,
    };
    let args = build_args(&spec);
    let ffmpeg = find_ffmpeg(None, None).expect("ffmpeg not found");

    let mut process = RecordingProcess::spawn(&ffmpeg, &args).expect("spawn failed");
    println!("recording 8s with ffmpeg pid={:?}", process.pid());
    tokio::time::sleep(std::time::Duration::from_secs(8)).await;

    // graceful stop like the app does (stdin "q")
    process.stop_gracefully().await;
    let code = process.wait().await;
    println!("ffmpeg exited with {code:?}");

    let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    println!("recorded file: {} ({} bytes)", output.display(), size);
    assert!(size > 512 * 1024, "expected >512KB of video, got {size} bytes");

    // ts → mp4 conversion
    let mp4 = convert_to_mp4(&ffmpeg, Path::new(&output), true)
        .await
        .expect("convert failed");
    let mp4_size = std::fs::metadata(&mp4).map(|m| m.len()).unwrap_or(0);
    println!("converted: {} ({} bytes)", mp4.display(), mp4_size);
    assert!(mp4_size > 512 * 1024, "expected >512KB mp4");

    let _ = std::fs::remove_file(&mp4);
}

// Diagnose douyin recordings failing in ~1s with exit code Some(-22) on the
// CI-bundled ffmpeg master build: resolve the real room, run the exact app
// args against both the local and the installed ffmpeg, print exit + stderr.
// Run: cargo test --test recording_live diagnose_douyin_exit_22 -- --ignored --nocapture
use std::process::Stdio;

async fn run_ffmpeg_capped(
    ffmpeg: &Path,
    args: &[String],
    max_secs: u64,
) -> (Option<i32>, bool, String) {
    let mut cmd = tokio::process::Command::new(ffmpeg);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    liverecorder_lib::recorder::ffmpeg::hide_console(&mut cmd);
    let mut child = cmd.spawn().expect("spawn failed");
    let mut stderr_pipe = child.stderr.take().expect("stderr");
    let reader = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf).await;
        String::from_utf8_lossy(&buf).into_owned()
    });

    let exited_on_own = match tokio::time::timeout(
        std::time::Duration::from_secs(max_secs),
        child.wait(),
    )
    .await
    {
        Ok(_status) => true,
        Err(_) => {
            // still running after max_secs: recording works; stop gracefully
            if let Some(stdin) = child.stdin.as_mut() {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(b"q").await;
                let _ = stdin.flush().await;
            }
            child.stdin = None;
            match tokio::time::timeout(std::time::Duration::from_secs(8), child.wait()).await {
                Ok(_) => false,
                Err(_) => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    false
                }
            }
        }
    };
    let code = if exited_on_own {
        child.try_wait().ok().flatten().and_then(|s| s.code())
    } else {
        None
    };
    let stderr = reader.await.unwrap_or_default();
    (code, exited_on_own, stderr)
}

fn make_spec<'a>(stream_url: &'a str, out: &'a Path) -> RecordSpec<'a> {
    RecordSpec {
        stream_url,
        output_path: out,
        format: "mp4",
        split_enabled: true,
        split_seconds: 1800,
        proxy: None,
        extra_headers: "",
        overseas: false,
    }
}

fn print_result(label: &str, code: Option<i32>, ran_to_cap: bool, stderr: &str) {
    println!("=== {label} ===");
    println!("exited_on_own={ran_to_cap} exit_code={code:?}");
    let start = stderr.len().saturating_sub(2500);
    println!("--- stderr tail ---\n{}", &stderr[start..]);
}

#[tokio::test]
#[ignore]
async fn diagnose_douyin_exit_22() {
    let state = resolver::resolve(
        resolver::Platform::Douyin,
        "https://live.douyin.com/272979857435",
        "原画",
        None,
        None,
    )
    .await
    .expect("resolve failed");
    println!("is_live={} anchor={:?}", state.is_live, state.anchor_name);
    let stream_url = state.flv_url.clone().or(state.m3u8_url).expect("no stream url");
    println!("stream_url={stream_url}");

    let dir = std::env::temp_dir().join("liverecorder-douyin-diag");
    std::fs::create_dir_all(&dir).unwrap();


    // 1. local dev ffmpeg (resolver picks binaries/ffmpeg.exe or PATH)
    let local = find_ffmpeg(None, None).expect("no local ffmpeg");
    println!("local ffmpeg: {}", local.display());
    let out1 = dir.join("diag-local.mp4");
    let args1 = build_args(&make_spec(&stream_url, &out1));
    let (code, ran, stderr) = run_ffmpeg_capped(&local, &args1, 15).await;
    print_result("local ffmpeg", code, !ran, &stderr);

    // 2. the ffmpeg bundled with the installed v1.0.0 (CI master build)
    let installed = Path::new("C:/Users/Ryan/AppData/Local/BananaRecorder/binaries/ffmpeg.exe");
    if installed.is_file() {
        println!("installed ffmpeg: {}", installed.display());
        let out2 = dir.join("diag-installed.mp4");
        let args2 = build_args(&make_spec(&stream_url, &out2));
        let (code, ran, stderr) = run_ffmpeg_capped(installed, &args2, 15).await;
        print_result("installed (CI master) ffmpeg", code, !ran, &stderr);
    }
}
