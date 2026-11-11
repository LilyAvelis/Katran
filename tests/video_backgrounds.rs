use katran::video_backgrounds::{catalogue, commit, get, prepare};
use std::{fs, process::Command};

#[test]
fn invalid_media_and_ids_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let bad = temp.path().join("bad.mp4");
    fs::write(&bad, b"not a movie").unwrap();
    assert!(prepare(&bad).is_err());
    assert!(get(temp.path(), "../../outside").is_err());
    let large = temp.path().join("large.mp4");
    let f = fs::File::create(&large).unwrap();
    f.set_len(128 * 1024 * 1024 + 1).unwrap();
    assert!(prepare(&large).is_err());
}

#[test]
fn import_strips_audio_and_preserves_original() {
    // FFmpeg is an optional external runtime dependency; absence is covered by
    // the explicit error path, not by requiring it on every contributor machine.
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x90:rate=24",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440",
            "-t",
            "2",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-y",
        ])
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    let before = fs::read(&source).unwrap();
    let result = commit(temp.path(), prepare(&source).unwrap()).unwrap();
    assert_eq!(fs::read(&source).unwrap(), before);
    assert_eq!(result["width"], 160);
    assert_eq!(result["height"], 90);
    let id = result["id"].as_str().unwrap();
    assert_eq!(get(temp.path(), id).unwrap(), result);
    assert_eq!(catalogue(temp.path()).unwrap().as_array().unwrap().len(), 1);
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
        ])
        .arg(result["path"].as_str().unwrap())
        .output()
        .unwrap();
    assert!(probe.status.success());
    assert!(probe.stdout.is_empty());
    let managed = std::path::PathBuf::from(result["path"].as_str().unwrap());
    fs::write(&managed, b"tampered").unwrap();
    assert!(commit(temp.path(), prepare(&source).unwrap()).is_err());
    let metadata = temp
        .path()
        .join("video-backgrounds")
        .join(format!("{id}.json"));
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
    value["width"] = serde_json::json!(99999);
    fs::write(metadata, value.to_string()).unwrap();
    assert!(get(temp.path(), id).is_err());
}
