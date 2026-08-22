use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    path::{Path, PathBuf},
};

use memmap2::{Mmap, MmapOptions};
use rusqlite::{Connection, OpenFlags};

use crate::{
    MANIA_ANALYZER_ALGORITHM_ID, MANIA_ANALYZER_SNAPSHOT, MANIA_ANALYZER_VERSION,
    MANIA_NORMALIZATION_VERSION, ManiaAnalyzer, ManiaBeatmapMetadata, ManiaDatasetInfo,
    ManiaFeatureRecord, ManiaGameMod, ManiaModFeatureRecord, ManiaModeFamily, ManiaNormalizer,
    ManiaPattern, ManiaQueryOptions, ManiaQueryResult, ManiaQueryTarget, RuntimeError,
    mania_index::{ManiaBucketIndex, classification_tier, distance_components, final_distance},
};

const FEATURE_HEADER: &[u8; 8] = b"ODLMAN1\0";
const FEATURE_HEADER_LEN: usize = FEATURE_HEADER.len();
const MOD_FEATURE_HEADER: &[u8; 8] = b"ODLMMV1\0";
const MOD_FEATURE_HEADER_LEN: usize = MOD_FEATURE_HEADER.len();

#[derive(Debug)]
pub struct ManiaDataset {
    feature_map: Mmap,
    record_size: usize,
    offsets: HashMap<u64, usize>,
    metadata: HashMap<u64, ManiaBeatmapMetadata>,
    checksum_ids: HashMap<String, u64>,
    index: ManiaBucketIndex,
    normalizer: ManiaNormalizer,
    analyzer: ManiaAnalyzer,
    info: ManiaDatasetInfo,
    mod_records: HashMap<(u64, ManiaGameMod), ManiaFeatureRecord>,
}

impl ManiaDataset {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(RuntimeError::invalid(
                "configured Mania similarity index directory is unavailable",
            ));
        }
        for relative in [
            "mania-metadata.sqlite",
            "mania-features-v1.bin",
            "normalizers/mania-v1.bin",
            "indexes/mania-v1.buckets",
            "indexes/mania-v1.buckets.sha256",
        ] {
            if !root.join(relative).is_file() {
                return Err(RuntimeError::invalid(format!(
                    "required Mania v1 file is missing: {relative}"
                )));
            }
        }

        let metadata_path = root.join("mania-metadata.sqlite");
        reject_sqlite_sidecars(&metadata_path)?;
        let normalizer = ManiaNormalizer::load(root).map_err(|error| match error {
            crate::ManiaNormalizeError::Incompatible(message) => {
                RuntimeError::incompatible(format!("Mania normalizer is incompatible: {message}"))
            }
            other => RuntimeError::invalid(other.to_string()),
        })?;
        let index = ManiaBucketIndex::read(root).map_err(|message| {
            if message.contains("version") {
                RuntimeError::incompatible(message)
            } else {
                RuntimeError::invalid(message)
            }
        })?;

        let feature_path = root.join("mania-features-v1.bin");
        let feature_file = File::open(&feature_path)
            .map_err(|_| RuntimeError::invalid("normalized Mania feature file is missing"))?;
        // SAFETY: ManiaDataset only accepts an immutable data directory and never writes to it.
        // The caller must not externally replace/truncate an opened dataset file, matching the
        // existing standard Dataset mmap contract.
        let feature_map = unsafe { MmapOptions::new().map(&feature_file) }
            .map_err(|_| RuntimeError::invalid("normalized Mania feature file cannot be mapped"))?;
        if feature_map.get(..FEATURE_HEADER_LEN) != Some(FEATURE_HEADER) {
            return Err(RuntimeError::invalid(
                "normalized Mania feature file has an invalid header",
            ));
        }
        let record_size = bincode::serialized_size(&ManiaFeatureRecord::default())
            .map_err(|_| RuntimeError::invalid("Mania feature record format is invalid"))?
            as usize;
        if feature_map.len() == FEATURE_HEADER_LEN
            || !(feature_map.len() - FEATURE_HEADER_LEN).is_multiple_of(record_size)
        {
            return Err(RuntimeError::invalid(
                "normalized Mania feature file has a truncated record",
            ));
        }
        let file_record_count = (feature_map.len() - FEATURE_HEADER_LEN) / record_size;

        let mod_records = read_mod_records(root)?;

        let connection = open_immutable(&metadata_path)?;
        validate_schema(&connection)?;
        validate_state(&connection, file_record_count)?;
        let (metadata, data_cutoff_at) = read_metadata(&connection)?;
        for ((beatmap_id, game_mod), record) in &mod_records {
            let metadata = metadata.get(beatmap_id).ok_or_else(|| {
                RuntimeError::invalid("Mania mod feature references unknown beatmap")
            })?;
            if *game_mod == ManiaGameMod::Nm
                || record.beatmap_id != *beatmap_id
                || record.beatmapset_id != metadata.beatmapset_id
                || record.key_count != metadata.key_count
            {
                return Err(RuntimeError::invalid(
                    "Mania mod feature metadata does not match the dataset",
                ));
            }
        }
        let checksum_ids = metadata
            .values()
            .map(|metadata| (metadata.checksum.to_ascii_lowercase(), metadata.beatmap_id))
            .collect();
        let analysis_rows = read_analysis_rows(&connection)?;
        if metadata.len() != file_record_count || analysis_rows.len() != file_record_count {
            return Err(RuntimeError::invalid(
                "Mania SQLite, feature file, and scan counts do not agree",
            ));
        }

        let total_analysis_rows = connection
            .query_row("SELECT COUNT(*) FROM mania_analyses", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|_| RuntimeError::invalid("cannot count Mania analysis rows"))?;
        if total_analysis_rows < 0 || total_analysis_rows as usize != analysis_rows.len() {
            return Err(RuntimeError::incompatible(
                "Mania SQLite contains analyses outside the v1 snapshot",
            ));
        }

        let mut offsets = HashMap::with_capacity(file_record_count);
        let mut expected_coverage = BTreeMap::new();
        let mut records_by_key_count = BTreeMap::<u8, usize>::new();
        let mut occupied_offsets = HashSet::with_capacity(file_record_count);
        for (beatmap_id, offset) in analysis_rows {
            let metadata_row = metadata.get(&beatmap_id).ok_or_else(|| {
                RuntimeError::invalid("Mania analysis references missing metadata")
            })?;
            validate_offset(offset, record_size, feature_map.len())?;
            if !occupied_offsets.insert(offset) {
                return Err(RuntimeError::invalid(
                    "multiple Mania analyses reference the same feature offset",
                ));
            }
            let record = deserialize_record(&feature_map, offset, record_size)?;
            validate_record(&record)?;
            if record.beatmap_id != beatmap_id
                || record.beatmapset_id != metadata_row.beatmapset_id
                || record.key_count != metadata_row.key_count
                || record.mode_family != metadata_row.mode_family
                || record.dominant_pattern != metadata_row.dominant_pattern
            {
                return Err(RuntimeError::invalid(format!(
                    "Mania feature record {beatmap_id} does not match SQLite metadata"
                )));
            }
            if offsets.insert(beatmap_id, offset).is_some() {
                return Err(RuntimeError::invalid("duplicate Mania analysis beatmap ID"));
            }
            *records_by_key_count.entry(record.key_count).or_default() += 1;
            expected_coverage.insert(
                beatmap_id,
                (
                    record.key_count,
                    record.difficulty_band,
                    record.beatmapset_id,
                    record.mode_family,
                    offset as u64,
                ),
            );
        }
        if occupied_offsets.len() != file_record_count
            || occupied_offsets
                .iter()
                .any(|offset| (*offset - FEATURE_HEADER_LEN) / record_size >= file_record_count)
        {
            return Err(RuntimeError::invalid(
                "Mania SQLite offsets do not cover the normalized feature file",
            ));
        }
        if records_by_key_count.keys().copied().collect::<Vec<_>>() != [4, 6, 7]
            || normalizer.cohort_sizes() != records_by_key_count
        {
            return Err(RuntimeError::invalid(
                "Mania key-count cohorts do not agree across SQLite, features, and normalizer",
            ));
        }
        if index.coverage() != expected_coverage {
            return Err(RuntimeError::invalid(
                "Mania bucket index does not exactly cover normalized records",
            ));
        }
        reject_sqlite_sidecars(&metadata_path)?;

        Ok(Self {
            feature_map,
            record_size,
            offsets,
            metadata,
            checksum_ids,
            index,
            normalizer,
            analyzer: ManiaAnalyzer::new(),
            info: ManiaDatasetInfo {
                record_count: file_record_count,
                records_by_key_count,
                analyzer_version: MANIA_ANALYZER_VERSION,
                normalization_version: MANIA_NORMALIZATION_VERSION,
                algorithm_id: MANIA_ANALYZER_ALGORITHM_ID.into(),
                data_cutoff_at,
                supports_dynamic_weighting: false,
            },
            mod_records,
        })
    }

    pub fn info(&self) -> &ManiaDatasetInfo {
        &self.info
    }

    pub fn contains(&self, beatmap_id: u64) -> bool {
        self.offsets.contains_key(&beatmap_id)
    }

    pub fn contains_mod(&self, beatmap_id: u64, game_mod: ManiaGameMod) -> bool {
        game_mod == ManiaGameMod::Nm && self.contains(beatmap_id)
            || self.mod_records.contains_key(&(beatmap_id, game_mod))
    }

    pub fn beatmap_id_for_checksum(&self, checksum: &str) -> Option<u64> {
        self.checksum_ids
            .get(&checksum.to_ascii_lowercase())
            .copied()
    }

    pub fn target_for_id(&self, beatmap_id: u64) -> Result<ManiaQueryTarget, RuntimeError> {
        self.target_for_id_with_mod(beatmap_id, ManiaGameMod::Nm)
    }

    pub fn target_for_id_with_mod(
        &self,
        beatmap_id: u64,
        game_mod: ManiaGameMod,
    ) -> Result<ManiaQueryTarget, RuntimeError> {
        let metadata = self
            .metadata
            .get(&beatmap_id)
            .cloned()
            .ok_or_else(RuntimeError::unknown)?;
        let offset = *self
            .offsets
            .get(&beatmap_id)
            .ok_or_else(RuntimeError::unknown)?;
        let record = if game_mod == ManiaGameMod::Nm {
            self.record_at_offset(offset)?
        } else {
            *self
                .mod_records
                .get(&(beatmap_id, game_mod))
                .ok_or_else(|| {
                    RuntimeError::analysis(format!(
                        "Mania {} variant {} is missing from the dataset",
                        beatmap_id,
                        game_mod.as_str()
                    ))
                })?
        };
        let mut metadata = metadata;
        metadata.mode_family = record.mode_family;
        metadata.dominant_pattern = record.dominant_pattern;
        Ok(ManiaQueryTarget {
            metadata,
            record,
            game_mod,
        })
    }

    /// Analyze an external map with the same v1 analyzer and cohort normalizer.
    ///
    /// For an online download, pass the requested BeatmapID as `source_beatmap_id` so stale or
    /// zero IDs embedded in old ranked files cannot redirect identity. For a local file, pass
    /// `None` and the embedded ID (or deterministic SHA-256 fallback) is retained.
    pub fn analyze_target(
        &self,
        bytes: &[u8],
        source_beatmap_id: Option<u64>,
    ) -> Result<ManiaQueryTarget, RuntimeError> {
        self.analyze_target_with_mod(bytes, source_beatmap_id, ManiaGameMod::Nm)
    }

    pub fn analyze_target_with_mod(
        &self,
        bytes: &[u8],
        source_beatmap_id: Option<u64>,
        game_mod: ManiaGameMod,
    ) -> Result<ManiaQueryTarget, RuntimeError> {
        let analyzed = match source_beatmap_id {
            Some(beatmap_id) => self
                .analyzer
                .analyze_bytes_with_beatmap_id_and_mod(bytes, beatmap_id, game_mod),
            None => self.analyzer.analyze_bytes_with_mod(bytes, game_mod),
        };
        let (metadata, raw) = analyzed
            .map_err(|error| RuntimeError::analysis(format!("Mania analysis failed: {error}")))?;
        let record = self
            .normalizer
            .transform(&raw)
            .map_err(|error| RuntimeError::analysis(error.to_string()))?;
        Ok(ManiaQueryTarget {
            metadata,
            record,
            game_mod,
        })
    }

    pub fn query(
        &self,
        target: &ManiaQueryTarget,
        options: &ManiaQueryOptions,
    ) -> Result<Vec<ManiaQueryResult>, RuntimeError> {
        if !(1..=150).contains(&options.result_limit) {
            return Err(RuntimeError::invalid(
                "Mania query limit must be between 1 and 150",
            ));
        }
        validate_record(&target.record)?;
        if target.metadata.beatmap_id != target.record.beatmap_id
            || target.metadata.beatmapset_id != target.record.beatmapset_id
            || target.metadata.key_count != target.record.key_count
            || target.metadata.mode_family != target.record.mode_family
            || target.metadata.dominant_pattern != target.record.dominant_pattern
        {
            return Err(RuntimeError::invalid(
                "Mania query metadata and feature record do not agree",
            ));
        }

        if options.candidate_mods.is_empty()
            || options
                .candidate_mods
                .iter()
                .any(|game_mod| !ManiaGameMod::ALL.contains(game_mod))
        {
            return Err(RuntimeError::invalid(
                "Mania candidate mod pool is empty or invalid",
            ));
        }
        let mut ranked = Vec::new();
        let mut seen = HashSet::new();
        for game_mod in options.candidate_mods.iter().copied() {
            let mut lookup = target.record;
            lookup.difficulty_band = (target.record.difficulty_band as i16
                - game_mod.difficulty_band_offset())
            .clamp(0, 9) as u8;
            for entry in self.index.candidates(&lookup, options) {
                if !seen.insert((entry.beatmap_id, game_mod)) {
                    continue;
                }
                let Ok(variant) = self.target_for_id_with_mod(entry.beatmap_id, game_mod) else {
                    // A dataset may legitimately contain NoMod records for maps
                    // whose source file was unavailable during the DT/HT build.
                    // Omit that variant instead of failing the entire query.
                    continue;
                };
                let candidate = variant.record;
                if candidate.beatmap_id != entry.beatmap_id
                    || candidate.beatmapset_id != entry.beatmapset_id
                    || candidate.key_count != target.record.key_count
                {
                    return Err(RuntimeError::invalid(format!(
                        "Mania index entry {} does not match its feature record",
                        entry.beatmap_id
                    )));
                }
                let components = distance_components(target.record, candidate);
                ranked.push((variant, components, final_distance(components)));
            }
        }
        ranked.sort_by(|left, right| {
            left.2
                .total_cmp(&right.2)
                .then_with(|| left.1.skill.total_cmp(&right.1.skill))
                .then_with(|| {
                    classification_tier(target.record, left.0.record)
                        .cmp(&classification_tier(target.record, right.0.record))
                })
                .then_with(|| left.0.record.beatmap_id.cmp(&right.0.record.beatmap_id))
                .then_with(|| (left.0.game_mod as u8).cmp(&(right.0.game_mod as u8)))
        });
        ranked.truncate(options.result_limit);
        ranked
            .into_iter()
            .map(|(variant, components, final_distance)| {
                Ok(ManiaQueryResult {
                    metadata: variant.metadata,
                    record: variant.record,
                    final_distance,
                    components,
                    game_mod: variant.game_mod,
                })
            })
            .collect()
    }

    fn record_at_offset(&self, offset: usize) -> Result<ManiaFeatureRecord, RuntimeError> {
        deserialize_record(&self.feature_map, offset, self.record_size)
    }
}

fn validate_offset(offset: usize, record_size: usize, file_len: usize) -> Result<(), RuntimeError> {
    if offset < FEATURE_HEADER_LEN
        || !(offset - FEATURE_HEADER_LEN).is_multiple_of(record_size)
        || offset
            .checked_add(record_size)
            .is_none_or(|end| end > file_len)
    {
        return Err(RuntimeError::invalid(
            "Mania SQLite contains an invalid normalized feature offset",
        ));
    }
    Ok(())
}

fn deserialize_record(
    feature_map: &[u8],
    offset: usize,
    record_size: usize,
) -> Result<ManiaFeatureRecord, RuntimeError> {
    validate_offset(offset, record_size, feature_map.len())?;
    bincode::deserialize(&feature_map[offset..offset + record_size])
        .map_err(|_| RuntimeError::invalid("normalized Mania feature record is invalid"))
}

fn read_mod_records(
    root: &Path,
) -> Result<HashMap<(u64, ManiaGameMod), ManiaFeatureRecord>, RuntimeError> {
    let path = root.join("mania-mod-features-v1.bin");
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let bytes = std::fs::read(&path)
        .map_err(|_| RuntimeError::invalid("Mania mod feature file cannot be read"))?;
    if bytes.len() < MOD_FEATURE_HEADER_LEN
        || bytes[..MOD_FEATURE_HEADER_LEN] != *MOD_FEATURE_HEADER
    {
        return Err(RuntimeError::invalid(
            "Mania mod feature file has an invalid header",
        ));
    }
    let record_size = bincode::serialized_size(&ManiaModFeatureRecord {
        beatmap_id: 1,
        game_mod: ManiaGameMod::Nm,
        record: ManiaFeatureRecord::default(),
    })
    .map_err(|_| RuntimeError::invalid("Mania mod feature record format is invalid"))?
        as usize;
    if (bytes.len() - MOD_FEATURE_HEADER_LEN) % record_size != 0 {
        return Err(RuntimeError::invalid(
            "Mania mod feature file has a truncated record",
        ));
    }
    let mut records = HashMap::new();
    for chunk in bytes[MOD_FEATURE_HEADER_LEN..].chunks_exact(record_size) {
        let entry: ManiaModFeatureRecord = bincode::deserialize(chunk)
            .map_err(|_| RuntimeError::invalid("Mania mod feature record is invalid"))?;
        if entry.beatmap_id == 0 || entry.game_mod == ManiaGameMod::Nm {
            return Err(RuntimeError::invalid(
                "Mania mod feature record has an invalid mod",
            ));
        }
        validate_record(&entry.record)?;
        if entry.record.beatmap_id != entry.beatmap_id
            || entry.record.key_count != 4
                && entry.record.key_count != 6
                && entry.record.key_count != 7
        {
            return Err(RuntimeError::invalid(
                "Mania mod feature metadata does not match record",
            ));
        }
        if records
            .insert((entry.beatmap_id, entry.game_mod), entry.record)
            .is_some()
        {
            return Err(RuntimeError::invalid("duplicate Mania mod feature record"));
        }
    }
    Ok(records)
}

fn validate_record(record: &ManiaFeatureRecord) -> Result<(), RuntimeError> {
    if record.beatmap_id == 0
        || record.analyzer_version != MANIA_ANALYZER_VERSION
        || record.normalization_version != MANIA_NORMALIZATION_VERSION
        || !matches!(record.key_count, 4 | 6 | 7)
        || record.difficulty_band > 9
        || !(0.0..=1.0).contains(&record.difficulty_percentile)
        || record
            .searchable_vector()
            .into_iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        || [
            record.base.bpm,
            record.base.length_seconds,
            record.base.active_length_seconds,
            record.base.note_count,
            record.base.row_count,
            record.base.avg_nps,
            record.base.peak_nps,
            record.base.break_density,
            record.base.sv_change_rate,
        ]
        .into_iter()
        .any(|value| !value.is_finite() || value < 0.0)
    {
        return Err(RuntimeError::invalid(format!(
            "invalid normalized Mania record {}",
            record.beatmap_id
        )));
    }
    Ok(())
}

fn reject_sqlite_sidecars(path: &Path) -> Result<(), RuntimeError> {
    let base = path.to_string_lossy();
    for suffix in ["-wal", "-shm", "-journal"] {
        if PathBuf::from(format!("{base}{suffix}")).exists() {
            return Err(RuntimeError::invalid(format!(
                "immutable Mania metadata has an unexpected SQLite {suffix} sidecar"
            )));
        }
    }
    Ok(())
}

fn open_immutable(path: &Path) -> Result<Connection, RuntimeError> {
    let absolute = path
        .canonicalize()
        .map_err(|_| RuntimeError::invalid("Mania metadata SQLite is unavailable"))?;
    let mut normalized = absolute.to_string_lossy().replace('\\', "/");
    // `Path::canonicalize` returns a Windows verbatim path (`//?/E:/...`).
    // SQLite URI filenames use the ordinary drive/UNC spelling instead.
    if let Some(path) = normalized.strip_prefix("//?/UNC/") {
        normalized = format!("//{path}");
    } else if let Some(path) = normalized.strip_prefix("//?/") {
        normalized = path.to_owned();
    }
    let encoded = percent_encode_sqlite_path(normalized.as_bytes());
    // Use the fully-qualified `file:///E:/...` spelling on Windows. SQLite's
    // URI parser can otherwise treat the drive prefix as an authority.
    let uri = if encoded.as_bytes().get(1) == Some(&b':') {
        format!("file:///{encoded}?mode=ro&immutable=1")
    } else {
        format!("file:{encoded}?mode=ro&immutable=1")
    };
    Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| {
        RuntimeError::invalid(format!(
            "Mania metadata SQLite cannot be opened read-only: {error}"
        ))
    })
}

fn percent_encode_sqlite_path(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len());
    for byte in bytes {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b':' | b'.' | b'-' | b'_' | b'~' => {
                encoded.push(*byte as char)
            }
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn validate_schema(connection: &Connection) -> Result<(), RuntimeError> {
    let table_names = connection
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type='table' AND name LIKE 'mania_%' ORDER BY name",
        )
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| RuntimeError::invalid("cannot inspect Mania SQLite schema"))?;
    if table_names != ["mania_analyses", "mania_beatmaps", "mania_state"] {
        return Err(RuntimeError::incompatible(
            "Mania SQLite does not have the exact v1 table set",
        ));
    }

    validate_columns(
        connection,
        "mania_beatmaps",
        &[
            ("beatmap_id", "INTEGER", false, 1),
            ("beatmapset_id", "INTEGER", true, 0),
            ("checksum", "TEXT", true, 0),
            ("artist", "TEXT", true, 0),
            ("title", "TEXT", true, 0),
            ("version", "TEXT", true, 0),
            ("creator", "TEXT", true, 0),
            ("online_url", "TEXT", true, 0),
            ("key_count", "INTEGER", true, 0),
            ("mode_family", "INTEGER", true, 0),
            ("dominant_pattern", "INTEGER", true, 0),
            ("updated_at", "INTEGER", true, 0),
        ],
    )?;
    validate_columns(
        connection,
        "mania_analyses",
        &[
            ("beatmap_id", "INTEGER", true, 1),
            ("analyzer_version", "INTEGER", true, 2),
            ("normalization_version", "INTEGER", true, 0),
            ("raw_offset", "INTEGER", true, 0),
            ("normalized_offset", "INTEGER", false, 0),
            ("status", "INTEGER", true, 0),
        ],
    )?;
    validate_columns(
        connection,
        "mania_state",
        &[("key", "TEXT", false, 1), ("value", "TEXT", true, 0)],
    )
}

fn validate_columns(
    connection: &Connection,
    table: &str,
    expected: &[(&str, &str, bool, i64)],
) -> Result<(), RuntimeError> {
    let sql = format!("PRAGMA table_info({table})");
    let actual = connection
        .prepare(&sql)
        .and_then(|mut statement| {
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)? != 0,
                        row.get::<_, i64>(5)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| RuntimeError::invalid("cannot inspect Mania SQLite columns"))?;
    let expected = expected
        .iter()
        .map(|(name, ty, not_null, primary_key)| {
            (
                (*name).to_owned(),
                (*ty).to_owned(),
                *not_null,
                *primary_key,
            )
        })
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(RuntimeError::incompatible(format!(
            "Mania SQLite table {table} does not match the v1 schema"
        )));
    }
    Ok(())
}

fn state_value(connection: &Connection, key: &str) -> Result<String, RuntimeError> {
    connection
        .query_row("SELECT value FROM mania_state WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .map_err(|_| RuntimeError::invalid(format!("Mania state {key} is missing")))
}

fn validate_state(connection: &Connection, file_record_count: usize) -> Result<(), RuntimeError> {
    if state_value(connection, "analyzer_snapshot")? != MANIA_ANALYZER_SNAPSHOT {
        return Err(RuntimeError::incompatible(
            "Mania analyzer snapshot does not match Analyzer v1",
        ));
    }
    let eligible = state_value(connection, "scan_eligible")?
        .parse::<usize>()
        .map_err(|_| RuntimeError::invalid("Mania scan_eligible is invalid"))?;
    let _unsupported = state_value(connection, "scan_unsupported")?
        .parse::<usize>()
        .map_err(|_| RuntimeError::invalid("Mania scan_unsupported is invalid"))?;
    let failed = state_value(connection, "scan_failed")?
        .parse::<usize>()
        .map_err(|_| RuntimeError::invalid("Mania scan_failed is invalid"))?;
    if failed != 0 {
        return Err(RuntimeError::invalid(format!(
            "Mania source scan contains {failed} failed beatmaps"
        )));
    }
    if eligible != file_record_count {
        return Err(RuntimeError::invalid(
            "Mania scan_eligible does not match the feature file",
        ));
    }
    Ok(())
}

fn read_metadata(
    connection: &Connection,
) -> Result<(HashMap<u64, ManiaBeatmapMetadata>, Option<i64>), RuntimeError> {
    let mut statement = connection
        .prepare(
            "SELECT beatmap_id,beatmapset_id,checksum,artist,title,version,creator,
                    online_url,key_count,mode_family,dominant_pattern,updated_at
             FROM mania_beatmaps ORDER BY beatmap_id",
        )
        .map_err(|_| RuntimeError::invalid("cannot read Mania metadata"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
            ))
        })
        .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
        .map_err(|_| RuntimeError::invalid("cannot decode Mania metadata"))?;
    let mut metadata = HashMap::with_capacity(rows.len());
    let mut data_cutoff_at = None;
    for (
        beatmap_id,
        beatmapset_id,
        checksum,
        artist,
        title,
        version,
        creator,
        online_url,
        key_count,
        family,
        pattern,
        updated_at,
    ) in rows
    {
        if beatmap_id <= 0
            || beatmapset_id < 0
            || !matches!(key_count, 4 | 6 | 7)
            || !(0..=3).contains(&family)
            || !(0..=5).contains(&pattern)
            || checksum.len() != 64
            || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(RuntimeError::invalid("invalid Mania metadata row"));
        }
        let beatmap_id = beatmap_id as u64;
        let value = ManiaBeatmapMetadata {
            beatmap_id,
            beatmapset_id: beatmapset_id as u64,
            checksum,
            artist,
            title,
            version,
            creator,
            online_url,
            key_count: key_count as u8,
            mode_family: mode_family(family),
            dominant_pattern: pattern_value(pattern),
        };
        if metadata.insert(beatmap_id, value).is_some() {
            return Err(RuntimeError::invalid("duplicate Mania metadata beatmap ID"));
        }
        data_cutoff_at =
            Some(data_cutoff_at.map_or(updated_at, |value: i64| value.max(updated_at)));
    }
    Ok((metadata, data_cutoff_at))
}

fn read_analysis_rows(connection: &Connection) -> Result<Vec<(u64, usize)>, RuntimeError> {
    let mut statement = connection
        .prepare(
            "SELECT beatmap_id,normalized_offset FROM mania_analyses
             WHERE analyzer_version=1 AND normalization_version=1 AND status=2
             ORDER BY beatmap_id",
        )
        .map_err(|_| RuntimeError::invalid("cannot read normalized Mania analyses"))?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
        .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
        .map_err(|_| RuntimeError::invalid("cannot decode normalized Mania analyses"))?;
    rows.into_iter()
        .map(|(beatmap_id, offset)| {
            if beatmap_id <= 0 || offset < 0 {
                return Err(RuntimeError::invalid("invalid Mania analysis row"));
            }
            let offset = usize::try_from(offset)
                .map_err(|_| RuntimeError::invalid("Mania analysis offset is out of range"))?;
            Ok((beatmap_id as u64, offset))
        })
        .collect()
}

fn mode_family(value: i64) -> ManiaModeFamily {
    match value {
        1 => ManiaModeFamily::Hb,
        2 => ManiaModeFamily::Mix,
        3 => ManiaModeFamily::Ln,
        _ => ManiaModeFamily::Rc,
    }
}

fn pattern_value(value: i64) -> ManiaPattern {
    match value {
        1 => ManiaPattern::Chordstream,
        2 => ManiaPattern::Jacks,
        3 => ManiaPattern::Coordination,
        4 => ManiaPattern::Density,
        5 => ManiaPattern::Wildcard,
        _ => ManiaPattern::Stream,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        fs::{self, File},
        io::{Seek, Write},
        time::SystemTime,
    };

    use rusqlite::params;
    use serde::Serialize;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    use super::*;
    use crate::{
        MANIA_DIFFICULTY_DIMENSIONS, ManiaBaseFeatures, ManiaDifficultyVector,
        ManiaRawFeatureRecord, ManiaStyleVector, RuntimeErrorKind,
    };

    type SqliteSnapshot = Vec<(String, Option<(u64, Option<SystemTime>)>)>;

    #[derive(Serialize)]
    struct FixtureKeyNormalizer {
        key_count: u8,
        axes: [Vec<f32>; MANIA_DIFFICULTY_DIMENSIONS],
        overall: Vec<f32>,
    }

    #[derive(Serialize)]
    struct FixtureNormalizer {
        version: u32,
        analyzer_version: u32,
        keys: Vec<FixtureKeyNormalizer>,
    }

    #[derive(Serialize)]
    struct FixtureBucketEntry {
        beatmap_id: u64,
        beatmapset_id: u64,
        mode_family: ManiaModeFamily,
        normalized_offset: u64,
    }

    #[derive(Serialize)]
    struct FixtureBucket {
        key_count: u8,
        difficulty_band: u8,
        entries: Vec<FixtureBucketEntry>,
    }

    #[derive(Serialize)]
    struct FixtureIndex {
        normalization_version: u32,
        analyzer_version: u32,
        buckets: Vec<FixtureBucket>,
    }

    fn mania_map(id: u64, set: u64, keys: u8) -> Vec<u8> {
        let objects = (0..48)
            .map(|index| {
                let lane = index % keys as usize;
                let x = ((lane as f64 + 0.5) * 512.0 / keys as f64).floor() as usize;
                format!("{x},192,{},1,0,0:0:0:0:", index * 140)
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "osu file format v14\n\n[General]\nMode:3\n\n[Metadata]\nTitle:Map {id}\nArtist:Artist\nCreator:Mapper\nVersion:{keys}K\nBeatmapID:{id}\nBeatmapSetID:{set}\n\n[Difficulty]\nCircleSize:{keys}\nOverallDifficulty:8\n\n[TimingPoints]\n0,500,4,2,0,100,1,0\n\n[HitObjects]\n{objects}\n"
        )
        .into_bytes()
    }

    fn fixture_normalizer(root: &Path) {
        fs::create_dir_all(root.join("normalizers")).unwrap();
        let key = |key_count: u8, count: usize| FixtureKeyNormalizer {
            key_count,
            axes: std::array::from_fn(|_| (0..count).map(|value| value as f32).collect()),
            overall: if count == 1 {
                vec![0.5]
            } else {
                (0..count)
                    .map(|value| value as f32 / (count - 1) as f32)
                    .collect()
            },
        };
        let normalizer = FixtureNormalizer {
            version: 1,
            analyzer_version: 1,
            keys: vec![key(4, 4), key(6, 1), key(7, 1)],
        };
        bincode::serialize_into(
            File::create(root.join("normalizers/mania-v1.bin")).unwrap(),
            &normalizer,
        )
        .unwrap();
    }

    fn build_fixture(root: &Path) -> Vec<u8> {
        fs::create_dir_all(root.join("indexes")).unwrap();
        fs::create_dir_all(root.join("beatmaps")).unwrap();
        fixture_normalizer(root);
        let source = mania_map(10, 1, 4);
        for (id, set, keys) in [
            (10, 1, 4),
            (11, 1, 4),
            (20, 2, 4),
            (30, 3, 4),
            (60, 6, 6),
            (70, 7, 7),
        ] {
            fs::write(
                root.join("beatmaps").join(format!("{id}.osu")),
                mania_map(id, set, keys),
            )
            .unwrap();
        }
        let (source_metadata, raw) = ManiaAnalyzer::new().analyze_bytes(&source).unwrap();
        let normalizer = ManiaNormalizer::load(root).unwrap();
        let source_record = normalizer.transform(&raw).unwrap();

        let mut records = vec![source_record];
        for (id, set) in [(11, 1), (20, 2), (30, 3)] {
            let mut record = source_record;
            record.beatmap_id = id;
            record.beatmapset_id = set;
            records.push(record);
        }
        for (id, set, key_count) in [(60, 6, 6), (70, 7, 7)] {
            records.push(ManiaFeatureRecord {
                beatmap_id: id,
                beatmapset_id: set,
                difficulty: ManiaDifficultyVector::from_array([0.5; 8]),
                style: ManiaStyleVector {
                    stream: 1.0,
                    ..ManiaStyleVector::default()
                },
                base: ManiaBaseFeatures {
                    bpm: 120.0,
                    length_seconds: 30.0,
                    active_length_seconds: 30.0,
                    note_count: 100.0,
                    row_count: 100.0,
                    avg_nps: 3.0,
                    peak_nps: 5.0,
                    ..ManiaBaseFeatures::default()
                },
                difficulty_percentile: 0.5,
                difficulty_band: 5,
                key_count,
                mode_family: ManiaModeFamily::Rc,
                dominant_pattern: ManiaPattern::Stream,
                analyzer_version: 1,
                normalization_version: 1,
            });
        }

        let mut feature_file = File::create(root.join("mania-features-v1.bin")).unwrap();
        feature_file.write_all(FEATURE_HEADER).unwrap();
        let mut offsets = BTreeMap::new();
        for record in &records {
            let offset = feature_file.stream_position().unwrap();
            bincode::serialize_into(&mut feature_file, record).unwrap();
            offsets.insert(record.beatmap_id, offset);
        }
        feature_file.sync_all().unwrap();

        let mut mod_file = File::create(root.join("mania-mod-features-v1.bin")).unwrap();
        mod_file.write_all(MOD_FEATURE_HEADER).unwrap();
        for record in records.iter().copied() {
            for (game_mod, scale) in [(ManiaGameMod::Dt, 1.5_f32), (ManiaGameMod::Ht, 0.75_f32)] {
                let mut variant = record;
                variant.base.bpm *= scale;
                variant.base.avg_nps *= scale;
                bincode::serialize_into(
                    &mut mod_file,
                    &ManiaModFeatureRecord {
                        beatmap_id: variant.beatmap_id,
                        game_mod,
                        record: variant,
                    },
                )
                .unwrap();
            }
        }
        mod_file.sync_all().unwrap();

        let mut buckets = BTreeMap::<(u8, u8), Vec<FixtureBucketEntry>>::new();
        for record in &records {
            buckets
                .entry((record.key_count, record.difficulty_band))
                .or_default()
                .push(FixtureBucketEntry {
                    beatmap_id: record.beatmap_id,
                    beatmapset_id: record.beatmapset_id,
                    mode_family: record.mode_family,
                    normalized_offset: offsets[&record.beatmap_id],
                });
        }
        let index = FixtureIndex {
            normalization_version: 1,
            analyzer_version: 1,
            buckets: buckets
                .into_iter()
                .map(|((key_count, difficulty_band), entries)| FixtureBucket {
                    key_count,
                    difficulty_band,
                    entries,
                })
                .collect(),
        };
        let index_bytes = bincode::serialize(&index).unwrap();
        fs::write(root.join("indexes/mania-v1.buckets"), &index_bytes).unwrap();
        fs::write(
            root.join("indexes/mania-v1.buckets.sha256"),
            hex::encode(Sha256::digest(&index_bytes)),
        )
        .unwrap();

        let connection = Connection::open(root.join("mania-metadata.sqlite")).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE mania_beatmaps (
                   beatmap_id INTEGER PRIMARY KEY,
                   beatmapset_id INTEGER NOT NULL,
                   checksum TEXT NOT NULL,
                   artist TEXT NOT NULL,
                   title TEXT NOT NULL,
                   version TEXT NOT NULL,
                   creator TEXT NOT NULL,
                   online_url TEXT NOT NULL,
                   key_count INTEGER NOT NULL,
                   mode_family INTEGER NOT NULL,
                   dominant_pattern INTEGER NOT NULL,
                   updated_at INTEGER NOT NULL DEFAULT (unixepoch())
                 );
                 CREATE INDEX idx_mania_beatmaps_key_count ON mania_beatmaps(key_count);
                 CREATE TABLE mania_analyses (
                   beatmap_id INTEGER NOT NULL,
                   analyzer_version INTEGER NOT NULL,
                   normalization_version INTEGER NOT NULL DEFAULT 0,
                   raw_offset INTEGER NOT NULL,
                   normalized_offset INTEGER,
                   status INTEGER NOT NULL,
                   PRIMARY KEY (beatmap_id, analyzer_version),
                   FOREIGN KEY (beatmap_id) REFERENCES mania_beatmaps(beatmap_id)
                 );
                 CREATE INDEX idx_mania_analyses_normalized
                   ON mania_analyses(analyzer_version, normalization_version, status);
                 CREATE TABLE mania_state (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )
            .unwrap();
        for record in &records {
            let checksum = if record.beatmap_id == 10 {
                source_metadata.checksum.clone()
            } else {
                format!("{:064x}", record.beatmap_id)
            };
            connection
                .execute(
                    "INSERT INTO mania_beatmaps(
                       beatmap_id,beatmapset_id,checksum,artist,title,version,creator,online_url,
                       key_count,mode_family,dominant_pattern
                     ) VALUES(?1,?2,?3,'Artist','Title','Difficulty','Mapper',?4,?5,?6,?7)",
                    params![
                        record.beatmap_id as i64,
                        record.beatmapset_id as i64,
                        checksum,
                        format!("https://osu.ppy.sh/b/{}", record.beatmap_id),
                        record.key_count as i64,
                        record.mode_family as i64,
                        record.dominant_pattern as i64,
                    ],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO mania_analyses(
                       beatmap_id,analyzer_version,normalization_version,raw_offset,
                       normalized_offset,status
                     ) VALUES(?1,1,1,8,?2,2)",
                    params![record.beatmap_id as i64, offsets[&record.beatmap_id] as i64],
                )
                .unwrap();
        }
        for (key, value) in [
            ("analyzer_snapshot", MANIA_ANALYZER_SNAPSHOT.to_owned()),
            ("scan_eligible", records.len().to_string()),
            ("scan_unsupported", "0".to_owned()),
            ("scan_failed", "0".to_owned()),
        ] {
            connection
                .execute(
                    "INSERT INTO mania_state(key,value) VALUES(?1,?2)",
                    params![key, value],
                )
                .unwrap();
        }
        drop(connection);
        source
    }

    #[test]
    fn opens_queries_with_hard_key_isolation_and_stable_order() {
        let temp = tempdir().unwrap();
        let source = build_fixture(temp.path());
        let dataset = ManiaDataset::open(temp.path()).unwrap();
        assert_eq!(dataset.info().record_count, 6);
        assert_eq!(dataset.info().records_by_key_count.get(&4), Some(&4));
        assert!(!dataset.info().supports_dynamic_weighting);
        assert!(dataset.contains(10));
        assert!(!dataset.contains(999));

        let indexed = dataset.target_for_id(10).unwrap();
        let options = ManiaQueryOptions {
            result_limit: 2,
            include_same_set: false,
            ..ManiaQueryOptions::default()
        };
        let indexed_results = dataset.query(&indexed, &options).unwrap();
        assert_eq!(
            indexed_results
                .iter()
                .map(|result| result.record.beatmap_id)
                .collect::<Vec<_>>(),
            vec![20, 30]
        );
        assert!(indexed_results.iter().all(|result| {
            result.record.key_count == 4
                && result.record.beatmapset_id != indexed.record.beatmapset_id
        }));

        let external = dataset.analyze_target(&source, Some(10)).unwrap();
        assert_eq!(external.record, indexed.record);
        assert_eq!(
            dataset
                .query(&external, &options)
                .unwrap()
                .into_iter()
                .map(|result| result.record.beatmap_id)
                .collect::<Vec<_>>(),
            vec![20, 30]
        );
        let with_same_set = dataset
            .query(
                &indexed,
                &ManiaQueryOptions {
                    result_limit: 2,
                    include_same_set: true,
                    ..ManiaQueryOptions::default()
                },
            )
            .unwrap();
        assert_eq!(with_same_set[0].record.beatmap_id, 11);
    }

    #[test]
    fn authoritative_source_id_overrides_embedded_id() {
        let temp = tempdir().unwrap();
        let source = build_fixture(temp.path());
        let dataset = ManiaDataset::open(temp.path()).unwrap();
        let target = dataset.analyze_target(&source, Some(1234)).unwrap();
        assert_eq!(target.metadata.beatmap_id, 1234);
        assert_eq!(target.record.beatmap_id, 1234);
        assert_eq!(target.metadata.online_url, "https://osu.ppy.sh/b/1234");
    }

    #[test]
    fn mixed_mod_pool_returns_distinct_recomputed_variants() {
        let temp = tempdir().unwrap();
        build_fixture(temp.path());
        let dataset = ManiaDataset::open(temp.path()).unwrap();
        let target = dataset.target_for_id(10).unwrap();
        let results = dataset
            .query(
                &target,
                &ManiaQueryOptions {
                    result_limit: 6,
                    include_same_set: false,
                    candidate_mods: ManiaGameMod::ALL.to_vec(),
                },
            )
            .unwrap();
        assert_eq!(results.len(), 6);
        assert_eq!(
            results
                .iter()
                .map(|result| result.game_mod)
                .collect::<HashSet<_>>(),
            ManiaGameMod::ALL.into_iter().collect()
        );
        assert!(results.iter().any(|result| {
            result.game_mod == ManiaGameMod::Dt && result.record.base.bpm > target.record.base.bpm
        }));
        assert!(results.iter().any(|result| {
            result.game_mod == ManiaGameMod::Ht && result.record.base.bpm < target.record.base.bpm
        }));
    }

    #[test]
    fn query_limit_accepts_150_and_rejects_out_of_range_values() {
        let temp = tempdir().unwrap();
        build_fixture(temp.path());
        let dataset = ManiaDataset::open(temp.path()).unwrap();
        let target = dataset.target_for_id(10).unwrap();
        assert!(
            dataset
                .query(
                    &target,
                    &ManiaQueryOptions {
                        result_limit: 150,
                        include_same_set: false,
                        ..ManiaQueryOptions::default()
                    },
                )
                .is_ok()
        );
        for result_limit in [0, 151] {
            let error = dataset
                .query(
                    &target,
                    &ManiaQueryOptions {
                        result_limit,
                        include_same_set: false,
                        ..ManiaQueryOptions::default()
                    },
                )
                .unwrap_err();
            assert_eq!(error.kind(), RuntimeErrorKind::Invalid);
        }
    }

    #[test]
    fn opening_never_creates_sqlite_sidecars() {
        let temp = tempdir().unwrap();
        build_fixture(temp.path());
        let before = sqlite_snapshot(temp.path());
        let _dataset = ManiaDataset::open(temp.path()).unwrap();
        assert_eq!(sqlite_snapshot(temp.path()), before);
    }

    #[test]
    fn malformed_dataset_errors_are_classified() {
        let missing = tempdir().unwrap();
        build_fixture(missing.path());
        fs::remove_file(missing.path().join("indexes/mania-v1.buckets.sha256")).unwrap();
        assert_eq!(
            ManiaDataset::open(missing.path()).unwrap_err().kind(),
            RuntimeErrorKind::Invalid
        );

        let version = tempdir().unwrap();
        build_fixture(version.path());
        mutate_sqlite(
            version.path(),
            "UPDATE mania_state SET value='2:future' WHERE key='analyzer_snapshot'",
        );
        assert_eq!(
            ManiaDataset::open(version.path()).unwrap_err().kind(),
            RuntimeErrorKind::Incompatible
        );

        let checksum = tempdir().unwrap();
        build_fixture(checksum.path());
        fs::write(checksum.path().join("indexes/mania-v1.buckets"), b"corrupt").unwrap();
        assert_eq!(
            ManiaDataset::open(checksum.path()).unwrap_err().kind(),
            RuntimeErrorKind::Invalid
        );

        let offset = tempdir().unwrap();
        build_fixture(offset.path());
        mutate_sqlite(
            offset.path(),
            "UPDATE mania_analyses SET normalized_offset=9 WHERE beatmap_id=10",
        );
        assert_eq!(
            ManiaDataset::open(offset.path()).unwrap_err().kind(),
            RuntimeErrorKind::Invalid
        );

        let schema = tempdir().unwrap();
        build_fixture(schema.path());
        mutate_sqlite(
            schema.path(),
            "ALTER TABLE mania_state ADD COLUMN extra TEXT",
        );
        assert_eq!(
            ManiaDataset::open(schema.path()).unwrap_err().kind(),
            RuntimeErrorKind::Incompatible
        );

        let failed = tempdir().unwrap();
        build_fixture(failed.path());
        mutate_sqlite(
            failed.path(),
            "UPDATE mania_state SET value='1' WHERE key='scan_failed'",
        );
        assert_eq!(
            ManiaDataset::open(failed.path()).unwrap_err().kind(),
            RuntimeErrorKind::Invalid
        );
    }

    #[test]
    fn opt_in_ranked_index_is_complete_and_immutable() {
        let Some(root) = env::var_os("OPP_MANIA_SIMILARITY_INDEX").map(PathBuf::from) else {
            return;
        };
        let before = sqlite_snapshot(&root);
        let dataset = ManiaDataset::open(&root).unwrap();
        assert_eq!(dataset.info().record_count, 23_551);
        assert_eq!(
            dataset.info().records_by_key_count,
            BTreeMap::from([(4, 18_550), (6, 800), (7, 4_201)])
        );
        for key_count in [4, 6, 7] {
            let target_id = dataset
                .metadata
                .values()
                .filter(|metadata| metadata.key_count == key_count)
                .map(|metadata| metadata.beatmap_id)
                .min()
                .unwrap();
            let source_path = root.join("beatmaps").join(format!("{target_id}.osu"));
            if source_path.is_file() {
                let indexed = dataset.target_for_id(target_id).unwrap();
                let source = fs::read(source_path).unwrap();
                let external = dataset.analyze_target(&source, Some(target_id)).unwrap();
                assert_eq!(external.metadata.checksum, indexed.metadata.checksum);
                assert_eq!(external.record, indexed.record);
            }
        }
        let target_id = *dataset.offsets.keys().min().unwrap();
        let target = dataset.target_for_id(target_id).unwrap();
        let results = dataset
            .query(
                &target,
                &ManiaQueryOptions {
                    result_limit: 20,
                    include_same_set: false,
                    ..ManiaQueryOptions::default()
                },
            )
            .unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().all(|result| {
            result.record.key_count == target.record.key_count
                && (target.record.beatmapset_id == 0
                    || result.record.beatmapset_id != target.record.beatmapset_id)
        }));
        drop(dataset);
        assert_eq!(sqlite_snapshot(&root), before);
    }

    fn mutate_sqlite(root: &Path, sql: &str) {
        let connection = Connection::open(root.join("mania-metadata.sqlite")).unwrap();
        connection.execute_batch(sql).unwrap();
    }

    fn sqlite_snapshot(root: &Path) -> SqliteSnapshot {
        [
            "mania-metadata.sqlite",
            "mania-metadata.sqlite-wal",
            "mania-metadata.sqlite-shm",
            "mania-metadata.sqlite-journal",
        ]
        .into_iter()
        .map(|name| {
            let metadata = fs::metadata(root.join(name)).ok();
            (
                name.to_owned(),
                metadata.map(|metadata| (metadata.len(), metadata.modified().ok())),
            )
        })
        .collect()
    }

    // Preserve the exact raw record wire type in fixture code and ensure it remains linked.
    #[test]
    fn raw_wire_type_remains_v1() {
        assert_eq!(ManiaRawFeatureRecord::default().analyzer_version, 0);
    }
}
