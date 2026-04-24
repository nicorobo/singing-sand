use serde::{Deserialize, Serialize};
use ss_core::Track;
use ss_db::{Playlist, Tag};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackDto {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: f64,
    pub bpm: Option<f32>,
}

impl From<&Track> for TrackDto {
    fn from(t: &Track) -> Self {
        Self {
            id: t.id,
            title:  t.title.clone().unwrap_or_default(),
            artist: t.artist.clone().unwrap_or_default(),
            album:  t.album.clone().unwrap_or_default(),
            duration_secs: t.duration_secs.unwrap_or(0.0),
            bpm: t.bpm,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistDto {
    pub id: i64,
    pub name: String,
}

impl From<&Playlist> for PlaylistDto {
    fn from(p: &Playlist) -> Self {
        Self { id: p.id, name: p.name.clone() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagDto {
    pub id: i64,
    pub name: String,
    pub color: String,
}

impl From<&Tag> for TagDto {
    fn from(t: &Tag) -> Self {
        Self { id: t.id, name: t.name.clone(), color: t.color.clone() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirTreeItemDto {
    pub path: String,
    pub name: String,
    pub indent: u32,
    pub has_children: bool,
    pub is_expanded: bool,
    pub is_root: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandedTagItemDto {
    pub id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandedPlaylistItemDto {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandedTrackDto {
    pub tags: Vec<ExpandedTagItemDto>,
    pub playlists: Vec<ExpandedPlaylistItemDto>,
    pub notes: String,
    pub duration_formatted: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedTagDto {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub assigned: bool,
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformSettingsDto {
    pub amplitude_scale: f32,
    pub low_gain: f32,
    pub mid_gain: f32,
    pub high_gain: f32,
    pub display_style: String,
    pub color_scheme: String,
    pub normalize_mode: String,
    pub gamma: f32,
    pub noise_floor: f32,
    pub smoothing: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionChangedDto {
    pub selected_ids: Vec<i64>,
    pub tag_items: Vec<SelectedTagDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidebarDataDto {
    pub playlists: Vec<PlaylistDto>,
    pub tags: Vec<TagDto>,
    pub dir_tree: Vec<DirTreeItemDto>,
}
