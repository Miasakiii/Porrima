//! Track、MediaFormat 等媒体库条目模型。
//!
//! 字段与序列化形状严格对齐 docs/ipc-contract.md 的 `Track` 定义：
//! camelCase 字段名、`format` 为小写字符串、时间为 ISO 8601。

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 媒体格式。序列化为契约规定的小写字符串（`"flac"` / `"mp3"` / ... / `"other"`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaFormat {
    Flac,
    Mp3,
    M4a,
    Aac,
    Ogg,
    Opus,
    Wav,
    Aiff,
    Ape,
    Wv,
    Wma,
    Dsf,
    Dff,
    Other,
}

impl MediaFormat {
    /// 契约/数据库存储用的小写字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaFormat::Flac => "flac",
            MediaFormat::Mp3 => "mp3",
            MediaFormat::M4a => "m4a",
            MediaFormat::Aac => "aac",
            MediaFormat::Ogg => "ogg",
            MediaFormat::Opus => "opus",
            MediaFormat::Wav => "wav",
            MediaFormat::Aiff => "aiff",
            MediaFormat::Ape => "ape",
            MediaFormat::Wv => "wv",
            MediaFormat::Wma => "wma",
            MediaFormat::Dsf => "dsf",
            MediaFormat::Dff => "dff",
            MediaFormat::Other => "other",
        }
    }

    /// 从 `as_str()` 的字符串还原，未知值归为 `Other`。
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "flac" => MediaFormat::Flac,
            "mp3" => MediaFormat::Mp3,
            "m4a" => MediaFormat::M4a,
            "aac" => MediaFormat::Aac,
            "ogg" => MediaFormat::Ogg,
            "opus" => MediaFormat::Opus,
            "wav" => MediaFormat::Wav,
            "aiff" => MediaFormat::Aiff,
            "ape" => MediaFormat::Ape,
            "wv" => MediaFormat::Wv,
            "wma" => MediaFormat::Wma,
            "dsf" => MediaFormat::Dsf,
            "dff" => MediaFormat::Dff,
            _ => MediaFormat::Other,
        }
    }

    /// 从文件扩展名推断格式（大小写不敏感）。不支持的扩展名返回 `None`。
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "flac" => Some(MediaFormat::Flac),
            "mp3" => Some(MediaFormat::Mp3),
            "m4a" => Some(MediaFormat::M4a),
            "aac" => Some(MediaFormat::Aac),
            "ogg" | "oga" => Some(MediaFormat::Ogg),
            "opus" => Some(MediaFormat::Opus),
            "wav" | "wave" => Some(MediaFormat::Wav),
            "aiff" | "aif" => Some(MediaFormat::Aiff),
            "ape" => Some(MediaFormat::Ape),
            "wv" => Some(MediaFormat::Wv),
            "wma" => Some(MediaFormat::Wma),
            "dsf" => Some(MediaFormat::Dsf),
            "dff" => Some(MediaFormat::Dff),
            _ => None,
        }
    }

    /// 从文件路径推断格式；无扩展名或不支持时返回 `None`。
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(Self::from_extension)
    }

    /// 是否无损编码。`m4a` 容器可能是 ALAC 也可能是 AAC，按有损处理。
    pub fn is_lossless(&self) -> bool {
        matches!(
            self,
            MediaFormat::Flac
                | MediaFormat::Wav
                | MediaFormat::Aiff
                | MediaFormat::Ape
                | MediaFormat::Wv
                | MediaFormat::Dsf
                | MediaFormat::Dff
        )
    }
}

/// CUE 整轨来源：虚拟曲目在整轨音频文件内的时间窗口。
/// 序列化进契约 `Track.cueSource`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CueSource {
    /// CUE 文件绝对路径。
    pub cue_path: PathBuf,
    /// CUE 内的 TRACK 序号（01 起）。
    pub cue_index: u32,
    /// 轨起始时间（INDEX 01）。
    pub start_ms: u64,
    /// 轨结束时间（下一轨 INDEX 01）；文件内最后一轨为 None（播到文件尾）。
    pub end_ms: Option<u64>,
}

/// 媒体库曲目。字段与 docs/ipc-contract.md 的 `Track` 一一对应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// UUID v4。
    pub id: String,
    /// 文件绝对路径（CUE 虚拟曲目为整轨音频文件路径）。
    pub path: PathBuf,
    pub title: String,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub duration_ms: u64,
    pub format: MediaFormat,
    /// kbps，未知为 0。
    pub bitrate: u32,
    /// Hz，未知为 0。
    pub sample_rate: u32,
    pub bit_depth: Option<u8>,
    pub channels: Option<u8>,
    pub is_lossless: bool,
    pub has_embedded_cover: bool,
    pub play_count: u32,
    pub last_played: Option<DateTime<Utc>>,
    pub date_added: DateTime<Utc>,
    pub file_modified: DateTime<Utc>,
    /// CUE 整轨来源；普通文件曲目为 None。
    #[serde(default)]
    pub cue_source: Option<CueSource>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_serializes_as_contract_lowercase() {
        let cases = [
            (MediaFormat::Flac, "flac"),
            (MediaFormat::Mp3, "mp3"),
            (MediaFormat::M4a, "m4a"),
            (MediaFormat::Aac, "aac"),
            (MediaFormat::Ogg, "ogg"),
            (MediaFormat::Opus, "opus"),
            (MediaFormat::Wav, "wav"),
            (MediaFormat::Aiff, "aiff"),
            (MediaFormat::Ape, "ape"),
            (MediaFormat::Wv, "wv"),
            (MediaFormat::Wma, "wma"),
            (MediaFormat::Dsf, "dsf"),
            (MediaFormat::Dff, "dff"),
            (MediaFormat::Other, "other"),
        ];
        for (fmt, s) in cases {
            assert_eq!(serde_json::to_value(fmt).unwrap(), serde_json::json!(s));
            assert_eq!(MediaFormat::from_str_lossy(s), fmt);
            assert_eq!(fmt.as_str(), s);
        }
    }

    #[test]
    fn format_from_extension_is_case_insensitive() {
        assert_eq!(MediaFormat::from_extension("FLAC"), Some(MediaFormat::Flac));
        assert_eq!(MediaFormat::from_extension("Mp3"), Some(MediaFormat::Mp3));
        assert_eq!(MediaFormat::from_extension("aif"), Some(MediaFormat::Aiff));
        assert_eq!(MediaFormat::from_extension("wave"), Some(MediaFormat::Wav));
        assert_eq!(MediaFormat::from_extension("txt"), None);
        assert_eq!(MediaFormat::from_extension("mkv"), None);
    }

    #[test]
    fn format_from_path() {
        assert_eq!(
            MediaFormat::from_path(Path::new("a/b/歌.Flac")),
            Some(MediaFormat::Flac)
        );
        assert_eq!(MediaFormat::from_path(Path::new("noext")), None);
    }

    #[test]
    fn lossless_classification() {
        for f in [
            MediaFormat::Flac,
            MediaFormat::Wav,
            MediaFormat::Aiff,
            MediaFormat::Ape,
            MediaFormat::Wv,
            MediaFormat::Dsf,
            MediaFormat::Dff,
        ] {
            assert!(f.is_lossless(), "{f:?} should be lossless");
        }
        for f in [
            MediaFormat::Mp3,
            MediaFormat::M4a,
            MediaFormat::Aac,
            MediaFormat::Ogg,
            MediaFormat::Opus,
            MediaFormat::Wma,
            MediaFormat::Other,
        ] {
            assert!(!f.is_lossless(), "{f:?} should be lossy");
        }
    }

    #[test]
    fn track_serializes_camel_case_contract_shape() {
        let track = Track {
            id: "uuid-1".to_string(),
            path: PathBuf::new().join("music").join("a.flac"),
            title: "晴天".to_string(),
            artist: Some("周杰伦".to_string()),
            album_artist: None,
            album: Some("叶惠美".to_string()),
            track_number: Some(3),
            disc_number: None,
            year: Some(2003),
            genre: None,
            duration_ms: 269_000,
            format: MediaFormat::Flac,
            bitrate: 900,
            sample_rate: 44_100,
            bit_depth: Some(16),
            channels: Some(2),
            is_lossless: true,
            has_embedded_cover: false,
            play_count: 0,
            last_played: None,
            date_added: DateTime::parse_from_rfc3339("2026-07-28T13:52:21.845Z")
                .unwrap()
                .with_timezone(&Utc),
            file_modified: DateTime::parse_from_rfc3339("2026-07-01T00:00:00.000Z")
                .unwrap()
                .with_timezone(&Utc),
            cue_source: None,
        };
        let v = serde_json::to_value(&track).unwrap();
        // 契约要求的 camelCase 键必须全部存在
        for key in [
            "id",
            "path",
            "title",
            "artist",
            "albumArtist",
            "album",
            "trackNumber",
            "discNumber",
            "year",
            "genre",
            "durationMs",
            "format",
            "bitrate",
            "sampleRate",
            "bitDepth",
            "channels",
            "isLossless",
            "hasEmbeddedCover",
            "playCount",
            "lastPlayed",
            "dateAdded",
            "fileModified",
        ] {
            assert!(v.get(key).is_some(), "missing contract key: {key}");
        }
        assert_eq!(v["format"], "flac");
        assert_eq!(v["albumArtist"], serde_json::Value::Null);
        assert_eq!(v["durationMs"], 269_000);
        assert!(v["dateAdded"].as_str().unwrap().ends_with('Z'));
        // 普通曲目 cueSource 为 null
        assert_eq!(v["cueSource"], serde_json::Value::Null);
    }

    #[test]
    fn cue_source_serializes_camel_case() {
        let cue = CueSource {
            cue_path: PathBuf::from("D:/m/album.cue"),
            cue_index: 3,
            start_ms: 300_000,
            end_ms: Some(500_000),
        };
        let v = serde_json::to_value(&cue).unwrap();
        assert_eq!(v["cueIndex"], 3);
        assert_eq!(v["startMs"], 300_000);
        assert_eq!(v["endMs"], 500_000);
        assert!(v.get("cuePath").is_some());
        // 最后一轨 endMs 为 null
        let last = CueSource { end_ms: None, ..cue };
        assert_eq!(serde_json::to_value(&last).unwrap()["endMs"], serde_json::Value::Null);
    }
}
