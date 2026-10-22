//! local_analysis 的 Tauri 命令入口：反序列化参数，按任务形态调度到
//! 交互 / 阻塞 IO / 后台线程池，并统一以 `CommandResult` 返回可展示的错误。
use std::{path::Path, sync::Arc};

use super::models::LocalArtwork;
use super::models::{BackgroundSize, LocalBeatmapAudioPayload};
use super::models::{
    BeatmapQuery, LocalBeatmapDetail, LocalBeatmapSetSummary, LocalBeatmapSummary, LocalClient,
    LocalIndexLoadStatus, LocalLibrarySummary, LocalScanProgress, LocalSkinAssetPayload,
    LocalSkinDetail, LocalSkinPreview, LocalSkinSummary, LocalSourceStatus, Page, SkinQuery,
};
use crate::domain::Ruleset;
use tauri::{AppHandle, Emitter, State};

use crate::{
    error::{CommandError, CommandResult},
    state::AppState,
};

/// 批量查询谱面 ID 的本地存在状态（单次上限 1000 个）；
/// `client` 缺省时同时检查 stable 与 lazer。
#[tauri::command]
pub async fn query_local_beatmap_presence(
    ids: Vec<i32>,
    client: Option<LocalClient>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<super::models::LocalBeatmapPresence>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::blocking_io("query_local_beatmap_presence", move || {
        let span = crate::infrastructure::logging::global()
            .map(|log| log.operation("local_analysis.command", "beatmap_presence"));
        crate::infrastructure::logging::finish_span(span, service.beatmap_presence(ids, client))
    })
    .await?
}

/// 返回各客户端索引缓存的存储状态（`database` / `json` / `unscanned`）。
#[tauri::command]
pub async fn get_local_library_storage_status(
    state: State<'_, AppState>,
) -> CommandResult<Vec<super::models::LocalLibraryStorageStatus>> {
    let span = crate::infrastructure::logging::global()
        .map(|log| log.operation("local_analysis.command", "storage_status"));
    crate::infrastructure::logging::finish_span(span, state.local_analysis.library_storage_status())
}

/// 恢复缺失客户端的缓存索引并迁移到 SQLite 存储，返回迁移后的存储状态。
#[tauri::command]
pub async fn migrate_local_library_database(
    state: State<'_, AppState>,
) -> CommandResult<Vec<super::models::LocalLibraryStorageStatus>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::blocking_io("migrate_local_library_database", move || {
        let span = crate::infrastructure::logging::global()
            .map(|log| log.operation("local_analysis.command", "migrate_library"));
        crate::infrastructure::logging::finish_span(span, service.migrate_cached_indexes())
    })
    .await?
}

/// 返回 stable / lazer 两个数据源的探测状态（含覆盖模式与校验错误）。
#[tauri::command(async)]
pub async fn get_local_sources(
    state: State<'_, AppState>,
) -> CommandResult<Vec<LocalSourceStatus>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("get_local_sources", move || {
        service.source_statuses()
    })
    .await?
}

/// 获取最多 24 条去重后的背景图样本引用；结果在内存中缓存。
#[tauri::command]
pub async fn get_local_artwork_sample(
    state: State<'_, AppState>,
) -> CommandResult<Vec<LocalArtwork>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::blocking_io("get_local_artwork_sample", move || {
        let span = crate::infrastructure::logging::global()
            .map(|log| log.operation("local_analysis", "get_local_artwork_sample"));
        crate::infrastructure::logging::finish_span(span, service.artwork_sample())
    })
    .await?
}

/// 读取索引后台加载阶段与各客户端的监听 / 增量扫描状态。
#[tauri::command(async)]
pub fn get_local_index_status(state: State<'_, AppState>) -> CommandResult<LocalIndexLoadStatus> {
    state.local_analysis.index_load_status()
}

/// 手动指定客户端目录（stable 安装目录或 lazer 数据目录）；
/// 探测校验通过才会写入覆盖配置。
#[tauri::command(async)]
pub async fn set_local_source(
    client: LocalClient,
    path: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalSourceStatus> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("set_local_source", move || {
        service.set_source(client, Path::new(path.trim()))
    })
    .await?
}

/// 清除手动覆盖，恢复自动探测并返回新的数据源状态。
#[tauri::command(async)]
pub async fn reset_local_source(
    client: LocalClient,
    state: State<'_, AppState>,
) -> CommandResult<LocalSourceStatus> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("reset_local_source", move || {
        service.reset_source(client)
    })
    .await?
}

/// 读取该客户端最近一次扫描的库汇总；未扫描或数据源已变化时为 `None`。
#[tauri::command(async)]
pub fn get_local_summary(
    client: LocalClient,
    state: State<'_, AppState>,
) -> CommandResult<Option<LocalLibrarySummary>> {
    state.local_analysis.summary(client)
}

/// 扫描指定客户端并重建索引；`force` 为 true 时忽略增量复用与 lazer 的
/// Realm 指纹缓存。进度通过 `local-scan-progress` 事件推送给前端。
#[tauri::command]
/// 供前端调用的 Tauri 命令：完成该功能模块的业务操作。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn scan_local_source(
    client: LocalClient,
    force: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<LocalLibrarySummary> {
    let service = Arc::clone(&state.local_analysis);
    let emit_event = Arc::new(move |progress: LocalScanProgress| {
        let _ = app.emit("local-scan-progress", progress);
    });
    tokio::task::spawn_blocking(move || service.scan(client, force, emit_event))
        .await
        .map_err(|error| {
            CommandError::new(
                "LOCAL_SCAN_TASK_ERROR",
                format!("本地扫描任务异常结束：{error}"),
            )
        })?
}

/// 请求取消指定客户端正在进行的扫描；当前无扫描时静默成功。
#[tauri::command]
/// 供前端调用的 Tauri 命令：请求取消正在进行的任务。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub fn cancel_local_scan(client: LocalClient, state: State<'_, AppState>) -> CommandResult<()> {
    state.local_analysis.cancel_scan(client)
}

/// 按筛选、排序与分页条件查询本地谱面列表。
#[tauri::command(async)]
pub async fn query_local_beatmaps(
    query: BeatmapQuery,
    state: State<'_, AppState>,
) -> CommandResult<Page<LocalBeatmapSummary>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("query_local_beatmaps", move || {
        service.query_beatmaps(query)
    })
    .await?
}

/// 按条件查询谱面集分页列表；集内难度列表同样应用查询过滤。
#[tauri::command(async)]
pub async fn query_local_beatmap_sets(
    query: BeatmapQuery,
    state: State<'_, AppState>,
) -> CommandResult<Page<LocalBeatmapSetSummary>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("query_local_beatmap_sets", move || {
        service.query_beatmap_sets(query)
    })
    .await?
}

/// 读取单张谱面详情，并现场计算 strain 难度曲线。
#[tauri::command(async)]
pub async fn get_local_beatmap_detail(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalBeatmapDetail> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("get_local_beatmap_detail", move || {
        service.beatmap_detail(client, &resource_id)
    })
    .await?
}

/// 获取谱面文件的磁盘路径；lazer 会先按 Realm 清单物化成 Stable 式目录。
#[tauri::command(async)]
pub async fn get_local_beatmap_path(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("get_local_beatmap_path", move || {
        service.beatmap_file_path(client, &resource_id)
    })
    .await?
}

/// 读取谱面背景缩略图（JPEG data URL）；相同键的并发请求会被合并执行，
/// 缩略图按文件内容与修改时间磁盘缓存。
#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_local_beatmap_background(
    client: LocalClient,
    resource_id: String,
    size: Option<BackgroundSize>,
    state: State<'_, AppState>,
) -> CommandResult<Option<String>> {
    let service = Arc::clone(&state.local_analysis);
    let key = (
        client,
        resource_id.clone(),
        matches!(size, Some(BackgroundSize::Stage)),
        service.background_revision(client)?,
    );
    service
        .background_requests
        .run(key, || async {
            let service = Arc::clone(&service);
            crate::infrastructure::tasks::interactive("local_analysis", move || {
                let span = crate::infrastructure::logging::global()
                    .map(|log| log.operation("local_analysis", "get_local_beatmap_background"));
                let result = match size.unwrap_or_default() {
                    BackgroundSize::Thumbnail => service.beatmap_background(client, &resource_id),
                    BackgroundSize::Stage => service.beatmap_background_sized(
                        client,
                        &resource_id,
                        BackgroundSize::Stage,
                    ),
                };
                crate::infrastructure::logging::finish_span(span, result)
            })
            .await
            .map_err(|error| {
                CommandError::new(
                    "LOCAL_BACKGROUND_TASK_ERROR",
                    format!("谱面背景处理任务异常结束：{error}"),
                )
            })?
        })
        .await
}

/// 按 `set_key` 与规则集获取完整谱面集汇总。
#[tauri::command(async)]
pub async fn get_local_beatmap_set(
    client: LocalClient,
    set_key: String,
    ruleset: Ruleset,
    state: State<'_, AppState>,
) -> CommandResult<LocalBeatmapSetSummary> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("get_local_beatmap_set", move || {
        service.complete_beatmap_set(client, &set_key, ruleset)
    })
    .await?
}

/// 在满足查询条件的谱面集中随机抽取一个，可排除刚抽过的集合。
#[tauri::command(async)]
pub async fn pick_random_local_beatmap_set(
    query: BeatmapQuery,
    exclude_set_key: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Option<LocalBeatmapSetSummary>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("pick_random_local_beatmap_set", move || {
        service.random_beatmap_set(query, exclude_set_key.as_deref())
    })
    .await?
}

/// 读取谱面音频并内联为 base64 载荷用于试听。
#[tauri::command]
pub async fn get_local_beatmap_audio(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalBeatmapAudioPayload> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("beatmap_audio", move || {
        service.beatmap_audio(client, &resource_id)
    })
    .await
    .map_err(|e| CommandError::new("LOCAL_AUDIO_TASK_ERROR", e.to_string()))?
}

/// 将谱面集导出为 `.osz` 文件并返回导出路径；lazer 按 Realm 清单
/// 从内容寻址存储取回原始文件。
#[tauri::command]
/// 供前端调用的 Tauri 命令：将资源导出到用户指定的位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn export_local_beatmap_set(
    client: LocalClient,
    set_key: String,
    out_dir: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::background("local_analysis", move || {
        service.export_beatmap_set_osz(client, &set_key, Path::new(&out_dir))
    })
    .await
    .map_err(|error| {
        CommandError::new(
            "LOCAL_EXPORT_TASK_ERROR",
            format!("谱面集导出任务异常结束：{error}"),
        )
    })?
}

/// 将皮肤导出为 `.osk` 文件并返回导出路径。
#[tauri::command]
/// 供前端调用的 Tauri 命令：将资源导出到用户指定的位置。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn export_local_skin(
    client: LocalClient,
    skin_resource_id: String,
    out_dir: String,
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::background("local_analysis", move || {
        service.export_skin_osk(client, &skin_resource_id, Path::new(&out_dir))
    })
    .await
    .map_err(|error| {
        CommandError::new(
            "LOCAL_EXPORT_TASK_ERROR",
            format!("Skin 导出任务异常结束：{error}"),
        )
    })?
}

/// 按条件查询皮肤分页列表（搜索匹配名称 / 作者 / 版本）。
#[tauri::command(async)]
pub async fn query_local_skins(
    query: SkinQuery,
    state: State<'_, AppState>,
) -> CommandResult<Page<LocalSkinSummary>> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("query_local_skins", move || {
        service.query_skins(query)
    })
    .await?
}

/// 读取皮肤详情（配置分节与可选资源统计）。
#[tauri::command(async)]
pub async fn get_local_skin_detail(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalSkinDetail> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("get_local_skin_detail", move || {
        service.skin_detail(client, &resource_id)
    })
    .await?
}

/// 枚举皮肤内可预览的图片与音效资源。
#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_local_skin_preview(
    client: LocalClient,
    resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalSkinPreview> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("skin_preview", move || {
        service.skin_preview(client, &resource_id)
    })
    .await
    .map_err(|error| {
        CommandError::new(
            "LOCAL_SKIN_PREVIEW_TASK_ERROR",
            format!("Skin 预览索引任务异常结束：{error}"),
        )
    })?
}

/// 读取皮肤单个资源并以 data URL 返回（图片缩放转 PNG，音效原样内联）。
#[tauri::command]
/// 供前端调用的 Tauri 命令：读取当前状态或详情。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn get_local_skin_asset(
    client: LocalClient,
    skin_resource_id: String,
    asset_resource_id: String,
    state: State<'_, AppState>,
) -> CommandResult<LocalSkinAssetPayload> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::interactive("local_analysis", move || {
        service.skin_asset(client, &skin_resource_id, &asset_resource_id)
    })
    .await
    .map_err(|error| {
        CommandError::new(
            "LOCAL_SKIN_ASSET_TASK_ERROR",
            format!("Skin 资源预览任务异常结束：{error}"),
        )
    })?
}

/// 替换 stable 皮肤中的单个资源（扩展名必须一致）；`save_as_new` 为 true
/// 时先复制为新皮肤再在副本上替换，完成后自动重新扫描。lazer 皮肤按
/// 内容哈希存储，不支持就地替换。
#[tauri::command]
/// 供前端调用的 Tauri 命令：完成该功能模块的业务操作。
/// 前端输入在命令层反序列化；失败统一通过 `CommandResult` 返回可展示的原因。
pub async fn replace_local_skin_asset(
    client: LocalClient,
    skin_resource_id: String,
    asset_resource_id: String,
    replacement_path: String,
    save_as_new: bool,
    new_skin_name: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let service = Arc::clone(&state.local_analysis);
    crate::infrastructure::tasks::background("local_analysis", move || {
        service.replace_skin_asset(
            client,
            &skin_resource_id,
            &asset_resource_id,
            Path::new(&replacement_path),
            save_as_new,
            new_skin_name.as_deref(),
        )
    })
    .await
    .map_err(|error| {
        CommandError::new(
            "LOCAL_SKIN_REPLACE_TASK_ERROR",
            format!("Skin 资源替换任务异常结束：{error}"),
        )
    })?
}
