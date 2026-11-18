//! Ephemeral, single-session media playback policy. Native resources belong to
//! the injected backend; the caller owns source validation and UI context.
use crate::{ApiError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub id: i64,
    pub revision: String,
    pub context: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Play,
    Pause,
    Seek { position_ms: u64 },
    SetVolume { volume: f64 },
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackState {
    Loading,
    Playing,
    Paused,
    Ended,
    Stopped,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Acceleration {
    pub requested_gpu: bool,
    pub renderer: String,
    pub decoder: String,
    pub fallback_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub state: PlaybackState,
    pub duration_ms: Option<u64>,
    pub position_ms: u64,
    pub volume: f64,
    pub has_audio: bool,
    pub width: u32,
    pub height: u32,
    pub error: Option<String>,
    #[serde(default)]
    pub acceleration: Option<Acceleration>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub serial: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Session {
    pub token: u64,
    pub identity: Identity,
    pub snapshot: Snapshot,
}

/// open may enqueue loading; close must be idempotent and clean up even after
/// a failed/partial open. Implementations must not open resources in new().
pub trait Backend: Send {
    fn open(&mut self, source: Source) -> Result<()>;
    fn control(&mut self, action: Action) -> Result<()>;
    fn snapshot(&mut self) -> Result<Snapshot>;
    fn frame(&mut self, after_serial: u64) -> Result<Option<Frame>>;
    fn close(&mut self);
}

/// Explicit unsupported platform; constructing it has no side effects.
#[derive(Default)]
pub struct Unsupported;
impl Backend for Unsupported {
    fn open(&mut self, _: Source) -> Result<()> {
        Err(ApiError::new(
            "media_unsupported",
            "Media playback is unavailable on this platform",
        ))
    }
    fn control(&mut self, _: Action) -> Result<()> {
        Err(ApiError::new(
            "media_unsupported",
            "Media playback is unavailable on this platform",
        ))
    }
    fn snapshot(&mut self) -> Result<Snapshot> {
        Err(ApiError::new(
            "media_unsupported",
            "Media playback is unavailable on this platform",
        ))
    }
    fn frame(&mut self, _: u64) -> Result<Option<Frame>> {
        Err(ApiError::new(
            "media_unsupported",
            "Media playback is unavailable on this platform",
        ))
    }
    fn close(&mut self) {}
}

struct Active {
    token: u64,
    identity: Identity,
}
pub struct Player {
    backend: Box<dyn Backend>,
    active: Option<Active>,
}
impl Default for Player {
    fn default() -> Self {
        Self::new(Box::new(Unsupported))
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.close();
    }
}
impl Player {
    pub fn new(backend: Box<dyn Backend>) -> Self {
        Self {
            backend,
            active: None,
        }
    }
    /// A new open invalidates the old session before touching the new source.
    pub fn open(&mut self, identity: Identity, source: Source) -> Result<Session> {
        self.close();
        // Never reuse tokens after a backend replacement or a new Player.
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let token = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| {
                ApiError::new(
                    "media_token_exhausted",
                    "Playback session counter exhausted",
                )
            })?;
        if let Err(error) = self.backend.open(source) {
            self.backend.close();
            return Err(error);
        }
        let snapshot = match self.backend.snapshot() {
            Ok(value) => value,
            Err(error) => {
                self.backend.close();
                return Err(error);
            }
        };
        self.active = Some(Active {
            token,
            identity: identity.clone(),
        });
        Ok(Session {
            token,
            identity,
            snapshot,
        })
    }
    /// Inspect the binding without querying the backend or starting work.
    pub fn current_identity(&self) -> Option<&Identity> {
        self.active.as_ref().map(|active| &active.identity)
    }
    fn require(&self, token: u64) -> Result<()> {
        if self
            .active
            .as_ref()
            .is_none_or(|active| active.token != token)
        {
            return Err(ApiError::new(
                "stale_media_session",
                "Playback session is no longer current",
            ));
        }
        Ok(())
    }
    pub fn state(&mut self) -> Result<Option<Session>> {
        let Some(active) = &self.active else {
            return Ok(None);
        };
        Ok(Some(Session {
            token: active.token,
            identity: active.identity.clone(),
            snapshot: self.backend.snapshot()?,
        }))
    }
    pub fn control(&mut self, token: u64, action: Action) -> Result<Option<Session>> {
        self.require(token)?;
        match action {
            Action::Stop => {
                self.close();
                return Ok(None);
            }
            Action::SetVolume { volume }
                if !volume.is_finite() || !(0.0..=1.0).contains(&volume) =>
            {
                return Err(ApiError::new(
                    "invalid_media_volume",
                    "Volume must be finite and between 0 and 1",
                ));
            }
            Action::Seek { position_ms } => {
                let duration = self.backend.snapshot()?.duration_ms.ok_or_else(|| {
                    ApiError::new("media_seek_unavailable", "Duration is not yet available")
                })?;
                if position_ms > duration {
                    return Err(ApiError::new(
                        "invalid_media_seek",
                        "Seek position exceeds duration",
                    ));
                }
            }
            _ => (),
        }
        self.backend.control(action)?;
        self.state()
    }
    pub fn frame(&mut self, token: u64, after_serial: u64) -> Result<Option<Frame>> {
        self.require(token)?;
        let Some(frame) = self.backend.frame(after_serial)? else {
            return Ok(None);
        };
        let expected = (frame.width as usize)
            .checked_mul(frame.height as usize)
            .and_then(|n| n.checked_mul(4));
        if frame.serial <= after_serial
            || frame.width == 0
            || frame.height == 0
            || frame.width > 1280
            || frame.height > 720
            || expected != Some(frame.rgba.len())
        {
            return Err(ApiError::new(
                "invalid_media_frame",
                "Playback returned an invalid frame",
            ));
        }
        Ok(Some(frame))
    }
    pub fn close(&mut self) {
        if self.active.take().is_some() {
            self.backend.close();
        }
    }
}
