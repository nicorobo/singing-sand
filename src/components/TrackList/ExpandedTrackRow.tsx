import React, { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ExpandedTrackDto } from "../../types";
import { TagChip } from "../TagChip/TagChip";
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

  const handlePlaylistRemove = async (playlistId: number) => {
    await invoke("remove_from_playlist", { playlist_id: playlistId, track_id: trackId });
    setData((d) =>
      d ? { ...d, playlists: d.playlists.filter((p) => p.id !== playlistId) } : d
    );
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
        {data.playlists.map((p) => (
          <span key={p.id} className={styles.playlistBadge}>
            {p.name}
            <button
              className={styles.playlistRemoveBtn}
              onClick={() => handlePlaylistRemove(p.id)}
              title={`Remove from "${p.name}"`}
            >
              ×
            </button>
          </span>
        ))}
      </div>

      {data.tags.length > 0 && (
        <div className={styles.expandedTags}>
          {data.tags.map((tag) => (
            <TagChip
              key={tag.id}
              name={tag.name}
              color={tag.color}
              onRemove={() => handleTagRemove(tag.id)}
            />
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
