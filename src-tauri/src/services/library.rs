//! 媒体库扫描服务：目录递归遍历 → 元数据读取 → 批量入库 → 失效条目清理。
//!
//! 本模块不依赖 Tauri：进度通过回调上报，取消通过 `AtomicBool`，
//! 便于脱离应用环境单测。事件节流（每 50 个文件一次）由调用方控制。

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use walkdir::WalkDir;

use crate::db::store::Store;
use crate::error::AppResult;
use crate::models::track::{CueSource, MediaFormat, Track};

use super::{cue, metadata};

/// 扫描进度事件 payload（契约事件 `library:scan-progress`）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanEvent {
    pub scanned_files: u64,
    pub total_files: Option<u64>,
    pub current_path: String,
    pub done: bool,
    pub error: Option<String>,
}

impl ScanEvent {
    fn progress(scanned: u64, path: &str) -> Self {
        ScanEvent {
            scanned_files: scanned,
            total_files: None,
            current_path: path.to_string(),
            done: false,
            error: None,
        }
    }

    fn finished(scanned: u64) -> Self {
        ScanEvent {
            scanned_files: scanned,
            total_files: Some(scanned),
            current_path: String::new(),
            done: true,
            error: None,
        }
    }
}

/// 扫描结果统计。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScanOutcome {
    pub indexed: u64,
    pub skipped_errors: u64,
    pub pruned: u64,
    pub cancelled: bool,
}

/// 扫描一组目录并同步到数据库。
///
/// - 已入库条目按 path upsert（幂等，重扫安全）。
/// - 目录下已消失的文件从库中清除（仅针对本次扫描目录前缀）。
/// - `cancel` 置位时在下一个文件边界停止并返回 `cancelled = true`。
pub fn scan(
    dirs: &[String],
    store: &Store,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(ScanEvent),
) -> AppResult<ScanOutcome> {
    let mut outcome = ScanOutcome::default();
    let mut batch: Vec<Track> = Vec::with_capacity(200);
    let mut scanned: u64 = 0;

    'dirs: for dir in dirs {
        let mut seen: HashSet<String> = HashSet::new();

        // ---- CUE 预扫描：生成虚拟曲目，记录被覆盖的整轨文件 ----
        let mut covered: HashSet<String> = HashSet::new();
        let mut alive_cues: HashSet<String> = HashSet::new();
        for entry in WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if cancel.load(Ordering::Relaxed) {
                outcome.cancelled = true;
                break 'dirs;
            }
            if !entry.file_type().is_file() || !is_cue(entry.path()) {
                continue;
            }
            let cue_str = entry.path().to_string_lossy().into_owned();
            match cue_virtual_tracks(entry.path()) {
                Ok(groups) if !groups.is_empty() => {
                    alive_cues.insert(cue_str.clone());
                    for (audio_path, tracks) in groups {
                        // 清掉已从 CUE 消失的轨号；整文件行不再单独展示
                        let keep: Vec<u32> = tracks
                            .iter()
                            .filter_map(|t| t.cue_source.as_ref().map(|c| c.cue_index))
                            .collect();
                        store.prune_cue_indexes(&cue_str, &keep)?;
                        store.delete_whole_file_track(&audio_path)?;
                        covered.insert(audio_path.clone());
                        seen.insert(audio_path);
                        batch.extend(tracks);
                    }
                }
                Ok(_) => {
                    // 解析成功但无有效轨（音频缺失等）：视为失效，下方统一清理
                    tracing::warn!(path = %cue_str, "cue has no playable tracks");
                }
                Err(e) => {
                    outcome.skipped_errors += 1;
                    tracing::warn!(path = %cue_str, error = %e, "skip unreadable cue");
                }
            }
        }
        // 已删除/失效的 CUE：清理其全部虚拟曲目
        for old_cue in store.cue_paths_under(dir)? {
            if !alive_cues.contains(&old_cue) {
                outcome.pruned += store.delete_by_cue_path(&old_cue)?;
            }
        }

        // ---- 音频文件扫描（被 CUE 覆盖的跳过） ----
        for entry in WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if cancel.load(Ordering::Relaxed) {
                outcome.cancelled = true;
                break 'dirs;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if MediaFormat::from_path(path).is_none() {
                continue;
            }

            let path_str = path.to_string_lossy().into_owned();
            if covered.contains(&path_str) {
                continue; // 虚拟曲目已入 batch，seen 已登记
            }
            seen.insert(path_str.clone());
            scanned += 1;
            if scanned.is_multiple_of(50) {
                on_progress(ScanEvent::progress(scanned, &path_str));
            }

            match metadata::read_track(path) {
                Ok(track) => {
                    batch.push(track);
                    if batch.len() >= 200 {
                        store.upsert_tracks(&batch)?;
                        outcome.indexed += batch.len() as u64;
                        batch.clear();
                    }
                }
                Err(e) => {
                    outcome.skipped_errors += 1;
                    tracing::warn!(path = %path_str, error = %e, "skip unreadable file");
                }
            }
        }

        // 清理该目录下已消失的条目
        for old in store.paths_under(dir)? {
            if !seen.contains(&old) {
                store.delete_by_path(&old)?;
                outcome.pruned += 1;
            }
        }
    }

    if !batch.is_empty() {
        store.upsert_tracks(&batch)?;
        outcome.indexed += batch.len() as u64;
    }

    on_progress(ScanEvent::finished(scanned));
    Ok(outcome)
}

/// 导入外部文件（命令行参数 / 文件关联打开）。
///
/// - 支持音频文件与 .cue 文件（cue 展开为全部虚拟曲目），其余静默跳过；
/// - 已入库的按 path 复用现有条目（id 不变；整轨文件命中 CUE 虚拟曲目时
///   返回全部分轨），未入库的读标签后插入；
/// - 单个文件读取失败不中断整批，记日志后继续。
pub fn import_files(store: &Store, paths: &[String]) -> AppResult<Vec<Track>> {
    let mut out = Vec::new();
    for raw in paths {
        // 统一成绝对路径（argv 可能是相对路径）；不用 canonicalize，
        // 避免 Windows \\?\ 前缀导致与扫描入库的路径形式不一致。
        let path = std::path::absolute(Path::new(raw)).unwrap_or_else(|_| Path::new(raw).into());
        if !path.is_file() {
            continue;
        }

        // .cue：展开为虚拟曲目入库（upsert 后回读，保留已有 id）
        if is_cue(&path) {
            match cue_virtual_tracks(&path) {
                Ok(groups) => {
                    for (audio_str, tracks) in groups {
                        store.delete_whole_file_track(&audio_str)?;
                        for t in &tracks {
                            store.upsert_track(t)?;
                        }
                        let rows = store.get_tracks_by_path(&audio_str)?;
                        out.extend(rows.into_iter().filter(|r| r.cue_source.is_some()));
                    }
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skip unreadable cue on import");
                }
            }
            continue;
        }

        if MediaFormat::from_path(&path).is_none() {
            continue;
        }
        let path_str = path.to_string_lossy().into_owned();
        let existing = store.get_tracks_by_path(&path_str)?;
        if !existing.is_empty() {
            // 整轨文件被 CUE 覆盖时返回全部分轨；普通文件即单条
            out.extend(existing);
            continue;
        }
        match metadata::read_track(&path) {
            Ok(track) => {
                store.upsert_track(&track)?;
                out.push(track);
            }
            Err(e) => {
                tracing::warn!(path = %path_str, error = %e, "skip unreadable file on import");
            }
        }
    }
    Ok(out)
}

/// 是否 .cue 文件（扩展名大小写不敏感）。
fn is_cue(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cue"))
}

/// 解析 CUE 并组装虚拟曲目，按整轨音频文件分组返回 `(音频路径, 曲目列表)`。
///
/// - FILE 相对路径基于 CUE 所在目录解析；音频缺失/格式不支持的 FILE 段跳过；
/// - 轨结束时间 = 同 FILE 内下一轨的起始；末轨为 None（播到文件尾）；
/// - 流属性（格式/码率/采样率等）继承自整轨文件，标签字段 CUE 优先。
fn cue_virtual_tracks(cue_path: &Path) -> AppResult<Vec<(String, Vec<Track>)>> {
    let sheet = cue::parse_file(cue_path)?;
    let parent = cue_path.parent().unwrap_or_else(|| Path::new(""));
    let mut out = Vec::new();

    for file in &sheet.files {
        let audio = {
            let p = Path::new(&file.audio);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                parent.join(p)
            }
        };
        if !audio.is_file() || MediaFormat::from_path(&audio).is_none() {
            tracing::warn!(cue = %cue_path.display(), audio = %audio.display(), "cue references missing/unsupported audio");
            continue;
        }
        let base = match metadata::read_track(&audio) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(audio = %audio.display(), error = %e, "skip unreadable cue audio");
                continue;
            }
        };

        let mut tracks = Vec::new();
        for (i, t) in file.tracks.iter().enumerate() {
            let end_ms = file.tracks.get(i + 1).map(|n| n.start_ms);
            let duration_ms = end_ms.unwrap_or(base.duration_ms).saturating_sub(t.start_ms);
            tracks.push(Track {
                id: uuid::Uuid::new_v4().to_string(),
                path: audio.clone(),
                title: t
                    .title
                    .clone()
                    .unwrap_or_else(|| format!("Track {:02}", t.number)),
                artist: t
                    .performer
                    .clone()
                    .or_else(|| sheet.album_performer.clone())
                    .or_else(|| base.artist.clone()),
                album_artist: sheet
                    .album_performer
                    .clone()
                    .or_else(|| base.album_artist.clone()),
                album: sheet.album_title.clone().or_else(|| base.album.clone()),
                track_number: Some(t.number),
                disc_number: base.disc_number,
                year: sheet.year.or(base.year),
                genre: sheet.genre.clone().or_else(|| base.genre.clone()),
                duration_ms,
                format: base.format,
                bitrate: base.bitrate,
                sample_rate: base.sample_rate,
                bit_depth: base.bit_depth,
                channels: base.channels,
                is_lossless: base.is_lossless,
                has_embedded_cover: base.has_embedded_cover,
                play_count: 0,
                last_played: None,
                date_added: chrono::Utc::now(),
                file_modified: base.file_modified,
                cue_source: Some(CueSource {
                    cue_path: cue_path.to_path_buf(),
                    cue_index: t.number,
                    start_ms: t.start_ms,
                    end_ms,
                }),
            });
        }
        if !tracks.is_empty() {
            out.push((audio.to_string_lossy().into_owned(), tracks));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 最小合法 WAV（与 metadata 测试相同的构造）。
    fn write_minimal_wav(path: &Path) {
        let data_size: u32 = 8000;
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
    fn scans_directory_indexes_supported_and_prunes_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("music");
        std::fs::create_dir_all(music.join("sub")).unwrap();
        write_minimal_wav(&music.join("a.wav"));
        write_minimal_wav(&music.join("sub").join("b.wav"));
        std::fs::write(music.join("cover.jpg"), b"not audio").unwrap();

        let store = Store::open_in_memory().unwrap();
        let cancel = AtomicBool::new(false);
        let events = std::sync::Mutex::new(Vec::new());
        let cb = |e: ScanEvent| events.lock().unwrap().push(e);

        let dirs = vec![music.to_string_lossy().into_owned()];
        let outcome = scan(&dirs, &store, &cancel, &cb).unwrap();

        assert_eq!(outcome.indexed, 2);
        assert_eq!(outcome.pruned, 0);
        assert!(!outcome.cancelled);
        assert_eq!(store.stats().unwrap().track_count, 2);
        // 最后一次事件必须是 done
        assert!(events.lock().unwrap().last().unwrap().done);

        // 删掉一个文件再扫：应被清理
        std::fs::remove_file(music.join("a.wav")).unwrap();
        let outcome2 = scan(&dirs, &store, &cancel, &cb).unwrap();
        assert_eq!(outcome2.pruned, 1);
        assert_eq!(store.stats().unwrap().track_count, 1);

        // 重扫幂等：不重复
        let outcome3 = scan(&dirs, &store, &cancel, &cb).unwrap();
        assert_eq!(outcome3.pruned, 0);
        assert_eq!(store.stats().unwrap().track_count, 1);
    }

    #[test]
    fn import_files_reuses_existing_and_skips_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        // a 在扫描目录内，b 在扫描目录外（验证导入时的新插入路径）
        let music = dir.path().join("music");
        std::fs::create_dir_all(&music).unwrap();
        let a = music.join("a.wav");
        let b = dir.path().join("b.wav");
        write_minimal_wav(&a);
        write_minimal_wav(&b);
        std::fs::write(dir.path().join("c.txt"), b"nope").unwrap();

        let store = Store::open_in_memory().unwrap();
        // a 先经扫描入库
        let cancel = AtomicBool::new(false);
        scan(
            &[music.to_string_lossy().into_owned()],
            &store,
            &cancel,
            &|_| {},
        )
        .unwrap();
        assert_eq!(store.stats().unwrap().track_count, 1);
        let existing = store
            .get_tracks_by_path(&a.to_string_lossy())
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let paths = vec![
            a.to_string_lossy().into_owned(),
            b.to_string_lossy().into_owned(),
            dir.path().join("c.txt").to_string_lossy().into_owned(),
            dir.path().join("missing.flac").to_string_lossy().into_owned(),
        ];
        let imported = import_files(&store, &paths).unwrap();
        // txt 与不存在的文件被跳过；a 复用已有 id
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].id, existing.id);
        // 不重复入库
        assert_eq!(store.stats().unwrap().track_count, 2);
    }

    #[test]
    fn cancellation_stops_early() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..10 {
            write_minimal_wav(&dir.path().join(format!("{i}.wav")));
        }
        let store = Store::open_in_memory().unwrap();
        let cancel = AtomicBool::new(true); // 一开始就取消
        let dirs = vec![dir.path().to_string_lossy().into_owned()];
        let outcome = scan(&dirs, &store, &cancel, &|_| {}).unwrap();
        assert!(outcome.cancelled);
        assert_eq!(store.stats().unwrap().track_count, 0);
    }

    #[test]
    fn cue_creates_virtual_tracks_and_hides_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("music");
        std::fs::create_dir_all(&music).unwrap();
        write_minimal_wav(&music.join("整轨.wav"));
        std::fs::write(
            music.join("整轨.cue"),
            "PERFORMER \"周杰伦\"\nTITLE \"叶惠美\"\nFILE \"整轨.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"以父之名\"\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    TITLE \"懦夫\"\n    INDEX 01 00:05:00\n",
        )
        .unwrap();

        let store = Store::open_in_memory().unwrap();
        let cancel = AtomicBool::new(false);
        let dirs = vec![music.to_string_lossy().into_owned()];
        scan(&dirs, &store, &cancel, &|_| {}).unwrap();

        // 只有 2 条虚拟曲目，整文件不单独入库
        let page = store
            .list_tracks(&crate::db::store::ListQuery {
                limit: 10,
                sort_by: Some("title".into()),
                sort_dir: Some("asc".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.total, 2);
        let t1 = page
            .tracks
            .iter()
            .find(|t| t.title == "以父之名")
            .expect("track 1");
        let cue1 = t1.cue_source.as_ref().unwrap();
        assert_eq!(cue1.cue_index, 1);
        assert_eq!(cue1.start_ms, 0);
        assert_eq!(cue1.end_ms, Some(5_000));
        assert_eq!(t1.duration_ms, 5_000);
        assert_eq!(t1.album.as_deref(), Some("叶惠美"));
        assert_eq!(t1.artist.as_deref(), Some("周杰伦"));

        let t2 = page.tracks.iter().find(|t| t.title == "懦夫").unwrap();
        assert_eq!(t2.cue_source.as_ref().unwrap().end_ms, None);

        // 重扫幂等：数量不变、id 保留
        let id1 = t1.id.clone();
        scan(&dirs, &store, &cancel, &|_| {}).unwrap();
        assert_eq!(store.stats().unwrap().track_count, 2);
        let again = store.get_track(&id1).unwrap();
        assert_eq!(again.title, "以父之名");

        // 删掉 cue 再扫：虚拟曲目清理，整文件恢复为单曲
        std::fs::remove_file(music.join("整轨.cue")).unwrap();
        scan(&dirs, &store, &cancel, &|_| {}).unwrap();
        let page = store
            .list_tracks(&crate::db::store::ListQuery {
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.total, 1);
        assert!(page.tracks[0].cue_source.is_none());
        assert_eq!(page.tracks[0].title, "整轨");
    }

    #[test]
    fn import_cue_file_expands_virtual_tracks() {
        let dir = tempfile::tempdir().unwrap();
        write_minimal_wav(&dir.path().join("a.wav"));
        let cue = dir.path().join("a.cue");
        std::fs::write(
            &cue,
            "FILE \"a.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 00:03:00\n",
        )
        .unwrap();

        let store = Store::open_in_memory().unwrap();
        let imported =
            import_files(&store, &[cue.to_string_lossy().into_owned()]).unwrap();
        assert_eq!(imported.len(), 2);
        assert!(imported.iter().all(|t| t.cue_source.is_some()));

        // 再次导入整轨音频本身：命中虚拟曲目，返回全部分轨而非新建整文件行
        let again = import_files(
            &store,
            &[dir.path().join("a.wav").to_string_lossy().into_owned()],
        )
        .unwrap();
        assert_eq!(again.len(), 2);
        assert_eq!(store.stats().unwrap().track_count, 2);
    }
}
