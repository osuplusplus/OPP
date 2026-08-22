//! Build the immutable DT/HT feature sidecar for an existing Mania v1 dataset.
//!
//! Usage:
//!   cargo run --example build_mod_features -- <dataset-root> <mania-beatmaps>

use std::{env, fs, io::Write, path::PathBuf};

use osu_difficulty_runtime::{ManiaDataset, ManiaGameMod, ManiaModFeatureRecord};
use sha2::{Digest, Sha256};

const HEADER: &[u8; 8] = b"ODLMMV1\0";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let dataset_root = PathBuf::from(args.next().ok_or("missing dataset root")?);
    let source_root = PathBuf::from(args.next().ok_or("missing mania-beatmaps directory")?);
    let dataset = ManiaDataset::open(&dataset_root)?;
    let mut output = fs::File::create(dataset_root.join("mania-mod-features-v1.bin"))?;
    output.write_all(HEADER)?;
    let mut count = 0usize;
    for entry in fs::read_dir(source_root)? {
        let path = entry?.path();
        if path.extension().and_then(|v| v.to_str()) != Some("osu") {
            continue;
        }
        let filename_id = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .parse::<u64>()
            .ok();
        let bytes = fs::read(&path)?;
        let checksum = hex::encode(Sha256::digest(&bytes));
        let beatmap_id = dataset
            .beatmap_id_for_checksum(&checksum)
            .or_else(|| filename_id.filter(|id| dataset.contains(*id)));
        let Some(beatmap_id) = beatmap_id else {
            continue;
        };
        if !dataset.contains(beatmap_id) {
            continue;
        }
        for game_mod in [ManiaGameMod::Dt, ManiaGameMod::Ht] {
            let target = dataset.analyze_target_with_mod(&bytes, Some(beatmap_id), game_mod)?;
            let entry = ManiaModFeatureRecord {
                beatmap_id,
                game_mod,
                record: target.record,
            };
            bincode::serialize_into(&mut output, &entry)?;
            count += 1;
        }
    }
    output.sync_all()?;
    eprintln!("wrote {count} DT/HT records to {}", dataset_root.display());
    Ok(())
}
