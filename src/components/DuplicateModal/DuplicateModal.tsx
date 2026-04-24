import { useUiStore } from "../../stores/uiStore";
import styles from "./DuplicateModal.module.scss";

export function DuplicateModal() {
  const message = useUiStore((s) => s.duplicateMessage);
  const setDuplicateMessage = useUiStore((s) => s.setDuplicateMessage);

  if (!message) return null;

  return (
    <div className={styles.backdrop} onClick={() => setDuplicateMessage(null)}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <div className={styles.icon}>⚠</div>
        <p className={styles.message}>{message}</p>
        <button className={styles.okBtn} onClick={() => setDuplicateMessage(null)}>
          OK
        </button>
      </div>
    </div>
  );
}
