//! 播放状态机核心（不依赖 Tauri，纯逻辑可单测）。
//!
//! `PlayerCore` 只维护状态与决策：调用方把返回值 `EngineCmd`
//! 翻译成实际的引擎动作（见 commands/player.rs）。
//!
//! 播放模式语义：
//! - Sequential：顺序播完即止（end-file 到队列尾 → Stopped）
//! - RepeatAll：队列循环；next 到尾部回绕到 0
//! - RepeatOne：end-file 重播当前；显式 next/previous 仍按顺序切
//! - Shuffle：进入时乱序队列（当前曲目保持原位），end-file 到尾即停；
//!   切回其他模式时恢复原始顺序

use crate::models::player_state::{PlayMode, PlayStatus, PlayerState};

/// 需要引擎执行的动作。`Load` 携带 track id，由调用方解析路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineCmd {
    Load { track_id: String },
    Pause,
    Resume,
    Stop,
    Seek { position_ms: u64 },
    SetVolume { volume: u32 },
}

/// 计入一次播放的最小时长阈值（或过半时长，取先满足者）。
const PLAY_COUNT_MIN_MS: u64 = 30_000;

#[derive(Debug, Default)]
pub struct PlayerCore {
    state: PlayerState,
    /// shuffle 前的原始队列（切回顺序模式时恢复）。
    original_queue: Option<Vec<String>>,
    /// 当前曲目是否已计入播放统计（每次新文件加载重置）。
    stats_recorded: bool,
    /// 待记录的播放统计曲目 id（阈值刚跨过时置位，由调用方取走写库）。
    stats_pending: Option<String>,
    /// 恢复态：当前曲目尚未加载进引擎（启动恢复后为 true，首次播放触发 Load）。
    awaiting_load: bool,
    /// 恢复后待跳转的位置（file-loaded 后 seek 到此再清零）。
    resume_position_ms: u64,
}

impl PlayerCore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> &PlayerState {
        &self.state
    }

    /// 取走并清空待记录的播放统计曲目 id（commands 层据此写库）。
    pub fn take_stats_pending(&mut self) -> Option<String> {
        self.stats_pending.take()
    }

    /// 从持久化快照恢复（不自动播放）：设为暂停并标记待加载，首次播放时加载并 seek 到历史位置。
    /// 调用方（commands 层）应先校验当前曲目仍在库中。
    pub fn restore(&mut self, snapshot: PlayerState) {
        let has_track = snapshot.current_track_id.is_some()
            && !snapshot.queue.is_empty()
            && snapshot.queue_index < snapshot.queue.len();
        self.state = snapshot;
        // 恢复的队列即当前顺序（无原始顺序信息，切模式时以当前为准）。
        self.original_queue = None;
        self.stats_recorded = false;
        self.stats_pending = None;
        if has_track {
            self.resume_position_ms = self.state.position_ms;
            self.awaiting_load = true;
            self.state.status = PlayStatus::Paused;
        } else {
            self.state.current_track_id = None;
            self.state.queue.clear();
            self.state.queue_index = 0;
            self.state.position_ms = 0;
            self.state.duration_ms = 0;
            self.state.status = PlayStatus::Stopped;
            self.awaiting_load = false;
            self.resume_position_ms = 0;
        }
    }

    /// 设置队列并播放指定位置。
    pub fn play_queue(&mut self, ids: Vec<String>, start_index: usize) -> Option<EngineCmd> {
        if ids.is_empty() || start_index >= ids.len() {
            return None;
        }
        self.state.queue = ids;
        self.state.queue_index = start_index;
        if self.state.play_mode == PlayMode::Shuffle {
            self.shuffle_queue_keep_current();
        }
        Some(self.load_current())
    }

    /// 跳到队列内指定位置播放（play_track 命中当前队列时使用）。
    pub fn play_at(&mut self, index: usize) -> Option<EngineCmd> {
        if index >= self.state.queue.len() {
            return None;
        }
        self.state.queue_index = index;
        Some(self.load_current())
    }

    /// 播放/暂停切换。无当前曲目时无动作。
    pub fn toggle(&mut self) -> Option<EngineCmd> {
        match self.state.status {
            PlayStatus::Playing => {
                self.state.status = PlayStatus::Paused;
                Some(EngineCmd::Pause)
            }
            PlayStatus::Paused => {
                if self.awaiting_load {
                    // 恢复态首次播放：加载当前曲目（file-loaded 后 seek 到 resume 位置）。
                    self.awaiting_load = false;
                    self.state.status = PlayStatus::Playing;
                    let track_id = self.state.queue[self.state.queue_index].clone();
                    self.state.current_track_id = Some(track_id.clone());
                    return Some(EngineCmd::Load { track_id });
                }
                self.state.status = PlayStatus::Playing;
                Some(EngineCmd::Resume)
            }
            PlayStatus::Stopped => None,
        }
    }

    pub fn stop(&mut self) -> EngineCmd {
        self.state.status = PlayStatus::Stopped;
        self.state.position_ms = 0;
        EngineCmd::Stop
    }

    /// 显式下一首。到队尾：RepeatAll 回绕，其余返回 None（无动作）。
    pub fn next(&mut self) -> Option<EngineCmd> {
        if self.state.queue.is_empty() {
            return None;
        }
        let last = self.state.queue.len() - 1;
        match self.state.queue_index {
            i if i < last => {
                self.state.queue_index = i + 1;
                Some(self.load_current())
            }
            _ if self.state.play_mode == PlayMode::RepeatAll => {
                self.state.queue_index = 0;
                Some(self.load_current())
            }
            _ => None,
        }
    }

    /// 显式上一首。队首时重播当前（回到 0 秒）。
    pub fn previous(&mut self) -> Option<EngineCmd> {
        if self.state.queue.is_empty() {
            return None;
        }
        if self.state.queue_index > 0 {
            self.state.queue_index -= 1;
            return Some(self.load_current());
        }
        Some(EngineCmd::Seek { position_ms: 0 })
    }

    /// 播放自然结束（end-file）时的推进。
    pub fn on_end_file(&mut self) -> Option<EngineCmd> {
        if self.state.queue.is_empty() {
            self.state.status = PlayStatus::Stopped;
            return None;
        }
        match self.state.play_mode {
            PlayMode::RepeatOne => Some(self.load_current()),
            PlayMode::RepeatAll => {
                let last = self.state.queue.len() - 1;
                self.state.queue_index = if self.state.queue_index >= last {
                    0
                } else {
                    self.state.queue_index + 1
                };
                Some(self.load_current())
            }
            PlayMode::Sequential | PlayMode::Shuffle => {
                let last = self.state.queue.len() - 1;
                if self.state.queue_index >= last {
                    self.state.status = PlayStatus::Stopped;
                    self.state.position_ms = 0;
                    None
                } else {
                    self.state.queue_index += 1;
                    Some(self.load_current())
                }
            }
        }
    }

    pub fn seek(&mut self, position_ms: u64) -> EngineCmd {
        self.state.position_ms = position_ms;
        EngineCmd::Seek { position_ms }
    }

    pub fn set_volume(&mut self, volume: u32) -> EngineCmd {
        let volume = volume.min(100);
        self.state.volume = volume;
        EngineCmd::SetVolume { volume }
    }

    pub fn set_muted(&mut self, muted: bool) -> EngineCmd {
        self.state.muted = muted;
        // 静音用音量 0 实现，恢复时回到原音量
        EngineCmd::SetVolume {
            volume: if muted { 0 } else { self.state.volume },
        }
    }

    pub fn set_mode(&mut self, mode: PlayMode) {
        if self.state.play_mode == mode {
            return;
        }
        self.state.play_mode = mode;
        match mode {
            PlayMode::Shuffle => {
                self.original_queue = Some(self.state.queue.clone());
                self.shuffle_queue_keep_current();
            }
            _ => {
                if let Some(original) = self.original_queue.take() {
                    // 恢复原始顺序，并找回当前曲目在原顺序中的位置
                    let current_id = self.state.queue.get(self.state.queue_index).cloned();
                    self.state.queue = original;
                    if let Some(id) = current_id {
                        if let Some(pos) = self.state.queue.iter().position(|t| *t == id) {
                            self.state.queue_index = pos;
                        }
                    }
                }
            }
        }
    }

    /// 引擎事件：进度更新。
    pub fn on_progress(&mut self, position_ms: u64, duration_ms: u64) {
        self.state.position_ms = position_ms;
        if duration_ms > 0 {
            self.state.duration_ms = duration_ms;
        }
        self.note_play_progress();
    }

    /// 播放进度达阈值（≥ 30s 或 ≥ 50%）时，标记当前曲目待计入统计（每次加载只记一次）。
    fn note_play_progress(&mut self) {
        if self.stats_recorded {
            return;
        }
        let Some(id) = self.state.current_track_id.as_ref() else {
            return;
        };
        let pos = self.state.position_ms;
        let dur = self.state.duration_ms;
        let crossed = pos >= PLAY_COUNT_MIN_MS || (dur > 0 && pos.saturating_mul(2) >= dur);
        if crossed {
            self.stats_pending = Some(id.clone());
            self.stats_recorded = true;
        }
    }

    /// 引擎事件：暂停状态变化（外部/媒体键导致）。
    pub fn on_pause_changed(&mut self, paused: bool) {
        if self.state.current_track_id.is_none() {
            return;
        }
        self.state.status = if paused {
            PlayStatus::Paused
        } else {
            PlayStatus::Playing
        };
    }

    /// 引擎事件：新文件加载完成（拿到真实时长）。
    /// 恢复态首次加载时返回 Seek 以跳到历史位置；否则从头播放返回 None。
    pub fn on_file_loaded(&mut self, duration_ms: u64) -> Option<EngineCmd> {
        // 新一次播放开始：重置统计计数门控（repeat-one 重播也各计一次）。
        self.stats_recorded = false;
        self.stats_pending = None;
        self.state.duration_ms = duration_ms;
        self.state.status = PlayStatus::Playing;
        if self.resume_position_ms > 0 {
            let pos = if duration_ms > 0 {
                self.resume_position_ms.min(duration_ms)
            } else {
                self.resume_position_ms
            };
            self.resume_position_ms = 0;
            self.state.position_ms = pos;
            Some(EngineCmd::Seek { position_ms: pos })
        } else {
            self.state.position_ms = 0;
            None
        }
    }

    // ---------- 队列编辑（Phase 2） ----------

    /// 追加曲目到队列；`next=true` 插到当前曲目之后（“下一首播放”）。
    /// 不改变播放状态；空队列时仅入列不自动播放。
    pub fn queue_add(&mut self, ids: Vec<String>, next: bool) {
        if ids.is_empty() {
            return;
        }
        // shuffle 激活时同步追加到原始队列尾部，保持成员一致
        if let Some(orig) = self.original_queue.as_mut() {
            orig.extend(ids.iter().cloned());
        }
        if next && !self.state.queue.is_empty() {
            let at = (self.state.queue_index + 1).min(self.state.queue.len());
            self.state.queue.splice(at..at, ids);
        } else {
            self.state.queue.extend(ids);
        }
    }

    /// 移除队列指定位置。移除当前曲目时：播放中→自动播下一首；
    /// 队列清空→停止。越界无动作。
    pub fn queue_remove(&mut self, index: usize) -> Option<EngineCmd> {
        if index >= self.state.queue.len() {
            return None;
        }
        let removed = self.state.queue.remove(index);
        if let Some(orig) = self.original_queue.as_mut() {
            if let Some(pos) = orig.iter().position(|t| *t == removed) {
                orig.remove(pos);
            }
        }

        if self.state.queue.is_empty() {
            let was_active = self.state.status != PlayStatus::Stopped;
            self.state.queue_index = 0;
            self.state.current_track_id = None;
            self.state.status = PlayStatus::Stopped;
            self.state.position_ms = 0;
            self.state.duration_ms = 0;
            return was_active.then_some(EngineCmd::Stop);
        }

        use std::cmp::Ordering;
        match index.cmp(&self.state.queue_index) {
            Ordering::Less => {
                self.state.queue_index -= 1;
                None
            }
            Ordering::Greater => None,
            Ordering::Equal => {
                // 移除的是当前曲目：index 不变即指向原“下一首”，尾部则回退
                if self.state.queue_index >= self.state.queue.len() {
                    self.state.queue_index = self.state.queue.len() - 1;
                }
                match self.state.status {
                    PlayStatus::Stopped => {
                        self.state.current_track_id = None;
                        None
                    }
                    _ => Some(self.load_current()),
                }
            }
        }
    }

    /// 队内移动（拖拽排序）：当前曲目索引跟随调整。越界/同位无动作。
    pub fn queue_move(&mut self, from: usize, to: usize) {
        let len = self.state.queue.len();
        if from >= len || to >= len || from == to {
            return;
        }
        let item = self.state.queue.remove(from);
        self.state.queue.insert(to, item);
        let idx = self.state.queue_index;
        self.state.queue_index = if idx == from {
            to
        } else if from < idx && to >= idx {
            idx - 1
        } else if from > idx && to <= idx {
            idx + 1
        } else {
            idx
        };
    }

    /// 清空队列：保留正在播放/暂停的当前曲目（行业惯例，不打断播放）。
    pub fn queue_clear(&mut self) {
        let current = self
            .state
            .current_track_id
            .clone()
            .filter(|_| self.state.status != PlayStatus::Stopped);
        match current {
            Some(id) => {
                self.state.queue = vec![id];
                self.state.queue_index = 0;
            }
            None => {
                self.state.queue.clear();
                self.state.queue_index = 0;
                self.state.current_track_id = None;
            }
        }
        // 清空后无“原始顺序”可言
        self.original_queue = None;
    }

    fn load_current(&mut self) -> EngineCmd {
        let track_id = self.state.queue[self.state.queue_index].clone();
        self.state.current_track_id = Some(track_id.clone());
        self.state.status = PlayStatus::Playing;
        self.state.position_ms = 0;
        // 显式加载取消恢复态与待跳转位置
        self.awaiting_load = false;
        self.resume_position_ms = 0;
        EngineCmd::Load { track_id }
    }

    /// 乱序队列，当前曲目保持在当前 index。
    fn shuffle_queue_keep_current(&mut self) {
        let len = self.state.queue.len();
        if len <= 2 {
            return;
        }
        let idx = self.state.queue_index;
        // Fisher-Yates，固定种子无需——每次进入 shuffle 重新随机即可。
        // 用简单 LCG 避免引入 rand 依赖。
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
            .unwrap_or(0x9e3779b97f4a7c15);
        let mut next_rand = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        for i in (1..len).rev() {
            let j = next_rand() % (i + 1);
            self.state.queue.swap(i, j);
        }
        // 保持当前曲目在原 index
        if let Some(pos) = self
            .state
            .queue
            .iter()
            .position(|t| Some(t) == self.state.current_track_id.as_ref())
        {
            if pos != idx {
                self.state.queue.swap(pos, idx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_with_queue(n: usize, start: usize) -> PlayerCore {
        let mut core = PlayerCore::new();
        let ids: Vec<String> = (0..n).map(|i| format!("t{i}")).collect();
        core.play_queue(ids, start);
        core
    }

    fn load_id(cmd: &EngineCmd) -> &str {
        match cmd {
            EngineCmd::Load { track_id } => track_id,
            other => panic!("expected Load, got {other:?}"),
        }
    }

    #[test]
    fn play_queue_sets_current_and_plays() {
        let core = core_with_queue(3, 1);
        assert_eq!(core.state().current_track_id.as_deref(), Some("t1"));
        assert_eq!(core.state().status, PlayStatus::Playing);
        assert_eq!(core.state().queue_index, 1);
    }

    #[test]
    fn play_queue_rejects_out_of_range() {
        let mut core = PlayerCore::new();
        assert!(core.play_queue(vec!["t0".into()], 5).is_none());
        assert!(core.play_queue(vec![], 0).is_none());
    }

    #[test]
    fn play_at_jumps_within_queue() {
        let mut core = core_with_queue(3, 0);
        assert_eq!(load_id(&core.play_at(2).unwrap()), "t2");
        assert_eq!(core.state().queue_index, 2);
        assert_eq!(core.state().status, PlayStatus::Playing);
        // 越界无动作，状态不变
        assert!(core.play_at(9).is_none());
        assert_eq!(core.state().queue_index, 2);
    }

    #[test]
    fn toggle_pause_resume() {
        let mut core = core_with_queue(2, 0);
        assert_eq!(core.toggle(), Some(EngineCmd::Pause));
        assert_eq!(core.state().status, PlayStatus::Paused);
        assert_eq!(core.toggle(), Some(EngineCmd::Resume));
        assert_eq!(core.state().status, PlayStatus::Playing);

        // Stopped 状态 toggle 无动作
        let mut idle = PlayerCore::new();
        assert_eq!(idle.toggle(), None);
    }

    #[test]
    fn sequential_advances_and_stops_at_end() {
        let mut core = core_with_queue(2, 0);
        assert_eq!(load_id(&core.next().unwrap()), "t1");
        // 到队尾后 next 无动作
        assert_eq!(core.next(), None);
        // end-file：第一首→第二首→停止
        let mut core = core_with_queue(2, 0);
        assert_eq!(load_id(&core.on_end_file().unwrap()), "t1");
        assert_eq!(core.on_end_file(), None);
        assert_eq!(core.state().status, PlayStatus::Stopped);
    }

    #[test]
    fn repeat_all_wraps_around() {
        let mut core = core_with_queue(2, 1);
        core.set_mode(PlayMode::RepeatAll);
        assert_eq!(load_id(&core.next().unwrap()), "t0");
        let mut core = core_with_queue(2, 1);
        core.set_mode(PlayMode::RepeatAll);
        assert_eq!(load_id(&core.on_end_file().unwrap()), "t0");
    }

    #[test]
    fn repeat_one_replays_on_endfile_but_next_advances() {
        let mut core = core_with_queue(2, 0);
        core.set_mode(PlayMode::RepeatOne);
        assert_eq!(load_id(&core.on_end_file().unwrap()), "t0");
        assert_eq!(core.state().queue_index, 0);
        // 显式 next 仍按顺序切
        assert_eq!(load_id(&core.next().unwrap()), "t1");
        // 显式 next 到尾后无动作（repeat-one 不 wrap）
        assert_eq!(core.next(), None);
    }

    #[test]
    fn previous_at_start_restarts_current() {
        let mut core = core_with_queue(3, 0);
        assert_eq!(core.previous(), Some(EngineCmd::Seek { position_ms: 0 }));
        let mut core = core_with_queue(3, 2);
        assert_eq!(load_id(&core.previous().unwrap()), "t1");
    }

    #[test]
    fn shuffle_keeps_membership_and_restores_order() {
        let mut core = core_with_queue(10, 4);
        core.set_mode(PlayMode::Shuffle);
        // 成员不变、当前曲目仍在当前 index
        let mut sorted = core.state().queue.clone();
        sorted.sort();
        assert_eq!(sorted, (0..10).map(|i| format!("t{i}")).collect::<Vec<_>>());
        assert_eq!(core.state().queue[core.state().queue_index], "t4");
        // 切回顺序模式恢复原顺序
        core.set_mode(PlayMode::Sequential);
        assert_eq!(
            core.state().queue,
            (0..10).map(|i| format!("t{i}")).collect::<Vec<_>>()
        );
        assert_eq!(core.state().queue_index, 4);
    }

    #[test]
    fn volume_clamped_and_mute_uses_zero_volume() {
        let mut core = core_with_queue(1, 0);
        assert_eq!(core.set_volume(150), EngineCmd::SetVolume { volume: 100 });
        assert_eq!(core.state().volume, 100);
        assert_eq!(core.set_volume(60), EngineCmd::SetVolume { volume: 60 });
        assert_eq!(core.set_muted(true), EngineCmd::SetVolume { volume: 0 });
        assert!(core.state().muted);
        assert_eq!(core.set_muted(false), EngineCmd::SetVolume { volume: 60 });
        assert!(!core.state().muted);
    }

    #[test]
    fn progress_and_file_loaded_events() {
        let mut core = core_with_queue(1, 0);
        core.on_file_loaded(180_000);
        assert_eq!(core.state().duration_ms, 180_000);
        assert_eq!(core.state().position_ms, 0);
        core.on_progress(5_000, 180_000);
        assert_eq!(core.state().position_ms, 5_000);
        // duration 为 0 时不覆盖已知时长
        core.on_progress(6_000, 0);
        assert_eq!(core.state().duration_ms, 180_000);
        core.on_pause_changed(true);
        assert_eq!(core.state().status, PlayStatus::Paused);
    }

    #[test]
    fn play_count_records_once_after_threshold() {
        let mut core = core_with_queue(2, 0); // 当前 t0
        core.on_file_loaded(180_000); // 180s
        assert!(core.take_stats_pending().is_none());
        core.on_progress(10_000, 180_000); // <30s 且 <50%
        assert!(core.take_stats_pending().is_none());
        core.on_progress(30_000, 180_000); // 跨过 30s
        assert_eq!(core.take_stats_pending().as_deref(), Some("t0"));
        // 同一次播放只计一次
        core.on_progress(90_000, 180_000);
        assert!(core.take_stats_pending().is_none());

        // 切到下一首并重新加载：可再次计入（短曲过半）
        core.on_end_file();
        core.on_file_loaded(10_000); // 10s
        core.on_progress(6_000, 10_000); // >50%
        assert_eq!(core.take_stats_pending().as_deref(), Some("t1"));
    }

    // ---------- 队列编辑 ----------

    #[test]
    fn queue_add_appends_or_inserts_after_current() {
        let mut core = core_with_queue(3, 1); // 当前 t1
        core.queue_add(vec!["x".into()], false);
        assert_eq!(core.state().queue, vec!["t0", "t1", "t2", "x"]);
        core.queue_add(vec!["y".into(), "z".into()], true);
        assert_eq!(core.state().queue, vec!["t0", "t1", "y", "z", "t2", "x"]);
        // 当前曲目与播放状态不受影响
        assert_eq!(core.state().current_track_id.as_deref(), Some("t1"));
        assert_eq!(core.state().queue_index, 1);

        // 空队列：仅入列不自动播放
        let mut idle = PlayerCore::new();
        idle.queue_add(vec!["a".into()], true);
        assert_eq!(idle.state().queue, vec!["a"]);
        assert_eq!(idle.state().status, PlayStatus::Stopped);
        assert_eq!(idle.state().current_track_id, None);
    }

    #[test]
    fn queue_remove_adjusts_index_and_handles_current() {
        // 移除当前之前：index 前移，无引擎动作
        let mut core = core_with_queue(3, 1);
        assert_eq!(core.queue_remove(0), None);
        assert_eq!(core.state().queue, vec!["t1", "t2"]);
        assert_eq!(core.state().queue_index, 0);

        // 移除当前（播放中）：自动播原下一首
        let mut core = core_with_queue(3, 1);
        let cmd = core.queue_remove(1).unwrap();
        assert_eq!(load_id(&cmd), "t2");
        assert_eq!(core.state().queue, vec!["t0", "t2"]);
        assert_eq!(core.state().queue_index, 1);

        // 移除尾部当前：回退到新尾部
        let mut core = core_with_queue(2, 1);
        let cmd = core.queue_remove(1).unwrap();
        assert_eq!(load_id(&cmd), "t0");
        assert_eq!(core.state().queue_index, 0);

        // 移除最后一首：停止 + Stop
        let mut core = core_with_queue(1, 0);
        assert_eq!(core.queue_remove(0), Some(EngineCmd::Stop));
        assert_eq!(core.state().status, PlayStatus::Stopped);
        assert_eq!(core.state().current_track_id, None);
        assert!(core.state().queue.is_empty());

        // 越界无动作
        let mut core = core_with_queue(2, 0);
        assert_eq!(core.queue_remove(5), None);
        assert_eq!(core.state().queue.len(), 2);
    }

    #[test]
    fn queue_move_keeps_current_index_tracking() {
        // 移动当前曲目本身
        let mut core = core_with_queue(4, 1);
        core.queue_move(1, 3);
        assert_eq!(core.state().queue, vec!["t0", "t2", "t3", "t1"]);
        assert_eq!(core.state().queue_index, 3);
        assert_eq!(core.state().current_track_id.as_deref(), Some("t1"));

        // 从前方移到当前之后：index 前移
        let mut core = core_with_queue(4, 2);
        core.queue_move(0, 3);
        assert_eq!(core.state().queue, vec!["t1", "t2", "t3", "t0"]);
        assert_eq!(core.state().queue_index, 1);

        // 从后方移到当前之前：index 后移
        let mut core = core_with_queue(4, 1);
        core.queue_move(3, 0);
        assert_eq!(core.state().queue, vec!["t3", "t0", "t1", "t2"]);
        assert_eq!(core.state().queue_index, 2);

        // 越界/同位无动作
        let mut core = core_with_queue(2, 0);
        core.queue_move(0, 0);
        core.queue_move(5, 1);
        assert_eq!(core.state().queue, vec!["t0", "t1"]);
    }

    #[test]
    fn queue_clear_keeps_playing_track() {
        let mut core = core_with_queue(3, 1);
        core.queue_clear();
        assert_eq!(core.state().queue, vec!["t1"]);
        assert_eq!(core.state().queue_index, 0);
        assert_eq!(core.state().status, PlayStatus::Playing);

        // 停止态：全部清空
        let mut core = core_with_queue(3, 1);
        core.stop();
        core.queue_clear();
        assert!(core.state().queue.is_empty());
        assert_eq!(core.state().current_track_id, None);
    }

    #[test]
    fn queue_edits_sync_with_shuffle_original_queue() {
        let mut core = core_with_queue(5, 2);
        core.set_mode(PlayMode::Shuffle);
        core.queue_add(vec!["x".into()], false);
        // 移除一首非当前曲目
        let victim_idx = if core.state().queue_index == 0 { 1 } else { 0 };
        let victim = core.state().queue[victim_idx].clone();
        core.queue_remove(victim_idx);
        // 切回顺序模式：成员 = 原 5 首 - victim + x
        core.set_mode(PlayMode::Sequential);
        let mut members = core.state().queue.clone();
        members.sort();
        let mut expected: Vec<String> = (0..5)
            .map(|i| format!("t{i}"))
            .filter(|t| *t != victim)
            .chain(std::iter::once("x".to_string()))
            .collect();
        expected.sort();
        assert_eq!(members, expected);
    }
}
