use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
use tempfile::tempdir;

#[test]
fn real_process_five_objects_restart_and_import() {
    let root = tempdir().unwrap();
    let destination = tempdir().unwrap();
    let run = |requests: Vec<Value>| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_katran"))
            .arg("serve-stdio")
            .arg(root.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        for (i, command) in requests.into_iter().enumerate() {
            writeln!(
                input,
                "{}",
                json!({"api_version":1,"request_id":i.to_string(),"command":command})
            )
            .unwrap();
        }
        drop(input);
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>()
    };
    let first = run((1..=5).map(|n| json!({"type":"ingest_bytes","name":format!("image{n}.bin"),"bytes":[n],"source":"process-test"})).collect());
    assert!(first.iter().all(|r| r["ok"] == true));
    let next = run(vec![
        json!({"type":"list"}),
        json!({"type":"import","ids":[1,3,5],"destination":destination.path()}),
    ]);
    assert_eq!(next[0]["result"]["events"].as_array().unwrap().len(), 5);
    for n in [1u8, 3, 5] {
        assert_eq!(
            std::fs::read(destination.path().join(format!("image{n}.bin"))).unwrap(),
            vec![n]
        );
    }
}
