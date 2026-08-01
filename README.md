# Porrima

> 一个现代 UI、轻量安装、音乐/视频双模式分离的跨平台桌面播放器。

Porrima 基于 **Tauri v2 + libmpv**，用 mpv 的全格式解码能力做出精致的桌面 UI，目标安装包约 37MB（约为 VLC 的 1/3）。

**当前状态：Phase 1（音乐 MVP）已完成** —— 可扫描本地目录、以 libmpv 播放音乐、完整的曲库列表与播放控制。后续阶段（歌词/封面/队列、HiFi 输出、视频模式、在线功能）见 [PROJECT.md](./PROJECT.md) 路线图。

---

## 已实现功能（Phase 1）

- **媒体库**：递归扫描指定目录，SQLite + FTS5 全文模糊搜索，按标题/艺术家/专辑/时长/添加时间/播放次数排序，曲目列表虚拟滚动（10 万+ 曲目不卡顿）。
- **播放控制**：libmpv 引擎，播放/暂停/停止、进度 seek、音量/静音、上一首/下一首、四种播放模式（顺序 / 列表循环 / 单曲循环 / 随机）。
- **界面**：自定义标题栏、侧边栏导航、常驻底部 PlayerBar、暗/亮/跟随系统主题。
- **单实例 + 文件打开**：二次启动聚焦已有窗口；命令行参数 / 文件关联传入的音频文件自动入库后播放（库外文件先提取标签入库）。
- **全局快捷键**：`Space` 播放/暂停，`Ctrl+←/→` 上/下一首，`Ctrl+↑/↓` 音量 ±5，`Ctrl+M` 静音。

> 扫描识别的音频扩展名：`flac` `mp3` `m4a` `aac` `ogg` `opus` `wav` `aiff` `ape` `wv` `wma` `dsf` `dff`。实际播放由 mpv 处理，格式覆盖更广。

## 技术栈

| 层 | 选型 |
|----|------|
| 应用框架 | Tauri v2（Rust 后端 + WebView 前端） |
| 播放引擎 | libmpv，经 [`tauri-plugin-libmpv`](https://github.com/nini22P/tauri-plugin-libmpv)（锁定 0.3.2） |
| 前端 | React 19 · TypeScript · Vite 7 · Tailwind CSS v4 · shadcn/ui · Zustand |
| 元数据 | lofty（标签/封面）· walkdir（目录遍历） |
| 存储 | rusqlite（bundled SQLite）+ FTS5 trigram |

架构与决策依据详见 [PROJECT.md](./PROJECT.md)；前后端 IPC 契约见 [docs/ipc-contract.md](./docs/ipc-contract.md)；引擎选型验证结论见 [docs/phase0-findings.md](./docs/phase0-findings.md)。

## 环境要求

- **Rust** stable（含 `cargo`）
- **Node.js** ≥ 20 与 **pnpm**
- 平台：Phase 1 已在 **Windows** 上完整验证；macOS / Linux 目标保留，尚未验证。
- libmpv 运行库（`libmpv-2.dll` / `libmpv-wrapper.dll`）已随仓库置于 `src-tauri/lib/`，无需额外下载。

## 快速开始

```bash
# 1. 安装前端依赖
pnpm install

# 2. 启动开发（Tauri 会自动拉起 Vite dev server）
pnpm tauri dev
```

首次运行：进入 **设置 → 扫描目录**，输入音乐目录的绝对路径（如 `D:\Music`）并添加 → 触发扫描 → 回到曲库，双击任意曲目开始播放。

> libmpv 运行库若需重新拉取或更新，可执行 `npx tauri-plugin-libmpv-api setup-lib`（在项目根运行，落盘到 `src-tauri/lib/`）。该命令从 GitHub 下载 DLL；网络受限时需配置代理（见 [docs/phase0-findings.md](./docs/phase0-findings.md)）。

## 构建

```bash
pnpm tauri build
```

产物位于 `src-tauri/target/release/bundle/`。Windows 下生成 MSI 与 NSIS 安装包；**NSIS 安装包实测约 32MB**（≤40MB 目标达成，含经 LZMA 压缩的 libmpv 运行库）。

## 目录结构

```
Porrima/
├── src/                  # React 前端（组件 / 页面 / stores / lib）
│   └── lib/engine.ts     # libmpv 引擎适配器（前端控制面）
├── src-tauri/            # Rust 后端
│   ├── src/commands/     # Tauri IPC 命令（player / library / settings）
│   ├── src/services/     # 业务逻辑（player 状态机 / library 扫描 / metadata）
│   ├── src/db/           # rusqlite + FTS5 存储与迁移
│   └── lib/              # 随包分发的 libmpv DLL
└── docs/                 # IPC 契约、技术验证结论
```

## 开发脚本

| 命令 | 说明 |
|------|------|
| `pnpm tauri dev` | 开发模式运行桌面应用 |
| `pnpm tauri build` | 打包生产安装包 |
| `pnpm build` | 仅前端类型检查（tsc）+ 构建产物 |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 运行 Rust 单元测试 |

## 许可

libmpv 运行库（`libmpv-2.dll` 等）以 LGPL 许可随包分发。项目自身许可待定。
