//! Bootstrap config remains outside the database so an unavailable disk is recoverable.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::{CommandError, CommandResult},
    infrastructure::local_database::fs_io,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DatabaseConfig {
    pub(super) version: u32,
    pub(super) directory: PathBuf,
    pub(super) database_uuid: String,
}

impl DatabaseConfig {
    pub(super) fn load(path: &Path) -> CommandResult<Option<Self>> {
        let backup = path.with_extension("json.bak");
        let primary_exists = fs_io("exists", path, path.try_exists())?;
        let source = if primary_exists {
            path
        } else if fs_io("exists", &backup, backup.try_exists())? {
            &backup
        } else {
            return Ok(None);
        };
        // A present but invalid config must never turn into a fresh-library prompt.
        let bytes = fs_io("read", source, fs::read(source))?;
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|error| CommandError::from_error("DATABASE_CONFIG_INVALID", error))?;
        if config.version != 1
            || !config.directory.is_absolute()
            || Uuid::parse_str(&config.database_uuid).is_err()
        {
            return Err(CommandError::new(
                "DATABASE_CONFIG_INVALID",
                "数据库位置配置无效，请保留配置文件并恢复备份",
            ));
        }
        // Recover an interrupted Windows rename, preserving its original UUID.
        if !primary_exists {
            config.save(path)?;
        }
        Ok(Some(config))
    }

    pub(super) fn save(&self, path: &Path) -> CommandResult<()> {
        let bytes = serde_json::to_vec_pretty(self)?;
        let temporary = path.with_extension("json.tmp");
        let backup = path.with_extension("json.bak");
        let mut file = fs_io("create", &temporary, fs::File::create(&temporary))?;
        fs_io("write", &temporary, file.write_all(&bytes))?;
        fs_io("sync", &temporary, file.sync_all())?;
        drop(file);
        let had_primary = fs_io("exists", path, path.try_exists())?;
        if had_primary {
            if fs_io("exists", &backup, backup.try_exists())? {
                fs_io("remove", &backup, fs::remove_file(&backup))?;
            }
            fs_io("rename_to_backup", path, fs::rename(path, &backup))?;
        }
        if let Err(error) = fs_io("publish", path, fs::rename(&temporary, path)) {
            if had_primary {
                let _ = fs_io("restore_backup", path, fs::rename(&backup, path));
            }
            return Err(error);
        }
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            let directory = fs_io("open_directory", parent, fs::File::open(parent))?;
            fs_io("sync_directory", parent, directory.sync_all())?;
        }
        // Keep the previous config as a recovery aid. Loading prefers the primary.
        Ok(())
    }
}
