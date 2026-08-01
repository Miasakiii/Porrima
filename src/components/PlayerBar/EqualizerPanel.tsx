import { useState } from "react";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  EQ_BANDS,
  EQ_PRESETS,
  getEqualizerGains,
  resetEqualizer,
  setEqualizer,
} from "@/lib/engine";

interface EqualizerPanelProps {
  open: boolean;
  onClose: () => void;
}

/** 频段标签格式化。 */
function bandLabel(hz: number): string {
  return hz >= 1000 ? `${hz / 1000}k` : String(hz);
}

/** 10 段均衡器面板：预设 + 垂直滑块。 */
export function EqualizerPanel({ open, onClose }: EqualizerPanelProps) {
  const [gains, setGains] = useState<number[]>(() => getEqualizerGains());
  const [activePreset, setActivePreset] = useState("平坦");

  if (!open) return null;

  const handleBand = (index: number, value: number) => {
    const next = [...gains];
    next[index] = value;
    setGains(next);
    setActivePreset("");
    void setEqualizer(next);
  };

  const handlePreset = (name: string, values: number[]) => {
    setGains([...values]);
    setActivePreset(name);
    void setEqualizer(values);
  };

  const handleReset = () => {
    setGains([0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    setActivePreset("平坦");
    void resetEqualizer();
  };

  return (
    <div className="absolute bottom-[76px] left-1/2 z-50 w-[420px] -translate-x-1/2 rounded-lg border border-border bg-popover p-4 shadow-lg">
      {/* 标题栏 */}
      <div className="mb-3 flex items-center justify-between">
        <span className="text-xs font-medium text-popover-foreground">均衡器</span>
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

      {/* 预设 */}
      <div className="mb-3 flex flex-wrap gap-1">
        {Object.entries(EQ_PRESETS).map(([name, values]) => (
          <button
            key={name}
            type="button"
            onClick={() => handlePreset(name, values)}
            className={cn(
              "rounded px-2 py-0.5 text-[11px] transition-colors",
              activePreset === name
                ? "bg-accent text-accent-foreground"
                : "bg-muted text-muted-foreground hover:text-foreground",
            )}
          >
            {name}
          </button>
        ))}
      </div>

      {/* 10 段垂直滑块 */}
      <div className="flex items-end justify-between gap-1">
        {EQ_BANDS.map((hz, i) => (
          <div key={hz} className="flex flex-col items-center gap-1">
            {/* 增益值 */}
            <span className="text-[9px] tabular-nums text-muted-foreground">
              {gains[i] > 0 ? "+" : ""}{gains[i]}
            </span>
            {/* 垂直滑块：用旋转的水平 range 实现 */}
            <div className="relative flex h-24 w-5 items-center justify-center">
              <input
                type="range"
                min={-12}
                max={12}
                step={1}
                value={gains[i]}
                onChange={(e) => handleBand(i, Number(e.target.value))}
                className="absolute h-24 w-1 cursor-pointer appearance-none rounded-full bg-muted accent-[var(--accent)] [writing-mode:vertical-lr] [direction:rtl] [&::-webkit-slider-thumb]:size-2.5 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-foreground"
              />
            </div>
            {/* 频率标签 */}
            <span className="text-[9px] text-muted-foreground">{bandLabel(hz)}</span>
          </div>
        ))}
      </div>

      {/* 底部 dB 标尺提示 */}
      <div className="mt-2 flex justify-between text-[9px] text-muted-foreground/60">
        <span>-12dB</span>
        <span>0</span>
        <span>+12dB</span>
      </div>
    </div>
  );
}
