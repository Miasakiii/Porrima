# Porrima IPC 契约（Phase 1–2）

前后端唯一权威接口定义。Rust command 名用 snake_case，参数/字段序列化用 camelCase（Rust 侧 `#[serde(rename_all = "camelCase")]`）。所有 command 错误返回 `AppError`：`{ "kind": string, "message": string }`。

## 数据类型

### Track
```jsonc
{
  "id": "string (uuid)",
  "path": "string (绝对路径)",
  "title": "string",
  "artist": "string | null",
  "albumArtist": "string | null",
  "album": "string | null",
  "trackNumber": "number | null",
  "discNumber": "number | null",
  "year": "number | null",
  "genre": "string | null",
  "durationMs": "number",
  "format": "string",          // 小写: "flac" | "mp3" | "m4a" | "aac" | "ogg" | "opus" | "wav" | "aiff" | "ape" | "wv" | "wma" | "dsf" | "dff" | "other"
  "bitrate": "number",         // kbps，未知为 0
  "sampleRate": "number",      // Hz，未知为 0
  "bitDepth": "number | null",
  "channels": "number | null",
  "isLossless": "boolean",
  "hasEmbeddedCover": "boolean",
  "playCount": "number",
  "lastPlayed": "string | null",   // ISO 8601
  "dateAdded": "string",           // ISO 8601
  "fileModified": "string",        // ISO 8601
  "cueSource": {                    // CUE 整轨虚拟曲目；普通文件曲目为 null
    "cuePath": "string",            // CUE 文件绝对路径
    "cueIndex": "number",           // TRACK 序号（01 起）
    "startMs": "number",            // 轨起始（整轨文件内绝对时间）
    "endMs": "number | null"        // 下一轨起始；末轨为 null（播到文件尾）
  }
}
```

> CUE 整轨：扫描时 `.cue` 解析为若干虚拟曲目（共享整轨文件 path，按 `(path, cueIndex)` 去重），被覆盖的整轨文件不再作为单曲展示；CUE 删除后重扫自动恢复。命令行/关联打开 `.cue` 文件展开为全部分轨入队。前端无需感知时间平移：`positionMs`/`durationMs`/`seek` 均为轨内相对时间，窗口映射在 Rust 侧完成。

### Settings
```jsonc
{
  "theme": "'dark' | 'light' | 'system'",   // 默认 dark
  "scanDirs": "string[]"
}
```

### AlbumSummary（`list_albums` 返回元素）
```jsonc
{
  "id": "string",                 // name+albumArtist 的稳定哈希，仅供前端 React key/选中；不用于查询
  "name": "string | null",        // 专辑名；null = 未知专辑（album 为空/NULL）
  "albumArtist": "string | null", // COALESCE(album_artist, artist)；null = 未知艺术家
  "year": "number | null",        // 代表年份（首轨）
  "trackCount": "number",
  "totalDurationMs": "number",
  "coverTrackId": "string"        // 代表曲目 id（首轨），前端用 get_cover 取封面
}
```

### ArtistSummary（`list_artists` 返回元素）
```jsonc
{
  "name": "string | null",        // COALESCE(artist, album_artist)；null = 未知艺术家
  "albumCount": "number",         // 不同专辑数（不计空专辑）
  "trackCount": "number",
  "totalDurationMs": "number"
}
```

### PlaylistSummary（`list_playlists` / `create_playlist` / `rename_playlist` 返回）
```jsonc
{
  "id": "string (uuid)",
  "name": "string",
  "description": "string | null",
  "trackCount": "number",
  "createdAt": "string",          // RFC3339
  "updatedAt": "string"           // RFC3339
}
```

### StatsSummary（`get_stats_summary` 返回）
```jsonc
{
  "trackCount": "number",
  "totalDurationMs": "number",
  "losslessCount": "number",
  "totalPlays": "number",         // 所有曲目 play_count 之和
  "playedCount": "number",        // 有播放记录（play_count>0）的曲目数
  "formats": "{ format: string, count: number }[]"   // 按数量倒序
}
```

### PlayerState（`get_player_state` 返回）
```jsonc
{
  "currentTrackId": "string | null",
  "status": "'playing' | 'paused' | 'stopped'",
  "positionMs": "number",
  "durationMs": "number",
  "volume": "number",              // 0-100 整数
  "muted": "boolean",
  "playMode": "'sequential' | 'shuffle' | 'repeat-one' | 'repeat-all'",
  "queue": "string[]",             // track id 有序列表
  "queueIndex": "number"
}
```

## Commands

### 媒体库
| Command | 参数 | 返回 | 说明 |
|---|---|---|---|
| `scan_library` | 无 | `null` | 扫描 Settings.scanDirs，立即返回，进度走事件 |
| `cancel_scan` | 无 | `null` | 取消进行中的扫描 |
| `list_tracks` | `{ offset: number, limit: number, sortBy?: SortBy, sortDir?: 'asc'|'desc', search?: string }` | `{ tracks: Track[], total: number }` | search 非空时走 FTS5 trigram 匹配 title/artist/album；sortBy: `'title'|'artist'|'album'|'durationMs'|'dateAdded'|'playCount'`，默认 dateAdded desc |
| `get_track` | `{ id: string }` | `Track` | |
| `get_tracks` | `{ ids: string[] }` | `Track[]` | 批量取曲目（队列页）；保持入参顺序，不存在的 id 静默跳过 |
| `get_library_stats` | 无 | `{ trackCount: number, totalDurationMs: number, losslessCount: number }` | |
| `get_stats_summary` | 无 | `StatsSummary` | 统计页概览（总数/时长/无损/总播放/已播放/格式分布） |
| `list_recently_played` | `{ limit: number }` | `Track[]` | 有 last_played 的曲目按时间倒序（limit 限 1-500） |
| `list_most_played` | `{ limit: number }` | `Track[]` | play_count>0 按次数倒序（limit 限 1-500） |
| `list_albums` | 无 | `AlbumSummary[]` | 按 (albumArtist, name) 升序，未知分组排最后；空库返回 `[]` |
| `list_artists` | 无 | `ArtistSummary[]` | 按 name 升序，未知分组排最后 |
| `get_album_tracks` | `{ album: string \| null, albumArtist: string \| null }` | `Track[]` | 按碟号/轨号/标题排序；key 与 `list_albums` 一致，未知专辑传 null |
| `get_artist_tracks` | `{ artist: string \| null }` | `Track[]` | 按专辑/碟号/轨号排序；未知艺术家传 null |
| `get_cover` | `{ id: string }` | `CoverPayload \| null` | 内嵌图片优先，回退同目录 cover/folder/front × jpg/jpeg/png/webp；无封面返回 null |
| `get_cover_color` | `{ id: string }` | `CoverColor \| null` | 封面代表色（缩放 64x64 + 饱和度加权直方图），会话内缓存；无封面/解码失败返回 null。前端用于动态强调色 |
| `get_lyrics` | `{ id: string }` | `LyricsPayload \| null` | 同目录同名 .lrc 优先（UTF-8/GBK 自动探测），回退内嵌歌词标签；无歌词返回 null |

#### CoverPayload
```jsonc
{ "mimeType": "string", "dataBase64": "string" }   // 图片二进制的 base64，前端拼 data URI 使用
```

#### CoverColor
```jsonc
{ "r": "number", "g": "number", "b": "number" }   // sRGB 0-255；前端换算到 OKLCH 并按主题裁剪后写入 --accent
```

#### LyricsPayload
```jsonc
{ "source": "'lrcFile' | 'embedded'", "text": "string" }   // 原始歌词文本，LRC 时间轴解析在前端 src/lib/lrc.ts
```

### 播放
| Command | 参数 | 返回 | 说明 |
|---|---|---|---|
| `play_track` | `{ id: string }` | `null` | 播放指定曲目，并将其所在上下文（当前列表）设为队列 |
| `play_queue` | `{ ids: string[], startIndex: number }` | `null` | 显式设置队列并播放 |
| `toggle_play` | 无 | `null` | 播放/暂停切换 |
| `stop` | 无 | `null` | |
| `next_track` / `previous_track` | 无 | `null` | 按 playMode 语义切换 |
| `seek` | `{ positionMs: number }` | `null` | |
| `set_volume` | `{ volume: number }` | `null` | 0-100 |
| `set_muted` | `{ muted: boolean }` | `null` | |
| `set_play_mode` | `{ mode: PlayMode }` | `null` | |
| `get_player_state` | 无 | `PlayerState` | 前端启动时拉一次全量 |

### 队列编辑
| Command | 参数 | 返回 | 说明 |
|---|---|---|---|
| `queue_add` | `{ ids: string[], next: boolean }` | `null` | next=true 插到当前曲目之后（“下一首播放”），否则追加末尾；不打断播放，空队列时仅入列不自动播放；无效 id 静默过滤，全部无效报 invalid_argument |
| `queue_remove` | `{ index: number }` | `null` | 移除当前曲目时：播放中→自动播下一首；队列清空→停止；越界无动作 |
| `queue_move` | `{ from: number, to: number }` | `null` | 拖拽排序；当前曲目索引跟随调整；越界/同位无动作 |
| `queue_clear` | 无 | `null` | 清空队列，保留正在播放/暂停的当前曲目（不打断播放） |

所有队列命令执行后都推送全量 `{kind:"state"}`。

### 设置
| Command | 参数 | 返回 | 说明 |
|---|---|---|---|
| `get_settings` | 无 | `Settings` | |
| `update_settings` | `{ settings: Settings }` | `Settings` | 全量替换，返回生效值 |

### 播放列表
| Command | 参数 | 返回 | 说明 |
|---|---|---|---|
| `list_playlists` | 无 | `PlaylistSummary[]` | 按最近更新倒序 |
| `create_playlist` | `{ name: string, description?: string \| null }` | `PlaylistSummary` | 空名（trim 后）报 invalid_argument |
| `rename_playlist` | `{ id: string, name: string, description?: string \| null }` | `PlaylistSummary` | 改名/改描述；空名报 invalid_argument |
| `delete_playlist` | `{ id: string }` | `null` | 删列表，曲目关联由外键级联清理 |
| `get_playlist_tracks` | `{ id: string }` | `Track[]` | 按 position 排序；已删曲目自然不在结果中 |
| `add_to_playlist` | `{ id: string, trackIds: string[] }` | `null` | 追加末尾，允许重复；库中不存在的 id 静默过滤；列表不存在报 not_found |
| `remove_from_playlist` | `{ id: string, index: number }` | `null` | 按展示顺序下标移除；越界无动作 |
| `move_in_playlist` | `{ id: string, from: number, to: number }` | `null` | 拖拽重排；越界/同位无动作 |

## 事件与 Channel

- **播放进度（高频，用 `tauri::ipc::Channel`）**：前端 `invoke('watch_player', { channel })` 注册一次（重复注册幂等，替换旧 channel）。payload：
  ```jsonc
  { "kind": "progress", "positionMs": number, "durationMs": number }
  { "kind": "state", "state": PlayerState }        // 切歌/暂停/模式变化等全量推送
  ```
  progress 推送频率 ≤ 4Hz。
- **扫描进度（低频，用 tauri event）**：事件名 `library:scan-progress`，payload `{ scannedFiles: number, totalFiles: number | null, currentPath: string, done: boolean, error: string | null }`，每 100 个文件或 500ms 至少一次。
- 前端封装统一在 `src/lib/ipc.ts`（invoke）与 `src/lib/events.ts`（事件/Channel 订阅 hook）。

## 引擎适配（内部接口）

> 非前后端产品契约，是 Phase 0 决策（事件走「前端适配器 invoke 转发」）落地的内部接口。mpv 控制面只在前端（tauri-plugin-libmpv JS API），Rust `PlayerCore` 拥有状态机。实现见 `src/lib/engine.ts` 与 `src-tauri/src/commands/player.rs`。

### `engine_event`（前端适配器 → Rust command）
转发 mpv 属性/事件。参数 `{ event: string, value?: number | boolean }`，返回 `null`。时间值单位为**秒**（mpv 原生），Rust 侧换算为 ms。

| event | value | 处理 |
|---|---|---|
| `time-pos` | number（秒） | 更新位置，节流 ≤4Hz 后推 progress |
| `duration` | number（秒） | 更新时长，推 progress |
| `pause` | boolean | 外部暂停/恢复，推 state |
| `file-loaded` | number（秒，时长） | 新文件加载，推 state |
| `end-file` | 无 | 自然播完（前端以 `eof-reached=true` 触发），状态机推进后推 state |

### `engine_ready`（前端适配器 → Rust command）
适配器 init 成功后调用。无参数，返回 `null`。Rust 侧同步一次音量到引擎，并处理启动时挂起的命令行/文件关联待播放文件。

### `engine:cmd`（Rust → 前端适配器，tauri event）
`PlayerCore` 产出的动作，前端适配器执行对应插件调用。payload（tagged，camelCase）：
```jsonc
{ "kind": "load", "path": string, "startMs": number | null, "endMs": number | null }
                                      // loadfile + 取消暂停；CUE 虚拟曲目附带区间（整轨内绝对 ms），
                                      // 适配器设 mpv start/end 选项后 loadfile；普通曲目为 null（复位 none）
{ "kind": "pause" }                   // set pause=true
{ "kind": "resume" }                  // set pause=false
{ "kind": "stop" }                    // stop
{ "kind": "seek", "positionMs": number }   // seek absolute（转秒；CUE 曲目已平移为整轨绝对时间）
{ "kind": "setVolume", "volume": number }  // set volume 0-100
```
