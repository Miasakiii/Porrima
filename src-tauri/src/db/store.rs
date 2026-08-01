//! SQLite 访问层：Track CRUD、分页/搜索/排序查询、统计、设置读写。
//!
//! - 连接构造即执行迁移（`migrations::run`），调用方拿到的就是最新 schema。
//! - 时间统一存 RFC3339 字符串；路径存原始字符串（Windows 反斜杠原样保存）。
//! - 全文搜索走 FTS5(trigram)：>= 3 字符的查询用语例匹配，
//!   1-2 字符的短查询 trigram 索引覆盖不到，退化为 LIKE（曲库量级下可接受）。

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};
use crate::models::settings::Settings;
use crate::models::track::{CueSource, MediaFormat, Track};

use super::migrations;

/// 列表查询参数（对应契约 `list_tracks`）。
#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    pub offset: u32,
    pub limit: u32,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    pub search: Option<String>,
}

/// 分页结果。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackPage {
    pub tracks: Vec<Track>,
    pub total: u64,
}

/// 媒体库统计（对应契约 `get_library_stats`）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub track_count: u64,
    pub total_duration_ms: u64,
    pub lossless_count: u64,
}

/// 专辑聚合摘要（对应契约 `list_albums`）。
/// 按 (专辑名, 专辑艺术家) 分组；`name`/`album_artist` 为 None 时前端显示「未知专辑/未知艺术家」。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumSummary {
    /// 稳定标识（name+album_artist 哈希），仅供前端做 React key/选中态。
    pub id: String,
    pub name: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u32>,
    pub track_count: u64,
    pub total_duration_ms: u64,
    /// 代表曲目 id（专辑首轨），前端据此取封面。
    pub cover_track_id: String,
}

/// 艺术家聚合摘要（对应契约 `list_artists`）。`name` 为 None 时前端显示「未知艺术家」。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistSummary {
    pub name: Option<String>,
    pub album_count: u64,
    pub track_count: u64,
    pub total_duration_ms: u64,
}

/// 播放列表摘要（对应契约 `list_playlists` / create / rename 返回）。
/// 时间戳以 RFC3339 字符串直接返回（前端仅展示，不参与计算）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub track_count: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// 统计概览（对应契约 `get_stats_summary`）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSummary {
    pub track_count: u64,
    pub total_duration_ms: u64,
    pub lossless_count: u64,
    pub total_plays: u64,
    pub played_count: u64,
    pub formats: Vec<FormatCount>,
}

/// 单个格式的曲目数（格式分布）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatCount {
    pub format: String,
    pub count: u64,
}

pub struct Store {
    conn: Connection,
    /// 数据库文件路径（内存库为 None）。扫描线程用它打开第二个连接。
    path: Option<std::path::PathBuf>,
}

impl Store {
    /// 打开（必要时创建）数据库并迁移到最新 schema。
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path).map_err(db_err)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(db_err)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(db_err)?;
        conn.pragma_update(None, "busy_timeout", 5000)
            .map_err(db_err)?;
        migrations::run(&conn)?;
        Ok(Store {
            conn,
            path: Some(path),
        })
    }

    /// 数据库文件路径（内存库返回 None）。
    pub fn path(&self) -> Option<std::path::PathBuf> {
        self.path.clone()
    }

    /// 内存库，仅供测试。同样开启外键，使播放列表级联行为与生产一致。
    #[cfg(test)]
    pub(crate) fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(db_err)?;
        migrations::run(&conn)?;
        Ok(Store { conn, path: None })
    }

    // ---------- Track 写入 ----------

    /// 按 (path, cue_index) 幂等 upsert（扫描重复执行安全）。
    /// 已存在时保留 id / play_count / last_played / date_added。
    pub fn upsert_track(&self, track: &Track) -> AppResult<()> {
        upsert_one(&self.conn, track)
    }

    /// 批量 upsert，包在一个事务里（扫描入库的热路径）。
    pub fn upsert_tracks(&self, tracks: &[Track]) -> AppResult<u64> {
        self.conn.execute_batch("BEGIN").map_err(db_err)?;
        let result = (|| {
            for track in tracks {
                upsert_one(&self.conn, track)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.conn.execute_batch("COMMIT").map_err(db_err)?;
                Ok(tracks.len() as u64)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    // ---------- Track 查询 ----------

    pub fn get_track(&self, id: &str) -> AppResult<Track> {
        self.conn
            .query_row("SELECT * FROM tracks WHERE id = ?1", [id], row_to_track)
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| AppError::NotFound(format!("track {id}")))
    }

    /// 按文件路径查找（命令行/文件关联打开时去重用）。
    /// 同一整轨文件可能对应多条 CUE 虚拟曲目，按 cue_index 升序返回。
    pub fn get_tracks_by_path(&self, path: &str) -> AppResult<Vec<Track>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM tracks WHERE path = ?1 ORDER BY cue_index")
            .map_err(db_err)?;
        collect_tracks(&mut stmt, [path])
    }

    /// 列出某目录前缀下所有已入库路径（扫描增删对比用）。
    pub fn paths_under(&self, dir: &str) -> AppResult<Vec<String>> {
        let prefix = format!("{}%", dir.trim_end_matches(['/', '\\']));
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM tracks WHERE path LIKE ?1")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([prefix], |r| r.get::<_, String>(0))
            .map_err(db_err)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(db_err)?);
        }
        Ok(out)
    }

    pub fn delete_by_path(&self, path: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM tracks WHERE path = ?1", [path])
            .map_err(db_err)?;
        Ok(())
    }

    // ---------- CUE 虚拟曲目维护 ----------

    /// 某目录前缀下所有已入库的 CUE 文件路径（去重）。
    pub fn cue_paths_under(&self, dir: &str) -> AppResult<Vec<String>> {
        let prefix = format!("{}%", dir.trim_end_matches(['/', '\\']));
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT cue_path FROM tracks WHERE path LIKE ?1 AND cue_path IS NOT NULL")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([prefix], |r| r.get::<_, String>(0))
            .map_err(db_err)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(db_err)?);
        }
        Ok(out)
    }

    /// 删除来自指定 CUE 文件的全部虚拟曲目（CUE 文件已删除/失效时）。
    pub fn delete_by_cue_path(&self, cue_path: &str) -> AppResult<u64> {
        let n = self
            .conn
            .execute("DELETE FROM tracks WHERE cue_path = ?1", [cue_path])
            .map_err(db_err)?;
        Ok(n as u64)
    }

    /// 删除整文件曲目行（cue_index=0）：文件被 CUE 覆盖后不再作为单曲展示。
    pub fn delete_whole_file_track(&self, path: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM tracks WHERE path = ?1 AND cue_index = 0",
                [path],
            )
            .map_err(db_err)?;
        Ok(())
    }

    /// 清理某 CUE 文件下已不存在的轨号（CUE 内容变更、轨数减少时）。
    pub fn prune_cue_indexes(&self, cue_path: &str, keep: &[u32]) -> AppResult<()> {
        if keep.is_empty() {
            self.delete_by_cue_path(cue_path)?;
            return Ok(());
        }
        let placeholders = vec!["?"; keep.len()].join(",");
        let sql = format!(
            "DELETE FROM tracks WHERE cue_path = ? AND cue_index NOT IN ({placeholders})"
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(cue_path.to_string())];
        for k in keep {
            params_vec.push(Box::new(*k as i64));
        }
        self.conn
            .execute(
                &sql,
                rusqlite::params_from_iter(params_vec.iter().map(|b| b.as_ref())),
            )
            .map_err(db_err)?;
        Ok(())
    }

    /// 分页/搜索/排序列表。排序字段白名单校验，拒绝任何非预期列名。
    pub fn list_tracks(&self, q: &ListQuery) -> AppResult<TrackPage> {
        let sort_col = match q.sort_by.as_deref().unwrap_or("dateAdded") {
            "title" => "title COLLATE NOCASE",
            "artist" => "artist COLLATE NOCASE",
            "album" => "album COLLATE NOCASE",
            "durationMs" => "duration_ms",
            "playCount" => "play_count",
            "dateAdded" => "date_added",
            other => {
                return Err(AppError::InvalidArgument(format!(
                    "unsupported sortBy: {other}"
                )))
            }
        };
        let sort_dir = match q.sort_dir.as_deref().unwrap_or("desc") {
            "asc" => "ASC",
            "desc" => "DESC",
            other => {
                return Err(AppError::InvalidArgument(format!(
                    "unsupported sortDir: {other}"
                )))
            }
        };

        let search = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let limit = q.limit.clamp(1, 1000) as i64;
        let offset = q.offset as i64;

        let (tracks, total) = match search {
            Some(s) => self.search_tracks(s, sort_col, sort_dir, limit, offset)?,
            None => {
                let sql = format!(
                    "SELECT * FROM tracks ORDER BY {sort_col} {sort_dir}, rowid LIMIT ?1 OFFSET ?2"
                );
                let mut stmt = self.conn.prepare(&sql).map_err(db_err)?;
                let tracks = collect_tracks(&mut stmt, params![limit, offset])?;
                let total: i64 = self
                    .conn
                    .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
                    .map_err(db_err)?;
                (tracks, total as u64)
            }
        };
        Ok(TrackPage { tracks, total })
    }

    /// FTS trigram 搜索（>=3 字符）或 LIKE 回退（1-2 字符）。
    fn search_tracks(
        &self,
        search: &str,
        sort_col: &str,
        sort_dir: &str,
        limit: i64,
        offset: i64,
    ) -> AppResult<(Vec<Track>, u64)> {
        let chars = search.chars().count();
        if chars >= 3 {
            // 语例匹配：双引号包裹并转义内部引号，命中任一索引列。
            let phrase = format!("\"{}\"", search.replace('"', "\"\""));
            let sql = format!(
                "SELECT tracks.* FROM tracks
                 JOIN tracks_fts ON tracks_fts.rowid = tracks.rowid
                 WHERE tracks_fts MATCH ?1
                 ORDER BY {sort_col} {sort_dir}, tracks.rowid LIMIT ?2 OFFSET ?3"
            );
            let mut stmt = self.conn.prepare(&sql).map_err(db_err)?;
            let tracks = collect_tracks(&mut stmt, params![phrase, limit, offset])?;
            let total: i64 = self
                .conn
                .query_row(
                    "SELECT count(*) FROM tracks
                     JOIN tracks_fts ON tracks_fts.rowid = tracks.rowid
                     WHERE tracks_fts MATCH ?1",
                    [phrase],
                    |r| r.get(0),
                )
                .map_err(db_err)?;
            Ok((tracks, total as u64))
        } else {
            let like = format!("%{}%", like_escape(search));
            let sql = format!(
                "SELECT * FROM tracks
                 WHERE title LIKE ?1 ESCAPE '\\' OR artist LIKE ?1 ESCAPE '\\' OR album LIKE ?1 ESCAPE '\\'
                 ORDER BY {sort_col} {sort_dir}, rowid LIMIT ?2 OFFSET ?3"
            );
            let mut stmt = self.conn.prepare(&sql).map_err(db_err)?;
            let tracks = collect_tracks(&mut stmt, params![like, limit, offset])?;
            let total: i64 = self
                .conn
                .query_row(
                    "SELECT count(*) FROM tracks
                     WHERE title LIKE ?1 ESCAPE '\\' OR artist LIKE ?1 ESCAPE '\\' OR album LIKE ?1 ESCAPE '\\'",
                    [like],
                    |r| r.get(0),
                )
                .map_err(db_err)?;
            Ok((tracks, total as u64))
        }
    }

    pub fn stats(&self) -> AppResult<LibraryStats> {
        let (count, duration, lossless): (i64, i64, i64) = self
            .conn
            .query_row(
                "SELECT count(*), coalesce(sum(duration_ms), 0), coalesce(sum(is_lossless), 0)
                 FROM tracks",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(db_err)?;
        Ok(LibraryStats {
            track_count: count as u64,
            total_duration_ms: duration as u64,
            lossless_count: lossless as u64,
        })
    }

    // ---------- 播放统计 ----------

    /// 记一次完整播放：play_count +1，last_played=now。
    pub fn record_play(&self, id: &str) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE tracks SET play_count = play_count + 1, last_played = ?2 WHERE id = ?1",
                params![id, now],
            )
            .map_err(db_err)?;
        Ok(())
    }

    /// 最近播放（有 last_played 的曲目，按时间倒序）。
    pub fn recently_played(&self, limit: u32) -> AppResult<Vec<Track>> {
        let limit = limit.clamp(1, 500) as i64;
        let mut stmt = self
            .conn
            .prepare(
                "SELECT * FROM tracks WHERE last_played IS NOT NULL
                 ORDER BY last_played DESC LIMIT ?1",
            )
            .map_err(db_err)?;
        collect_tracks(&mut stmt, params![limit])
    }

    /// 常听排行（play_count>0，按次数倒序，次数相同按最近播放）。
    pub fn most_played(&self, limit: u32) -> AppResult<Vec<Track>> {
        let limit = limit.clamp(1, 500) as i64;
        let mut stmt = self
            .conn
            .prepare(
                "SELECT * FROM tracks WHERE play_count > 0
                 ORDER BY play_count DESC, last_played DESC LIMIT ?1",
            )
            .map_err(db_err)?;
        collect_tracks(&mut stmt, params![limit])
    }

    /// 统计概览：总数/时长/无损、总播放/已播放数、格式分布。
    pub fn stats_summary(&self) -> AppResult<StatsSummary> {
        let (track_count, total_duration_ms, lossless_count, total_plays, played_count): (
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = self
            .conn
            .query_row(
                "SELECT count(*), coalesce(sum(duration_ms),0), coalesce(sum(is_lossless),0),
                        coalesce(sum(play_count),0),
                        coalesce(sum(CASE WHEN play_count>0 THEN 1 ELSE 0 END),0)
                 FROM tracks",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .map_err(db_err)?;

        let mut stmt = self
            .conn
            .prepare(
                "SELECT format, count(*) FROM tracks GROUP BY format ORDER BY count(*) DESC, format",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(FormatCount {
                    format: r.get::<_, String>(0)?,
                    count: r.get::<_, i64>(1)? as u64,
                })
            })
            .map_err(db_err)?;
        let mut formats = Vec::new();
        for row in rows {
            formats.push(row.map_err(db_err)?);
        }

        Ok(StatsSummary {
            track_count: track_count as u64,
            total_duration_ms: total_duration_ms as u64,
            lossless_count: lossless_count as u64,
            total_plays: total_plays as u64,
            played_count: played_count as u64,
            formats,
        })
    }

    // ---------- 专辑 / 艺术家聚合 ----------

    /// 全部专辑摘要，按 (专辑艺术家, 专辑名) 升序；未知分组排最后。
    ///
    /// 借助 SQLite「查询中恰有一个 min()/max() 聚合时，同 SELECT 的裸列取自极值所在行」
    /// 这一特性：用 MIN(disc*大数 + track) 选出每张专辑首轨，其 `id`/`year` 即代表封面曲目与年份。
    pub fn albums(&self) -> AppResult<Vec<AlbumSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    NULLIF(album, '') AS name,
                    COALESCE(NULLIF(album_artist, ''), NULLIF(artist, '')) AS aartist,
                    COUNT(*) AS track_count,
                    COALESCE(SUM(duration_ms), 0) AS total_ms,
                    year AS rep_year,
                    id AS cover_track_id,
                    MIN(COALESCE(disc_number, 1) * 100000 + COALESCE(track_number, 0)) AS _ord
                 FROM tracks
                 GROUP BY name, aartist
                 ORDER BY aartist IS NULL, aartist COLLATE NOCASE,
                          name IS NULL, name COLLATE NOCASE",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |row| {
                let name: Option<String> = row.get("name")?;
                let album_artist: Option<String> = row.get("aartist")?;
                let year: Option<i64> = row.get("rep_year")?;
                Ok(AlbumSummary {
                    id: album_id(name.as_deref(), album_artist.as_deref()),
                    name,
                    album_artist,
                    year: year.map(|y| y as u32),
                    track_count: row.get::<_, i64>("track_count")? as u64,
                    total_duration_ms: row.get::<_, i64>("total_ms")? as u64,
                    cover_track_id: row.get("cover_track_id")?,
                })
            })
            .map_err(db_err)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(db_err)?);
        }
        Ok(out)
    }

    /// 全部艺术家摘要（按曲目 artist 归组，回退 album_artist），未知分组排最后。
    pub fn artists(&self) -> AppResult<Vec<ArtistSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    COALESCE(NULLIF(artist, ''), NULLIF(album_artist, '')) AS aname,
                    COUNT(DISTINCT NULLIF(album, '')) AS album_count,
                    COUNT(*) AS track_count,
                    COALESCE(SUM(duration_ms), 0) AS total_ms
                 FROM tracks
                 GROUP BY aname
                 ORDER BY aname IS NULL, aname COLLATE NOCASE",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ArtistSummary {
                    name: row.get("aname")?,
                    album_count: row.get::<_, i64>("album_count")? as u64,
                    track_count: row.get::<_, i64>("track_count")? as u64,
                    total_duration_ms: row.get::<_, i64>("total_ms")? as u64,
                })
            })
            .map_err(db_err)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(db_err)?);
        }
        Ok(out)
    }

    /// 某专辑的全部曲目，按 (碟号, 轨号, 标题) 排序。分组键用 `IS`（NULL 安全）匹配，
    /// 与 `albums()` 的归组表达式保持一致，未知专辑/艺术家传 None 即可命中。
    pub fn album_tracks(
        &self,
        album: Option<&str>,
        album_artist: Option<&str>,
    ) -> AppResult<Vec<Track>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT * FROM tracks
                 WHERE NULLIF(album, '') IS ?1
                   AND COALESCE(NULLIF(album_artist, ''), NULLIF(artist, '')) IS ?2
                 ORDER BY COALESCE(disc_number, 1), COALESCE(track_number, 0),
                          title COLLATE NOCASE, rowid",
            )
            .map_err(db_err)?;
        collect_tracks(&mut stmt, params![album, album_artist])
    }

    /// 某艺术家的全部曲目，按 (专辑, 碟号, 轨号, 标题) 排序，便于前端按专辑分段。
    pub fn artist_tracks(&self, artist: Option<&str>) -> AppResult<Vec<Track>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT * FROM tracks
                 WHERE COALESCE(NULLIF(artist, ''), NULLIF(album_artist, '')) IS ?1
                 ORDER BY NULLIF(album, '') IS NULL, NULLIF(album, '') COLLATE NOCASE,
                          COALESCE(disc_number, 1), COALESCE(track_number, 0),
                          title COLLATE NOCASE, rowid",
            )
            .map_err(db_err)?;
        collect_tracks(&mut stmt, params![artist])
    }

    // ---------- 播放列表 ----------

    /// 新建播放列表（空名拒绝）。
    pub fn create_playlist(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> AppResult<PlaylistSummary> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::InvalidArgument("playlist name is empty".into()));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let desc = description.map(str::trim).filter(|s| !s.is_empty());
        self.conn
            .execute(
                "INSERT INTO playlists (id, name, description, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![id, name, desc, now],
            )
            .map_err(db_err)?;
        self.get_playlist_summary(&id)
    }

    /// 全部播放列表摘要，按最近更新倒序。
    pub fn list_playlists(&self) -> AppResult<Vec<PlaylistSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT p.id, p.name, p.description, p.created_at, p.updated_at,
                        (SELECT count(*) FROM playlist_tracks pt WHERE pt.playlist_id = p.id) AS track_count
                 FROM playlists p
                 ORDER BY p.updated_at DESC, p.name COLLATE NOCASE",
            )
            .map_err(db_err)?;
        let rows = stmt.query_map([], row_to_playlist_summary).map_err(db_err)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(db_err)?);
        }
        Ok(out)
    }

    fn get_playlist_summary(&self, id: &str) -> AppResult<PlaylistSummary> {
        self.conn
            .query_row(
                "SELECT p.id, p.name, p.description, p.created_at, p.updated_at,
                        (SELECT count(*) FROM playlist_tracks pt WHERE pt.playlist_id = p.id) AS track_count
                 FROM playlists p WHERE p.id = ?1",
                [id],
                row_to_playlist_summary,
            )
            .optional()
            .map_err(db_err)?
            .ok_or_else(|| AppError::NotFound(format!("playlist {id}")))
    }

    /// 重命名 / 改描述（空名拒绝）。
    pub fn rename_playlist(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
    ) -> AppResult<PlaylistSummary> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::InvalidArgument("playlist name is empty".into()));
        }
        let desc = description.map(str::trim).filter(|s| !s.is_empty());
        let now = Utc::now().to_rfc3339();
        let n = self
            .conn
            .execute(
                "UPDATE playlists SET name = ?2, description = ?3, updated_at = ?4 WHERE id = ?1",
                params![id, name, desc, now],
            )
            .map_err(db_err)?;
        if n == 0 {
            return Err(AppError::NotFound(format!("playlist {id}")));
        }
        self.get_playlist_summary(id)
    }

    /// 删除播放列表（playlist_tracks 由 ON DELETE CASCADE 清理）。
    pub fn delete_playlist(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM playlists WHERE id = ?1", [id])
            .map_err(db_err)?;
        Ok(())
    }

    /// 播放列表的全部曲目，按 position 排序；已不在库的条目自然被 INNER JOIN 滤掉。
    pub fn playlist_tracks(&self, id: &str) -> AppResult<Vec<Track>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT t.* FROM playlist_tracks pt
                 JOIN tracks t ON t.id = pt.track_id
                 WHERE pt.playlist_id = ?1
                 ORDER BY pt.position",
            )
            .map_err(db_err)?;
        collect_tracks(&mut stmt, [id])
    }

    /// 读取有序 track_id 列表（内部维护顺序用，含已删曲目的悬空条目）。
    fn playlist_track_ids(&self, id: &str) -> AppResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(db_err)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(db_err)?);
        }
        Ok(out)
    }

    /// 追加曲目到列表末尾（允许重复）；自动过滤库中不存在的 id。
    pub fn add_to_playlist(&self, id: &str, track_ids: &[String]) -> AppResult<()> {
        self.get_playlist_summary(id)?; // 列表不存在时报 NotFound
        let mut valid: Vec<&String> = Vec::new();
        for tid in track_ids {
            let exists = self
                .conn
                .query_row("SELECT 1 FROM tracks WHERE id = ?1", [tid], |_| Ok(()))
                .optional()
                .map_err(db_err)?
                .is_some();
            if exists {
                valid.push(tid);
            }
        }
        if valid.is_empty() {
            return Ok(());
        }
        let start: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?1",
                [id],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        self.conn.execute_batch("BEGIN").map_err(db_err)?;
        let res = (|| -> rusqlite::Result<()> {
            for (i, tid) in valid.iter().enumerate() {
                self.conn.execute(
                    "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
                    params![id, tid, start + i as i64],
                )?;
            }
            self.conn.execute(
                "UPDATE playlists SET updated_at = ?2 WHERE id = ?1",
                params![id, Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })();
        match res {
            Ok(()) => {
                self.conn.execute_batch("COMMIT").map_err(db_err)?;
                Ok(())
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(db_err(e))
            }
        }
    }

    /// 按展示顺序下标移除一项；越界无动作。
    pub fn remove_from_playlist(&self, id: &str, index: usize) -> AppResult<()> {
        let mut ids = self.playlist_track_ids(id)?;
        if index >= ids.len() {
            return Ok(());
        }
        ids.remove(index);
        self.rewrite_playlist(id, &ids)
    }

    /// 拖拽重排：把下标 from 移到 to；越界/同位无动作。
    pub fn move_in_playlist(&self, id: &str, from: usize, to: usize) -> AppResult<()> {
        let mut ids = self.playlist_track_ids(id)?;
        if from >= ids.len() || to >= ids.len() || from == to {
            return Ok(());
        }
        let item = ids.remove(from);
        ids.insert(to, item);
        self.rewrite_playlist(id, &ids)
    }

    /// 用给定有序 id 列表重写某列表的曲目（事务内 DELETE + 顺序 INSERT，位置 0..n）。
    fn rewrite_playlist(&self, id: &str, ids: &[String]) -> AppResult<()> {
        self.conn.execute_batch("BEGIN").map_err(db_err)?;
        let res = (|| -> rusqlite::Result<()> {
            self.conn
                .execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", [id])?;
            for (i, tid) in ids.iter().enumerate() {
                self.conn.execute(
                    "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
                    params![id, tid, i as i64],
                )?;
            }
            self.conn.execute(
                "UPDATE playlists SET updated_at = ?2 WHERE id = ?1",
                params![id, Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })();
        match res {
            Ok(()) => {
                self.conn.execute_batch("COMMIT").map_err(db_err)?;
                Ok(())
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(db_err(e))
            }
        }
    }

    // ---------- Settings ----------

    const SETTINGS_KEY: &'static str = "settings";
    const PLAYER_STATE_KEY: &'static str = "player_state";

    pub fn get_settings(&self) -> AppResult<Settings> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [Self::SETTINGS_KEY],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?;
        match raw {
            Some(text) => Ok(serde_json::from_str(&text)?),
            None => Ok(Settings::default()),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> AppResult<()> {
        let text = serde_json::to_string(settings)?;
        self.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![Self::SETTINGS_KEY, text],
            )
            .map_err(db_err)?;
        Ok(())
    }

    // ---------- 播放状态持久化 ----------

    /// 保存播放状态快照（退出时调用）。
    pub fn save_player_state(&self, state: &crate::models::player_state::PlayerState) -> AppResult<()> {
        let text = serde_json::to_string(state)?;
        self.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![Self::PLAYER_STATE_KEY, text],
            )
            .map_err(db_err)?;
        Ok(())
    }

    /// 加载播放状态快照（启动时调用）；未保存过或解析失败返回 None。
    pub fn load_player_state(&self) -> Option<crate::models::player_state::PlayerState> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [Self::PLAYER_STATE_KEY],
                |r| r.get(0),
            )
            .optional()
            .ok()?;
        raw.and_then(|text| serde_json::from_str(&text).ok())
    }

    // ---------- 视频续播位置 ----------

    /// 保存视频播放位置（key = "vpos:{path}"）。
    pub fn save_video_position(&self, path: &str, position_ms: u64) -> AppResult<()> {
        let key = format!("vpos:{path}");
        self.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, position_ms.to_string()],
            )
            .map_err(db_err)?;
        Ok(())
    }

    /// 获取视频上次播放位置；未保存过返回 0。
    pub fn get_video_position(&self, path: &str) -> u64 {
        let key = format!("vpos:{path}");
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [key],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }
}

// ---------- helpers ----------

fn db_err(e: rusqlite::Error) -> AppError {
    AppError::Db(e.to_string())
}

fn path_str(track: &Track) -> String {
    track.path.to_string_lossy().into_owned()
}

fn fmt_time(t: DateTime<Utc>) -> String {
    t.to_rfc3339()
}

fn parse_time(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn parse_time_opt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|t| t.with_timezone(&Utc))
}

/// LIKE 通配符转义（配合 `ESCAPE '\'`）。
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 专辑稳定标识：name + album_artist 的 FNV-1a(64) 哈希十六进制。
/// 仅用于前端 React key/选中态，不参与后端查询（曲目按原始字段过滤）。
fn album_id(name: Option<&str>, album_artist: Option<&str>) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    // \u{1f}（单元分隔符）隔开两字段，避免 ("ab","c") 与 ("a","bc") 撞键；
    // None 与空串用不同哨兵区分。
    for part in [name.unwrap_or("\u{0}"), "\u{1f}", album_artist.unwrap_or("\u{0}")] {
        for b in part.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

fn row_to_track(row: &Row) -> rusqlite::Result<Track> {
    let cue_path: Option<String> = row.get("cue_path")?;
    let cue_source = cue_path.map(|p| -> rusqlite::Result<CueSource> {
        Ok(CueSource {
            cue_path: std::path::PathBuf::from(p),
            cue_index: row.get::<_, i64>("cue_index")? as u32,
            start_ms: row.get::<_, Option<i64>>("cue_start_ms")?.unwrap_or(0) as u64,
            end_ms: row.get::<_, Option<i64>>("cue_end_ms")?.map(|v| v as u64),
        })
    }).transpose()?;
    Ok(Track {
        id: row.get("id")?,
        path: std::path::PathBuf::from(row.get::<_, String>("path")?),
        title: row.get("title")?,
        artist: row.get("artist")?,
        album_artist: row.get("album_artist")?,
        album: row.get("album")?,
        track_number: row.get("track_number")?,
        disc_number: row.get("disc_number")?,
        year: row.get("year")?,
        genre: row.get("genre")?,
        duration_ms: row.get::<_, i64>("duration_ms")? as u64,
        format: MediaFormat::from_str_lossy(&row.get::<_, String>("format")?),
        bitrate: row.get::<_, i64>("bitrate")? as u32,
        sample_rate: row.get::<_, i64>("sample_rate")? as u32,
        bit_depth: row.get("bit_depth")?,
        channels: row.get("channels")?,
        is_lossless: row.get::<_, i64>("is_lossless")? != 0,
        has_embedded_cover: row.get::<_, i64>("has_embedded_cover")? != 0,
        play_count: row.get::<_, i64>("play_count")? as u32,
        last_played: parse_time_opt(row.get("last_played")?),
        date_added: parse_time(row.get("date_added")?),
        file_modified: parse_time(row.get("file_modified")?),
        cue_source,
    })
}

fn collect_tracks(
    stmt: &mut rusqlite::Statement,
    p: impl rusqlite::Params,
) -> AppResult<Vec<Track>> {
    let rows = stmt.query_map(p, row_to_track).map_err(db_err)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(db_err)?);
    }
    Ok(out)
}

fn row_to_playlist_summary(row: &Row) -> rusqlite::Result<PlaylistSummary> {
    Ok(PlaylistSummary {
        id: row.get("id")?,
        name: row.get("name")?,
        description: row.get("description")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        track_count: row.get::<_, i64>("track_count")? as u64,
    })
}

/// 单条 upsert（Store::upsert_track 与事务批量共用）。
/// 冲突键 (path, cue_index)：cue_index=0 为普通整文件曲目。
fn upsert_one(conn: &Connection, track: &Track) -> AppResult<()> {
    let cue = track.cue_source.as_ref();
    conn.execute(
        "INSERT INTO tracks (
            id, path, title, artist, album_artist, album, track_number, disc_number,
            year, genre, duration_ms, format, bitrate, sample_rate, bit_depth, channels,
            is_lossless, has_embedded_cover, play_count, last_played, date_added, file_modified,
            cue_path, cue_index, cue_start_ms, cue_end_ms
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
            ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26
         )
         ON CONFLICT(path, cue_index) DO UPDATE SET
            title = excluded.title, artist = excluded.artist,
            album_artist = excluded.album_artist, album = excluded.album,
            track_number = excluded.track_number, disc_number = excluded.disc_number,
            year = excluded.year, genre = excluded.genre,
            duration_ms = excluded.duration_ms, format = excluded.format,
            bitrate = excluded.bitrate, sample_rate = excluded.sample_rate,
            bit_depth = excluded.bit_depth, channels = excluded.channels,
            is_lossless = excluded.is_lossless,
            has_embedded_cover = excluded.has_embedded_cover,
            file_modified = excluded.file_modified,
            cue_path = excluded.cue_path,
            cue_start_ms = excluded.cue_start_ms,
            cue_end_ms = excluded.cue_end_ms",
        params![
            track.id,
            path_str(track),
            track.title,
            track.artist,
            track.album_artist,
            track.album,
            track.track_number,
            track.disc_number,
            track.year,
            track.genre,
            track.duration_ms as i64,
            track.format.as_str(),
            track.bitrate as i64,
            track.sample_rate as i64,
            track.bit_depth,
            track.channels,
            track.is_lossless as i64,
            track.has_embedded_cover as i64,
            track.play_count as i64,
            track.last_played.map(fmt_time),
            fmt_time(track.date_added),
            fmt_time(track.file_modified),
            cue.map(|c| c.cue_path.to_string_lossy().into_owned()),
            cue.map(|c| c.cue_index as i64).unwrap_or(0),
            cue.map(|c| c.start_ms as i64),
            cue.and_then(|c| c.end_ms).map(|v| v as i64),
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_track(id: &str, path: &str, title: &str, artist: Option<&str>) -> Track {
        Track {
            id: id.to_string(),
            path: PathBuf::from(path),
            title: title.to_string(),
            artist: artist.map(str::to_string),
            album_artist: None,
            album: Some("叶惠美".to_string()),
            track_number: Some(1),
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
            date_added: Utc::now(),
            file_modified: Utc::now(),
            cue_source: None,
        }
    }

    #[test]
    fn upsert_and_get_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let t = sample_track("id-1", "D:/Music/a.flac", "晴天", Some("周杰伦"));
        store.upsert_track(&t).unwrap();
        let got = store.get_track("id-1").unwrap();
        assert_eq!(got.title, "晴天");
        assert_eq!(got.artist.as_deref(), Some("周杰伦"));
        assert_eq!(got.format, MediaFormat::Flac);
        assert!(got.is_lossless);
        assert!(store.get_track("nope").is_err());
    }

    #[test]
    fn upsert_is_idempotent_by_path_and_preserves_play_fields() {
        let store = Store::open_in_memory().unwrap();
        let t = sample_track("id-1", "D:/Music/a.flac", "晴天", Some("周杰伦"));
        store.upsert_track(&t).unwrap();

        // 人工累计播放数据
        store
            .conn
            .execute(
                "UPDATE tracks SET play_count = 5, last_played = '2026-07-01T00:00:00.000Z' WHERE id = 'id-1'",
                [],
            )
            .unwrap();

        // 同 path 再扫描入库（新 id、改标题）：不新增行，play_count 保留
        let mut t2 = sample_track(
            "id-2",
            "D:/Music/a.flac",
            "晴天 (Remastered)",
            Some("周杰伦"),
        );
        t2.play_count = 0;
        store.upsert_track(&t2).unwrap();

        let page = store.list_tracks(&ListQuery::default()).unwrap();
        assert_eq!(page.total, 1);
        let got = store.get_track("id-1").unwrap();
        assert_eq!(got.title, "晴天 (Remastered)");
        assert_eq!(got.play_count, 5);
        assert!(got.last_played.is_some());
    }

    #[test]
    fn batch_upsert_and_stats() {
        let store = Store::open_in_memory().unwrap();
        let tracks = vec![
            sample_track("1", "D:/m/a.flac", "A", None),
            sample_track("2", "D:/m/b.mp3", "B", None),
            sample_track("3", "D:/m/c.wav", "C", None),
        ];
        assert_eq!(store.upsert_tracks(&tracks).unwrap(), 3);
        let stats = store.stats().unwrap();
        assert_eq!(stats.track_count, 3);
        assert_eq!(stats.total_duration_ms, 269_000 * 3);
        // sample 全部是 flac（无损标记由调用方设置），lossless = 3
        assert_eq!(stats.lossless_count, 3);
    }

    #[test]
    fn pagination_and_sorting() {
        let store = Store::open_in_memory().unwrap();
        let tracks: Vec<Track> = (0..5)
            .map(|i| {
                sample_track(
                    &format!("{i}"),
                    &format!("D:/m/{i}.flac"),
                    &format!("T{0}{0}{0}", 5 - i),
                    None,
                )
            })
            .collect();
        store.upsert_tracks(&tracks).unwrap();

        let q = ListQuery {
            offset: 1,
            limit: 2,
            sort_by: Some("title".into()),
            sort_dir: Some("asc".into()),
            search: None,
        };
        let page = store.list_tracks(&q).unwrap();
        assert_eq!(page.total, 5);
        assert_eq!(page.tracks.len(), 2);
        assert_eq!(page.tracks[0].title, "T222");

        // 排序字段白名单
        let bad = ListQuery {
            sort_by: Some("title; DROP TABLE tracks".into()),
            ..Default::default()
        };
        assert!(store.list_tracks(&bad).is_err());
    }

    #[test]
    fn fts_trigram_matches_chinese_substring() {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_track(&sample_track(
                "1",
                "D:/m/a.flac",
                "最长的电影",
                Some("周杰伦"),
            ))
            .unwrap();
        store
            .upsert_track(&sample_track("2", "D:/m/b.flac", "青花瓷", Some("周杰伦")))
            .unwrap();

        // 3 字子串（跨"词"边界）能命中 —— unicode61 做不到
        let q = ListQuery {
            search: Some("的电影".into()),
            ..Default::default()
        };
        let page = store.list_tracks(&q).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.tracks[0].title, "最长的电影");

        // 按艺术家搜
        let q2 = ListQuery {
            search: Some("周杰伦".into()),
            ..Default::default()
        };
        assert_eq!(store.list_tracks(&q2).unwrap().total, 2);
    }

    #[test]
    fn short_search_falls_back_to_like() {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_track(&sample_track("1", "D:/m/a.flac", "晴天", Some("周杰伦")))
            .unwrap();
        // 2 字查询（trigram 覆盖不到）走 LIKE
        let q = ListQuery {
            search: Some("晴天".into()),
            ..Default::default()
        };
        assert_eq!(store.list_tracks(&q).unwrap().total, 1);
        let none = ListQuery {
            search: Some("不存在的".into()),
            ..Default::default()
        };
        // "不存在"是 3 字走 FTS，"的"拆开不存在完整 trigram
        assert_eq!(store.list_tracks(&none).unwrap().total, 0);
    }

    #[test]
    fn paths_under_and_delete() {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_tracks(&[
                sample_track("1", "D:/Music/a.flac", "A", None),
                sample_track("2", "D:/Music/sub/b.flac", "B", None),
                sample_track("3", "E:/Other/c.flac", "C", None),
            ])
            .unwrap();
        let mut paths = store.paths_under("D:/Music").unwrap();
        paths.sort();
        assert_eq!(paths.len(), 2);
        store.delete_by_path("D:/Music/a.flac").unwrap();
        assert_eq!(store.paths_under("D:/Music").unwrap().len(), 1);
    }

    #[test]
    fn settings_roundtrip_and_default() {
        let store = Store::open_in_memory().unwrap();
        // 未写过时返回默认
        assert_eq!(store.get_settings().unwrap(), Settings::default());

        let s = Settings {
            theme: crate::models::settings::Theme::Light,
            scan_dirs: vec!["D:/Music".into()],
            ..Default::default()
        };
        store.save_settings(&s).unwrap();
        assert_eq!(store.get_settings().unwrap(), s);
    }

    /// 构造带专辑归属的曲目（在 sample_track 基础上覆盖分组相关字段）。
    fn album_track(
        id: &str,
        title: &str,
        album: Option<&str>,
        album_artist: Option<&str>,
        artist: Option<&str>,
        track_number: Option<u32>,
        duration_ms: u64,
    ) -> Track {
        let mut t = sample_track(id, &format!("D:/m/{id}.flac"), title, artist);
        t.album = album.map(str::to_string);
        t.album_artist = album_artist.map(str::to_string);
        t.track_number = track_number;
        t.duration_ms = duration_ms;
        t
    }

    #[test]
    fn albums_and_artists_aggregate() {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_tracks(&[
                album_track("1", "以父之名", Some("叶惠美"), Some("周杰伦"), Some("周杰伦"), Some(1), 1000),
                album_track("2", "晴天", Some("叶惠美"), Some("周杰伦"), Some("周杰伦"), Some(2), 2000),
                album_track("3", "简单爱", Some("范特西"), Some("周杰伦"), Some("周杰伦"), Some(1), 3000),
                album_track("4", "无名", None, None, None, None, 500),
            ])
            .unwrap();

        let albums = store.albums().unwrap();
        assert_eq!(albums.len(), 3); // 叶惠美 / 范特西 / 未知专辑
        let ye = albums
            .iter()
            .find(|a| a.name.as_deref() == Some("叶惠美"))
            .unwrap();
        assert_eq!(ye.album_artist.as_deref(), Some("周杰伦"));
        assert_eq!(ye.track_count, 2);
        assert_eq!(ye.total_duration_ms, 3000);
        assert_eq!(ye.year, Some(2003));
        assert_eq!(ye.cover_track_id, "1"); // 首轨（track_number=1）作代表封面
        assert!(albums.iter().any(|a| a.name.is_none())); // 未知专辑成组
        // 稳定 id：同名同艺术家哈希一致，且不同专辑不撞
        assert_eq!(ye.id, super::album_id(Some("叶惠美"), Some("周杰伦")));
        assert_ne!(albums[0].id, albums[1].id);

        let artists = store.artists().unwrap();
        let zjl = artists
            .iter()
            .find(|a| a.name.as_deref() == Some("周杰伦"))
            .unwrap();
        assert_eq!(zjl.album_count, 2);
        assert_eq!(zjl.track_count, 3);
        assert_eq!(zjl.total_duration_ms, 6000);
        assert!(artists.iter().any(|a| a.name.is_none())); // 未知艺术家成组
    }

    #[test]
    fn album_and_artist_tracks_filter_and_order() {
        let store = Store::open_in_memory().unwrap();
        // 乱序入库，验证查询侧排序
        store
            .upsert_tracks(&[
                album_track("2", "晴天", Some("叶惠美"), Some("周杰伦"), Some("周杰伦"), Some(2), 2000),
                album_track("1", "以父之名", Some("叶惠美"), Some("周杰伦"), Some("周杰伦"), Some(1), 1000),
                album_track("3", "简单爱", Some("范特西"), Some("周杰伦"), Some("周杰伦"), Some(1), 3000),
                album_track("9", "散轨", None, None, None, None, 500),
            ])
            .unwrap();

        // 专辑内按轨号升序
        let ye = store.album_tracks(Some("叶惠美"), Some("周杰伦")).unwrap();
        assert_eq!(
            ye.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["1", "2"]
        );
        // 未知专辑（None,None）可命中散轨
        let unknown = store.album_tracks(None, None).unwrap();
        assert_eq!(unknown.len(), 1);
        assert_eq!(unknown[0].id, "9");

        // 艺术家全部曲目跨专辑，按专辑名再轨号排序
        // （叶 U+53F6 < 范 U+8303，故叶惠美 在 范特西 之前）。
        let all = store.artist_tracks(Some("周杰伦")).unwrap();
        assert_eq!(
            all.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["1", "2", "3"]
        );
        assert_eq!(all[0].album.as_deref(), Some("叶惠美"));
    }

    #[test]
    fn playlist_crud_and_ordering() {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_tracks(&[
                sample_track("t1", "D:/m/1.flac", "A", Some("X")),
                sample_track("t2", "D:/m/2.flac", "B", Some("X")),
                sample_track("t3", "D:/m/3.flac", "C", Some("X")),
            ])
            .unwrap();

        let pl = store.create_playlist("我的歌单", None).unwrap();
        assert_eq!(pl.name, "我的歌单");
        assert_eq!(pl.track_count, 0);
        assert!(store.create_playlist("   ", None).is_err()); // 空名拒绝

        // 添加（允许重复），非法 id 被过滤
        store
            .add_to_playlist(&pl.id, &["t1".into(), "t2".into(), "nope".into(), "t1".into()])
            .unwrap();
        let ids: Vec<String> = store
            .playlist_tracks(&pl.id)
            .unwrap()
            .iter()
            .map(|t| t.id.clone())
            .collect();
        assert_eq!(ids, ["t1", "t2", "t1"]);
        assert_eq!(store.list_playlists().unwrap()[0].track_count, 3);

        // 移动：下标 0 移到末尾 -> [t2, t1, t1]
        store.move_in_playlist(&pl.id, 0, 2).unwrap();
        let ids: Vec<String> = store
            .playlist_tracks(&pl.id)
            .unwrap()
            .iter()
            .map(|t| t.id.clone())
            .collect();
        assert_eq!(ids, ["t2", "t1", "t1"]);

        // 移除下标 1 -> [t2, t1]；越界无动作
        store.remove_from_playlist(&pl.id, 1).unwrap();
        store.remove_from_playlist(&pl.id, 9).unwrap();
        let ids: Vec<String> = store
            .playlist_tracks(&pl.id)
            .unwrap()
            .iter()
            .map(|t| t.id.clone())
            .collect();
        assert_eq!(ids, ["t2", "t1"]);

        // 重命名 + 改描述
        let renamed = store.rename_playlist(&pl.id, "新名字", Some("desc")).unwrap();
        assert_eq!(renamed.name, "新名字");
        assert_eq!(renamed.description.as_deref(), Some("desc"));

        // 删除曲目 t1 → 外键级联从歌单出列（仅余 t2）
        store.delete_by_path("D:/m/1.flac").unwrap();
        let ids: Vec<String> = store
            .playlist_tracks(&pl.id)
            .unwrap()
            .iter()
            .map(|t| t.id.clone())
            .collect();
        assert_eq!(ids, ["t2"]);

        // 删除歌单
        store.delete_playlist(&pl.id).unwrap();
        assert!(store.list_playlists().unwrap().is_empty());
        assert!(store.playlist_tracks(&pl.id).unwrap().is_empty());
    }

    #[test]
    fn play_stats_record_recent_and_top() {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_tracks(&[
                sample_track("a", "D:/m/a.flac", "A", Some("X")),
                sample_track("b", "D:/m/b.flac", "B", Some("X")),
                sample_track("c", "D:/m/c.flac", "C", Some("X")),
            ])
            .unwrap();

        // 未播放：概览为 0，列表为空
        let s0 = store.stats_summary().unwrap();
        assert_eq!(s0.track_count, 3);
        assert_eq!(s0.total_plays, 0);
        assert_eq!(s0.played_count, 0);
        assert_eq!(s0.formats.iter().map(|f| f.count).sum::<u64>(), 3);
        assert!(store.recently_played(10).unwrap().is_empty());
        assert!(store.most_played(10).unwrap().is_empty());

        // 记录：a×3, b×1（c 未播）
        for _ in 0..3 {
            store.record_play("a").unwrap();
        }
        store.record_play("b").unwrap();

        let s = store.stats_summary().unwrap();
        assert_eq!(s.total_plays, 4);
        assert_eq!(s.played_count, 2);

        // 常听：按次数确定性排序（a=3 > b=1）
        let top = store.most_played(10).unwrap();
        assert_eq!(
            top.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(store.get_track("a").unwrap().play_count, 3);

        // 最近：用确定时间戳验证倒序（避免时钟分辨率导致的不确定）
        store
            .conn
            .execute("UPDATE tracks SET last_played='2026-01-01T00:00:00Z' WHERE id='a'", [])
            .unwrap();
        store
            .conn
            .execute("UPDATE tracks SET last_played='2026-01-02T00:00:00Z' WHERE id='b'", [])
            .unwrap();
        let recent = store.recently_played(10).unwrap();
        assert_eq!(
            recent.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
    }
}
