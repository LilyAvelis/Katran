use katran::api::wire_v2;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

#[test]
fn v2_media_sessions_and_nested_open_keep_lossless_identities() {
    for command in [
        json!({"type":"begin_job","command":{"type":"open_media","id":"-4611686018427387905","revision":"r"}}),
        json!({"type":"media_frame","session":"18446744073709551615","after_serial":"9007199254740992"}),
        json!({"type":"control_media","session":"18446744073709551615","action":{"type":"seek","position_ms":"9007199254740992"}}),
    ] {
        assert!(
            wire_v2::decode(json!({"api_version":2,"request_id":"media","command":command}))
                .is_ok()
        );
    }
    for token in [json!(9007199254740992_u64), json!("01"), json!("-1")] {
        assert!(wire_v2::decode(json!({"api_version":2,"request_id":"media","command":{"type":"control_media","session":token,"action":{"type":"pause"}}})).is_err());
    }
}

#[test]
fn v2_rejects_unsafe_or_malformed_id_inputs_and_handles_nested_ids() {
    for value in [
        json!(9007199254740992_i64),
        json!("01"),
        json!("-0"),
        json!("9223372036854775808"),
        json!("1e3"),
    ] {
        let request =
            json!({"api_version":2,"request_id":"x","command":{"type":"select","id":value}});
        assert!(wire_v2::decode(request).is_err());
    }
    let request = json!({"api_version":2,"request_id":"x","command":{"type":"begin_job","command":{"type":"copy_events_checked","items":[{"id":"-4611686018427387905","revision":"r"}]}}});
    assert!(wire_v2::decode(request).is_ok());
    let mut response =
        json!({"id":-4611686018427387905_i64,"safe":9007199254740991_i64,"other":"123"});
    wire_v2::encode(&mut response);
    assert_eq!(response["id"], "-4611686018427387905");
    assert_eq!(response["safe"], 9007199254740991_i64);
    assert_eq!(response["other"], "123");
    // String-valued command IDs remain strings.
    assert!(
        wire_v2::decode(
            json!({"api_version":2,"request_id":"x","command":{"type":"run_action","id":"123"}})
        )
        .is_ok()
    );
}

#[test]
fn stdio_v2_emoji_id_round_trips_without_changing_v1() {
    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_katran"))
        .arg("serve-stdio")
        .arg(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    {
        let mut send = |version: u32, command: Value| -> Value {
            writeln!(
                input,
                "{}",
                json!({"api_version":version,"request_id":"test","command":command})
            )
            .unwrap();
            input.flush().unwrap();
            let mut line = String::new();
            output.read_line(&mut line).unwrap();
            serde_json::from_str(&line).unwrap()
        };
        let image_path = root.path().join("synthetic.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]))
            .save(&image_path)
            .unwrap();
        assert_eq!(
            send(
                2,
                json!({"type":"import_theme_background","path":image_path})
            )["ok"],
            true
        );
        assert_eq!(
            send(2, json!({"type":"set_tab","tab":"desktop"}))["ok"],
            true
        );
        let desktop = send(
            2,
            json!({"type":"browse_shelf_sorted","query":"","limit":10,"cursor":null}),
        );
        let desktop_id = desktop["result"]["events"][0]["id"].as_str().unwrap();
        assert!(desktop_id.parse::<i64>().unwrap().unsigned_abs() > 9_007_199_254_740_991);
        assert_eq!(send(2, json!({"type":"get","id":desktop_id}))["ok"], true);
        for command in [
            json!({"type":"set_tab_enabled","tab":"pods","enabled":true}),
            json!({"type":"set_tab","tab":"pods"}),
            json!({"type":"set_emoji_catalogue","enabled":true}),
        ] {
            assert_eq!(send(2, command)["ok"], true);
        }
        let browse =
            json!({"type":"browse_shelf_sorted","query":"cow face","limit":10,"cursor":null});
        let v2 = send(2, browse.clone());
        assert_eq!(v2["api_version"], 2);
        let id = v2["result"]["events"][0]["id"].as_str().unwrap();
        assert!(id.parse::<i64>().unwrap().unsigned_abs() > 9_007_199_254_740_991);
        let desc = send(2, json!({"type":"describe_pod","id":id}));
        assert_eq!(desc["ok"], true, "{desc}");
        assert_eq!(
            desc["result"]["text"],
            v2["result"]["events"][0]["text_preview"]
        );
        let bad = send(
            2,
            json!({"type":"describe_pod","id":id.parse::<i64>().unwrap()}),
        );
        assert_eq!(bad["error"]["code"], "invalid_request");
        let v1 = send(1, browse);
        assert!(v1["result"]["events"][0]["id"].as_i64().is_some());
    }
    drop(input);
    child.wait().unwrap();
}

#[test]
fn job_ids_round_trip_as_unsigned_decimal_strings() {
    for n in [9_007_199_254_740_992_u64, u64::MAX] {
        let mut response = json!({"job_id":n});
        wire_v2::encode(&mut response);
        assert_eq!(response["job_id"], n.to_string());
        for command in ["poll_job", "cancel_job"] {
            let request = wire_v2::decode(json!({"api_version":2,"request_id":"x","command":{"type":command,"job_id":response["job_id"]}})).unwrap();
            match request.command {
                katran::Command::PollJob { job_id } | katran::Command::CancelJob { job_id } => {
                    assert_eq!(job_id.0, n)
                }
                _ => panic!("unexpected command"),
            }
            assert!(
                wire_v2::decode(
                    json!({"api_version":2,"request_id":"x","command":{"type":command,"job_id":n}})
                )
                .is_err()
            );
        }
    }
    for bad in ["-1", "01", "18446744073709551616"] {
        assert!(
            wire_v2::decode(
                json!({"api_version":2,"request_id":"x","command":{"type":"poll_job","job_id":bad}})
            )
            .is_err()
        );
    }
}

#[test]
fn overlay_window_identity_round_trips_without_precision_loss() {
    let identity = "18446744073709551615";
    let command = json!({"type":"summon_captured_overlay", "owner_window":identity,
        "capture":{"previous":{"hwnd":identity,"pid":1,"process_started":identity},
        "work_area":{"left":0,"top":0,"right":100,"bottom":100}}});
    let request =
        wire_v2::decode(json!({"api_version":2,"request_id":"x","command":command})).unwrap();
    match request.command {
        katran::Command::SummonCapturedOverlay {
            owner_window,
            capture,
        } => {
            assert_eq!(owner_window, u64::MAX);
            assert_eq!(capture.previous.hwnd, u64::MAX);
            assert_eq!(capture.previous.process_started, u64::MAX);
        }
        _ => panic!("unexpected command"),
    }
    assert!(wire_v2::decode(json!({"api_version":2,"request_id":"x","command":{"type":"summon_overlay","owner_window":u64::MAX}})).is_err());
}

#[test]
fn tab_summon_decodes_lossless_window_identity() {
    let identity = "18446744073709551615";
    let command = json!({"type":"summon_tab_overlay", "tab":"text", "owner_window":identity,
        "capture":{"previous":{"hwnd":identity,"pid":1,"process_started":identity},
        "work_area":{"left":0,"top":0,"right":100,"bottom":100}}});
    let request =
        wire_v2::decode(json!({"api_version":2,"request_id":"tab","command":command})).unwrap();
    match request.command {
        katran::Command::SummonTabOverlay {
            owner_window,
            capture: Some(capture),
            ..
        } => {
            assert_eq!(owner_window, u64::MAX);
            assert_eq!(capture.previous.hwnd, u64::MAX);
            assert_eq!(capture.previous.process_started, u64::MAX);
        }
        _ => panic!("unexpected command"),
    }
}

#[test]
fn video_actions_decode_optional_lossless_session() {
    for command in ["capture_media_frame", "toggle_media_pause"] {
        for session in [serde_json::Value::Null, json!(u64::MAX.to_string())] {
            let request = wire_v2::decode(json!({"api_version":2,"request_id":"video","command":{"type":command,"session":session}})).unwrap();
            let actual = match request.command {
                katran::Command::CaptureMediaFrame { session }
                | katran::Command::ToggleMediaPause { session } => session,
                _ => panic!("unexpected command"),
            };
            assert_eq!(actual, (!session.is_null()).then_some(u64::MAX));
        }
        assert!(wire_v2::decode(json!({"api_version":2,"request_id":"video","command":{"type":command,"session":u64::MAX}})).is_err());
        assert!(
            wire_v2::decode(
                json!({"api_version":2,"request_id":"video","command":{"type":command}})
            )
            .is_ok()
        );
    }
}
