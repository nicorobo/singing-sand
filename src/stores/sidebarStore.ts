import { create } from "zustand";

export type NavItem =
  | { type: "all" }
  | { type: "dir"; id: number }
  | { type: "playlist"; id: number }
  | { type: "tag"; id: number };

export interface DirItem {
  id: number;
  path: string;
  expanded: boolean;
  children: DirItem[];
}

export interface Playlist {
  id: number;
  name: string;
}

export interface SidebarTag {
  id: number;
  name: string;
  color: string;
}

interface SidebarState {
  nav: NavItem;
  dirs: DirItem[];
  playlists: Playlist[];
  tags: SidebarTag[];
  setNav: (nav: NavItem) => void;
  setDirs: (dirs: DirItem[]) => void;
  setPlaylists: (playlists: Playlist[]) => void;
  setTags: (tags: SidebarTag[]) => void;
}

export const useSidebarStore = create<SidebarState>((set) => ({
  nav: { type: "all" },
  dirs: [],
  playlists: [],
  tags: [],
  setNav: (nav) => set({ nav }),
  setDirs: (dirs) => set({ dirs }),
  setPlaylists: (playlists) => set({ playlists }),
  setTags: (tags) => set({ tags }),
}));
