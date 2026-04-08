use anyhow::Result;
use ss_db::Db;
use ss_waveform::{ColorScheme, DisplayStyle, WaveformRenderSettings};

const KEY_SHOW_LOW:        &str = "waveform.show_low";
const KEY_SHOW_MID:        &str = "waveform.show_mid";
const KEY_SHOW_HIGH:       &str = "waveform.show_high";
const KEY_AMPLITUDE_SCALE: &str = "waveform.amplitude_scale";
const KEY_LOW_GAIN:        &str = "waveform.low_gain";
const KEY_MID_GAIN:        &str = "waveform.mid_gain";
const KEY_HIGH_GAIN:       &str = "waveform.high_gain";
const KEY_DISPLAY_STYLE:   &str = "waveform.display_style";
const KEY_COLOR_SCHEME:    &str = "waveform.color_scheme";
const KEY_NORMALIZE:       &str = "waveform.normalize";

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

    let show_low = db.get_setting(KEY_SHOW_LOW).await?
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(def.show_low);
    let show_mid = db.get_setting(KEY_SHOW_MID).await?
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(def.show_mid);
    let show_high = db.get_setting(KEY_SHOW_HIGH).await?
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(def.show_high);
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
    let normalize = db.get_setting(KEY_NORMALIZE).await?
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(def.normalize);

    Ok(AppSettings {
        waveform: WaveformRenderSettings {
            show_low,
            show_mid,
            show_high,
            amplitude_scale,
            low_gain,
            mid_gain,
            high_gain,
            display_style,
            color_scheme,
            normalize,
        },
    })
}

pub async fn save_settings(db: &Db, s: &AppSettings) -> Result<()> {
    let w = &s.waveform;
    db.set_setting(KEY_SHOW_LOW,        &w.show_low.to_string()).await?;
    db.set_setting(KEY_SHOW_MID,        &w.show_mid.to_string()).await?;
    db.set_setting(KEY_SHOW_HIGH,       &w.show_high.to_string()).await?;
    db.set_setting(KEY_AMPLITUDE_SCALE, &w.amplitude_scale.to_string()).await?;
    db.set_setting(KEY_LOW_GAIN,        &w.low_gain.to_string()).await?;
    db.set_setting(KEY_MID_GAIN,        &w.mid_gain.to_string()).await?;
    db.set_setting(KEY_HIGH_GAIN,       &w.high_gain.to_string()).await?;
    db.set_setting(KEY_DISPLAY_STYLE,   display_style_to_str(w.display_style)).await?;
    db.set_setting(KEY_COLOR_SCHEME,    color_scheme_to_str(w.color_scheme)).await?;
    db.set_setting(KEY_NORMALIZE,       &w.normalize.to_string()).await?;
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
