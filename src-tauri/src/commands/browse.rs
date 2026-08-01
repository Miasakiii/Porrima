//! 文件浏览命令（Phase 4 视频模式）：列出目录内容，按视频格式过滤。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult};

/// 支持的视频扩展名。
const VIDEO_EXTS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "ts", "wmv", "rmvb", "m4v", "mpg", "mpeg", "3gp",
];

/// 目录条目（文件或子目录）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    /// 文件/目录名。
    pub name: String,
    /// 绝对路径。
    pub path: String,
    /// 是否为目录。
    pub is_dir: bool,
    /// 文件大小（字节）；目录为 0。
    pub size: u64,
}

/// 目录浏览结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseResult {
    /// 当前浏览的路径。
    pub current: String,
    /// 父目录路径（根目录时为 None）。
    pub parent: Option<String>,
    /// 子目录列表（按名称排序）。
    pub dirs: Vec<DirEntry>,
    /// 视频文件列表（按名称排序）。
    pub files: Vec<DirEntry>,
}

/// 列出指定目录的子目录与视频文件。
///
/// - `path` 为空时返回系统盘符/根目录列表（Windows 多盘符场景）。
/// - 仅返回视频格式文件，隐藏文件/系统目录被过滤。
#[tauri::command]
pub fn browse_dir(path: Option<String>) -> AppResult<BrowseResult> {
    match path {
        Some(ref p) if !p.is_empty() => list_dir(Path::new(p)),
        _ => list_roots(),
    }
}

/// Windows 多盘符：列出可用驱动器作为"根"。
fn list_roots() -> AppResult<BrowseResult> {
    #[cfg(windows)]
    {
        let mut dirs = Vec::new();
        // 遍历 A:-Z: 检测可用盘符
        for c in b'A'..=b'Z' {
            let root = format!("{}:\\", c as char);
            if Path::new(&root).exists() {
                dirs.push(DirEntry {
                    name: root.clone(),
                    path: root,
                    is_dir: true,
                    size: 0,
                });
            }
        }
        return Ok(BrowseResult {
            current: String::new(),
            parent: None,
            dirs,
            files: Vec::new(),
        });
    }
    #[cfg(not(windows))]
    {
        list_dir(Path::new("/"))
    }
}

fn list_dir(dir: &Path) -> AppResult<BrowseResult> {
    if !dir.is_dir() {
        return Err(AppError::NotFound(format!(
            "directory not found: {}",
            dir.display()
        )));
    }

    let read = std::fs::read_dir(dir)
        .map_err(|e| AppError::Other(format!("read_dir {}: {e}", dir.display())))?;

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // 过滤隐藏文件/目录（以 . 开头）和系统目录
        if name.starts_with('.') || name == "$RECYCLE.BIN" || name == "System Volume Information" {
            continue;
        }
        let path = entry.path();
        let is_dir = path.is_dir();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

        let item = DirEntry {
            name,
            path: path.to_string_lossy().into_owned(),
            is_dir,
            size,
        };

        if is_dir {
            dirs.push(item);
        } else if is_video_file(&path) {
            files.push(item);
        }
    }

    dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let parent = dir.parent().map(|p| p.to_string_lossy().into_owned());

    Ok(BrowseResult {
        current: dir.to_string_lossy().into_owned(),
        parent,
        dirs,
        files,
    })
}

fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// 直接播放视频文件（不入库，直接 loadfile）。
/// 返回文件路径供前端引擎适配器加载。
#[tauri::command]
pub fn play_video_file(path: String) -> AppResult<String> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(AppError::NotFound(format!("file not found: {path}")));
    }
    if !is_video_file(&p) {
        return Err(AppError::InvalidArgument(format!(
            "not a video file: {path}"
        )));
    }
    Ok(path)
}

// ---------- 视频续播位置存储 ----------

/// 保存视频播放位置（前端定期调用）。
#[tauri::command]
pub fn save_video_position(
    state: tauri::State<'_, super::AppState>,
    path: String,
    position_ms: u64,
) -> AppResult<()> {
    let store = state.store.lock().map_err(|_| AppError::Other("lock poisoned".into()))?;
    store.save_video_position(&path, position_ms)
}

/// 获取视频上次播放位置（播放前调用，用于续播）。
#[tauri::command]
pub fn get_video_position(
    state: tauri::State<'_, super::AppState>,
    path: String,
) -> AppResult<u64> {
    let store = state.store.lock().map_err(|_| AppError::Other("lock poisoned".into()))?;
    Ok(store.get_video_position(&path))
}

// ---------- 截图目录 ----------

/// 获取（并创建）视频截图保存目录。
///
/// 优先使用系统「图片」目录下的 `Porrima Screenshots` 子目录；
/// 取不到时退回应用数据目录。返回绝对路径字符串。
#[tauri::command]
pub fn get_screenshot_dir(app: tauri::AppHandle) -> AppResult<String> {
    use tauri::Manager;
    let base = app
        .path()
        .picture_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| AppError::Other(format!("resolve picture dir: {e}")))?;
    let dir = base.join("Porrima Screenshots");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Other(format!("create screenshot dir: {e}")))?;
    Ok(dir.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_ext_detection() {
        assert!(is_video_file(Path::new("D:/movie.mp4")));
        assert!(is_video_file(Path::new("D:/movie.MKV")));
        assert!(is_video_file(Path::new("D:/clip.webm")));
        assert!(!is_video_file(Path::new("D:/song.flac")));
        assert!(!is_video_file(Path::new("D:/readme.txt")));
        assert!(!is_video_file(Path::new("D:/noext")));
    }

    #[test]
    fn browse_nonexistent_dir_errors() {
        assert!(list_dir(Path::new("Z:/nonexistent_dir_xyz")).is_err());
    }
}
