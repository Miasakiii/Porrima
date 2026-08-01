import type { CoverColor } from "@/lib/types";

/**
 * 动态强调色：把当前曲目封面的代表色写入全局 --accent（及 --ring / --accent-foreground）。
 *
 * 设计（globals.css §4.2 已预留）：整套配色用 OKLCH。这里把后端返回的 sRGB
 * 换算到 OKLCH，明度固定为「主题友好值」（暗色偏亮 / 浅色偏暗）、彩度封顶，
 * 只保留封面的色相，保证 text-accent / bg-accent 在明暗主题下都有足够对比且不刺眼。
 * 覆盖以内联样式写在根元素上（优先级高于样式表），停止播放/无封面时移除以恢复默认。
 */

/** 被动态覆盖的 CSS 变量（clearAccent 时逐一移除）。 */
const ACCENT_VARS = ["--accent", "--accent-foreground", "--ring"] as const;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** sRGB(0-255) → OKLCH（L 0-1，C，H 度 0-360）。Björn Ottosson 的 OKLab 变换。 */
function srgbToOklch(r: number, g: number, b: number): { l: number; c: number; h: number } {
  const lin = (v: number) => {
    const s = v / 255;
    return s <= 0.04045 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  };
  const rl = lin(r);
  const gl = lin(g);
  const bl = lin(b);

  const l_ = Math.cbrt(0.4122214708 * rl + 0.5363325363 * gl + 0.0514459929 * bl);
  const m_ = Math.cbrt(0.2119034982 * rl + 0.6806995451 * gl + 0.1073969566 * bl);
  const s_ = Math.cbrt(0.0883024619 * rl + 0.2817188376 * gl + 0.6299787005 * bl);

  const l = 0.2104542553 * l_ + 0.793617785 * m_ - 0.0040720468 * s_;
  const a = 1.9779984951 * l_ - 2.428592205 * m_ + 0.4505937099 * s_;
  const bb = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.808675766 * s_;

  const c = Math.hypot(a, bb);
  let h = (Math.atan2(bb, a) * 180) / Math.PI;
  if (h < 0) h += 360;
  return { l, c, h };
}

/** 应用封面代表色为全局强调色（按当前明暗主题裁剪）。 */
export function applyAccent(color: CoverColor, theme: "dark" | "light"): void {
  const { c, h } = srgbToOklch(color.r, color.g, color.b);
  // 明度固定：暗色主题偏亮、浅色主题偏暗，保证与背景对比稳定；彩度封顶避免刺眼。
  const l = theme === "dark" ? 0.72 : 0.55;
  const chroma = clamp(c, 0, 0.15);
  const accent = `oklch(${l.toFixed(3)} ${chroma.toFixed(3)} ${h.toFixed(1)})`;
  // 强调色上的前景（按钮文字）：按明度选深/浅，保证可读。
  const fg = l >= 0.65 ? `oklch(0.16 0.01 ${h.toFixed(1)})` : "oklch(0.985 0.002 85)";

  const root = document.documentElement.style;
  root.setProperty("--accent", accent);
  root.setProperty("--ring", accent);
  root.setProperty("--accent-foreground", fg);
}

/** 清除动态强调色，恢复样式表默认（暗/亮主题各自的 --accent）。 */
export function clearAccent(): void {
  const root = document.documentElement.style;
  for (const v of ACCENT_VARS) root.removeProperty(v);
}
