import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLibraryStore } from "../stores/libraryStore";
import { useSidebarStore, NavItem } from "../stores/sidebarStore";
import { TrackDto } from "../types";

function navToSearchParams(nav: NavItem): { nav_kind: number; nav_id: number; nav_dir: string } {
  switch (nav.type) {
    case "all":      return { nav_kind: 0, nav_id: 0, nav_dir: "" };
    case "dir":      return { nav_kind: 1, nav_id: 0, nav_dir: nav.path };
    case "playlist": return { nav_kind: 2, nav_id: nav.id, nav_dir: "" };
    case "tag":      return { nav_kind: 3, nav_id: nav.id, nav_dir: "" };
  }
}

export function useNavigation() {
  const nav = useSidebarStore((s) => s.nav);
  const searchQuery = useLibraryStore((s) => s.searchQuery);
  const setTracks = useLibraryStore((s) => s.setTracks);

  const fetchTracks = useCallback(async (overrideNav?: NavItem) => {
    const activeNav = overrideNav ?? nav;
    try {
      let tracks: TrackDto[];
      if (searchQuery) {
        const { nav_kind, nav_id, nav_dir } = navToSearchParams(activeNav);
        tracks = await invoke<TrackDto[]>("search_tracks", {
          query: searchQuery,
          nav_kind,
          nav_id,
          nav_dir,
        });
      } else {
        switch (activeNav.type) {
          case "all":
            tracks = await invoke<TrackDto[]>("nav_all");
            break;
          case "dir":
            tracks = await invoke<TrackDto[]>("nav_select_dir", { path: activeNav.path });
            break;
          case "playlist":
            tracks = await invoke<TrackDto[]>("nav_playlist", { playlist_id: activeNav.id });
            break;
          case "tag":
            tracks = await invoke<TrackDto[]>("nav_tag", { tag_id: activeNav.id });
            break;
        }
      }
      setTracks(tracks);
    } catch (e) {
      console.error("fetchTracks failed:", e);
    }
  }, [nav, searchQuery, setTracks]);

  return { fetchTracks };
}
