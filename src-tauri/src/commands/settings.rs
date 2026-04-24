use tauri::State;

use crate::{
    dtos::WaveformSettingsDto,
    settings::{save_settings, AppSettings, color_scheme_to_str, display_style_to_str, normalize_mode_to_str},
    state::AppState,
};
use ss_waveform::{ColorScheme, DisplayStyle, NormalizeMode};

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<WaveformSettingsDto, String> {
    let s = state.render_settings.lock().unwrap().clone();
    Ok(WaveformSettingsDto {
        amplitude_scale: s.amplitude_scale,
        low_gain:        s.low_gain,
        mid_gain:        s.mid_gain,
        high_gain:       s.high_gain,
        display_style:   display_style_to_str(s.display_style).to_string(),
        color_scheme:    color_scheme_to_str(s.color_scheme).to_string(),
        normalize_mode:  normalize_mode_to_str(s.normalize_mode).to_string(),
        gamma:           s.gamma,
        noise_floor:     s.noise_floor,
        smoothing:       s.smoothing,
    })
}

#[tauri::command]
pub async fn update_waveform_setting(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let mut s = state.render_settings.lock().unwrap();
        match key.as_str() {
            "amplitude_scale" => s.amplitude_scale = value.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?,
            "low_gain"        => s.low_gain        = value.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?,
            "mid_gain"        => s.mid_gain        = value.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?,
            "high_gain"       => s.high_gain       = value.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?,
            "gamma"           => s.gamma           = value.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?,
            "noise_floor"     => s.noise_floor     = value.parse().map_err(|e: std::num::ParseFloatError| e.to_string())?,
            "smoothing"       => s.smoothing       = value.parse().map_err(|e: std::num::ParseIntError| e.to_string())?,
            "display_style"   => s.display_style   = match value.as_str() {
                "tophalf" => DisplayStyle::TopHalf,
                _         => DisplayStyle::Mirrored,
            },
            "color_scheme" => s.color_scheme = match value.as_str() {
                "monochrome" => ColorScheme::Monochrome,
                "perband"    => ColorScheme::PerBandSolid,
                "grayscale"  => ColorScheme::Grayscale,
                _            => ColorScheme::AdditivePeachBlueLavender,
            },
            "normalize_mode" => s.normalize_mode = match value.as_str() {
                "perband" => NormalizeMode::PerBand,
                "global"  => NormalizeMode::Global,
                _         => NormalizeMode::None,
            },
            _ => return Err(format!("unknown setting key: {key}")),
        }
    }
    // Persist async
    let db = std::sync::Arc::clone(&state.db);
    let s = state.render_settings.lock().unwrap().clone();
    tokio::spawn(async move {
        if let Err(e) = save_settings(&db, &AppSettings { waveform: s }).await {
            tracing::warn!("save_settings failed: {e}");
        }
    });
    Ok(())
}
