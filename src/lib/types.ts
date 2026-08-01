/**
 * IPC 契约（docs/ipc-contract.md）对应的前端类型定义。
 * 字段均为 camelCase，与 Rust 侧 `#[serde(rename_all = "camelCase")]` 对应。
 */

/** CUE 整轨来源：虚拟曲目在整轨音频文件内的时间窗口。 */
export interface CueSource {
  cuePath: string;
  /** CUE 内 TRACK 序号（01 起） */
  cueIndex: number;
  startMs: number;
  /** 文件内最后一轨为 null（播到文件尾） */
  endMs: number | null;
}

export interface Track {
  id: string;
  path: string;
  title: string;
  artist: string | null;
  albumArtist: string | null;
  album: string | null;
  trackNumber: number | null;
  discNumber: number | null;
  year: number | null;
  genre: string | null;
  durationMs: number;
  /** 小写: "flac" | "mp3" | "m4a" | "aac" | "ogg" | "opus" | "wav" | ... | "other" */
  format: string;
  /** kbps，未知为 0 */
  bitrate: number;
  /** Hz，未知为 0 */
  sampleRate: number;
  bitDepth: number | null;
  channels: number | null;
  isLossless: boolean;
  hasEmbeddedCover: boolean;
  playCount: number;
  lastPlayed: string | null;
  dateAdded: string;
  fileModified: string;
  /** CUE 整轨来源；普通文件曲目为 null */
  cueSource: CueSource | null;
}

export type Theme = "dark" | "light" | "system";

/** 音频输出后端。 */
export type AudioBackend = "system" | "wasapi-shared" | "wasapi-exclusive";

/** ReplayGain 模式。 */
export type ReplayGainMode = "off" | "track" | "album";

/** 音频输出配置（Phase 3）。 */
export interface AudioOutputConfig {
  backend: AudioBackend;
  /** 输出设备名称；null = 系统默认 */
  device: string | null;
  /** 无缝播放 */
  gapless: boolean;
  replayGain: ReplayGainMode;
  /** 无 RG 标签时 loudnorm 响度归一化 */
  loudnormFallback: boolean;
}

export interface Settings {
  theme: Theme;
  scanDirs: string[];
  audioOutput: AudioOutputConfig;
}

export type PlayerStatus = "playing" | "paused" | "stopped";

export type PlayMode = "sequential" | "shuffle" | "repeat-one" | "repeat-all";

export interface PlayerState {
  currentTrackId: string | null;
  status: PlayerStatus;
  positionMs: number;
  durationMs: number;
  /** 0-100 整数 */
  volume: number;
  muted: boolean;
  playMode: PlayMode;
  /** track id 有序列表 */
  queue: string[];
  queueIndex: number;
}

export type SortBy =
  | "title"
  | "artist"
  | "album"
  | "durationMs"
  | "dateAdded"
  | "playCount";

export type SortDir = "asc" | "desc";

export interface ListTracksResult {
  tracks: Track[];
  total: number;
}

export interface LibraryStats {
  trackCount: number;
  totalDurationMs: number;
  losslessCount: number;
}

/** `get_stats_summary` 返回：统计页概览。 */
export interface StatsSummary {
  trackCount: number;
  totalDurationMs: number;
  losslessCount: number;
  totalPlays: number;
  /** 有播放记录（play_count>0）的曲目数 */
  playedCount: number;
  formats: FormatCount[];
}

/** 单个格式的曲目数（格式分布）。 */
export interface FormatCount {
  format: string;
  count: number;
}

/** `list_albums` 返回元素：按 (albumArtist, name) 升序聚合的专辑摘要。 */
export interface AlbumSummary {
  /** name+albumArtist 的稳定哈希，仅供 React key/选中；不用于查询 */
  id: string;
  /** 专辑名；null = 未知专辑 */
  name: string | null;
  /** COALESCE(album_artist, artist)；null = 未知艺术家 */
  albumArtist: string | null;
  year: number | null;
  trackCount: number;
  totalDurationMs: number;
  /** 代表曲目 id（首轨），用 getCover 取封面 */
  coverTrackId: string;
}

/** `list_artists` 返回元素：按 name 升序聚合的艺术家摘要。 */
export interface ArtistSummary {
  /** COALESCE(artist, album_artist)；null = 未知艺术家 */
  name: string | null;
  /** 不同专辑数（不计空专辑） */
  albumCount: number;
  trackCount: number;
  totalDurationMs: number;
}

/** `list_playlists` / create / rename 返回的播放列表摘要。 */
export interface PlaylistSummary {
  id: string;
  name: string;
  description: string | null;
  trackCount: number;
  /** RFC3339 字符串 */
  createdAt: string;
  updatedAt: string;
}

/** `get_cover` 返回的封面数据（无封面时命令返回 null）。 */
export interface CoverPayload {
  mimeType: string;
  dataBase64: string;
}

/** `get_cover_color` 返回的封面代表色（sRGB 0-255）；无封面时返回 null。 */
export interface CoverColor {
  r: number;
  g: number;
  b: number;
}

export type LyricsSource = "lrcFile" | "embedded";

/** `get_lyrics` 返回的原始歌词文本（LRC 解析在前端 lib/lrc.ts）。 */
export interface LyricsPayload {
  source: LyricsSource;
  text: string;
}

/** `watch_player` Channel 推送的 payload。 */
export type WatchPlayerPayload =
  | { kind: "progress"; positionMs: number; durationMs: number }
  | { kind: "state"; state: PlayerState };

/** `library:scan-progress` 事件 payload。 */
export interface ScanProgress {
  scannedFiles: number;
  totalFiles: number | null;
  currentPath: string;
  done: boolean;
  error: string | null;
}

/**
 * `engine:cmd` 事件 payload（后端 → 引擎适配器，内部接口）。
 * 形状与 Rust 侧 `EngineCmdPayload` 一致（docs/ipc-contract.md）。
 */
export type EngineCmdPayload =
  | {
      kind: "load";
      path: string;
      /** CUE 虚拟曲目起播位置（整轨内绝对 ms）；普通曲目为 null */
      startMs: number | null;
      /** CUE 虚拟曲目结束位置；末轨/普通曲目为 null */
      endMs: number | null;
    }
  | { kind: "pause" }
  | { kind: "resume" }
  | { kind: "stop" }
  | { kind: "seek"; positionMs: number }
  | { kind: "setVolume"; volume: number }
  | {
      kind: "setAudioOptions";
      exclusive: boolean;
      device: string;
      gapless: boolean;
      replayGain: string;
      loudnormFallback: boolean;
    };
