import { useRef } from "react";
import { useDraggable } from "@dnd-kit/core";
import { SortableContext, verticalListSortingStrategy, useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useLibraryStore } from "../../stores/libraryStore";
import { useSidebarStore } from "../../stores/sidebarStore";
import { TrackDto } from "../../types";
import { TrackRow } from "./TrackRow";
import styles from "./TrackList.module.scss";

// ─── Draggable row (library / all-tracks view) ────────────────────────────────

interface RowProps {
  track: TrackDto;
}

function DraggableRow({ track }: RowProps) {
  const selectedIds = useLibraryStore((s) => s.selectedIds);
  const dragIds = selectedIds.has(track.id) ? Array.from(selectedIds) : [track.id];

  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({
    id: `track-${track.id}`,
    data: { type: "track", trackIds: dragIds, trackTitle: track.title || track.artist },
  });

  return (
    <div
      ref={setNodeRef}
      style={{ opacity: isDragging ? 0.4 : 1 }}
      {...attributes}
      {...listeners}
    >
      <TrackRow track={track} />
    </div>
  );
}

// ─── Sortable row (playlist view) ─────────────────────────────────────────────

function SortableRow({ track }: RowProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: track.id,
    data: { type: "sortable", trackId: track.id, trackTitle: track.title || track.artist },
  });

  return (
    <div
      ref={setNodeRef}
      style={{
        transform: CSS.Transform.toString(transform),
        transition,
        opacity: isDragging ? 0 : 1,
      }}
    >
      <TrackRow
        track={track}
        isReorderable
        dragHandleListeners={listeners}
        dragHandleAttributes={attributes}
      />
    </div>
  );
}

// ─── TrackList ────────────────────────────────────────────────────────────────

export function TrackList() {
  const tracks = useLibraryStore((s) => s.tracks);
  const expandedId = useLibraryStore((s) => s.expandedId);
  const nav = useSidebarStore((s) => s.nav);
  const parentRef = useRef<HTMLDivElement>(null);

  const isPlaylistNav = nav.type === "playlist";

  const rowVirtualizer = useVirtualizer({
    count: tracks.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (i) => (tracks[i]?.id === expandedId ? 240 : 40),
    measureElement:
      typeof window !== "undefined"
        ? (el) => el.getBoundingClientRect().height
        : undefined,
    overscan: 8,
  });

  if (tracks.length === 0) {
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
        {isPlaylistNav ? (
          <SortableContext
            items={tracks.map((t) => t.id)}
            strategy={verticalListSortingStrategy}
          >
            {rowVirtualizer.getVirtualItems().map((vItem) => {
              const track = tracks[vItem.index];
              return (
                <div
                  key={vItem.key}
                  data-index={vItem.index}
                  ref={rowVirtualizer.measureElement}
                  className={styles.virtualItem}
                  style={{ transform: `translateY(${vItem.start}px)` }}
                >
                  <SortableRow track={track} />
                </div>
              );
            })}
          </SortableContext>
        ) : (
          rowVirtualizer.getVirtualItems().map((vItem) => {
            const track = tracks[vItem.index];
            return (
              <div
                key={vItem.key}
                data-index={vItem.index}
                ref={rowVirtualizer.measureElement}
                className={styles.virtualItem}
                style={{ transform: `translateY(${vItem.start}px)` }}
              >
                <DraggableRow track={track} />
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

