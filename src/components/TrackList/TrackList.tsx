import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useLibraryStore } from "../../stores/libraryStore";
import { TrackRow } from "./TrackRow";
import styles from "./TrackList.module.scss";

export function TrackList() {
  const tracks = useLibraryStore((s) => s.tracks);
  const expandedId = useLibraryStore((s) => s.expandedId);
  const parentRef = useRef<HTMLDivElement>(null);

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
              <TrackRow track={track} />
            </div>
          );
        })}
      </div>
    </div>
  );
}
