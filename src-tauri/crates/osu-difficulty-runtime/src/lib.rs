//! Runtime-only compatibility layer for private osu-difficulty-lab datasets.
//! The data directory is treated as immutable and is never created or changed.

mod analyzer;
mod dataset;
mod types;

pub use analyzer::Analyzer;
pub use dataset::{Dataset, RuntimeError, RuntimeErrorKind, star_section};
pub use types::*;

pub const ANALYZER_VERSION: u32 = 4;
pub const ANALYZER_ALGORITHM_ID: &str = "five-dimension-slider-rosu-reading-v4";
pub const ROSU_PP_VERSION: &str = "Apeuriox/rosu-pp@pp-rework-202607#9a073d29";
pub const READING_ALGORITHM_VERSION: &str = "rosu-reading-pp-rework-202607-v1";
pub const OVERLAP_ALGORITHM_VERSION: &str = "overlap-visibility-spatial-strain-v1";
