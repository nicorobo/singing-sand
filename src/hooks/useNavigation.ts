import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLibraryStore } from "../stores/libraryStore";
import { useSidebarStore, NavItem } from "../stores/sidebarStore";
import { TrackDto } from "../types";

function navToSearchParams(nav: NavItem): { navKind: number; navId: number; navDir: string } {
  switch (nav.type) {
    case "all":      return { navKind: 0, navId: 0, navDir: "" };
    case "dir":      return { navKind: 1, navId: 0, navDir: nav.path };
    case "playlist": return { navKind: 2, navId: nav.id, navDir: "" };
    case "tag":      return { navKind: 3, navId: nav.id, navDir: "" };
    case "group":    return { navKind: 4, navId: nav.id, navDir: "" };
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
        const { navKind, navId, navDir } = navToSearchParams(activeNav);
        tracks = await invoke<TrackDto[]>("search_tracks", {
          query: searchQuery,
          navKind,
          navId,
          navDir,
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
            tracks = await invoke<TrackDto[]>("nav_playlist", { playlistId: activeNav.id });
            break;
          case "tag":
            tracks = await invoke<TrackDto[]>("nav_tag", { tagId: activeNav.id });
            break;
          case "group":
            tracks = await invoke<TrackDto[]>("nav_group", { groupId: activeNav.id });
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
