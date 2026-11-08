// Recording file naming, mirroring main.py's rules.

use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The character-class from main.py:73 (`rstr`) that gets replaced with "_".
const DIRTY_CHARS: &str = r#"[\\/:*?"<>|&#。，, ~！· ]"#;

fn dirty_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(DIRTY_CHARS).expect("invalid dirty-chars regex"))
}

fn emoji_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // a pragmatic subset of emoji ranges, good enough for filenames
    RE.get_or_init(|| {
        Regex::new(
            r"[\u{1F300}-\u{1FAFF}\u{2600}-\u{27BF}\u{FE00}-\u{FE0F}\u{1F000}-\u{1F02F}\u{2B00}-\u{2BFF}\u{200D}]",
        )
        .expect("invalid emoji regex")
    })
}

/// Equivalent of main.py `clean_name`:
/// full-width parens -> half width, dirty chars -> "_", strip "_",
/// optionally remove emoji, empty result becomes 空白昵称.
pub fn clean_name(input: &str, clean_emoji: bool) -> String {
    let mut text = input.replace('（', "(").replace('）', ")");
    if clean_emoji {
        text = emoji_re().replace_all(&text, "").to_string();
    }
    let cleaned = dirty_re().replace_all(&text, "_").to_string();
    let trimmed = cleaned.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "空白昵称".to_string()
    } else {
        trimmed
    }
}

/// Build `{base}/{平台}/[{作者}/][{日期}/]` and base filename without extension.
pub fn build_output(
    base: &Path,
    platform_name: &str,
    anchor: &str,
    folder_by_author: bool,
    folder_by_time: bool,
) -> (PathBuf, String) {
    let mut dir = base.join(platform_name);
    if folder_by_author && !anchor.is_empty() {
        dir = dir.join(anchor);
    }
    if folder_by_time {
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        dir = dir.join(date);
    }
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let filename = format!("{anchor}_{stamp}");
    (dir, filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_dirty_characters() {
        assert_eq!(clean_name("小明: 直播/回放?", true), "小明__直播_回放");
        assert_eq!(clean_name("__张三__", true), "张三");
        assert_eq!(clean_name("（六）歌曲·串烧", true), "(六)歌曲_串烧");
        assert_eq!(clean_name("", true), "空白昵称");
        assert_eq!(clean_name("主播😍", true), "主播");
    }
}
