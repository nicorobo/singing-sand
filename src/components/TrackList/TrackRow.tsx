import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TrackDto, SelectionChangedDto } from "../../types";
import { useLibraryStore } from "../../stores/libraryStore";
import { usePlayerStore } from "../../stores/playerStore";
import { ExpandedTrackRow } from "./ExpandedTrackRow";
import styles from "./TrackList.module.scss";

function cx(...classes: (string | false | undefined)[]) {
  return classes.filter(Boolean).join(" ");
}

interface Props {
  track: TrackDto;
}

export function TrackRow({ track }: Props) {
  const selectedIds = useLibraryStore((s) => s.selectedIds);
  const setSelectedIds = useLibraryStore((s) => s.setSelectedIds);
  const setTagItems = useLibraryStore((s) => s.setTagItems);
  const expandedId = useLibraryStore((s) => s.expandedId);
  const setExpandedId = useLibraryStore((s) => s.setExpandedId);
  const setCurrentTrackId = usePlayerStore((s) => s.setCurrentTrackId);
  const setIsPlaying = usePlayerStore((s) => s.setIsPlaying);

  const isSelected = selectedIds.has(track.id);
  const isExpanded = expandedId === track.id;

  const [artError, setArtError] = useState(false);

  const handleClick = async (e: React.MouseEvent) => {
    try {
      const result = await invoke<SelectionChangedDto>("track_clicked", {
        id: track.id,
        shift: e.shiftKey,
        meta: e.metaKey || e.ctrlKey,
      });
      setSelectedIds(new Set(result.selected_ids));
      setTagItems(result.tag_items);
    } catch (err) {
      console.error("track_clicked failed:", err);
    }
  };

  const handleDoubleClick = async (e: React.MouseEvent) => {
    e.preventDefault();
    try {
      await invoke("play_track", { track_id: track.id });
      setCurrentTrackId(track.id);
      setIsPlaying(true);
    } catch (err) {
      console.error("play_track failed:", err);
    }
  };

  const handleExpandToggle = (e: React.MouseEvent) => {
    e.stopPropagation();
    setExpandedId(isExpanded ? null : track.id);
  };

  const durationFormatted = track.duration_secs > 0
    ? `${Math.floor(track.duration_secs / 60)}:${String(Math.floor(track.duration_secs) % 60).padStart(2, "0")}`
    : "";

  return (
    <>
      <div
        className={cx(
          styles.row,
          isSelected && styles.selected,
          isExpanded && styles.expanded
        )}
        onClick={handleClick}
        onDoubleClick={handleDoubleClick}
      >
        {!artError ? (
          <img
            className={styles.art}
            src={`art://localhost/${track.id}`}
            alt=""
            onError={() => setArtError(true)}
            draggable={false}
          />
        ) : (
          <div className={styles.artPlaceholder}>♪</div>
        )}

        <div className={styles.info}>
          <div className={styles.title}>{track.title || track.artist || "Unknown"}</div>
          <div className={styles.artist}>{track.artist}</div>
        </div>

        <span className={styles.duration}>{durationFormatted}</span>

        <span className={styles.bpm}>
          {track.bpm != null ? `${Math.round(track.bpm)} bpm` : ""}
        </span>

        <button
          title={isExpanded ? "Collapse" : "Expand"}
          onClick={handleExpandToggle}
          style={{ color: "var(--overlay0)", fontSize: 12, padding: "0 4px" }}
        >
          {isExpanded ? "▲" : "▼"}
        </button>
      </div>

      {isExpanded && (
        <ExpandedTrackRow trackId={track.id} durationSecs={track.duration_secs} />
      )}
    </>
  );
}
