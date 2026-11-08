// Douyin resolver — signed webcast api path (ab_sign port of spider.py:68)
// with the unsigned HTML parsing path (spider.py:230) as fallback.

use super::ab_sign::ab_sign;
use super::{http_client, pad_quality_list, quality_code, quality_index, RoomState};
use regex::Regex;
use serde_json::{Map, Value};
use url::form_urlencoded;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/115.0";
/// the webcast api path signs the query against THIS user agent (spider.py:73)
const SIGN_UA: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.5845.97 Safari/537.36 Core/1.116.567.400 QQBrowser/19.7.6764.400";

/// Embedded anonymous cookie (the ttwid is the critical part), same spirit as
/// the hardcoded cookie in spider.py. Users can override via settings.
const DEFAULT_COOKIE: &str = "ttwid=1%7CB1qls3GdnZhUov9o2NxOMxxYS2ff6OSvEWbv0ytbES4%7C1680522049%7C280d802d6d478e3e78d0c807f7c487e7ffec0ae4e5fdd6a0fe74c3c6af149511; my_rd=1; store-region=cn-fj; store-region-src=uid; __live_version__=%221.1.1.2169%22; live_can_add_dy_2_desktop=%221%22; webcast_local_quality=sd";

const WEBCAST_TTWID: &str = "ttwid=1%7C2iDIYVmjzMcpZ20fcaFde0VghXAA3NaNXE_SLR68IyE%7C1761045455%7Cab35197d5cfb21df6cbb2fa7ef1c9262206b062c315b9d04da746d0b37dfbc7d";

pub async fn resolve(
    url: &str,
    quality: &str,
    cookie: Option<&str>,
    proxy: Option<String>,
) -> Result<RoomState, String> {
    if url.contains("v.douyin.com") || url.contains("/user/") {
        return Err("暂不支持抖音短链/主页链接，请使用 https://live.douyin.com/房间号 形式的链接".into());
    }

    // primary: signed webcast api; fallback: page html parsing
    match resolve_via_api(url, quality, cookie, proxy.clone()).await {
        Ok(state) => Ok(state),
        Err(api_err) => {
            resolve_via_html(url, quality, cookie, proxy)
                .await
                .map_err(|html_err| format!("抖音签名接口失败({api_err})，页面解析也失败({html_err})"))
        }
    }
}

/// Signed webcast api path (spider.py get_douyin_web_stream_data).
async fn resolve_via_api(
    url: &str,
    quality: &str,
    cookie: Option<&str>,
    proxy: Option<String>,
) -> Result<RoomState, String> {
    let web_rid = url
        .split('?')
        .next()
        .unwrap_or(url)
        .rsplit("live.douyin.com/")
        .next()
        .map(|_| {
            url.split("live.douyin.com/")
                .last()
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default();
    if web_rid.is_empty() {
        return Err("无法解析 web_rid".into());
    }

    let pairs = [
        ("aid", "6383"),
        ("app_name", "douyin_web"),
        ("live_id", "1"),
        ("device_platform", "web"),
        ("language", "zh-CN"),
        ("browser_language", "zh-CN"),
        ("browser_platform", "Win32"),
        ("browser_name", "Chrome"),
        ("browser_version", "116.0.0.0"),
        ("web_rid", web_rid.as_str()),
        ("msToken", ""),
    ];
    let query: String = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs.iter())
        .finish();
    let a_bogus = ab_sign(&query, SIGN_UA);
    let api = format!("https://live.douyin.com/webcast/room/web/enter/?{query}&a_bogus={a_bogus}");

    let client = http_client(proxy)?;
    let mut request = client
        .get(&api)
        .header("User-Agent", SIGN_UA)
        .header("Referer", format!("https://live.douyin.com/{web_rid}"));
    request = request.header(
        "Cookie",
        cookie.unwrap_or(WEBCAST_TTWID),
    );
    let json: Value = request
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("JSON 解析失败: {e}"))?;

    let data = json.get("data").ok_or("接口返回缺少 data（可能触发风控）")?;
    // data.data is null/absent for rooms that are offline (or VR) — the python
    // original treats this as a quiet "not live" result, NOT an error
    // (spider.py:103-104 raise → main.py catches → {'anchor_name': ""} → wait)
    let room_data = data
        .get("data")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .cloned();
    let Some(mut room) = room_data else {
        let nickname = data
            .pointer("/user/nickname")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        return Ok(RoomState::offline(&nickname));
    };
    if let Some(obj) = room.as_object_mut() {
        let nickname = data
            .pointer("/user/nickname")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        obj.insert("anchor_name".into(), Value::String(nickname));
    }

    // merge origin quality from live_core_sdk_data pull_datas (spider.py:116-137)
    merge_sdk_origin(&mut room);

    select_stream(room, quality).await
}

/// Merge ORIGIN urls from live_core_sdk_data.pull_data.stream_data.
fn merge_sdk_origin(room: &mut Value) {
    let stream_json = room
        .pointer("/stream_url/live_core_sdk_data/pull_data/stream_data")
        .and_then(Value::as_str)
        .map(String::from)
        .or_else(|| {
            // alternative location: pull_datas.<key>.stream_data
            let datas = room.pointer("/stream_url/pull_datas")?.as_object()?;
            datas.values().next()?.get("stream_data").and_then(Value::as_str).map(String::from)
        });

    let Some(stream_json) = stream_json else { return };
    let Ok(parsed) = serde_json::from_str::<Value>(&stream_json) else { return };
    let Some(origin) = parsed.pointer("/data/origin/main") else { return };

    let vcodec = origin
        .pointer("/sdk_params/VCodec")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let Some(hls) = origin.get("hls").and_then(Value::as_str) else { return };
    let Some(flv) = origin.get("flv").and_then(Value::as_str) else { return };

    if let Some(obj) = room.pointer_mut("/stream_url/flv_pull_url").and_then(|v| v.as_object_mut()) {
        let mut merged = Map::new();
        merged.insert("ORIGIN".into(), Value::String(format!("{flv}&codec={vcodec}")));
        for (k, v) in obj.iter() {
            merged.insert(k.clone(), v.clone());
        }
        *obj = merged;
    }
    if let Some(obj) = room
        .pointer_mut("/stream_url/hls_pull_url_map")
        .and_then(|v| v.as_object_mut())
    {
        let mut merged = Map::new();
        merged.insert("ORIGIN".into(), Value::String(format!("{hls}&codec={vcodec}")));
        for (k, v) in obj.iter() {
            merged.insert(k.clone(), v.clone());
        }
        *obj = merged;
    }
}

/// Unsigned HTML parsing fallback (spider.py:230).
async fn resolve_via_html(
    url: &str,
    quality: &str,
    cookie: Option<&str>,
    proxy: Option<String>,
) -> Result<RoomState, String> {
    let client = http_client(proxy)?;
    let mut request = client
        .get(url)
        .header("User-Agent", UA)
        .header("Referer", "https://live.douyin.com/")
        .header("Accept-Language", "zh-CN,zh;q=0.8");
    request = request.header(
        "Cookie",
        cookie.unwrap_or(DEFAULT_COOKIE),
    );
    let html = request
        .send()
        .await
        .map_err(|e| format!("抖音页面请求失败: {e}"))?
        .text()
        .await
        .map_err(|e| format!("抖音页面读取失败: {e}"))?;

    let mut room = parse_room_from_html(&html)?;
    let stream_orientation = room
        .pointer("/stream_url/stream_orientation")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    merge_origin(&mut room, &html, stream_orientation);
    select_stream(room, quality).await
}

/// Extract the room json (with anchor_name + stream_url) from the page html.
fn parse_room_from_html(html: &str) -> Result<Value, String> {
    // locate the embedded room state blob
    let state_re = Regex::new(r#"(\{\\"state\\":.*?)]\\n"]\)"#).unwrap();
    let common_re = Regex::new(r#"(\{\\"common\\":.*?)]\\n"]\)</script><div hidden"#).unwrap();
    let json_str = state_re
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .or_else(|| {
            common_re
                .captures(html)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
        })
        .ok_or("抖音页面解析失败（未找到 state 数据）")?;

    let cleaned = json_str.replace('\\', "").replace("u0026", "&");

    let store_re = Regex::new(r#""roomStore":(.*?),"linkmicStore""#).unwrap();
    let store_match = store_re
        .captures(&cleaned)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or("抖音页面解析失败（未找到 roomStore）")?;

    let nick_re = Regex::new(r#""nickname":"(.*?)","avatar_thumb"#).unwrap();
    let anchor_name = nick_re
        .captures(&store_match)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();

    // python: room_store.split(',"has_commerce_goods"')[0] + '}}}' — the
    // '}}}' suffix is appended UNCONDITIONALLY, marker present or not
    let mut room_store = store_match;
    if let Some(pos) = room_store.find(r#","has_commerce_goods""#) {
        room_store = room_store[..pos].to_string();
    }
    room_store.push_str("}}}");


    let parsed: Value =
        serde_json::from_str(&room_store).map_err(|e| format!("抖音 roomStore JSON 解析失败: {e}"))?;
    let mut room = parsed
        .pointer("/roomInfo/room")
        .cloned()
        .ok_or("抖音页面解析失败（未找到 roomInfo.room）")?;
    if let Some(obj) = room.as_object_mut() {
        obj.insert("anchor_name".into(), Value::String(anchor_name));
    }
    Ok(room)
}

/// Port of stream.py get_douyin_stream_url: pick flv/hls url by quality index.
async fn select_stream(room: Value, quality: &str) -> Result<RoomState, String> {
    let anchor_name = room
        .get("anchor_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let status = room.get("status").and_then(Value::as_i64).unwrap_or(4);
    if status != 2 {
        return Ok(RoomState::offline(&anchor_name));
    }

    let stream_url = room
        .get("stream_url")
        .ok_or("抖音返回缺少 stream_url")?
        .clone();

    // quality-ordered lists (ORIGIN first after the origin merge)
    let mut flv_list: Vec<String> = match stream_url.get("flv_pull_url") {
        Some(Value::Object(map)) => map.values().filter_map(|v| v.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    };
    let mut m3u8_list: Vec<String> = match stream_url.get("hls_pull_url_map") {
        Some(Value::Object(map)) => map.values().filter_map(|v| v.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    };
    flv_list = pad_quality_list(flv_list);
    m3u8_list = pad_quality_list(m3u8_list);
    if flv_list.is_empty() {
        return Ok(RoomState::offline(&anchor_name));
    }

    let index = quality_index(quality_code(quality));
    let mut m3u8_url = m3u8_list.get(index).cloned().unwrap_or_default();
    let mut flv_url = flv_list.get(index).cloned().unwrap_or_default();

    // probe the m3u8; if unreachable, try the adjacent quality
    let probe_ok = probe_url(&m3u8_url).await;
    if !probe_ok {
        let alt = if index < 4 { index + 1 } else { index - 1 };
        m3u8_url = m3u8_list.get(alt).cloned().unwrap_or_default();
        flv_url = flv_list.get(alt).cloned().unwrap_or_default();
    }

    Ok(RoomState {
        anchor_name,
        is_live: true,
        title: room.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
        flv_url: Some(flv_url.clone()),
        m3u8_url: Some(if m3u8_url.is_empty() { flv_url } else { m3u8_url }),
        quality: quality.to_string(),
    })
}

async fn probe_url(url: &str) -> bool {
    if url.is_empty() {
        return false;
    }
    match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(6))
        .build()
    {
        Ok(client) => matches!(
            client
                .head(url)
                .header("User-Agent", UA)
                .header("Referer", "https://live.douyin.com/")
                .send()
                .await,
            Ok(resp) if resp.status().is_success()
        ),
        Err(_) => false,
    }
}

/// Merge the "origin" quality urls extracted from the secondary state blob,
/// mirroring the tail of spider.py get_douyin_stream_data. Enriches the room
/// with ORIGIN entries in flv_pull_url / hls_pull_url_map.
fn merge_origin(room: &mut Value, html: &str, stream_orientation: i64) {
    let re = Regex::new(r#"(\{\\"common\\":.*?)"]\)</script><script nonce="#).unwrap();
    let matches: Vec<String> = re
        .captures_iter(html)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();

    let mut origin_main: Option<Value> = None;
    if !matches.is_empty() {
        let pick = if stream_orientation == 1 { 0 } else { 1 };
        if let Some(blob) = matches.get(pick).or_else(|| matches.first()) {
            let cleaned = blob
                .replace('\\', "")
                .replace("\"{", "{")
                .replace("}\"", "}")
                .replace("u0026", "&");
            if let Ok(json) = serde_json::from_str::<Value>(&cleaned) {
                origin_main = json.pointer("/data/origin/main").cloned();
            }
        }
    }
    if origin_main.is_none() {
        let cleaned = html.replace('\\', "").replace("u0026", "&");
        let origin_re = Regex::new(r#"(?s)"origin":\{"main":(.*?),"dash""#).unwrap();
        if let Some(m) = origin_re.captures(&cleaned).and_then(|c| c.get(1)) {
            if let Ok(json) = serde_json::from_str::<Value>(&format!("{}", m.as_str().to_owned() + "}")) {
                origin_main = Some(json);
            }
        }
    }

    let Some(origin) = origin_main else { return };
    let vcodec = origin
        .pointer("/sdk_params/VCodec")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let Some(hls) = origin.get("hls").and_then(Value::as_str) else {
        return;
    };
    let Some(flv) = origin.get("flv").and_then(Value::as_str) else {
        return;
    };

    if let Some(obj) = room.pointer_mut("/stream_url/flv_pull_url").and_then(|v| v.as_object_mut()) {
        let mut merged = Map::new();
        merged.insert("ORIGIN".into(), Value::String(format!("{flv}&codec={vcodec}")));
        for (k, v) in obj.iter() {
            merged.insert(k.clone(), v.clone());
        }
        *obj = merged;
    }
    if let Some(obj) = room.pointer_mut("/stream_url/hls_pull_url_map").and_then(|v| v.as_object_mut()) {
        let mut merged = Map::new();
        merged.insert("ORIGIN".into(), Value::String(format!("{hls}&codec={vcodec}")));
        for (k, v) in obj.iter() {
            merged.insert(k.clone(), v.clone());
        }
        *obj = merged;
    }
}
