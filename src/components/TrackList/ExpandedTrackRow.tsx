import React, { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ExpandedTrackDto } from "../../types";
import styles from "./TrackList.module.scss";

function formatDuration(secs: number): string {
  const total = Math.floor(secs);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

interface Props {
  trackId: number;
  durationSecs: number;
}

export function ExpandedTrackRow({ trackId, durationSecs }: Props) {
  const [data, setData] = useState<ExpandedTrackDto | null>(null);
  const [notes, setNotes] = useState("");
  const debounceRef = useRef<number | null>(null);

  useEffect(() => {
    invoke<ExpandedTrackDto>("expand_track", { track_id: trackId }).then((d) => {
      setData(d);
      setNotes(d.notes);
    });
    return () => {
      if (debounceRef.current !== null) clearTimeout(debounceRef.current);
    };
  }, [trackId]);

  const handleTagRemove = async (tagId: number) => {
    const tags = await invoke<ExpandedTrackDto["tags"]>("remove_tag_from_expanded", {
      track_id: trackId,
      tag_id: tagId,
    });
    setData((d) => (d ? { ...d, tags } : d));
  };

  const handleNotesChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const text = e.target.value;
    setNotes(text);
    if (debounceRef.current !== null) clearTimeout(debounceRef.current);
    debounceRef.current = window.setTimeout(() => {
      invoke("save_notes", { track_id: trackId, text });
    }, 600);
  };

  if (!data) return null;

  return (
    <div className={styles.expandedRow}>
      <div className={styles.expandedMeta}>
        <span>{formatDuration(durationSecs)}</span>
        {data.playlists.length > 0 && (
          <span className={styles.expandedPlaylists}>
            In: {data.playlists.map((p) => p.name).join(", ")}
          </span>
        )}
      </div>

      {data.tags.length > 0 && (
        <div className={styles.expandedTags}>
          {data.tags.map((tag) => (
            <span
              key={tag.id}
              className={styles.tagChip}
              style={{ backgroundColor: tag.color + "33", color: tag.color }}
            >
              {tag.name}
              <button onClick={() => handleTagRemove(tag.id)} title="Remove tag">×</button>
            </span>
          ))}
        </div>
      )}

      <textarea
        className={styles.notes}
        value={notes}
        onChange={handleNotesChange}
        placeholder="Notes…"
        onClick={(e) => e.stopPropagation()}
      />
    </div>
  );
}
