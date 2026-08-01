//! 业务逻辑层。
//!
//! service 不依赖 Tauri，便于单测；由 commands 层调用。

pub mod cover;
pub mod cue;
pub mod library;
pub mod lyrics;
pub mod metadata;
pub mod online;
pub mod player;
pub mod settings;
