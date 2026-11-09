use katran::{
    Command,
    diagnostics::{Logger, summarize_command},
};
use serde_json::{Value, json};
use std::{fs, time::Duration};

#[test]
fn records_are_parseable_capped_and_session_scoped() {
    let root = tempfile::tempdir().unwrap();
    let logger = Logger::start(root.path()).unwrap();
    logger.record("input", json!({"text":"synthetic"}));
    logger.record("paste", json!({"nested":[{"text":"normal query sk-SYNTHETIC0123456789; ghp_SYNTHETIC0123456789; github_pat_SYNTHETIC0123456789"}],"ordinary":"find sk-short examples"}));
    logger
        .clone()
        .record("oversized", json!({"text":"x".repeat(20000)}));
    assert!(logger.flush(Duration::from_secs(5)));
    let content = fs::read_to_string(root.path().join("logs/events.jsonl")).unwrap();
    let rows: Vec<Value> = content
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["session"], rows[1]["session"]);
    assert_eq!(rows[0]["sequence"], 0);
    assert_eq!(rows[1]["sequence"], 1);
    assert_eq!(
        rows[1]["data"]["nested"][0]["text"],
        "normal query [REDACTED]; [REDACTED]; [REDACTED]"
    );
    assert_eq!(rows[1]["data"]["ordinary"], "find sk-short examples");
    assert!(!content.contains("SYNTHETIC0123456789"));
    assert_eq!(rows[2]["data"]["truncated"], true);
}
#[test]
fn command_summary_never_contains_ingested_bytes() {
    let command = Command::IngestBytes {
        name: "synthetic.png".into(),
        bytes: vec![11, 22, 33],
        source: "test".into(),
    };
    let summary = summarize_command(&command);
    assert!(summary.get("bytes").is_none());
    assert_eq!(summary["byte_count"], 3);
    let nested = Command::IngestWithOrigin {
        origin: Default::default(),
        command: Box::new(command),
    };
    let nested_summary = summarize_command(&nested);
    assert!(nested_summary["command"].get("bytes").is_none());
    assert_eq!(nested_summary["command"]["byte_count"], 3);
}
