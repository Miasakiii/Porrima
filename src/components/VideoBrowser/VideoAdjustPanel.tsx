import { useState } from "react";
import { RotateCcw, RotateCw, X } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  ASPECT_RATIOS,
  resetVideoAdjustments,
  setAspectRatio,
  setBrightness,
  setContrast,
  setGamma,
  setHue,
  setSaturation,
  setSubtitleDelay,
  setVideoRotation,
} from "@/lib/engine";

interface VideoAdjustPanelProps {
  open: boolean;
  onClose: () => void;
}

interface SliderState {
  brightness: number;
  contrast: number;
  saturation: number;
  hue: number;
  gamma: number;
}

const DEFAULT_SLIDERS: SliderState = {
  brightness: 0,
  contrast: 0,
  saturation: 0,
  hue: 0,
  gamma: 0,
};

const SLIDER_LABELS: Record<keyof SliderState, string> = {
  brightness: "亮度",
  contrast: "对比度",
  saturation: "饱和度",
  hue: "色调",
  gamma: "伽马",
};

const SETTERS: Record<keyof SliderState, (v: number) => Promise<void>> = {
  brightness: setBrightness,
  contrast: setContrast,
  saturation: setSaturation,
  hue: setHue,
  gamma: setGamma,
};

/** 画面调节面板：比例/旋转/亮度/对比度/饱和度/色调/伽马 + 字幕延迟。 */
export function VideoAdjustPanel({ open, onClose }: VideoAdjustPanelProps) {
  const [sliders, setSliders] = useState<SliderState>(DEFAULT_SLIDERS);
  const [aspect, setAspect] = useState("");
  const [rotation, setRotation] = useState(0);
  const [subDelayMs, setSubDelayMs] = useState(0);

  if (!open) return null;

  const handleSlider = (key: keyof SliderState, value: number) => {
    setSliders((s) => ({ ...s, [key]: value }));
    void SETTERS[key](value);
  };

  const handleAspect = (value: string) => {
    setAspect(value);
    void setAspectRatio(value);
  };

  const handleRotate = (delta: number) => {
    const next = ((rotation + delta) % 360 + 360) % 360;
    setRotation(next);
    void setVideoRotation(next);
  };

  const handleReset = () => {
    setSliders(DEFAULT_SLIDERS);
    setAspect("");
    setRotation(0);
    setSubDelayMs(0);
    void resetVideoAdjustments();
    void setSubtitleDelay(0);
  };

  const handleSubDelay = (ms: number) => {
    setSubDelayMs(ms);
    void setSubtitleDelay(ms / 1000);
  };

  return (
    <div className="absolute right-4 bottom-16 z-50 w-72 rounded-lg border border-border bg-popover p-3 shadow-lg">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-xs font-medium text-popover-foreground">画面调节</span>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={handleReset}
            className="rounded px-1.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            重置
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-0.5 text-muted-foreground hover:text-foreground"
          >
            <X className="size-3.5" />
          </button>
        </div>
      </div>

      {/* 画面比例 */}
      <div className="mb-2">
        <span className="mb-1 block text-[11px] text-muted-foreground">画面比例</span>
        <div className="flex flex-wrap gap-1">
          {ASPECT_RATIOS.map(({ value, label }) => (
            <button
              key={value}
              type="button"
              onClick={() => handleAspect(value)}
              className={cn(
                "rounded px-2 py-0.5 text-[11px] transition-colors",
                aspect === value
                  ? "bg-accent text-accent-foreground"
                  : "bg-muted text-muted-foreground hover:text-foreground",
              )}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {/* 旋转 */}
      <div className="mb-2 flex items-center justify-between">
        <span className="text-[11px] text-muted-foreground">旋转</span>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => handleRotate(-90)}
            className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <RotateCcw className="size-3.5" />
          </button>
          <span className="w-8 text-center text-[11px] tabular-nums text-foreground">
            {rotation}°
          </span>
          <button
            type="button"
            onClick={() => handleRotate(90)}
            className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <RotateCw className="size-3.5" />
          </button>
        </div>
      </div>

      {/* 色彩滑块 */}
      <div className="space-y-1.5">
        {(Object.keys(SLIDER_LABELS) as (keyof SliderState)[]).map((key) => (
          <div key={key} className="flex items-center gap-2">
            <span className="w-10 shrink-0 text-[11px] text-muted-foreground">
              {SLIDER_LABELS[key]}
            </span>
            <input
              type="range"
              min={-100}
              max={100}
              value={sliders[key]}
              onChange={(e) => handleSlider(key, Number(e.target.value))}
              className="h-1 flex-1 cursor-pointer appearance-none rounded-full bg-muted accent-[var(--accent)] [&::-webkit-slider-thumb]:size-2.5 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-foreground"
            />
            <span className="w-7 shrink-0 text-right text-[10px] tabular-nums text-muted-foreground">
              {sliders[key]}
            </span>
          </div>
        ))}
      </div>

      {/* 字幕延迟 */}
      <div className="mt-2 border-t border-border pt-2">
        <div className="mb-1 flex items-center justify-between">
          <span className="text-[11px] text-muted-foreground">字幕延迟</span>
          <button
            type="button"
            onClick={() => handleSubDelay(0)}
            className="rounded px-1.5 py-0.5 text-[10px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            归零
          </button>
        </div>
        <div className="flex items-center gap-2">
          <input
            type="range"
            min={-5000}
            max={5000}
            step={50}
            value={subDelayMs}
            onChange={(e) => handleSubDelay(Number(e.target.value))}
            className="h-1 flex-1 cursor-pointer appearance-none rounded-full bg-muted accent-[var(--accent)] [&::-webkit-slider-thumb]:size-2.5 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-foreground"
          />
          <span className="w-14 shrink-0 text-right text-[10px] tabular-nums text-muted-foreground">
            {subDelayMs > 0 ? "+" : ""}{subDelayMs}ms
          </span>
        </div>
        <div className="mt-1 flex justify-between text-[9px] text-muted-foreground/60">
          <span>-5s（提前）</span>
          <span>+5s（延迟）</span>
        </div>
      </div>
    </div>
  );
}
