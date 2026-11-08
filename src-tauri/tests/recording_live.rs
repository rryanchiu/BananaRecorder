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
    let ffmpeg = find_ffmpeg(None).expect("ffmpeg not found");

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
