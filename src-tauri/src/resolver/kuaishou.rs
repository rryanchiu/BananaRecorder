// Kuaishou resolver — html __INITIAL_STATE__ parsing.
// Ports src/spider.py get_kuaishou_stream_data + src/stream.py get_kuaishou_stream_url.

use super::{http_client, RoomState};
use regex::Regex;
use serde_json::Value;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/115.0";

pub async fn resolve(
    url: &str,
    quality: &str,
    cookie: Option<&str>,
    proxy: Option<String>,
) -> Result<RoomState, String> {
    let client = http_client(proxy)?;
    let mut request = client.get(url).header("User-Agent", UA);
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    let html = request
        .send()
        .await
        .map_err(|e| format!("快手页面请求失败: {e}"))?
        .text()
        .await
        .map_err(|e| format!("快手页面读取失败: {e}"))?;

    let state_re = Regex::new(
        r#"<script>window\.__INITIAL_STATE__=(.*?);\(function\(\)\{var s;"#,
    )
    .unwrap();
    let state_json = state_re
        .captures(&html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .ok_or("快手页面解析失败（未找到 __INITIAL_STATE__）")?;

    // extract the liveStream fragment: ({"liveStream"...}),"gameInfo
    let play_re = Regex::new(r#"(\{"liveStream".*?),"gameInfo"#).unwrap();
    let play_json = play_re
        .captures(state_json)
        .and_then(|c| c.get(1))
        .map(|m| format!("{}", m.as_str().to_owned()) + "}")
        .ok_or("快手页面解析失败（未找到 liveStream）")?;

    let play_list: Value = serde_json::from_str(&play_json)
        .map_err(|e| format!("快手 JSON 解析失败: {e}"))?;

    let anchor_name = play_list
        .pointer("/author/name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    if play_list.get("errorType").is_some() || play_list.get("liveStream").is_none() {
        return Ok(RoomState::offline(&anchor_name));
    }

    let live_stream = play_list.get("liveStream").ok_or("快手返回缺少 liveStream")?;
    let play_urls = live_stream.get("playUrls");
    let Some(play_urls) = play_urls else {
        return Ok(RoomState::offline(&anchor_name));
    };

    // representation list under playUrls.h264.adaptationSet
    let representations = play_urls
        .get("h264")
        .and_then(|h| h.pointer("/adaptationSet/representation"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if representations.is_empty() {
        return Ok(RoomState::offline(&anchor_name));
    }

    // sort by bitrate desc, pick by the bitrate ladder (stream.py quality_mapping_bit)
    let mut reps = representations;
    reps.sort_by(|a, b| {
        let ba = a.get("bitrate").and_then(Value::as_i64).unwrap_or(0);
        let bb = b.get("bitrate").and_then(Value::as_i64).unwrap_or(0);
        bb.cmp(&ba)
    });

    let code = super::quality_code(quality);
    let max_bitrate: i64 = match code {
        "BD" => 4000,
        "UHD" => 2000,
        "HD" => 1000,
        "SD" => 800,
        "LD" => 600,
        _ => 99_999, // OD
    };
    let index = reps
        .iter()
        .position(|r| r.get("bitrate").and_then(Value::as_i64).unwrap_or(0) <= max_bitrate)
        .unwrap_or(reps.len().saturating_sub(1));

    let flv_url = reps[index]
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if flv_url.is_empty() {
        return Ok(RoomState::offline(&anchor_name));
    }

    Ok(RoomState {
        anchor_name,
        is_live: true,
        title: live_stream
            .get("caption")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        flv_url: Some(flv_url.clone()),
        m3u8_url: Some(flv_url),
        quality: quality.to_string(),
    })
}
