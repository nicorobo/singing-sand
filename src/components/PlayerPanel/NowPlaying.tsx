import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { usePlayerStore } from "../../stores/playerStore";
import { useLibraryStore } from "../../stores/libraryStore";
import styles from "./PlayerPanel.module.scss";

function cx(...classes: (string | false | undefined)[]) {
  return classes.filter(Boolean).join(" ");
}

export function NowPlaying() {
  const currentTrackId = usePlayerStore((s) => s.currentTrackId);
  const currentTrackTitle = usePlayerStore((s) => s.currentTrackTitle);
  const currentTrackArtist = usePlayerStore((s) => s.currentTrackArtist);
  const isPlaying = usePlayerStore((s) => s.isPlaying);
  const setIsPlaying = usePlayerStore((s) => s.setIsPlaying);
  const setCurrentTrack = usePlayerStore((s) => s.setCurrentTrack);
  const tracks = useLibraryStore((s) => s.tracks);
  const [artError, setArtError] = useState(false);

  const currentIdx = tracks.findIndex((t) => t.id === currentTrackId);

  const playAdjacentTrack = async (delta: -1 | 1) => {
    const idx = currentIdx + delta;
    if (idx < 0 || idx >= tracks.length) return;
    const track = tracks[idx];
    try {
      await invoke("play_track", { trackId: track.id });
      setCurrentTrack(track.id, track.title, track.artist);
      setIsPlaying(true);
    } catch (e) {
      console.error("play_track failed:", e);
    }
  };

  const handlePlayPause = async () => {
    try {
      if (isPlaying) {
        await invoke("pause");
        setIsPlaying(false);
      } else {
        await invoke("play");
        setIsPlaying(true);
      }
    } catch (e) {
      console.error("play/pause failed:", e);
    }
  };

  return (
    <div className={styles.nowPlaying}>
      <div className={styles.artWrap}>
        {currentTrackId && !artError ? (
          <img
            className={styles.artImg}
            src={`art://localhost/${currentTrackId}`}
            alt=""
            onError={() => setArtError(true)}
            draggable={false}
          />
        ) : (
          <span>♪</span>
        )}
      </div>

      <div className={styles.trackInfo}>
        <div className={styles.trackName}>
          {currentTrackTitle || (currentTrackId ? "Unknown" : "No track")}
        </div>
        <div className={styles.trackArtist}>{currentTrackArtist}</div>
        <div className={styles.controls}>
          <button
            className={styles.controlBtn}
            onClick={() => playAdjacentTrack(-1)}
            disabled={currentIdx <= 0}
            title="Previous"
          >
            ⏮
          </button>
          <button
            className={cx(styles.controlBtn, styles.primary)}
            onClick={handlePlayPause}
            title={isPlaying ? "Pause" : "Play"}
          >
            {isPlaying ? "⏸" : "▶"}
          </button>
          <button
            className={styles.controlBtn}
            onClick={() => playAdjacentTrack(1)}
            disabled={currentIdx < 0 || currentIdx >= tracks.length - 1}
            title="Next"
          >
            ⏭
          </button>
        </div>
      </div>
    </div>
  );
}
