import { create } from "zustand";

interface PlayerState {
  currentTrackId: number | null;
  currentTrackTitle: string;
  currentTrackArtist: string;
  position: number;
  duration: number;
  isPlaying: boolean;
  settingsOpen: boolean;
  setCurrentTrack: (id: number, title: string, artist: string) => void;
  setCurrentTrackId: (id: number | null) => void;
  setPosition: (position: number) => void;
  setDuration: (duration: number) => void;
  setIsPlaying: (playing: boolean) => void;
  setSettingsOpen: (open: boolean) => void;
}

export const usePlayerStore = create<PlayerState>((set) => ({
  currentTrackId: null,
  currentTrackTitle: "",
  currentTrackArtist: "",
  position: 0,
  duration: 0,
  isPlaying: false,
  settingsOpen: false,
  setCurrentTrack: (id, title, artist) =>
    set({ currentTrackId: id, currentTrackTitle: title, currentTrackArtist: artist }),
  setCurrentTrackId: (id) => set({ currentTrackId: id }),
  setPosition: (position) => set({ position }),
  setDuration: (duration) => set({ duration }),
  setIsPlaying: (isPlaying) => set({ isPlaying }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
}));
