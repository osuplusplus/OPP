use std::{
    path::Path,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use tokio::sync::{Mutex as AsyncMutex, oneshot};

use crate::{
    account::{AvatarCache, CredentialStore},
    beatmaphub::BeatmapHubService,
    collections::CollectionService,
    danser::DanserRuntime,
    error::CommandResult,
    game_session::{GameMonitorRuntime, GameSessionRuntime},
    local_analysis::LocalAnalysisService,
    obs::ObsRuntime,
    online_beatmaps::providers::ProviderRegistry,
    osu_api::OsuApi,
    similarity::SimilarityRuntime,
    skin_workshop::SkinWorkshopService,
    storage::StateStore,
    tosu::TosuRuntime,
};

#[derive(Default)]
pub struct OAuthRuntime {
    pub cancel: Option<oneshot::Sender<()>>,
    pub state: Option<String>,
}

pub struct AppState {
    pub api: OsuApi,
    pub providers: ProviderRegistry,
    pub avatar_cache: AvatarCache,
    pub credentials: CredentialStore,
    pub local_analysis: Arc<LocalAnalysisService>,
    pub skin_workshop: Arc<SkinWorkshopService>,
    pub collections: Arc<CollectionService>,
    pub beatmaphub: Arc<BeatmapHubService>,
    pub similarity: Arc<SimilarityRuntime>,
    pub store: StateStore,
    pub oauth: Mutex<OAuthRuntime>,
    pub beatmap_download: Mutex<Option<Arc<AtomicBool>>>,
    pub collection_task_cancel: AtomicBool,
    pub token_refresh: AsyncMutex<()>,
    pub game_session: GameSessionRuntime,
    pub game_monitor: Arc<GameMonitorRuntime>,
    pub danser: Arc<DanserRuntime>,
    pub tosu: Arc<TosuRuntime>,
    pub obs: Arc<ObsRuntime>,
}

impl AppState {
    pub fn new(app_data_dir: &Path) -> CommandResult<Self> {
        let store = StateStore::load(app_data_dir)?;
        let local_analysis = Arc::new(LocalAnalysisService::new(app_data_dir)?);
        let skin_workshop = Arc::new(SkinWorkshopService::new(
            app_data_dir,
            Arc::clone(&local_analysis),
        )?);
        let collections = Arc::new(CollectionService::new(app_data_dir)?);
        let beatmaphub = Arc::new(BeatmapHubService::new(app_data_dir)?);
        local_analysis.set_thumbnail_cache_limit_mb(store.snapshot()?.settings.cache_limit_mb)?;
        Ok(Self {
            api: OsuApi::new()?,
            providers: ProviderRegistry::new()?,
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
            obs: Arc::new(ObsRuntime::default()),
        })
    }
}
