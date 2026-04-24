import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useLibraryStore } from "../../stores/libraryStore";
import { useSidebarStore } from "../../stores/sidebarStore";
import { TrackDto } from "../../types";
import { TrackRow } from "./TrackRow";
import styles from "./TrackList.module.scss";

function cx(...classes: (string | false | undefined)[]) {
  return classes.filter(Boolean).join(" ");
}

export function TrackList() {
  const tracks = useLibraryStore((s) => s.tracks);
  const expandedId = useLibraryStore((s) => s.expandedId);
  const nav = useSidebarStore((s) => s.nav);
  const parentRef = useRef<HTMLDivElement>(null);

  // Local ordered tracks for playlist reorder (mirrors store tracks, updated optimistically)
  const [localTracks, setLocalTracks] = useState<TrackDto[]>(tracks);
  useEffect(() => { setLocalTracks(tracks); }, [tracks]);

  const isPlaylistNav = nav.type === "playlist";
  const dragSourceIdx = useRef<number | null>(null);
  const [dragOverIdx, setDragOverIdx] = useState<number | null>(null);

  const displayTracks = isPlaylistNav ? localTracks : tracks;

  const rowVirtualizer = useVirtualizer({
    count: displayTracks.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (i) => (displayTracks[i]?.id === expandedId ? 240 : 40),
    measureElement:
      typeof window !== "undefined"
        ? (el) => el.getBoundingClientRect().height
        : undefined,
    overscan: 8,
  });

  const handleDragStart = (e: React.DragEvent, track: TrackDto, index: number) => {
    e.dataTransfer.setData("track-id", String(track.id));
    e.dataTransfer.effectAllowed = isPlaylistNav ? "move" : "copy";
    if (isPlaylistNav) {
      e.dataTransfer.setData("row-index", String(index));
      dragSourceIdx.current = index;
    }
  };

  const handleDragOver = (e: React.DragEvent, index: number) => {
    if (!isPlaylistNav) return;
    if (!e.dataTransfer.types.includes("row-index")) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    setDragOverIdx(index);
  };

  const handleDragLeave = () => {
    setDragOverIdx(null);
  };

  const handleDrop = async (e: React.DragEvent, toIndex: number) => {
    e.preventDefault();
    setDragOverIdx(null);
    if (!isPlaylistNav || nav.type !== "playlist") return;
    const fromIndex = dragSourceIdx.current;
    dragSourceIdx.current = null;
    if (fromIndex === null || fromIndex === toIndex) return;

    // Reorder locally
    const next = [...localTracks];
    const [moved] = next.splice(fromIndex, 1);
    next.splice(toIndex, 0, moved);
    setLocalTracks(next);

    // Persist to backend
    try {
      await invoke("reorder_playlist_tracks", {
        playlist_id: nav.id,
        new_order: next.map((t) => t.id),
      });
    } catch (err) {
      console.error("reorder_playlist_tracks failed:", err);
      setLocalTracks(tracks); // revert
    }
  };

  const handleDragEnd = () => {
    dragSourceIdx.current = null;
    setDragOverIdx(null);
  };

  if (displayTracks.length === 0) {
    return (
      <div className={styles.container}>
        <div className={styles.empty}>
          No tracks. Add a directory to get started.
        </div>
      </div>
    );
  }

  return (
    <div ref={parentRef} className={styles.container}>
      <div
        className={styles.inner}
        style={{ height: rowVirtualizer.getTotalSize() }}
      >
        {rowVirtualizer.getVirtualItems().map((vItem) => {
          const track = displayTracks[vItem.index];
          const idx = vItem.index;
          return (
            <div
              key={vItem.key}
              data-index={vItem.index}
              ref={rowVirtualizer.measureElement}
              className={cx(
                styles.virtualItem,
                dragOverIdx === idx && styles.dropLine,
              )}
              style={{ transform: `translateY(${vItem.start}px)` }}
              draggable
              onDragStart={(e) => handleDragStart(e, track, idx)}
              onDragOver={(e) => handleDragOver(e, idx)}
              onDragLeave={handleDragLeave}
              onDrop={(e) => handleDrop(e, idx)}
              onDragEnd={handleDragEnd}
            >
              <TrackRow track={track} isReorderable={isPlaylistNav} />
            </div>
          );
        })}
      </div>
    </div>
  );
}
