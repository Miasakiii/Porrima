# Phase 0 技术验证结论

> 日期：2026-07-28。验证环境：Windows，Rust 1.95 / Node 24 / pnpm 11。
> 完整排雷过程见会话记录；spike 产物保留在 `.spikes/`。

## 结论一览

| 验证项 | 结果 | 说明 |
|---|---|---|
| 0.1 官方 example 跑通 | ✅ 通过 | commit `5da4e04`（无 tag，package.json 0.3.2），播放/暂停/seek/事件全通 |
| 0.2 集成可行性 | ✅ 通过 | issue #2 未复现，根因已查明（init 失败被次生报错掩盖，非 bug） |
| 0.3 视频 wid 嵌入 | ⏸ 推迟 | Phase 1 音频场景设 `video:'no'` 跳过嵌入；wid 冒烟并入 Phase 4 探路 |
| 0.4 NSIS 包体实测 | ✅ 通过 | Phase 1 收尾实测：NSIS `Porrima_0.1.0_x64-setup.exe` = **32.3MB**（≤40MB 目标达成）；MSI 43.6MB（压缩弱，非主分发目标）。壳体 exe 13.6MB，随包 libmpv-2.dll 93.6MB 经 LZMA 压缩后落在预期区间 |
| 0.5 事件机制 | ✅ 已明确 | 插件 `emit_to(label, "mpv-event-<label>", json)`，time-pos ~15-17Hz |

**引擎方案最终决定：采用 tauri-plugin-libmpv（锁 0.3.2 / commit 5da4e04）。** 音频链路 Windows 完整验证通过；Rust 侧仍保留 PlayerService 状态机抽象，引擎细节隔离在适配层后。

## 关键事实

- **DLL**：`libmpv-wrapper.dll`（0.37MB，MPL/LGPL）+ `libmpv-2.dll`（93.6MB，zhongfly mpv-dev-lgpl 构建，LGPL）。`npx tauri-plugin-libmpv-api setup-lib` 在项目根执行，落盘 `src-tauri/lib/`。
- **dev 链路**：`tauri.conf.json` 必须配 `bundle.resources: ["lib/**/*"]`，tauri-build 构建时拷到 `target/debug/lib/`，插件从 `exe_dir/lib` 加载；`src-tauri/lib/` 必须先有文件再首次 cargo build。
- **事件**：payload `{"event":"property-change","name":"time-pos","data":N,...}` 等；time-pos ~15Hz **必须节流**（Rust 侧采样到 ≤4Hz 再推 Channel）；暂停期间无事件；init 后立即推一轮全量初值。
- **init 失败必须显性上报**（toast/状态栏），不能仅 console.error——issue #2 的教训。
- **本机网络**：github.com 直连被重置；setup-lib 需 `NODE_USE_ENV_PROXY=1 HTTPS_PROXY=http://127.0.0.1:1088`。
- **React StrictMode**：destroy→init 插件可容错，loadfile 等动作需一次性守卫。

## 集成步骤（已验证顺序）

1. `pnpm add tauri-plugin-libmpv-api`；Cargo 加 `tauri-plugin-libmpv = "=0.3.2"`
2. `NODE_USE_ENV_PROXY=1 HTTPS_PROXY=http://127.0.0.1:1088 npx tauri-plugin-libmpv-api setup-lib`（项目根执行）
3. `tauri.conf.json`：`bundle.resources` 加 `lib/**/*`
4. capabilities 加 `libmpv:default`
5. 前端 init：`initialOptions: { video: 'no', 'keep-open': 'yes' }`，`observedProperties: ['time-pos','duration','pause','volume','eof-reached']`

## 对架构的调整（重要）

插件的控制面只暴露 JS API（command/init），事件经 `emit_to` 推向 webview。Rust 侧 `listen_any` 对 `emit_to` 定向事件的可见性没有可靠保证，**最终决策：事件走「前端适配器 invoke 转发」**，不依赖 listen_any 语义。因此：

- **前端 engine 适配器**（W3/集成环节实现）：负责插件 init（`video:'no'`）与 init 失败的显性上报；订阅插件 `mpv-event-main` 事件，把相关事件（time-pos/duration/pause/end-file/file-loaded 等）经 `invoke('engine_event', ...)` 原样转发给 Rust；订阅 Rust 发出的 `engine:cmd` 事件并执行对应插件调用（load/pause/resume/seek/volume/stop）。
- **Rust PlayerService** 拥有队列/播放模式/状态机（可单测）：接收 `engine_event` 更新内部状态（time-pos 节流至 ≤4Hz），通过 `watch_player` Channel 推送 progress/state，end-file 触发状态机选下一首后发 `engine:cmd`——契约 `docs/ipc-contract.md` 完全不变，前端 UI（W4）无需任何调整。
- 包体预算：DLL 94MB 经 NSIS LZMA 压缩预期 ~26-30MB，加上壳体 ≤40MB 目标仍然成立，收尾实测确认。

## Phase 1 收尾补充（2026-07-29）

- **播放链路打通**：后端 `commands/player.rs` 实现全部契约播放命令 + `watch_player` Channel + 内部 `engine_event`/`engine_ready`/`engine:cmd` 适配接口；前端 `src/lib/engine.ts` 适配器（`video:'no'` 音频链路，init 失败 toast 显性上报）。契约 `docs/ipc-contract.md` 新增「引擎适配（内部接口）」小节。
- **eof 处理**：`keep-open=yes` 下自然播完不发 `end-file`，改以 `eof-reached=true` property-change 触发状态机推进（`src/lib/engine.ts`）。
- **NSIS 实测**：安装包 32.3MB，≤40MB 目标达成（见上表 0.4）。
- **单实例 + 命令行打开**：`tauri-plugin-single-instance` 最先注册；命令行/二次实例文件参数经 `library::import_files` 入库后播放（库外文件先提取标签入库）。
- **TitleBar**：`decorations:false` + 自定义标题栏挂载，capabilities 补 `core:window` 最小/最大化/关闭/拖拽等权限。
- **验证**：`cargo test` 46 passed；`pnpm build`（tsc + vite）通过；release bundle（MSI + NSIS）构建成功。
