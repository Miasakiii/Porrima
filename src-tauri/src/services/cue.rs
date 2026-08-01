//! CUE sheet 解析（Phase 2 整轨支持）。
//!
//! 只关心分轨所需的最小子集：FILE / TRACK ... AUDIO / INDEX 00|01 /
//! TITLE / PERFORMER / REM GENRE / REM DATE。时间格式 `mm:ss:ff`（ff 为
//! 1/75 秒帧）。编码复用歌词模块的 UTF-8/GBK 探测（decode_text）。
//!
//! 虚拟曲目的组装（读整轨文件元数据、算结束时间）在 library 扫描侧完成，
//! 本模块保持纯文本解析、可单测。

use std::path::Path;

use crate::error::{AppError, AppResult};

use super::lyrics::decode_text;

/// 解析后的 CUE 单（专辑级信息 + 若干 FILE 段）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CueSheet {
    pub album_title: Option<String>,
    pub album_performer: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u32>,
    pub files: Vec<CueFile>,
}

/// FILE 段：一个整轨音频文件与其中的轨。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CueFile {
    /// FILE 行引用的音频文件名（相对 CUE 所在目录或绝对路径）。
    pub audio: String,
    pub tracks: Vec<CueTrack>,
}

/// TRACK 段（仅 AUDIO 类型）。
#[derive(Debug, Clone, PartialEq)]
pub struct CueTrack {
    /// TRACK 序号（01 起）。
    pub number: u32,
    pub title: Option<String>,
    pub performer: Option<String>,
    /// 轨起始（INDEX 01 优先，缺失回退 INDEX 00）。
    pub start_ms: u64,
}

/// 读取并解析 CUE 文件。
pub fn parse_file(path: &Path) -> AppResult<CueSheet> {
    let bytes = std::fs::read(path).map_err(AppError::Io)?;
    Ok(parse(&decode_text(&bytes)))
}

/// 解析 CUE 文本。容错：无法识别的行忽略；缺 INDEX 的轨丢弃；
/// 非 AUDIO 轨（如 MODE1/2352 数据轨）跳过。
pub fn parse(text: &str) -> CueSheet {
    let mut sheet = CueSheet::default();

    /// 解析中的轨（INDEX 可能后到）。
    struct PendingTrack {
        number: u32,
        is_audio: bool,
        title: Option<String>,
        performer: Option<String>,
        index00_ms: Option<u64>,
        index01_ms: Option<u64>,
    }

    let mut pending: Option<PendingTrack> = None;

    // 把攒下的轨落到当前 FILE（丢弃无 INDEX / 非 AUDIO 的）
    fn flush(pending: &mut Option<PendingTrack>, files: &mut [CueFile]) {
        let Some(t) = pending.take() else { return };
        let Some(file) = files.last_mut() else { return };
        if !t.is_audio {
            return;
        }
        let Some(start_ms) = t.index01_ms.or(t.index00_ms) else {
            return;
        };
        file.tracks.push(CueTrack {
            number: t.number,
            title: t.title,
            performer: t.performer,
            start_ms,
        });
    }

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let (keyword, rest) = match line.split_once(char::is_whitespace) {
            Some((k, r)) => (k.to_ascii_uppercase(), r.trim()),
            None => (line.to_ascii_uppercase(), ""),
        };

        match keyword.as_str() {
            "REM" => {
                // REM GENRE xxx / REM DATE yyyy（其余 REM 忽略）
                if let Some((sub, value)) = rest.split_once(char::is_whitespace) {
                    let value = unquote(value.trim());
                    match sub.to_ascii_uppercase().as_str() {
                        "GENRE" if !value.is_empty() => sheet.genre = Some(value),
                        "DATE" => sheet.year = value.parse().ok(),
                        _ => {}
                    }
                }
            }
            "FILE" => {
                flush(&mut pending, &mut sheet.files);
                // FILE "name.ext" WAVE —— 去掉尾部类型词，取引号内文件名
                let name = rest
                    .rsplit_once(char::is_whitespace)
                    .map(|(n, _type)| n.trim())
                    .unwrap_or(rest);
                sheet.files.push(CueFile {
                    audio: unquote(name),
                    tracks: Vec::new(),
                });
            }
            "TRACK" => {
                flush(&mut pending, &mut sheet.files);
                let mut parts = rest.split_whitespace();
                let number: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                let is_audio = parts
                    .next()
                    .is_none_or(|t| t.eq_ignore_ascii_case("AUDIO"));
                pending = Some(PendingTrack {
                    number,
                    is_audio,
                    title: None,
                    performer: None,
                    index00_ms: None,
                    index01_ms: None,
                });
            }
            "INDEX" => {
                if let Some(t) = pending.as_mut() {
                    let mut parts = rest.split_whitespace();
                    let idx: Option<u32> = parts.next().and_then(|s| s.parse().ok());
                    let time = parts.next().and_then(parse_msf);
                    match (idx, time) {
                        (Some(0), Some(ms)) => t.index00_ms = Some(ms),
                        (Some(1), Some(ms)) => t.index01_ms = Some(ms),
                        _ => {}
                    }
                }
            }
            "TITLE" => {
                let value = unquote(rest);
                match pending.as_mut() {
                    Some(t) => t.title = non_empty(value),
                    None => sheet.album_title = non_empty(value),
                }
            }
            "PERFORMER" => {
                let value = unquote(rest);
                match pending.as_mut() {
                    Some(t) => t.performer = non_empty(value),
                    None => sheet.album_performer = non_empty(value),
                }
            }
            _ => {}
        }
    }
    flush(&mut pending, &mut sheet.files);

    // 轨按序号排序（个别 CUE 乱序书写）
    for file in &mut sheet.files {
        file.tracks.sort_by_key(|t| t.number);
    }
    sheet
}

/// `mm:ss:ff` → 毫秒（ff 为 1/75 秒帧，四舍五入）。
fn parse_msf(s: &str) -> Option<u64> {
    let mut it = s.split(':');
    let mm: u64 = it.next()?.parse().ok()?;
    let ss: u64 = it.next()?.parse().ok()?;
    let ff: u64 = it.next()?.parse().ok()?;
    if it.next().is_some() || ss >= 60 || ff >= 75 {
        return None;
    }
    Some((mm * 60 + ss) * 1000 + (ff * 1000 + 37) / 75)
}

/// 去掉包裹引号；无引号则原样返回。
fn unquote(s: &str) -> String {
    let s = s.trim();
    s.strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(s)
        .to_string()
}

fn non_empty(s: String) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
REM GENRE "Pop"
REM DATE 2003
REM COMMENT "ExactAudioCopy v1.0"
PERFORMER "周杰伦"
TITLE "叶惠美"
FILE "叶惠美.flac" WAVE
  TRACK 01 AUDIO
    TITLE "以父之名"
    PERFORMER "周杰伦"
    INDEX 00 00:00:00
    INDEX 01 00:00:33
  TRACK 02 AUDIO
    TITLE "懦夫"
    INDEX 01 05:42:50
"#;

    #[test]
    fn parses_album_and_tracks() {
        let sheet = parse(SAMPLE);
        assert_eq!(sheet.album_title.as_deref(), Some("叶惠美"));
        assert_eq!(sheet.album_performer.as_deref(), Some("周杰伦"));
        assert_eq!(sheet.genre.as_deref(), Some("Pop"));
        assert_eq!(sheet.year, Some(2003));
        assert_eq!(sheet.files.len(), 1);

        let file = &sheet.files[0];
        assert_eq!(file.audio, "叶惠美.flac");
        assert_eq!(file.tracks.len(), 2);
        // INDEX 01 优先于 INDEX 00：33 帧 = 440ms
        assert_eq!(file.tracks[0].start_ms, 440);
        assert_eq!(file.tracks[0].title.as_deref(), Some("以父之名"));
        // 5:42 + 50/75s = 342_000 + 667ms
        assert_eq!(file.tracks[1].start_ms, 342_667);
        assert_eq!(file.tracks[1].performer, None);
    }

    #[test]
    fn falls_back_to_index00_and_skips_trackless() {
        let text = r#"
FILE "a.ape" WAVE
  TRACK 01 AUDIO
    INDEX 00 00:10:00
  TRACK 02 AUDIO
    TITLE "无 INDEX 的轨被丢弃"
"#;
        let sheet = parse(text);
        assert_eq!(sheet.files[0].tracks.len(), 1);
        assert_eq!(sheet.files[0].tracks[0].start_ms, 10_000);
    }

    #[test]
    fn skips_non_audio_tracks_and_supports_multi_file() {
        let text = r#"
FILE "cd1.wav" WAVE
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 00:30:00
FILE "cd2.wav" WAVE
  TRACK 03 AUDIO
    INDEX 01 00:00:00
"#;
        let sheet = parse(text);
        assert_eq!(sheet.files.len(), 2);
        assert_eq!(sheet.files[0].tracks.len(), 1);
        assert_eq!(sheet.files[0].tracks[0].number, 2);
        assert_eq!(sheet.files[1].tracks[0].number, 3);
    }

    #[test]
    fn msf_time_conversion_and_validation() {
        assert_eq!(parse_msf("00:00:00"), Some(0));
        assert_eq!(parse_msf("01:00:00"), Some(60_000));
        assert_eq!(parse_msf("00:01:74"), Some(1_000 + 987)); // 74/75≈986.7→987
        assert_eq!(parse_msf("100:00:00"), Some(6_000_000)); // 长音频分钟可超 99
        assert_eq!(parse_msf("00:60:00"), None);
        assert_eq!(parse_msf("00:00:75"), None);
        assert_eq!(parse_msf("bad"), None);
    }

    #[test]
    fn parse_file_reads_gbk_encoded_cue() {
        let dir = tempfile::tempdir().unwrap();
        let cue = dir.path().join("album.cue");
        let (gbk, _, _) =
            encoding_rs::GBK.encode("TITLE \"中文专辑\"\nFILE \"整轨.ape\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n");
        std::fs::write(&cue, &gbk).unwrap();

        let sheet = parse_file(&cue).unwrap();
        assert_eq!(sheet.album_title.as_deref(), Some("中文专辑"));
        assert_eq!(sheet.files[0].audio, "整轨.ape");
    }
}
