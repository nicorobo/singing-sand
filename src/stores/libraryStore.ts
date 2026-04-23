import { create } from "zustand";

export interface Track {
  id: number;
  title: string;
  artist: string;
  album: string;
  duration: number;
  bpm: number | null;
  path: string;
  tags: Tag[];
}

export interface Tag {
  id: number;
  name: string;
  color: string;
}

interface LibraryState {
  tracks: Track[];
  selectedIds: Set<number>;
  expandedId: number | null;
  searchQuery: string;
  setTracks: (tracks: Track[]) => void;
  setSelectedIds: (ids: Set<number>) => void;
  setExpandedId: (id: number | null) => void;
  setSearchQuery: (q: string) => void;
}

export const useLibraryStore = create<LibraryState>((set) => ({
  tracks: [],
  selectedIds: new Set(),
  expandedId: null,
  searchQuery: "",
  setTracks: (tracks) => set({ tracks }),
  setSelectedIds: (selectedIds) => set({ selectedIds }),
  setExpandedId: (expandedId) => set({ expandedId }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),
}));
