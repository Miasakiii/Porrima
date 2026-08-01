//! 音频文件元数据读取（lofty）。
//!
//! 从文件标签 + 流属性构建 `Track`：标签缺失的字段回退为空/文件名，
//! 时长/码率/采样率等来自解码器探测的属性，与标签无关。

use std::path::Path;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey};

use crate::error::{AppError, AppResult};
use crate::models::track::{MediaFormat, Track};

/// 读取单个音频文件，构建待入库的 `Track`（id 为新 uuid，play 字段为零值）。
pub fn read_track(path: &Path) -> AppResult<Track> {
    let format = MediaFormat::from_path(path).ok_or_else(|| {
        AppError::InvalidArgument(format!("unsupported format: {}", path.display()))
    })?;

    let tagged = Probe::open(path)
        .map_err(meta_err)?
        .read()
        .map_err(meta_err)?;

    let props = tagged.properties();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let (
        mut title,
        mut artist,
        mut album_artist,
        mut album,
        mut track_number,
        mut disc_number,
        mut year,
        mut genre,
        mut has_cover,
    ) = (None, None, None, None, None, None, None, None, false);
    if let Some(tag) = tag {
        title = tag.title().map(|s| s.into_owned());
        artist = tag.artist().map(|s| s.into_owned());
        album_artist = tag.get_string(ItemKey::AlbumArtist).map(str::to_string);
        album = tag.album().map(|s| s.into_owned());
        track_number = tag.track();
        disc_number = tag.disk();
        year = tag
            .get_string(ItemKey::Year)
            .and_then(|s| s.parse::<u32>().ok());
        genre = tag.genre().map(|s| s.into_owned());
        has_cover = !tag.pictures().is_empty();
    }

    // 标题回退：去扩展名的文件名
    let title = title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Unknown".to_string())
    });

    let file_modified = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(chrono::DateTime::from)
        .unwrap_or_else(|_| chrono::Utc::now());

    Ok(Track {
        id: uuid::Uuid::new_v4().to_string(),
        path: path.to_path_buf(),
        title,
        artist,
        album_artist,
        album,
        track_number,
        disc_number,
        year,
        genre,
        duration_ms: props.duration().as_millis() as u64,
        format,
        bitrate: props
            .overall_bitrate()
            .or(props.audio_bitrate())
            .unwrap_or(0),
        sample_rate: props.sample_rate().unwrap_or(0),
        bit_depth: props.bit_depth(),
        channels: props.channels(),
        is_lossless: format.is_lossless(),
        has_embedded_cover: has_cover,
        play_count: 0,
        last_played: None,
        date_added: chrono::Utc::now(),
        file_modified,
        cue_source: None,
    })
}

fn meta_err(e: lofty::error::LoftyError) -> AppError {
    AppError::Other(format!("metadata error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小合法 WAV 文件（44 字节头 + 数据段），lofty 可读属性但无标签。
    fn write_minimal_wav(path: &Path) {
        let data_size: u32 = 44100 * 2; // 0.5s 16bit mono
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(36 + data_size).to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&1u16.to_le_bytes()); // mono
        buf.extend_from_slice(&44100u32.to_le_bytes());
        buf.extend_from_slice(&(44100u32 * 2).to_le_bytes()); // byte rate
        buf.extend_from_slice(&2u16.to_le_bytes()); // block align
        buf.extend_from_slice(&16u16.to_le_bytes()); // bit depth
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.resize(44 + data_size as usize, 0);
        std::fs::write(path, buf).unwrap();
    }

    #[test]
    fn reads_wav_properties_and_falls_back_to_filename() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("测试曲目.wav");
        write_minimal_wav(&path);

        let track = read_track(&path).unwrap();
        assert_eq!(track.format, MediaFormat::Wav);
        assert_eq!(track.title, "测试曲目");
        assert_eq!(track.sample_rate, 44_100);
        assert_eq!(track.bit_depth, Some(16));
        assert_eq!(track.channels, Some(1));
        assert!(track.is_lossless);
        assert!(!track.has_embedded_cover);
        // 时长 1s（44100 采样 / 44.1kHz，允许解码器取整误差）
        assert!(
            (900..=1100).contains(&track.duration_ms),
            "duration_ms={}",
            track.duration_ms
        );
    }

    #[test]
    fn rejects_unsupported_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, b"hello").unwrap();
        assert!(read_track(&path).is_err());
    }
}
