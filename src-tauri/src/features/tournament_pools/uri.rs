use std::{collections::HashMap, sync::Mutex};

use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tauri_plugin_deep_link::DeepLinkExt;
use url::Url;

use super::models::TournamentPoolRef;
use crate::{
    error::{CommandError, CommandResult},
    infrastructure::logging::{finish_span, global},
    state::AppState,
};

#[derive(Debug, Clone, Serialize)]
pub struct TournamentLink {
    pub id: u64,
    pub reference: TournamentPoolRef,
}

#[derive(Default)]
pub struct LinkInbox {
    next_id: u64,
    pending: Option<TournamentLink>,
}

impl LinkInbox {
    pub(super) fn receive(&mut self, reference: TournamentPoolRef) -> TournamentLink {
        if let Some(pending) = &self.pending
            && pending.reference == reference
        {
            return pending.clone();
        }
        self.next_id += 1;
        let link = TournamentLink {
            id: self.next_id,
            reference,
        };
        self.pending = Some(link.clone());
        link
    }

    pub(super) fn acknowledge(&mut self, id: u64) {
        if self.pending.as_ref().is_some_and(|link| link.id == id) {
            self.pending = None;
        }
    }
}

pub type TournamentLinks = Mutex<LinkInbox>;

pub(super) fn parse(raw: &str) -> CommandResult<TournamentPoolRef> {
    let invalid = || CommandError::new("INVALID_OPP_URI", "无效的 OPP 比赛图池链接");
    if raw.len() > 2048 {
        return Err(invalid());
    }
    let url = Url::parse(raw).map_err(|_| invalid())?;
    if url.scheme() != "opp"
        || url.host_str() != Some("mappool")
        || !matches!(url.path(), "/rino" | "/import")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let mut params = HashMap::new();
    for (key, value) in url.query_pairs() {
        if !(if url.path() == "/import" {
            key == "url"
        } else {
            matches!(key.as_ref(), "season" | "category")
        }) || params
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Err(invalid());
        }
    }
    if url.path() == "/import" {
        let source = super::standard::source_url(&params.remove("url").ok_or_else(invalid)?)?;
        return Ok(TournamentPoolRef {
            provider: "opp".into(),
            season: String::new(),
            category: String::new(),
            url: Some(source.to_string()),
        });
    }
    let reference = TournamentPoolRef {
        url: None,
        provider: "rino".into(),
        season: params.remove("season").ok_or_else(invalid)?,
        category: params.remove("category").ok_or_else(invalid)?,
    };
    reference.validate()?;
    Ok(reference)
}

fn receive(app: &tauri::AppHandle, urls: Vec<Url>) {
    for url in urls {
        let span = global().map(|logger| logger.operation("tournament_pools", "receive_uri"));
        let result = (|| {
            let reference = parse(url.as_str())?;
            let links = app.state::<TournamentLinks>();
            let link = links
                .lock()
                .map_err(|_| CommandError::new("TOURNAMENT_LINK_STATE", "唤起状态不可用"))?
                .receive(reference);
            app.emit_to("main", "tournament-pool-open", &link)?;
            Ok(())
        })();
        if finish_span(span, result).is_ok() {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                let mini = state.music.snapshot().is_ok_and(|snapshot| snapshot.mini);
                if (mini || app.get_webview_window("main").is_none())
                    && let Err(error) = crate::features::music_player::music_window_mode(
                        false,
                        None,
                        app.clone(),
                        state,
                    )
                    .await
                {
                    crate::log_warn!("tournament_pools", "无法恢复主窗口：{}", error);
                }
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            });
        }
    }
}

pub(crate) fn initialize(app: &tauri::AppHandle) {
    // A portable binary has no installer to register the protocol. Never register a dev/helper path.
    #[cfg(all(not(debug_assertions), any(windows, target_os = "linux")))]
    {
        let span = global().map(|logger| logger.operation("tournament_pools", "register_protocol"));
        let result = app
            .deep_link()
            .register_all()
            .map_err(|e| CommandError::from_error("PROTOCOL_REGISTRATION_FAILED", e));
        let _ = finish_span(span, result);
    }
    let handle = app.clone();
    app.deep_link()
        .on_open_url(move |event| receive(&handle, event.urls()));
    match app.deep_link().get_current() {
        Ok(Some(urls)) => receive(app, urls),
        Ok(None) => {}
        Err(error) => crate::log_warn!("tournament_pools", "无法读取启动链接：{}", error),
    }
}

#[tauri::command]
pub fn get_pending_tournament_link(
    links: State<'_, TournamentLinks>,
) -> CommandResult<Option<TournamentLink>> {
    let span =
        global().map(|logger| logger.operation("tournament_pools", "get_pending_tournament_link"));
    finish_span(
        span,
        links
            .lock()
            .map(|inbox| inbox.pending.clone())
            .map_err(|_| CommandError::new("TOURNAMENT_LINK_STATE", "唤起状态不可用")),
    )
}

#[tauri::command]
pub fn acknowledge_tournament_link(
    id: u64,
    links: State<'_, TournamentLinks>,
) -> CommandResult<()> {
    let span =
        global().map(|logger| logger.operation("tournament_pools", "acknowledge_tournament_link"));
    finish_span(
        span,
        links
            .lock()
            .map(|mut inbox| inbox.acknowledge(id))
            .map_err(|_| CommandError::new("TOURNAMENT_LINK_STATE", "唤起状态不可用")),
    )
}
