import { NowPlaying } from "./NowPlaying";
import { Waveform } from "./Waveform";
import styles from "./PlayerPanel.module.scss";

export function PlayerPanel() {
  return (
    <div className={styles.panel}>
      <NowPlaying />
      <Waveform />
    </div>
  );
}
