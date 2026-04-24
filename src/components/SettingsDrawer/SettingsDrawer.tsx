import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { usePlayerStore } from "../../stores/playerStore";
import { WaveformSettingsDto } from "../../types";
import styles from "./SettingsDrawer.module.scss";

const DEFAULTS: WaveformSettingsDto = {
  amplitude_scale: 1.0,
  low_gain: 1.0,
  mid_gain: 1.0,
  high_gain: 1.0,
  display_style: "mirrored",
  color_scheme: "additive",
  normalize_mode: "none",
  gamma: 1.0,
  noise_floor: 0.0,
  smoothing: 0,
};

interface SliderRowProps {
  label: string;
  settingKey: string;
  value: number;
  min: number;
  max: number;
  step: number;
  isInt?: boolean;
  onChange: (key: string, value: string) => void;
}

function SliderRow({ label, settingKey, value, min, max, step, isInt, onChange }: SliderRowProps) {
  return (
    <div className={styles.row}>
      <label className={styles.label}>{label}</label>
      <input
        type="range"
        className={styles.slider}
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => {
          const raw = isInt ? String(Math.round(parseFloat(e.target.value))) : e.target.value;
          onChange(settingKey, raw);
        }}
      />
      <span className={styles.value}>
        {isInt ? Math.round(value) : value.toFixed(step < 0.1 ? 2 : 1)}
      </span>
    </div>
  );
}

interface SelectRowProps {
  label: string;
  settingKey: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (key: string, value: string) => void;
}

function SelectRow({ label, settingKey, value, options, onChange }: SelectRowProps) {
  return (
    <div className={styles.row}>
      <label className={styles.label}>{label}</label>
      <select
        className={styles.select}
        value={value}
        onChange={(e) => onChange(settingKey, e.target.value)}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>{o.label}</option>
        ))}
      </select>
    </div>
  );
}

export function SettingsDrawer() {
  const settingsOpen = usePlayerStore((s) => s.settingsOpen);
  const setSettingsOpen = usePlayerStore((s) => s.setSettingsOpen);

  const [settings, setSettings] = useState<WaveformSettingsDto>(DEFAULTS);
  const debounceRef = useRef<Record<string, ReturnType<typeof setTimeout>>>({});

  useEffect(() => {
    if (settingsOpen) {
      invoke<WaveformSettingsDto>("get_settings").then(setSettings).catch(console.error);
    }
  }, [settingsOpen]);

  const handleChange = useCallback((key: string, value: string) => {
    // Optimistic local update
    setSettings((prev) => ({
      ...prev,
      [key]: key === "display_style" || key === "color_scheme" || key === "normalize_mode"
        ? value
        : key === "smoothing"
        ? parseInt(value, 10)
        : parseFloat(value),
    }));

    // Debounce backend call slightly (avoid flooding on fast slider drags)
    if (debounceRef.current[key]) clearTimeout(debounceRef.current[key]);
    debounceRef.current[key] = setTimeout(() => {
      invoke("update_waveform_setting", { key, value }).catch(console.error);
    }, 40);
  }, []);

  if (!settingsOpen) return null;

  return (
    <div className={styles.drawer}>
      <div className={styles.header}>
        <span>Waveform Settings</span>
        <button className={styles.close} onClick={() => setSettingsOpen(false)} title="Close">×</button>
      </div>

      <div className={styles.body}>
        <div className={styles.group}>
          <div className={styles.groupLabel}>Display</div>
          <SelectRow
            label="Style"
            settingKey="display_style"
            value={settings.display_style}
            options={[
              { value: "mirrored", label: "Mirrored" },
              { value: "tophalf", label: "Top Half" },
            ]}
            onChange={handleChange}
          />
          <SelectRow
            label="Color Scheme"
            settingKey="color_scheme"
            value={settings.color_scheme}
            options={[
              { value: "additive", label: "Additive (Peach/Blue/Lavender)" },
              { value: "monochrome", label: "Monochrome" },
              { value: "perband", label: "Per-Band Solid" },
              { value: "grayscale", label: "Grayscale" },
            ]}
            onChange={handleChange}
          />
          <SelectRow
            label="Normalize"
            settingKey="normalize_mode"
            value={settings.normalize_mode}
            options={[
              { value: "none", label: "None" },
              { value: "perband", label: "Per Band" },
              { value: "global", label: "Global" },
            ]}
            onChange={handleChange}
          />
        </div>

        <div className={styles.group}>
          <div className={styles.groupLabel}>Amplitude</div>
          <SliderRow
            label="Scale"
            settingKey="amplitude_scale"
            value={settings.amplitude_scale}
            min={0.1} max={5.0} step={0.1}
            onChange={handleChange}
          />
          <SliderRow
            label="Gamma"
            settingKey="gamma"
            value={settings.gamma}
            min={0.1} max={3.0} step={0.1}
            onChange={handleChange}
          />
          <SliderRow
            label="Noise Floor"
            settingKey="noise_floor"
            value={settings.noise_floor}
            min={0} max={0.5} step={0.01}
            onChange={handleChange}
          />
          <SliderRow
            label="Smoothing"
            settingKey="smoothing"
            value={settings.smoothing}
            min={0} max={20} step={1}
            isInt
            onChange={handleChange}
          />
        </div>

        <div className={styles.group}>
          <div className={styles.groupLabel}>Band Gains</div>
          <SliderRow
            label="Low"
            settingKey="low_gain"
            value={settings.low_gain}
            min={0} max={3.0} step={0.1}
            onChange={handleChange}
          />
          <SliderRow
            label="Mid"
            settingKey="mid_gain"
            value={settings.mid_gain}
            min={0} max={3.0} step={0.1}
            onChange={handleChange}
          />
          <SliderRow
            label="High"
            settingKey="high_gain"
            value={settings.high_gain}
            min={0} max={3.0} step={0.1}
            onChange={handleChange}
          />
        </div>
      </div>
    </div>
  );
}
