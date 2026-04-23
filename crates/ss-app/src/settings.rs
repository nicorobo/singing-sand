use anyhow::Result;
use ss_db::Db;
use ss_waveform::{ColorScheme, DisplayStyle, NormalizeMode, WaveformRenderSettings};

const KEY_AMPLITUDE_SCALE: &str = "waveform.amplitude_scale";
const KEY_LOW_GAIN:        &str = "waveform.low_gain";
const KEY_MID_GAIN:        &str = "waveform.mid_gain";
const KEY_HIGH_GAIN:       &str = "waveform.high_gain";
const KEY_DISPLAY_STYLE:   &str = "waveform.display_style";
const KEY_COLOR_SCHEME:    &str = "waveform.color_scheme";
const KEY_NORMALIZE_MODE:  &str = "waveform.normalize_mode";
const KEY_GAMMA:           &str = "waveform.gamma";
const KEY_NOISE_FLOOR:     &str = "waveform.noise_floor";
const KEY_SMOOTHING:       &str = "waveform.smoothing";

pub struct AppSettings {
    pub waveform: WaveformRenderSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { waveform: WaveformRenderSettings::default() }
    }
}

pub async fn load_settings(db: &Db) -> Result<AppSettings> {
    let def = WaveformRenderSettings::default();

    let amplitude_scale = db.get_setting(KEY_AMPLITUDE_SCALE).await?
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(def.amplitude_scale);
    let low_gain = db.get_setting(KEY_LOW_GAIN).await?
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(def.low_gain);
    let mid_gain = db.get_setting(KEY_MID_GAIN).await?
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(def.mid_gain);
    let high_gain = db.get_setting(KEY_HIGH_GAIN).await?
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(def.high_gain);
    let display_style = db.get_setting(KEY_DISPLAY_STYLE).await?
        .and_then(|v| parse_display_style(&v))
        .unwrap_or(def.display_style);
    let color_scheme = db.get_setting(KEY_COLOR_SCHEME).await?
        .and_then(|v| parse_color_scheme(&v))
        .unwrap_or(def.color_scheme);
    let normalize_mode = db.get_setting(KEY_NORMALIZE_MODE).await?
        .and_then(|v| parse_normalize_mode(&v))
        .unwrap_or(def.normalize_mode);
    let gamma = db.get_setting(KEY_GAMMA).await?
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(def.gamma);
    let noise_floor = db.get_setting(KEY_NOISE_FLOOR).await?
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(def.noise_floor);
    let smoothing = db.get_setting(KEY_SMOOTHING).await?
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(def.smoothing);

    Ok(AppSettings {
        waveform: WaveformRenderSettings {
            amplitude_scale,
            low_gain,
            mid_gain,
            high_gain,
            display_style,
            color_scheme,
            normalize_mode,
            gamma,
            noise_floor,
            smoothing,
        },
    })
}

pub async fn save_settings(db: &Db, s: &AppSettings) -> Result<()> {
    let w = &s.waveform;
    db.set_setting(KEY_AMPLITUDE_SCALE, &w.amplitude_scale.to_string()).await?;
    db.set_setting(KEY_LOW_GAIN,        &w.low_gain.to_string()).await?;
    db.set_setting(KEY_MID_GAIN,        &w.mid_gain.to_string()).await?;
    db.set_setting(KEY_HIGH_GAIN,       &w.high_gain.to_string()).await?;
    db.set_setting(KEY_DISPLAY_STYLE,   display_style_to_str(w.display_style)).await?;
    db.set_setting(KEY_COLOR_SCHEME,    color_scheme_to_str(w.color_scheme)).await?;
    db.set_setting(KEY_NORMALIZE_MODE,  normalize_mode_to_str(w.normalize_mode)).await?;
    db.set_setting(KEY_GAMMA,           &w.gamma.to_string()).await?;
    db.set_setting(KEY_NOISE_FLOOR,     &w.noise_floor.to_string()).await?;
    db.set_setting(KEY_SMOOTHING,       &w.smoothing.to_string()).await?;
    Ok(())
}

fn parse_display_style(s: &str) -> Option<DisplayStyle> {
    match s {
        "mirrored" => Some(DisplayStyle::Mirrored),
        "tophalf"  => Some(DisplayStyle::TopHalf),
        _ => None,
    }
}

fn display_style_to_str(s: DisplayStyle) -> &'static str {
    match s {
        DisplayStyle::Mirrored => "mirrored",
        DisplayStyle::TopHalf  => "tophalf",
    }
}

fn parse_normalize_mode(s: &str) -> Option<NormalizeMode> {
    match s {
        "none"    => Some(NormalizeMode::None),
        "perband" => Some(NormalizeMode::PerBand),
        "global"  => Some(NormalizeMode::Global),
        _ => None,
    }
}

fn normalize_mode_to_str(m: NormalizeMode) -> &'static str {
    match m {
        NormalizeMode::None    => "none",
        NormalizeMode::PerBand => "perband",
        NormalizeMode::Global  => "global",
    }
}

fn parse_color_scheme(s: &str) -> Option<ColorScheme> {
    match s {
        "additive"    => Some(ColorScheme::AdditivePeachBlueLavender),
        "monochrome"  => Some(ColorScheme::Monochrome),
        "perband"     => Some(ColorScheme::PerBandSolid),
        "grayscale"   => Some(ColorScheme::Grayscale),
        _ => None,
    }
}

fn color_scheme_to_str(s: ColorScheme) -> &'static str {
    match s {
        ColorScheme::AdditivePeachBlueLavender => "additive",
        ColorScheme::Monochrome                => "monochrome",
        ColorScheme::PerBandSolid              => "perband",
        ColorScheme::Grayscale                 => "grayscale",
    }
}
