import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  DndContext,
  DragEndEvent,
  DragOverlay,
  DragStartEvent,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import { arrayMove } from "@dnd-kit/sortable";
import { Sidebar } from "./components/Sidebar/Sidebar";
import { SearchBar } from "./components/SearchBar/SearchBar";
import { TrackList } from "./components/TrackList/TrackList";
import { PlayerPanel } from "./components/PlayerPanel/PlayerPanel";
import { TagAssignmentPanel } from "./components/TagAssignmentPanel/TagAssignmentPanel";
import { AnalysisOverlay } from "./components/AnalysisOverlay/AnalysisOverlay";
import { SettingsDrawer } from "./components/SettingsDrawer/SettingsDrawer";
import { DuplicateModal } from "./components/DuplicateModal/DuplicateModal";
import { useSidebarStore } from "./stores/sidebarStore";
import { useLibraryStore } from "./stores/libraryStore";
import { useNavigation } from "./hooks/useNavigation";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { useAppInteractions } from "./hooks/useAppInteractions";
import { SidebarDataDto } from "./types";
import styles from "./App.module.scss";

interface ActiveDragData {
  type: string;
  trackIds?: number[];
  trackTitle?: string;
}

function DragGhost({ data }: { data: ActiveDragData }) {
  if (data.type === "track") {
    const count = data.trackIds?.length ?? 1;
    return (
      <div className={styles.dragGhost}>
        {count > 1 ? `${count} tracks` : (data.trackTitle || "Track")}
      </div>
    );
  }
  if (data.type === "sortable") {
    return <div className={styles.dragGhost}>{data.trackTitle || "Track"}</div>;
  }
  return null;
}

export default function App() {
  const setDirs = useSidebarStore((s) => s.setDirs);
  const setPlaylists = useSidebarStore((s) => s.setPlaylists);
  const setGroups = useSidebarStore((s) => s.setGroups);
  const setTags = useSidebarStore((s) => s.setTags);
  const nav = useSidebarStore((s) => s.nav);
  const tracks = useLibraryStore((s) => s.tracks);
  const setTracks = useLibraryStore((s) => s.setTracks);
  const { fetchTracks } = useNavigation();

  const [activeDrag, setActiveDrag] = useState<ActiveDragData | null>(null);
  const [dropHighlightId, setDropHighlightId] = useState<number | null>(null);
  const hoveredPlaylistIdRef = useRef<number | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } })
  );

  const onLibraryChanged = useCallback(() => {
    fetchTracks();
  }, [fetchTracks]);

  useTauriEvents(onLibraryChanged);
  useAppInteractions();

  useEffect(() => {
    invoke<SidebarDataDto>("get_sidebar_data").then((data) => {
      setDirs(data.dir_tree);
      setPlaylists(data.playlists);
      setGroups(data.groups);
      setTags(data.tags);
    });
    fetchTracks();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Track which playlist node is under the pointer during a track drag.
  // react-arborist's inner DndContext means we can't use useDroppable there,
  // so we scan data-playlist-id attributes via elementsFromPoint instead.
  useEffect(() => {
    if (!activeDrag || activeDrag.type !== "track") return;
    const rafPending = { current: false };
    const handler = (e: PointerEvent) => {
      hoveredPlaylistIdRef.current = null;
      for (const el of document.elementsFromPoint(e.clientX, e.clientY)) {
        const pid = (el as HTMLElement).dataset?.playlistId;
        if (pid) {
          hoveredPlaylistIdRef.current = parseInt(pid, 10);
          break;
        }
      }
      if (!rafPending.current) {
        rafPending.current = true;
        requestAnimationFrame(() => {
          setDropHighlightId(hoveredPlaylistIdRef.current);
          rafPending.current = false;
        });
      }
    };
    document.addEventListener("pointermove", handler);
    return () => {
      document.removeEventListener("pointermove", handler);
      hoveredPlaylistIdRef.current = null;
      setDropHighlightId(null);
    };
  }, [activeDrag]);

  const handleDragStart = (event: DragStartEvent) => {
    setActiveDrag((event.active.data.current as ActiveDragData) ?? null);
  };

  const handleDragEnd = async (event: DragEndEvent) => {
    const { active, over } = event;
    const playlistDropId = hoveredPlaylistIdRef.current;
    setActiveDrag(null);

    const activeData = active.data.current as ActiveDragData | undefined;
    const overData = over?.data.current as Record<string, unknown> | undefined;

    // Playlist track reorder (sortable within the same playlist)
    if (
      over &&
      activeData?.type === "sortable" &&
      overData?.type === "sortable" &&
      nav.type === "playlist" &&
      active.id !== over.id
    ) {
      const oldIdx = tracks.findIndex((t) => t.id === active.id);
      const newIdx = tracks.findIndex((t) => t.id === over.id);
      if (oldIdx === -1 || newIdx === -1) return;

      const original = tracks;
      const reordered = arrayMove(tracks, oldIdx, newIdx);
      setTracks(reordered);

      try {
        await invoke("reorder_playlist_tracks", {
          playlistId: nav.id,
          newOrder: reordered.map((t) => t.id),
        });
      } catch (err) {
        console.error("reorder_playlist_tracks failed:", err);
        setTracks(original);
      }
      return;
    }

    // Add tracks to playlist — detected via data-playlist-id pointermove scan
    if (activeData?.type === "track" && playlistDropId !== null) {
      const selIds = activeData.trackIds ?? [];
      if (!selIds.length) return;
      try {
        await invoke("add_selected_to_playlist", {
          playlistId: playlistDropId,
          selIds,
        });
      } catch (err) {
        console.error("add_selected_to_playlist failed:", err);
      }
    }
  };

  return (
    <DndContext sensors={sensors} onDragStart={handleDragStart} onDragEnd={handleDragEnd}>
      <div className="app">
        <Sidebar dropHighlightId={dropHighlightId} />
        <div className="main">
          <SearchBar />
          <TrackList />
          <TagAssignmentPanel />
          <PlayerPanel />
          <AnalysisOverlay />
          <SettingsDrawer />
        </div>
        <DuplicateModal />
      </div>
      <DragOverlay dropAnimation={null}>
        {activeDrag ? <DragGhost data={activeDrag} /> : null}
      </DragOverlay>
    </DndContext>
  );
}
