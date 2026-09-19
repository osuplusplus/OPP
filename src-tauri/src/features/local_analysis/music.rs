//! Small resource descriptors that remain usable after the analysis index is released.
use super::LocalClient;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MusicAsset {
    pub client: LocalClient,
    pub resource_id: String,
    pub root: PathBuf,
    pub audio: PathBuf,
    pub artwork: Option<PathBuf>,
    pub beatmap: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct MusicCandidate {
    pub client: LocalClient,
    pub resource_id: String,
    pub set_key: String,
    pub set_id: Option<i32>,
    pub beatmap_id: Option<i32>,
    pub checksum: Option<String>,
    pub title: String,
    pub artist: String,
    pub matches: bool,
    pub asset: Option<MusicAsset>,
}
