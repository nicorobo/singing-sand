import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { usePlayerStore } from "../stores/playerStore";

export function useAppInteractions() {
  const isPlaying = usePlayerStore((s) => s.isPlaying);
  const setIsPlaying = usePlayerStore((s) => s.setIsPlaying);
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (document.activeElement as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

      if (e.key === " ") {
        e.preventDefault();
        if (isPlaying) {
          invoke("pause").then(() => setIsPlaying(false)).catch(console.error);
        } else {
          invoke("play").then(() => setIsPlaying(true)).catch(console.error);
        }
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        if (duration > 0) {
          const fraction = Math.max(0, (position - 5) / duration);
          invoke("seek", { fraction }).catch(console.error);
        }
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        if (duration > 0) {
          const fraction = Math.min(1, (position + 5) / duration);
          invoke("seek", { fraction }).catch(console.error);
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isPlaying, setIsPlaying, position, duration]);

  // Drag-drop directories onto the window
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    getCurrentWebview().onDragDropEvent(async (event) => {
      if (event.payload.type === "drop") {
        for (const path of event.payload.paths) {
          try {
            await invoke("add_directory_path", { path });
          } catch (err) {
            console.error("add_directory_path failed:", err);
          }
        }
      }
    }).then((fn) => { unlisten = fn; }).catch(console.error);

    return () => { unlisten?.(); };
  }, []);
}
