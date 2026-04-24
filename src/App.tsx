import { useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Sidebar } from "./components/Sidebar/Sidebar";
import { SearchBar } from "./components/SearchBar/SearchBar";
import { TrackList } from "./components/TrackList/TrackList";
import { PlayerPanel } from "./components/PlayerPanel/PlayerPanel";
import { useSidebarStore } from "./stores/sidebarStore";
import { useNavigation } from "./hooks/useNavigation";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { SidebarDataDto } from "./types";

export default function App() {
  const setDirs = useSidebarStore((s) => s.setDirs);
  const setPlaylists = useSidebarStore((s) => s.setPlaylists);
  const setTags = useSidebarStore((s) => s.setTags);
  const { fetchTracks } = useNavigation();

  const onLibraryChanged = useCallback(() => {
    fetchTracks();
  }, [fetchTracks]);

  useTauriEvents(onLibraryChanged);

  useEffect(() => {
    invoke<SidebarDataDto>("get_sidebar_data").then((data) => {
      setDirs(data.dir_tree);
      setPlaylists(data.playlists);
      setTags(data.tags);
    });
    fetchTracks();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="app">
      <Sidebar />
      <div className="main">
        <SearchBar />
        <TrackList />
        <PlayerPanel />
      </div>
    </div>
  );
}
