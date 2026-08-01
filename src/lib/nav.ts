/** 前端简单路由状态（Phase 1 不引入 react-router）。 */
export type NavPage =
  | "library"
  | "albums"
  | "artists"
  | "playlists"
  | "queue"
  | "stats"
  | "video"
  | "settings";

export const NAV_PAGE_LABEL: Record<NavPage, string> = {
  library: "媒体库",
  albums: "专辑",
  artists: "艺术家",
  playlists: "播放列表",
  queue: "播放队列",
  stats: "统计",
  video: "视频",
  settings: "设置",
};
