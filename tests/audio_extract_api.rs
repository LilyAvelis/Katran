use katran::{Command, Request, Service, domain_jobs::JobId, interaction::SelectionMode};
use katran::{copy_out::ClipboardSink, interaction::ShelfTab};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::Command as Process,
    time::{Duration, Instant},
};

fn request(api: &mut Service, command: Command) -> Value {
    serde_json::to_value(api.execute(Request {
        api_version: 1,
        request_id: "audio-extract-test".into(),
        command,
    }))
    .unwrap()
}

struct FileClipboard(bool);

impl ClipboardSink for FileClipboard {
    fn write(&mut self, paths: &[std::path::PathBuf]) -> Result<(u32, Vec<String>), String> {
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].extension().and_then(|part| part.to_str()),
            Some("m4a")
        );
        assert!(paths[0].is_file());
        if self.0 {
            Ok((0, Vec::new()))
        } else {
            Err("synthetic clipboard unavailable".into())
        }
    }
}
fn call(api: &mut Service, command: Command) -> Value {
    let response = request(api, command);
    assert_eq!(response["ok"], true, "{response}");
    response["result"].clone()
}
fn tools_available() -> bool {
    ["ffmpeg", "ffprobe"].iter().all(|name| {
        Process::new(name)
            .arg("-version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}
fn fixture(path: &Path, audio: bool) {
    let mut process = Process::new("ffmpeg");
    process.args([
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=80x60:rate=10",
    ]);
    if audio {
        process.args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=44100",
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-map",
            "2:a:0",
            "-c:a",
            "aac",
        ]);
    }
    let status = process
        .args(["-t", "1", "-c:v", "mpeg4", "-y"])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}
fn select(api: &mut Service, folder: &Path, source: &Path) {
    call(
        api,
        Command::ChooseFolder {
            path: folder.to_owned(),
        },
    );
    let page = call(
        api,
        Command::BrowseShelfSorted {
            query: "".into(),
            limit: 100,
            cursor: None,
        },
    );
    let event = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| {
            event["path"]
                .as_str()
                .is_some_and(|path| Path::new(path).file_name() == source.file_name())
        })
        .unwrap_or_else(|| panic!("source missing from folder: {page}"));
    let id = event["id"].as_i64().unwrap();
    call(api, Command::SetVisible { ids: vec![id] });
    call(
        api,
        Command::Select {
            id,
            mode: SelectionMode::Replace,
        },
    );
}
fn begin(api: &mut Service, track: Option<u32>) -> Value {
    request(
        api,
        Command::BeginJob {
            command: Box::new(Command::ExtractVideoAudio { audio_track: track }),
        },
    )
}
fn wait(api: &mut Service, id: JobId) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = call(api, Command::PollJob { job_id: id });
        if matches!(
            status["state"].as_str(),
            Some("succeeded" | "failed" | "cancelled")
        ) {
            return status;
        }
        assert!(Instant::now() < deadline, "timed out: {status}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn run(api: &mut Service, track: Option<u32>) -> Value {
    let started = begin(api, track);
    assert_eq!(started["ok"], true, "{started}");
    wait(api, JobId(started["result"]["job_id"].as_u64().unwrap()))
}
fn assert_failed(api: &mut Service, track: Option<u32>, code: &str) {
    let started = begin(api, track);
    if started["ok"] == false {
        assert_eq!(started["error"]["code"], code, "{started}");
        return;
    }
    let status = wait(api, JobId(started["result"]["job_id"].as_u64().unwrap()));
    assert_eq!(status["state"], "failed", "{status}");
    assert_eq!(status["error"]["code"], code, "{status}");
}
fn audio_properties(path: &Path) {
    assert_eq!(path.extension().and_then(|s| s.to_str()), Some("m4a"));
    let output = Process::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,codec_type:format=format_name",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let info: Value = serde_json::from_slice(&output.stdout).unwrap();
    let streams = info["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 1, "{info}");
    assert_eq!(streams[0]["codec_type"], "audio");
    assert_eq!(streams[0]["codec_name"], "aac");
    assert!(
        info["format"]["format_name"]
            .as_str()
            .unwrap()
            .contains("mp4")
    );
}
fn estimated_frequency(path: &Path) -> f64 {
    let output = Process::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-ac", "1", "-ar", "44100", "-f", "s16le", "-"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let samples: Vec<i16> = output
        .stdout
        .chunks_exact(2)
        .map(|s| i16::from_le_bytes([s[0], s[1]]))
        .collect();
    assert!(samples.len() > 22050);
    // Ignore AAC priming and ending; each negative-to-positive crossing is one period.
    let start = samples.len() / 4;
    let end = samples.len() * 3 / 4;
    let crossings = samples[start..end]
        .windows(2)
        .filter(|s| s[0] <= 0 && s[1] > 0)
        .count();
    crossings as f64 * 44100.0 / (end - start) as f64
}
fn m4a_files(folder: &Path) -> Vec<std::path::PathBuf> {
    if !folder.exists() {
        return Vec::new();
    }
    fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("audio-"))
                && p.is_dir()
        })
        .map(|p| p.join("audio.m4a"))
        .filter(|p| p.is_file())
        .collect()
}
fn assert_output_path(path: &Path, output_dir: &Path) {
    let canonical = path.canonicalize().unwrap();
    let output_dir = output_dir.canonicalize().unwrap();
    let directory = canonical.parent().unwrap();
    assert_eq!(directory.parent(), Some(output_dir.as_path()));
    assert!(
        directory
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("audio-")
    );
    assert_eq!(canonical.file_name().unwrap(), "audio.m4a");
}

#[test]
fn selected_audio_ordinal_and_failure_paths() {
    if !tools_available() {
        eprintln!("SKIP: ffmpeg and ffprobe are required");
        return;
    }
    let profile = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("два трека.mp4");
    fixture(&source, true);
    let original = fs::read(&source).unwrap();
    let mut api = Service::open(profile.path(), 100_000_000).unwrap();
    api.set_media_capture_clipboard_sink(Box::new(FileClipboard(true)));
    select(&mut api, folder.path(), &source);
    let first = run(&mut api, None);
    assert_eq!(first["state"], "succeeded", "{first}");
    assert_eq!(first["result"]["files"]["ok"], true);
    assert_eq!(first["result"]["clipboard"]["ok"], true);
    assert_eq!(first["result"]["event"]["kind"], "file_reference");
    assert_eq!(first["result"]["audio_track"], 0);
    let first_path = Path::new(first["result"]["path"].as_str().unwrap());
    let output_dir = profile.path().join("audio-extractions");
    assert_output_path(first_path, &output_dir);
    audio_properties(first_path);
    let first_bytes = fs::read(first_path).unwrap();
    let frequency = estimated_frequency(first_path);
    assert!(
        (frequency - 440.0).abs() < 25.0,
        "first track: {frequency} Hz"
    );
    let second = run(&mut api, Some(1));
    assert_eq!(second["state"], "succeeded", "{second}");
    assert_eq!(second["result"]["audio_track"], 1);
    let second_path = Path::new(second["result"]["path"].as_str().unwrap());
    assert_output_path(second_path, &output_dir);
    assert_ne!(first_path, second_path);
    assert_eq!(fs::read(first_path).unwrap(), first_bytes);
    audio_properties(second_path);
    let frequency = estimated_frequency(second_path);
    assert!(
        (frequency - 880.0).abs() < 35.0,
        "second track: {frequency} Hz"
    );
    let second_bytes = fs::read(second_path).unwrap();
    api.set_media_capture_clipboard_sink(Box::new(FileClipboard(false)));
    let repeat = run(&mut api, Some(1));
    assert_eq!(repeat["state"], "succeeded", "{repeat}");
    assert_eq!(repeat["result"]["files"]["ok"], true);
    assert_eq!(repeat["result"]["clipboard"]["ok"], false);
    assert_eq!(repeat["result"]["audio_track"], 1);
    let repeat_path = Path::new(repeat["result"]["path"].as_str().unwrap());
    assert_output_path(repeat_path, &output_dir);
    assert_ne!(repeat_path, first_path);
    assert_ne!(repeat_path, second_path);
    audio_properties(repeat_path);
    assert_eq!(fs::read(first_path).unwrap(), first_bytes);
    assert_eq!(fs::read(second_path).unwrap(), second_bytes);
    let before = m4a_files(&output_dir);
    assert_eq!(before.len(), 3);
    assert_failed(&mut api, Some(2), "invalid_audio_track");
    assert_eq!(m4a_files(&output_dir).len(), before.len());
    assert_eq!(fs::read(first_path).unwrap(), first_bytes);
    assert_eq!(fs::read(&source).unwrap(), original);
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Files,
        },
    );
    let files = call(
        &mut api,
        Command::BrowseShelfSorted {
            query: String::new(),
            limit: 10,
            cursor: None,
        },
    );
    assert_eq!(files["events"].as_array().unwrap().len(), 3);
}

#[test]
fn silent_and_damaged_sources_fail_without_output() {
    if !tools_available() {
        eprintln!("SKIP: ffmpeg and ffprobe are required");
        return;
    }
    for damaged in [false, true] {
        let profile = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("source.mp4");
        if damaged {
            fs::write(&source, b"not a video").unwrap();
        } else {
            fixture(&source, false);
        }
        let original = fs::read(&source).unwrap();
        let mut api = Service::open(profile.path(), 100_000_000).unwrap();
        select(&mut api, folder.path(), &source);
        assert_failed(
            &mut api,
            None,
            if damaged {
                "audio_extract_error"
            } else {
                "no_audio"
            },
        );
        assert!(m4a_files(&profile.path().join("audio-extractions")).is_empty());
        assert_eq!(fs::read(&source).unwrap(), original);
    }
}

#[test]
fn cancelled_before_commit_does_not_publish() {
    if !tools_available() {
        eprintln!("SKIP: ffmpeg and ffprobe are required");
        return;
    }
    let profile = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("cancel.mp4");
    fixture(&source, true);
    let original = fs::read(&source).unwrap();
    let mut api = Service::open(profile.path(), 100_000_000).unwrap();
    select(&mut api, folder.path(), &source);
    let started = begin(&mut api, None);
    assert_eq!(started["ok"], true, "{started}");
    let job_id = JobId(started["result"]["job_id"].as_u64().unwrap());
    let cancelled = request(&mut api, Command::CancelJob { job_id });
    assert_eq!(
        cancelled["ok"], true,
        "cancellation must win before dispatcher commit: {cancelled}"
    );
    let status = wait(&mut api, job_id);
    assert_eq!(status["state"], "cancelled", "{status}");
    assert!(m4a_files(&profile.path().join("audio-extractions")).is_empty());
    assert_eq!(fs::read(&source).unwrap(), original);
}
