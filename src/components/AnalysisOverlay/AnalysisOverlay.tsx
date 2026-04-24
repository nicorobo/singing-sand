import { useUiStore } from "../../stores/uiStore";
import styles from "./AnalysisOverlay.module.scss";

export function AnalysisOverlay() {
  const count = useUiStore((s) => s.pendingAnalysisCount);
  if (count === 0) return null;

  return (
    <div className={styles.overlay}>
      <span className={styles.spinner}>⟳</span>
      Analyzing {count} track{count !== 1 ? "s" : ""}…
    </div>
  );
}
