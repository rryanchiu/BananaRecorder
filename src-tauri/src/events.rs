// In-memory log ring buffer + event emission helpers.

use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

pub struct LogBuffer {
    lines: Mutex<VecDeque<String>>,
    cap: usize,
}

impl LogBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            lines: Mutex::new(VecDeque::with_capacity(cap)),
            cap,
        }
    }

    pub fn push(&self, level: &str, message: &str) {
        let timestamp = chrono::Local::now().format("%H:%M:%S");
        let mut buf = self.lines.lock().unwrap();
        if buf.len() >= self.cap {
            buf.pop_front();
        }
        buf.push_back(format!("[{timestamp}] [{level}] {message}"));
    }

    pub fn tail(&self, count: usize) -> Vec<String> {
        let buf = self.lines.lock().unwrap();
        let start = buf.len().saturating_sub(count);
        buf.iter().skip(start).cloned().collect()
    }
}

/// Log to the ring buffer and emit a live event to the frontend.
pub fn app_log(app: &AppHandle, buffer: &LogBuffer, level: &str, message: &str) {
    buffer.push(level, message);
    let _ = app.emit(
        "recorder://log",
        serde_json::json!({ "level": level, "message": message }),
    );
}
