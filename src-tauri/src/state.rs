//! 应用级共享状态及其初始化逻辑。

use std::{
    path::Path,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use tokio::sync::{Mutex as AsyncMutex, oneshot};

use crate::{
    error::CommandResult,
    features::{
        account::{AvatarCache, CredentialStore},
        beatmaphub::BeatmapHubService,
        collections::CollectionService,
        danser::DanserRuntime,
        game_session::{GameMonitorRuntime, GameSessionRuntime},
        local_analysis::LocalAnalysisService,
        obs::ObsRuntime,
        online_beatmaps::{OnlineArtworkCache, providers::ProviderRegistry},
        overlay::irc::OverlayRuntime,
        similarity::SimilarityRuntime,
        skin_workshop::SkinWorkshopService,
        tablet_driver::OtdRuntime,
        tosu::TosuRuntime,
        translation::TranslationService,
    },
    infrastructure::{osu_api::OsuApi, storage::StateStore},
};

/// 进行中的一次 OAuth 授权流程：`state` 为随授权请求发送的 OAuth state 参数，
/// `cancel` 用于中止仍在等待回调的登录任务。
#[derive(Default)]
pub struct OAuthRuntime {
    pub cancel: Option<oneshot::Sender<()>>,
    pub state: Option<String>,
}

/// 应用级共享状态：聚合各 feature 的服务与运行时句柄，在 `setup` 阶段构建后
/// 交给 Tauri 管理（`app.manage`），命令层通过 `State<AppState>` 访问。
pub struct AppState {
    pub local_database: Arc<crate::features::local_database::LocalDatabaseService>,
    pub career: Arc<crate::features::career::CareerService>,
    pub api: OsuApi,
    pub providers: ProviderRegistry,
    pub online_artwork: OnlineArtworkCache,
    pub avatar_cache: AvatarCache,
    pub credentials: CredentialStore,
    pub local_analysis: Arc<LocalAnalysisService>,
    pub skin_workshop: Arc<SkinWorkshopService>,
    pub collections: Arc<CollectionService>,
    pub local_scores: crate::features::local_scores::LocalScoreService,
    pub beatmaphub: Arc<BeatmapHubService>,
    pub similarity: Arc<SimilarityRuntime>,
    pub store: Arc<StateStore>,
    pub oauth: Mutex<OAuthRuntime>,
    /// 进行中谱面下载的取消标记，任务侧持有 `Arc` 克隆；`None` 表示当前无下载任务。
    pub beatmap_download: Mutex<Option<Arc<AtomicBool>>>,
    /// 集合后台任务（谱面补齐下载等长任务）的取消标记。
    pub collection_task_cancel: AtomicBool,
    /// 令牌刷新互斥锁：并发触发刷新时只放行一个任务真正请求令牌端点。
    pub token_refresh: AsyncMutex<()>,
    pub game_session: GameSessionRuntime,
    pub game_monitor: Arc<GameMonitorRuntime>,
    pub danser: Arc<DanserRuntime>,
    pub tosu: Arc<TosuRuntime>,
    pub overlay: Arc<OverlayRuntime>,
    pub translation: Arc<TranslationService>,
    pub otd: Arc<OtdRuntime>,
    pub obs: Arc<ObsRuntime>,
    pub music: crate::features::music_player::MusicRuntime,
    pub music_only: AtomicBool,
    pub music_frontend_task: AtomicBool,
}

impl AppState {
    /// 组装全部共享服务并建立它们之间的依赖关系；整体包裹在一个日志 span 中，
    /// 任一步失败都会记录错误并以 `CommandError` 返回。
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        let span = crate::infrastructure::logging::global()
            .map(|logger| logger.operation("app.lifecycle", "initialize_services"));
        // 闭包包住全部组装步骤，统一交给 `finish_span` 记录成败与耗时。
        let result = (|| {
            let store = Arc::new(StateStore::load(app_data_dir)?);
            let local_analysis = Arc::new(LocalAnalysisService::new(app_data_dir)?);
            let local_database = Arc::new(
                crate::features::local_database::LocalDatabaseService::new(app_data_dir),
            );
            let career = Arc::new(crate::features::career::CareerService::new(Arc::clone(
                &local_database,
            )));
            local_analysis.attach_database(Arc::clone(&local_database))?;
            let skin_workshop = Arc::new(SkinWorkshopService::new(
                app_data_dir,
                Arc::clone(&local_analysis),
            )?);
            let collections = Arc::new(CollectionService::new(app_data_dir)?);
            let beatmaphub = Arc::new(BeatmapHubService::new(app_data_dir)?);
            local_analysis
                .set_thumbnail_cache_limit_mb(store.settings_snapshot()?.cache_limit_mb)?;
            Ok(Self {
                local_database,
                career,
                api: OsuApi::new()?,
                providers: ProviderRegistry::new()?,
                online_artwork: OnlineArtworkCache::new(app_data_dir)?,
                avatar_cache: AvatarCache::new(app_data_dir)?,
                credentials: CredentialStore,
                local_analysis,
                skin_workshop,
                collections,
                local_scores: Default::default(),
                beatmaphub,
                similarity: Arc::new(SimilarityRuntime::default()),
                store,
                oauth: Mutex::new(OAuthRuntime::default()),
                beatmap_download: Mutex::new(None),
                collection_task_cancel: AtomicBool::new(false),
                token_refresh: AsyncMutex::new(()),
                game_session: GameSessionRuntime::default(),
                game_monitor: Arc::new(GameMonitorRuntime::default()),
                danser: Arc::new(DanserRuntime::default()),
                tosu: Arc::new(TosuRuntime::default()),
                overlay: Arc::new(OverlayRuntime::default()),
                translation: Arc::new(TranslationService::new()),
                otd: Arc::new(OtdRuntime::default()),
                obs: Arc::new(ObsRuntime::default()),
                music: crate::features::music_player::MusicRuntime::new(app_data_dir)?,
                music_only: AtomicBool::new(false),
                music_frontend_task: AtomicBool::new(false),
            })
        })();
        crate::infrastructure::logging::finish_span(span, result)
    }
}
