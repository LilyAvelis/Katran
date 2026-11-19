//! Explicit, visible optional-component setup. Reading status never starts Python.
use crate::{ApiError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    Emoji,
    Folder,
    FolderGpu,
    Translate,
    Upscale,
}
impl Component {
    pub fn name(self) -> &'static str {
        match self {
            Self::Emoji => "emoji",
            Self::Folder => "folder",
            Self::FolderGpu => "folder_gpu",
            Self::Translate => "translate",
            Self::Upscale => "upscale",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Install,
    Verify,
}

pub trait Installation: Send {
    fn finished(&mut self) -> Result<bool>;
}
pub trait Launcher: Send {
    fn launch(&mut self, request: &Path, bundle: &Path) -> Result<Box<dyn Installation>>;
}
struct Native;
#[cfg(any(windows, test))]
fn quote_windows(value: &[u16]) -> Vec<u16> {
    let mut result = vec![34];
    let mut slashes = 0;
    for &c in value {
        if c == 92 {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            92,
            if c == 34 { slashes * 2 + 1 } else { slashes },
        ));
        slashes = 0;
        result.push(c);
    }
    result.extend(std::iter::repeat_n(92, slashes * 2));
    result.push(34);
    result
}
#[cfg(windows)]
struct NativeProcess(usize);
#[cfg(windows)]
impl Installation for NativeProcess {
    fn finished(&mut self) -> Result<bool> {
        let mut code = 0;
        if unsafe {
            windows_sys::Win32::System::Threading::GetExitCodeProcess(self.0 as _, &mut code)
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(code != 259)
    }
}
#[cfg(windows)]
impl Drop for NativeProcess {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0 as _);
        }
    }
}
impl Launcher for Native {
    fn launch(&mut self, request: &Path, bundle: &Path) -> Result<Box<dyn Installation>> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::System::Threading::{
                CREATE_NEW_CONSOLE, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
            };
            let shell = std::env::var_os("SystemRoot")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
            let args = [
                shell.as_os_str().to_owned(),
                "-NoProfile".into(),
                "-NoExit".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                bundle
                    .join("scripts/ai-components/terminal.ps1")
                    .into_os_string(),
                "-Request".into(),
                request.as_os_str().to_owned(),
            ];
            let mut command = Vec::new();
            for arg in &args {
                if !command.is_empty() {
                    command.push(32);
                }
                command.extend(quote_windows(&arg.encode_wide().collect::<Vec<_>>()));
            }
            if command.len() > 32766 {
                return Err(ApiError::new(
                    "ai_launch_failed",
                    "Installer command is too long",
                ));
            }
            command.push(0);
            let executable: Vec<_> = shell.as_os_str().encode_wide().chain(Some(0)).collect();
            let startup = STARTUPINFOW {
                cb: std::mem::size_of::<STARTUPINFOW>() as u32,
                ..Default::default()
            };
            let mut process = PROCESS_INFORMATION::default();
            // No STARTF_USESTDHANDLES: child gets its own visible console streams,
            // rather than inheriting the GUI host's absent/redirected handles.
            if unsafe {
                CreateProcessW(
                    executable.as_ptr(),
                    command.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    CREATE_NEW_CONSOLE,
                    std::ptr::null(),
                    std::ptr::null(),
                    &startup,
                    &mut process,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(process.hThread);
            }
            Ok(Box::new(NativeProcess(process.hProcess as usize)))
        }
        #[cfg(not(windows))]
        {
            let _ = (request, bundle);
            Err(ApiError::new(
                "unsupported_platform",
                "Visible AI installation currently requires Windows",
            ))
        }
    }
}
struct Running {
    id: String,
    directory: PathBuf,
    process: Box<dyn Installation>,
}
pub struct Manager {
    launcher: Box<dyn Launcher>,
    running: BTreeMap<Component, Running>,
    bundle: Option<PathBuf>,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            launcher: Box::new(Native),
            running: BTreeMap::new(),
            bundle: None,
        }
    }
}
impl Manager {
    pub fn with_launcher(launcher: Box<dyn Launcher>, bundle: PathBuf) -> Self {
        Self {
            launcher,
            running: BTreeMap::new(),
            bundle: Some(bundle),
        }
    }
    fn refresh(&mut self) -> Result<()> {
        let mut done = Vec::new();
        for (component, run) in &mut self.running {
            if run.directory.join("result.json").is_file() {
                done.push(*component);
            } else if run.process.finished()? {
                fs::write(run.directory.join("result.json"),json!({"id":run.id,"component":component,"state":"failed","verified":false,"code":"terminal_closed","message":"Terminal closed before reporting a result","log_path":run.directory.join("install.log")}).to_string())?;
                done.push(*component);
            }
        }
        for component in done {
            self.running.remove(&component);
        }
        Ok(())
    }
    pub fn launch(
        &mut self,
        profile: &Path,
        component: Component,
        action: Action,
    ) -> Result<Value> {
        self.refresh()?;
        if self.running.contains_key(&component) {
            return Err(ApiError::new(
                "ai_install_busy",
                "This component already has an active terminal operation",
            ));
        }
        let bundle = match &self.bundle {
            Some(bundle) => bundle.clone(),
            None => crate::ai_component_bundle::materialize(profile)?,
        };
        if !bundle.join("scripts/ai-components/terminal.ps1").is_file() {
            return Err(ApiError::new(
                "ai_installer_missing",
                "AI installer scripts are missing from this Katran installation",
            ));
        }
        let profile = fs::canonicalize(profile)?;
        let id = crate::folder_semantic::new_session();
        let directory = profile.join("ai-installations").join(&id);
        fs::create_dir_all(&directory)?;
        let directory = fs::canonicalize(directory)?;
        if !directory.starts_with(&profile) {
            return Err(ApiError::new(
                "ai_install_path_invalid",
                "Installation directory escapes the profile",
            ));
        }
        let request_path = directory.join("request.json");
        let image = match component {
            Component::Upscale => {
                let models = crate::upscale_components::MODELS
                    .into_iter()
                    .map(|name| {
                        let found = crate::upscale_components::discover(&profile, name)?;
                        Ok((name, found.model))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()?;
                json!({
                    "python": crate::upscale_components::resolve_python(&profile),
                    "models": models,
                })
            }
            Component::Translate => {
                let status = crate::translate_components::discover(
                    &profile,
                    crate::translate_components::MODEL,
                )?;
                json!({
                    "ocr_root": crate::translate_components::ocr_root(&profile)?,
                    "paddle_python": status.paddle_python,
                    "vision_python": status.manga_python,
                    "ollama": status.ollama_exe,
                    "ollama_models": status.ollama_models,
                })
            }
            _ => Value::Null,
        };
        fs::write(&request_path,json!({"id":id,"component":component,"action":action,"profile":profile,"bundle_root":bundle,"log_path":directory.join("install.log"),"result_path":directory.join("result.json"),"image":image}).to_string())?;
        let process = self.launcher.launch(&request_path, &bundle)?;
        self.running.insert(
            component,
            Running {
                id: id.clone(),
                directory: directory.clone(),
                process,
            },
        );
        Ok(
            json!({"id":id,"component":component,"action":action,"state":"running","visible_terminal":true,"log_path":directory.join("install.log")}),
        )
    }
    pub fn status(&mut self, profile: &Path) -> Result<Value> {
        self.refresh()?;
        let bundle_available = cfg!(windows) || self.bundle.is_some();
        let mut latest: BTreeMap<Component, (String, Value)> = BTreeMap::new();
        if let Ok(entries) = fs::read_dir(profile.join("ai-installations")) {
            // Status is bounded, metadata-only; it never imports Python or hashes models.
            let mut entries: Vec<_> = entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .collect();
            entries
                .sort_by_key(|e| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).ok()));
            for entry in entries.into_iter().take(128) {
                let Ok(request) = read_json(&entry.path().join("request.json")) else {
                    continue;
                };
                let Ok(component) =
                    serde_json::from_value::<Component>(request["component"].clone())
                else {
                    continue;
                };
                let Some(id) = request["id"].as_str() else {
                    continue;
                };
                if latest.contains_key(&component) {
                    continue;
                }
                let receipt = read_json(&entry.path().join("result.json"))
                    .ok()
                    .filter(|r| r["id"] == id && r["component"] == request["component"]);
                let active = self.running.get(&component).is_some_and(|run| run.id == id);
                latest.insert(component,(id.into(),json!({"state":if active {"running"} else if receipt.is_some(){"finished"}else{"unknown"},"receipt":receipt,"log_path":entry.path().join("install.log")})));
            }
        }
        let legacy = profile.join("emoji-ai-runtime");
        let folder = profile.join("folder-ai-runtime");
        let emoji_present = legacy
            .join("local-experiments/emoji-ai/katran-e-runtime.json")
            .is_file();
        let components = [
            Component::Emoji,
            Component::Folder,
            Component::FolderGpu,
            Component::Translate,
            Component::Upscale,
        ]
        .into_iter()
        .map(|component| {
            let installed = match component {
                Component::Emoji => emoji_present,
                Component::Folder => folder.join("manifest.json").is_file() || emoji_present,
                Component::FolderGpu => profile
                    .join("folder-ai-gpu-runtime/manifest.json")
                    .is_file(),
                Component::Translate => crate::translate_components::discover(
                    profile,
                    crate::translate_components::MODEL,
                )
                .is_ok_and(|status| {
                    use crate::ImageTranslationSource::{Chinese, Japanese, Korean};
                    [Japanese, Chinese, Korean]
                        .into_iter()
                        .all(|source| status.resolve(source).is_ok())
                }),
                Component::Upscale => crate::upscale_components::MODELS.into_iter().all(|name| {
                    crate::upscale_components::discover(profile, name)
                        .is_ok_and(|status| status.python.is_some() && status.model.is_some())
                }),
            };
            json!({
                "component": component,
                "files_found": installed,
                "operation": latest.remove(&component).map(|(_, value)| value),
                "installer_available": bundle_available,
            })
        })
        .collect::<Vec<_>>();
        Ok(json!({"components":components,"status_kind":"metadata_and_last_terminal_receipt"}))
    }
}
fn read_json(path: &Path) -> Result<Value> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(32769).read_to_end(&mut bytes)?;
    if bytes.len() > 32768 {
        return Err(ApiError::new(
            "invalid_receipt",
            "Receipt exceeds size limit",
        ));
    }
    serde_json::from_slice(&bytes).map_err(|e| ApiError::new("invalid_receipt", e))
}

impl crate::Service {
    pub fn set_ai_component_launcher(&mut self, launcher: Box<dyn Launcher>, bundle: PathBuf) {
        self.ai_components = Manager::with_launcher(launcher, bundle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn console_arguments_preserve_spaces_quotes_and_trailing_slashes() {
        for (input, expected) in [
            (r"C:\a b\", "\"C:\\a b\\\\\""),
            ("a\"b", "\"a\\\"b\""),
            ("привет & $x", "\"привет & $x\""),
        ] {
            assert_eq!(
                String::from_utf16(&quote_windows(&input.encode_utf16().collect::<Vec<_>>()))
                    .unwrap(),
                expected
            );
        }
    }
}
