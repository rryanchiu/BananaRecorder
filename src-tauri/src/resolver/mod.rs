// Live stream source resolution. Each platform implements one function with
// the same contract: given a room url, produce RoomState.

pub mod ab_sign;
pub mod bilibili;
pub mod douyin;
pub mod huya;
pub mod kuaishou;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Platform {
    Douyin,
    Bilibili,
    Huya,
    Kuaishou,
}

impl Platform {
    pub fn detect(url: &str) -> Option<Platform> {
        if url.contains("douyin.com") {
            Some(Platform::Douyin)
        } else if url.contains("bilibili.com") {
            Some(Platform::Bilibili)
        } else if url.contains("huya.com") {
            Some(Platform::Huya)
        } else if url.contains("kuaishou.com") {
            Some(Platform::Kuaishou)
        } else {
            None
        }
    }

    /// Chinese display name, used for folder grouping (mirrors main.py).
    pub fn display_name(&self) -> &'static str {
        match self {
            Platform::Douyin => "抖音直播",
            Platform::Bilibili => "哔哩哔哩直播",
            Platform::Huya => "虎牙直播",
            Platform::Kuaishou => "快手直播",
        }
    }

    /// Lower-case code, used for settings lookups.
    pub fn key(&self) -> &'static str {
        match self {
            Platform::Douyin => "douyin",
            Platform::Bilibili => "bilibili",
            Platform::Huya => "huya",
            Platform::Kuaishou => "kuaishou",
        }
    }
}

/// 中文画质 -> 平台无关的代码（main.py get_quality_code）。
pub fn quality_code(quality: &str) -> &'static str {
    match quality {
        "蓝光" => "BD",
        "超清" => "UHD",
        "高清" => "HD",
        "标清" => "SD",
        "流畅" => "LD",
        _ => "OD", // 原画
    }
}

/// 代码 -> 通用清晰度索引（stream.py QUALITY_MAPPING: OD/BD=0, UHD=1, HD=2, SD=3, LD=4）。
pub fn quality_index(code: &str) -> usize {
    match code {
        "BD" => 0,
        "UHD" => 1,
        "HD" => 2,
        "SD" => 3,
        "LD" => 4,
        _ => 0,
    }
}

/// Pad a quality-ordered url list so indexing never goes out of bounds
/// (stream.py pads by repeating the last element).
pub fn pad_quality_list(mut list: Vec<String>) -> Vec<String> {
    if list.is_empty() {
        return list;
    }
    let last = list.last().cloned().unwrap();
    while list.len() < 5 {
        list.push(last.clone());
    }
    list
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomState {
    pub anchor_name: String,
    pub is_live: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub flv_url: Option<String>,
    #[serde(default)]
    pub m3u8_url: Option<String>,
    #[serde(default)]
    pub quality: String,
}

impl RoomState {
    pub fn offline(anchor_name: &str) -> Self {
        Self {
            anchor_name: anchor_name.to_string(),
            is_live: false,
            title: String::new(),
            flv_url: None,
            m3u8_url: None,
            quality: String::new(),
        }
    }
}

/// Resolve a room url into its current state.
pub async fn resolve(
    platform: Platform,
    url: &str,
    quality: &str,
    cookie: Option<&str>,
    proxy: Option<String>,
) -> Result<RoomState, String> {
    match platform {
        Platform::Douyin => douyin::resolve(url, quality, cookie, proxy).await,
        Platform::Bilibili => bilibili::resolve(url, quality, cookie, proxy).await,
        Platform::Huya => huya::resolve(url, quality, cookie, proxy).await,
        Platform::Kuaishou => kuaishou::resolve(url, quality, cookie, proxy).await,
    }
}

pub fn http_client(proxy: Option<String>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:127.0) Gecko/20100101 Firefox/127.0");
    if let Some(proxy) = proxy {
        builder = builder.proxy(reqwest::Proxy::all(&proxy).map_err(|e| e.to_string())?);
    }
    builder.build().map_err(|e| e.to_string())
}

/// Extract a room id from the last url path segment, mirroring the
/// `url.split('?')[0].rsplit('/', 1)[1]` pattern used all over main.py.
pub fn last_path_segment(url: &str) -> String {
    let without_query = url.split('?').next().unwrap_or(url);
    let trimmed = without_query.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or("").to_string()
}
