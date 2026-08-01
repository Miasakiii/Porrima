//! 封面提取：内嵌图片优先，回退同目录本地封面文件。
//!
//! 读取顺序（Phase 2 契约）：
//! 1. lofty 读取标签内嵌图片（ID3 APIC / FLAC PICTURE / MP4 covr）
//! 2. 同目录 cover / folder / front + jpg / jpeg / png / webp 组合
//!
//! 另提供 `cover_color`：解码封面后提取一个适合做 UI 强调色的代表色（image crate）。

use std::path::Path;

use lofty::file::TaggedFileExt;
use lofty::probe::Probe;

use crate::error::{AppError, AppResult};

/// 封面二进制 + MIME。
#[derive(Debug, Clone)]
pub struct CoverData {
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// 封面代表色（sRGB）。前端换算到 OKLCH 并按主题裁剪明度后写入 --accent。
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct CoverColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// 同目录本地封面候选文件名（按优先级）。
const LOCAL_STEMS: [&str; 3] = ["cover", "folder", "front"];
const LOCAL_EXTS: [(&str, &str); 4] = [
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("png", "image/png"),
    ("webp", "image/webp"),
];

/// 读取音频文件的封面；两种来源都没有时返回 `Ok(None)`。
pub fn read_cover(path: &Path) -> AppResult<Option<CoverData>> {
    if let Some(cover) = read_embedded(path)? {
        return Ok(Some(cover));
    }
    Ok(read_local(path))
}

/// 内嵌封面：取第一张图片（lofty 已按 front-cover 优先排序主标签图片）。
fn read_embedded(path: &Path) -> AppResult<Option<CoverData>> {
    let tagged = Probe::open(path)
        .map_err(|e| AppError::Other(format!("cover probe error: {e}")))?
        .read()
        .map_err(|e| AppError::Other(format!("cover read error: {e}")))?;

    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    let Some(tag) = tag else { return Ok(None) };
    let Some(picture) = tag.pictures().first() else {
        return Ok(None);
    };
    if picture.data().is_empty() {
        return Ok(None);
    }
    let mime_type = picture
        .mime_type()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "image/jpeg".to_string());
    Ok(Some(CoverData {
        mime_type,
        data: picture.data().to_vec(),
    }))
}

/// 同目录本地封面：cover/folder/front × jpg/jpeg/png/webp，命中即返回。
fn read_local(path: &Path) -> Option<CoverData> {
    let dir = path.parent()?;
    for stem in LOCAL_STEMS {
        for (ext, mime) in LOCAL_EXTS {
            let candidate = dir.join(format!("{stem}.{ext}"));
            if let Ok(data) = std::fs::read(&candidate) {
                if !data.is_empty() {
                    return Some(CoverData {
                        mime_type: mime.to_string(),
                        data,
                    });
                }
            }
        }
    }
    None
}

/// 提取封面代表色；无封面或解码失败时返回 `Ok(None)`。
/// 解码走 image crate，随后由 `dominant_color` 做缩放 + 直方图取色。
pub fn cover_color(path: &Path) -> AppResult<Option<CoverColor>> {
    let Some(cover) = read_cover(path)? else {
        return Ok(None);
    };
    match image::load_from_memory(&cover.data) {
        Ok(img) => Ok(Some(dominant_color(&img))),
        Err(e) => {
            // 非法/不支持的图片不应让播放链路失败，静默降级为无主题色。
            tracing::debug!(error = %e, "cover color decode failed");
            Ok(None)
        }
    }
}

/// 从封面图取一个代表色：缩放到 <=64x64 后按「饱和度加权直方图」取主色。
///
/// - 每通道量化到 5bit（32^3 桶），累加加权计数与原色，取分数最高桶的加权均值；
/// - 跳过近黑/近白像素（多为背景/纸面），避免主题色发灰发暗；
/// - 纯灰度/极端图回退为整体像素均值。
fn dominant_color(img: &image::DynamicImage) -> CoverColor {
    let small = img.thumbnail(64, 64).to_rgb8();

    // key: (r5<<10)|(g5<<5)|b5 -> (weight, r*w, g*w, b*w)
    let mut buckets: std::collections::HashMap<u16, (f64, f64, f64, f64)> =
        std::collections::HashMap::new();
    let mut best_key: Option<u16> = None;
    let mut best_weight = 0.0f64;
    let mut sum = (0.0f64, 0.0f64, 0.0f64);
    let mut n = 0u64;

    for p in small.pixels() {
        let [r, g, b] = p.0;
        let (rf, gf, bf) = (r as f64, g as f64, b as f64);
        sum.0 += rf;
        sum.1 += gf;
        sum.2 += bf;
        n += 1;

        // HSL 明度/饱和度（0..1）
        let max = rf.max(gf).max(bf) / 255.0;
        let min = rf.min(gf).min(bf) / 255.0;
        let l = (max + min) / 2.0;
        if !(0.08..=0.95).contains(&l) {
            continue; // 近黑/近白：多为背景，跳过
        }
        let sat = if (max - min).abs() < 1e-6 {
            0.0
        } else {
            (max - min) / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6)
        };
        let w = sat + 0.12; // 基底让低饱和像素也计入一点
        let key = (((r >> 3) as u16) << 10) | (((g >> 3) as u16) << 5) | ((b >> 3) as u16);
        let e = buckets.entry(key).or_insert((0.0, 0.0, 0.0, 0.0));
        e.0 += w;
        e.1 += rf * w;
        e.2 += gf * w;
        e.3 += bf * w;
        if e.0 > best_weight {
            best_weight = e.0;
            best_key = Some(key);
        }
    }

    if let Some(k) = best_key {
        let e = buckets[&k];
        if e.0 > 0.0 {
            return CoverColor {
                r: (e.1 / e.0).round().clamp(0.0, 255.0) as u8,
                g: (e.2 / e.0).round().clamp(0.0, 255.0) as u8,
                b: (e.3 / e.0).round().clamp(0.0, 255.0) as u8,
            };
        }
    }
    // 回退：整体均值（全黑/全白/空图）
    if n == 0 {
        return CoverColor { r: 128, g: 128, b: 128 };
    }
    CoverColor {
        r: (sum.0 / n as f64).round() as u8,
        g: (sum.1 / n as f64).round() as u8,
        b: (sum.2 / n as f64).round() as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小合法 WAV（无标签、无内嵌封面）。与 metadata 测试保持一致。
    fn write_minimal_wav(path: &Path) {
        let data_size: u32 = 44100 * 2;
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
        std::fs::write(path, buf).unwrap();
    }

    #[test]
    fn returns_none_without_any_cover() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.wav");
        write_minimal_wav(&audio);
        assert!(read_cover(&audio).unwrap().is_none());
    }

    #[test]
    fn falls_back_to_local_cover_file() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.wav");
        write_minimal_wav(&audio);
        std::fs::write(dir.path().join("cover.png"), b"\x89PNG fake").unwrap();

        let cover = read_cover(&audio).unwrap().expect("local cover expected");
        assert_eq!(cover.mime_type, "image/png");
        assert_eq!(cover.data, b"\x89PNG fake");
    }

    #[test]
    fn prefers_cover_stem_over_folder() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.wav");
        write_minimal_wav(&audio);
        std::fs::write(dir.path().join("folder.jpg"), b"folder-art").unwrap();
        std::fs::write(dir.path().join("cover.jpg"), b"cover-art").unwrap();

        let cover = read_cover(&audio).unwrap().unwrap();
        assert_eq!(cover.mime_type, "image/jpeg");
        assert_eq!(cover.data, b"cover-art");
    }

    #[test]
    fn ignores_empty_local_cover_file() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.wav");
        write_minimal_wav(&audio);
        std::fs::write(dir.path().join("cover.jpg"), b"").unwrap();
        assert!(read_cover(&audio).unwrap().is_none());
    }

    #[test]
    fn dominant_color_picks_vibrant_over_dark_background() {
        // 大面积近黑背景 + 中间一块红：应取红而非黑
        let mut img = image::RgbImage::from_pixel(16, 16, image::Rgb([8, 8, 8]));
        for y in 4..12 {
            for x in 4..12 {
                img.put_pixel(x, y, image::Rgb([210, 40, 40]));
            }
        }
        let c = dominant_color(&image::DynamicImage::ImageRgb8(img));
        assert!(
            c.r > 150 && c.g < 90 && c.b < 90,
            "expected red-dominant, got {:?}",
            (c.r, c.g, c.b)
        );
    }

    #[test]
    fn cover_color_reads_local_png() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.wav");
        write_minimal_wav(&audio);
        // 同目录纯蓝封面
        let img = image::RgbImage::from_pixel(8, 8, image::Rgb([30, 60, 200]));
        image::DynamicImage::ImageRgb8(img)
            .save(dir.path().join("cover.png"))
            .unwrap();

        let c = cover_color(&audio).unwrap().expect("color expected");
        assert!(
            c.b > c.r && c.b > c.g,
            "expected blue-dominant, got {:?}",
            (c.r, c.g, c.b)
        );
    }

    #[test]
    fn cover_color_none_without_cover() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.wav");
        write_minimal_wav(&audio);
        assert!(cover_color(&audio).unwrap().is_none());
    }
}
