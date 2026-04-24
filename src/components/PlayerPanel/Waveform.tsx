import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { usePlayerStore } from "../../stores/playerStore";
import styles from "./PlayerPanel.module.scss";

export function Waveform() {
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const currentTrackId = usePlayerStore((s) => s.currentTrackId);
  const containerRef = useRef<HTMLDivElement>(null);
  const blobUrlRef = useRef<string | null>(null);
  const [waveformUrl, setWaveformUrl] = useState<string | null>(null);

  const fetchWaveform = useCallback(async (trackId: number) => {
    const el = containerRef.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    const w = Math.max(1, Math.round(width));
    const h = Math.max(1, Math.round(height));
    try {
      const result = await invoke("get_waveform", { track_id: trackId, width: w, height: h });
      const bytes = new Uint8Array(result as ArrayBuffer);
      const blob = new Blob([bytes], { type: "image/png" });
      if (blobUrlRef.current) URL.revokeObjectURL(blobUrlRef.current);
      const url = URL.createObjectURL(blob);
      blobUrlRef.current = url;
      setWaveformUrl(url);
    } catch (e) {
      console.error("get_waveform failed:", e);
    }
  }, []);

  // Listen for waveform-ready events
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<{ track_id: number }>("waveform-ready", (e) => {
      fetchWaveform(e.payload.track_id);
    }).then((fn) => { unlisten = fn; });
    return () => {
      unlisten?.();
      if (blobUrlRef.current) URL.revokeObjectURL(blobUrlRef.current);
    };
  }, [fetchWaveform]);

  // Re-fetch on container resize (debounced)
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const observer = new ResizeObserver(() => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        if (currentTrackId !== null) {
          fetchWaveform(currentTrackId);
        }
      }, 150);
    });
    observer.observe(el);
    return () => {
      observer.disconnect();
      if (timer) clearTimeout(timer);
    };
  }, [currentTrackId, fetchWaveform]);

  const handleClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const fraction = (e.clientX - rect.left) / rect.width;
    invoke("seek", { fraction }).catch(console.error);
  };

  const playheadPct =
    duration > 0 ? `${((position / duration) * 100).toFixed(2)}%` : "0%";

  return (
    <div ref={containerRef} className={styles.waveformWrap} onClick={handleClick}>
      {waveformUrl ? (
        <img
          className={styles.waveformImg}
          src={waveformUrl}
          alt="waveform"
          draggable={false}
        />
      ) : (
        <div className={styles.waveformEmpty}>No track loaded</div>
      )}
      {duration > 0 && (
        <div
          className={styles.playhead}
          style={{ left: playheadPct }}
        />
      )}
    </div>
  );
}
