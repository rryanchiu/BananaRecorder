// Bilibili resolver — official open APIs, no signing required.
// Ports src/spider.py:677-780 (room info + play url).

use super::{http_client, last_path_segment, quality_code, RoomState};
use serde_json::Value;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:127.0) Gecko/20100101 Firefox/127.0";

/// qn values: 30000=杜比 20000=4K 10000=原画 400=蓝光 250=超清 150=高清 80=流畅
fn qn_number(code: &str) -> u32 {
    match code {
        "BD" => 400,
        "UHD" => 250,
        "HD" => 150,
        "SD" => 80,
        "LD" => 80,
        _ => 10000,
    }
}

pub async fn resolve(
    url: &str,
    quality: &str,
    cookie: Option<&str>,
    proxy: Option<String>,
) -> Result<RoomState, String> {
    let client = http_client(proxy)?;
    let room_id = last_path_segment(url);
    if room_id.is_empty() || !room_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("无法从链接中解析B站房间号: {url}"));
    }

    let qn = qn_number(quality_code(quality));

    let mut request = client
        .get(format!(
            "https://api.live.bilibili.com/xlive/web-room/v2/index/getRoomPlayInfo?room_id={room_id}&protocol=0,1&format=0,1,2&codec=0,1,2&qn={qn}&platform=web&ptype=8&dolby=5&panorama=1&hdr_type=0,1"
        ))
        .header("User-Agent", UA)
        .header("Origin", "https://live.bilibili.com")
        .header("Referer", format!("https://live.bilibili.com/{room_id}"));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    let json: Value = request.send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let data = json.get("data").ok_or("B站接口返回缺少 data")?;
    // NOTE: the v2 getRoomPlayInfo response lays these fields flat on `data`
    // (no room_info sub-object); short room ids resolve to the real room_id.
    let uid = data.get("uid").and_then(Value::as_i64).unwrap_or(0);
    let real_room_id = data.get("room_id").and_then(Value::as_i64).unwrap_or(0);
    let live_status = data
        .get("live_status")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    // room title from the plain room info api
    let title = match client
        .get(format!(
            "https://api.live.bilibili.com/room/v1/Room/get_info?room_id={real_room_id}"
        ))
        .header("User-Agent", UA)
        .send()
        .await
    {
        Ok(resp) => resp
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v.pointer("/data/title").and_then(Value::as_str).map(String::from))
            .unwrap_or_default(),
        Err(_) => String::new(),
    };

    // anchor name from the master info api
    let mut anchor_name = String::new();
    let master: Value = client
        .get(format!(
            "https://api.live.bilibili.com/live_user/v1/Master/info?uid={uid}"
        ))
        .header("User-Agent", UA)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(name) = master.pointer("/data/info/uname").and_then(Value::as_str) {
        anchor_name = name.to_string();
    }

    if live_status == 0 {
        return Ok(RoomState::offline(&anchor_name));
    }

    // pick the stream url: codec list sorted by current_qn desc, then index by
    // the quality ladder used in main.py ({10000:0, 400:1, 250:2, 150:3, 80:4})
    let ladder = match qn {
        400 => 1,
        250 => 2,
        150 => 3,
        80 => 4,
        _ => 0,
    };
    let mut record_url: Option<String> = None;
    if let Some(streams) = data
        .pointer("/playurl_info/playurl/stream")
        .and_then(Value::as_array)
    {
        for stream in streams {
            if let Some(codecs) = stream
                .pointer("/format/0/codec")
                .and_then(Value::as_array)
            {
                let mut sorted: Vec<&Value> = codecs.iter().collect();
                sorted.sort_by(|a, b| {
                    let qa = a.get("current_qn").and_then(Value::as_i64).unwrap_or(0);
                    let qb = b.get("current_qn").and_then(Value::as_i64).unwrap_or(0);
                    qb.cmp(&qa)
                });
                let index = ladder.min(sorted.len().saturating_sub(1));
                if let Some(codec) = sorted.get(index) {
                    let base_url = codec.get("base_url").and_then(Value::as_str).unwrap_or("");
                    if let Some(url_info) = codec.get("url_info").and_then(Value::as_array) {
                        if let Some(info0) = url_info.first() {
                            let host = info0.get("host").and_then(Value::as_str).unwrap_or("");
                            let extra = info0.get("extra").and_then(Value::as_str).unwrap_or("");
                            if !base_url.is_empty() && !host.is_empty() {
                                record_url = Some(format!("{host}{base_url}{extra}"));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(play_url) = record_url {
        Ok(RoomState {
            anchor_name,
            is_live: true,
            title,
            flv_url: Some(play_url.clone()),
            m3u8_url: Some(play_url),
            quality: quality.to_string(),
        })
    } else {
        Ok(RoomState::offline(&anchor_name))
    }
}
