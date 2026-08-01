import { useEffect, useState } from "react";
import { AudioLines, Subtitles, X } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  getAudioTracks,
  getSubtitleTracks,
  setAudioTrack,
  setSubtitleDelay,
  setSubtitleTrack,
  type AudioTrack,
  type SubtitleTrack,
} from "@/lib/engine";

interface TrackPanelProps {
  open: boolean;
  onClose: () => void;
}

/** 字幕/音轨选择面板（视频控制栏弹出）。 */
export function TrackPanel({ open, onClose }: TrackPanelProps) {
  const [subTracks, setSubTracks] = useState<SubtitleTrack[]>([]);
  const [audioTracks, setAudioTracks] = useState<AudioTrack[]>([]);
  const [activeSid, setActiveSid] = useState<number>(1);
  const [activeAid, setActiveAid] = useState<number>(1);
  const [subDelay, setSubDelay] = useState(0);

  // 面板打开时刷新轨道列表
  useEffect(() => {
    if (!open) return;
    void getSubtitleTracks().then(setSubTracks);
    void getAudioTracks().then(setAudioTracks);
  }, [open]);

  if (!open) return null;

  const handleSubSelect = async (id: number) => {
    setActiveSid(id);
    await setSubtitleTrack(id);
  };

  const handleAudioSelect = async (id: number) => {
    setActiveAid(id);
    await setAudioTrack(id);
  };

  const adjustDelay = async (deltaMs: number) => {
    const next = subDelay + deltaMs / 1000;
    setSubDelay(next);
    await setSubtitleDelay(next);
  };

  return (
    <div className="absolute right-4 bottom-16 z-50 w-64 rounded-lg border border-border bg-popover p-3 shadow-lg">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-xs font-medium text-popover-foreground">轨道设置</span>
        <button
          type="button"
          onClick={onClose}
          className="rounded p-0.5 text-muted-foreground hover:text-foreground"
        >
          <X className="size-3.5" />
        </button>
      </div>

      {/* 字幕轨道 */}
      <div className="mb-3">
        <div className="mb-1 flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
          <Subtitles className="size-3" />
          字幕
        </div>
        <div className="space-y-0.5">
          <TrackButton
            label="禁用字幕"
            active={activeSid === 0}
            onClick={() => void handleSubSelect(0)}
          />
          {subTracks.map((t) => (
            <TrackButton
              key={t.id}
              label={`${t.title} [${t.lang}]`}
              active={activeSid === t.id}
              onClick={() => void handleSubSelect(t.id)}
            />
          ))}
          {subTracks.length === 0 && (
            <p className="px-2 py-1 text-[11px] text-muted-foreground">无可用字幕</p>
          )}
        </div>
      </div>

      {/* 字幕延迟 */}
      {subTracks.length > 0 && activeSid > 0 && (
        <div className="mb-3 flex items-center justify-between rounded-md bg-muted/50 px-2 py-1.5">
          <span className="text-[11px] text-muted-foreground">字幕延迟</span>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={() => void adjustDelay(-250)}
              className="rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-muted hover:text-foreground"
            >
              -250ms
            </button>
            <span className="w-14 text-center text-[11px] tabular-nums text-foreground">
              {(subDelay * 1000).toFixed(0)}ms
            </span>
            <button
              type="button"
              onClick={() => void adjustDelay(250)}
              className="rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-muted hover:text-foreground"
            >
              +250ms
            </button>
          </div>
        </div>
      )}

      {/* 音轨 */}
      <div>
        <div className="mb-1 flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
          <AudioLines className="size-3" />
          音轨
        </div>
        <div className="space-y-0.5">
          {audioTracks.map((t) => (
            <TrackButton
              key={t.id}
              label={`${t.title} [${t.lang}]`}
              active={activeAid === t.id}
              onClick={() => void handleAudioSelect(t.id)}
            />
          ))}
          {audioTracks.length === 0 && (
            <p className="px-2 py-1 text-[11px] text-muted-foreground">无可用音轨</p>
          )}
        </div>
      </div>
    </div>
  );
}

function TrackButton({
  label,
  active,
  onClick,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "w-full rounded-md px-2 py-1 text-left text-xs transition-colors",
        active
          ? "bg-accent/15 font-medium text-accent"
          : "text-popover-foreground hover:bg-muted",
      )}
    >
      {label}
    </button>
  );
}
