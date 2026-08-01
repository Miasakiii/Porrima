//! 歌词读取：同目录同名 .lrc 文件优先，回退标签内嵌歌词。
//!
//! LRC 时间轴解析在前端完成（`src/lib/lrc.ts`），后端只负责定位来源
//! 并把文本解码为 UTF-8（.lrc 常见 GBK 编码，需探测转换）。

use std::path::Path;

use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::ItemKey;

use crate::error::{AppError, AppResult};

/// 歌词来源（序列化进 IPC payload）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LyricsSource {
    LrcFile,
    Embedded,
}

/// 歌词文本 + 来源。
#[derive(Debug, Clone)]
pub struct LyricsData {
    pub source: LyricsSource,
    pub text: String,
}

/// 读取音频文件的歌词；两种来源都没有时返回 `Ok(None)`。
pub fn read_lyrics(path: &Path) -> AppResult<Option<LyricsData>> {
    if let Some(text) = read_lrc_file(path)? {
        return Ok(Some(LyricsData {
            source: LyricsSource::LrcFile,
            text,
        }));
    }
    Ok(read_embedded(path)?.map(|text| LyricsData {
        source: LyricsSource::Embedded,
        text,
    }))
}

/// 同目录同名 .lrc：`song.flac` → `song.lrc`。
fn read_lrc_file(path: &Path) -> AppResult<Option<String>> {
    let lrc_path = path.with_extension("lrc");
    let bytes = match std::fs::read(&lrc_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(AppError::Io(e)),
    };
    let text = decode_text(&bytes);
    if text.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(text))
}

/// 内嵌歌词标签（LYRICS / USLT 等，lofty 统一为 ItemKey::Lyrics）。
fn read_embedded(path: &Path) -> AppResult<Option<String>> {
    let tagged = Probe::open(path)
        .map_err(|e| AppError::Other(format!("lyrics probe error: {e}")))?
        .read()
        .map_err(|e| AppError::Other(format!("lyrics read error: {e}")))?;

    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    Ok(tag
        .and_then(|t| t.get_string(ItemKey::Lyrics))
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty()))
}

/// 文本解码：UTF-8（含 BOM）优先，失败回退 GBK，仍失败则 UTF-8 lossy。
/// 也供 CUE 解析复用（cue 文件同样常见 GBK 编码）。
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    // UTF-8 BOM
    let stripped = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if let Ok(s) = std::str::from_utf8(stripped) {
        return s.to_string();
    }
    let (decoded, _, had_errors) = encoding_rs::GBK.decode(stripped);
    if !had_errors {
        return decoded.into_owned();
    }
    String::from_utf8_lossy(stripped).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_sibling_lrc_file() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.flac");
        std::fs::write(&audio, b"fake").unwrap();
        std::fs::write(
            dir.path().join("song.lrc"),
            "[00:01.00]第一行\n[00:02.00]第二行\n",
        )
        .unwrap();

        let lyrics = read_lyrics(&audio).unwrap().expect("lrc expected");
        assert_eq!(lyrics.source, LyricsSource::LrcFile);
        assert!(lyrics.text.contains("第一行"));
    }

    #[test]
    fn decodes_gbk_lrc_file() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.flac");
        std::fs::write(&audio, b"fake").unwrap();
        let (gbk_bytes, _, _) = encoding_rs::GBK.encode("[00:01.00]中文歌词");
        std::fs::write(dir.path().join("song.lrc"), &gbk_bytes).unwrap();

        let lyrics = read_lyrics(&audio).unwrap().unwrap();
        assert!(lyrics.text.contains("中文歌词"));
    }

    #[test]
    fn strips_utf8_bom() {
        let mut bytes = b"\xEF\xBB\xBF".to_vec();
        bytes.extend_from_slice("[00:00.00]hi".as_bytes());
        assert_eq!(decode_text(&bytes), "[00:00.00]hi");
    }

    #[test]
    fn returns_none_when_no_lyrics() {
        let dir = tempfile::tempdir().unwrap();
        // 无标签 WAV：无 .lrc 也无内嵌歌词
        let audio = dir.path().join("song.wav");
        let data_size: u32 = 4;
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(36 + data_size).to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&44100u32.to_le_bytes());
        buf.extend_from_slice(&(44100u32 * 2).to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&16u16.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.resize(44 + data_size as usize, 0);
        std::fs::write(&audio, buf).unwrap();

        assert!(read_lyrics(&audio).unwrap().is_none());
    }

    #[test]
    fn ignores_empty_lrc_file() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.flac");
        std::fs::write(&audio, b"fake").unwrap();
        std::fs::write(dir.path().join("song.lrc"), "  \n ").unwrap();
        // .lrc 为空白 → 尝试内嵌（假 flac 会探测失败，但 .lrc 已判 None）
        // 假 flac 内嵌读取报错属预期：这里只断言不会把空白当歌词
        let result = read_lyrics(&audio);
        match result {
            Ok(v) => assert!(v.is_none()),
            Err(_) => {} // 内嵌探测失败可接受
        }
    }
}
