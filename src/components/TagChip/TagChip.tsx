import styles from "./TagChip.module.scss";

interface Props {
  name: string;
  color: string;
  onRemove?: () => void;
}

export function TagChip({ name, color, onRemove }: Props) {
  return (
    <span
      className={styles.chip}
      style={{ backgroundColor: color + "33", color }}
    >
      {name}
      {onRemove && (
        <button
          className={styles.remove}
          onClick={(e) => { e.stopPropagation(); onRemove(); }}
          title="Remove"
        >
          ×
        </button>
      )}
    </span>
  );
}
