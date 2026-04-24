import { create } from "zustand";

interface UiState {
  pendingAnalysisCount: number;
  duplicateMessage: string | null;
  setPendingAnalysisCount: (count: number) => void;
  setDuplicateMessage: (msg: string | null) => void;
}

export const useUiStore = create<UiState>((set) => ({
  pendingAnalysisCount: 0,
  duplicateMessage: null,
  setPendingAnalysisCount: (pendingAnalysisCount) => set({ pendingAnalysisCount }),
  setDuplicateMessage: (duplicateMessage) => set({ duplicateMessage }),
}));
