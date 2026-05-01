import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Tree } from "react-arborist";
import type { NodeApi, NodeRendererProps, MoveHandler, RenameHandler, TreeApi } from "react-arborist";
import { invoke } from "@tauri-apps/api/core";
import { confirm } from "@tauri-apps/plugin-dialog";
import { useSidebarStore, NavItem } from "../../stores/sidebarStore";
import { useLibraryStore } from "../../stores/libraryStore";
import { usePlayerStore } from "../../stores/playerStore";
import { DirTreeItemDto, PlaylistDto, PlaylistGroupDto, SidebarPlaylistDataDto, TagDto } from "../../types";
import { useNavigation } from "../../hooks/useNavigation";
import styles from "./Sidebar.module.scss";

function cx(...classes: (string | false | undefined)[]) {
  return classes.filter(Boolean).join(" ");
}

const ROW_HEIGHT = 26;

// ─── DirTree ──────────────────────────────────────────────────────────────────

interface DirTreeNode {
  id: string;
  name: string;
  path: string;
  isRoot: boolean;
  hasChildren: boolean;
  children?: DirTreeNode[];
}

function buildDirTree(items: DirTreeItemDto[]): DirTreeNode[] {
  const roots: DirTreeNode[] = [];
  const stack: DirTreeNode[] = [];
  for (const item of items) {
    const node: DirTreeNode = {
      id: item.path,
      name: item.name,
      path: item.path,
      isRoot: item.is_root,
      hasChildren: item.has_children,
      children: item.has_children ? [] : undefined,
    };
    if (item.indent === 0) {
      roots.push(node);
      stack.length = 0;
      stack.push(node);
    } else {
      while (stack.length > item.indent) stack.pop();
      const parent = stack[stack.length - 1];
      if (parent) (parent.children ??= []).push(node);
      stack.push(node);
    }
  }
  return roots;
}

function DirNode({ node, style }: NodeRendererProps<DirTreeNode>) {
  const nav = useSidebarStore((s) => s.nav);
  const setNav = useSidebarStore((s) => s.setNav);
  const { fetchTracks } = useNavigation();

  const isActive = nav.type === "dir" && nav.path === node.data.path;

  const handleClick = async (e: React.MouseEvent) => {
    e.stopPropagation();
    const next: NavItem = { type: "dir", path: node.data.path };
    setNav(next);
    await fetchTracks(next);
  };

  const handleToggle = (e: React.MouseEvent) => {
    e.stopPropagation();
    node.toggle();
  };

  const handleRemove = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (await confirm(`Remove directory from library?\n${node.data.path}`)) {
      await invoke("remove_scanned_dir", { path: node.data.path });
    }
  };

  return (
    <div
      style={style}
      className={cx(styles.dirItem, isActive && styles.active)}
      onClick={handleClick}
    >
      <span className={styles.dirToggle}>
        {node.data.hasChildren ? (
          <button onClick={handleToggle} title={node.isOpen ? "Collapse" : "Expand"}>
            {node.isOpen ? "▾" : "▸"}
          </button>
        ) : null}
      </span>
      <span className={styles.dirName} title={node.data.path}>{node.data.name}</span>
      {node.data.isRoot && (
        <button
          className={styles.removeDir}
          onClick={handleRemove}
          title="Remove directory"
        >
          ×
        </button>
      )}
    </div>
  );
}

function DirTree() {
  const dirs = useSidebarStore((s) => s.dirs);
  const treeRef = useRef<TreeApi<DirTreeNode>>(null);
  const [treeHeight, setTreeHeight] = useState(ROW_HEIGHT);

  const treeData = useMemo(() => buildDirTree(dirs), [dirs]);

  // Compute initial open state from backend on first mount only.
  // After mount arborist manages open/close state locally.
  const initialOpenState = useMemo(() => {
    const o: Record<string, boolean> = {};
    dirs.forEach((d) => { if (d.is_expanded) o[d.path] = true; });
    return o;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const refreshHeight = useCallback(() => {
    if (treeRef.current) {
      setTreeHeight(Math.max(treeRef.current.visibleNodes.length * ROW_HEIGHT, ROW_HEIGHT));
    }
  }, []);

  useEffect(() => { setTimeout(refreshHeight, 0); }, [treeData, refreshHeight]);

  const handleToggle = useCallback(async (id: string) => {
    await invoke("toggle_dir_expanded", { path: id });
    setTimeout(refreshHeight, 0);
  }, [refreshHeight]);

  if (dirs.length === 0) {
    return <p className={styles.emptyHint}>No directories added</p>;
  }

  return (
    <Tree
      ref={treeRef}
      data={treeData}
      initialOpenState={initialOpenState}
      openByDefault={false}
      onToggle={handleToggle}
      disableDrag
      disableDrop
      disableEdit
      disableMultiSelection
      idAccessor="id"
      childrenAccessor="children"
      height={treeHeight}
      rowHeight={ROW_HEIGHT}
      indent={12}
    >
      {DirNode}
    </Tree>
  );
}

// ─── PlaylistTree ─────────────────────────────────────────────────────────────

interface PlaylistTreeNode {
  id: string;
  name: string;
  isGroup: boolean;
  playlistId?: number;
  groupId?: number;
  children?: PlaylistTreeNode[];
}

function buildPlaylistTree(
  playlists: PlaylistDto[],
  groups: PlaylistGroupDto[],
): PlaylistTreeNode[] {
  const groupNodes = new Map<number, PlaylistTreeNode>();

  // Create group nodes
  for (const g of groups) {
    groupNodes.set(g.id, {
      id: `g-${g.id}`,
      name: g.name,
      isGroup: true,
      groupId: g.id,
      children: [],
    });
  }

  // Create playlist nodes
  const playlistNodes: PlaylistTreeNode[] = playlists.map((p) => ({
    id: `p-${p.id}`,
    name: p.name,
    isGroup: false,
    playlistId: p.id,
  }));

  const roots: PlaylistTreeNode[] = [];

  // Place groups into their parents (sorted by position)
  const sortedGroups = [...groups].sort((a, b) => a.position - b.position);
  for (const g of sortedGroups) {
    const node = groupNodes.get(g.id)!;
    if (g.parent_id === null) {
      roots.push(node);
    } else {
      const parent = groupNodes.get(g.parent_id);
      if (parent) (parent.children ??= []).push(node);
    }
  }

  // Place playlists into their groups (sorted by position)
  const sortedPlaylists = [...playlists].sort((a, b) => a.position - b.position);
  for (const p of sortedPlaylists) {
    const node = playlistNodes.find((n) => n.playlistId === p.id)!;
    if (p.group_id === null) {
      roots.push(node);
    } else {
      const parent = groupNodes.get(p.group_id);
      if (parent) (parent.children ??= []).push(node);
    }
  }

  // Sort roots by position
  roots.sort((a, b) => {
    const ap = a.isGroup
      ? (groups.find((g) => g.id === a.groupId)?.position ?? 0)
      : (playlists.find((p) => p.id === a.playlistId)?.position ?? 0);
    const bp = b.isGroup
      ? (groups.find((g) => g.id === b.groupId)?.position ?? 0)
      : (playlists.find((p) => p.id === b.playlistId)?.position ?? 0);
    return ap - bp;
  });

  return roots;
}

interface PlaylistNodeProps extends NodeRendererProps<PlaylistTreeNode> {
  dropHighlightId: number | null;
  onDelete: (node: NodeApi<PlaylistTreeNode>) => void;
}

function PlaylistNode({ node, style, dragHandle, dropHighlightId, onDelete }: PlaylistNodeProps) {
  const nav = useSidebarStore((s) => s.nav);
  const setNav = useSidebarStore((s) => s.setNav);
  const { fetchTracks } = useNavigation();

  const isActive =
    (node.data.isGroup && nav.type === "group" && nav.id === node.data.groupId) ||
    (!node.data.isGroup && nav.type === "playlist" && nav.id === node.data.playlistId);

  const isDropTarget = !node.data.isGroup && node.data.playlistId === dropHighlightId;

  const handleClick = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (node.isEditing) return;
    if (node.data.isGroup) {
      const next: NavItem = { type: "group", id: node.data.groupId! };
      setNav(next);
      await fetchTracks(next);
    } else {
      const next: NavItem = { type: "playlist", id: node.data.playlistId! };
      setNav(next);
      await fetchTracks(next);
    }
  };

  const handleToggle = (e: React.MouseEvent) => {
    e.stopPropagation();
    node.toggle();
  };

  const handleDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    onDelete(node);
  };

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    node.edit();
  };

  return (
    <div
      style={style}
      ref={dragHandle}
      className={cx(
        node.data.isGroup ? styles.groupNode : styles.playlistItem,
        isActive && styles.active,
        isDropTarget && styles.dropTarget,
      )}
      onClick={handleClick}
      onDoubleClick={handleDoubleClick}
      {...(!node.data.isGroup && node.data.playlistId !== undefined
        ? { "data-playlist-id": node.data.playlistId }
        : {})}
    >
      {node.data.isGroup && (
        <span className={styles.dirToggle}>
          <button onClick={handleToggle} title={node.isOpen ? "Collapse" : "Expand"}>
            {node.isOpen ? "▾" : "▸"}
          </button>
        </span>
      )}
      {node.data.isGroup && <span className={styles.groupIcon}>📁</span>}
      {node.isEditing ? (
        <input
          autoFocus
          defaultValue={node.data.name}
          className={styles.inlineInput}
          onBlur={(e) => node.submit(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") node.submit((e.target as HTMLInputElement).value);
            if (e.key === "Escape") node.reset();
          }}
          onClick={(e) => e.stopPropagation()}
        />
      ) : (
        <span className={styles.playlistName}>{node.data.name}</span>
      )}
      {!node.isEditing && (
        <button
          className={styles.deletePlaylist}
          onClick={handleDelete}
          title={node.data.isGroup ? "Delete group" : "Delete playlist"}
        >
          ×
        </button>
      )}
    </div>
  );
}

function PlaylistTree({ dropHighlightId }: { dropHighlightId: number | null }) {
  const playlists = useSidebarStore((s) => s.playlists);
  const groups = useSidebarStore((s) => s.groups);
  const setPlaylists = useSidebarStore((s) => s.setPlaylists);
  const setGroups = useSidebarStore((s) => s.setGroups);
  const nav = useSidebarStore((s) => s.nav);
  const setNav = useSidebarStore((s) => s.setNav);
  const { fetchTracks } = useNavigation();

  const treeRef = useRef<TreeApi<PlaylistTreeNode>>(null);
  const [treeHeight, setTreeHeight] = useState(ROW_HEIGHT);

  const [creatingPlaylist, setCreatingPlaylist] = useState(false);
  const [creatingGroup, setCreatingGroup] = useState(false);
  const [newName, setNewName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  const treeData = useMemo(
    () => buildPlaylistTree(playlists, groups),
    [playlists, groups],
  );

  const refreshHeight = useCallback(() => {
    if (treeRef.current) {
      setTreeHeight(Math.max(treeRef.current.visibleNodes.length * ROW_HEIGHT, ROW_HEIGHT));
    }
  }, []);

  useEffect(() => { setTimeout(refreshHeight, 0); }, [treeData, refreshHeight]);

  const handleToggle = useCallback(() => {
    setTimeout(refreshHeight, 0);
  }, [refreshHeight]);

  const startCreate = (kind: "playlist" | "group") => {
    if (kind === "playlist") setCreatingPlaylist(true);
    else setCreatingGroup(true);
    setNewName("");
    setTimeout(() => inputRef.current?.focus(), 0);
  };

  const cancelCreate = () => {
    setCreatingPlaylist(false);
    setCreatingGroup(false);
    setNewName("");
  };

  const submitCreate = async () => {
    const name = newName.trim();
    if (!name) { cancelCreate(); return; }
    try {
      const cmd = creatingGroup ? "create_playlist_group" : "create_playlist";
      const args = creatingGroup ? { name, parentId: null } : { name, groupId: null };
      const updated = await invoke<SidebarPlaylistDataDto>(cmd, args);
      setPlaylists(updated.playlists);
      setGroups(updated.groups);
    } catch (err) {
      console.error("create failed:", err);
    }
    cancelCreate();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") submitCreate();
    else if (e.key === "Escape") cancelCreate();
  };

  const handleDelete = useCallback(async (node: NodeApi<PlaylistTreeNode>) => {
    const label = node.data.isGroup ? `Delete group "${node.data.name}"?` : `Delete playlist "${node.data.name}"?`;
    if (!await confirm(label)) return;
    try {
      const cmd = node.data.isGroup ? "delete_playlist_group" : "delete_playlist";
      const args = node.data.isGroup
        ? { groupId: node.data.groupId! }
        : { playlistId: node.data.playlistId! };
      const updated = await invoke<SidebarPlaylistDataDto>(cmd, args);
      setPlaylists(updated.playlists);
      setGroups(updated.groups);

      // Navigate away if current nav was deleted
      if (
        (node.data.isGroup && nav.type === "group" && nav.id === node.data.groupId) ||
        (!node.data.isGroup && nav.type === "playlist" && nav.id === node.data.playlistId)
      ) {
        const next: NavItem = { type: "all" };
        setNav(next);
        await fetchTracks(next);
      }
    } catch (err) {
      console.error("delete failed:", err);
    }
  }, [nav, setNav, fetchTracks, setPlaylists, setGroups]);

  const handleMove: MoveHandler<PlaylistTreeNode> = useCallback(async ({ dragIds, parentId, index }) => {
    if (dragIds.length === 0) return;
    const draggedId = dragIds[0];
    const isGroup = draggedId.startsWith("g-");
    const nodeId = parseInt(draggedId.slice(2), 10);
    // parentId is either null (root) or "g-{id}" (group)
    const newParentId = parentId ? parseInt(parentId.slice(2), 10) : null;

    try {
      const updated = await invoke<SidebarPlaylistDataDto>("move_playlist_node", {
        nodeType: isGroup ? "group" : "playlist",
        nodeId,
        newParentId,
        beforeIndex: index,
      });
      setPlaylists(updated.playlists);
      setGroups(updated.groups);
    } catch (err) {
      console.error("move_playlist_node failed:", err);
    }
    setTimeout(refreshHeight, 0);
  }, [setPlaylists, setGroups, refreshHeight]);

  const handleRename: RenameHandler<PlaylistTreeNode> = useCallback(async ({ id, name }) => {
    const isGroup = id.startsWith("g-");
    const nodeId = parseInt(id.slice(2), 10);
    const cmd = isGroup ? "rename_playlist_group" : "rename_playlist";
    const args = isGroup ? { groupId: nodeId, name } : { playlistId: nodeId, name };
    try {
      const updated = await invoke<SidebarPlaylistDataDto>(cmd, args);
      setPlaylists(updated.playlists);
      setGroups(updated.groups);
    } catch (err) {
      console.error(`${cmd} failed:`, err);
    }
  }, [setPlaylists, setGroups]);

  const NodeRenderer = useCallback(
    (props: NodeRendererProps<PlaylistTreeNode>) => (
      <PlaylistNode {...props} dropHighlightId={dropHighlightId} onDelete={handleDelete} />
    ),
    [dropHighlightId, handleDelete],
  );

  const isEmpty = playlists.length === 0 && groups.length === 0;

  return (
    <>
      <div className={styles.sectionHeader}>
        <span>Playlists</span>
        <div className={styles.sectionHeaderBtns}>
          <button onClick={() => startCreate("playlist")} title="New playlist">+</button>
          <button onClick={() => startCreate("group")} title="New group">⊞</button>
        </div>
      </div>

      {isEmpty && !creatingPlaylist && !creatingGroup && (
        <p className={styles.emptyHint}>No playlists</p>
      )}

      {!isEmpty && (
        <Tree
          ref={treeRef}
          data={treeData}
          onMove={handleMove}
          onRename={handleRename}
          onToggle={handleToggle}
          disableEdit
          disableMultiSelection
          disableDrop={({ parentNode }) =>
            // Disallow only when the target parent is a playlist leaf (isGroup === false).
            // The internal root node has isGroup === undefined, so root-level drops are allowed.
            parentNode.data?.isGroup === false
          }
          idAccessor="id"
          childrenAccessor="children"
          height={treeHeight}
          rowHeight={ROW_HEIGHT}
          indent={12}
        >
          {NodeRenderer}
        </Tree>
      )}

      {(creatingPlaylist || creatingGroup) && (
        <div className={styles.inlineForm}>
          <input
            ref={inputRef}
            className={styles.inlineInput}
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={handleKeyDown}
            onBlur={submitCreate}
            placeholder={creatingGroup ? "Group name…" : "Playlist name…"}
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
      const updated = await invoke<TagDto[]>("delete_tag", { tagId: tag.id });
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
        tagId: editingId,
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
  const settingsOpen = usePlayerStore((s) => s.settingsOpen);
  const setSettingsOpen = usePlayerStore((s) => s.setSettingsOpen);

  const handleAddDir = async () => {
    await invoke("add_directory");
  };

  return (
    <div className={styles.footer}>
      <button className={styles.addDirBtn} onClick={handleAddDir}>
        <span>⊕</span>
        <span>Add Directory</span>
      </button>
      <button
        className={styles.settingsBtn}
        title={settingsOpen ? "Close settings" : "Waveform settings"}
        onClick={() => setSettingsOpen(!settingsOpen)}
        style={settingsOpen ? { color: "var(--blue, #89b4fa)", background: "var(--surface0, #313244)" } : undefined}
      >
        ⚙
      </button>
    </div>
  );
}

// ─── Sidebar ─────────────────────────────────────────────────────────────────

interface SidebarProps {
  dropHighlightId: number | null;
}

export function Sidebar({ dropHighlightId }: SidebarProps) {
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
          <PlaylistTree dropHighlightId={dropHighlightId} />
        </div>

        <div className={styles.section}>
          <TagPills />
        </div>
      </nav>

      <SidebarFooter />
    </aside>
  );
}
