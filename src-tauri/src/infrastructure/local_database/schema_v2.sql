CREATE TABLE resource_entries (
    id INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL REFERENCES library_sources(id) ON DELETE CASCADE,
    entry_key TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    UNIQUE(source_id, entry_key)
);
ALTER TABLE beatmap_files ADD COLUMN resource_entry_id INTEGER REFERENCES resource_entries(id) ON DELETE CASCADE;
CREATE UNIQUE INDEX idx_beatmap_files_resource_entry ON beatmap_files(resource_entry_id);
CREATE TABLE library_snapshots (
    client TEXT PRIMARY KEY CHECK(client IN ('stable', 'lazer')),
    source_id INTEGER NOT NULL UNIQUE REFERENCES library_sources(id) ON DELETE CASCADE,
    index_schema INTEGER NOT NULL,
    difficulty_algorithm TEXT NOT NULL,
    summary_json TEXT NOT NULL,
    diagnostics_json TEXT NOT NULL,
    revision TEXT NOT NULL,
    entry_count INTEGER NOT NULL CHECK(entry_count >= 0)
);
