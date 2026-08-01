//! schema 版本迁移（user_version PRAGMA）。
//!
//! 迁移按顺序定义在 `MIGRATIONS` 中，下标 + 1 即目标版本号。
//! `run` 读取 `PRAGMA user_version`，把落后的迁移依次在事务里执行。
//! 新增 schema 变更时只追加，不修改已有迁移。

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

/// v1：tracks 表、settings kv 表、FTS5(trigram) 虚表 + 同步触发器。
///
/// - FTS tokenizer 刻意选 `trigram` 而非 unicode61：unicode61 对中文无分词，
///   整句会成为一个 token，无法做子串匹配；trigram 以 3 字符滑窗建索引，
///   中文/英文子串（>= 3 字符）都能命中。
/// - `tracks.id` 为 TEXT 主键（uuid），表保留隐式 rowid，FTS 通过 rowid 关联。
/// - 触发器保证 tracks 增/删/改时 FTS 同步，业务代码无需手工维护索引。
const V1: &str = r#"
CREATE TABLE tracks (
    id                 TEXT PRIMARY KEY,
    path               TEXT NOT NULL UNIQUE,
    title              TEXT NOT NULL,
    artist             TEXT,
    album_artist       TEXT,
    album              TEXT,
    track_number       INTEGER,
    disc_number        INTEGER,
    year               INTEGER,
    genre              TEXT,
    duration_ms        INTEGER NOT NULL DEFAULT 0,
    format             TEXT NOT NULL,
    bitrate            INTEGER NOT NULL DEFAULT 0,
    sample_rate        INTEGER NOT NULL DEFAULT 0,
    bit_depth          INTEGER,
    channels           INTEGER,
    is_lossless        INTEGER NOT NULL DEFAULT 0,
    has_embedded_cover INTEGER NOT NULL DEFAULT 0,
    play_count         INTEGER NOT NULL DEFAULT 0,
    last_played        TEXT,
    date_added         TEXT NOT NULL,
    file_modified      TEXT NOT NULL
);

CREATE INDEX idx_tracks_artist ON tracks(artist);
CREATE INDEX idx_tracks_album ON tracks(album);
CREATE INDEX idx_tracks_date_added ON tracks(date_added);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL  -- JSON
);

CREATE VIRTUAL TABLE tracks_fts USING fts5(
    title,
    artist,
    album,
    content = 'tracks',
    tokenize = 'trigram'
);

CREATE TRIGGER tracks_fts_insert AFTER INSERT ON tracks BEGIN
    INSERT INTO tracks_fts(rowid, title, artist, album)
    VALUES (new.rowid, new.title, coalesce(new.artist, ''), coalesce(new.album, ''));
END;

CREATE TRIGGER tracks_fts_delete AFTER DELETE ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album)
    VALUES ('delete', old.rowid, old.title, coalesce(old.artist, ''), coalesce(old.album, ''));
END;

CREATE TRIGGER tracks_fts_update AFTER UPDATE ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album)
    VALUES ('delete', old.rowid, old.title, coalesce(old.artist, ''), coalesce(old.album, ''));
    INSERT INTO tracks_fts(rowid, title, artist, album)
    VALUES (new.rowid, new.title, coalesce(new.artist, ''), coalesce(new.album, ''));
END;
"#;

/// 全部迁移，按版本顺序排列。
const MIGRATIONS: &[&str] = &[V1, V2, V3];

/// v2（CUE 整轨）：tracks 增加 cue_path / cue_index / cue_start_ms / cue_end_ms，
/// 唯一键从 `path` 改为 `(path, cue_index)`（同一整轨文件承载多个虚拟曲目，
/// cue_index=0 表示普通整文件曲目）。SQLite 不能改 UNIQUE 约束，重建表后
/// 用 FTS `rebuild` 同步外部内容索引（rowid 已变）。
const V2: &str = r#"
DROP TRIGGER tracks_fts_insert;
DROP TRIGGER tracks_fts_delete;
DROP TRIGGER tracks_fts_update;

CREATE TABLE tracks_v2 (
    id                 TEXT PRIMARY KEY,
    path               TEXT NOT NULL,
    title              TEXT NOT NULL,
    artist             TEXT,
    album_artist       TEXT,
    album              TEXT,
    track_number       INTEGER,
    disc_number        INTEGER,
    year               INTEGER,
    genre              TEXT,
    duration_ms        INTEGER NOT NULL DEFAULT 0,
    format             TEXT NOT NULL,
    bitrate            INTEGER NOT NULL DEFAULT 0,
    sample_rate        INTEGER NOT NULL DEFAULT 0,
    bit_depth          INTEGER,
    channels           INTEGER,
    is_lossless        INTEGER NOT NULL DEFAULT 0,
    has_embedded_cover INTEGER NOT NULL DEFAULT 0,
    play_count         INTEGER NOT NULL DEFAULT 0,
    last_played        TEXT,
    date_added         TEXT NOT NULL,
    file_modified      TEXT NOT NULL,
    cue_path           TEXT,
    cue_index          INTEGER NOT NULL DEFAULT 0,
    cue_start_ms       INTEGER,
    cue_end_ms         INTEGER,
    UNIQUE(path, cue_index)
);

INSERT INTO tracks_v2 (
    id, path, title, artist, album_artist, album, track_number, disc_number,
    year, genre, duration_ms, format, bitrate, sample_rate, bit_depth, channels,
    is_lossless, has_embedded_cover, play_count, last_played, date_added, file_modified
) SELECT
    id, path, title, artist, album_artist, album, track_number, disc_number,
    year, genre, duration_ms, format, bitrate, sample_rate, bit_depth, channels,
    is_lossless, has_embedded_cover, play_count, last_played, date_added, file_modified
  FROM tracks;

DROP TABLE tracks;
ALTER TABLE tracks_v2 RENAME TO tracks;

CREATE INDEX idx_tracks_artist ON tracks(artist);
CREATE INDEX idx_tracks_album ON tracks(album);
CREATE INDEX idx_tracks_date_added ON tracks(date_added);
CREATE INDEX idx_tracks_cue_path ON tracks(cue_path);

INSERT INTO tracks_fts(tracks_fts) VALUES('rebuild');

CREATE TRIGGER tracks_fts_insert AFTER INSERT ON tracks BEGIN
    INSERT INTO tracks_fts(rowid, title, artist, album)
    VALUES (new.rowid, new.title, coalesce(new.artist, ''), coalesce(new.album, ''));
END;

CREATE TRIGGER tracks_fts_delete AFTER DELETE ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album)
    VALUES ('delete', old.rowid, old.title, coalesce(old.artist, ''), coalesce(old.album, ''));
END;

CREATE TRIGGER tracks_fts_update AFTER UPDATE ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album)
    VALUES ('delete', old.rowid, old.title, coalesce(old.artist, ''), coalesce(old.album, ''));
    INSERT INTO tracks_fts(rowid, title, artist, album)
    VALUES (new.rowid, new.title, coalesce(new.artist, ''), coalesce(new.album, ''));
END;
"#;

/// v3（播放列表）：playlists 元信息 + playlist_tracks 有序关联。
///
/// - `playlist_tracks` 主键 (playlist_id, position) 保证同一列表内位置唯一，按 position 排序；
/// - 两条外键均 ON DELETE CASCADE：删列表清空其曲目；曲目被删（文件移除/CUE 失效）时自动出列；
///   级联需连接开启 `PRAGMA foreign_keys=ON`（Store::open 与测试内存库均已设置）。
/// - position 可能因级联删除出现空洞，读取只依赖 ORDER BY position，增删改由业务层重写位置修复。
const V3: &str = r#"
CREATE TABLE playlists (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE playlist_tracks (
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    track_id    TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, position)
);

CREATE INDEX idx_playlist_tracks_track ON playlist_tracks(track_id);
"#;

/// 把数据库迁移到最新版本。幂等：已是最新时什么都不做。
pub fn run(conn: &Connection) -> AppResult<()> {
    let current: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(db_err)?;

    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let target = (i + 1) as u32;
        if current < target {
            conn.execute_batch("BEGIN").map_err(db_err)?;
            let result = conn
                .execute_batch(sql)
                .and_then(|()| conn.pragma_update(None, "user_version", target));
            match result {
                Ok(()) => conn.execute_batch("COMMIT").map_err(db_err)?,
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(db_err(e));
                }
            }
            tracing::info!(version = target, "database migrated");
        }
    }
    Ok(())
}

fn db_err(e: rusqlite::Error) -> AppError {
    AppError::Db(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    fn user_version(conn: &Connection) -> u32 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = ?1",
            [name],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            > 0
    }

    #[test]
    fn fresh_db_migrates_to_latest() {
        let conn = mem();
        assert_eq!(user_version(&conn), 0);
        run(&conn).unwrap();
        assert_eq!(user_version(&conn), MIGRATIONS.len() as u32);

        for name in ["tracks", "settings", "tracks_fts", "playlists", "playlist_tracks"] {
            assert!(table_exists(&conn, name), "missing table {name}");
        }
        for trigger in [
            "tracks_fts_insert",
            "tracks_fts_delete",
            "tracks_fts_update",
        ] {
            assert!(table_exists(&conn, trigger), "missing trigger {trigger}");
        }
    }

    #[test]
    fn migration_is_idempotent_and_preserves_data() {
        let conn = mem();
        run(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, format, date_added, file_modified)
             VALUES ('a', '/x.flac', 't', 'flac', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [],
        )
        .unwrap();

        // 重复执行不报错、数据不丢、版本不变
        run(&conn).unwrap();
        run(&conn).unwrap();
        assert_eq!(user_version(&conn), MIGRATIONS.len() as u32);
        let n: i64 = conn
            .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn v2_upgrade_preserves_rows_and_allows_shared_path() {
        let conn = mem();
        // 先只跑 v1，写入数据（含播放统计）
        conn.execute_batch("BEGIN").unwrap();
        conn.execute_batch(super::V1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute_batch("COMMIT").unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, format, play_count, date_added, file_modified)
             VALUES ('a', '/m/整轨.flac', '整轨', '周杰伦', 'flac', 7, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [],
        )
        .unwrap();

        // 升级到最新：数据保留，cue 字段为默认值
        run(&conn).unwrap();
        let (count, cue_index, play_count): (i64, i64, i64) = conn
            .query_row(
                "SELECT count(*), cue_index, play_count FROM tracks WHERE id = 'a'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((count, cue_index, play_count), (1, 0, 7));

        // 同 path 不同 cue_index 可共存；重复 (path, cue_index) 被拒
        conn.execute(
            "INSERT INTO tracks (id, path, title, format, cue_path, cue_index, cue_start_ms, date_added, file_modified)
             VALUES ('b', '/m/整轨.flac', '第一轨', 'flac', '/m/整轨.cue', 1, 0, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [],
        )
        .unwrap();
        let dup = conn.execute(
            "INSERT INTO tracks (id, path, title, format, cue_index, date_added, file_modified)
             VALUES ('c', '/m/整轨.flac', 'x', 'flac', 0, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [],
        );
        assert!(dup.is_err());

        // FTS 索引在重建后仍同步（旧数据 rebuild + 新数据触发器）
        let hits: i64 = conn
            .query_row(
                "SELECT count(*) FROM tracks_fts WHERE tracks_fts MATCH '\"周杰伦\"'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
        let hits2: i64 = conn
            .query_row(
                "SELECT count(*) FROM tracks_fts WHERE tracks_fts MATCH '\"第一轨\"'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits2, 1);
    }

    #[test]
    fn fts_triggers_keep_index_in_sync() {
        let conn = mem();
        run(&conn).unwrap();

        let insert = "INSERT INTO tracks (id, path, title, artist, format, date_added, file_modified)
                      VALUES (?1, ?2, ?3, ?4, 'flac', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')";
        conn.execute(insert, ("a", "/最长的电影.flac", "最长的电影", "周杰伦"))
            .unwrap();

        let search = |q: &str| -> i64 {
            conn.query_row(
                "SELECT count(*) FROM tracks_fts WHERE tracks_fts MATCH ?1",
                [q],
                |r| r.get(0),
            )
            .unwrap()
        };

        // trigram：跨"分词"边界的 3 字子串也能命中（unicode61 做不到）
        assert_eq!(search("\"的电影\""), 1);
        assert_eq!(search("\"周杰伦\""), 1);

        // UPDATE 后旧内容搜不到、新内容能搜到
        conn.execute("UPDATE tracks SET title = '青花瓷' WHERE id = 'a'", [])
            .unwrap();
        assert_eq!(search("\"的电影\""), 0);

        // DELETE 后索引同步清除
        conn.execute("DELETE FROM tracks WHERE id = 'a'", [])
            .unwrap();
        assert_eq!(search("\"周杰伦\""), 0);
    }
}
