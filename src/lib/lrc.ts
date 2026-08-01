/**
 * LRC 歌词解析（docs/ipc-contract.md：后端返回原始文本，时间轴解析在前端）。
 *
 * 支持：
 * - 单行多时间标签 `[00:12.34][01:05.67]副歌`
 * - 时间格式 `[mm:ss]` / `[mm:ss.x]` ~ `[mm:ss.xxx]`
 * - 全局偏移标签 `[offset:±ms]`（正值提前，LRC 惯例）
 * - 无任何时间标签时降级为纯文本（synced=false）
 */

export interface LrcLine {
  /** 行起始时间；纯文本歌词恒为 0 */
  timeMs: number;
  text: string;
}

export interface ParsedLyrics {
  /** 是否含时间轴（决定 UI 用同步滚动还是静态居中） */
  synced: boolean;
  /** synced 时按 timeMs 升序 */
  lines: LrcLine[];
}

/** `[mm:ss]` 或 `[mm:ss.frac]`，分钟可超两位（长音频） */
const TIME_TAG = /\[(\d{1,3}):(\d{1,2})(?:\.(\d{1,3}))?\]/g;
const OFFSET_TAG = /^\[offset:\s*([+-]?\d+)\s*\]$/i;
/** 元信息标签（ti/ar/al/by/re/ve 等）：`[xx:yy]` 中 xx 非纯数字 */
const META_TAG = /^\[[a-z@#]+:.*\]$/i;

export function parseLrc(raw: string): ParsedLyrics {
  let offsetMs = 0;
  const synced: LrcLine[] = [];
  const plain: string[] = [];

  for (const rawLine of raw.split(/\r\n|\n|\r/)) {
    const line = rawLine.trim();
    if (!line) continue;

    const offsetMatch = OFFSET_TAG.exec(line);
    if (offsetMatch) {
      offsetMs = Number.parseInt(offsetMatch[1], 10);
      continue;
    }
    if (META_TAG.test(line)) continue;

    TIME_TAG.lastIndex = 0;
    const times: number[] = [];
    let match: RegExpExecArray | null;
    let lastEnd = 0;
    // 只解析行首连续的时间标签
    while ((match = TIME_TAG.exec(line)) !== null && match.index === lastEnd) {
      const minutes = Number.parseInt(match[1], 10);
      const seconds = Number.parseInt(match[2], 10);
      // ".5"→500ms、".55"→550ms、".555"→555ms
      const frac = match[3] ? Number.parseInt(match[3].padEnd(3, "0"), 10) : 0;
      times.push((minutes * 60 + seconds) * 1000 + frac);
      lastEnd = TIME_TAG.lastIndex;
    }

    if (times.length === 0) {
      plain.push(line);
      continue;
    }
    const text = line.slice(lastEnd).trim();
    for (const t of times) {
      synced.push({ timeMs: t, text });
    }
  }

  if (synced.length > 0) {
    // LRC 惯例：offset 正值表示歌词整体提前
    for (const l of synced) {
      l.timeMs = Math.max(0, l.timeMs - offsetMs);
    }
    synced.sort((a, b) => a.timeMs - b.timeMs);
    return { synced: true, lines: synced };
  }
  return {
    synced: false,
    lines: plain.map((text) => ({ timeMs: 0, text })),
  };
}

/**
 * 定位当前播放位置对应的歌词行（最后一个 timeMs ≤ positionMs 的行）。
 * 未到第一行时间时返回 -1。lines 需按 timeMs 升序（parseLrc 已保证）。
 */
export function findActiveLine(lines: LrcLine[], positionMs: number): number {
  let lo = 0;
  let hi = lines.length - 1;
  let ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid].timeMs <= positionMs) {
      ans = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return ans;
}
