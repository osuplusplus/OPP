//! Runtime-only compatibility layer for private osu-difficulty-lab datasets.
//! The data directory is treated as immutable and is never created or changed.

mod analyzer;
mod dataset;
mod mania_analyzer;
mod mania_dataset;
mod mania_index;
mod mania_normalizer;
mod mania_types;
mod types;

pub use analyzer::Analyzer;
pub use dataset::{Dataset, RuntimeError, RuntimeErrorKind, star_section};
pub use mania_analyzer::{ManiaAnalyzeError, ManiaAnalyzer};
pub use mania_dataset::ManiaDataset;
pub use mania_normalizer::{ManiaNormalizeError, ManiaNormalizer, overall_intensity};
pub use mania_types::*;
pub use types::*;

pub const ANALYZER_VERSION: u32 = 4;
pub const ANALYZER_ALGORITHM_ID: &str = "five-dimension-slider-rosu-reading-v4";
pub const ROSU_PP_VERSION: &str = "Apeuriox/rosu-pp@pp-rework-202607#9a073d29";
pub const READING_ALGORITHM_VERSION: &str = "rosu-reading-pp-rework-202607-v1";
pub const OVERLAP_ALGORITHM_VERSION: &str = "overlap-visibility-spatial-strain-v1";

/// Independent, immutable compatibility snapshot for osu!mania similarity.
/// This is pinned to osu-difficulty-lab commit
/// 1fa21fa6a5144992df58efe7ce9d96019981fad3.
pub const MANIA_ANALYZER_VERSION: u32 = 1;
pub const MANIA_NORMALIZATION_VERSION: u32 = 1;
pub const MANIA_ANALYZER_ALGORITHM_ID: &str = "mania-roxy-interlude-similarity-v1";
pub const MANIA_ANALYZER_SNAPSHOT: &str = "1:mania-roxy-interlude-similarity-v1";
pub const MANIA_PROVENANCE_COMMIT: &str = "1fa21fa6a5144992df58efe7ce9d96019981fad3";
