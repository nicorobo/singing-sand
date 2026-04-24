import { create } from "zustand";
import { SelectedTagDto, TrackDto } from "../types";

interface LibraryState {
  tracks: TrackDto[];
  selectedIds: Set<number>;
  expandedId: number | null;
  searchQuery: string;
  tagItems: SelectedTagDto[];
  setTracks: (tracks: TrackDto[]) => void;
  setSelectedIds: (ids: Set<number>) => void;
  setExpandedId: (id: number | null) => void;
  setSearchQuery: (q: string) => void;
  setTagItems: (items: SelectedTagDto[]) => void;
}

export const useLibraryStore = create<LibraryState>((set) => ({
  tracks: [],
  selectedIds: new Set(),
  expandedId: null,
  searchQuery: "",
  tagItems: [],
  setTracks: (tracks) => set({ tracks }),
  setSelectedIds: (selectedIds) => set({ selectedIds }),
  setExpandedId: (expandedId) => set({ expandedId }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),
  setTagItems: (tagItems) => set({ tagItems }),
}));
