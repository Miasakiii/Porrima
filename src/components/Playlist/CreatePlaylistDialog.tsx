import { useEffect, useState } from "react";
import { usePlaylistStore } from "@/stores/playlistStore";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";

/**
 * 新建播放列表对话框（全局挂载一次，由 playlistStore.createDialogOpen 驱动）。
 * 从“添加到播放列表 → 新建”触发时携带 pendingTrackIds，创建后一并加入。
 */
export function CreatePlaylistDialog() {
  const open = usePlaylistStore((s) => s.createDialogOpen);
  const pendingCount = usePlaylistStore((s) => s.pendingTrackIds.length);
  const close = usePlaylistStore((s) => s.closeCreateDialog);
  const confirmCreate = usePlaylistStore((s) => s.confirmCreate);
  const [name, setName] = useState("");

  // 每次打开重置输入
  useEffect(() => {
    if (open) setName("");
  }, [open]);

  const submit = () => {
    const n = name.trim();
    if (n) void confirmCreate(n);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (!o) close();
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>新建播放列表</DialogTitle>
          <DialogDescription>
            {pendingCount > 0
              ? `创建后将加入 ${pendingCount} 首曲目`
              : "为你的歌单起个名字"}
          </DialogDescription>
        </DialogHeader>
        <Input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
          placeholder="播放列表名称"
          maxLength={100}
        />
        <DialogFooter>
          <Button variant="outline" onClick={close}>
            取消
          </Button>
          <Button onClick={submit} disabled={!name.trim()}>
            创建
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
