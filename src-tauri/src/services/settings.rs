//! 设置读写与校验。
//!
//! 存储走 `db::store`（settings kv 表），这里只做输入清洗：
//! 去空白、去空串、按序去重，保证写入的数据始终干净。

use crate::models::settings::Settings;

/// 清洗设置：scanDirs 去空去重（保持原有顺序）。
pub fn sanitize(mut settings: Settings) -> Settings {
    let mut seen = std::collections::HashSet::new();
    settings.scan_dirs.retain(|d| {
        let trimmed = d.trim();
        !trimmed.is_empty() && seen.insert(trimmed.to_string())
    });
    for d in &mut settings.scan_dirs {
        *d = d.trim().to_string();
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::settings::Theme;

    #[test]
    fn sanitize_dedupes_and_trims() {
        let s = Settings {
            theme: Theme::Dark,
            scan_dirs: vec![
                " D:/Music ".into(),
                "".into(),
                "   ".into(),
                "D:/Music".into(),
                "E:/Video".into(),
            ],
            ..Default::default()
        };
        let out = sanitize(s);
        assert_eq!(out.scan_dirs, vec!["D:/Music", "E:/Video"]);
    }
}
