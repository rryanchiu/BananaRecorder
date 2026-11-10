// Douyin resolver — signed webcast api path (ab_sign port of spider.py:68)
// with the unsigned HTML parsing path (spider.py:230) as fallback, plus
// profile-homepage / short-link support via the amemv reflow api
// (spider.py get_douyin_app_stream_data + room.py get_sec_user_id).

use super::ab_sign::ab_sign;
use super::{http_client, pad_quality_list, quality_code, quality_index, RoomState};
use regex::Regex;
use serde_json::{Map, Value};
use url::form_urlencoded;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/115.0";
/// the webcast api path signs the query against THIS user agent (spider.py:73)
const SIGN_UA: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.5845.97 Safari/537.36 Core/1.116.567.400 QQBrowser/19.7.6764.400";
/// the reflow api signs against THIS user agent (spider.py:147)
const APP_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0";
/// mobile UA used to follow profile/short-link redirects (room.py HEADERS)
const MOBILE_UA: &str = "Mozilla/5.0 (Linux; Android 11; SAMSUNG SM-G973U) AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/14.2 Chrome/87.0.4280.141 Mobile Safari/537.36";

/// Embedded anonymous cookie (the ttwid is the critical part), same spirit as
/// the hardcoded cookie in spider.py. Users can override via settings.
const DEFAULT_COOKIE: &str = "ttwid=1%7CB1qls3GdnZhUov9o2NxOMxxYS2ff6OSvEWbv0ytbES4%7C1680522049%7C280d802d6d478e3e78d0c807f7c487e7ffec0ae4e5fdd6a0fe74c3c6af149511; my_rd=1; store-region=cn-fj; store-region-src=uid; __live_version__=%221.1.1.2169%22; live_can_add_dy_2_desktop=%221%22; webcast_local_quality=sd";

const WEBCAST_TTWID: &str = "ttwid=1%7C2iDIYVmjzMcpZ20fcaFde0VghXAA3NaNXE_SLR68IyE%7C1761045455%7Cab35197d5cfb21df6cbb2fa7ef1c9262206b062c315b9d04da746d0b37dfbc7d";

/// redirect-following cookie (room.py:29)
const REDIRECT_COOKIE: &str = "s_v_web_id=verify_lk07kv74_QZYCUApD_xhiB_405x_Ax51_GYO9bUIyZQVf";
/// iesdouyin share-page cookie (room.py:90)
const IES_TTWID: &str = "ttwid=1%7C4ejCkU2bKY76IySQENJwvGhg1IQZrgGEupSyTKKfuyk%7C1740470403%7Cbc9ad2ee341f1a162f9e27f4641778030d1ae91e31f9df6553a8f2efa3bdb7b4; __ac_nonce=0683e59f3009cc48fbab0; __ac_signature=_02B4Z6wo00f01mG6waQAAIDB9JUCzFb6.TZhmsUAAPBf34; __ac_referer=__ac_blank";

/// fixed verifyFp used by the reflow api (spider.py:158)
const VERIFY_FP: &str = "verify_hwj52020_7szNlAB7_pxNY_48Vh_ALKF_GA1Uf3yteoOY";

pub async fn resolve(
    url: &str,
    quality: &str,
    cookie: Option<&str>,
    proxy: Option<String>,
) -> Result<RoomState, String> {
    // follow-page links (www.douyin.com/follow/live/{web_rid}?anchor_id=...)
    // carry the plain room number — same enter api as live.douyin.com rooms
    if url.contains("follow/live/") {
        let web_rid = url
            .split("follow/live/")
            .last()
            .unwrap_or("")
            .split('?')
            .next()
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        if web_rid.is_empty() || !web_rid.chars().all(|c| c.is_ascii_digit()) {
            return Err("无法从关注页链接中解析直播间房间号".into());
        }
        return resolve_web_rid(&web_rid, quality, cookie, proxy).await;
    }

    if url.contains("live.douyin.com") {
        // live room url: signed webcast api, html page parsing as fallback
        let web_rid = url
            .split("live.douyin.com/")
            .last()
            .unwrap_or("")
            .split('?')
            .next()
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        if web_rid.is_empty() {
            return Err("无法解析抖音房间号".into());
        }
        match resolve_web_rid(&web_rid, quality, cookie, proxy.clone()).await {
            Ok(state) => Ok(state),
            Err(api_err) => {
                resolve_via_html(url, quality, cookie, proxy)
                    .await
                    .map_err(|html_err| {
                        format!("抖音签名接口失败({api_err})，页面解析也失败({html_err})")
                    })
            }
        }
    } else {
        // profile home page (www.douyin.com/user/xxx) or v.douyin.com short link
        resolve_profile(url, quality, cookie, proxy).await
    }
}

// ---------------------------------------------------------------------------
// profile home page / short link → live room (spider.py:144-226, room.py)
// ---------------------------------------------------------------------------

/// Follow redirects on a profile/short link; when douyin bounces us to the
/// amemv reflow endpoint we get room_id + sec_user_id and query the reflow
/// api for the room. Otherwise fall back to the douyin-number route.
async fn resolve_profile(
    url: &str,
    quality: &str,
    cookie: Option<&str>,
    proxy: Option<String>,
) -> Result<RoomState, String> {
    let client = http_client(proxy.clone())?;
    let resp = client
        .get(url)
        .header("User-Agent", MOBILE_UA)
        .header("Accept-Language", "zh-CN,zh;q=0.8")
        .header("Cookie", REDIRECT_COOKIE)
        .send()
        .await
        .map_err(|e| format!("抖音主页请求失败: {e}"))?;
    let final_url = resp.url().to_string();
    drop(resp);

    if final_url.contains("reflow/") {
        // redirect carries room_id + sec_user_id → reflow api
        let sec_re = Regex::new(r#"sec_user_id=([\w-]+)(&|#|$)"#).unwrap();
        let sec_user_id = sec_re
            .captures(&final_url)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or("重定向链接中未找到 sec_user_id")?;
        let room_id = final_url
            .split('?')
            .next()
            .unwrap_or("")
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        if room_id.is_empty() {
            return Err("重定向链接中未找到 room_id".into());
        }
        return fetch_room_by_reflow(&room_id, &sec_user_id, quality, cookie, proxy).await;
    }

    // no reflow redirect: profile page → douyin number → live room route
    // (room.py get_unique_id)
    let sec_user_id = final_url
        .split('?')
        .next()
        .unwrap_or("")
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    if sec_user_id.is_empty() {
        return Err("无法从链接中解析抖音用户".into());
    }

    let share_client = http_client(proxy.clone())?;
    let share_html = share_client
        .get(format!("https://www.iesdouyin.com/share/user/{sec_user_id}"))
        .header("User-Agent", MOBILE_UA)
        .header("Cookie", IES_TTWID)
        .send()
        .await
        .map_err(|e| format!("抖音分享页请求失败: {e}"))?
        .text()
        .await
        .map_err(|e| format!("抖音分享页读取失败: {e}"))?;

    let room_key =
        profile_room_key(&share_html).ok_or("未找到抖音号或 short_id，该主页可能无法转为直播间链接")?;

    // python fallback (spider.py:192-193): douyin number → live page html parse
    let room_url = format!("https://live.douyin.com/{room_key}");
    resolve_via_html(&room_url, quality, cookie, proxy).await
}

/// Live-room route key from a profile share page: the custom 抖音号 (`unique_id`)
/// when set, otherwise the numeric `short_id`. Both work as
/// `live.douyin.com/<key>`; accounts without a custom ID have an empty
/// `unique_id`, which used to make profile links unusable.
fn profile_room_key(share_html: &str) -> Option<String> {
    let uid_re = Regex::new(r#"unique_id":"(.*?)","verification_type"#).unwrap();
    let unique_id = uid_re
        .captures_iter(share_html)
        .last()
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|id| !id.is_empty());
    if unique_id.is_some() {
        return unique_id;
    }

    let short_re = Regex::new(r#""short_id":"(.*?)""#).unwrap();
    short_re
        .captures(share_html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|id| !id.is_empty())
}

/// amemv reflow api for profile-resolved rooms (spider.py get_app_data).
async fn fetch_room_by_reflow(
    room_id: &str,
    sec_user_id: &str,
    quality: &str,
    cookie: Option<&str>,
    proxy: Option<String>,
) -> Result<RoomState, String> {
    let pairs = [
        ("verifyFp", VERIFY_FP),
        ("type_id", "0"),
        ("live_id", "1"),
        ("room_id", room_id),
        ("sec_user_id", sec_user_id),
        ("version_code", "99.99.99"),
        ("app_id", "1128"),
    ];
    let query: String = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs.iter())
        .finish();
    let a_bogus = ab_sign(&query, APP_UA);
    let api = format!(
        "https://webcast.amemv.com/webcast/room/reflow/info/?{query}&a_bogus={a_bogus}"
    );

    let client = http_client(proxy)?;
    let mut request = client
        .get(&api)
        .header("User-Agent", APP_UA)
        .header("Referer", "https://live.douyin.com/")
        .header("Accept-Language", "zh-CN,zh;q=0.8");
    request = request.header("Cookie", cookie.unwrap_or(DEFAULT_COOKIE));
    let json: Value = request
        .send()
        .await
        .map_err(|e| format!("reflow 接口请求失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("reflow 接口 JSON 解析失败: {e}"))?;

    let data = json.get("data").ok_or("reflow 接口返回缺少 data（可能触发风控）")?;
    // data.room is absent for offline rooms — quiet "not live", not an error
    let Some(mut room) = data.get("room").cloned() else {
        return Ok(RoomState::offline(""));
    };
    let nickname = room
        .pointer("/owner/nickname")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if let Some(obj) = room.as_object_mut() {
        obj.insert("anchor_name".into(), Value::String(nickname));
    }

    merge_sdk_origin(&mut room);
    select_stream(room, quality).await
}

// ---------------------------------------------------------------------------
// live room url → signed webcast api (spider.py get_douyin_web_stream_data)
// ---------------------------------------------------------------------------

async fn resolve_web_rid(
    web_rid: &str,
    quality: &str,
    cookie: Option<&str>,
    proxy: Option<String>,
) -> Result<RoomState, String> {
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
        ("web_rid", web_rid),
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
    let nickname = data
        .pointer("/user/nickname")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if let Some(obj) = room.as_object_mut() {
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
    match parse_room_from_ssr(html) {
        Ok(room) => Ok(room),
        Err(ssr_err) => parse_room_from_legacy_blob(html)
            .map_err(|legacy_err| format!("{ssr_err}；{legacy_err}")),
    }
}

/// Douyin now ships its state in a Next.js flight payload:
/// `self.__pace_f.push([1,"d:[...]"])`, with JSON stored inside a JS string.
/// The room lives at `state.roomStore.roomInfo.room`, and the nickname at
/// `state.roomStore.roomInfo.anchor.nickname`.
fn parse_room_from_ssr(html: &str) -> Result<Value, String> {
    const MARKER: &str = "self.__pace_f.push([1,\"";
    let mut cursor = 0usize;
    while let Some(offset) = html[cursor..].find(MARKER) {
        let payload_start = cursor + offset + MARKER.len();
        let Some(end_offset) = html[payload_start..].find("\"])</script>") else {
            break;
        };
        let payload_end = payload_start + end_offset;
        cursor = payload_end + 1;
        let raw = &html[payload_start..payload_end];
        if !raw.contains("\\\"state\\\"") {
            continue;
        }
        let Some(decoded) = decode_js_string(raw) else {
            continue;
        };
        let json = decoded.strip_prefix("d:").unwrap_or(&decoded);
        let Ok(value) = serde_json::from_str::<Value>(json) else {
            continue;
        };
        let Some(room_store) = find_json_key(&value, "roomStore") else {
            continue;
        };
        let Some(room_info) = room_store.get("roomInfo") else {
            continue;
        };
        let Some(mut room) = room_info.get("room").cloned() else {
            continue;
        };
        let anchor_name = room_info
            .pointer("/anchor/nickname")
            .and_then(Value::as_str)
            .or_else(|| room.pointer("/owner/nickname").and_then(Value::as_str))
            .unwrap_or_default()
            .to_string();
        if let Some(obj) = room.as_object_mut() {
            obj.insert("anchor_name".into(), Value::String(anchor_name));
        }
        return Ok(room);
    }
    Err("抖音页面解析失败（未找到 SSR state 数据）".into())
}

/// JSON string escapes are a subset of JS string escapes, so wrapping the
/// payload in quotes lets serde_json decode it (including surrogate pairs).
fn decode_js_string(raw: &str) -> Option<String> {
    serde_json::from_str::<String>(&format!("\"{raw}\"")).ok()
}

fn find_json_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|v| find_json_key(v, key))),
        Value::Array(items) => items.iter().find_map(|v| find_json_key(v, key)),
        _ => None,
    }
}

/// Older page format: an escaped `{"state":...}` blob inside a script tag.
fn parse_room_from_legacy_blob(html: &str) -> Result<Value, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_next_flight_state() {
        let payload = serde_json::json!([
            "$",
            "$L12",
            null,
            {
                "state": {
                    "roomStore": {
                        "roomInfo": {
                            "anchor": { "nickname": "测试主播" },
                            "room": {
                                "status": 4,
                                "title": "标题",
                                "owner": { "nickname": "备用名" }
                            }
                        }
                    }
                }
            }
        ]);
        let json = format!("d:{payload}");
        let escaped = json.replace('\\', "\\\\").replace('"', "\\\"");
        let html = format!(
            "<script nonce=\"x\">self.__pace_f.push([1,\"{escaped}\\n\"])</script>"
        );

        let room = parse_room_from_html(&html).expect("parse ssr state");
        assert_eq!(
            room.get("anchor_name").and_then(Value::as_str),
            Some("测试主播")
        );
        assert_eq!(room.get("status").and_then(Value::as_i64), Some(4));
        assert_eq!(room.get("title").and_then(Value::as_str), Some("标题"));
    }

    #[test]
    fn profile_key_falls_back_to_short_id() {
        let no_custom =
            r#""user_info":{"short_id":"1293092199","unique_id":"","verification_type":1}"#;
        assert_eq!(
            profile_room_key(no_custom).as_deref(),
            Some("1293092199")
        );

        let custom = r#""unique_id":"abc_123","verification_type":1,"short_id":"999""#;
        assert_eq!(profile_room_key(custom).as_deref(), Some("abc_123"));
    }
}
