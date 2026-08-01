//! 在线功能：歌词搜索（lrclib.net）+ 封面补全（MusicBrainz + Cover Art Archive）。
//!
//! 歌词：GET https://lrclib.net/api/get?artist_name={artist}&track_name={title}&album_name={album}
//! 封面：MusicBrainz 搜索 release → Cover Art Archive 下载 front-500

use serde::Deserialize;
use std::sync::LazyLock;

use crate::error::{AppError, AppResult};

/// 全局共享 HTTP 客户端（连接池复用，避免每次搜索新建 TCP 连接）。
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("Porrima/0.1 (https://github.com/porrima)")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("failed to build HTTP client")
});

/// lrclib.net API 响应。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcLibResponse {
    #[serde(default)]
    synced_lyrics: Option<String>,
    #[serde(default)]
    plain_lyrics: Option<String>,
}

/// 在线歌词搜索结果。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineLyrics {
    /// 同步歌词（LRC 格式）；无则为 null。
    pub synced_lyrics: Option<String>,
    /// 纯文本歌词；无则为 null。
    pub plain_lyrics: Option<String>,
}

/// 调用 lrclib.net API 搜索歌词。
///
/// 优先返回 synced（LRC）版本；两者都没有时返回 Err。
pub async fn search_lyrics(
    title: &str,
    artist: Option<&str>,
    album: Option<&str>,
) -> AppResult<OnlineLyrics> {
    let mut req = HTTP_CLIENT
        .get("https://lrclib.net/api/get")
        .query(&[("track_name", title)]);

    if let Some(a) = artist.filter(|s| !s.is_empty()) {
        req = req.query(&[("artist_name", a)]);
    }
    if let Some(al) = album.filter(|s| !s.is_empty()) {
        req = req.query(&[("album_name", al)]);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| AppError::Other(format!("lyrics request failed: {e}")))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AppError::NotFound("no lyrics found on lrclib.net".into()));
    }
    if !resp.status().is_success() {
        return Err(AppError::Other(format!(
            "lrclib.net returned status {}",
            resp.status()
        )));
    }

    let body: LrcLibResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("failed to parse lyrics response: {e}")))?;

    if body.synced_lyrics.is_none() && body.plain_lyrics.is_none() {
        return Err(AppError::NotFound("no lyrics content in response".into()));
    }

    Ok(OnlineLyrics {
        synced_lyrics: body.synced_lyrics.filter(|s| !s.trim().is_empty()),
        plain_lyrics: body.plain_lyrics.filter(|s| !s.trim().is_empty()),
    })
}

// ---------- 在线封面补全（MusicBrainz + Cover Art Archive） ----------

/// MusicBrainz release 搜索结果。
#[derive(Debug, Deserialize)]
struct MbRelease {
    id: String,
}

#[derive(Debug, Deserialize)]
struct MbSearchResponse {
    #[serde(default)]
    releases: Vec<MbRelease>,
}

/// 在线封面搜索结果。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineCover {
    /// 图片 MIME 类型。
    pub mime_type: String,
    /// Base64 编码的图片数据。
    pub data_base64: String,
}

/// 通过 MusicBrainz 搜索专辑，再从 Cover Art Archive 下载封面。
///
/// 流程：artist + album → MusicBrainz release search → MBID → CAA front-500。
pub async fn search_cover(
    artist: &str,
    album: &str,
) -> AppResult<OnlineCover> {
    // Step 1: MusicBrainz 搜索 release
    let query = format!("artist:\"{artist}\" AND release:\"{album}\"");
    let mb_resp = HTTP_CLIENT
        .get("https://musicbrainz.org/ws/2/release/")
        .query(&[("query", query.as_str()), ("fmt", "json"), ("limit", "1")])
        .send()
        .await
        .map_err(|e| AppError::Other(format!("MusicBrainz request failed: {e}")))?;

    if !mb_resp.status().is_success() {
        return Err(AppError::Other(format!(
            "MusicBrainz returned status {}",
            mb_resp.status()
        )));
    }

    let mb_body: MbSearchResponse = mb_resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("failed to parse MusicBrainz response: {e}")))?;

    let release_id = mb_body
        .releases
        .first()
        .map(|r| r.id.clone())
        .ok_or_else(|| AppError::NotFound("no matching release on MusicBrainz".into()))?;

    // Step 2: Cover Art Archive 下载封面（front-500 缩略图）
    let caa_url = format!("https://coverartarchive.org/release/{release_id}/front-500");
    let caa_resp = HTTP_CLIENT
        .get(&caa_url)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("Cover Art Archive request failed: {e}")))?;

    if caa_resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AppError::NotFound("no cover art for this release".into()));
    }
    if !caa_resp.status().is_success() {
        return Err(AppError::Other(format!(
            "Cover Art Archive returned status {}",
            caa_resp.status()
        )));
    }

    let mime = caa_resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();

    let bytes = caa_resp
        .bytes()
        .await
        .map_err(|e| AppError::Other(format!("failed to read cover data: {e}")))?;

    use base64::Engine as _;
    let data_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    Ok(OnlineCover {
        mime_type: mime,
        data_base64,
    })
}
