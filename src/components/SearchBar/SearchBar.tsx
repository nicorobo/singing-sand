import React, { useRef, useEffect, useCallback } from "react";
import { useLibraryStore } from "../../stores/libraryStore";
import { useNavigation } from "../../hooks/useNavigation";
import styles from "./SearchBar.module.scss";

export function SearchBar() {
  const searchQuery = useLibraryStore((s) => s.searchQuery);
  const setSearchQuery = useLibraryStore((s) => s.setSearchQuery);
  const { fetchTracks } = useNavigation();
  const debounceRef = useRef<number | null>(null);
  // Always hold the latest fetchTracks so the timeout closure never goes stale
  const fetchTracksRef = useRef(fetchTracks);
  useEffect(() => { fetchTracksRef.current = fetchTracks; }, [fetchTracks]);

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const q = e.target.value;
      setSearchQuery(q);
      if (debounceRef.current !== null) clearTimeout(debounceRef.current);
      debounceRef.current = window.setTimeout(() => {
        fetchTracksRef.current();
      }, 300);
    },
    [setSearchQuery]
  );

  const handleClear = useCallback(() => {
    setSearchQuery("");
    if (debounceRef.current !== null) clearTimeout(debounceRef.current);
    fetchTracksRef.current();
  }, [setSearchQuery]);

  useEffect(() => {
    return () => {
      if (debounceRef.current !== null) clearTimeout(debounceRef.current);
    };
  }, []);

  return (
    <div className={styles.wrapper}>
      <div className={styles.inner}>
        <span className={styles.icon}>⌕</span>
        <input
          className={styles.input}
          type="text"
          placeholder="Search tracks…"
          value={searchQuery}
          onChange={handleChange}
        />
        {searchQuery && (
          <button className={styles.clear} onClick={handleClear} title="Clear">
            ×
          </button>
        )}
      </div>
    </div>
  );
}
