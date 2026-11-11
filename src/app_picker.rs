//! A bounded, revision-bound application catalogue shared by GUI and API clients.
use crate::{
    ApiError, Event, Result, Service,
    app_handlers::{self, AppHandler},
    open_file,
};
use base64::Engine;
use serde_json::{Value, json};
use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Mutex},
};

const MAX_SESSIONS: usize = 8;
#[derive(Clone)]
pub(crate) struct Session {
    token: String,
    pub(crate) event: Event,
    handlers: HashSet<String>,
}
#[derive(Default)]
pub(crate) struct Sessions {
    next: u64,
    entries: VecDeque<Session>,
}
impl Sessions {
    pub(crate) fn release(&mut self, token: &str) -> bool {
        let previous = self.entries.len();
        self.entries.retain(|s| s.token != token);
        previous != self.entries.len()
    }
}

pub(crate) fn discover(
    event: &Event,
    provider: &dyn app_handlers::Provider,
) -> Result<Vec<AppHandler>> {
    open_file::validate(event)?;
    let Some(extension) = event_extension(event) else {
        return Ok(Vec::new());
    };
    let mut seen = HashSet::new();
    let mut apps: Vec<_> = provider
        .discover(&extension)?
        .into_iter()
        .take(app_handlers::MAX_HANDLERS)
        .filter(|app| !app.id.is_empty() && app.id.len() <= 256 && seen.insert(app.id.clone()))
        .map(|mut app| {
            app.name = app
                .name
                .chars()
                .filter(|c| !c.is_control())
                .take(256)
                .collect();
            app.icon = app.icon.filter(|icon| {
                (1..=64).contains(&icon.width)
                    && (1..=64).contains(&icon.height)
                    && icon.rgba_base64.len() <= 21848
                    && base64::engine::general_purpose::STANDARD
                        .decode(&icon.rgba_base64)
                        .is_ok_and(|bytes| bytes.len() == (icon.width * icon.height * 4) as usize)
            });
            app
        })
        .filter(|app| !app.name.trim().is_empty())
        .collect();
    apps.sort_by_key(|app| (!app.recommended, app.name.to_lowercase()));
    Ok(apps)
}

pub(crate) fn event_extension(event:&Event)->Option<String> {
    if event.kind==crate::EventKind::LinkFragment {Some(".html".into())}else{app_handlers::extension(&event.name)}
}

pub(crate) struct AppLauncher {
    pub(crate) provider: Arc<Mutex<Box<dyn app_handlers::Provider>>>,
    pub(crate) fallback: Arc<Mutex<Box<dyn open_file::Launcher>>>,
    pub(crate) extension: String,
    pub(crate) app_id: Option<String>,
    pub(crate) cancellation: Option<crate::domain_jobs::Cancellation>,
}
impl open_file::Launcher for AppLauncher {
    fn authorize(&self) -> Result<()> {
        if let Some(cancel) = &self.cancellation {
            cancel.begin_effect()?;
        }
        Ok(())
    }
    fn launch(&self, path: &std::path::Path, _: open_file::Operation) -> Result<()> {
        if let Some(id) = &self.app_id {
            self.provider
                .lock()
                .map_err(|_| {
                    ApiError::new("handlers_unavailable", "Application provider lock poisoned")
                })?
                .invoke(&self.extension, id, path)
        } else {
            self.fallback
                .lock()
                .map_err(|_| ApiError::new("open_busy", "File launcher lock poisoned"))?
                .launch(path, open_file::Operation::OpenWith)
        }
    }
}

impl Service {
    pub(crate) fn list_open_apps(&mut self, id: i64) -> Result<Value> {
        let event = self.get(id)?;
        let apps = {
            let provider = self.app_provider.lock().map_err(|_| {
                ApiError::new("handlers_unavailable", "Application provider lock poisoned")
            })?;
            discover(&event, provider.as_ref())?
        };
        self.commit_open_apps(event, apps)
    }
    pub(crate) fn commit_open_apps(
        &mut self,
        event: Event,
        apps: Vec<AppHandler>,
    ) -> Result<Value> {
        let current = self
            .get(event.id)
            .map_err(|_| ApiError::new("stale_selection", "File no longer exists on the shelf"))?;
        if current.revision != event.revision {
            return Err(ApiError::new("stale_selection", "File revision changed"));
        }
        self.app_sessions.next = self.app_sessions.next.checked_add(1).ok_or_else(|| {
            ApiError::new("session_exhausted", "Application session counter exhausted")
        })?;
        let token = format!(
            "open-apps-{}-{}",
            std::process::id(),
            self.app_sessions.next
        );
        if self.app_sessions.entries.len() >= MAX_SESSIONS {
            self.app_sessions.entries.pop_front();
        }
        self.app_sessions.entries.push_back(Session {
            token: token.clone(),
            event: event.clone(),
            handlers: apps.iter().map(|a| a.id.clone()).collect(),
        });
        Ok(
            json!({"open_apps":true,"token":token,"id":event.id,"revision":event.revision,"name":event.name,"apps":apps}),
        )
    }
    pub(crate) fn app_session(&self, token: &str, app_id: Option<&str>) -> Result<Session> {
        let session = self
            .app_sessions
            .entries
            .iter()
            .find(|s| s.token == token)
            .ok_or_else(|| {
                ApiError::new(
                    "open_apps_expired",
                    "Application list expired; open it again",
                )
            })?;
        if app_id.is_some_and(|id| !session.handlers.contains(id)) {
            return Err(ApiError::new(
                "unknown_application",
                "Application was not in the offered list",
            ));
        }
        let current = self
            .get(session.event.id)
            .map_err(|_| ApiError::new("stale_selection", "File no longer exists on the shelf"))?;
        if current.revision != session.event.revision {
            return Err(ApiError::new("stale_selection", "File revision changed"));
        }
        Ok(session.clone())
    }
    pub(crate) fn app_launcher(
        &self,
        event: &Event,
        app_id: Option<String>,
        cancellation: Option<crate::domain_jobs::Cancellation>,
    ) -> AppLauncher {
        AppLauncher {
            provider: Arc::clone(&self.app_provider),
            fallback: Arc::clone(&self.file_launcher),
            extension: event_extension(event).unwrap_or_default(),
            app_id,
            cancellation,
        }
    }
    pub(crate) fn open_with_app(&mut self, token: &str, app_id: Option<String>) -> Result<Value> {
        let session = self.app_session(token, app_id.as_deref())?;
        let launcher = self.app_launcher(&session.event, app_id, None);
        let mut result = open_file::execute_with_cache(
            &session.event,
            open_file::Operation::OpenWith,
            &launcher,
            &self.root,
            self.quota,
        )?;
        result["application_id"] = json!(launcher.app_id);
        Ok(result)
    }
}
