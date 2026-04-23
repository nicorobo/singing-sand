import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { usePlayerStore } from "../stores/playerStore";
import { useUiStore } from "../stores/uiStore";

export function useTauriEvents() {
  const setPosition = usePlayerStore((s) => s.setPosition);
  const setDuration = usePlayerStore((s) => s.setDuration);
  const setIsPlaying = usePlayerStore((s) => s.setIsPlaying);
  const setPendingAnalysisCount = useUiStore((s) => s.setPendingAnalysisCount);

  useEffect(() => {
    const unlisten: Array<() => void> = [];

    listen<{ position: number; duration: number }>("position-changed", (e) => {
      setPosition(e.payload.position);
      setDuration(e.payload.duration);
    }).then((fn) => unlisten.push(fn));

    listen<Record<string, never>>("track-finished", () => {
      setIsPlaying(false);
    }).then((fn) => unlisten.push(fn));

    listen<{ pending_count: number }>("analysis-progress", (e) => {
      setPendingAnalysisCount(e.payload.pending_count);
    }).then((fn) => unlisten.push(fn));

    return () => {
      unlisten.forEach((fn) => fn());
    };
  }, [setPosition, setDuration, setIsPlaying, setPendingAnalysisCount]);
}
