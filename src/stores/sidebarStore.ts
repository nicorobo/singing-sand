import { create } from "zustand";
import { DirTreeItemDto, PlaylistDto, TagDto } from "../types";

export type NavItem =
  | { type: "all" }
  | { type: "dir"; path: string }
  | { type: "playlist"; id: number }
  | { type: "tag"; id: number };

interface SidebarState {
  nav: NavItem;
  dirs: DirTreeItemDto[];
  playlists: PlaylistDto[];
  tags: TagDto[];
  setNav: (nav: NavItem) => void;
  setDirs: (dirs: DirTreeItemDto[]) => void;
  setPlaylists: (playlists: PlaylistDto[]) => void;
  setTags: (tags: TagDto[]) => void;
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
