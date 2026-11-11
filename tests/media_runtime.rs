use katran::media_runtime;

#[test]
fn tool_names_are_fixed_and_status_is_structured() {
    assert!(media_runtime::command("../../ffmpeg").is_err());
    assert!(media_runtime::command("cmd").is_err());
    let status = media_runtime::status();
    assert!(status["available"].is_boolean());
    assert!(status["source"].is_string());
}
