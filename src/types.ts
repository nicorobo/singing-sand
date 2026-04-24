export interface TrackDto {
  id: number;
  title: string;
  artist: string;
  album: string;
  duration_secs: number;
  bpm: number | null;
}

export interface PlaylistDto {
  id: number;
  name: string;
}

export interface TagDto {
  id: number;
  name: string;
  color: string;
}

export interface DirTreeItemDto {
  path: string;
  name: string;
  indent: number;
  has_children: boolean;
  is_expanded: boolean;
  is_root: boolean;
}

export interface ExpandedTagItemDto {
  id: number;
  name: string;
  color: string;
}

export interface ExpandedPlaylistItemDto {
  id: number;
  name: string;
}

export interface ExpandedTrackDto {
  tags: ExpandedTagItemDto[];
  playlists: ExpandedPlaylistItemDto[];
  notes: string;
  duration_formatted: string;
}

export interface SelectedTagDto {
  id: number;
  name: string;
  color: string;
  assigned: boolean;
  partial: boolean;
}

export interface SelectionChangedDto {
  selected_ids: number[];
  tag_items: SelectedTagDto[];
}

export interface WaveformSettingsDto {
  amplitude_scale: number;
  low_gain: number;
  mid_gain: number;
  high_gain: number;
  display_style: string;
  color_scheme: string;
  normalize_mode: string;
  gamma: number;
  noise_floor: number;
  smoothing: number;
}

export interface SidebarDataDto {
  playlists: PlaylistDto[];
  tags: TagDto[];
  dir_tree: DirTreeItemDto[];
}
