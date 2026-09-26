CREATE TABLE database_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    database_uuid TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    purpose TEXT NOT NULL CHECK (purpose = 'opp_local_library')
);
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY CHECK (version > 0),
    applied_at TEXT NOT NULL
);
CREATE TABLE library_sources (
    id INTEGER PRIMARY KEY,
    client TEXT NOT NULL CHECK (client IN ('stable', 'lazer')),
    root_path TEXT NOT NULL,
    last_successful_scan_at TEXT,
    completeness TEXT CHECK (completeness IN ('complete', 'partial')),
    UNIQUE (client, root_path)
);
CREATE TABLE beatmap_files (
    id INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL REFERENCES library_sources(id) ON DELETE CASCADE,
    source_key TEXT NOT NULL,
    physical_path TEXT,
    logical_path TEXT,
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    modified_ms INTEGER CHECK (modified_ms >= 0),
    content_hash TEXT,
    beatmap_md5 TEXT,
    UNIQUE (source_id, source_key)
);
CREATE INDEX idx_beatmap_files_md5 ON beatmap_files(beatmap_md5);
CREATE TABLE beatmap_metadata (
    file_id INTEGER PRIMARY KEY REFERENCES beatmap_files(id) ON DELETE CASCADE,
    beatmap_id INTEGER,
    beatmap_set_id INTEGER,
    ruleset TEXT NOT NULL CHECK (ruleset IN ('osu', 'taiko', 'fruits', 'mania')),
    title TEXT NOT NULL,
    title_unicode TEXT NOT NULL DEFAULT '',
    artist TEXT NOT NULL,
    artist_unicode TEXT NOT NULL DEFAULT '',
    creator TEXT NOT NULL,
    difficulty_name TEXT NOT NULL
);
CREATE INDEX idx_beatmap_metadata_beatmap_id ON beatmap_metadata(beatmap_id);
CREATE INDEX idx_beatmap_metadata_set_id ON beatmap_metadata(beatmap_set_id);
