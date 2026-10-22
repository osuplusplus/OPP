//! 本地数据库服务：管理“读配置 → 打开数据库 → 错误恢复”的状态机。
//! 约束：已有配置（即使库当前不可用）绝不允许被覆盖式初始化，防止静默丢库。
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use crate::{
    error::{CommandError, CommandResult},
    infrastructure::{
        local_database::{self, LibraryDatabase, fs_io},
        logging::{finish_span, global},
        platform,
    },
};

use super::{
    config::DatabaseConfig,
    models::{DatabasePhase, LocalDatabaseStatus},
};

/// 惰性加载的运行时状态；`loaded` 保证首次访问后才读磁盘，启动线程不做任何数据库 IO。
#[derive(Default)]
struct Runtime {
    loaded: bool,
    /// 引导配置文件损坏（而非缺失），此状态下禁止覆盖式初始化。
    config_error: bool,
    config: Option<DatabaseConfig>,
    database: Option<LibraryDatabase>,
    error: Option<CommandError>,
}

/// 本地数据库状态机；所有方法先取 `runtime` 锁并确保已加载，再按状态分派。
pub(crate) struct LocalDatabaseService {
    config_path: PathBuf,
    recommended: PathBuf,
    runtime: Mutex<Runtime>,
}

impl LocalDatabaseService {
    /// 返回已配置的库目录，供其他 feature 定位数据文件；未配置时为 `None`。
    pub(crate) fn configured_directory(&self) -> CommandResult<Option<PathBuf>> {
        let mut runtime = self.lock()?;
        self.ensure_loaded(&mut runtime);
        Ok(runtime
            .config
            .as_ref()
            .map(|config| config.directory.clone()))
    }

    /// 在已打开的数据库上执行操作：就绪时返回 `Some(结果)`；未配置返回 `Ok(None)`；
    /// 处于错误态则返回先前记录的错误，让调用方感知失败原因。
    pub(crate) fn with_database<T>(
        &self,
        operation: &str,
        work: impl FnOnce(&mut LibraryDatabase) -> CommandResult<T>,
    ) -> CommandResult<Option<T>> {
        let mut runtime = self.lock()?;
        self.ensure_loaded(&mut runtime);
        let Some(database) = runtime.database.as_mut() else {
            return match &runtime.error {
                Some(error) => Err(error.clone()),
                None => Ok(None),
            };
        };
        let span = global().map(|logger| logger.operation("local_database", operation));
        finish_span(span, work(database)).map(Some)
    }
    // No database IO on the Tauri setup thread; the startup query opens it on the IO pool.
    /// 配置文件固定为 `<应用数据目录>/local-database.json`，推荐目录为 `<应用数据目录>/datasets`。
    pub(crate) fn new(app_data_dir: &Path) -> Self {
        Self {
            config_path: app_data_dir.join("local-database.json"),
            recommended: app_data_dir.join("datasets"),
            runtime: Mutex::new(Runtime::default()),
        }
    }

    /// 返回数据库状态快照（首次调用会触发一次加载）。
    pub(crate) fn status(&self) -> CommandResult<LocalDatabaseStatus> {
        let mut runtime = self.lock()?;
        self.ensure_loaded(&mut runtime);
        Ok(self.snapshot(&runtime))
    }

    /// 首次初始化：仅在完全未配置且配置文件未损坏时允许；校验并创建目录、建库、写配置。
    /// 指向同一目录的重复调用幂等返回就绪，其余情况一律拒绝，防止覆盖既有库。
    pub(crate) fn initialize(&self, directory: &str) -> CommandResult<LocalDatabaseStatus> {
        let mut runtime = self.lock()?;
        self.ensure_loaded(&mut runtime);
        // Existing configuration (including offline/corrupt databases) is never replaced here.
        if let Some(config) = &runtime.config {
            let selected = self.resolve_directory(directory, false)?;
            if selected == config.directory && runtime.database.is_some() {
                return Ok(self.snapshot(&runtime));
            }
            return Err(CommandError::new(
                "DATABASE_ALREADY_CONFIGURED",
                "已有数据库配置；路径失效时请重试或重新定位原数据库",
            ));
        }
        // Invalid bootstrap config is an error, not permission to overwrite it.
        if runtime.config_error {
            return Err(runtime.error.clone().expect("checked error"));
        }
        let result = (|| {
            let directory = self.resolve_directory(directory, true)?;
            let database = local_database::open(&directory, true, None)?;
            let config = DatabaseConfig {
                version: 1,
                directory,
                database_uuid: database.uuid.clone(),
            };
            // A failed config write leaves the valid library available for explicit retry.
            config.save(&self.config_path)?;
            runtime.config = Some(config);
            runtime.database = Some(database);
            Ok(())
        })();
        self.complete(&mut runtime, result)
    }

    /// 关闭连接并从磁盘重新加载配置与数据库，用于路径恢复、文件解锁后的重试。
    pub(crate) fn retry(&self) -> CommandResult<LocalDatabaseStatus> {
        let mut runtime = self.lock()?;
        self.close(&mut runtime);
        runtime.loaded = false;
        self.ensure_loaded(&mut runtime);
        Ok(self.snapshot(&runtime))
    }

    /// 重新定位：到新目录按 UUID 找回“同一个”数据库并更新配置；数据库正常使用中禁止换位置。
    pub(crate) fn locate(&self, directory: &str) -> CommandResult<LocalDatabaseStatus> {
        let mut runtime = self.lock()?;
        self.ensure_loaded(&mut runtime);
        if runtime.database.is_some() {
            return Err(CommandError::new(
                "DATABASE_ALREADY_CONFIGURED",
                "数据库正在正常使用，本期不支持更换保存位置",
            ));
        }
        let config = runtime.config.clone().ok_or_else(|| {
            CommandError::new(
                "DATABASE_NOT_CONFIGURED",
                "没有可恢复的数据库身份，请先完成首次配置或恢复位置配置文件",
            )
        })?;
        let result = (|| {
            let directory = self.resolve_directory(directory, false)?;
            let database = local_database::open_for_recovery(&directory, &config.database_uuid)?;
            let config = DatabaseConfig {
                directory,
                ..config
            };
            config.save(&self.config_path)?;
            runtime.config = Some(config);
            runtime.database = Some(database);
            Ok(())
        })();
        self.complete(&mut runtime, result)
    }

    /// 应用退出时关闭数据库连接（尽力而为，拿不到锁则跳过）。
    pub(crate) fn shutdown(&self) {
        if let Ok(mut runtime) = self.lock() {
            self.close(&mut runtime);
        }
    }

    fn close(&self, runtime: &mut Runtime) {
        if let Some(database) = runtime.database.take() {
            let span = global().map(|logger| logger.operation("local_database", "close_library"));
            let result = database
                .connection
                .close()
                .map_err(|(_, error)| local_database::sql_error(error));
            let _ = finish_span(span, result);
        }
    }

    /// 惰性加载的唯一入口：读取配置（损坏则记 `config_error` 并保留错误），
    /// 再按配置打开数据库并校验 UUID；任何失败只记入 `runtime.error`，不 panic。
    fn ensure_loaded(&self, runtime: &mut Runtime) {
        if runtime.loaded {
            return;
        }
        runtime.loaded = true;
        runtime.config = None;
        runtime.error = None;
        runtime.config_error = false;
        let result = (|| {
            runtime.config = match DatabaseConfig::load(&self.config_path) {
                Ok(config) => config,
                Err(error) => {
                    runtime.config_error = true;
                    return Err(error);
                }
            };
            if let Some(config) = &runtime.config {
                let directory =
                    self.resolve_directory(&config.directory.to_string_lossy(), false)?;
                runtime.database = Some(local_database::open(
                    &directory,
                    false,
                    Some(&config.database_uuid),
                )?);
            }
            Ok(())
        })();
        if let Err(error) = result {
            runtime.error = Some(error);
        }
    }

    /// 校验并规范化目录：必须是本机绝对路径的文件夹（拒绝 UNC/网络映射盘）。
    /// `create` 模式下先找到已存在的祖先并校验后才创建目录，防止借新建子目录绕过网络盘检查；
    /// 创建后对规范化结果再验一次，保证最终落库的路径仍是本地路径。
    fn resolve_directory(&self, directory: &str, create: bool) -> CommandResult<PathBuf> {
        let path = Path::new(directory);
        if directory.trim().is_empty() || !path.is_absolute() {
            return Err(CommandError::new(
                "DATABASE_PATH_INVALID",
                "请选择绝对路径的本机文件夹",
            ));
        }
        // Reject UNC/mapped network drives before even creating a directory.
        platform::validate_local_database_directory(path)?;
        if create {
            let mut ancestor = path;
            while !fs_io("exists", ancestor, ancestor.try_exists())? {
                ancestor = ancestor.parent().ok_or_else(|| {
                    CommandError::new("DATABASE_PATH_INVALID", "无法定位保存目录所在磁盘")
                })?;
            }
            let resolved = fs_io(
                "canonicalize_ancestor",
                ancestor,
                fs::canonicalize(ancestor),
            )?;
            platform::validate_local_database_directory(&resolved)?;
        }
        if create {
            fs_io("create_directory", path, fs::create_dir_all(path))?;
        }
        let canonical = fs_io("canonicalize", path, fs::canonicalize(path))?;
        platform::validate_local_database_directory(&canonical)?;
        let metadata = fs_io("metadata", &canonical, fs::metadata(&canonical))?;
        if !metadata.is_dir() {
            return Err(CommandError::new(
                "DATABASE_PATH_INVALID",
                "请选择文件夹而不是文件",
            ));
        }
        Ok(canonical)
    }

    /// 状态变更操作的结果统一入口：成功清空错误，失败则记录并原样返回。
    fn complete(
        &self,
        runtime: &mut Runtime,
        result: CommandResult<()>,
    ) -> CommandResult<LocalDatabaseStatus> {
        match result {
            Ok(()) => {
                runtime.error = None;
                Ok(self.snapshot(runtime))
            }
            Err(error) => {
                runtime.error = Some(error.clone());
                Err(error)
            }
        }
    }

    /// 由运行时状态推导对外快照：阶段优先级为 就绪 > 错误 > 未配置。
    fn snapshot(&self, runtime: &Runtime) -> LocalDatabaseStatus {
        LocalDatabaseStatus {
            phase: if runtime.database.is_some() {
                DatabasePhase::Ready
            } else if runtime.error.is_some() {
                DatabasePhase::Error
            } else {
                DatabasePhase::Unconfigured
            },
            directory: runtime
                .config
                .as_ref()
                .map(|config| config.directory.to_string_lossy().into_owned()),
            recommended_directory: self.recommended.to_string_lossy().into_owned(),
            database_uuid: runtime
                .config
                .as_ref()
                .map(|config| config.database_uuid.clone()),
            schema_version: runtime.database.as_ref().map(|database| database.version),
            can_initialize: runtime.config.is_none() && !runtime.config_error,
            error: runtime.error.clone(),
        }
    }

    /// 获取运行时锁；Mutex 中毒（持锁线程 panic）视为不可恢复，提示重启应用。
    fn lock(&self) -> CommandResult<MutexGuard<'_, Runtime>> {
        self.runtime.lock().map_err(|_| {
            CommandError::new("DATABASE_STATE_ERROR", "数据库运行状态异常，请重启 OPP")
        })
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
