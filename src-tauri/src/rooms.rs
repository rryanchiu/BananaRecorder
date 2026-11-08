// Room registry persisted as JSON.

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: String,
    pub enabled: bool,
    pub quality: String,
    pub url: String,
    pub name: String,
}

pub fn room_id(url: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(url.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    digest[..12].to_string()
}

pub fn normalize_url(raw: &str) -> Result<String, String> {
    let url = raw.trim();
    if url.is_empty() {
        return Err("直播间链接不能为空".into());
    }
    let normalized = if url.contains("://") {
        url.to_string()
    } else {
        format!("https://{url}")
    };
    // very light validation: must contain a dot in the host part
    let after_scheme = normalized.split("://").nth(1).unwrap_or("");
    let host = after_scheme.split('/').next().unwrap_or("");
    if !host.contains('.') {
        return Err(format!("无法识别的直播间链接: {normalized}"));
    }
    Ok(normalized)
}

pub fn load_rooms(file: &Path) -> Vec<Room> {
    match std::fs::read(file) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn save_rooms(file: &Path, rooms: &[Room]) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(rooms).map_err(|e| e.to_string())?;
    std::fs::write(file, json).map_err(|e| e.to_string())
}
