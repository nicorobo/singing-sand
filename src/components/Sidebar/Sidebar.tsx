import React, { useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSidebarStore, NavItem } from "../../stores/sidebarStore";
import { useLibraryStore } from "../../stores/libraryStore";
import { DirTreeItemDto, PlaylistDto, TagDto } from "../../types";
import { useNavigation } from "../../hooks/useNavigation";
import styles from "./Sidebar.module.scss";

function cx(...classes: (string | false | undefined)[]) {
  return classes.filter(Boolean).join(" ");
}

// ─── DirTree ────────────────────────────────────────────────────────────────

function DirTree() {
  const dirs = useSidebarStore((s) => s.dirs);
  const nav = useSidebarStore((s) => s.nav);
  const setNav = useSidebarStore((s) => s.setNav);
  const { fetchTracks } = useNavigation();

  const handleToggle = async (e: React.MouseEvent, item: DirTreeItemDto) => {
    e.stopPropagation();
    await invoke("toggle_dir_expanded", { path: item.path });
  };

  const handleClick = async (item: DirTreeItemDto) => {
    const next: NavItem = { type: "dir", path: item.path };
    setNav(next);
    await fetchTracks(next);
  };

  const handleRemove = async (e: React.MouseEvent, path: string) => {
    e.stopPropagation();
    if (confirm(`Remove directory from library?\n${path}`)) {
      await invoke("remove_scanned_dir", { path });
    }
  };

  if (dirs.length === 0) {
    return <p className={styles.emptyHint}>No directories added</p>;
  }

  return (
    <>
      {dirs.map((item) => {
        const isActive = nav.type === "dir" && nav.path === item.path;
        const indent = item.indent * 12;
        return (
          <div
            key={item.path}
            className={cx(styles.dirItem, isActive && styles.active)}
            style={{ paddingLeft: 8 + indent }}
            onClick={() => handleClick(item)}
          >
            <span className={styles.dirToggle}>
              {item.has_children ? (
                <button
                  onClick={(e) => handleToggle(e, item)}
                  title={item.is_expanded ? "Collapse" : "Expand"}
                >
                  {item.is_expanded ? "▾" : "▸"}
                </button>
              ) : null}
            </span>
            <span className={styles.dirName} title={item.path}>{item.name}</span>
            {item.is_root && (
              <button
                className={styles.removeDir}
                onClick={(e) => handleRemove(e, item.path)}
                title="Remove directory"
              >
                ×
              </button>
            )}
          </div>
        );
      })}
    </>
  );
}

// ─── PlaylistList ────────────────────────────────────────────────────────────

function PlaylistList() {
  const playlists = useSidebarStore((s) => s.playlists);
  const setPlaylists = useSidebarStore((s) => s.setPlaylists);
  const nav = useSidebarStore((s) => s.nav);
  const setNav = useSidebarStore((s) => s.setNav);
  const { fetchTracks } = useNavigation();

  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [dragOverId, setDragOverId] = useState<number | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleClick = async (id: number) => {
    const next: NavItem = { type: "playlist", id };
    setNav(next);
    await fetchTracks(next);
  };

  const handleDelete = async (e: React.MouseEvent, p: PlaylistDto) => {
    e.stopPropagation();
    if (!confirm(`Delete playlist "${p.name}"?`)) return;
    try {
      const updated = await invoke<PlaylistDto[]>("delete_playlist", { playlist_id: p.id });
      setPlaylists(updated);
      if (nav.type === "playlist" && nav.id === p.id) {
        const next: NavItem = { type: "all" };
        setNav(next);
        await fetchTracks(next);
      }
    } catch (err) {
      console.error("delete_playlist failed:", err);
    }
  };

  const startCreate = () => {
    setCreating(true);
    setNewName("");
    setTimeout(() => inputRef.current?.focus(), 0);
  };

  const cancelCreate = () => {
    setCreating(false);
    setNewName("");
  };

  const submitCreate = async () => {
    const name = newName.trim();
    if (!name) { cancelCreate(); return; }
    try {
      const updated = await invoke<PlaylistDto[]>("create_playlist", { name });
      setPlaylists(updated);
    } catch (err) {
      console.error("create_playlist failed:", err);
    }
    cancelCreate();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") submitCreate();
    else if (e.key === "Escape") cancelCreate();
  };

  const handleDragOver = (e: React.DragEvent, id: number) => {
    if (!e.dataTransfer.types.includes("track-id")) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    setDragOverId(id);
  };

  const handleDragLeave = () => setDragOverId(null);

  const handleDrop = async (e: React.DragEvent, playlistId: number) => {
    e.preventDefault();
    setDragOverId(null);
    const raw = e.dataTransfer.getData("track-id");
    if (!raw) return;
    const trackId = parseInt(raw, 10);
    if (isNaN(trackId)) return;
    try {
      await invoke("add_to_playlist", { playlist_id: playlistId, track_id: trackId });
    } catch (err) {
      console.error("add_to_playlist failed:", err);
    }
  };

  return (
    <>
      <div className={styles.sectionHeader}>
        <span>Playlists</span>
        <button onClick={startCreate} title="New playlist">+</button>
      </div>

      {playlists.length === 0 && !creating && (
        <p className={styles.emptyHint}>No playlists</p>
      )}

      {playlists.map((p) => (
        <div
          key={p.id}
          className={cx(
            styles.playlistItem,
            nav.type === "playlist" && nav.id === p.id && styles.active,
            dragOverId === p.id && styles.dropTarget,
          )}
          onClick={() => handleClick(p.id)}
          onDragOver={(e) => handleDragOver(e, p.id)}
          onDragLeave={handleDragLeave}
          onDrop={(e) => handleDrop(e, p.id)}
        >
          <span className={styles.playlistName}>{p.name}</span>
          <button
            className={styles.deletePlaylist}
            onClick={(e) => handleDelete(e, p)}
            title="Delete playlist"
          >
            ×
          </button>
        </div>
      ))}

      {creating && (
        <div className={styles.inlineForm}>
          <input
            ref={inputRef}
            className={styles.inlineInput}
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={handleKeyDown}
            onBlur={submitCreate}
            placeholder="Playlist name…"
          />
        </div>
      )}
    </>
  );
}

// ─── TagPills ────────────────────────────────────────────────────────────────

interface TagEditForm {
  name: string;
  color: string;
}

function TagPills() {
  const tags = useSidebarStore((s) => s.tags);
  const setTags = useSidebarStore((s) => s.setTags);
  const nav = useSidebarStore((s) => s.nav);
  const setNav = useSidebarStore((s) => s.setNav);
  const { fetchTracks } = useNavigation();

  const [creating, setCreating] = useState(false);
  const [newForm, setNewForm] = useState<TagEditForm>({ name: "", color: "#89b4fa" });
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editForm, setEditForm] = useState<TagEditForm>({ name: "", color: "#89b4fa" });
  const nameInputRef = useRef<HTMLInputElement>(null);
  const editNameRef = useRef<HTMLInputElement>(null);

  const handleClick = async (id: number) => {
    if (editingId === id) return;
    const next: NavItem = { type: "tag", id };
    setNav(next);
    await fetchTracks(next);
  };

  const handleDelete = async (e: React.MouseEvent, tag: TagDto) => {
    e.stopPropagation();
    try {
      const updated = await invoke<TagDto[]>("delete_tag", { tag_id: tag.id });
      setTags(updated);
      if (nav.type === "tag" && nav.id === tag.id) {
        const next: NavItem = { type: "all" };
        setNav(next);
        await fetchTracks(next);
      }
    } catch (err) {
      console.error("delete_tag failed:", err);
    }
  };

  const startCreate = () => {
    setCreating(true);
    setNewForm({ name: "", color: "#89b4fa" });
    setTimeout(() => nameInputRef.current?.focus(), 0);
  };

  const cancelCreate = () => {
    setCreating(false);
    setNewForm({ name: "", color: "#89b4fa" });
  };

  const submitCreate = async () => {
    const name = newForm.name.trim();
    if (!name) { cancelCreate(); return; }
    try {
      const updated = await invoke<TagDto[]>("create_tag", { name, color: newForm.color });
      setTags(updated);
    } catch (err) {
      console.error("create_tag failed:", err);
    }
    cancelCreate();
  };

  const startEdit = (e: React.MouseEvent, tag: TagDto) => {
    e.stopPropagation();
    setEditingId(tag.id);
    setEditForm({ name: tag.name, color: tag.color });
    setCreating(false);
    setTimeout(() => editNameRef.current?.focus(), 0);
  };

  const cancelEdit = () => {
    setEditingId(null);
  };

  const submitEdit = async () => {
    if (editingId === null) return;
    const name = editForm.name.trim();
    if (!name) { cancelEdit(); return; }
    try {
      const updated = await invoke<TagDto[]>("update_tag", {
        tag_id: editingId,
        name,
        color: editForm.color,
      });
      setTags(updated);
    } catch (err) {
      console.error("update_tag failed:", err);
    }
    cancelEdit();
  };

  const handleCreateKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") submitCreate();
    else if (e.key === "Escape") cancelCreate();
  };

  const handleEditKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") submitEdit();
    else if (e.key === "Escape") cancelEdit();
  };

  return (
    <>
      <div className={styles.sectionHeader}>
        <span>Tags</span>
        <button onClick={startCreate} title="New tag">+</button>
      </div>

      {tags.length === 0 && !creating && (
        <p className={styles.emptyHint}>No tags</p>
      )}

      <div className={styles.tagPills}>
        {tags.map((tag) => {
          const isActive = nav.type === "tag" && nav.id === tag.id;
          const isEditing = editingId === tag.id;

          if (isEditing) {
            return (
              <div key={tag.id} className={styles.tagEditInline}>
                <input
                  type="color"
                  className={styles.colorInput}
                  value={editForm.color}
                  onChange={(e) => setEditForm((f) => ({ ...f, color: e.target.value }))}
                />
                <input
                  ref={editNameRef}
                  className={styles.tagNameInput}
                  value={editForm.name}
                  onChange={(e) => setEditForm((f) => ({ ...f, name: e.target.value }))}
                  onKeyDown={handleEditKeyDown}
                  onBlur={submitEdit}
                />
              </div>
            );
          }

          return (
            <span
              key={tag.id}
              className={cx(styles.tagPill, isActive && styles.active)}
              style={{ backgroundColor: tag.color + "33", color: tag.color }}
              onClick={() => handleClick(tag.id)}
            >
              {tag.name}
              <button
                className={styles.tagEditBtn}
                onClick={(e) => startEdit(e, tag)}
                title="Edit tag"
              >
                ✎
              </button>
              <button
                className={styles.tagDeleteBtn}
                onClick={(e) => handleDelete(e, tag)}
                title="Delete tag"
              >
                ×
              </button>
            </span>
          );
        })}
      </div>

      {creating && (
        <div className={styles.tagCreateForm}>
          <input
            type="color"
            className={styles.colorInput}
            value={newForm.color}
            onChange={(e) => setNewForm((f) => ({ ...f, color: e.target.value }))}
          />
          <input
            ref={nameInputRef}
            className={styles.tagNameInput}
            value={newForm.name}
            onChange={(e) => setNewForm((f) => ({ ...f, name: e.target.value }))}
            onKeyDown={handleCreateKeyDown}
            onBlur={submitCreate}
            placeholder="Tag name…"
          />
        </div>
      )}
    </>
  );
}

// ─── SidebarFooter ───────────────────────────────────────────────────────────

function SidebarFooter() {
  const handleAddDir = async () => {
    await invoke("add_directory");
  };

  return (
    <div className={styles.footer}>
      <button className={styles.addDirBtn} onClick={handleAddDir}>
        <span>⊕</span>
        <span>Add Directory</span>
      </button>
      <button className={styles.settingsBtn} title="Settings (coming soon)">
        ⚙
      </button>
    </div>
  );
}

// ─── Sidebar ─────────────────────────────────────────────────────────────────

export function Sidebar() {
  const nav = useSidebarStore((s) => s.nav);
  const setNav = useSidebarStore((s) => s.setNav);
  const setExpandedId = useLibraryStore((s) => s.setExpandedId);
  const { fetchTracks } = useNavigation();

  const handleNavAll = async () => {
    const next: NavItem = { type: "all" };
    setNav(next);
    setExpandedId(null);
    await fetchTracks(next);
  };

  return (
    <aside className={styles.sidebar}>
      <nav className={styles.nav}>
        <div className={styles.section}>
          <div
            className={cx(styles.navItem, nav.type === "all" && styles.active)}
            onClick={handleNavAll}
          >
            ♪ All Tracks
          </div>
        </div>

        <div className={styles.section}>
          <div className={styles.sectionHeader}>
            <span>Directories</span>
          </div>
          <DirTree />
        </div>

        <div className={styles.section}>
          <PlaylistList />
        </div>

        <div className={styles.section}>
          <TagPills />
        </div>
      </nav>

      <SidebarFooter />
    </aside>
  );
}
