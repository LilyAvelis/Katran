use katran::{ApiError, Result, updater::*};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
const NOTICES: &[u8] = b"Synthetic third-party notices\n";
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Default)]
struct Fake {
    replies: Mutex<HashMap<String, (u16, Vec<u8>)>>,
    calls: Mutex<Vec<(String, usize)>>,
}
impl Transport for Fake {
    fn get(&self, url: &str, limit: usize) -> Result<HttpResponse> {
        self.calls.lock().unwrap().push((url.into(), limit));
        let (status, body) = self
            .replies
            .lock()
            .unwrap()
            .get(url)
            .cloned()
            .ok_or_else(|| ApiError {
                code: "fake_network".into(),
                message: "Unexpected URL".into(),
            })?;
        Ok(HttpResponse { status, body })
    }
}
impl Fake {
    fn put(&self, url: &str, status: u16, bytes: Vec<u8>) {
        self.replies
            .lock()
            .unwrap()
            .insert(url.into(), (status, bytes));
    }
}
fn url(version: &str, name: &str) -> String {
    format!("https://github.com/LilyAvelis/Katran/releases/download/v{version}/{name}")
}
fn build(version: &str, exe: &[u8]) -> Value {
    json!({"version":version,"profile":"release","capture_backend":"windows-events","update_format":1,"minimum_launcher_protocol":1,"executable_sha256":hash(exe),"third_party_notices_sha256":hash(NOTICES),"sources":[{"path":"C:\\ignored-source-path","sha256":"not-used-as-path"}]})
}
fn installed(root: &Path, version: &str, exe: &[u8]) -> PathBuf {
    let id = hash(exe)[..12].to_owned();
    let folder = root.join("runtime/versions").join(&id);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("katran-ui.exe"), exe).unwrap();
    fs::write(
        folder.join("build-manifest.json"),
        serde_json::to_vec(&build(version, exe)).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("runtime/Katran.exe"),
        b"synthetic stable launcher",
    )
    .unwrap();
    fs::write(root.join("runtime/current-version.txt"), &id).unwrap();
    folder.join("katran-ui.exe")
}
fn zip(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in members {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    if !members
        .iter()
        .any(|(name, _)| *name == "THIRD-PARTY-NOTICES.txt")
    {
        writer
            .start_file(
                "THIRD-PARTY-NOTICES.txt",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(NOTICES).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn good_zip(version: &str) -> Vec<u8> {
    let exe = b"synthetic next executable";
    zip(&[
        ("katran-ui.exe", exe.to_vec()),
        (
            "build-manifest.json",
            serde_json::to_vec(&build(version, exe)).unwrap(),
        ),
    ])
}
fn publish(fake: &Fake, version: &str, archive: Vec<u8>, change: impl FnOnce(&mut Value)) {
    let archive_url = url(version, &format!("Katran-{version}-update.zip"));
    let mut manifest = json!({"schema_version":1,"version":version,"tag":format!("v{version}"),"repository":"LilyAvelis/Katran","minimum_launcher_protocol":1,"data_schema_version":6,"update":{"url":archive_url,"size_bytes":archive.len(),"sha256":hash(&archive)}});
    change(&mut manifest);
    let bytes = serde_json::to_vec(&manifest).unwrap();
    fake.put(LATEST_URL,200,serde_json::to_vec(&json!({"tag_name":format!("v{version}"),"draft":false,"prerelease":false,"assets":[{"name":"release.json","browser_download_url":url(version,"release.json"),"size":bytes.len()},{"name":format!("Katran-{version}-update.zip"),"browser_download_url":archive_url,"size":archive.len()}]})).unwrap());
    fake.put(&url(version, "release.json"), 200, bytes);
    fake.put(&archive_url, 200, archive);
}
fn fixture() -> (tempfile::TempDir, Updater, Arc<Fake>) {
    let root = tempfile::tempdir().unwrap();
    let exe = installed(root.path(), "2.0.0", b"synthetic running executable");
    let ctx = UpdateContext::from_executable(&exe).unwrap();
    let fake = Arc::new(Fake::default());
    let updater = Updater::new(Some(ctx), fake.clone(), "2.0.0").unwrap();
    (root, updater, fake)
}
fn pointer(root: &Path) -> String {
    fs::read_to_string(root.join("runtime/current-version.txt")).unwrap()
}
fn stages(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root.join("runtime/versions"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".update-stage-")
        })
        .collect()
}
#[test]
fn manual_only_unsupported_and_first_release_404() {
    let fake = Arc::new(Fake::default());
    let updater = Updater::new(None, fake.clone(), "2.0.0").unwrap();
    assert_eq!(updater.check().unwrap().state, "unsupported");
    assert!(fake.calls.lock().unwrap().is_empty());
    let (root, updater, fake) = fixture();
    fake.put(LATEST_URL, 404, vec![]);
    assert_eq!(updater.check().unwrap().state, "unavailable");
    assert!(stages(root.path()).is_empty());
    assert_eq!(fake.calls.lock().unwrap().len(), 1);
}
#[test]
fn verified_update_changes_only_immutable_runtime_and_pointer_on_commit() {
    let (root, updater, fake) = fixture();
    let previous = pointer(root.path());
    let data = root.path().join("katran-data/index.sqlite3");
    fs::create_dir_all(data.parent().unwrap()).unwrap();
    fs::write(&data, b"synthetic personal data sentinel").unwrap();
    publish(&fake, "2.1.0", good_zip("2.1.0"), |_| {});
    let status = updater.check().unwrap();
    assert_eq!(status.state, "available");
    let release = status.release.unwrap();
    let prepared = updater.prepare(&release).unwrap();
    assert_eq!(pointer(root.path()), previous);
    assert_eq!(stages(root.path()).len(), 1);
    let result = updater.commit(prepared).unwrap();
    assert_eq!(result.version, "2.1.0");
    assert!(result.restart_required);
    assert_eq!(pointer(root.path()).trim(), result.build_id);
    assert_eq!(
        fs::read(&data).unwrap(),
        b"synthetic personal data sentinel"
    );
    assert!(
        root.path()
            .join("runtime/versions")
            .join(previous)
            .join("katran-ui.exe")
            .exists()
    );
    assert!(stages(root.path()).is_empty());
    assert_eq!(updater.check().unwrap().state, "up_to_date");
}
#[test]
fn cancelled_preparation_and_mutated_stage_keep_current_pointer() {
    let (root, updater, fake) = fixture();
    let previous = pointer(root.path());
    publish(&fake, "2.1.0", good_zip("2.1.0"), |_| {});
    let release = updater.check().unwrap().release.unwrap();
    let prepared = updater.prepare(&release).unwrap();
    assert_eq!(updater.prepare(&release).err().unwrap().code, "update_busy");
    drop(prepared);
    assert!(stages(root.path()).is_empty());
    let prepared = updater.prepare(&release).unwrap();
    fs::write(stages(root.path())[0].join("katran-ui.exe"), b"tampered").unwrap();
    assert_eq!(
        updater.commit(prepared).err().unwrap().code,
        "invalid_update"
    );
    assert_eq!(pointer(root.path()), previous);
    assert!(stages(root.path()).is_empty());
}
#[test]
fn changed_pointer_never_overwrites_newer_installed_version() {
    let (root, updater, fake) = fixture();
    publish(&fake, "2.1.0", good_zip("2.1.0"), |_| {});
    let release = updater.check().unwrap().release.unwrap();
    let prepared = updater.prepare(&release).unwrap();
    installed(
        root.path(),
        "2.2.0",
        b"newer installation by another process",
    );
    let newer = pointer(root.path());
    assert_eq!(updater.commit(prepared).err().unwrap().code, "stale_update");
    assert_eq!(pointer(root.path()), newer);
}
#[test]
fn zip_whitelist_and_build_manifest_are_mandatory() {
    for name in [
        "../katran-data/index.sqlite3",
        "/katran-ui.exe",
        "C:\\escape.exe",
        "katran-ui.exe:stream",
        "folder/katran-ui.exe",
    ] {
        let (root, updater, fake) = fixture();
        let previous = pointer(root.path());
        let archive = zip(&[
            (name, b"evil".to_vec()),
            ("build-manifest.json", b"{}".to_vec()),
        ]);
        publish(&fake, "2.1.0", archive, |_| {});
        let release = updater.check().unwrap().release.unwrap();
        assert_eq!(
            updater.prepare(&release).err().unwrap().code,
            "invalid_update"
        );
        assert_eq!(pointer(root.path()), previous);
        assert!(stages(root.path()).is_empty());
    }
    for (field, value) in [
        ("capture_backend", json!("polling")),
        ("version", json!("2.9.0")),
        ("profile", json!("debug")),
        ("update_format", json!(2)),
        ("minimum_launcher_protocol", json!(2)),
        ("executable_sha256", json!("0".repeat(64))),
    ] {
        let (_root, updater, fake) = fixture();
        let exe = b"next";
        let mut manifest = build("2.1.0", exe);
        manifest[field] = value;
        publish(
            &fake,
            "2.1.0",
            zip(&[
                ("katran-ui.exe", exe.to_vec()),
                (
                    "build-manifest.json",
                    serde_json::to_vec(&manifest).unwrap(),
                ),
            ]),
            |_| {},
        );
        let release = updater.check().unwrap().release.unwrap();
        assert!(updater.prepare(&release).is_err());
    }
}
#[test]
fn official_https_artifact_metadata_limits_and_stable_versions_are_checked() {
    for (field, value) in [
        ("repository", json!("attacker/Katran")),
        ("minimum_launcher_protocol", json!(2)),
        ("version", json!("2.1.0-beta.1")),
        ("tag", json!("vgarbage")),
        ("schema_version", json!(2)),
    ] {
        let (_root, updater, fake) = fixture();
        publish(&fake, "2.1.0", good_zip("2.1.0"), |manifest| {
            manifest[field] = value
        });
        assert_eq!(updater.check().unwrap_err().code, "invalid_update");
    }
    for asset_url in [
        "http://github.com/LilyAvelis/Katran/releases/download/v2.1.0/Katran-2.1.0-update.zip",
        "https://github.com.evil.test/LilyAvelis/Katran/releases/download/v2.1.0/Katran-2.1.0-update.zip",
        "https://github.com/Other/Repo/releases/download/v2.1.0/Katran-2.1.0-update.zip",
    ] {
        let (_root, updater, fake) = fixture();
        publish(&fake, "2.1.0", good_zip("2.1.0"), |m| {
            m["update"]["url"] = json!(asset_url)
        });
        assert_eq!(updater.check().unwrap_err().code, "invalid_update");
    }
    let (_root, updater, fake) = fixture();
    publish(&fake, "2.1.0", good_zip("2.1.0"), |m| {
        m["update"]["sha256"] = json!("0".repeat(64))
    });
    let release = updater.check().unwrap().release.unwrap();
    assert_eq!(
        updater.prepare(&release).err().unwrap().code,
        "invalid_update"
    );
    fake.put(LATEST_URL, 200, vec![b'x'; MAX_MANIFEST_BYTES + 1]);
    assert_eq!(updater.check().unwrap_err().code, "invalid_update");
}
#[test]
fn unpackaged_context_is_rejected_and_running_version_cannot_downgrade() {
    let root = tempfile::tempdir().unwrap();
    let exe = root.path().join("katran-ui.exe");
    fs::write(&exe, b"unpackaged").unwrap();
    assert!(UpdateContext::from_executable(&exe).is_err());
    let (_root, updater, fake) = fixture();
    publish(&fake, "1.9.0", good_zip("1.9.0"), |_| {});
    assert_eq!(updater.check().unwrap().state, "up_to_date");
}
#[test]
fn redirect_hosts_require_official_https_no_credentials_or_odd_ports() {
    for value in [
        "https://github.com/a",
        "https://release-assets.githubusercontent.com/a?signature=synthetic",
        "https://objects.githubusercontent.com/a",
    ] {
        assert!(is_trusted_download_url(&url::Url::parse(value).unwrap()));
    }
    for value in [
        "http://github.com/a",
        "https://github.com.evil.test/a",
        "https://evil.test/a",
        "https://user:password@github.com/a",
        "https://github.com:8443/a",
    ] {
        assert!(!is_trusted_download_url(&url::Url::parse(value).unwrap()));
    }
}
fn api(service: &mut katran::Service, command: katran::Command) -> Value {
    let reply = service.execute(katran::Request {
        api_version: 1,
        request_id: "updater-api".into(),
        command,
    });
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}
fn poll(service: &mut katran::Service, id: u64) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let result = api(
            service,
            katran::Command::PollJob {
                job_id: katran::domain_jobs::JobId(id),
            },
        );
        if !matches!(
            result["state"].as_str(),
            Some("queued" | "running" | "ready")
        ) {
            return result;
        }
        assert!(std::time::Instant::now() < deadline, "{result}");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
#[test]
fn service_api_checks_manually_then_installs_after_verified_worker_preparation() {
    let (root, updater, fake) = fixture();
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let sentinel = source.path().join("original.txt");
    fs::write(&sentinel, b"untouched source").unwrap();
    let mut service = katran::Service::open(data.path(), katran::DEFAULT_QUOTA).unwrap();
    service.set_updater(updater);
    assert_eq!(
        api(&mut service, katran::Command::GetUpdateStatus)["state"],
        "not_checked"
    );
    assert!(fake.calls.lock().unwrap().is_empty());
    publish(&fake, "2.1.0", good_zip("2.1.0"), |_| {});
    let old = pointer(root.path());
    let check = api(
        &mut service,
        katran::Command::BeginJob {
            command: Box::new(katran::Command::CheckUpdates),
        },
    );
    let checked = poll(&mut service, check["job_id"].as_u64().unwrap());
    assert_eq!(checked["state"], "succeeded");
    assert_eq!(checked["result"]["state"], "available");
    let install = api(
        &mut service,
        katran::Command::BeginJob {
            command: Box::new(katran::Command::InstallUpdate),
        },
    );
    assert_eq!(pointer(root.path()), old);
    let installed = poll(&mut service, install["job_id"].as_u64().unwrap());
    assert_eq!(installed["state"], "succeeded", "{installed}");
    assert_eq!(installed["result"]["state"], "installed");
    assert_eq!(installed["result"]["installed"]["restart_required"], true);
    assert_ne!(pointer(root.path()), old);
    assert_eq!(fs::read(&sentinel).unwrap(), b"untouched source");
    assert!(
        api(&mut service, katran::Command::List { limit: 10 })["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn service_cancel_before_install_effect_retains_pointer_and_data() {
    let (root, updater, fake) = fixture();
    let data = tempfile::tempdir().unwrap();
    let mut service = katran::Service::open(data.path(), katran::DEFAULT_QUOTA).unwrap();
    service.set_updater(updater);
    let stored = api(
        &mut service,
        katran::Command::IngestBytes {
            name: "synthetic.txt".into(),
            bytes: b"data sentinel".to_vec(),
            source: "synthetic".into(),
        },
    );
    publish(&fake, "2.1.0", good_zip("2.1.0"), |_| {});
    let check = api(
        &mut service,
        katran::Command::BeginJob {
            command: Box::new(katran::Command::CheckUpdates),
        },
    );
    assert_eq!(
        poll(&mut service, check["job_id"].as_u64().unwrap())["state"],
        "succeeded"
    );
    let old = pointer(root.path());
    let install = api(
        &mut service,
        katran::Command::BeginJob {
            command: Box::new(katran::Command::InstallUpdate),
        },
    );
    let id = install["job_id"].as_u64().unwrap();
    api(
        &mut service,
        katran::Command::CancelJob {
            job_id: katran::domain_jobs::JobId(id),
        },
    );
    assert_eq!(poll(&mut service, id)["state"], "cancelled");
    assert_eq!(pointer(root.path()), old);
    assert_eq!(
        fs::read(stored["path"].as_str().unwrap()).unwrap(),
        b"data sentinel"
    );
}
#[test]
fn notices_are_required_hashed_plain_text_and_finally_reverified() {
    for notices in [
        vec![],
        vec![0xff],
        b"notice\0hidden".to_vec(),
        b"altered notice".to_vec(),
    ] {
        let (root, updater, fake) = fixture();
        let before = pointer(root.path());
        let exe = b"synthetic next executable";
        let archive = zip(&[
            ("katran-ui.exe", exe.to_vec()),
            (
                "build-manifest.json",
                serde_json::to_vec(&build("2.1.0", exe)).unwrap(),
            ),
            ("THIRD-PARTY-NOTICES.txt", notices),
        ]);
        publish(&fake, "2.1.0", archive, |_| {});
        let release = updater.check().unwrap().release.unwrap();
        assert!(updater.prepare(&release).is_err());
        assert_eq!(pointer(root.path()), before);
    }
    let (root, updater, fake) = fixture();
    publish(&fake, "2.1.0", good_zip("2.1.0"), |_| {});
    let before = pointer(root.path());
    let prepared = updater
        .prepare(&updater.check().unwrap().release.unwrap())
        .unwrap();
    fs::write(
        stages(root.path())[0].join("THIRD-PARTY-NOTICES.txt"),
        b"tampered",
    )
    .unwrap();
    assert!(updater.commit(prepared).is_err());
    assert_eq!(pointer(root.path()), before);
}
#[test]
fn immutable_hash_directory_collision_is_rejected_and_future_format_compatible_major_is_allowed() {
    let (root, updater, fake) = fixture();
    publish(&fake, "3.0.0", good_zip("3.0.0"), |_| {});
    assert_eq!(updater.check().unwrap().state, "available");
    let release = updater.check().unwrap().release.unwrap();
    let prepared = updater.prepare(&release).unwrap();
    let destination = root
        .path()
        .join("runtime/versions")
        .join(&hash(b"synthetic next executable")[..12]);
    fs::create_dir(&destination).unwrap();
    fs::write(
        destination.join("katran-ui.exe"),
        b"foreign immutable contents",
    )
    .unwrap();
    fs::write(destination.join("build-manifest.json"), b"{}").unwrap();
    let previous = pointer(root.path());
    assert!(updater.commit(prepared).is_err());
    assert_eq!(pointer(root.path()), previous);
    assert_eq!(
        fs::read(destination.join("katran-ui.exe")).unwrap(),
        b"foreign immutable contents"
    );
}
