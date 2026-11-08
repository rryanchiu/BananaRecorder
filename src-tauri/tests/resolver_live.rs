// Live network tests — run explicitly with: cargo test -- --ignored
// These hit real platform APIs and are skipped in normal CI runs.

use liverecorder_lib::resolver;

#[tokio::test]
#[ignore]
async fn bilibili_resolve_live_room() {
    // 哔哩哔哩官方直播间，常态开播
    let state = resolver::resolve(
        resolver::Platform::Bilibili,
        "https://live.bilibili.com/6",
        "原画",
        None,
        None,
    )
    .await
    .expect("bilibili resolve failed");
    println!(
        "bilibili: anchor={} live={} url={:?}",
        state.anchor_name,
        state.is_live,
        state.flv_url.as_deref().map(|u| &u[..60.min(u.len())])
    );
    assert!(!state.anchor_name.is_empty(), "anchor name should resolve");
    if state.is_live {
        assert!(state.flv_url.is_some(), "live room should have a stream url");
    }
}

#[tokio::test]
#[ignore]
async fn huya_resolve_live_room() {
    // 虎牙 LOL 官方赛事直播间（常年开播）
    let state = resolver::resolve(
        resolver::Platform::Huya,
        "https://www.huya.com/660000",
        "原画",
        None,
        None,
    )
    .await;
    // huya room may legitimately be offline; only assert no signing crash
    match state {
        Ok(state) => println!(
            "huya: anchor={} live={}",
            state.anchor_name, state.is_live
        ),
        Err(err) => println!("huya resolve error (may be room-specific): {err}"),
    }
}

#[tokio::test]
#[ignore]
async fn kuaishou_resolve_room() {
    let state = resolver::resolve(
        resolver::Platform::Kuaishou,
        "https://live.kuaishou.com/u/normalDisturb",
        "原画",
        None,
        None,
    )
    .await;
    match state {
        Ok(state) => println!(
            "kuaishou: anchor={} live={}",
            state.anchor_name, state.is_live
        ),
        Err(err) => println!("kuaishou resolve error (may be room-specific): {err}"),
    }
}

#[tokio::test]
#[ignore]
async fn douyin_resolve_room() {
    let state = resolver::resolve(
        resolver::Platform::Douyin,
        "https://live.douyin.com/272979857435",
        "原画",
        None,
        None,
    )
    .await
    .expect("douyin resolve should not error");
    println!(
        "douyin: anchor={} live={} title={} flv={}",
        state.anchor_name,
        state.is_live,
        state.title,
        state.flv_url.as_deref().map(|u| &u[..60.min(u.len())]).unwrap_or("-")
    );
}
