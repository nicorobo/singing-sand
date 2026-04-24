import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSidebarStore, NavItem } from "../../stores/sidebarStore";
import { useLibraryStore } from "../../stores/libraryStore";
import { DirTreeItemDto } from "../../types";
import { useNavigation } from "../../hooks/useNavigation";
import styles from "./Sidebar.module.scss";

function cx(...classes: (string | false | undefined)[]) {
  return classes.filter(Boolean).join(" ");
}

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

function PlaylistList() {
  const playlists = useSidebarStore((s) => s.playlists);
  const nav = useSidebarStore((s) => s.nav);
  const setNav = useSidebarStore((s) => s.setNav);
  const { fetchTracks } = useNavigation();

  const handleClick = async (id: number) => {
    const next: NavItem = { type: "playlist", id };
    setNav(next);
    await fetchTracks(next);
  };

  if (playlists.length === 0) {
    return <p className={styles.emptyHint}>No playlists</p>;
  }

  return (
    <>
      {playlists.map((p) => (
        <div
          key={p.id}
          className={cx(styles.playlistItem, nav.type === "playlist" && nav.id === p.id && styles.active)}
          onClick={() => handleClick(p.id)}
        >
          {p.name}
        </div>
      ))}
    </>
  );
}

function TagPills() {
  const tags = useSidebarStore((s) => s.tags);
  const nav = useSidebarStore((s) => s.nav);
  const setNav = useSidebarStore((s) => s.setNav);
  const { fetchTracks } = useNavigation();

  const handleClick = async (id: number) => {
    const next: NavItem = { type: "tag", id };
    setNav(next);
    await fetchTracks(next);
  };

  if (tags.length === 0) {
    return <p className={styles.emptyHint}>No tags</p>;
  }

  return (
    <div className={styles.tagPills}>
      {tags.map((tag) => (
        <span
          key={tag.id}
          className={cx(styles.tagPill, nav.type === "tag" && nav.id === tag.id && styles.active)}
          style={{ backgroundColor: tag.color + "33", color: tag.color }}
          onClick={() => handleClick(tag.id)}
          title={tag.name}
        >
          {tag.name}
        </span>
      ))}
    </div>
  );
}

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
          <div className={styles.sectionHeader}>
            <span>Playlists</span>
          </div>
          <PlaylistList />
        </div>

        <div className={styles.section}>
          <div className={styles.sectionHeader}>
            <span>Tags</span>
          </div>
          <TagPills />
        </div>
      </nav>

      <SidebarFooter />
    </aside>
  );
}
