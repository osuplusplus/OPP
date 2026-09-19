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
        similarity::SimilarityRuntime,
        skin_workshop::SkinWorkshopService,
        tablet_driver::OtdRuntime,
        tosu::TosuRuntime,
    },
    infrastructure::{osu_api::OsuApi, storage::StateStore},
};

#[derive(Default)]
pub struct OAuthRuntime {
    pub cancel: Option<oneshot::Sender<()>>,
    pub state: Option<String>,
}

pub struct AppState {
    pub api: OsuApi,
    pub providers: ProviderRegistry,
    pub online_artwork: OnlineArtworkCache,
    pub avatar_cache: AvatarCache,
    pub credentials: CredentialStore,
    pub local_analysis: Arc<LocalAnalysisService>,
    pub skin_workshop: Arc<SkinWorkshopService>,
    pub collections: Arc<CollectionService>,
    pub beatmaphub: Arc<BeatmapHubService>,
    pub similarity: Arc<SimilarityRuntime>,
    pub store: Arc<StateStore>,
    pub oauth: Mutex<OAuthRuntime>,
    pub beatmap_download: Mutex<Option<Arc<AtomicBool>>>,
    pub collection_task_cancel: AtomicBool,
    pub token_refresh: AsyncMutex<()>,
    pub game_session: GameSessionRuntime,
    pub game_monitor: Arc<GameMonitorRuntime>,
    pub danser: Arc<DanserRuntime>,
    pub tosu: Arc<TosuRuntime>,
    pub otd: Arc<OtdRuntime>,
    pub obs: Arc<ObsRuntime>,
    pub music: crate::features::music_player::MusicRuntime,
    pub music_only: AtomicBool,
    pub music_frontend_task: AtomicBool,
}

impl AppState {
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        let span = crate::infrastructure::logging::global()
            .map(|logger| logger.operation("app.lifecycle", "initialize_services"));
        let result = (|| {
            let store = Arc::new(StateStore::load(app_data_dir)?);
            let local_analysis = Arc::new(LocalAnalysisService::new(app_data_dir)?);
            let skin_workshop = Arc::new(SkinWorkshopService::new(
                app_data_dir,
                Arc::clone(&local_analysis),
            )?);
            let collections = Arc::new(CollectionService::new(app_data_dir)?);
            let beatmaphub = Arc::new(BeatmapHubService::new(app_data_dir)?);
            local_analysis
                .set_thumbnail_cache_limit_mb(store.settings_snapshot()?.cache_limit_mb)?;
            Ok(Self {
                api: OsuApi::new()?,
                providers: ProviderRegistry::new()?,
                online_artwork: OnlineArtworkCache::new(app_data_dir)?,
                avatar_cache: AvatarCache::new(app_data_dir)?,
                credentials: CredentialStore,
                local_analysis,
                skin_workshop,
                collections,
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
