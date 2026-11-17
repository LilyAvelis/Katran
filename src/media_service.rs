//! Playback authorization and immutable source preparation.
use crate::{
    media_playback::{self, Action, Identity, Source},
    *,
};
use base64::Engine;
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: SystemTime,
}
fn stamp(path: &Path) -> Result<Stamp> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(ApiError::new(
            "invalid_media_source",
            "Expected a regular media file",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(ApiError::new(
                "invalid_media_source",
                "Reparse points are not media sources",
            ));
        }
    }
    Ok(Stamp {
        len: metadata.len(),
        modified: metadata.modified()?,
    })
}

pub(crate) struct PreparedMedia {
    event: Event,
    identity: Identity,
    path: PathBuf,
    stamp: Stamp,
    epoch: u64,
}
pub(crate) struct MediaState {
    player: media_playback::Player,
    lease: Option<(PathBuf, Stamp)>,
    context: Option<(i64, String)>,
    epoch: u64,
}
impl MediaState {
    pub(crate) fn new(read_only: bool) -> Self {
        #[cfg(windows)]
        let player = if read_only {
            media_playback::Player::default()
        } else {
            media_playback::Player::new(Box::new(
                crate::media_playback_windows::NativeBackend::default(),
            ))
        };
        #[cfg(not(windows))]
        let player = {
            let _ = read_only;
            media_playback::Player::default()
        };
        Self {
            player,
            lease: None,
            context: None,
            epoch: 0,
        }
    }
    fn invalidate(&mut self) {
        self.epoch = self.epoch.saturating_add(1);
        self.player.close();
        self.lease = None;
    }
}
impl Service {
    pub fn set_media_backend(&mut self, backend: Box<dyn media_playback::Backend>) -> Result<()> {
        if self.read_only {
            return Err(ApiError::new(
                "read_only",
                "Preview client cannot create a player",
            ));
        }
        self.media.invalidate();
        self.media.player = media_playback::Player::new(backend);
        Ok(())
    }
    fn media_context(&self) -> Option<(i64, String)> {
        if !self.window_visible || self.interaction.settings_open {
            return None;
        }
        let (id, mode) = if let Some(id) = self.interaction.preview {
            (id, "preview")
        } else if self.interaction.view == interaction::View::Details {
            let id = self.interaction.cursor?;
            if !self.interaction.visible.contains(&id) {
                return None;
            }
            (id, "details")
        } else {
            return None;
        };
        Some((
            id,
            serde_json::to_string(&(self.interaction.tab, &self.interaction.folder_path, mode))
                .ok()?,
        ))
    }
    pub(crate) fn reconcile_media(&mut self) {
        let context = self.media_context();
        if self.media.context != context {
            self.media.invalidate();
            self.media.context = context;
        }
        // Ordinary catalogue lookups are indexed; avoid scanning the Desktop
        // catalogue on every frame. The lease separately checks the actual file.
        let identity = self.media.player.current_identity().cloned();
        if let Some(identity) = identity
            && !desktop_projection::is_desktop_id(identity.id)
            && !self
                .get(identity.id)
                .is_ok_and(|event| event.revision == identity.revision)
        {
            self.media.invalidate();
        }
    }
    pub(crate) fn start_media_prepare(
        &mut self,
        id: i64,
        revision: String,
    ) -> Result<(Event, Identity, u64, Option<String>)> {
        self.media.invalidate();
        let (current, context) = self
            .media_context()
            .ok_or_else(|| ApiError::new("media_context_closed", "Open a media preview first"))?;
        if current != id {
            return Err(ApiError::new(
                "media_context_closed",
                "Media is not the active preview",
            ));
        }
        let event = self.get(id)?;
        if event.revision != revision {
            return Err(ApiError::new("stale_selection", "Media revision changed"));
        }
        if !matches!(
            event.kind,
            EventKind::FileReference | EventKind::ManagedImage
        ) || !(file_types::is_native_video(&event.path)
            || file_types::is_native_video(Path::new(&event.name)))
        {
            return Err(ApiError::new(
                "invalid_media_source",
                "Expected a video file",
            ));
        }
        let desktop_digest = if desktop_projection::is_desktop_id(id) {
            let (resolved, typed) = desktop_projection::get(&self.root, id)?;
            if resolved.path != event.path || resolved.revision != event.revision {
                return Err(ApiError::new("stale_selection", "Desktop asset changed"));
            }
            Some(
                typed
                    .split_once(':')
                    .ok_or_else(|| {
                        ApiError::new("invalid_media_source", "Invalid Desktop identity")
                    })?
                    .1
                    .to_owned(),
            )
        } else {
            None
        };
        Ok((
            event,
            Identity {
                id,
                revision,
                context,
            },
            self.media.epoch,
            desktop_digest,
        ))
    }
    pub(crate) fn commit_media(&mut self, prepared: PreparedMedia) -> Result<serde_json::Value> {
        if prepared.epoch != self.media.epoch
            || self.media_context()
                != Some((prepared.identity.id, prepared.identity.context.clone()))
        {
            return Err(ApiError::new(
                "stale_media_context",
                "Media preview changed while loading",
            ));
        }
        let current = self.get(prepared.identity.id)?;
        if current.revision != prepared.event.revision
            || current.path != prepared.event.path
            || stamp(&prepared.path)? != prepared.stamp
        {
            return Err(ApiError::new(
                "source_changed",
                "Media source changed while loading",
            ));
        }
        let session = self.media.player.open(
            prepared.identity,
            Source {
                path: prepared.path.clone(),
            },
        )?;
        self.media.lease = Some((prepared.path, prepared.stamp));
        Ok(serde_json::json!(session))
    }
    pub(crate) fn open_media(&mut self, id: i64, revision: String) -> Result<serde_json::Value> {
        let (event, identity, epoch, desktop_digest) = self.start_media_prepare(id, revision)?;
        self.commit_media(prepare(event, identity, epoch, desktop_digest)?)
    }
    fn check_media_lease(&mut self) -> Result<()> {
        if let Some((path, expected)) = &self.media.lease
            && stamp(path).ok().as_ref() != Some(expected)
        {
            self.media.invalidate();
            return Err(ApiError::new("source_changed", "Media source changed"));
        }
        Ok(())
    }
    pub(crate) fn control_media(
        &mut self,
        token: u64,
        action: Action,
    ) -> Result<serde_json::Value> {
        self.check_media_lease()?;
        let result = self.media.player.control(token, action)?;
        if result.is_none() {
            self.media.lease = None;
        }
        Ok(serde_json::json!(result))
    }
    pub(crate) fn media_state(&mut self) -> Result<serde_json::Value> {
        self.check_media_lease()?;
        Ok(serde_json::json!(self.media.player.state()?))
    }
    pub(crate) fn media_frame(
        &mut self,
        token: u64,
        after_serial: u64,
    ) -> Result<serde_json::Value> {
        self.check_media_lease()?;
        let frame = self.media.player.frame(token, after_serial)?.map(|f| serde_json::json!({"serial":f.serial,"width":f.width,"height":f.height,"rgba_base64":base64::engine::general_purpose::STANDARD.encode(f.rgba)}));
        Ok(serde_json::json!({"session":token,"frame":frame}))
    }
}
pub(crate) fn prepare(
    event: Event,
    identity: Identity,
    epoch: u64,
    desktop_digest: Option<String>,
) -> Result<PreparedMedia> {
    let path = event.path.clone();
    if !path.is_absolute() {
        return Err(ApiError::new(
            "invalid_media_source",
            "Media path must be absolute",
        ));
    }
    // Reject links in the entire path, not just the leaf. The media engine
    // receives a pathname and must not follow a junction to a different source.
    for ancestor in path.ancestors() {
        let meta = std::fs::symlink_metadata(ancestor)?;
        if meta.file_type().is_symlink() {
            return Err(ApiError::new(
                "invalid_media_source",
                "Linked media paths are not supported",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(ApiError::new(
                    "invalid_media_source",
                    "Reparse points are not media sources",
                ));
            }
        }
    }
    let before = stamp(&path)?;
    let expected = if desktop_projection::is_desktop_id(event.id) {
        Some(
            desktop_digest
                .ok_or_else(|| ApiError::new("invalid_media_source", "Missing Desktop identity"))?,
        )
    } else {
        event
            .revision
            .strip_prefix("ref:")
            .or_else(|| event.revision.strip_prefix("blob:"))
            .map(str::to_owned)
    };
    if let Some(expected) = expected {
        if preview::content_hash(&path)? != expected {
            return Err(ApiError::new(
                "source_changed",
                "Media content no longer matches its revision",
            ));
        }
    } else if preview::revision(&path, None) != event.revision {
        return Err(ApiError::new(
            "source_changed",
            "Media source revision changed",
        ));
    }
    let after = stamp(&path)?;
    if before != after {
        return Err(ApiError::new(
            "source_changed",
            "Media source changed during validation",
        ));
    }
    Ok(PreparedMedia {
        event,
        identity,
        path,
        stamp: after,
        epoch,
    })
}
