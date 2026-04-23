import { create } from "zustand";

interface UiState {
  pendingAnalysisCount: number;
  setPendingAnalysisCount: (count: number) => void;
}

export const useUiStore = create<UiState>((set) => ({
  pendingAnalysisCount: 0,
  setPendingAnalysisCount: (pendingAnalysisCount) =>
    set({ pendingAnalysisCount }),
}));
