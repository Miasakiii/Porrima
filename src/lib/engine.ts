import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import {
  command as mpvCommand,
  destroy,
  getProperty,
  init as mpvInit,
  listenEvents,
  observeProperties,
  setProperty,
  type MpvObservableProperty,
} from "tauri-plugin-libmpv-api";
import { AppEvents } from "@/lib/events";
import { usePlayerStore } from "@/stores/playerStore";
import type { EngineCmdPayload } from "@/lib/types";

/**
 * 引擎适配器（Phase 0 最终决策，docs/phase0-findings.md）：
 * mpv 控制面只在前端，本模块负责——
 * 1. 插件 init（audio-only：video:'no'）与 init 失败的显性上报；
 * 2. 订阅 mpv 属性/事件，经 invoke('engine_event') 原样转发给 Rust；
 * 3. 订阅 Rust 的 `engine:cmd` 事件并执行对应插件调用。
 *
 * 状态机在 Rust（PlayerCore），本模块不做任何播放决策。
 */

const OBSERVED_PROPERTIES = [
  ["pause", "flag"],
  ["time-pos", "double", "none"],
  ["duration", "double", "none"],
  ["eof-reached", "flag", "none"],
] as const satisfies MpvObservableProperty[];

/** 模块级守卫：React StrictMode 双调用 / 重复挂载只初始化一次。 */
let initPromise: Promise<boolean> | null = null;

/** 初始化引擎适配器。返回是否成功（失败已 toast 上报，不抛出）。 */
export function initEngine(): Promise<boolean> {
  initPromise ??= doInit();
  return initPromise;
}

async function doInit(): Promise<boolean> {
  try {
    // 先挂 engine:cmd 监听，避免 init 后的后端指令窗口期丢失
    await listen<EngineCmdPayload>(AppEvents.EngineCmd, (e) => {
      void execute(e.payload);
    });

    // StrictMode/热重载容错：清掉可能残留的实例再 init（无实例时报错忽略）
    try {
      await destroy();
    } catch {
      // ignore
    }

    await mpvInit({
      initialOptions: {
        // 视频输出：嵌入 Tauri 窗口原生表面（--wid 由插件自动设置）
        vo: "gpu-next",
        hwdec: "auto-safe",
        "keep-open": "yes",
        // 音频文件无视频流时不创建窗口，仅音频输出
        "force-window": "no",
      },
      observedProperties: OBSERVED_PROPERTIES,
    });

    await observeProperties(OBSERVED_PROPERTIES, ({ name, data }) => {
      switch (name) {
        case "pause":
          if (typeof data === "boolean") forward("pause", data);
          break;
        case "time-pos":
          if (typeof data === "number") forward("time-pos", data);
          break;
        case "duration":
          if (typeof data === "number") forward("duration", data);
          break;
        case "eof-reached":
          // keep-open=yes：自然播完不发 end-file 事件，以 eof-reached=true 为准
          if (data === true) forward("end-file");
          break;
      }
    });

    await listenEvents((event) => {
      if (event.event === "file-loaded") {
        // file-loaded 时 duration 通常已可读，取到一并转发（拿不到给 0，
        // 后续 duration property-change 会校准）
        void getProperty("duration", "double")
          .then((d) => forward("file-loaded", typeof d === "number" ? d : 0))
          .catch(() => forward("file-loaded", 0));
      }
    });

    // 通知后端适配器就绪：同步音量、处理启动挂起的命令行文件
    await invoke("engine_ready");
    return true;
  } catch (err) {
    // Phase 0 教训（issue #2）：init 失败必须显性上报，不能只 console.error
    console.error("[engine] mpv 初始化失败:", err);
    // 引擎不可用 → 播放能力不可用，标记状态供 UI 降级（覆盖 playerStore.init 可能的 true）
    usePlayerStore.setState({ backendReady: false });
    toast.error("播放引擎初始化失败", {
      description: `无法加载 libmpv，播放功能不可用。${String(err)}`,
      duration: Infinity,
    });
    return false;
  }
}

/** mpv 事件 → Rust（时间值保持 mpv 原生单位：秒）。 */
function forward(event: string, value?: number | boolean): void {
  void invoke("engine_event", { event, value }).catch((err) => {
    console.warn(`[engine] 转发 ${event} 失败:`, err);
  });
}

/** 执行后端下发的引擎指令。 */
async function execute(cmd: EngineCmdPayload): Promise<void> {
  try {
    switch (cmd.kind) {
      case "load":
        // CUE 虚拟曲目：用 mpv start/end 选项限定播放区间（下次 loadfile 生效）；
        // 普通曲目显式复位为 none，避免残留上一轨的区间
        await setProperty(
          "start",
          cmd.startMs != null ? String(cmd.startMs / 1000) : "none",
        );
        await setProperty(
          "end",
          cmd.endMs != null ? String(cmd.endMs / 1000) : "none",
        );
        await mpvCommand("loadfile", [cmd.path]);
        await setProperty("pause", false);
        break;
      case "pause":
        await setProperty("pause", true);
        break;
      case "resume":
        await setProperty("pause", false);
        break;
      case "stop":
        await mpvCommand("stop");
        break;
      case "seek":
        await mpvCommand("seek", [cmd.positionMs / 1000, "absolute"]);
        break;
      case "setVolume":
        await setProperty("volume", cmd.volume);
        break;
      case "setAudioOptions":
        await applyAudioOptions(cmd);
        break;
    }
  } catch (err) {
    console.warn("[engine] 指令执行失败:", cmd, err);
    if (cmd.kind === "load") {
      toast.error("播放失败", { description: String(err) });
    }
  }
}

/** 应用音频输出选项到 mpv 属性（运行时生效，无需重启引擎）。 */
async function applyAudioOptions(cmd: {
  exclusive: boolean;
  device: string;
  gapless: boolean;
  replayGain: string;
  loudnormFallback: boolean;
}): Promise<void> {
  // WASAPI 独占模式
  await setProperty("audio-exclusive", cmd.exclusive ? "yes" : "no");
  // 输出设备（"auto" 为系统默认）
  await setProperty("audio-device", cmd.device);
  // 无缝播放
  await setProperty("gapless-audio", cmd.gapless ? "yes" : "no");
  // ReplayGain
  await setProperty("replaygain", cmd.replayGain);
  // loudnorm 状态记录，重建 af 时统一处理
  loudnormActive = cmd.loudnormFallback;
  await rebuildAudioFilter();
}

// ---------- 均衡器（10 段 + loudnorm 统一管理 af） ----------

/** 均衡器频段 (Hz)。 */
export const EQ_BANDS = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000] as const;

/** 均衡器预设。 */
export const EQ_PRESETS: Record<string, number[]> = {
  "平坦": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  "流行": [-1, -1, 0, 2, 4, 4, 2, 0, -1, -1],
  "摇滚": [4, 3, 1, 0, -1, 0, 1, 2, 3, 4],
  "古典": [3, 2, 1, 0, 0, 0, 0, 1, 2, 3],
  "爵士": [3, 2, 1, 2, -1, -1, 0, 1, 2, 3],
  "低音增强": [6, 5, 4, 2, 0, 0, 0, 0, 0, 0],
};

/** 模块级 EQ 状态（各频段增益 dB）。 */
let eqGains: number[] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
let loudnormActive = false;

/** 重建 mpv af 滤镜链（EQ + 可选 loudnorm）。 */
async function rebuildAudioFilter(): Promise<void> {
  const parts: string[] = [];

  // EQ 段：仅非零增益的频段加入滤镜链
  const hasEQ = eqGains.some((g) => g !== 0);
  if (hasEQ) {
    for (let i = 0; i < EQ_BANDS.length; i++) {
      if (eqGains[i] !== 0) {
        const width = EQ_BANDS[i] * 1.5; // 带宽约 1.5 倍中心频率
        parts.push(
          `equalizer=f=${EQ_BANDS[i]}:width_type=h:width=${width}:g=${eqGains[i]}`,
        );
      }
    }
  }

  // loudnorm 响度归一化
  if (loudnormActive) {
    parts.push("lavfi=[loudnorm=I=-16:TP=-1.5:LRA=11]");
  }

  await setProperty("af", parts.join(","));
}

/** 设置均衡器各频段增益（dB，-12 ~ +12）。 */
export async function setEqualizer(gains: number[]): Promise<void> {
  eqGains = gains.slice(0, EQ_BANDS.length);
  await rebuildAudioFilter();
}

/** 获取当前 EQ 增益。 */
export function getEqualizerGains(): number[] {
  return [...eqGains];
}

/** 重置均衡器。 */
export async function resetEqualizer(): Promise<void> {
  eqGains = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
  await rebuildAudioFilter();
}

// ---------- 字幕控制（Phase 4，前端直接操作 mpv） ----------

/** 字幕轨道信息。 */
export interface SubtitleTrack {
  id: number;
  title: string;
  lang: string;
  external: boolean;
}

/** 获取当前文件的字幕轨道列表。 */
export async function getSubtitleTracks(): Promise<SubtitleTrack[]> {
  try {
    const trackList = await getProperty("track-list", "node") as Record<string, unknown>[] | null;
    if (!Array.isArray(trackList)) return [];
    return trackList
      .filter((t) => t.type === "sub")
      .map((t) => ({
        id: (t.id as number) ?? 0,
        title: (t.title as string) || (t.external ? "外挂字幕" : `轨道 ${(t.id as number) + 1}`),
        lang: (t.lang as string) || "und",
        external: (t.external as boolean) ?? false,
      }));
  } catch {
    return [];
  }
}

/** 切换字幕轨道（id=0 为禁用字幕，实际轨道 id 从 1 开始）。 */
export async function setSubtitleTrack(sid: number): Promise<void> {
  await setProperty("sid", sid);
}

/** 设置字幕延迟（秒，正值为延迟，负值为提前）。 */
export async function setSubtitleDelay(seconds: number): Promise<void> {
  await setProperty("sub-delay", seconds);
}

/** 获取当前字幕延迟。 */
export async function getSubtitleDelay(): Promise<number> {
  try {
    const v = await getProperty("sub-delay", "double");
    return typeof v === "number" ? v : 0;
  } catch {
    return 0;
  }
}

/** 加载外挂字幕文件。 */
export async function loadSubtitleFile(path: string): Promise<void> {
  await mpvCommand("sub-add", [path]);
}

// ---------- 音轨控制 ----------

/** 音轨信息。 */
export interface AudioTrack {
  id: number;
  title: string;
  lang: string;
}

/** 获取当前文件的音轨列表。 */
export async function getAudioTracks(): Promise<AudioTrack[]> {
  try {
    const trackList = await getProperty("track-list", "node") as Record<string, unknown>[] | null;
    if (!Array.isArray(trackList)) return [];
    return trackList
      .filter((t) => t.type === "audio")
      .map((t) => ({
        id: (t.id as number) ?? 0,
        title: (t.title as string) || `音轨 ${(t.id as number) + 1}`,
        lang: (t.lang as string) || "und",
      }));
  } catch {
    return [];
  }
}

/** 切换音轨。 */
export async function setAudioTrack(aid: number): Promise<void> {
  await setProperty("aid", aid);
}

/** 截取当前帧保存为 PNG（screenshot-to-file → 图片/Porrima Screenshots）。 */
export async function takeScreenshot(): Promise<void> {
  try {
    const dir = await invoke<string>("get_screenshot_dir");
    const ts = new Date()
      .toISOString()
      .replace(/[:.]/g, "-")
      .replace("T", "_")
      .slice(0, 19);
    const filename = `Porrima_${ts}.png`;
    // Windows 路径分隔符
    const filepath = `${dir}\\${filename}`;
    await mpvCommand("screenshot-to-file", [filepath, "video"]);
    toast.success("截图已保存", { description: filename });
  } catch (err) {
    console.warn("[screenshot] 截图失败:", err);
    toast.error("截图失败", { description: String(err) });
  }
}

// ---------- 画面调节 ----------

/** 画面比例选项。 */
export const ASPECT_RATIOS = [
  { value: "", label: "自适应" },
  { value: "16:9", label: "16:9" },
  { value: "4:3", label: "4:3" },
  { value: "2.35:1", label: "2.35:1" },
  { value: "1:1", label: "1:1" },
] as const;

/** 设置画面比例。 */
export async function setAspectRatio(ratio: string): Promise<void> {
  await setProperty("video-aspect-override", ratio || "-1");
}

/** 设置视频旋转（0/90/180/270）。 */
export async function setVideoRotation(degrees: number): Promise<void> {
  await setProperty("video-rotate", degrees);
}

/** 设置亮度 (-100 ~ 100)。 */
export async function setBrightness(v: number): Promise<void> {
  await setProperty("brightness", v);
}

/** 设置对比度 (-100 ~ 100)。 */
export async function setContrast(v: number): Promise<void> {
  await setProperty("contrast", v);
}

/** 设置饱和度 (-100 ~ 100)。 */
export async function setSaturation(v: number): Promise<void> {
  await setProperty("saturation", v);
}

/** 设置色调 (-100 ~ 100)。 */
export async function setHue(v: number): Promise<void> {
  await setProperty("hue", v);
}

/** 设置伽马 (-100 ~ 100)。 */
export async function setGamma(v: number): Promise<void> {
  await setProperty("gamma", v);
}

/** 重置所有画面调节。 */
export async function resetVideoAdjustments(): Promise<void> {
  await setProperty("brightness", 0);
  await setProperty("contrast", 0);
  await setProperty("saturation", 0);
  await setProperty("hue", 0);
  await setProperty("gamma", 0);
  await setProperty("video-rotate", 0);
  await setProperty("video-aspect-override", "-1");
}

// ---------- 字幕样式 ----------

/** 设置字幕字号。 */
export async function setSubFontSize(size: number): Promise<void> {
  await setProperty("sub-font-size", size);
}

/** 设置字幕颜色（十六进制，如 "#FFFFFF"）。 */
export async function setSubColor(hex: string): Promise<void> {
  // mpv 使用 BBGGRR 格式
  const r = hex.slice(1, 3);
  const g = hex.slice(3, 5);
  const b = hex.slice(5, 7);
  await setProperty("sub-color", `#${b}${g}${r}`);
}

/** 设置字幕底部边距（像素）。 */
export async function setSubMarginY(px: number): Promise<void> {
  await setProperty("sub-margin-y", px);
}
