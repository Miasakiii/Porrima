# Porrima — 轻量跨平台音视频播放器

> 一个现代 UI、轻量安装、音乐/视频双模式分离的跨平台桌面播放器。

---

## 一、项目定位

### 为什么做这个

- VLC 全能但 UI 粗糙，什么都能播但什么都不精致
- foobar2000 极客但学习成本高，只做音频
- Aria 只做音乐且仅 Windows，不支持视频
- 主流播放器要么臃肿（Electron 200MB+），要么过时（Winamp/WMP）

### 差异化

| 维度 | VLC | foobar2000 | Aria | 本项目 |
|------|-----|-----------|------|--------|
| 安装包 | ~120MB | ~10MB | ~150MB+ | **~37MB** |
| UI 现代感 | ✗ | ✗ | ✓ | ✓ |
| 音乐模式 | 基础 | 强 | 强 | 强 |
| 视频模式 | 强 | ✗ | ✗ | 强 |
| 音视频分离 | ✗ | ✗ | — | ✓ |
| HiFi 输出 | 部分 | ✓ | ✓ | ✓ |
| 跨平台 | ✓ | Windows | Windows | ✓ |

**一句话**：用 VLC 的格式覆盖能力，做出 Aria 的 UI 体验，包体控制在 VLC 的 1/3。

---

## 二、技术栈

### 2.1 总览

| 层 | 选型 | 理由 |
|----|------|------|
| 应用框架 | **Tauri v2** | 安装包 ~3MB 骨架，跨平台，2026 年生产就绪 |
| 播放引擎 | **libmpv** | 全格式音视频解码，硬件加速，WASAPI 独占输出 |
| 前端 | **React 19 + TypeScript + Tailwind CSS v4** | 生态最大，组件库全，Tauri 官方模板支持 |
| 组件库 | **shadcn/ui** | 无依赖，可自由修改，适合桌面应用 |
| 状态管理 | **Zustand** | 轻量，无 boilerplate |
| 音频元数据 | **lofty** (Rust) | 读取 ID3/Vorbis/FLAC/APE/MP4 标签和封面 |
| CUE 解析 | 自研 / cue_sheet crate | 整轨文件分轨处理 |
| 嵌入式数据库 | **rusqlite (SQLite) + FTS5** | 媒体库索引、模糊搜索（FTS5 全文索引）、播放记录、设置存储 |
| 文件监听 | **notify** (Rust) | 跨平台文件系统事件，增量更新媒体库 |
| 系统媒体控制 | **souvlaki** (Rust) | SMTC(Win) / MPRIS(Linux) / Now Playing(Mac) 统一封装，媒体键响应 |
| HTTP 客户端 | **reqwest** (Rust) | 在线歌词/封面搜索 |
| 图像处理 | **image** (Rust) | 封面缩放、主题色提取 |

### 2.2 播放引擎决策：为什么统一用 libmpv

**核心理由**：mpv 一个引擎覆盖全部音视频格式，不造轮子。

| 格式类别 | 具体格式 |
|----------|----------|
| 音频容器 | MP3, AAC, M4A, FLAC, APE, WV(WavPack), WAV, AIFF, OGG, Opus, WMA, DSD(DSF/DFF), CAF |
| 视频容器 | MP4, MKV, AVI, MOV, WebM, FLV, TS, WMV, RMVB |
| 视频编码 | H.264, H.265/HEVC, VP9, AV1, MPEG-2/4 |
| 音频编码 | AAC, MP3, FLAC, AC3, DTS, DTS-HD, TrueHD, PCM |
| 字幕 | SRT, ASS, SSA, PGS, VobSub |
| 网络流 | HTTP, HLS(m3u8), RTMP, RTSP |

**mpv 还提供**：
- 硬件加速：DXVA2(Win) / VideoToolbox(Mac) / VAAPI-Vulkan(Linux)
- 音频直出：WASAPI 独占(Win) / CoreAudio(Mac) / ALSA(Linux)
- 音频处理：均衡器、音量归一化、声道映射

**已有生态**：`tauri-plugin-libmpv` crate（nini22P/tauri-plugin-libmpv）— Tauri v2 原生插件，可直接集成。

**为什么不用 Symphonia 做播放**：
- 缺少 APE、WavPack、DSD、WMA、AC3、DTS 支持
- 无硬件加速、无 WASAPI 独占输出
- 无视频能力
- 维护 Symphonia（音乐）+ mpv（视频）两套引擎增加复杂度

**Symphonia 的定位**：仅用于元数据提取的备选方案，主元数据读取走 lofty。

### 2.3 前端技术选型理由

| 选项 | 采纳 | 理由 |
|------|------|------|
| React vs Vue vs Svelte | React 19 | 生态最大、shadcn/ui 支持最好、Tauri 官方模板首选 |
| Tailwind vs CSS Modules vs styled-components | Tailwind v4 | 原子化 CSS，主题系统完善，适合播放器 UI 定制 |
| Zustand vs Redux vs Jotai | Zustand | API 极简，无 boilerplate，适合中等复杂度状态 |

### 2.4 数据库决策：为什么用 rusqlite + FTS5 而非 sled

- **sled 风险高**：1.0 长期未发布，维护近乎停滞，不适合作为核心存储
- **需求本质是关系型**：模糊搜索、多字段排序、按艺术家/专辑/年份/格式聚合、分页、播放统计排行——KV 存储全部需要手写内存索引
- **FTS5 直接覆盖模糊搜索**：标题/艺术家/专辑全文索引，中文分词可用 unicode61 tokenizer
- **迁移机制成熟**：版本化 schema migration（`user_version` PRAGMA），有大量成熟实践
- **包体增量仅 ~1MB**：rusqlite `bundled` 特性静态编译 SQLite，无外部依赖

---

## 三、架构设计

### 3.1 分层架构图

```
┌─────────────────────────────────────────────────────────┐
│                      UI Layer (Web)                       │
│                                                           │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐ │
│  │  音乐模式     │  │  视频模式     │  │    设置页面     │ │
│  │              │  │              │  │                │ │
│  │  · 媒体库    │  │  · 文件浏览   │  │  · 主题/外观   │ │
│  │  · 专辑/艺人 │  │  · 视频播放   │  │  · 音频输出    │ │
│  │  · 播放列表  │  │  · 字幕管理   │  │  · 扫描目录    │ │
│  │  · 歌词显示  │  │  · 音轨切换   │  │  · 在线功能    │ │
│  │  · 频谱可视化│  │  · 画中画     │  │  · 快捷键     │ │
│  │  · 均衡器    │  │  · 截图       │  │  · 语言       │ │
│  │  · 播放队列  │  │  · 画面调节   │  │  · 关于       │ │
│  └──────────────┘  └──────────────┘  └────────────────┘ │
│                                                           │
│  ┌─────────────────────────────────────────────────────┐ │
│  │                 PlayerBar (全局底栏)                  │ │
│  │  封面 | 曲目信息 | 播放控制 | 进度条 | 音量 | 功能入口 │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                           │
├─────────────────────────────────────────────────────────┤
│                 Tauri IPC Bridge (Commands)               │
│          前端 invoke() ←→ Rust #[tauri::command]          │
│          Rust emit() → 前端 listen()  (事件推送)          │
├─────────────────────────────────────────────────────────┤
│                   Service Layer (Rust)                     │
│                                                           │
│  ┌────────────┐ ┌────────────┐ ┌──────────────────────┐ │
│  │ PlayerSvc  │ │ LibrarySvc │ │ MetadataSvc          │ │
│  │            │ │            │ │                      │ │
│  │ · mpv 生命周期管理│ │ · 媒体库索引  │ │ · lofty 标签读取    │ │
│  │ · 播放控制  │ │ · 增量扫描   │ │ · 封面提取           │ │
│  │ · 音频输出  │ │ · 搜索/排序  │ │ · 主题色计算         │ │
│  │ · seek/音量 │ │ · 统计       │ │ · CUE 解析          │ │
│  └────────────┘ └────────────┘ └──────────────────────┘ │
│                                                           │
│  ┌────────────┐ ┌────────────┐ ┌──────────────────────┐ │
│  │ SettingsSvc│ │ OnlineSvc  │ │ FileWatchSvc         │ │
│  │            │ │            │ │                      │ │
│  │ · 配置持久化│ │ · 歌词搜索  │ │ · 文件系统事件监听    │ │
│  │ · 默认值   │ │ · 封面补全  │ │ · 新增/删除/修改通知  │ │
│  │ · 导入导出 │ │ · 可选模块  │ │ · 增量更新触发        │ │
│  └────────────┘ └────────────┘ └──────────────────────┘ │
│                                                           │
├─────────────────────────────────────────────────────────┤
│                   Core Layer (Crates)                      │
│                                                           │
│  ┌──────────┐ ┌──────┐ ┌──────┐ ┌───────┐ ┌─────────┐ │
│  │ libmpv   │ │lofty │ │sqlite│ │reqwest│ │ notify  │ │
│  │ 播放引擎  │ │元数据│ │数据库│ │ HTTP  │ │文件监听  │ │
│  └──────────┘ └──────┘ └──────┘ └───────┘ └─────────┘ │
│  ┌──────────┐ ┌──────────┐                              │
│  │ image    │ │ serde    │                              │
│  │图像处理   │ │序列化     │                              │
│  └──────────┘ └──────────┘                              │
└─────────────────────────────────────────────────────────┘
```

### 3.2 数据流

#### 播放一首歌

```
用户双击曲目
    │
    ▼
UI 组件 → invoke('play_track', { id })
    │
    ▼
Rust PlayerSvc
    ├── LibrarySvc.get_track(id) → 获取文件路径
    ├── mpv.loadfile(path) → 开始解码播放
    ├── MetadataSvc.get_cover(id) → 提取封面 → emit('cover:update', data)
    ├── MetadataSvc.get_lyrics(id) → 提取歌词 → emit('lyrics:update', data)
    └── LibrarySvc.record_play(id) → 更新播放统计
    │
    ▼
mpv 播放事件循环
    ├── on_file_loaded → emit('player:track_loaded', metadata)
    ├── on_time_pos    → emit('player:progress', { pos, duration })
    ├── on_end_file    → emit('player:ended') → PlayerSvc 按模式播放下一首
    └── on_pause       → emit('player:paused')
    │
    ▼
前端 listen 事件 → 更新 UI 状态
```

#### 搜索歌词（在线功能）

```
用户点击"搜索歌词"
    │
    ▼
UI → invoke('search_lyrics', { title, artist, album })
    │
    ▼
Rust OnlineSvc
    ├── reqwest::get(lrclib_api_url) → JSON 响应
    ├── 解析匹配结果
    └── 返回候选歌词列表
    │
    ▼
UI 展示候选 → 用户选择 → invoke('save_lyrics', { track_id, lyrics })
    │
    ▼
Rust MetadataSvc
    ├── 写入 .lrc 文件（同目录）
    └── 更新数据库记录
```

### 3.3 数据模型

```rust
/// 曲目
struct Track {
    id: String,                    // UUID
    path: PathBuf,                 // 文件绝对路径
    title: String,                 // 标题（来自标签或文件名）
    artist: Option<String>,        // 艺术家
    album_artist: Option<String>,  // 专辑艺术家
    album: Option<String>,         // 专辑
    track_number: Option<u32>,     // 曲目号
    disc_number: Option<u32>,      // 碟号
    year: Option<u32>,             // 年份
    genre: Option<String>,         // 流派
    duration_ms: u64,              // 时长（毫秒）
    format: MediaFormat,           // 格式
    bitrate: u32,                  // 码率 (kbps)
    sample_rate: u32,              // 采样率 (Hz)
    bit_depth: u8,                 // 位深
    channels: u8,                  // 声道数
    is_lossless: bool,             // 是否无损
    has_embedded_cover: bool,      // 是否有内嵌封面
    lyrics_source: LyricsSource,   // 歌词来源（内嵌/lrc文件/在线）
    play_count: u32,               // 播放次数
    last_played: Option<DateTime>, // 最后播放时间
    date_added: DateTime,          // 添加时间
    file_modified: DateTime,       // 文件修改时间（用于增量更新）
    cue_source: Option<CueInfo>,   // CUE 来源信息
}

/// CUE 整轨信息
struct CueInfo {
    cue_path: PathBuf,             // CUE 文件路径
    index: u32,                    // 在 CUE 中的序号
    start_time_ms: u64,            // 起始时间
    end_time_ms: Option<u64>,      // 结束时间（下一轨或文件结尾）
}

/// 媒体格式枚举
enum MediaFormat {
    // 无损
    Flac, Alac, Ape, WavPack, Wav, Aiff, Dsd,
    // 有损
    Mp3, Aac, OggVorbis, Opus, Wma,
    // 视频
    Mp4, Mkv, Avi, Mov, WebM, Flv, Ts, Wmv,
    // 其他
    Other(String),
}

/// 播放列表
struct Playlist {
    id: String,
    name: String,
    description: Option<String>,
    track_ids: Vec<String>,        // 有序曲目 ID 列表
    created_at: DateTime,
    updated_at: DateTime,
}

/// 专辑
struct Album {
    id: String,                    // album_artist + album name 哈希
    name: String,
    artist: Option<String>,
    year: Option<u32>,
    cover_path: Option<PathBuf>,   // 缓存的封面文件路径
    track_ids: Vec<String>,
}

/// 播放状态
struct PlayerState {
    current_track: Option<String>, // Track ID
    status: PlayStatus,            // Playing / Paused / Stopped
    position_ms: u64,              // 当前位置
    duration_ms: u64,              // 总时长
    volume: f32,                   // 音量 0.0-1.0
    muted: bool,
    play_mode: PlayMode,           // 顺序/随机/单曲循环/列表循环
    queue: Vec<String>,            // 播放队列
    queue_index: usize,            // 当前队列位置
}

enum PlayStatus { Playing, Paused, Stopped }
enum PlayMode { Sequential, Shuffle, RepeatOne, RepeatAll }

/// 应用设置
struct Settings {
    theme: Theme,                  // Light / Dark / System
    language: String,              // zh-CN / en-US
    scan_dirs: Vec<PathBuf>,       // 媒体扫描目录
    audio_output: AudioOutputConfig,
    online_features: OnlineConfig,
    shortcuts: HashMap<String, String>,
    window_state: WindowState,
}

enum Theme { Light, Dark, System }

struct AudioOutputConfig {
    backend: AudioBackend,         // System / WasapiShared / WasapiExclusive
    device: Option<String>,        // 输出设备名称
    buffer_ms: u32,                // 缓冲区大小
}

enum AudioBackend { System, WasapiShared, WasapiExclusive }

struct OnlineConfig {
    enabled: bool,
    auto_search_lyrics: bool,      // 自动搜索歌词
    auto_search_cover: bool,       // 自动补全封面
}
```

---

## 四、功能模块详细设计

### 4.1 音乐模式

#### 4.1.1 媒体库

| 功能 | 说明 |
|------|------|
| 目录扫描 | 用户指定一个或多个目录，递归扫描支持的音频格式 |
| 增量更新 | `notify` 监听文件系统，新增/删除/修改自动同步数据库 |
| CUE 整轨 | 检测 `.cue` 文件 → 解析 REM/FILE/TRACK/INDEX → 创建虚拟曲目，播放时 mpv seek 到对应时间段 |
| 智能分类 | 按艺术家、专辑、格式、年份、流派自动归类 |
| 搜索过滤 | 标题、艺术家、专辑模糊搜索（SQLite FTS5 全文索引），支持格式/无损等过滤器 |
| 排序 | 按标题、艺术家、专辑、时长、码率、添加时间、播放次数排序 |

#### 4.1.2 播放控制

| 功能 | 说明 |
|------|------|
| 基本控制 | 播放 / 暂停 / 停止 |
| 切歌 | 上一首 / 下一首 |
| 播放模式 | 顺序播放 / 随机播放 / 单曲循环 / 列表循环 |
| 进度条 | 拖拽 seek，显示当前时间 / 总时长 |
| 音量 | 滑块控制 0-100%，静音切换，鼠标滚轮调节 |
| 播放队列 | 插入、移除、拖拽排序，"播放下一首"功能 |
| 倍速 | 0.5x / 0.75x / 1.0x / 1.25x / 1.5x / 2.0x / 3.0x |
| 均衡器 | 预设（流行/摇滚/古典/爵士/自定义）+ 手动调节各频段 |

#### 4.1.3 音频输出

| 模式 | 平台 | 说明 |
|------|------|------|
| 系统默认 | 全平台 | 走系统混音器，兼容性最好 |
| WASAPI 共享 | Windows | 低延迟，与其他应用共存 |
| WASAPI 独占 | Windows | 绕过混音器，HiFi 直出，独占声卡 |
| CoreAudio | macOS | 系统原生高质量输出 |
| ALSA / PulseAudio | Linux | 系统音频 |

支持选择输出设备（多声卡场景）。

#### 4.1.4 歌词

| 功能 | 说明 |
|------|------|
| 内嵌歌词 | 读取音频文件内的歌词标签（LYRICS/UNSYNCEDLYRICS） |
| LRC 文件 | 自动加载同目录同名 `.lrc` 文件 |
| 在线搜索 | 调用 lrclib.net API，按标题+艺术家匹配（可选功能） |
| 同步显示 | LRC 时间轴驱动，高亮当前行，平滑滚动 |
| 歌词编辑 | 手动调整时间偏移（±ms） |
| 纯文本歌词 | 无时间轴的歌词居中显示 |

#### 4.1.5 封面与视觉

| 功能 | 说明 |
|------|------|
| 内嵌封面 | lofty 读取 ID3 APIC / FLAC PICTURE / MP4 covr |
| 本地封面 | 自动检测同目录 cover.jpg / folder.jpg / front.jpg |
| 在线补全 | MusicBrainz + Cover Art Archive API（可选） |
| 主题色 | image crate 缩放封面到 64x64 → k-means 取主色 → 动态调整 UI 强调色 |
| 频谱可视化 | Rust 侧 FFT：mpv 音频回调取 PCM → rustfft 计算 → 每帧仅推送 32~64 个频段值到前端 Canvas 绘制（P2，详见第七章难点表） |
| 波形显示 | 可选，显示音频波形 |

#### 4.1.6 统计

| 功能 | 说明 |
|------|------|
| 播放计数 | 每次完整播放（>30s 或 >50%）计数 +1 |
| 最近播放 | 按时间倒序，快速回到最近听的歌 |
| 常听排行 | 按播放次数排序 |
| 格式统计 | 各格式占比、无损/有损占比 |

#### 4.1.7 无缝播放与音量归一化（HiFi 必备）

| 功能 | 说明 |
|------|------|
| 无缝播放 (Gapless) | mpv `--gapless-audio=yes`，专辑连续曲目零间隙衔接（Live 专辑/古典乐场景必备） |
| ReplayGain | lofty 读取标签中的 RG 信息，mpv `--replaygain=track/album` 应用增益 |
| 无 RG 标签降级 | 可选启用 mpv `loudnorm` 滤镜做实时响度归一化 |

### 4.2 视频模式

#### 4.2.1 文件浏览

| 功能 | 说明 |
|------|------|
| 目录浏览 | 树形目录，过滤支持的视频格式 |
| 缩略图 | mpv 截帧生成视频缩略图（可选，有性能开销） |
| 最近播放 | 记录上次播放位置，支持续播 |
| 文件信息 | 显示分辨率、编码、时长、文件大小 |

#### 4.2.2 播放控制

| 功能 | 说明 |
|------|------|
| 基本控制 | 同音乐模式 |
| 全屏 | F11 切换，双击切换 |
| 画中画 | 系统级画中画窗口（平台支持时） |
| 画面比例 | 原始 / 16:9 / 4:3 / 2.35:1 / 填充 / 自适应 |
| 画面旋转 | 0° / 90° / 180° / 270° |
| 色彩调节 | 亮度 / 对比度 / 饱和度 / 色调 / 伽马 |
| 跳转 | 进度条 seek，键盘 ← → 逐帧，长按倍速 |

#### 4.2.3 字幕

| 功能 | 说明 |
|------|------|
| 自动加载 | 同目录同名字幕文件，mpv 自动检测 |
| 手动加载 | 菜单选择字幕文件 |
| 多字幕轨 | 内嵌字幕轨 + 外挂字幕轨切换 |
| 样式调整 | 字号、颜色、阴影、位置、背景 |
| 时间偏移 | 字幕延迟/提前 ±ms |

#### 4.2.4 音轨

| 功能 | 说明 |
|------|------|
| 多音轨切换 | 内嵌多音轨选择（如多语言） |
| 音频延迟 | 音频超前/滞后调整（唇 sync） |

#### 4.2.5 截图

| 功能 | 说明 |
|------|------|
| 截取当前帧 | 保存为 PNG，含选项：原始分辨率 / 按窗口 |
| 截图目录 | 可配置保存位置，默认"图片/播放器截图" |

### 4.3 通用功能

#### 4.3.1 设置

| 分类 | 设置项 |
|------|--------|
| 外观 | 主题（亮/暗/跟随系统）、强调色、字体大小 |
| 语言 | 中文 / English |
| 音频 | 输出后端、输出设备、缓冲区大小、音量归一化 |
| 媒体库 | 扫描目录管理、忽略规则（如 .nomedia） |
| 在线功能 | 开关、自动搜索歌词、自动补全封面、缓存管理 |
| 快捷键 | 查看/自定义所有快捷键 |
| 窗口 | 启动时窗口大小/位置、最小化到托盘 |
| 关于 | 版本、更新检查、开源许可、项目主页 |

#### 4.3.2 默认快捷键

| 操作 | 快捷键 | 操作 | 快捷键 |
|------|--------|------|--------|
| 播放/暂停 | Space | 全屏 | F11 |
| 上一首 | Ctrl+← | 截图 | F12 |
| 下一首 | Ctrl+→ | 倍速+ | ] |
| 音量+ | Ctrl+↑ | 倍速- | [ |
| 音量- | Ctrl+↓ | 画面比例 | A |
| 静音 | Ctrl+M | 字幕切换 | S |
| 退出 | Ctrl+Q | 音轨切换 | D |

#### 4.3.3 系统托盘

- 最小化到托盘，不退出
- 托盘右键菜单：播放/暂停、上一首、下一首、显示窗口、退出
- 托盘图标 tooltip 显示当前曲目

#### 4.3.4 文件关联与单实例

| 功能 | 说明 |
|------|------|
| 文件关联 | 安装时注册支持的音视频扩展名，系统"打开方式"可选本应用 |
| 双击打开 | 关联文件双击直接启动播放，按文件类型自动进入音乐/视频模式 |
| 单实例 | tauri-plugin-single-instance，二次启动时将文件路径转发给已运行实例 |
| 命令行参数 | `porrima <file...>` 直接播放，多文件自动入队 |

#### 4.3.5 系统媒体控制

| 平台 | 集成方式 | 说明 |
|------|----------|------|
| Windows | SMTC | 系统媒体浮窗显示封面/曲目信息，媒体键控制播放 |
| macOS | Now Playing | 控制中心 / 触控栏集成 |
| Linux | MPRIS | 兼容 playerctl / GNOME 媒体控制 |
| 全平台 | 全局媒体键 | Play/Pause/Next/Prev 硬件键响应，souvlaki crate 统一封装 |

---

## 五、项目目录结构

> 下为 **Phase 1 已落地的实际结构**。后续阶段计划新增的模块（歌词/封面/CUE/视频/在线/播放列表/频谱/均衡器等）见文末清单与第六章路线图，届时补充到对应目录。

```
Porrima/
│
├── src-tauri/                          # Rust 后端
│   ├── src/
│   │   ├── main.rs                     # 可执行入口
│   │   ├── lib.rs                      # 库入口：插件注册、命令装配、启动
│   │   ├── error.rs                    # 统一错误类型 AppError
│   │   │
│   │   ├── commands/                   # Tauri Commands（IPC 接口层）
│   │   │   ├── mod.rs                  # AppState、ping
│   │   │   ├── player.rs               # 播放命令 + 引擎适配（engine_event / engine:cmd）
│   │   │   ├── library.rs              # 媒体库：scan/cancel/list/get/stats
│   │   │   └── settings.rs             # 设置：get/update
│   │   │
│   │   ├── services/                   # 业务逻辑层
│   │   │   ├── mod.rs
│   │   │   ├── player.rs               # PlayerCore 播放状态机（纯逻辑，可单测）
│   │   │   ├── library.rs              # 目录扫描、增删同步、import_files
│   │   │   ├── metadata.rs             # lofty 标签读取
│   │   │   └── settings.rs             # 设置读写、目录校验
│   │   │
│   │   ├── models/                     # 数据模型
│   │   │   ├── mod.rs
│   │   │   ├── track.rs                # Track, MediaFormat
│   │   │   ├── player_state.rs         # PlayerState, PlayStatus, PlayMode
│   │   │   └── settings.rs             # Settings, Theme
│   │   │
│   │   └── db/                         # 数据持久化
│   │       ├── mod.rs
│   │       ├── store.rs                # rusqlite 连接封装、FTS5 查询
│   │       └── migrations.rs           # schema 版本迁移（user_version）
│   │
│   ├── capabilities/default.json       # 权限能力（core/window/opener/libmpv）
│   ├── lib/                            # 随包分发的 libmpv 运行库（libmpv-2.dll 等）
│   ├── icons/                          # 应用图标
│   ├── Cargo.toml
│   ├── build.rs
│   └── tauri.conf.json                 # Tauri 配置
│
├── src/                                # React 前端
│   ├── main.tsx                        # 入口
│   ├── App.tsx                         # 根组件、布局、启动初始化
│   │
│   ├── components/
│   │   ├── ui/                         # shadcn/ui 基础组件（button/dialog/slider/...）
│   │   ├── TitleBar.tsx                # 自定义标题栏
│   │   ├── PlayerBar/PlayerBar.tsx     # 底部播放条
│   │   ├── Sidebar/Sidebar.tsx         # 侧边栏导航
│   │   └── TrackList/TrackList.tsx     # 曲目列表（虚拟滚动）
│   │
│   ├── pages/
│   │   ├── Music/LibraryPage.tsx       # 曲库首页
│   │   ├── Settings/SettingsPage.tsx   # 设置（主题 + 扫描目录）
│   │   └── PlaceholderPage.tsx         # 队列/统计占位页
│   │
│   ├── hooks/
│   │   └── useKeyboard.ts              # 全局快捷键
│   │
│   ├── stores/                         # 状态管理（Zustand）
│   │   ├── playerStore.ts              # 播放状态 + 引擎命令封装
│   │   ├── libraryStore.ts             # 媒体库缓存、扫描进度
│   │   └── settingsStore.ts            # 设置状态
│   │
│   ├── lib/                            # 工具库
│   │   ├── commands.ts                 # IPC 命令类型化封装
│   │   ├── engine.ts                   # libmpv 引擎适配器（前端控制面）
│   │   ├── events.ts                   # 事件 / Channel 订阅
│   │   ├── ipc.ts                      # invoke 封装 + 错误归一化
│   │   ├── types.ts                    # 契约类型
│   │   ├── theme.ts                    # 主题应用与解析
│   │   ├── format.ts / nav.ts / utils.ts
│   │
│   └── styles/globals.css              # Tailwind v4 入口 + 全局样式
│
├── docs/                               # 设计文档
│   ├── ipc-contract.md                 # 前后端 IPC 契约（含引擎适配内部接口）
│   └── phase0-findings.md              # 引擎选型技术验证结论
│
├── public/                             # 静态资源
├── index.html
├── package.json
├── tsconfig.json / tsconfig.node.json
├── vite.config.ts                      # Vite + @tailwindcss/vite（Tailwind v4 无独立 config）
├── components.json                     # shadcn/ui 配置
├── README.md
└── PROJECT.md
```

**后续阶段计划新增**（见第六章路线图，尚未落地）：

- 后端 `services/`：`cue_parser.rs`、`lyrics.rs`、`cover.rs`、`media_controls.rs`（souvlaki）、`file_watch.rs`（notify）；`utils/`（主题色 k-means、路径工具）
- 后端 `models/`：`playlist.rs`、`album.rs`（含 CueInfo）；`commands/`：`playlist.rs`、`online.rs`
- 前端 `components/`：`LyricsOverlay/`、`Spectrum/`、`Equalizer/`、`VideoPlayer/`
- 前端 `pages/`：专辑 / 艺术家 / 播放列表 / 队列 / 统计、视频模式（浏览器 + 播放页）、设置细分页

---

## 六、开发路线图

### Phase 1：骨架 + 音乐 MVP（3-4 周）— ✅ 已完成（2026-07-29）

> **里程碑**：能扫描目录，播放音乐，显示基本信息。**已达成**。

| 任务 | 优先级 | 预计 | 状态 |
|------|--------|------|------|
| Tauri v2 项目初始化，目录结构搭建 | P0 | 2d | ✅ |
| 集成 libmpv（tauri-plugin-libmpv），验证跨平台播放 | P0 | 3d | ✅ Win |
| Rust：PlayerService 基础（play/pause/stop/seek/volume） | P0 | 3d | ✅ |
| Rust：lofty 元数据读取 + 目录扫描 | P0 | 3d | ✅ |
| Rust：SQLite 数据库（rusqlite + FTS5）+ Track CRUD | P0 | 2d | ✅ |
| 前端：基础布局（Sidebar + 内容区 + PlayerBar） | P0 | 3d | ✅ |
| 前端：曲库列表页（虚拟滚动） | P0 | 2d | ✅ |
| 前端：播放控制 UI + 进度条 + 音量 | P0 | 2d | ✅ |
| IPC 联调：前端 ↔ Rust 播放和媒体库命令 | P0 | 2d | ✅ |
| 基础主题（亮/暗） | P1 | 1d | ✅ 含跟随系统 |
| 单实例 + 命令行参数打开文件（tauri-plugin-single-instance） | P1 | 1d | ✅ |

**交付物**：可运行的桌面应用，能扫描本地音乐目录并播放。**已交付**——`cargo test` 46 项通过、`pnpm build` 通过、NSIS 安装包实测 32.3MB（≤40MB）。

> 说明：跨平台播放当前仅 **Windows** 完整验证；macOS / Linux 为保留目标，待后续阶段验证。引擎链路架构（前端适配器 ↔ Rust PlayerCore ↔ `engine:cmd`）见 `docs/phase0-findings.md` 与 `docs/ipc-contract.md`。

#### Phase 1 验收清单

**功能验收**（对照里程碑“能扫描目录、播放音乐、显示基本信息”）

- [x] 设置页添加/移除扫描目录，触发递归扫描，进度事件实时更新
- [x] 曲库列表虚拟滚动、按列排序、FTS5 关键词搜索
- [x] 双击曲目以当前列表为队列播放
- [x] 播放/暂停、停止、seek、音量、静音
- [x] 上/下一首 + 四种播放模式（顺序/列表循环/单曲循环/随机），end-file 自动切歌与队尾停止
- [x] 曲目基本信息展示（标题/艺术家/专辑/时长/格式徽标：无损位深采样率 / 有损码率）
- [x] 暗/亮/跟随系统主题，重启保持
- [x] 自定义标题栏（拖拽 + 最小化/最大化/关闭）
- [x] 单实例：二次启动聚焦已有窗口
- [x] 命令行/文件参数打开：库外文件入库后播放
- [x] 全局快捷键：Space / Ctrl+←→ / Ctrl+↑↓ / Ctrl+M

**质量门禁**

- [x] `cargo test`：46 项通过
- [x] `pnpm build`（tsc + vite）：通过
- [x] `pnpm tauri build`：MSI + NSIS 构建成功，NSIS 32.3MB（≤40MB 目标）
- [x] IPC 契约文档与实现一致（`docs/ipc-contract.md`）

**已知限制 / 顺延项**

- 引擎跨平台播放仅 Windows 验证（macOS/Linux 顺延）
- 扫描目录选择暂用文本输入路径（原生目录对话框待接入 tauri-plugin-dialog）
- 安装后端到端冒烟（真实音频、双击关联文件）需人工在桌面环境执行
- 队列页 / 统计页为占位（顺延至 Phase 2/3）

---

### Phase 2：音乐体验完善（2-3 周）— ✅ 已完成

> **里程碑**：歌词、封面、队列、CUE，音乐模式基本完整。

| 任务 | 优先级 | 预计 | 状态 |
|------|--------|------|------|
| 歌词：LRC 解析 + 同步滚动显示 | P0 | 3d | ✅ |
| 歌词：同目录 .lrc 文件自动加载（含内嵌标签回退、GBK 探测） | P0 | 1d | ✅ |
| 封面：内嵌封面读取 + 显示 | P0 | 2d | ✅ |
| 封面：本地 cover.jpg 检测 | P1 | 1d | ✅ |
| 主题色：封面主色提取 → UI 强调色动态变化 | P1 | 2d | ✅ |
| 播放队列：插入/移除/拖拽排序/清空 | P0 | 2d | ✅ |
| 播放模式：顺序/随机/单曲循环/列表循环 | P0 | 1d | ✅ Phase 1 已交付 |
| CUE 整轨解析 + 虚拟曲目创建 | P0 | 3d | ✅ |
| 搜索功能：标题/艺术家/专辑模糊搜索 | P1 | 2d | ✅ Phase 1 已交付（FTS5） |
| 专辑页面 + 艺术家页面 | P1 | 3d | ✅ |
| 播放列表：创建/编辑/删除 | P1 | 2d | ✅ |

**交付物**：音乐模式体验完整，歌词同步，封面显示，CUE 支持。

---

### Phase 3：音频高级功能（2-3 周）

> **里程碑**：HiFi 输出，gapless，系统媒体控制，频谱，均衡器。

| 任务 | 优先级 | 预计 | 状态 |
|------|--------|------|------|
| WASAPI 共享/独占输出切换（Windows） | P0 | 2d | ✅ |
| 输出设备选择 | P1 | 1d | ✅ 模型预留，UI 待设备枚举 |
| 无缝播放（--gapless-audio）+ ReplayGain 应用 | P1 | 2d | ✅ |
| 系统媒体控制：SMTC/MPRIS/Now Playing + 媒体键（souvlaki） | P0 | 2d | ✅ Win 编译验证 |
| 实时频谱可视化（Rust 侧 FFT → 事件推送 → Canvas） | P2 | 3d | |
| 均衡器：预设 + 手动频段调节 | P2 | 3d | |
| 播放统计：播放次数、最近播放、常听排行 | P1 | 2d | ✅ |
| 系统托盘：最小化到托盘 + 托盘菜单 | P1 | 1d | ✅ |
| 播放状态缓存（关闭后恢复上次状态） | P1 | 1d | ✅ |

**交付物**：音频能力达到 HiFi 播放器水准。

> 进展：**系统媒体控制已落地**（`commands/media_controls.rs`，souvlaki）——切歌/暂停/停止时向系统浮窗同步标题/艺术家/专辑/封面/进度，媒体键与浮窗按钮（播放/暂停/上下一首/seek）回调到 PlayerCore。仅 Windows 完成编译验证，实际浮窗/媒体键行为需桌面环境人工实测；macOS/Linux 保留。

---

### Phase 4：视频模式（2-3 周）

> **里程碑**：视频播放、字幕、画中画。

| 任务 | 优先级 | 预计 |
|------|--------|------|
| 视频渲染区域嵌入（mpv --wid） | P0 | 3d |
| 视频播放页 UI | P0 | 2d |
| 文件浏览器（目录树 + 视频过滤） | P0 | 2d |
| 字幕：自动加载 + 手动加载 + 多轨切换 | P0 | 2d |
| 字幕：样式调整 + 时间偏移 | P1 | 1d |
| 音轨切换 | P1 | 1d |
| 全屏切换（F11 + 双击） | P0 | 1d |
| 画中画模式 | P2 | 2d |
| 画面比例 / 旋转 / 色彩调节 | P1 | 2d |
| 截图功能 | P1 | 1d |
| 最近播放 + 续播 | P1 | 1d |
| 文件关联"打开方式"处理（单实例路径转发 → 按类型进入音乐/视频模式） | P0 | 2d |

**交付物**：视频模式完整可用，字幕支持良好。

---

### Phase 5：在线功能 + 打磨（2 周）

> **里程碑**：可选在线功能，设置完善，跨平台打包发布。

| 任务 | 优先级 | 预计 |
|------|--------|------|
| 在线歌词搜索（lrclib.net API） | P1 | 2d |
| 在线封面补全（MusicBrainz + Cover Art Archive） | P2 | 2d |
| 设置页面：全部设置项 | P1 | 2d |
| 快捷键自定义 | P2 | 2d |
| 多语言支持（中/英） | P2 | 2d |
| 跨平台打包：Windows .msi / macOS .dmg / Linux .AppImage（含文件关联注册） | P0 | 3d |
| README / 用户文档 | P1 | 1d |
| Bug 修复 + 性能优化 | P0 | 持续 |

**交付物**：v1.0 发布，功能完整，三平台可安装。

---

## 七、关键技术难点与对策

| 难点 | 风险 | 对策 |
|------|------|------|
| **libmpv 跨平台分发** | 各平台 mpv 库文件不同，分发方式不同 | Win：随包分发 DLL；Mac：编译为 .dylib 或用 Homebrew；Linux：依赖系统包 + 安装指引 |
| **mpv 视频嵌入 Webview** | Tauri webview 与原生窗口坐标对齐 | 使用 mpv `--wid` 参数 + Tauri v2 获取 native window handle；如遇兼容问题退而使用独立视频窗口 |
| **CUE 整轨播放** | 时间精度、索引边界、多 FILE 支持 | 解析 INDEX 00/01，用 mpv seek 精确定位；多 FILE 场景单独处理 |
| **主题色提取性能** | 大封面图取色慢 | 缩放到 64x64 后取色，结果缓存到 SQLite，仅首次计算 |
| **频谱可视化** | mpv 在原生层出声，WebView 的 Web Audio API 拿不到 PCM；PCM 全量走 IPC 开销过大 | **确定方案**：Rust 侧 mpv audio callback 取 PCM → rustfft 做 FFT → 每帧仅 emit 32~64 个频段值（约 30fps）给前端 Canvas 绘制；实现复杂度高，优先级降为 P2，v1.0 可延期 |
| **大媒体库性能** | 10万+ 曲目列表卡顿 | 前端虚拟滚动（react-virtual）；后端分页查询 + 索引优化 |
| **DSD 输出** | DSD over PCM 需要声卡支持 | mpv 原生支持 DoP，自动检测声卡能力，降级为 PCM 转码 |
| **WASAPI 独占兼容性** | 部分声卡驱动不支持独占 | 检测失败时自动回退共享模式并提示用户 |

---

## 八、在线功能设计（可选模块）

### 设计原则

- **完全可选**：设置中一键开关，关闭后零网络请求
- **无需登录**：不绑定任何在线音乐平台账号
- **隐私优先**：仅发送歌曲标题+艺术家用于搜索，不上传文件内容
- **本地优先**：在线获取的数据缓存到本地，下次直接使用

### 8.1 歌词搜索

**数据源**：lrclib.net（开源歌词数据库，无需 API Key）

```
请求：GET https://lrclib.net/api/get?artist_name={artist}&track_name={title}&album_name={album}
响应：{ syncedLyrics: "[00:12.34]歌词...", plainLyrics: "歌词..." }
```

**流程**：
1. 用户点击"搜索歌词" 或 开启自动搜索
2. 后端发送请求（title + artist + album）
3. 匹配结果返回，优先选择 synced (LRC) 版本
4. 用户确认后保存为 .lrc 文件 + 更新数据库

### 8.2 封面补全

**数据源**：MusicBrainz API（免费，无需 Key）+ Cover Art Archive

```
步骤1：搜索 MusicBrainz → 获取 release ID
GET https://musicbrainz.org/ws/2/release/?query=artist:{artist}+release:{album}&fmt=json

步骤2：获取封面
GET https://coverartarchive.org/release/{mbid}/front-500
```

**流程**：
1. 检测到曲目无封面
2. 按 artist + album 搜索 MusicBrainz
3. 下载封面缩略图（500px）
4. 保存到本地缓存目录
5. 更新数据库

---

## 九、包体预估

| 组件 | Windows | macOS | Linux |
|------|---------|-------|-------|
| Tauri 框架 + WebView | ~2MB | ~2MB (系统 WebView) | ~2MB |
| libmpv 运行时 | ~30MB | ~25MB | 系统依赖 (~0MB) |
| 应用代码 (Rust + JS) | ~3MB | ~3MB | ~3MB |
| 图标/资源 | ~1MB | ~1MB | ~1MB |
| **合计** | **~36MB** | **~31MB** | **~6MB** |

对比：
- VLC：~120MB
- PotPlayer：~30MB（仅 Windows）
- mpv 原版：~40MB
- Electron 类播放器：~200MB+

---

## 十、从 Aria 借鉴的设计点

| 借鉴点 | 说明 | 如何应用 |
|--------|------|----------|
| 封面主题色动态化 | 播放页根据封面颜色调整 UI 主题 | image crate 缩放取色 → CSS 变量动态更新 |
| WASAPI 输出模式 | 支持系统/共享/独占三种模式 | mpv 配置 `--audio-exclusive=yes` + 前端切换 UI |
| 音质信息展示 | 显示格式、码率、采样率、位深、无损标识 | lofty 读取 → Track 模型 → 前端 Badge 展示 |
| 桌面体验 | 自定义标题栏、托盘后台、状态缓存 | Tauri 自定义窗口 + tray-icon 插件 |
| CUE 整轨支持 | 整轨文件分轨播放 | Rust CUE 解析器 + mpv seek |
| 统计功能 | 常听歌曲、播放记录 | SQLite 记录播放事件 → 前端统计页 |

**不借鉴**：
- 网易云登录同步（不做在线平台集成）
- 扫码登录（不涉及账号体系）
- 流媒体播放（纯本地播放器，在线仅限歌词/封面）

---

## 十一、后续扩展方向（v2.0+）

以下不在 v1.0 范围内，但架构预留扩展空间：

- **移动端**：Tauri v2 支持 iOS/Android，未来可扩展
- **DLNA/投屏**：发现局域网设备，投屏播放
- **播客/RSS**：订阅播客源，在线播放
- **音乐标签编辑**：手动编辑 ID3/Vorbis 标签
- **音频格式转换**：基于 mpv/ffmpeg 的转码功能
- **插件系统**：开放插件接口，社区扩展
- **同步功能**：跨设备播放列表同步（WebDAV/自建服务）
