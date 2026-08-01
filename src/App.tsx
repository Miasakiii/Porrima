import { useEffect, useState } from "react";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";
import { TitleBar } from "@/components/TitleBar";
import { Sidebar } from "@/components/Sidebar/Sidebar";
import { PlayerBar } from "@/components/PlayerBar/PlayerBar";
import { LyricsPanel } from "@/components/Lyrics/LyricsPanel";
import { LibraryPage } from "@/pages/Music/LibraryPage";
import { AlbumsPage } from "@/pages/Music/AlbumsPage";
import { ArtistsPage } from "@/pages/Music/ArtistsPage";
import { PlaylistsPage } from "@/pages/Music/PlaylistsPage";
import { StatsPage } from "@/pages/Music/StatsPage";
import { QueuePage } from "@/pages/Queue/QueuePage";
import { CreatePlaylistDialog } from "@/components/Playlist/CreatePlaylistDialog";
import { SettingsPage } from "@/pages/Settings/SettingsPage";
import { VideoPage } from "@/pages/Video/VideoPage";
import { AppEvents, useTauriEvent } from "@/lib/events";
import { type NavPage } from "@/lib/nav";
import { cn } from "@/lib/utils";
import { initEngine } from "@/lib/engine";
import { useKeyboard } from "@/hooks/useKeyboard";
import { useDynamicAccent } from "@/hooks/useDynamicAccent";
import { useLibraryStore } from "@/stores/libraryStore";
import { usePlayerStore } from "@/stores/playerStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { ScanProgress } from "@/lib/types";

/**
 * 正式布局（设计规范 4.4）：TitleBar + Sidebar 220px + Content + 底部 PlayerBar 72px 常驻。
 * 页面切换用简单前端状态，不引入 react-router。
 */
function App() {
  const [page, setPage] = useState<NavPage>("library");

  // 启动初始化：主题/设置 → 引擎适配器 → 播放器状态 + Channel → 曲库首屏
  useEffect(() => {
    void useSettingsStore.getState().load();
    // 引擎适配器先于 playerStore.init：engine:cmd 监听就绪后再接受后端指令
    void initEngine();
    void usePlayerStore.getState().init();
    void useLibraryStore.getState().refresh();
  }, []);

  // 扫描进度事件（library:scan-progress）
  useTauriEvent<ScanProgress>(AppEvents.LibraryScanProgress, (p) => {
    useLibraryStore.getState().handleScanProgress(p);
  });

  // 视频文件打开（文件关联/命令行）：切换到视频页并播放
  useTauriEvent<string>(AppEvents.VideoOpen, (path) => {
    setPage("video");
    // 通过自定义事件通知 VideoPage 播放该文件
    window.dispatchEvent(new CustomEvent("porrima:play-video", { detail: path }));
  });

  // 拖放文件播放：音频入库播放，视频进入视频模式
  useEffect(() => {
    // 阻止浏览器默认拖放行为（否则 WebView 会尝试打开文件，Tauri 事件不触发）
    const prevent = (e: DragEvent) => e.preventDefault();
    document.addEventListener("dragover", prevent);
    document.addEventListener("drop", prevent);

    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "drop" && payload.paths.length > 0) {
        void invoke("open_dropped_files", { paths: payload.paths });
      }
    });
    return () => {
      document.removeEventListener("dragover", prevent);
      document.removeEventListener("drop", prevent);
      unlisten.then((fn) => fn());
    };
  }, []);

  // 视频页：body 透明让 mpv 视频透出；离开视频页恢复
  useEffect(() => {
    if (page === "video") {
      document.body.classList.add("video-transparent");
    } else {
      document.body.classList.remove("video-transparent");
    }
    return () => document.body.classList.remove("video-transparent");
  }, [page]);

  useKeyboard();

  // 当前曲目封面主题色 → 全局强调色
  useDynamicAccent();

  return (
    <TooltipProvider>
      <div className={cn(
        "flex h-screen flex-col overflow-hidden text-foreground",
        page !== "video" && "bg-background",
      )}>
        <TitleBar />
        <div className="relative flex min-h-0 flex-1">
          <Sidebar page={page} onNavigate={setPage} />
          <main className="min-w-0 flex-1">
            {page === "library" && <LibraryPage onNavigate={setPage} />}
            {page === "albums" && <AlbumsPage />}
            {page === "artists" && <ArtistsPage />}
            {page === "playlists" && <PlaylistsPage />}
            {page === "queue" && <QueuePage />}
            {page === "stats" && <StatsPage />}
            {page === "video" && <VideoPage />}
            {page === "settings" && <SettingsPage />}
          </main>
          {/* 歌词浮层：覆盖 Sidebar+内容区，不遮 PlayerBar */}
          <LyricsPanel />
        </div>
        <PlayerBar />
      </div>
      <Toaster position="bottom-right" />
      <CreatePlaylistDialog />
    </TooltipProvider>
  );
}

export default App;
