// Huya resolver — mobile app api + pure Rust wsSecret signing.
// Ports src/spider.py get_huya_app_stream_url + src/stream.py:210-264 get_anti_code.

use super::{http_client, last_path_segment, quality_code, RoomState};
use base64::Engine;
use md5::{Digest, Md5};
use serde_json::Value;
use url::form_urlencoded;

const MOBILE_UA: &str = "ios/7.830 (ios 17.0; ; iPhone 15 (A2846/A3089/A3090/A3092))";

fn md5_hex(input: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

fn query_value<'a>(pairs: &'a [(String, String)], key: &str) -> Result<&'a str, String> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| format!("虎牙 anti_code 缺少 {key} 参数"))
}

/// Port of stream.py get_anti_code — mobile sdk signing.
fn get_anti_code(old_anti_code: &str, stream_name: &str) -> Result<String, String> {
    let params_t: u64 = 100;
    let sdk_version: u64 = 2403051612;

    let t13 = chrono::Utc::now().timestamp_millis() as u64;
    let sdk_sid = t13;

    let rand_part: u64 = (rand::random::<f64>() * 1000.0) as u64;
    let init_uuid = ((t13 % 10_000_000_000) * 1000 + rand_part) % 4_294_967_295;
    let uid: u64 = rand::random::<u64>() % 1_000_000 + 1_400_000_000_000;
    let seq_id = uid + sdk_sid;

    let target_unix_time = (t13 + 110_624) / 1000;
    let ws_time = format!("{target_unix_time:x}");

    let pairs = parse_query(old_anti_code);
    let fm_raw = query_value(&pairs, "fm")?;
    let ctype = query_value(&pairs, "ctype")?.to_string();
    let fs = query_value(&pairs, "fs")?.to_string();

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(fm_raw)
        .map_err(|e| format!("虎牙 fm 参数 base64 解码失败: {e}"))?;
    let decoded_str = String::from_utf8_lossy(&decoded).to_string();
    let ws_secret_pf = decoded_str
        .split('_')
        .next()
        .ok_or("虎牙 fm 参数解析失败")?
        .to_string();

    let ws_secret_hash = md5_hex(&format!("{seq_id}|{ctype}|{params_t}"));
    let ws_secret = format!("{ws_secret_pf}_{uid}_{stream_name}_{ws_secret_hash}_{ws_time}");
    let ws_secret_md5 = md5_hex(&ws_secret);

    Ok(format!(
        "wsSecret={ws_secret_md5}&wsTime={ws_time}&seqid={seq_id}&ctype={ctype}&ver=1&fs={fs}&uuid={init_uuid}&u={uid}&t={params_t}&sv={sdk_version}&sdk_sid={sdk_sid}&codec=264"
    ))
}

pub async fn resolve(
    url: &str,
    quality: &str,
    cookie: Option<&str>,
    proxy: Option<String>,
) -> Result<RoomState, String> {
    let client = http_client(proxy)?;
    let mut room_id = last_path_segment(url);

    // numeric room ids go straight to the api; alphanumeric nicknames need a
    // page scrape first to find the numeric room id
    if room_id.chars().any(|c| c.is_alphabetic()) {
        let mut request = client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:123.0) Gecko/20100101 Firefox/123.0");
        if let Some(cookie) = cookie {
            request = request.header("Cookie", cookie);
        }
        let html = request.send().await.map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;
        let re = regex::Regex::new(r#""ProfileRoom":(\d+),"sPrivateHost"#).unwrap();
        room_id = re
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or("虎牙房间号解析失败，请使用 https://www.huya.com/+房间号 形式的链接")?;
    }

    let api = format!(
        "https://mp.huya.com/cache.php?m=Live&do=profileRoom&roomid={room_id}&showSecret=1"
    );
    let json: Value = client
        .get(&api)
        .header("User-Agent", MOBILE_UA)
        .header("Referer", "https://servicewechat.com/wx74767bf0b684f7d3/301/page-frame.html")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let data = json.get("data").ok_or("虎牙接口返回缺少 data")?;
    let anchor_name = data
        .pointer("/profileInfo/nick")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let live_status = data
        .get("realLiveStatus")
        .and_then(Value::as_str)
        .unwrap_or("");
    let title = data
        .pointer("/liveData/introduction")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    if live_status != "ON" {
        return Ok(RoomState::offline(&anchor_name));
    }

    let stream_list = data
        .pointer("/stream/baseSteamInfoList")
        .and_then(Value::as_array)
        .ok_or("虎牙接口返回缺少 baseSteamInfoList")?;
    let first = stream_list
        .first()
        .ok_or("虎牙返回的流列表为空")?
        .clone();

    let stream_name = first
        .get("sStreamName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let flv_base = first.get("sFlvUrl").and_then(Value::as_str).unwrap_or("");
    let flv_suffix = first.get("sFlvUrlSuffix").and_then(Value::as_str).unwrap_or("flv");
    let hls_base = first.get("sHlsUrl").and_then(Value::as_str).unwrap_or("");
    let hls_suffix = first.get("sHlsUrlSuffix").and_then(Value::as_str).unwrap_or("m3u8");
    let flv_anti_code = first
        .get("sFlvAntiCode")
        .and_then(Value::as_str)
        .unwrap_or("");

    if stream_name.is_empty() || flv_base.is_empty() {
        return Ok(RoomState::offline(&anchor_name));
    }

    let new_anti_code = get_anti_code(flv_anti_code, &stream_name)?;
    let mut flv_url = format!("{flv_base}/{stream_name}.{flv_suffix}?{new_anti_code}&ratio=");
    let mut m3u8_url = format!("{hls_base}/{stream_name}.{hls_suffix}?{new_anti_code}&ratio=");

    // optional higher qualities from the exsphd parameter (stream.py quality block)
    let code = quality_code(quality);
    if code != "OD" && code != "BD" {
        if let Some((_, exsphd)) = flv_anti_code.split_once("&exsphd=") {
            let re = regex::Regex::new(r"(?<=264_)\d+").unwrap();
            let mut rates: Vec<&str> =
                re.find_iter(exsphd).map(|m| m.as_str()).collect();
            rates.reverse();
            while rates.len() < 5 {
                let last = *rates.last().unwrap_or(&"2000");
                rates.push(last);
            }
            // UHD=1 HD=2 SD=3 LD=4 in the reversed list (index 0 reserved for OD)
            let ladder = super::quality_index(code);
            let index = ladder.min(rates.len().saturating_sub(1));
            flv_url += rates[index];
            m3u8_url += rates[index];
        }
    }

    Ok(RoomState {
        anchor_name,
        is_live: true,
        title,
        flv_url: Some(flv_url),
        m3u8_url: Some(m3u8_url),
        quality: quality.to_string(),
    })
}
