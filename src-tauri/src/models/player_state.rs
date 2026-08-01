//! 播放状态模型（契约 docs/ipc-contract.md 的 `PlayerState`）。

use serde::{Deserialize, Serialize};

/// 播放状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayStatus {
    Playing,
    Paused,
    Stopped,
}

/// 播放模式。序列化为契约的 kebab-case（`"repeat-one"` 等）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlayMode {
    Sequential,
    Shuffle,
    RepeatOne,
    RepeatAll,
}

impl PlayMode {
    /// 设置页/PlayerBar 的循环顺序。
    pub fn next(self) -> Self {
        match self {
            PlayMode::Sequential => PlayMode::RepeatAll,
            PlayMode::RepeatAll => PlayMode::RepeatOne,
            PlayMode::RepeatOne => PlayMode::Shuffle,
            PlayMode::Shuffle => PlayMode::Sequential,
        }
    }
}

/// 播放全量状态。`watch_player` 的 `{kind:"state"}` payload 与 `get_player_state` 返回值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub current_track_id: Option<String>,
    pub status: PlayStatus,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// 0-100 整数。
    pub volume: u32,
    pub muted: bool,
    pub play_mode: PlayMode,
    /// 有序曲目 id 列表（shuffle 时为乱序后的顺序）。
    pub queue: Vec<String>,
    pub queue_index: usize,
}

impl Default for PlayerState {
    fn default() -> Self {
        PlayerState {
            current_track_id: None,
            status: PlayStatus::Stopped,
            position_ms: 0,
            duration_ms: 0,
            volume: 80,
            muted: false,
            play_mode: PlayMode::Sequential,
            queue: Vec::new(),
            queue_index: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_contract_shape() {
        let s = PlayerState {
            current_track_id: Some("t1".into()),
            status: PlayStatus::Playing,
            play_mode: PlayMode::RepeatOne,
            queue: vec!["t1".into()],
            ..Default::default()
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["status"], "playing");
        assert_eq!(v["playMode"], "repeat-one");
        for key in [
            "currentTrackId",
            "status",
            "positionMs",
            "durationMs",
            "volume",
            "muted",
            "playMode",
            "queue",
            "queueIndex",
        ] {
            assert!(v.get(key).is_some(), "missing contract key: {key}");
        }
    }

    #[test]
    fn mode_cycles_in_ui_order() {
        let mut m = PlayMode::Sequential;
        m = m.next();
        assert_eq!(m, PlayMode::RepeatAll);
        m = m.next();
        assert_eq!(m, PlayMode::RepeatOne);
        m = m.next();
        assert_eq!(m, PlayMode::Shuffle);
        m = m.next();
        assert_eq!(m, PlayMode::Sequential);
    }
}
