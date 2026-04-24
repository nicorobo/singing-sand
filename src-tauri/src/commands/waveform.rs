use ss_audio::analyze_track;
use ss_waveform::{render_to_pixels, ViewPort, WaveformBucket};
use tauri::{ipc::Response, State};

use crate::state::AppState;

#[tauri::command]
pub async fn get_waveform(
    track_id: i64,
    width: u32,
    height: u32,
    state: State<'_, AppState>,
) -> Result<Response, String> {
    let bands: Vec<[f32; 3]> = match state.db.get_waveform_bands(track_id).await.map_err(|e| e.to_string())? {
        Some(b) => b,
        None => {
            let path = state
                .db
                .get_track(track_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("track not found")?
                .path;
            let result = tokio::task::spawn_blocking(move || analyze_track(&path, 1000))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            result.waveform.iter().map(|b| b.to_array()).collect()
        }
    };

    let buckets: Vec<WaveformBucket> = bands.iter().map(|&a| WaveformBucket::from_array(a)).collect();
    let settings = state.render_settings.lock().unwrap().clone();
    let vp = ViewPort { width, height, start_pct: 0.0, end_pct: 1.0 };
    let rgb = render_to_pixels(&buckets, &settings, vp);

    let png = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let img = image::RgbImage::from_raw(width, height, rgb)
            .ok_or("invalid pixel buffer dimensions")?;
        let mut cursor = std::io::Cursor::new(Vec::new());
        img.write_to(&mut cursor, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        Ok(cursor.into_inner())
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(Response::new(png))
}
