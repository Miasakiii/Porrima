/** 毫秒 → "mm:ss"（超过 1 小时为 "h:mm:ss"）。 */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "0:00";
  const totalSec = Math.floor(ms / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  const ss = String(s).padStart(2, "0");
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${ss}`;
  return `${m}:${ss}`;
}

/** 采样率 Hz → "44.1kHz" / "96kHz"。 */
export function formatSampleRate(hz: number): string {
  if (!hz) return "";
  const khz = hz / 1000;
  return `${Number.isInteger(khz) ? khz : khz.toFixed(1)}kHz`;
}
