import { invoke } from "@tauri-apps/api/core";
import { useLibraryStore } from "../../stores/libraryStore";
import { SelectedTagDto } from "../../types";
import styles from "./TagAssignmentPanel.module.scss";

function cx(...classes: (string | false | undefined)[]) {
  return classes.filter(Boolean).join(" ");
}

export function TagAssignmentPanel() {
  const selectedIds = useLibraryStore((s) => s.selectedIds);
  const tagItems = useLibraryStore((s) => s.tagItems);
  const setTagItems = useLibraryStore((s) => s.setTagItems);

  if (selectedIds.size === 0 || tagItems.length === 0) return null;

  const handleToggle = async (tag: SelectedTagDto) => {
    try {
      const updated = await invoke<SelectedTagDto[]>("toggle_tag_for_selection", {
        tag_id: tag.id,
        sel_ids: Array.from(selectedIds),
      });
      setTagItems(updated);
    } catch (err) {
      console.error("toggle_tag_for_selection failed:", err);
    }
  };

  return (
    <div className={styles.panel}>
      <span className={styles.label}>
        {selectedIds.size} selected
      </span>
      <div className={styles.tags}>
        {tagItems.map((tag) => (
          <button
            key={tag.id}
            className={cx(
              styles.pill,
              tag.assigned && styles.assigned,
              tag.partial && styles.partial,
            )}
            style={
              tag.assigned
                ? { backgroundColor: tag.color + "33", color: tag.color, borderColor: tag.color }
                : tag.partial
                ? { backgroundColor: tag.color + "18", color: tag.color, borderColor: tag.color + "80" }
                : { color: tag.color + "aa", borderColor: tag.color + "44" }
            }
            onClick={() => handleToggle(tag)}
            title={
              tag.assigned
                ? `Remove "${tag.name}" from selection`
                : `Add "${tag.name}" to selection`
            }
          >
            {tag.partial && <span className={styles.partialDot}>◐ </span>}
            {tag.name}
          </button>
        ))}
      </div>
    </div>
  );
}
