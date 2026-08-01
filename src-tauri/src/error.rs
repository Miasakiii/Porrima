//! 统一错误类型。
//!
//! 所有 command / service 的错误都收敛到 `AppError`，
//! 序列化为 `{ "kind": "...", "message": "..." }` 结构传给前端，
//! 前端可依据 `kind` 做分类处理（见 src/lib/ipc.ts）。

use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("database error: {0}")]
    Db(String),

    #[error("player error: {0}")]
    Player(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// 稳定的机器可读错误类别，随 `kind` 字段返回给前端。
    fn kind(&self) -> &'static str {
        match self {
            AppError::Io(_) => "io",
            AppError::Json(_) => "json",
            AppError::Db(_) => "db",
            AppError::Player(_) => "player",
            AppError::NotFound(_) => "not_found",
            AppError::InvalidArgument(_) => "invalid_argument",
            AppError::Other(_) => "other",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("kind", self.kind())?;
        map.serialize_entry("message", &self.to_string())?;
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_kind_message_shape() {
        let err = AppError::NotFound("track 42".to_string());
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "kind": "not_found", "message": "not found: track 42" })
        );
    }

    #[test]
    fn converts_io_error() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = AppError::from(io);
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["kind"], "io");
        assert!(v["message"].as_str().unwrap().contains("denied"));
    }
}
