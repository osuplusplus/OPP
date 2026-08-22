use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    path::{Path, PathBuf},
};

use hnsw::{Hnsw, Searcher};
use memmap2::{Mmap, MmapOptions};
use rand_pcg::Pcg64;
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use space::{Metric, Neighbor};
use thiserror::Error;

use crate::{
    ANALYZER_ALGORITHM_ID, ANALYZER_VERSION, Analyzer, AnalyzerConfig, BaseFeatures,
    BeatmapFeatureRecord, BeatmapMetadata, DatasetInfo, DifficultyVector, DifficultyWeights,
    DynamicWeightProfile, OVERLAP_ALGORITHM_VERSION, ParameterVector, QueryFilters, QueryOptions,
    QueryResponse, QueryResult, QueryTarget, READING_ALGORITHM_VERSION, ROSU_PP_VERSION,
    WeightingMode,
};

const FEATURE_HEADER_LEN: usize = 32;
const FEATURE_FORMAT_VERSION: u32 = 1;
// `hnsw::Hnsw::nearest` requires its destination buffer to be no larger than
// its `ef` search pool. The runtime caps `ef` at 128, so the candidate buffer
// must use that same cap or a large index will panic inside the dependency.
const CANDIDATE_LIMIT: usize = 128;
const IGNORED_DIMENSION_PROBES: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
const MIN_STATS_SAMPLE_COUNT: u64 = 200;
const MIN_STDDEV: f32 = 0.05;

type Graph = Hnsw<WeightedL2, [f32; 5], Pcg64, 16, 32>;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct WeightedL2;

impl Metric<[f32; 5]> for WeightedL2 {
    type Unit = u64;

    fn distance(&self, left: &[f32; 5], right: &[f32; 5]) -> Self::Unit {
        left.iter()
            .zip(right)
            .map(|(left, right)| (*left as f64 - *right as f64).powi(2))
            .sum::<f64>()
            .to_bits()
    }
}

#[derive(Serialize, Deserialize)]
struct IndexFile {
    labels: Vec<u64>,
    graph: Graph,
    normalization_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NormalizerFile {
    version: u32,
    quantiles: [Vec<f32>; 5],
}

impl NormalizerFile {
    fn transform(&self, raw: DifficultyVector) -> DifficultyVector {
        let values = raw.as_array();
        let mut normalized = [0.0; 5];
        for (index, value) in values.into_iter().enumerate() {
            normalized[index] = rank(&self.quantiles[index], value);
        }
        DifficultyVector::from_array(normalized)
    }
}

fn rank(values: &[f32], value: f32) -> f32 {
    if values.len() <= 1 {
        return 0.0;
    }
    let index = values.partition_point(|candidate| *candidate <= value);
    (index.saturating_sub(1) as f32 / (values.len() - 1) as f32).clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    Invalid,
    Incompatible,
    UnknownBeatmap,
    Analysis,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct RuntimeError {
    kind: RuntimeErrorKind,
    message: String,
}

impl RuntimeError {
    pub fn kind(&self) -> RuntimeErrorKind {
        self.kind
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self {
            kind: RuntimeErrorKind::Invalid,
            message: message.into(),
        }
    }

    pub(crate) fn incompatible(message: impl Into<String>) -> Self {
        Self {
            kind: RuntimeErrorKind::Incompatible,
            message: message.into(),
        }
    }

    pub(crate) fn unknown() -> Self {
        Self {
            kind: RuntimeErrorKind::UnknownBeatmap,
            message: "beatmap is not present in the configured index".into(),
        }
    }

    pub(crate) fn analysis(message: impl Into<String>) -> Self {
        Self {
            kind: RuntimeErrorKind::Analysis,
            message: message.into(),
        }
    }
}

pub struct Dataset {
    metadata_path: PathBuf,
    feature_map: Mmap,
    record_size: usize,
    offsets: HashMap<u64, usize>,
    main_index: IndexFile,
    delta_index: Option<IndexFile>,
    normalizer: NormalizerFile,
    analyzer: Analyzer,
    info: DatasetInfo,
    has_star_metadata: bool,
    star_ratings: HashMap<u64, f32>,
    supports_dynamic_weighting: bool,
}

impl Dataset {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, RuntimeError> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(RuntimeError::invalid(
                "configured similarity index directory is unavailable",
            ));
        }

        let main_index = read_index(&root.join("indexes/difficulty-main.hnsw"))?;
        let normalization_version = main_index.normalization_version;
        if normalization_version == 0 {
            return Err(RuntimeError::incompatible(
                "the index declares an unsupported normalization version",
            ));
        }
        let delta_path = root.join("indexes/difficulty-delta.hnsw");
        let delta_index = if delta_path.exists() {
            let index = read_index(&delta_path)?;
            if index.normalization_version != normalization_version {
                return Err(RuntimeError::incompatible(
                    "main and delta indexes use different normalization versions",
                ));
            }
            Some(index)
        } else {
            None
        };

        let normalizer_path = root
            .join("normalizers")
            .join(format!("v{normalization_version}.bin"));
        let normalizer: NormalizerFile = bincode::deserialize_from(
            File::open(&normalizer_path)
                .map_err(|_| RuntimeError::invalid("normalizer file is missing"))?,
        )
        .map_err(|_| RuntimeError::invalid("normalizer file is invalid"))?;
        if normalizer.version != normalization_version {
            return Err(RuntimeError::incompatible(
                "normalizer version does not match the index",
            ));
        }

        let feature_path = root.join(format!("features-v{normalization_version}.bin"));
        let feature_file = File::open(&feature_path)
            .map_err(|_| RuntimeError::invalid("normalized feature file is missing"))?;
        // SAFETY: the dataset is documented and validated as immutable while OPP is using it.
        // OPP never writes to the selected directory, so the mapped file cannot be truncated by us.
        let feature_map = unsafe { MmapOptions::new().map(&feature_file) }
            .map_err(|_| RuntimeError::invalid("normalized feature file cannot be mapped"))?;
        validate_feature_header(&feature_map)?;
        let record_size = bincode::serialized_size(&BeatmapFeatureRecord::default())
            .map_err(|_| RuntimeError::invalid("feature record format is invalid"))?
            as usize;

        let metadata_path = root.join("metadata.sqlite");
        let connection = open_read_only(&metadata_path)?;
        validate_algorithm(&connection)?;
        let offsets = read_offsets(
            &connection,
            normalization_version,
            record_size,
            feature_map.len(),
        )?;
        if offsets.is_empty() {
            return Err(RuntimeError::invalid(
                "the configured index contains no normalized beatmaps",
            ));
        }
        if main_index
            .labels
            .iter()
            .chain(delta_index.iter().flat_map(|index| index.labels.iter()))
            .any(|beatmap_id| !offsets.contains_key(beatmap_id))
        {
            return Err(RuntimeError::invalid(
                "the HNSW index references missing feature records",
            ));
        }

        let has_star_metadata = supports_star_metadata(&connection)?;
        let star_ratings = if has_star_metadata {
            read_star_ratings(&connection)?
        } else {
            HashMap::new()
        };
        let supports_dynamic_weighting =
            supports_dynamic_schema(&connection, normalization_version)?;
        let info = DatasetInfo {
            record_count: offsets.len(),
            analyzer_version: ANALYZER_VERSION,
            normalization_version,
            algorithm_id: ANALYZER_ALGORITHM_ID.into(),
            data_cutoff_at: read_data_cutoff(&connection, normalization_version)?,
            supports_dynamic_weighting,
        };
        Ok(Self {
            metadata_path,
            feature_map,
            record_size,
            offsets,
            main_index,
            delta_index,
            normalizer,
            analyzer: Analyzer::new(AnalyzerConfig::default()),
            info,
            has_star_metadata,
            star_ratings,
            supports_dynamic_weighting,
        })
    }

    pub fn info(&self) -> &DatasetInfo {
        &self.info
    }

    pub fn contains(&self, beatmap_id: u64) -> bool {
        self.offsets.contains_key(&beatmap_id)
    }

    pub fn target_for_id(&self, beatmap_id: u64) -> Result<QueryTarget, RuntimeError> {
        Ok(QueryTarget {
            metadata: self.metadata_for(beatmap_id)?,
            record: self.record_for_id(beatmap_id)?,
        })
    }

    pub fn analyze_target(&self, bytes: &[u8]) -> Result<QueryTarget, RuntimeError> {
        let (metadata, raw) = self
            .analyzer
            .analyze_bytes(bytes)
            .map_err(|error| RuntimeError::analysis(error.to_string()))?;
        let record = BeatmapFeatureRecord {
            beatmap_id: raw.beatmap_id,
            beatmapset_id: raw.beatmapset_id,
            difficulty: self.normalizer.transform(raw.raw_difficulty),
            base: raw.base,
            overlap: raw.overlap,
            analyzer_version: raw.analyzer_version,
            normalization_version: self.normalizer.version,
            mod_profile: raw.mod_profile,
            flags: 0,
        };
        Ok(QueryTarget { metadata, record })
    }

    pub fn query(
        &self,
        target: &QueryTarget,
        options: &QueryOptions,
    ) -> Result<Vec<QueryResult>, RuntimeError> {
        Ok(self.query_with_profile(target, options)?.results)
    }

    pub fn query_with_profile(
        &self,
        target: &QueryTarget,
        options: &QueryOptions,
    ) -> Result<QueryResponse, RuntimeError> {
        validate_options(options)?;
        let (ids, weights, parameter_weight, weight_profile) = match options.weighting {
            WeightingMode::Manual {
                difficulty_weights,
                parameter_weight,
            } => {
                let ids = if parameter_weight > 0.0 {
                    self.offsets.keys().copied().collect()
                } else {
                    let mut ids = HashSet::new();
                    let vectors =
                        candidate_vectors(target.record.difficulty.as_array(), difficulty_weights);
                    for index in std::iter::once(&self.main_index).chain(self.delta_index.iter()) {
                        for vector in &vectors {
                            ids.extend(candidates(index, *vector));
                        }
                    }
                    ids
                };
                (ids, difficulty_weights, parameter_weight, None)
            }
            WeightingMode::Dynamic {
                lower_sections,
                upper_sections,
            } => {
                if !self.supports_dynamic_weighting {
                    return Err(RuntimeError::incompatible(
                        "this dataset does not contain dynamic star-section statistics",
                    ));
                }
                let stars = target
                    .metadata
                    .star_rating
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| RuntimeError::analysis("target star rating is unavailable"))?;
                let center = star_section(stars)?;
                let lower = i32::try_from(lower_sections)
                    .map_err(|_| RuntimeError::analysis("lower section range is too large"))?;
                let upper = i32::try_from(upper_sections)
                    .map_err(|_| RuntimeError::analysis("upper section range is too large"))?;
                let candidate_min = center.saturating_sub(lower);
                let candidate_max = center.saturating_add(upper);
                let connection = open_read_only(&self.metadata_path)?;
                let ids = read_section_ids(
                    &connection,
                    self.info.normalization_version,
                    candidate_min,
                    candidate_max,
                )?;
                let profile = dynamic_profile(
                    &connection,
                    self.info.normalization_version,
                    stars,
                    candidate_min,
                    candidate_max,
                    target.record.difficulty,
                    parameter_vector(target.record.base),
                )?;
                let weights = profile.weights;
                let parameter_weight = profile.parameter_weight;
                (ids, weights, parameter_weight, Some(profile))
            }
        };

        let mut scored = Vec::new();
        for beatmap_id in ids {
            if beatmap_id == target.record.beatmap_id {
                continue;
            }
            let candidate = self.record_for_id(beatmap_id)?;
            if target.record.beatmapset_id != 0
                && candidate.beatmapset_id == target.record.beatmapset_id
            {
                continue;
            }
            if !matches_filters(candidate.base, &options.filters) {
                continue;
            }
            if !matches_star_filter(
                self.star_ratings.get(&beatmap_id).copied(),
                &options.filters,
            ) {
                continue;
            }
            let difficulty_distance =
                difficulty_distance(target.record.difficulty, candidate.difficulty, weights);
            let base_distance = parameter_distance(target.record.base, candidate.base);
            let final_distance = combined_distance(
                target.record.difficulty,
                candidate.difficulty,
                weights,
                base_distance,
                parameter_weight,
            );
            scored.push((
                beatmap_id,
                candidate,
                final_distance,
                difficulty_distance,
                base_distance,
            ));
        }
        scored.sort_by(|left, right| {
            left.2
                .total_cmp(&right.2)
                .then_with(|| left.0.cmp(&right.0))
        });

        let mut seen_sets = HashSet::new();
        let mut results = Vec::with_capacity(options.result_limit);
        for (beatmap_id, record, final_distance, difficulty_distance, base_distance) in scored {
            if !seen_sets.insert(record.beatmapset_id) {
                continue;
            }
            results.push(QueryResult {
                metadata: self.metadata_for(beatmap_id)?,
                record,
                final_distance,
                difficulty_distance,
                base_distance,
            });
            if results.len() == options.result_limit {
                break;
            }
        }
        Ok(QueryResponse {
            results,
            weight_profile,
        })
    }

    fn record_for_id(&self, beatmap_id: u64) -> Result<BeatmapFeatureRecord, RuntimeError> {
        let offset = *self
            .offsets
            .get(&beatmap_id)
            .ok_or_else(RuntimeError::unknown)?;
        let end = offset
            .checked_add(self.record_size)
            .ok_or_else(|| RuntimeError::invalid("feature record offset overflow"))?;
        let bytes = self
            .feature_map
            .get(offset..end)
            .ok_or_else(|| RuntimeError::invalid("feature record is outside the data file"))?;
        bincode::deserialize(bytes)
            .map_err(|_| RuntimeError::invalid("feature record cannot be decoded"))
    }

    fn metadata_for(&self, beatmap_id: u64) -> Result<BeatmapMetadata, RuntimeError> {
        let connection = open_read_only(&self.metadata_path)?;
        let star_select = if self.has_star_metadata {
            ",star_rating"
        } else {
            ",NULL"
        };
        connection
            .query_row(
                &format!("SELECT beatmap_id,beatmapset_id,checksum,artist,title,version,creator,online_url{star_select} \
                 FROM beatmaps WHERE beatmap_id=?1"),
                [beatmap_id as i64],
                |row| {
                    Ok(BeatmapMetadata {
                        beatmap_id: row.get::<_, i64>(0)? as u64,
                        beatmapset_id: row.get::<_, i64>(1)? as u64,
                        checksum: row.get(2)?,
                        artist: row.get(3)?,
                        title: row.get(4)?,
                        version: row.get(5)?,
                        creator: row.get(6)?,
                        online_url: row.get(7)?,
                        star_rating: row.get(8)?,
                    })
                },
            )
            .map_err(|_| RuntimeError::invalid("beatmap metadata is missing"))
    }
}

fn read_index(path: &Path) -> Result<IndexFile, RuntimeError> {
    let bytes = fs::read(path).map_err(|_| RuntimeError::invalid("HNSW index file is missing"))?;
    let checksum_path = PathBuf::from(format!("{}.sha256", path.to_string_lossy()));
    let saved = fs::read_to_string(checksum_path)
        .map_err(|_| RuntimeError::invalid("HNSW checksum file is missing"))?;
    if saved.trim() != hex::encode(Sha256::digest(&bytes)) {
        return Err(RuntimeError::invalid("HNSW index checksum does not match"));
    }
    bincode::deserialize(&bytes).map_err(|_| RuntimeError::invalid("HNSW index cannot be decoded"))
}

fn validate_feature_header(bytes: &[u8]) -> Result<(), RuntimeError> {
    if bytes.len() < FEATURE_HEADER_LEN || &bytes[..7] != b"ODLNORM" {
        return Err(RuntimeError::invalid(
            "normalized feature file header is invalid",
        ));
    }
    let version = u32::from_le_bytes(
        bytes[8..12]
            .try_into()
            .map_err(|_| RuntimeError::invalid("feature format version is missing"))?,
    );
    if version != FEATURE_FORMAT_VERSION {
        return Err(RuntimeError::incompatible(
            "normalized feature format version is unsupported",
        ));
    }
    Ok(())
}

fn open_read_only(path: &Path) -> Result<Connection, RuntimeError> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| RuntimeError::invalid("metadata database is missing or invalid"))
}

fn table_columns(connection: &Connection, table: &str) -> Result<HashSet<String>, RuntimeError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|_| RuntimeError::invalid("metadata schema cannot be read"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|_| RuntimeError::invalid("metadata schema cannot be read"))?;
    rows.collect::<Result<HashSet<_>, _>>()
        .map_err(|_| RuntimeError::invalid("metadata schema cannot be read"))
}

fn supports_star_metadata(connection: &Connection) -> Result<bool, RuntimeError> {
    let beatmaps = table_columns(connection, "beatmaps")?;
    Ok(beatmaps.contains("star_rating") && beatmaps.contains("star_section"))
}

fn supports_dynamic_schema(
    connection: &Connection,
    normalization_version: u32,
) -> Result<bool, RuntimeError> {
    if !supports_star_metadata(connection)? {
        return Ok(false);
    }
    let stats = table_columns(connection, "star_section_stats")?;
    const REQUIRED: [&str; 20] = [
        "star_section",
        "analyzer_version",
        "normalization_version",
        "sample_count",
        "aim_sum",
        "speed_sum",
        "reading_sum",
        "slider_sum",
        "overlap_sum",
        "aim_sum_squares",
        "speed_sum_squares",
        "reading_sum_squares",
        "slider_sum_squares",
        "overlap_sum_squares",
        "ar_sum",
        "cs_sum",
        "od_sum",
        "ar_sum_squares",
        "cs_sum_squares",
        "od_sum_squares",
    ];
    if !REQUIRED.iter().all(|column| stats.contains(*column)) {
        return Ok(false);
    }
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM star_section_stats
                WHERE analyzer_version=?1 AND normalization_version=?2
                  AND sample_count>0
                  AND aim_sum IS NOT NULL AND speed_sum IS NOT NULL
                  AND reading_sum IS NOT NULL AND slider_sum IS NOT NULL
                  AND overlap_sum IS NOT NULL
                  AND aim_sum_squares IS NOT NULL AND speed_sum_squares IS NOT NULL
                  AND reading_sum_squares IS NOT NULL AND slider_sum_squares IS NOT NULL
                  AND overlap_sum_squares IS NOT NULL
                  AND ar_sum IS NOT NULL AND cs_sum IS NOT NULL AND od_sum IS NOT NULL
                  AND ar_sum_squares IS NOT NULL AND cs_sum_squares IS NOT NULL
                  AND od_sum_squares IS NOT NULL
            )",
            params![ANALYZER_VERSION as i64, normalization_version as i64],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| RuntimeError::invalid("star-section statistics cannot be validated"))
}

fn read_star_ratings(connection: &Connection) -> Result<HashMap<u64, f32>, RuntimeError> {
    let mut statement = connection
        .prepare("SELECT beatmap_id,star_rating FROM beatmaps WHERE star_rating IS NOT NULL")
        .map_err(|_| RuntimeError::invalid("star-rating metadata cannot be read"))?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, f32>(1)?)))
        .map_err(|_| RuntimeError::invalid("star-rating metadata cannot be read"))?;
    let mut ratings = HashMap::new();
    for row in rows {
        let (id, stars) =
            row.map_err(|_| RuntimeError::invalid("star-rating metadata is invalid"))?;
        if id >= 0 && stars.is_finite() && stars >= 0.0 {
            ratings.insert(id as u64, stars);
        }
    }
    Ok(ratings)
}

/// Maps a no-mod star rating to its fixed-width 0.1-star bucket.
pub fn star_section(stars: f32) -> Result<i32, RuntimeError> {
    if !stars.is_finite() || stars < 0.0 {
        return Err(RuntimeError::analysis(
            "star rating must be finite and non-negative",
        ));
    }
    // The dataset formula uses 1e-6. Runtime ratings arrive as f32, whose
    // decimal conversion error needs a slightly wider guard at boundaries.
    let section = ((stars as f64 / 0.1) + 1e-5).floor();
    if section > i32::MAX as f64 {
        return Err(RuntimeError::analysis("star rating is too large"));
    }
    Ok(section as i32)
}

fn read_section_ids(
    connection: &Connection,
    normalization_version: u32,
    min_section: i32,
    max_section: i32,
) -> Result<HashSet<u64>, RuntimeError> {
    let mut statement = connection
        .prepare(
            "SELECT beatmaps.beatmap_id FROM beatmaps
             INNER JOIN analyses ON analyses.beatmap_id=beatmaps.beatmap_id
             WHERE beatmaps.star_section BETWEEN ?1 AND ?2
               AND beatmaps.star_rating IS NOT NULL
               AND analyses.analyzer_version=?3
               AND analyses.normalization_version=?4
               AND analyses.status=2",
        )
        .map_err(|_| RuntimeError::invalid("star-section metadata cannot be read"))?;
    let rows = statement
        .query_map(
            params![
                min_section,
                max_section,
                ANALYZER_VERSION as i64,
                normalization_version as i64
            ],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|_| RuntimeError::invalid("star-section metadata cannot be read"))?;
    let mut ids = HashSet::new();
    for row in rows {
        let id = row.map_err(|_| RuntimeError::invalid("star-section beatmap id is invalid"))?;
        if id >= 0 {
            ids.insert(id as u64);
        }
    }
    Ok(ids)
}

#[derive(Debug, Clone, Copy, Default)]
struct SectionAggregate {
    count: u64,
    sum: [f64; 5],
    sum_squares: [f64; 5],
    parameter_sum: [f64; 3],
    parameter_sum_squares: [f64; 3],
    invalid_parameter_rows: u64,
}

#[derive(Debug, Clone, Default)]
struct ProfileValues {
    mean: DifficultyVector,
    stddev: DifficultyVector,
    delta: DifficultyVector,
    z_score: DifficultyVector,
    weights: DifficultyWeights,
    parameter_mean: ParameterVector,
    parameter_stddev: ParameterVector,
    parameter_delta: ParameterVector,
    parameter_z_score: ParameterVector,
    parameter_group_z_score: f32,
    parameter_weight: f32,
    fallback_reason: Option<String>,
}

fn aggregate_sections(
    connection: &Connection,
    normalization_version: u32,
    min_section: i32,
    max_section: i32,
) -> Result<SectionAggregate, RuntimeError> {
    connection
        .query_row(
            "SELECT COALESCE(SUM(sample_count),0),
                    COALESCE(SUM(aim_sum),0), COALESCE(SUM(speed_sum),0),
                    COALESCE(SUM(reading_sum),0), COALESCE(SUM(slider_sum),0),
                    COALESCE(SUM(overlap_sum),0), COALESCE(SUM(aim_sum_squares),0),
                    COALESCE(SUM(speed_sum_squares),0),
                    COALESCE(SUM(reading_sum_squares),0),
                    COALESCE(SUM(slider_sum_squares),0),
                    COALESCE(SUM(overlap_sum_squares),0),
                    COALESCE(SUM(ar_sum),0), COALESCE(SUM(cs_sum),0),
                    COALESCE(SUM(od_sum),0), COALESCE(SUM(ar_sum_squares),0),
                    COALESCE(SUM(cs_sum_squares),0), COALESCE(SUM(od_sum_squares),0),
                    COALESCE(SUM(CASE WHEN ar_sum IS NOT NULL AND cs_sum IS NOT NULL
                                      AND od_sum IS NOT NULL AND ar_sum_squares IS NOT NULL
                                      AND cs_sum_squares IS NOT NULL
                                      AND od_sum_squares IS NOT NULL
                                 THEN 0 ELSE 1 END),0)
             FROM star_section_stats
             WHERE analyzer_version=?1 AND normalization_version=?2
               AND star_section BETWEEN ?3 AND ?4",
            params![
                ANALYZER_VERSION as i64,
                normalization_version as i64,
                min_section,
                max_section
            ],
            |row| {
                let count = row.get::<_, i64>(0)?;
                Ok(SectionAggregate {
                    count: count.max(0) as u64,
                    sum: std::array::from_fn(|index| row.get(index + 1).unwrap_or(f64::NAN)),
                    sum_squares: std::array::from_fn(|index| {
                        row.get(index + 6).unwrap_or(f64::NAN)
                    }),
                    parameter_sum: std::array::from_fn(|index| {
                        row.get(index + 11).unwrap_or(f64::NAN)
                    }),
                    parameter_sum_squares: std::array::from_fn(|index| {
                        row.get(index + 14).unwrap_or(f64::NAN)
                    }),
                    invalid_parameter_rows: row.get::<_, i64>(17).unwrap_or(-1).max(0) as u64,
                })
            },
        )
        .map_err(|_| RuntimeError::invalid("star-section statistics cannot be read"))
}

fn dynamic_profile(
    connection: &Connection,
    normalization_version: u32,
    target_stars: f32,
    candidate_min: i32,
    candidate_max: i32,
    target: DifficultyVector,
    target_parameters: ParameterVector,
) -> Result<DynamicWeightProfile, RuntimeError> {
    let bounds = connection
        .query_row(
            "SELECT MIN(star_section),MAX(star_section) FROM star_section_stats
             WHERE analyzer_version=?1 AND normalization_version=?2",
            params![ANALYZER_VERSION as i64, normalization_version as i64],
            |row| Ok((row.get::<_, Option<i32>>(0)?, row.get::<_, Option<i32>>(1)?)),
        )
        .map_err(|_| RuntimeError::invalid("star-section statistics bounds cannot be read"))?;
    let (available_min, available_max) = match bounds {
        (Some(min), Some(max)) => (min, max),
        _ => (candidate_min, candidate_max),
    };
    let mut stats_min = candidate_min.max(available_min);
    let mut stats_max = candidate_max.min(available_max);
    if stats_min > stats_max {
        stats_min = candidate_min;
        stats_max = candidate_max;
    }
    let mut aggregate =
        aggregate_sections(connection, normalization_version, stats_min, stats_max)?;
    while aggregate.count < MIN_STATS_SAMPLE_COUNT
        && (stats_min > available_min || stats_max < available_max)
    {
        stats_min = stats_min.saturating_sub(1).max(available_min);
        stats_max = stats_max.saturating_add(1).min(available_max);
        aggregate = aggregate_sections(connection, normalization_version, stats_min, stats_max)?;
    }

    let target_values = target.as_array();
    let valid = aggregate.count >= 2
        && aggregate.sum.iter().all(|value| value.is_finite())
        && aggregate.sum_squares.iter().all(|value| value.is_finite())
        && aggregate
            .parameter_sum
            .iter()
            .all(|value| value.is_finite())
        && aggregate
            .parameter_sum_squares
            .iter()
            .all(|value| value.is_finite())
        && aggregate.invalid_parameter_rows == 0;
    let values = if valid {
        let count = aggregate.count as f64;
        let means = std::array::from_fn(|index| (aggregate.sum[index] / count) as f32);
        let stddevs = std::array::from_fn(|index| {
            let mean = aggregate.sum[index] / count;
            ((aggregate.sum_squares[index] / count - mean * mean)
                .max(0.0)
                .sqrt()) as f32
        });
        let parameter_means =
            std::array::from_fn(|index| (aggregate.parameter_sum[index] / count) as f32);
        let parameter_stddevs = std::array::from_fn(|index| {
            let mean = aggregate.parameter_sum[index] / count;
            ((aggregate.parameter_sum_squares[index] / count - mean * mean)
                .max(0.0)
                .sqrt()) as f32
        });
        if means.iter().all(|value| value.is_finite())
            && stddevs.iter().all(|value| value.is_finite())
            && parameter_means.iter().all(|value| value.is_finite())
            && parameter_stddevs.iter().all(|value| value.is_finite())
        {
            let deltas = std::array::from_fn(|index| target_values[index] - means[index]);
            let z =
                std::array::from_fn(|index| deltas[index].abs() / stddevs[index].max(MIN_STDDEV));
            let dynamic_weights =
                std::array::from_fn(|index| (0.25 + 0.75 * z[index]).clamp(0.25, 2.0));
            let parameter_deltas = std::array::from_fn(|index| {
                target_parameters.as_array()[index] - parameter_means[index]
            });
            let parameter_ranges = [11.0, 10.0, 11.0];
            let parameter_z = std::array::from_fn(|index| {
                let normalized_delta = parameter_deltas[index].abs() / parameter_ranges[index];
                let normalized_stddev = parameter_stddevs[index] / parameter_ranges[index];
                normalized_delta / normalized_stddev.max(MIN_STDDEV)
            });
            let parameter_group_z =
                (parameter_z.iter().map(|value| value.powi(2)).sum::<f32>() / 3.0).sqrt();
            let parameter_weight = (0.25 + 0.75 * parameter_group_z).clamp(0.25, 2.0);
            ProfileValues {
                mean: DifficultyVector::from_array(means),
                stddev: DifficultyVector::from_array(stddevs),
                delta: DifficultyVector::from_array(deltas),
                z_score: DifficultyVector::from_array(z),
                weights: DifficultyWeights::from_array(dynamic_weights),
                parameter_mean: ParameterVector::from_array(parameter_means),
                parameter_stddev: ParameterVector::from_array(parameter_stddevs),
                parameter_delta: ParameterVector::from_array(parameter_deltas),
                parameter_z_score: ParameterVector::from_array(parameter_z),
                parameter_group_z_score: parameter_group_z,
                parameter_weight,
                fallback_reason: None,
            }
        } else {
            fallback_profile_values("star-section statistics contain invalid values")
        }
    } else {
        fallback_profile_values(if aggregate.count < 2 {
            "fewer than two star-section samples are available"
        } else {
            "star-section statistics contain invalid values"
        })
    };

    Ok(DynamicWeightProfile {
        target_star_rating: target_stars,
        candidate_min_section: candidate_min,
        candidate_max_section: candidate_max,
        stats_min_section: stats_min,
        stats_max_section: stats_max,
        sample_count: aggregate.count,
        mean: values.mean,
        stddev: values.stddev,
        delta: values.delta,
        z_score: values.z_score,
        weights: values.weights,
        parameter_mean: values.parameter_mean,
        parameter_stddev: values.parameter_stddev,
        parameter_delta: values.parameter_delta,
        parameter_z_score: values.parameter_z_score,
        parameter_group_z_score: values.parameter_group_z_score,
        parameter_weight: values.parameter_weight,
        fallback_reason: values.fallback_reason,
    })
}

fn fallback_profile_values(reason: &str) -> ProfileValues {
    ProfileValues {
        weights: DifficultyWeights::default(),
        parameter_weight: 1.0,
        fallback_reason: Some(reason.into()),
        ..ProfileValues::default()
    }
}

fn validate_algorithm(connection: &Connection) -> Result<(), RuntimeError> {
    let versions = connection
        .query_row(
            "SELECT algorithm_id,rosu_pp_version,reading_version,overlap_version \
             FROM analysis_versions WHERE analyzer_version=?1",
            [ANALYZER_VERSION as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(|_| RuntimeError::incompatible("analyzer version is not supported"))?;
    if versions.0 != ANALYZER_ALGORITHM_ID
        || versions.1 != ROSU_PP_VERSION
        || versions.2 != READING_ALGORITHM_VERSION
        || versions.3 != OVERLAP_ALGORITHM_VERSION
    {
        return Err(RuntimeError::incompatible(
            "dataset algorithm snapshot does not match this runtime",
        ));
    }
    Ok(())
}

fn read_data_cutoff(
    connection: &Connection,
    normalization_version: u32,
) -> Result<Option<i64>, RuntimeError> {
    let columns = connection
        .prepare("PRAGMA table_info(beatmaps)")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|_| RuntimeError::invalid("metadata schema cannot be read"))?;
    if !columns.iter().any(|column| column == "updated_at") {
        return Ok(None);
    }

    connection
        .query_row(
            "SELECT MAX(beatmaps.updated_at)
             FROM beatmaps
             INNER JOIN analyses ON analyses.beatmap_id = beatmaps.beatmap_id
             WHERE analyses.analyzer_version = ?1
               AND analyses.normalization_version = ?2
               AND analyses.status = 2",
            [ANALYZER_VERSION as i64, normalization_version as i64],
            |row| row.get(0),
        )
        .map_err(|_| RuntimeError::invalid("metadata cutoff cannot be read"))
}

fn read_offsets(
    connection: &Connection,
    normalization_version: u32,
    record_size: usize,
    feature_length: usize,
) -> Result<HashMap<u64, usize>, RuntimeError> {
    let mut statement = connection
        .prepare(
            "SELECT beatmap_id,normalized_offset FROM analyses \
             WHERE analyzer_version=?1 AND normalization_version=?2 AND status=2",
        )
        .map_err(|_| RuntimeError::invalid("analysis metadata cannot be read"))?;
    let rows = statement
        .query_map(
            params![ANALYZER_VERSION as i64, normalization_version as i64],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .map_err(|_| RuntimeError::invalid("analysis offsets cannot be read"))?;
    let mut offsets = HashMap::new();
    for row in rows {
        let (beatmap_id, offset) =
            row.map_err(|_| RuntimeError::invalid("analysis offset is invalid"))?;
        let offset = offset
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| RuntimeError::invalid("analysis offset is missing"))?;
        if offset < FEATURE_HEADER_LEN
            || offset
                .checked_add(record_size)
                .is_none_or(|end| end > feature_length)
        {
            return Err(RuntimeError::invalid(
                "analysis offset is outside the feature file",
            ));
        }
        offsets.insert(beatmap_id as u64, offset);
    }
    Ok(offsets)
}

fn candidates(index: &IndexFile, vector: [f32; 5]) -> Vec<u64> {
    if index.labels.is_empty() {
        return Vec::new();
    }
    let mut searcher = Searcher::new();
    let mut destination = vec![
        Neighbor {
            index: usize::MAX,
            distance: u64::MAX,
        };
        CANDIDATE_LIMIT.min(index.labels.len()).max(1)
    ];
    index
        .graph
        .nearest(&vector, CANDIDATE_LIMIT, &mut searcher, &mut destination)
        .iter()
        .filter_map(|neighbor| index.labels.get(neighbor.index).copied())
        .collect()
}

fn candidate_vectors(vector: [f32; 5], weights: DifficultyWeights) -> Vec<[f32; 5]> {
    let weights = [
        weights.aim,
        weights.speed,
        weights.reading,
        weights.slider,
        weights.overlap,
    ];
    let mut vectors = vec![vector];
    for (dimension, weight) in weights.into_iter().enumerate() {
        if weight != 0.0 {
            continue;
        }
        for probe in IGNORED_DIMENSION_PROBES {
            if probe == vector[dimension] {
                continue;
            }
            let mut candidate = vector;
            candidate[dimension] = probe;
            vectors.push(candidate);
        }
    }
    vectors
}

fn validate_options(options: &QueryOptions) -> Result<(), RuntimeError> {
    if !(1..=150).contains(&options.result_limit) {
        return Err(RuntimeError::analysis(
            "result limit must be between 1 and 150",
        ));
    }
    let (difficulty, parameter_weight) = match options.weighting {
        WeightingMode::Manual {
            difficulty_weights,
            parameter_weight,
        } => (difficulty_weights, parameter_weight),
        WeightingMode::Dynamic {
            lower_sections,
            upper_sections,
        } => {
            if lower_sections > 200 || upper_sections > 200 {
                return Err(RuntimeError::analysis(
                    "dynamic section ranges must be between 0 and 200",
                ));
            }
            return Ok(());
        }
    };
    let weights = [
        difficulty.aim,
        difficulty.speed,
        difficulty.reading,
        difficulty.slider,
        difficulty.overlap,
    ];
    if weights
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
        || !parameter_weight.is_finite()
        || !(0.0..=2.0).contains(&parameter_weight)
        || weights.iter().any(|value| *value > 2.0)
        || (weights.iter().all(|value| *value == 0.0) && parameter_weight == 0.0)
    {
        return Err(RuntimeError::analysis(
            "similarity weights must be finite, between zero and two, and not all zero",
        ));
    }
    Ok(())
}

fn matches_filters(base: BaseFeatures, filters: &QueryFilters) -> bool {
    filters.min_ar.is_none_or(|value| base.ar >= value)
        && filters.max_ar.is_none_or(|value| base.ar <= value)
        && filters.min_cs.is_none_or(|value| base.cs >= value)
        && filters.max_cs.is_none_or(|value| base.cs <= value)
        && filters.min_od.is_none_or(|value| base.od >= value)
        && filters.max_od.is_none_or(|value| base.od <= value)
        && filters.min_bpm.is_none_or(|value| base.bpm >= value)
        && filters.max_bpm.is_none_or(|value| base.bpm <= value)
        && filters
            .min_length_seconds
            .is_none_or(|value| base.length_seconds >= value)
        && filters
            .max_length_seconds
            .is_none_or(|value| base.length_seconds <= value)
        && filters
            .min_object_density
            .is_none_or(|value| base.object_density >= value)
        && filters
            .max_object_density
            .is_none_or(|value| base.object_density <= value)
        && filters
            .min_circle_ratio
            .is_none_or(|value| base.circle_ratio >= value)
        && filters
            .max_circle_ratio
            .is_none_or(|value| base.circle_ratio <= value)
        && filters
            .min_slider_ratio
            .is_none_or(|value| base.slider_ratio >= value)
        && filters
            .max_slider_ratio
            .is_none_or(|value| base.slider_ratio <= value)
}

fn matches_star_filter(stars: Option<f32>, filters: &QueryFilters) -> bool {
    if filters.min_star.is_none() && filters.max_star.is_none() {
        return true;
    }
    stars.is_some_and(|stars| {
        filters.min_star.is_none_or(|minimum| stars >= minimum)
            && filters.max_star.is_none_or(|maximum| stars <= maximum)
    })
}

fn parameter_vector(base: BaseFeatures) -> ParameterVector {
    ParameterVector {
        ar: base.ar,
        cs: base.cs,
        od: base.od,
    }
}

fn parameter_distance(left: BaseFeatures, right: BaseFeatures) -> f32 {
    let differences = [
        (left.ar - right.ar) / 11.0,
        (left.cs - right.cs) / 10.0,
        (left.od - right.od) / 11.0,
    ];
    (differences.iter().map(|value| value.powi(2)).sum::<f32>() / 3.0).sqrt()
}

fn difficulty_squared_sum(
    left: DifficultyVector,
    right: DifficultyVector,
    weights: DifficultyWeights,
) -> (f32, f32) {
    let weights = weights.as_array();
    let sum = left
        .as_array()
        .iter()
        .zip(right.as_array())
        .zip(weights)
        .map(|((left, right), weight)| weight * (left - right).powi(2))
        .sum();
    (sum, weights.iter().sum())
}

fn combined_distance(
    left: DifficultyVector,
    right: DifficultyVector,
    weights: DifficultyWeights,
    parameter_distance: f32,
    parameter_weight: f32,
) -> f32 {
    let (difficulty_sum, difficulty_weight_sum) = difficulty_squared_sum(left, right, weights);
    ((difficulty_sum + parameter_weight * parameter_distance.powi(2))
        / (difficulty_weight_sum + parameter_weight))
        .sqrt()
}

fn difficulty_distance(
    left: DifficultyVector,
    right: DifficultyVector,
    weights: DifficultyWeights,
) -> f32 {
    let (sum, weight_sum) = difficulty_squared_sum(left, right, weights);
    if weight_sum == 0.0 {
        0.0
    } else {
        (sum / weight_sum).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use hnsw::Params;
    use tempfile::TempDir;

    use super::*;

    fn record(id: u64, set: u64, difficulty: [f32; 5], bpm: f32) -> BeatmapFeatureRecord {
        BeatmapFeatureRecord {
            beatmap_id: id,
            beatmapset_id: set,
            difficulty: DifficultyVector::from_array(difficulty),
            base: BaseFeatures {
                bpm,
                ar: 9.0,
                cs: 4.0,
                od: 8.0,
                length_seconds: 120.0,
                object_density: 4.0,
                circle_ratio: 0.6,
                slider_ratio: 0.4,
                ..BaseFeatures::default()
            },
            analyzer_version: ANALYZER_VERSION,
            normalization_version: 1,
            ..BeatmapFeatureRecord::default()
        }
    }

    fn create_dataset() -> TempDir {
        let directory = tempfile::tempdir().expect("temp directory");
        fs::create_dir_all(directory.path().join("indexes")).expect("index dir");
        fs::create_dir_all(directory.path().join("normalizers")).expect("normalizer dir");

        let records = [
            record(10, 1, [0.1; 5], 180.0),
            record(20, 2, [0.11; 5], 181.0),
            record(21, 2, [0.12; 5], 182.0),
            record(30, 3, [0.8; 5], 240.0),
        ];
        let mut features =
            File::create(directory.path().join("features-v1.bin")).expect("feature file");
        let mut header = [0_u8; FEATURE_HEADER_LEN];
        header[..7].copy_from_slice(b"ODLNORM");
        header[8..12].copy_from_slice(&FEATURE_FORMAT_VERSION.to_le_bytes());
        features.write_all(&header).expect("feature header");
        let record_size =
            bincode::serialized_size(&BeatmapFeatureRecord::default()).expect("record size");
        for value in records {
            bincode::serialize_into(&mut features, &value).expect("feature record");
        }
        features.flush().expect("flush feature file");

        let normalizer = NormalizerFile {
            version: 1,
            quantiles: std::array::from_fn(|_| vec![0.0, 1.0]),
        };
        bincode::serialize_into(
            File::create(directory.path().join("normalizers/v1.bin")).expect("normalizer"),
            &normalizer,
        )
        .expect("write normalizer");

        let connection =
            Connection::open(directory.path().join("metadata.sqlite")).expect("metadata");
        connection
            .execute_batch(
                "CREATE TABLE beatmaps (
                    beatmap_id INTEGER PRIMARY KEY, beatmapset_id INTEGER NOT NULL,
                    checksum TEXT NOT NULL, artist TEXT NOT NULL, title TEXT NOT NULL,
                    version TEXT NOT NULL, creator TEXT NOT NULL, online_url TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                 );
                 CREATE TABLE analyses (
                    beatmap_id INTEGER NOT NULL, analyzer_version INTEGER NOT NULL,
                    normalization_version INTEGER NOT NULL, normalized_offset INTEGER,
                    status INTEGER NOT NULL
                 );
                 CREATE TABLE analysis_versions (
                    analyzer_version INTEGER PRIMARY KEY, algorithm_id TEXT NOT NULL,
                    rosu_pp_version TEXT NOT NULL, reading_version TEXT NOT NULL,
                    overlap_version TEXT NOT NULL
                 );",
            )
            .expect("schema");
        connection
            .execute(
                "INSERT INTO analysis_versions VALUES (?1,?2,?3,?4,?5)",
                params![
                    ANALYZER_VERSION,
                    ANALYZER_ALGORITHM_ID,
                    ROSU_PP_VERSION,
                    READING_ALGORITHM_VERSION,
                    OVERLAP_ALGORITHM_VERSION
                ],
            )
            .expect("algorithm");
        for (index, value) in records.iter().enumerate() {
            connection
                .execute(
                    "INSERT INTO beatmaps VALUES (?1,?2,'checksum','artist',?3,'difficulty','mapper',?4,?5)",
                    params![
                        value.beatmap_id as i64,
                        value.beatmapset_id as i64,
                        format!("map {}", value.beatmap_id),
                        format!("https://osu.ppy.sh/b/{}", value.beatmap_id),
                        1_700_000_000_i64 + index as i64,
                    ],
                )
                .expect("metadata row");
            connection
                .execute(
                    "INSERT INTO analyses VALUES (?1,?2,1,?3,2)",
                    params![
                        value.beatmap_id as i64,
                        ANALYZER_VERSION,
                        FEATURE_HEADER_LEN as i64 + index as i64 * record_size as i64
                    ],
                )
                .expect("analysis row");
        }
        drop(connection);

        let mut graph = Graph::new_params(WeightedL2, Params::new().ef_construction(200));
        let mut searcher = Searcher::new();
        let mut labels = Vec::new();
        for value in records {
            graph.insert(value.difficulty.as_array(), &mut searcher);
            labels.push(value.beatmap_id);
        }
        write_test_index(
            directory.path().join("indexes/difficulty-main.hnsw"),
            &IndexFile {
                labels,
                graph,
                normalization_version: 1,
            },
        );
        let delta = IndexFile {
            labels: Vec::new(),
            graph: Graph::new_params(WeightedL2, Params::new().ef_construction(200)),
            normalization_version: 1,
        };
        write_test_index(
            directory.path().join("indexes/difficulty-delta.hnsw"),
            &delta,
        );
        directory
    }

    fn add_dynamic_schema(directory: &TempDir) {
        let connection =
            Connection::open(directory.path().join("metadata.sqlite")).expect("metadata");
        connection
            .execute_batch(
                "ALTER TABLE beatmaps ADD COLUMN star_rating REAL;
                 ALTER TABLE beatmaps ADD COLUMN star_section INTEGER;
                 CREATE INDEX idx_beatmaps_star_section ON beatmaps(star_section);
                 CREATE TABLE star_section_stats (
                    star_section INTEGER NOT NULL,
                    analyzer_version INTEGER NOT NULL,
                    normalization_version INTEGER NOT NULL,
                    sample_count INTEGER NOT NULL,
                    aim_sum REAL NOT NULL, speed_sum REAL NOT NULL,
                    reading_sum REAL NOT NULL, slider_sum REAL NOT NULL,
                    overlap_sum REAL NOT NULL, aim_sum_squares REAL NOT NULL,
                    speed_sum_squares REAL NOT NULL, reading_sum_squares REAL NOT NULL,
                    slider_sum_squares REAL NOT NULL, overlap_sum_squares REAL NOT NULL,
                    ar_sum REAL NOT NULL, cs_sum REAL NOT NULL, od_sum REAL NOT NULL,
                    ar_sum_squares REAL NOT NULL, cs_sum_squares REAL NOT NULL,
                    od_sum_squares REAL NOT NULL,
                    PRIMARY KEY(star_section,analyzer_version,normalization_version)
                 );
                 UPDATE beatmaps SET star_rating=6.1,star_section=61 WHERE beatmap_id=10;
                 UPDATE beatmaps SET star_rating=6.0,star_section=60 WHERE beatmap_id=20;
                 UPDATE beatmaps SET star_rating=6.2,star_section=62 WHERE beatmap_id=21;
                 UPDATE beatmaps SET star_rating=8.0,star_section=80 WHERE beatmap_id=30;",
            )
            .expect("dynamic schema");
        for section in [60_i32, 61, 62] {
            // A 0.2 mean and 0.1 standard deviation in all five normalized dimensions.
            connection
                .execute(
                    "INSERT INTO star_section_stats VALUES
                     (?1,?2,1,100,20,20,20,20,20,5,5,5,5,5,
                      900,400,800,8100,1600,6400)",
                    params![section, ANALYZER_VERSION],
                )
                .expect("section stats");
        }
    }

    fn manual_options() -> QueryOptions {
        QueryOptions {
            weighting: WeightingMode::Manual {
                difficulty_weights: DifficultyWeights::default(),
                parameter_weight: 1.0,
            },
            ..QueryOptions::default()
        }
    }

    fn write_test_index(path: PathBuf, index: &IndexFile) {
        let bytes = bincode::serialize(index).expect("serialize index");
        fs::write(&path, &bytes).expect("write index");
        fs::write(
            format!("{}.sha256", path.to_string_lossy()),
            hex::encode(Sha256::digest(&bytes)),
        )
        .expect("write checksum");
    }

    #[test]
    fn opens_and_queries_without_duplicate_sets() {
        let directory = create_dataset();
        let dataset = Dataset::open(directory.path()).expect("open dataset");
        assert_eq!(dataset.info().record_count, 4);
        assert_eq!(dataset.info().data_cutoff_at, Some(1_700_000_003));
        assert!(!dataset.info().supports_dynamic_weighting);
        let target = dataset.target_for_id(10).expect("target");
        let results = dataset.query(&target, &manual_options()).expect("query");
        assert_eq!(
            results
                .iter()
                .map(|result| result.metadata.beatmap_id)
                .collect::<Vec<_>>(),
            vec![20, 30]
        );
        assert!(results.iter().all(|result| {
            result.final_distance <= result.difficulty_distance && result.base_distance == 0.0
        }));
        assert!(!directory.path().join("metadata.sqlite-wal").exists());
        assert!(!directory.path().join("metadata.sqlite-shm").exists());
    }

    #[test]
    fn dynamic_query_scans_only_requested_star_sections() {
        let directory = create_dataset();
        add_dynamic_schema(&directory);
        let dataset = Dataset::open(directory.path()).expect("open dataset");
        assert!(dataset.info().supports_dynamic_weighting);
        let target = dataset.target_for_id(10).expect("target");
        let response = dataset
            .query_with_profile(
                &target,
                &QueryOptions {
                    weighting: WeightingMode::Dynamic {
                        lower_sections: 1,
                        upper_sections: 1,
                    },
                    ..QueryOptions::default()
                },
            )
            .expect("dynamic query");
        assert_eq!(
            response
                .results
                .iter()
                .map(|result| result.metadata.beatmap_id)
                .collect::<Vec<_>>(),
            vec![20]
        );
        let profile = response.weight_profile.expect("profile");
        assert_eq!(
            (profile.candidate_min_section, profile.candidate_max_section),
            (60, 62)
        );
        assert_eq!(profile.sample_count, 300);
        assert!(profile.fallback_reason.is_none());
    }

    #[test]
    fn star_sections_floor_stably_at_tenths() {
        assert_eq!(star_section(6.1).expect("section"), 61);
        assert_eq!(star_section(5.7).expect("section"), 57);
        assert_eq!(star_section(6.599_99).expect("section"), 65);
        let center = star_section(6.1).expect("section");
        assert_eq!((center - 4, center + 4), (57, 65));
    }

    #[test]
    fn normalized_distance_is_independent_of_weight_scale() {
        let left = DifficultyVector::from_array([0.0; 5]);
        let right = DifficultyVector::from_array([1.0, 0.0, 0.0, 0.0, 0.0]);
        let one = DifficultyWeights::from_array([1.0; 5]);
        let ten = DifficultyWeights::from_array([10.0; 5]);
        assert!(
            (difficulty_distance(left, right, one) - difficulty_distance(left, right, ten)).abs()
                < 1e-6
        );
    }

    #[test]
    fn feature_record_binary_layout_remains_unchanged() {
        assert_eq!(
            bincode::serialized_size(&BeatmapFeatureRecord::default()).expect("record size"),
            124
        );
    }

    #[test]
    fn parameter_distance_uses_fixed_ranges_and_one_shared_group() {
        let left = BaseFeatures::default();
        let right = BaseFeatures {
            ar: 11.0,
            cs: 10.0,
            od: 11.0,
            ..BaseFeatures::default()
        };
        assert!((parameter_distance(left, right) - 1.0).abs() < 1e-6);

        let final_distance = combined_distance(
            DifficultyVector::default(),
            DifficultyVector::default(),
            DifficultyWeights::from_array([1.0; 5]),
            (1.0_f32 / 3.0).sqrt(),
            1.0,
        );
        assert!((final_distance - (1.0_f32 / 18.0).sqrt()).abs() < 1e-6);
    }

    #[test]
    fn parameter_outlier_uses_normalized_rms_z_score() {
        let directory = create_dataset();
        add_dynamic_schema(&directory);
        let connection =
            Connection::open(directory.path().join("metadata.sqlite")).expect("metadata");
        let profile = dynamic_profile(
            &connection,
            1,
            6.1,
            60,
            62,
            DifficultyVector::from_array([0.2; 5]),
            ParameterVector {
                ar: 10.1,
                cs: 4.0,
                od: 8.0,
            },
        )
        .expect("dynamic profile");
        // AR differs by 10% of its fixed range, with the normalized stddev floor at 5%.
        assert!((profile.parameter_z_score.ar - 2.0).abs() < 1e-6);
        assert!((profile.parameter_group_z_score - (4.0_f32 / 3.0).sqrt()).abs() < 1e-6);
        assert!((profile.parameter_weight - (0.25 + 0.75 * (4.0_f32 / 3.0).sqrt())).abs() < 1e-6);
    }

    #[test]
    fn star_filter_is_applied_during_manual_exact_scan() {
        let directory = create_dataset();
        add_dynamic_schema(&directory);
        let dataset = Dataset::open(directory.path()).expect("open dataset");
        let target = dataset.target_for_id(10).expect("target");
        let response = dataset
            .query(
                &target,
                &QueryOptions {
                    weighting: WeightingMode::Manual {
                        difficulty_weights: DifficultyWeights::default(),
                        parameter_weight: 1.0,
                    },
                    filters: QueryFilters {
                        min_star: Some(6.15),
                        max_star: Some(6.25),
                        ..QueryFilters::default()
                    },
                    result_limit: 20,
                },
            )
            .expect("manual query");
        assert_eq!(
            response
                .iter()
                .map(|result| result.metadata.beatmap_id)
                .collect::<Vec<_>>(),
            vec![21]
        );
    }

    #[test]
    fn migrated_null_parameter_statistics_disable_dynamic_weighting() {
        let directory = create_dataset();
        add_dynamic_schema(&directory);
        let connection =
            Connection::open(directory.path().join("metadata.sqlite")).expect("metadata");
        connection
            .execute_batch(
                "PRAGMA foreign_keys=OFF;
                 CREATE TABLE replacement AS
                    SELECT star_section,analyzer_version,normalization_version,sample_count,
                           aim_sum,speed_sum,reading_sum,slider_sum,overlap_sum,
                           aim_sum_squares,speed_sum_squares,reading_sum_squares,
                           slider_sum_squares,overlap_sum_squares,
                           NULL AS ar_sum,NULL AS cs_sum,NULL AS od_sum,
                           NULL AS ar_sum_squares,NULL AS cs_sum_squares,NULL AS od_sum_squares
                    FROM star_section_stats;
                 DROP TABLE star_section_stats;
                 ALTER TABLE replacement RENAME TO star_section_stats;",
            )
            .expect("replace migrated stats");
        drop(connection);
        let dataset = Dataset::open(directory.path()).expect("open legacy migrated dataset");
        assert!(!dataset.info().supports_dynamic_weighting);
        assert!(
            dataset
                .query(
                    &dataset.target_for_id(10).expect("target"),
                    &manual_options()
                )
                .is_ok()
        );
    }

    #[test]
    fn legacy_five_dimension_statistics_keep_manual_mode_available() {
        let directory = create_dataset();
        let connection =
            Connection::open(directory.path().join("metadata.sqlite")).expect("metadata");
        connection
            .execute_batch(
                "ALTER TABLE beatmaps ADD COLUMN star_rating REAL;
                 ALTER TABLE beatmaps ADD COLUMN star_section INTEGER;
                 CREATE TABLE star_section_stats (
                    star_section INTEGER NOT NULL, analyzer_version INTEGER NOT NULL,
                    normalization_version INTEGER NOT NULL, sample_count INTEGER NOT NULL,
                    aim_sum REAL NOT NULL, speed_sum REAL NOT NULL, reading_sum REAL NOT NULL,
                    slider_sum REAL NOT NULL, overlap_sum REAL NOT NULL,
                    aim_sum_squares REAL NOT NULL, speed_sum_squares REAL NOT NULL,
                    reading_sum_squares REAL NOT NULL, slider_sum_squares REAL NOT NULL,
                    overlap_sum_squares REAL NOT NULL
                 );
                 INSERT INTO star_section_stats VALUES
                    (61,3,1,2,1,1,1,1,1,1,1,1,1,1);
                 UPDATE beatmaps SET star_rating=6.1,star_section=61;",
            )
            .expect("legacy dynamic schema");
        drop(connection);

        let dataset = Dataset::open(directory.path()).expect("open legacy dataset");
        assert!(!dataset.info().supports_dynamic_weighting);
        assert!(
            dataset
                .query(
                    &dataset.target_for_id(10).expect("target"),
                    &manual_options()
                )
                .is_ok()
        );
    }

    #[test]
    fn slider_outlier_receives_the_largest_dynamic_weight() {
        let directory = create_dataset();
        add_dynamic_schema(&directory);
        let connection =
            Connection::open(directory.path().join("metadata.sqlite")).expect("metadata");
        let target = DifficultyVector::from_array([0.2, 0.2, 0.2, 0.9, 0.2]);
        let profile = dynamic_profile(
            &connection,
            1,
            6.1,
            60,
            62,
            target,
            ParameterVector {
                ar: 9.0,
                cs: 4.0,
                od: 8.0,
            },
        )
        .expect("dynamic profile");
        assert_eq!(profile.weights.slider, 2.0);
        assert!(profile.weights.slider > profile.weights.aim);
        assert!(profile.delta.slider > 0.0);
        assert!(profile.fallback_reason.is_none());
    }

    #[test]
    fn sparse_statistics_expand_symmetrically_and_can_fallback() {
        let directory = create_dataset();
        add_dynamic_schema(&directory);
        let connection =
            Connection::open(directory.path().join("metadata.sqlite")).expect("metadata");
        connection
            .execute("UPDATE star_section_stats SET sample_count=1", [])
            .expect("make sparse");
        connection
            .execute(
                "INSERT INTO star_section_stats VALUES
                 (59,?1,1,200,40,40,40,40,40,10,10,10,10,10,
                  1800,800,1600,16200,3200,12800)",
                [ANALYZER_VERSION],
            )
            .expect("outer stats");
        let profile = dynamic_profile(
            &connection,
            1,
            6.1,
            61,
            61,
            DifficultyVector::from_array([0.2; 5]),
            ParameterVector {
                ar: 9.0,
                cs: 4.0,
                od: 8.0,
            },
        )
        .expect("expanded profile");
        assert_eq!(
            (profile.stats_min_section, profile.stats_max_section),
            (59, 62)
        );
        assert_eq!(profile.sample_count, 203);

        connection
            .execute("DELETE FROM star_section_stats", [])
            .expect("clear stats");
        let fallback = dynamic_profile(
            &connection,
            1,
            6.1,
            61,
            61,
            DifficultyVector::default(),
            ParameterVector::default(),
        )
        .expect("fallback profile");
        assert_eq!(fallback.weights, DifficultyWeights::default());
        assert!(fallback.fallback_reason.is_some());
    }

    #[test]
    fn probes_across_dimensions_excluded_from_scoring() {
        let vector = [0.1, 0.2, 0.3, 0.4, 0.5];
        let vectors = candidate_vectors(
            vector,
            DifficultyWeights {
                slider: 0.0,
                ..DifficultyWeights::default()
            },
        );

        assert_eq!(vectors[0], vector);
        assert!(vectors.iter().any(|candidate| candidate[3] == 0.0));
        assert!(vectors.iter().any(|candidate| candidate[3] == 1.0));
        assert!(
            vectors
                .iter()
                .all(|candidate| { candidate[..3] == vector[..3] && candidate[4] == vector[4] })
        );
    }

    #[test]
    fn rejects_a_tampered_index() {
        let directory = create_dataset();
        fs::write(
            directory.path().join("indexes/difficulty-main.hnsw.sha256"),
            "bad",
        )
        .expect("tamper checksum");
        let error = match Dataset::open(directory.path()) {
            Ok(_) => panic!("tampered dataset should be rejected"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), RuntimeErrorKind::Invalid);
    }

    #[test]
    fn rejects_an_incompatible_algorithm_snapshot() {
        let directory = create_dataset();
        let connection =
            Connection::open(directory.path().join("metadata.sqlite")).expect("metadata");
        connection
            .execute(
                "UPDATE analysis_versions SET algorithm_id='unsupported'",
                [],
            )
            .expect("change algorithm");
        drop(connection);

        let error = match Dataset::open(directory.path()) {
            Ok(_) => panic!("incompatible dataset should be rejected"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), RuntimeErrorKind::Incompatible);
    }

    #[test]
    fn analyzes_an_unindexed_standard_map() {
        let directory = create_dataset();
        let dataset = Dataset::open(directory.path()).expect("open dataset");
        let bytes = b"osu file format v14\n\n[General]\nMode:0\n\n[Metadata]\nTitle:Local\nArtist:Test\nCreator:Mapper\nVersion:Hard\nBeatmapID:999\nBeatmapSetID:999\n\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:7\nApproachRate:9\n\n[TimingPoints]\n0,500,4,2,0,100,1,0\n\n[HitObjects]\n64,64,0,1,0,0:0:0:0:\n448,320,500,1,0,0:0:0:0:\n64,64,1000,1,0,0:0:0:0:\n";
        let target = dataset.analyze_target(bytes).expect("analyze target");
        assert_eq!(target.metadata.beatmap_id, 999);
        assert_eq!(target.record.normalization_version, 1);
    }
}
