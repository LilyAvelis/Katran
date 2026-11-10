//! Manual public-release updater. This module never opens the user's data root.
//! Network/archive preparation belongs on a worker; only `commit` changes the
//! launcher pointer, after the dispatcher authorizes the final effect.
use crate::{ApiError, Result};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

pub const REPOSITORY: &str = "LilyAvelis/Katran";
pub const LATEST_URL: &str = "https://api.github.com/repos/LilyAvelis/Katran/releases/latest";
pub const MAX_ARCHIVE_BYTES: usize = 100 * 1024 * 1024;
pub const MAX_EXECUTABLE_BYTES: usize = 200 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_NOTICES_BYTES: usize = 8 * 1024 * 1024;
pub const LAUNCHER_PROTOCOL: u32 = 1;
static NEXT_STAGE: AtomicU64 = AtomicU64::new(1);
fn error(code: &str, message: impl std::fmt::Display) -> ApiError {
    ApiError::new(code, message)
}
fn invalid(message: impl std::fmt::Display) -> ApiError {
    error("invalid_update", message)
}
fn io_error(message: impl std::fmt::Display) -> ApiError {
    error("update_install_failed", message)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn stable_version(text: &str) -> Result<Version> {
    let version = Version::parse(text).map_err(invalid)?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(invalid(
            "Only stable semantic release versions are supported",
        ));
    }
    Ok(version)
}

pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}
/// Implementations must bound the streamed body; callers validate the bound again
/// so a test/custom transport cannot bypass the domain limits.
pub trait Transport: Send + Sync {
    fn get(&self, url: &str, max_bytes: usize) -> Result<HttpResponse>;
}
pub struct PublicGithubTransport {
    client: std::sync::Mutex<Option<reqwest::blocking::Client>>,
}
pub fn is_trusted_download_url(url: &url::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && matches!(
            url.host_str(),
            Some(
                "api.github.com"
                    | "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
                    | "github-releases.githubusercontent.com"
            )
        )
}
impl PublicGithubTransport {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: std::sync::Mutex::new(None),
        })
    }
    fn client(&self) -> Result<reqwest::blocking::Client> {
        let mut cached = self
            .client
            .lock()
            .map_err(|_| error("update_network_error", "Update HTTP client lock poisoned"))?;
        if let Some(client) = cached.as_ref() {
            return Ok(client.clone());
        }
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!(
                "Katran/",
                env!("CARGO_PKG_VERSION"),
                " manual-updater"
            ))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(180))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 || !is_trusted_download_url(attempt.url()) {
                    attempt.error("Update redirect left official GitHub HTTPS hosts")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|e| error("update_network_error", e))?;
        *cached = Some(client.clone());
        Ok(client)
    }
}
impl Transport for PublicGithubTransport {
    fn get(&self, url: &str, max_bytes: usize) -> Result<HttpResponse> {
        let parsed = url::Url::parse(url).map_err(invalid)?;
        if !is_trusted_download_url(&parsed) {
            return Err(invalid("Only official GitHub HTTPS downloads are allowed"));
        }
        let response = self
            .client()?
            .get(parsed)
            .header("Accept", "application/vnd.github+json")
            .send()
            .map_err(|e| error("update_network_error", e))?;
        if response
            .content_length()
            .is_some_and(|n| n > max_bytes as u64)
        {
            return Err(invalid("Update HTTP body exceeds limit"));
        }
        let status = response.status().as_u16();
        let mut body = Vec::new();
        response
            .take(max_bytes as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|e| error("update_network_error", e))?;
        if body.len() > max_bytes {
            return Err(invalid("Update HTTP body exceeds streamed limit"));
        }
        Ok(HttpResponse { status, body })
    }
}
fn get(transport: &dyn Transport, url: &str, limit: usize) -> Result<HttpResponse> {
    let r = transport.get(url, limit)?;
    if r.body.len() > limit {
        return Err(invalid("Update body exceeds limit"));
    }
    Ok(r)
}
fn asset_url(version: &str, name: &str) -> String {
    format!("https://github.com/{REPOSITORY}/releases/download/v{version}/{name}")
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UpdateAsset {
    url: String,
    size_bytes: u64,
    sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReleaseManifest {
    schema_version: u32,
    version: String,
    tag: String,
    repository: String,
    minimum_launcher_protocol: u32,
    data_schema_version: u32,
    update: UpdateAsset,
}
#[derive(Clone, Debug, Serialize)]
pub struct Release {
    manifest: ReleaseManifest,
}
impl Release {
    pub fn version(&self) -> &str {
        &self.manifest.version
    }
}
#[derive(Debug, Serialize)]
pub struct UpdateStatus {
    pub state: String,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub release: Option<Release>,
    pub message: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct InstalledUpdate {
    pub version: String,
    pub build_id: String,
    pub restart_required: bool,
}
#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}
#[derive(Deserialize)]
struct BuildManifest {
    #[serde(default)]
    third_party_notices_sha256: Option<String>,
    profile: String,
    version: String,
    capture_backend: String,
    executable_sha256: String,
    update_format: u32,
    minimum_launcher_protocol: u32,
}
fn build_manifest(bytes: &[u8]) -> Result<BuildManifest> {
    let text = std::str::from_utf8(bytes)
        .map_err(invalid)?
        .trim_start_matches('\u{feff}');
    serde_json::from_str(text).map_err(invalid)
}
fn validate_build(manifest: &BuildManifest, version: &str, exe_hash: &str) -> Result<()> {
    if manifest.profile != "release"
        || manifest.version != version
        || manifest.capture_backend != "windows-events"
        || manifest.update_format != 1
        || manifest.minimum_launcher_protocol == 0
        || manifest.minimum_launcher_protocol > LAUNCHER_PROTOCOL
        || manifest.executable_sha256 != exe_hash
        || !valid_hash(exe_hash)
    {
        return Err(invalid(
            "Build manifest version/backend/protocol/hash mismatch",
        ));
    }
    stable_version(version)?;
    Ok(())
}

/// Context is derived from an actual packaged executable, never from a command
/// parameter. `from_executable` permits synthetic filesystem fixtures in Rust.
#[derive(Clone, Debug)]
pub struct UpdateContext {
    root: PathBuf,
    runtime: PathBuf,
    versions: PathBuf,
}
fn plain(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.file_type().is_symlink() {
        return Err(invalid("Update paths cannot be symbolic links"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("Update paths cannot be reparse points"));
        }
    }
    Ok(metadata)
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    if !plain(path)?.is_file() {
        return Err(invalid("Expected regular update file"));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io_error)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > limit {
        return Err(invalid("Update file exceeds limit"));
    }
    Ok(bytes)
}
fn pointer_id(runtime: &Path) -> Result<String> {
    let bytes = read_bounded(&runtime.join("current-version.txt"), 64)?;
    let id = std::str::from_utf8(&bytes).map_err(invalid)?.trim();
    if id.len() != 12
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("Invalid launcher build pointer"));
    }
    Ok(id.into())
}
impl UpdateContext {
    pub fn from_current_exe() -> Result<Self> {
        Self::from_executable(&std::env::current_exe().map_err(io_error)?)
    }
    pub fn from_executable(exe: &Path) -> Result<Self> {
        if exe.file_name() != Some(std::ffi::OsStr::new("katran-ui.exe")) {
            return Err(error(
                "updates_unsupported",
                "Not running a packaged Katran executable",
            ));
        }
        plain(exe)?;
        let exe = fs::canonicalize(exe).map_err(io_error)?;
        let directory = exe
            .parent()
            .ok_or_else(|| invalid("Missing version directory"))?;
        let id = directory
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| invalid("Invalid version directory"))?;
        if id.len() != 12
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(error(
                "updates_unsupported",
                "Executable is outside immutable version layout",
            ));
        }
        let versions = directory
            .parent()
            .ok_or_else(|| invalid("Missing versions directory"))?;
        let runtime = versions
            .parent()
            .ok_or_else(|| invalid("Missing runtime directory"))?;
        if versions.file_name() != Some(std::ffi::OsStr::new("versions"))
            || runtime.file_name() != Some(std::ffi::OsStr::new("runtime"))
        {
            return Err(error(
                "updates_unsupported",
                "Executable is outside packaged runtime layout",
            ));
        }
        let root = runtime
            .parent()
            .ok_or_else(|| invalid("Missing installation root"))?
            .to_path_buf();
        let context = Self {
            root,
            runtime: runtime.into(),
            versions: versions.into(),
        };
        context.validate()?;
        let bytes = read_bounded(&exe, MAX_EXECUTABLE_BYTES)?;
        let digest = hash(&bytes);
        if !digest.starts_with(id) {
            return Err(invalid(
                "Running executable does not match immutable version ID",
            ));
        }
        let manifest = build_manifest(&read_bounded(
            &directory.join("build-manifest.json"),
            MAX_MANIFEST_BYTES,
        )?)?;
        validate_build(&manifest, &manifest.version, &digest)?;
        Ok(context)
    }
    fn validate(&self) -> Result<()> {
        for path in [&self.root, &self.runtime, &self.versions] {
            if !plain(path)?.is_dir() {
                return Err(invalid("Invalid installation directory"));
            }
        }
        if fs::canonicalize(&self.runtime).map_err(io_error)? != self.runtime
            || fs::canonicalize(&self.versions).map_err(io_error)? != self.versions
        {
            return Err(invalid("Installation directory identity changed"));
        }
        if !plain(&self.runtime.join("Katran.exe"))?.is_file() {
            return Err(error("updates_unsupported", "Stable launcher is missing"));
        }
        self.pointer_version()?;
        Ok(())
    }
    fn pointer_version(&self) -> Result<Version> {
        let id = pointer_id(&self.runtime)?;
        let directory = self.versions.join(&id);
        if !plain(&directory)?.is_dir() {
            return Err(invalid("Current version directory is missing"));
        }
        let exe = read_bounded(&directory.join("katran-ui.exe"), MAX_EXECUTABLE_BYTES)?;
        let digest = hash(&exe);
        if !digest.starts_with(&id) {
            return Err(invalid("Current version hash does not match pointer"));
        }
        let manifest = build_manifest(&read_bounded(
            &directory.join("build-manifest.json"),
            MAX_MANIFEST_BYTES,
        )?)?;
        validate_build(&manifest, &manifest.version, &digest)?;
        stable_version(&manifest.version)
    }
}
#[derive(Clone)]
pub struct Updater {
    context: Option<UpdateContext>,
    transport: Arc<dyn Transport>,
    current: Version,
    discover_native: bool,
    discovered: Arc<std::sync::OnceLock<Option<UpdateContext>>>,
}
impl Updater {
    pub fn new(
        context: Option<UpdateContext>,
        transport: Arc<dyn Transport>,
        current_version: &str,
    ) -> Result<Self> {
        Ok(Self {
            context,
            transport,
            current: stable_version(current_version)?,
            discover_native: false,
            discovered: Arc::new(std::sync::OnceLock::new()),
        })
    }
    pub fn native(current_version: &str) -> Result<Self> {
        let mut updater = Self::new(
            None,
            Arc::new(PublicGithubTransport::new()?),
            current_version,
        )?;
        updater.discover_native = true;
        Ok(updater)
    }
    fn context(&self) -> Result<&UpdateContext> {
        let context = self.context.as_ref().or_else(|| {
            if self.discover_native {
                self.discovered
                    .get_or_init(|| UpdateContext::from_current_exe().ok())
                    .as_ref()
            } else {
                None
            }
        });
        context.ok_or_else(|| {
            error(
                "updates_unsupported",
                "Manual updates require the packaged runtime and stable launcher",
            )
        })
    }
    pub fn check(&self) -> Result<UpdateStatus> {
        if self.context().is_err() {
            return Ok(UpdateStatus {
                state: "unsupported".into(),
                current_version: self.current.to_string(),
                latest_version: None,
                release: None,
                message: Some(
                    "Manual updates require an installed or portable release package".into(),
                ),
            });
        }
        self.context()?.validate()?;
        let response = get(self.transport.as_ref(), LATEST_URL, MAX_MANIFEST_BYTES)?;
        if response.status == 404 {
            return Ok(self.status(
                "unavailable",
                None,
                None,
                Some("No public release is available yet".into()),
            ));
        }
        if response.status != 200 {
            return Err(error(
                "update_network_error",
                format!("GitHub release request returned HTTP {}", response.status),
            ));
        }
        let release: GithubRelease = serde_json::from_slice(&response.body).map_err(invalid)?;
        if release.draft || release.prerelease {
            return Err(invalid("Draft/prerelease updates are not supported"));
        }
        let version_text = release
            .tag_name
            .strip_prefix('v')
            .ok_or_else(|| invalid("Expected stable vMAJOR.MINOR.PATCH release tag"))?;
        let version = stable_version(version_text)?;
        if version <= self.current || version <= self.context()?.pointer_version()? {
            return Ok(self.status("up_to_date", Some(version.to_string()), None, None));
        }
        let manifest_asset = release
            .assets
            .iter()
            .filter(|a| a.name == "release.json")
            .collect::<Vec<_>>();
        if manifest_asset.len() != 1
            || manifest_asset[0].browser_download_url != asset_url(version_text, "release.json")
            || manifest_asset[0].size > MAX_MANIFEST_BYTES as u64
        {
            return Err(invalid("Release manifest asset is missing or untrusted"));
        }
        let response = get(
            self.transport.as_ref(),
            &manifest_asset[0].browser_download_url,
            MAX_MANIFEST_BYTES,
        )?;
        if response.status != 200 {
            return Err(error(
                "update_network_error",
                "Cannot read release manifest",
            ));
        }
        let manifest: ReleaseManifest = serde_json::from_slice(&response.body).map_err(invalid)?;
        self.validate_release(&manifest)?;
        if manifest.version != version_text || manifest.tag != release.tag_name {
            return Err(invalid("GitHub release and manifest disagree"));
        }
        let assets = release
            .assets
            .iter()
            .filter(|a| a.name == format!("Katran-{version_text}-update.zip"))
            .collect::<Vec<_>>();
        if assets.len() != 1
            || assets[0].browser_download_url != manifest.update.url
            || assets[0].size != manifest.update.size_bytes
        {
            return Err(invalid(
                "Update asset metadata does not match release manifest",
            ));
        }
        Ok(self.status(
            "available",
            Some(version.to_string()),
            Some(Release { manifest }),
            None,
        ))
    }
    fn status(
        &self,
        state: &str,
        latest_version: Option<String>,
        release: Option<Release>,
        message: Option<String>,
    ) -> UpdateStatus {
        UpdateStatus {
            state: state.into(),
            current_version: self.current.to_string(),
            latest_version,
            release,
            message,
        }
    }
    fn validate_release(&self, manifest: &ReleaseManifest) -> Result<()> {
        let version = stable_version(&manifest.version)?;
        if version <= self.current
            || manifest.schema_version != 1
            || manifest.repository != REPOSITORY
            || manifest.tag != format!("v{version}")
            || manifest.minimum_launcher_protocol == 0
            || manifest.minimum_launcher_protocol > LAUNCHER_PROTOCOL
            || manifest.data_schema_version == 0
            || manifest.update.size_bytes == 0
            || manifest.update.size_bytes > MAX_ARCHIVE_BYTES as u64
            || !valid_hash(&manifest.update.sha256)
            || manifest.update.url
                != asset_url(
                    &manifest.version,
                    &format!("Katran-{}-update.zip", manifest.version),
                )
        {
            return Err(invalid("Unsupported or untrusted release manifest"));
        }
        Ok(())
    }
    pub fn prepare(&self, release: &Release) -> Result<PreparedUpdate> {
        self.validate_release(&release.manifest)?;
        let context = self.context()?.clone();
        context.validate()?;
        // File locks are released by the OS after a crash; the small persistent
        // lock file is intentional and never recursively cleaned.
        let lock_path = context.runtime.join(".update-lock");
        if lock_path.exists() {
            plain(&lock_path)?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(io_error)?;
        lock.try_lock().map_err(|e| error("update_busy", e))?;
        if stable_version(release.version())? <= context.pointer_version()? {
            return Err(invalid(
                "Update would replace an equal or newer installed version",
            ));
        }
        let pointer_before = pointer_id(&context.runtime)?;
        let response = get(
            self.transport.as_ref(),
            &release.manifest.update.url,
            MAX_ARCHIVE_BYTES,
        )?;
        if response.status != 200 {
            return Err(error(
                "update_network_error",
                "Update archive download failed",
            ));
        }
        if response.body.len() as u64 != release.manifest.update.size_bytes
            || hash(&response.body) != release.manifest.update.sha256
        {
            return Err(invalid("Update archive size or SHA-256 mismatch"));
        }
        // Exact-pinned ZipIterable avoids indexing an attacker-controlled number
        // of central-directory entries before the three-entry limit is checked.
        let mut archive = zip::unstable::read::ZipIterable::try_new(
            Cursor::new(response.body.as_slice()),
            Default::default(),
        )
        .map_err(invalid)?
        .enforce_header_consistency(true);
        if archive.len() != 3 {
            return Err(invalid(
                "Update archive must contain exactly three root files",
            ));
        }
        let mut exe = None;
        let mut manifest = None;
        let mut notices = None;
        let mut files = archive
            .files_with_cloned_reader(Default::default())
            .map_err(invalid)?;
        for _ in 0..3 {
            let file = files
                .next()
                .ok_or_else(|| invalid("Missing update archive member"))?
                .map_err(invalid)?;
            let name = file.name().map_err(invalid)?.into_owned();
            if file.is_dir()
                || file.unix_mode().is_some_and(|mode| {
                    let kind = mode & 0o170000;
                    kind != 0 && kind != 0o100000
                })
            {
                return Err(invalid("Update archive contains non-regular files"));
            }
            let limit = match name.as_str() {
                "katran-ui.exe" => MAX_EXECUTABLE_BYTES,
                "build-manifest.json" => MAX_MANIFEST_BYTES,
                "THIRD-PARTY-NOTICES.txt" => MAX_NOTICES_BYTES,
                _ => {
                    return Err(invalid(
                        "Update archive contains an unexpected or unsafe path",
                    ));
                }
            };
            if file.size() > limit as u64 {
                return Err(invalid("Expanded update member exceeds limit"));
            }
            let mut bytes = Vec::new();
            file.take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(invalid)?;
            if bytes.len() > limit {
                return Err(invalid("Expanded update member exceeds streamed limit"));
            }
            match name.as_str() {
                "katran-ui.exe" => {
                    if exe.replace(bytes).is_some() {
                        return Err(invalid("Duplicate executable member"));
                    }
                }
                "THIRD-PARTY-NOTICES.txt" => {
                    if notices.replace(bytes).is_some() {
                        return Err(invalid("Duplicate third-party notices member"));
                    }
                }
                _ => {
                    if manifest.replace(bytes).is_some() {
                        return Err(invalid("Duplicate build manifest member"));
                    }
                }
            }
        }
        let exe = exe.ok_or_else(|| invalid("Missing update executable"))?;
        if exe.is_empty() {
            return Err(invalid("Empty update executable"));
        }
        let manifest = manifest.ok_or_else(|| invalid("Missing build manifest"))?;
        let notices = notices.ok_or_else(|| invalid("Missing third-party notices"))?;
        let notice_text = std::str::from_utf8(&notices)
            .map_err(|_| invalid("Third-party notices must be UTF-8"))?;
        if notice_text.trim().is_empty() || notice_text.contains('\0') {
            return Err(invalid("Third-party notices must be nonempty plain text"));
        }
        let notices_hash = hash(&notices);
        let digest = hash(&exe);
        let build = build_manifest(&manifest)?;
        if build.third_party_notices_sha256.as_deref() != Some(notices_hash.as_str()) {
            return Err(invalid("Third-party notices digest mismatch"));
        }
        validate_build(&build, release.version(), &digest)?;
        if build.minimum_launcher_protocol != release.manifest.minimum_launcher_protocol {
            return Err(invalid("Release/build launcher requirements disagree"));
        }
        context.validate()?;
        let stage = Stage::create(&context.versions)?;
        write_new(&stage.path.join("katran-ui.exe"), &exe)?;
        write_new(&stage.path.join("build-manifest.json"), &manifest)?;
        write_new(&stage.path.join("THIRD-PARTY-NOTICES.txt"), &notices)?;
        Ok(PreparedUpdate {
            notices_hash,
            context,
            stage: Some(stage),
            _lock: lock,
            pointer_before,
            version: release.version().into(),
            digest,
            manifest_hash: hash(&manifest),
        })
    }
    pub fn commit(&self, mut prepared: PreparedUpdate) -> Result<InstalledUpdate> {
        let context = self.context()?;
        if context.runtime != prepared.context.runtime {
            return Err(invalid(
                "Prepared update belongs to a different installation",
            ));
        }
        context.validate()?;
        if pointer_id(&context.runtime)? != prepared.pointer_before
            || stable_version(&prepared.version)? <= context.pointer_version()?
        {
            return Err(error(
                "stale_update",
                "Installed version changed during preparation",
            ));
        }
        let stage = prepared
            .stage
            .as_ref()
            .ok_or_else(|| invalid("Missing update stage"))?;
        verify_files(&stage.path, &prepared)?;
        let build_id = prepared.digest[..12].to_owned();
        let destination = context.versions.join(&build_id);
        if destination.exists() {
            if !plain(&destination)?.is_dir() {
                return Err(invalid("Immutable update destination is not a directory"));
            }
            verify_files(&destination, &prepared)?;
        } else {
            fs::rename(&stage.path, &destination).map_err(io_error)?;
            prepared.stage = None;
        }
        verify_files(&destination, &prepared)?;
        context.validate()?;
        if pointer_id(&context.runtime)? != prepared.pointer_before {
            return Err(error(
                "stale_update",
                "Launcher pointer changed before commit",
            ));
        }
        let pointer = Stage::pointer_temp(&context.runtime, format!("{build_id}\r\n").as_bytes())?;
        replace_pointer(&pointer.path, &context.runtime.join("current-version.txt"))?;
        Ok(InstalledUpdate {
            version: prepared.version.clone(),
            build_id,
            restart_required: true,
        })
    }
}
fn verify_files(directory: &Path, prepared: &PreparedUpdate) -> Result<()> {
    if !plain(directory)?.is_dir() {
        return Err(invalid("Update directory changed"));
    }
    if hash(&read_bounded(
        &directory.join("katran-ui.exe"),
        MAX_EXECUTABLE_BYTES,
    )?) != prepared.digest
        || hash(&read_bounded(
            &directory.join("build-manifest.json"),
            MAX_MANIFEST_BYTES,
        )?) != prepared.manifest_hash
        || hash(&read_bounded(
            &directory.join("THIRD-PARTY-NOTICES.txt"),
            MAX_NOTICES_BYTES,
        )?) != prepared.notices_hash
    {
        return Err(invalid("Prepared update contents changed"));
    }
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}
pub struct PreparedUpdate {
    notices_hash: String,
    context: UpdateContext,
    stage: Option<Stage>,
    _lock: File,
    pointer_before: String,
    version: String,
    digest: String,
    manifest_hash: String,
}
struct Stage {
    path: PathBuf,
    parent: PathBuf,
    directory: bool,
}
impl Stage {
    fn unique(parent: &Path, prefix: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        parent.join(format!(
            "{prefix}{}-{stamp:032x}-{:016x}",
            std::process::id(),
            NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
        ))
    }
    fn create(parent: &Path) -> Result<Self> {
        let path = Self::unique(parent, ".update-stage-");
        fs::create_dir(&path).map_err(io_error)?;
        Ok(Self {
            path,
            parent: parent.into(),
            directory: true,
        })
    }
    fn pointer_temp(parent: &Path, bytes: &[u8]) -> Result<Self> {
        let path = Self::unique(parent, ".update-pointer-");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(io_error)?;
        let owned = Self {
            path,
            parent: parent.into(),
            directory: false,
        };
        file.write_all(bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        Ok(owned)
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if self.path.parent() != Some(self.parent.as_path())
            || plain(&self.parent).is_err()
            || fs::canonicalize(&self.parent).ok().as_ref() != Some(&self.parent)
        {
            return;
        }
        if let Ok(metadata) = plain(&self.path) {
            if metadata.file_type().is_symlink() {
                return;
            }
            if self.directory && metadata.is_dir() {
                for name in [
                    "katran-ui.exe",
                    "build-manifest.json",
                    "THIRD-PARTY-NOTICES.txt",
                ] {
                    let file = self.path.join(name);
                    if plain(&file).is_ok_and(|m| m.is_file()) {
                        let _ = fs::remove_file(file);
                    }
                }
                let _ = fs::remove_dir(&self.path);
            } else if !self.directory && metadata.is_file() {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}
#[cfg(windows)]
fn replace_pointer(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 1 | 8) } == 0 {
        return Err(io_error(std::io::Error::last_os_error()));
    }
    Ok(())
}
#[cfg(not(windows))]
fn replace_pointer(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination).map_err(io_error)
}
