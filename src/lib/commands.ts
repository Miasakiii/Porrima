import { Channel } from "@tauri-apps/api/core";
import { invokeCmd } from "@/lib/ipc";
import type {
  AlbumSummary,
  ArtistSummary,
  CoverColor,
  CoverPayload,
  LibraryStats,
  ListTracksResult,
  LyricsPayload,
  PlayMode,
  PlayerState,
  PlaylistSummary,
  Settings,
  SortBy,
  SortDir,
  StatsSummary,
  Track,
  WatchPlayerPayload,
} from "@/lib/types";

/**
 * 契约命令的类型化封装（docs/ipc-contract.md）。
 * 所有函数在失败时 reject 归一化后的 AppError，调用方负责捕获。
 */

// ---- 媒体库 ----

export function scanLibrary(): Promise<null> {
  return invokeCmd<null>("scan_library");
}

export function cancelScan(): Promise<null> {
  return invokeCmd<null>("cancel_scan");
}

export interface ListTracksParams {
  offset: number;
  limit: number;
  sortBy?: SortBy;
  sortDir?: SortDir;
  search?: string;
}

export function listTracks(params: ListTracksParams): Promise<ListTracksResult> {
  return invokeCmd<ListTracksResult>("list_tracks", { ...params });
}

export function getTrack(id: string): Promise<Track> {
  return invokeCmd<Track>("get_track", { id });
}

/** 批量取曲目（队列页）：保持入参顺序，不存在的 id 静默跳过。 */
export function getTracks(ids: string[]): Promise<Track[]> {
  return invokeCmd<Track[]>("get_tracks", { ids });
}

export function getLibraryStats(): Promise<LibraryStats> {
  return invokeCmd<LibraryStats>("get_library_stats");
}

/** 统计页概览（总数/时长/无损、总播放/已播放数、格式分布）。 */
export function getStatsSummary(): Promise<StatsSummary> {
  return invokeCmd<StatsSummary>("get_stats_summary");
}

/** 最近播放（按 last_played 倒序）。 */
export function listRecentlyPlayed(limit: number): Promise<Track[]> {
  return invokeCmd<Track[]>("list_recently_played", { limit });
}

/** 常听排行（按 play_count 倒序）。 */
export function listMostPlayed(limit: number): Promise<Track[]> {
  return invokeCmd<Track[]>("list_most_played", { limit });
}

/** 全部专辑摘要（按专辑艺术家/专辑名升序）。 */
export function listAlbums(): Promise<AlbumSummary[]> {
  return invokeCmd<AlbumSummary[]>("list_albums");
}

/** 全部艺术家摘要（按名称升序）。 */
export function listArtists(): Promise<ArtistSummary[]> {
  return invokeCmd<ArtistSummary[]>("list_artists");
}

/** 某专辑的全部曲目（按碟号/轨号排序）；未知专辑传 null。 */
export function getAlbumTracks(
  album: string | null,
  albumArtist: string | null,
): Promise<Track[]> {
  return invokeCmd<Track[]>("get_album_tracks", { album, albumArtist });
}

/** 某艺术家的全部曲目（按专辑/轨号排序）；未知艺术家传 null。 */
export function getArtistTracks(artist: string | null): Promise<Track[]> {
  return invokeCmd<Track[]>("get_artist_tracks", { artist });
}

/** 曲目封面（内嵌优先，回退同目录 cover/folder/front）；无封面返回 null。 */
export function getCover(id: string): Promise<CoverPayload | null> {
  return invokeCmd<CoverPayload | null>("get_cover", { id });
}

/** 封面代表色（会话内缓存）；无封面/解码失败返回 null。用于动态强调色。 */
export function getCoverColor(id: string): Promise<CoverColor | null> {
  return invokeCmd<CoverColor | null>("get_cover_color", { id });
}

/** 曲目歌词（同目录 .lrc 优先，回退内嵌标签）；无歌词返回 null。 */
export function getLyrics(id: string): Promise<LyricsPayload | null> {
  return invokeCmd<LyricsPayload | null>("get_lyrics", { id });
}

// ---- 播放 ----

export function playTrack(id: string): Promise<null> {
  return invokeCmd<null>("play_track", { id });
}

export function playQueue(ids: string[], startIndex: number): Promise<null> {
  return invokeCmd<null>("play_queue", { ids, startIndex });
}

export function togglePlay(): Promise<null> {
  return invokeCmd<null>("toggle_play");
}

export function stop(): Promise<null> {
  return invokeCmd<null>("stop");
}

export function nextTrack(): Promise<null> {
  return invokeCmd<null>("next_track");
}

export function previousTrack(): Promise<null> {
  return invokeCmd<null>("previous_track");
}

export function seek(positionMs: number): Promise<null> {
  return invokeCmd<null>("seek", { positionMs });
}

export function setVolume(volume: number): Promise<null> {
  return invokeCmd<null>("set_volume", { volume });
}

export function setMuted(muted: boolean): Promise<null> {
  return invokeCmd<null>("set_muted", { muted });
}

export function setPlayMode(mode: PlayMode): Promise<null> {
  return invokeCmd<null>("set_play_mode", { mode });
}

export function getPlayerState(): Promise<PlayerState> {
  return invokeCmd<PlayerState>("get_player_state");
}

// ---- 队列编辑 ----

/** 追加到队列；next=true 插到当前曲目之后（“下一首播放”）。 */
export function queueAdd(ids: string[], next: boolean): Promise<null> {
  return invokeCmd<null>("queue_add", { ids, next });
}

export function queueRemove(index: number): Promise<null> {
  return invokeCmd<null>("queue_remove", { index });
}

export function queueMove(from: number, to: number): Promise<null> {
  return invokeCmd<null>("queue_move", { from, to });
}

/** 清空队列（保留正在播放的当前曲目）。 */
export function queueClear(): Promise<null> {
  return invokeCmd<null>("queue_clear");
}

/**
 * 注册播放状态 Channel（重复注册幂等，后端替换旧 channel）。
 * progress 推送 ≤4Hz；state 为全量 PlayerState。
 */
export function watchPlayer(
  onMessage: (payload: WatchPlayerPayload) => void,
): Promise<null> {
  const channel = new Channel<WatchPlayerPayload>();
  channel.onmessage = onMessage;
  return invokeCmd<null>("watch_player", { channel });
}

// ---- 设置 ----

export function getSettings(): Promise<Settings> {
  return invokeCmd<Settings>("get_settings");
}

export function updateSettings(settings: Settings): Promise<Settings> {
  return invokeCmd<Settings>("update_settings", { settings });
}

// ---- 播放列表 ----

export function listPlaylists(): Promise<PlaylistSummary[]> {
  return invokeCmd<PlaylistSummary[]>("list_playlists");
}

export function createPlaylist(
  name: string,
  description?: string | null,
): Promise<PlaylistSummary> {
  return invokeCmd<PlaylistSummary>("create_playlist", { name, description: description ?? null });
}

export function renamePlaylist(
  id: string,
  name: string,
  description?: string | null,
): Promise<PlaylistSummary> {
  return invokeCmd<PlaylistSummary>("rename_playlist", {
    id,
    name,
    description: description ?? null,
  });
}

export function deletePlaylist(id: string): Promise<null> {
  return invokeCmd<null>("delete_playlist", { id });
}

export function getPlaylistTracks(id: string): Promise<Track[]> {
  return invokeCmd<Track[]>("get_playlist_tracks", { id });
}

/** 追加曲目到列表末尾（允许重复，库中不存在的 id 静默过滤）。 */
export function addToPlaylist(id: string, trackIds: string[]): Promise<null> {
  return invokeCmd<null>("add_to_playlist", { id, trackIds });
}

export function removeFromPlaylist(id: string, index: number): Promise<null> {
  return invokeCmd<null>("remove_from_playlist", { id, index });
}

export function moveInPlaylist(id: string, from: number, to: number): Promise<null> {
  return invokeCmd<null>("move_in_playlist", { id, from, to });
}

// ---- 在线歌词搜索（Phase 5） ----

export interface OnlineLyrics {
  syncedLyrics: string | null;
  plainLyrics: string | null;
}

/** 在线搜索歌词（lrclib.net）。 */
export function searchLyricsOnline(
  title: string,
  artist?: string | null,
  album?: string | null,
): Promise<OnlineLyrics> {
  return invokeCmd<OnlineLyrics>("search_lyrics_online", {
    title,
    artist: artist ?? null,
    album: album ?? null,
  });
}

/** 保存歌词到本地 .lrc 文件。 */
export function saveLyricsFile(trackId: string, lyricsText: string): Promise<null> {
  return invokeCmd<null>("save_lyrics_file", { trackId, lyricsText });
}

/** 在线搜索封面（MusicBrainz + Cover Art Archive）。 */
export function searchCoverOnline(
  artist: string,
  album: string,
): Promise<CoverPayload> {
  return invokeCmd<CoverPayload>("search_cover_online", { artist, album });
}
