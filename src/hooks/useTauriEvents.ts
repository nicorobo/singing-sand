import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { usePlayerStore } from "../stores/playerStore";
import { useUiStore } from "../stores/uiStore";
import { useSidebarStore } from "../stores/sidebarStore";
import { DirTreeItemDto, PlaylistDto, TagDto } from "../types";

export function useTauriEvents(onLibraryChanged: () => void) {
  const setPosition = usePlayerStore((s) => s.setPosition);
  const setDuration = usePlayerStore((s) => s.setDuration);
  const setIsPlaying = usePlayerStore((s) => s.setIsPlaying);
  const setCurrentTrack = usePlayerStore((s) => s.setCurrentTrack);
  const setPendingAnalysisCount = useUiStore((s) => s.setPendingAnalysisCount);
  const setDuplicateMessage = useUiStore((s) => s.setDuplicateMessage);
  const setDirs = useSidebarStore((s) => s.setDirs);
  const setPlaylists = useSidebarStore((s) => s.setPlaylists);
  const setTags = useSidebarStore((s) => s.setTags);

  useEffect(() => {
    const unlisten: Array<() => void> = [];

    listen<{ position: number; duration: number }>("position-changed", (e) => {
      setPosition(e.payload.position);
      setDuration(e.payload.duration);
    }).then((fn) => unlisten.push(fn));

    listen<Record<string, never>>("track-finished", () => {
      setIsPlaying(false);
    }).then((fn) => unlisten.push(fn));

    listen<{ track_id: number; duration: number; title: string; artist: string }>(
      "track-loaded",
      (e) => {
        setCurrentTrack(e.payload.track_id, e.payload.title, e.payload.artist);
        setDuration(e.payload.duration);
        setIsPlaying(true);
      }
    ).then((fn) => unlisten.push(fn));

    listen<{ pending_count: number }>("analysis-progress", (e) => {
      setPendingAnalysisCount(e.payload.pending_count);
    }).then((fn) => unlisten.push(fn));

    listen<DirTreeItemDto[]>("dir-tree-updated", (e) => {
      setDirs(e.payload);
    }).then((fn) => unlisten.push(fn));

    listen<PlaylistDto[]>("sidebar-playlists-updated", (e) => {
      setPlaylists(e.payload);
    }).then((fn) => unlisten.push(fn));

    listen<TagDto[]>("sidebar-tags-updated", (e) => {
      setTags(e.payload);
    }).then((fn) => unlisten.push(fn));

    listen<Record<string, never>>("library-changed", () => {
      onLibraryChanged();
    }).then((fn) => unlisten.push(fn));

    listen<{ message: string }>("dir-duplicate", (e) => {
      setDuplicateMessage(e.payload.message);
    }).then((fn) => unlisten.push(fn));

    return () => {
      unlisten.forEach((fn) => fn());
    };
  }, [setPosition, setDuration, setIsPlaying, setCurrentTrack,
      setPendingAnalysisCount, setDuplicateMessage, setDirs, setPlaylists, setTags, onLibraryChanged]);
}
