use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::Value;

fn call(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "profile-runtime".into(),
        command,
    })
}

#[test]
fn profile_runtime_reports_missing_then_present() {
    let data = tempfile::tempdir().unwrap();
    let runtime = data.path().join("emoji-ai-runtime");
    std::fs::create_dir(&runtime).unwrap();
    // This integration-test executable has a single test and no worker threads yet.
    unsafe {
        std::env::remove_var("KATRAN_EMOJI_AI_RUNTIME_ROOT");
        std::env::remove_var("KATRAN_E_RUNTIME_ROOT");
    }
    let mut api = Service::open(data.path(), 10_000).unwrap();
    let status = call(&mut api, Command::EmojiAiStatus);
    assert_eq!(status["error"]["code"], "model_unavailable", "{status}");
    assert!(
        status["error"]["message"]
            .as_str()
            .unwrap()
            .contains("docs/emoji-ai.md")
    );
    for command in [
        Command::SetTabEnabled {
            tab: ShelfTab::Pods,
            enabled: true,
        },
        Command::SetTab {
            tab: ShelfTab::Pods,
        },
        Command::SetEmojiCatalogue { enabled: true },
    ] {
        let result = call(&mut api, command);
        assert_eq!(result["ok"], true, "{result}");
    }
    let result = call(
        &mut api,
        Command::SearchEmojiAi {
            query: "cow".into(),
            offset: 0,
            limit: 2,
        },
    );
    assert_eq!(result["error"]["code"], "model_unavailable", "{result}");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("docs/emoji-ai.md")
    );

    // Synthetic names only: the cheap status checks presence, not model integrity.
    let mut paths = vec![
        "local-experiments/emoji-ai/worker.py",
        "local-experiments/emoji-ai/semantic_search.py",
        "local-experiments/emoji-ai/model/onnx/model.onnx",
        "local-experiments/emoji-ai/model/onnx/tokenizer.json",
        "local-experiments/emoji-ai/cache-cldr/manifest.json",
        "local-experiments/emoji-ai/cache-cldr/passages.npy",
        "assets/unicode/emoji-test.txt",
        "local-experiments/emoji-ai/cldr/cldr-annotations-full-en.json",
        "local-experiments/emoji-ai/cldr/cldr-annotations-derived-full-en.json",
        "local-experiments/emoji-ai/cldr/cldr-annotations-full-ru.json",
        "local-experiments/emoji-ai/cldr/cldr-annotations-derived-full-ru.json",
        "local-experiments/emoji-ai/katran-e-runtime.json",
    ];
    #[cfg(windows)]
    paths.push("local-experiments/emoji-ai/.venv/Scripts/python.exe");
    #[cfg(not(windows))]
    paths.push("local-experiments/emoji-ai/.venv/bin/python");
    for path in paths {
        let file = runtime.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, b"synthetic").unwrap();
    }
    let status = call(&mut api, Command::EmojiAiStatus);
    assert_eq!(status["ok"], true, "{status}");
    assert_eq!(status["result"]["available"], true);
    // Status is not proof the inputs are valid; search must still validate them.
    let result = call(
        &mut api,
        Command::SearchEmojiAi {
            query: "cow".into(),
            offset: 0,
            limit: 2,
        },
    );
    assert_eq!(result["error"]["code"], "emoji_ai_unavailable", "{result}");
}
