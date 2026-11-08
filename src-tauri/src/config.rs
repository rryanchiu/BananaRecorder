// Application settings persisted as JSON.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub save_path: String,
    pub save_format: String, // ts | mkv | flv | mp4
    pub quality: String,     // 原画 蓝光 超清 高清 标清 流畅
    pub delay_seconds: u64,
    pub split_enabled: bool,
    pub split_seconds: u64,
    pub convert_to_mp4: bool,
    pub delete_origin: bool,
    pub folder_by_author: bool,
    pub folder_by_time: bool,
    pub douyin_cookie: String,
    pub bilibili_cookie: String,
    pub proxy_addr: String,
    pub proxy_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            save_path: String::new(),
            save_format: "ts".into(),
            quality: "原画".into(),
            delay_seconds: 120,
            split_enabled: true,
            split_seconds: 1800,
            convert_to_mp4: true,
            delete_origin: true,
            folder_by_author: true,
            folder_by_time: false,
            douyin_cookie: String::new(),
            bilibili_cookie: String::new(),
            proxy_addr: String::new(),
            proxy_enabled: false,
        }
    }
}

impl AppSettings {
    pub fn save_dir(&self, fallback: &Path) -> PathBuf {
        if self.save_path.trim().is_empty() {
            fallback.join("downloads")
        } else {
            PathBuf::from(self.save_path.trim())
        }
    }

    /// Normalized proxy url (adds http:// when the scheme is missing).
    pub fn proxy_url(&self) -> Option<String> {
        if !self.proxy_enabled || self.proxy_addr.trim().is_empty() {
            return None;
        }
        let addr = self.proxy_addr.trim();
        if addr.contains("://") {
            Some(addr.to_string())
        } else {
            Some(format!("http://{addr}"))
        }
    }

    pub fn cookie_for(&self, host_contains: &str) -> Option<&str> {
        let cookie = if host_contains.contains("douyin") {
            &self.douyin_cookie
        } else if host_contains.contains("bilibili") {
            &self.bilibili_cookie
        } else {
            return None;
        };
        if cookie.trim().is_empty() {
            None
        } else {
            Some(cookie.trim())
        }
    }
}

pub fn load_settings(file: &Path) -> AppSettings {
    match std::fs::read(file) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save_settings(file: &Path, settings: &AppSettings) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(file, json).map_err(|e| e.to_string())
}
