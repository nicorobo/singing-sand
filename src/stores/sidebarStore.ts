import { create } from "zustand";
import { DirTreeItemDto, PlaylistDto, PlaylistGroupDto, TagDto } from "../types";

export type NavItem =
  | { type: "all" }
  | { type: "dir"; path: string }
  | { type: "playlist"; id: number }
  | { type: "group"; id: number }
  | { type: "tag"; id: number };

interface SidebarState {
  nav: NavItem;
  dirs: DirTreeItemDto[];
  playlists: PlaylistDto[];
  groups: PlaylistGroupDto[];
  tags: TagDto[];
  setNav: (nav: NavItem) => void;
  setDirs: (dirs: DirTreeItemDto[]) => void;
  setPlaylists: (playlists: PlaylistDto[]) => void;
  setGroups: (groups: PlaylistGroupDto[]) => void;
  setTags: (tags: TagDto[]) => void;
}

export const useSidebarStore = create<SidebarState>((set) => ({
  nav: { type: "all" },
  dirs: [],
  playlists: [],
  groups: [],
  tags: [],
  setNav: (nav) => set({ nav }),
  setDirs: (dirs) => set({ dirs }),
  setPlaylists: (playlists) => set({ playlists }),
  setGroups: (groups) => set({ groups }),
  setTags: (tags) => set({ tags }),
}));
