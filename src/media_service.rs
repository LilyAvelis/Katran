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
pub(crate) struct Stamp {
    len: u64,
    modified: SystemTime,
}
pub(crate) fn stamp(path: &Path) -> Result<Stamp> {
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
pub(crate) struct CaptureLease {
    token: u64,
    identity: Identity,
    epoch: u64,
    timeline: u64,
    path: PathBuf,
    stamp: Stamp,
    pub(crate) clipboard_sequence: u32,
}
pub(crate) struct MediaState {
    player: media_playback::Player,
    lease: Option<(PathBuf, Stamp)>,
    context: Option<(i64, String)>,
    epoch: u64,
    timeline: u64,
    capture_clipboard: Option<Box<dyn copy_out::ClipboardSink + Send>>,
    configured_gpu: Option<bool>,
}
impl MediaState {
    pub(crate) fn new(read_only: bool) -> Self {
        #[cfg(windows)]
        let player = if read_only {
            media_playback::Player::default()
        } else {
            media_playback::Player::new(Box::new(crate::media_playback_mpv::MpvBackend::default()))
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
            timeline: 0,
            capture_clipboard: None,
            configured_gpu: if cfg!(windows) && !read_only {
                Some(false)
            } else {
                None
            },
        }
    }
    fn invalidate(&mut self) {
        self.epoch = self.epoch.saturating_add(1);
        self.player.close();
        self.lease = None;
    }
}
impl Service {
    /// Injectable publication boundary for synthetic capture tests.
    pub fn set_media_capture_clipboard_sink(
        &mut self,
        sink: Box<dyn copy_out::ClipboardSink + Send>,
    ) {
        self.media.capture_clipboard = Some(sink);
    }
    pub(crate) fn publish_media_capture(
        &mut self,
        mut result: serde_json::Value,
        prepared: std::result::Result<(copy_out::Plan, copy_out::PreparedNativeClipboard), String>,
        clipboard_sequence: u32,
    ) -> serde_json::Value {
        struct SinkRef<'a>(&'a mut (dyn copy_out::ClipboardSink + Send));
        impl copy_out::ClipboardSink for SinkRef<'_> {
            fn write(
                &mut self,
                paths: &[PathBuf],
            ) -> std::result::Result<(u32, Vec<String>), String> {
                self.0.write(paths)
            }
            fn write_with_facts(
                &mut self,
                paths: &[PathBuf],
            ) -> std::result::Result<copy_out::ClipboardWrite, String> {
                self.0.write_with_facts(paths)
            }
        }
        let publication = if capture::sequence_number() != clipboard_sequence {
            Err("Clipboard changed since capture was requested".to_owned())
        } else {
            prepared.and_then(|(plan, mut native)| {
                if let Some(sink) = self.media.capture_clipboard.as_deref_mut() {
                    copy_out::commit(&plan, &mut SinkRef(sink))
                } else {
                    copy_out::commit(&plan, &mut native)
                }
            })
        };
        result["clipboard"] = match publication {
            Ok(receipt) => {
                let id = result["event"]["id"].as_i64();
                if receipt.failures.is_empty() {
                    if let Some(id) = id {
                        self.record_selection_publication(receipt.sequence, &[id]);
                    }
                } else {
                    self.record_clipboard_publication(receipt.sequence);
                }
                let image_copied = receipt.failures.is_empty()
                    && receipt
                        .publication
                        .as_ref()
                        .is_none_or(|p| p.formats.contains(&"CF_DIBV5"))
                    && !receipt
                        .warnings
                        .iter()
                        .any(|warning| warning.contains("pixels unavailable"));
                serde_json::json!({"ok":image_copied,"error": if image_copied { None } else { Some("Clipboard did not accept image pixels") },"warnings":receipt.warnings})
            }
            Err(error) => serde_json::json!({"ok":false,"error":error}),
        };
        result
    }
    pub fn set_media_backend(&mut self, backend: Box<dyn media_playback::Backend>) -> Result<()> {
        if self.read_only {
            return Err(ApiError::new(
                "read_only",
                "Preview client cannot create a player",
            ));
        }
        self.media.invalidate();
        self.media.player = media_playback::Player::new(backend);
        self.media.configured_gpu = None;
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
            serde_json::to_string(&(
                self.interaction.tab,
                &self.interaction.folder_path,
                mode,
                self.settings.video_gpu_acceleration,
            ))
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
        #[cfg(windows)]
        if let Some(current) = self.media.configured_gpu
            && current != self.settings.video_gpu_acceleration
        {
            self.media.player = media_playback::Player::new(Box::new(
                crate::media_playback_mpv::MpvBackend::with_gpu(
                    self.settings.video_gpu_acceleration,
                ),
            ));
            self.media.configured_gpu = Some(self.settings.video_gpu_acceleration);
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
        if !matches!(action, Action::SetVolume { .. }) {
            self.media.timeline = self.media.timeline.saturating_add(1);
        }
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
    /// Cheap action availability; never opens a player or decodes a frame.
    pub(crate) fn media_pause_availability(&mut self) -> serde_json::Value {
        match self.pause_session(None) {
            Ok(_) => serde_json::json!({"ready":true,"error":null}),
            Err(error) => serde_json::json!({"ready":false,"error":error}),
        }
    }
    fn pause_session(&mut self, requested: Option<u64>) -> Result<media_playback::Session> {
        self.check_media_lease()?;
        let session = self
            .media
            .player
            .state()?
            .ok_or_else(|| ApiError::new("media_unavailable", "Open a video preview first"))?;
        if requested.is_some_and(|token| token != session.token) {
            return Err(ApiError::new(
                "stale_media_session",
                "Playback session is no longer current",
            ));
        }
        if self.media_context() != Some((session.identity.id, session.identity.context.clone())) {
            return Err(ApiError::new(
                "media_context_closed",
                "Open a video preview first",
            ));
        }
        if session.snapshot.step_pending
            || !matches!(
                session.snapshot.state,
                media_playback::PlaybackState::Playing
                    | media_playback::PlaybackState::Paused
                    | media_playback::PlaybackState::Ended
            )
        {
            return Err(ApiError::new("media_busy", "The video frame is not ready"));
        }
        Ok(session)
    }
    pub(crate) fn toggle_media_pause(
        &mut self,
        requested: Option<u64>,
    ) -> Result<serde_json::Value> {
        let session = self.pause_session(requested)?;
        if session.snapshot.state == media_playback::PlaybackState::Ended {
            self.control_media(session.token, Action::Seek { position_ms: 0 })?;
        }
        self.control_media(
            session.token,
            if session.snapshot.state == media_playback::PlaybackState::Playing {
                Action::Pause
            } else {
                Action::Play
            },
        )
    }
    /// Cheap action availability; never opens a player or decodes a frame.
    pub(crate) fn media_capture_availability(&mut self) -> serde_json::Value {
        let eligible = self
            .media_context()
            .and_then(|(id, _)| self.get(id).ok())
            .is_some_and(|event| {
                file_types::is_native_video(&event.path)
                    || file_types::is_native_video(Path::new(&event.name))
            });
        match self.capture_session(None) {
            Ok(_) => serde_json::json!({"eligible": eligible, "ready": eligible, "error": null}),
            Err(error) => serde_json::json!({"eligible": eligible, "ready": false, "error": error}),
        }
    }
    fn capture_session(&mut self, requested: Option<u64>) -> Result<media_playback::Session> {
        self.check_media_lease()?;
        let session = self
            .media
            .player
            .state()?
            .ok_or_else(|| ApiError::new("media_capture_unavailable", "Open a video first"))?;
        if requested.is_some_and(|token| token != session.token) {
            return Err(ApiError::new(
                "stale_media_session",
                "Playback session is no longer current",
            ));
        }
        if self.media_context() != Some((session.identity.id, session.identity.context.clone())) {
            return Err(ApiError::new(
                "stale_media_context",
                "Video preview changed",
            ));
        }
        if !matches!(
            session.snapshot.state,
            media_playback::PlaybackState::Paused | media_playback::PlaybackState::Ended
        ) {
            return Err(ApiError::new(
                "media_capture_requires_pause",
                "Pause the video to save the current frame",
            ));
        }
        if session.snapshot.step_pending
            || session.snapshot.width == 0
            || session.snapshot.height == 0
        {
            return Err(ApiError::new(
                "media_capture_busy",
                "The selected video frame is not ready",
            ));
        }
        Ok(session)
    }
    pub(crate) fn capture_media_frame(
        &mut self,
        requested: Option<u64>,
    ) -> Result<serde_json::Value> {
        let session = self.capture_session(requested)?;
        let event = self.get(session.identity.id)?;
        if event.revision != session.identity.revision {
            return Err(ApiError::new("source_changed", "Video source changed"));
        }
        let (path, stamp) = self.media.lease.clone().ok_or_else(|| {
            ApiError::new(
                "media_capture_unavailable",
                "Video source lease is unavailable",
            )
        })?;
        let lease = CaptureLease {
            token: session.token,
            identity: session.identity,
            epoch: self.media.epoch,
            timeline: self.media.timeline,
            path,
            stamp,
            clipboard_sequence: capture::sequence_number(),
        };
        let references = self.reference_paths()?;
        self.jobs.check_admission(domain_jobs::Lane::Foreground)?;
        let task = self.media.player.request_capture(session.token)?;
        let root = self.root.clone();
        let quota = self.quota;
        let id = self
            .jobs
            .submit(domain_jobs::Lane::Foreground, move |cancel| {
                let prepared = media_capture::prepare(task, &cancel, &root, quota, &references)?;
                Ok(jobs_service::PreparedWork::MediaCapture { lease, prepared })
            })?;
        Ok(serde_json::json!({"job_id":id,"state":"queued"}))
    }
    pub(crate) fn validate_media_capture(&mut self, lease: &CaptureLease) -> Result<()> {
        if lease.epoch != self.media.epoch
            || lease.timeline != self.media.timeline
            || self.media_context() != Some((lease.identity.id, lease.identity.context.clone()))
        {
            return Err(ApiError::new(
                "stale_media_context",
                "Video preview changed during capture",
            ));
        }
        let session = self.capture_session(Some(lease.token))?;
        let event = self.get(lease.identity.id)?;
        if session.identity != lease.identity
            || event.revision != lease.identity.revision
            || event.path != lease.path
            || stamp(&lease.path)? != lease.stamp
        {
            return Err(ApiError::new(
                "source_changed",
                "Video source changed during capture",
            ));
        }
        Ok(())
    }
    pub(crate) fn commit_media_capture(
        &mut self,
        prepared: media_capture::PreparedCapture,
    ) -> Result<serde_json::Value> {
        let id =
            storage::commit_prepared_bytes(&mut self.db, &self.root, self.quota, prepared.blob)?;
        Ok(
            serde_json::json!({"event": self.get(id)?, "width": prepared.width, "height": prepared.height, "position_ms": prepared.position_ms}),
        )
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
