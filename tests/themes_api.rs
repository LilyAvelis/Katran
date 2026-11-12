use katran::{
    Command, Request, Service,
    theme::{Preset, ThemePreferences},
};
use serde_json::Value;
#[test]
fn window_animation_preferences_round_trip_through_theme_api() {
    let legacy: ThemePreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(legacy.animation_show_style, "slide_down");
    assert_eq!(legacy.animation_show_ms, 220);
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    let theme = ThemePreferences {
        animation_show_style: "none".into(),
        animation_hide_style: "slide_left".into(),
        animation_show_ms: 80,
        animation_hide_ms: 350,
        ..Default::default()
    };
    call(
        &mut api,
        Command::SetTheme {
            theme: theme.clone(),
        },
    );
    drop(api);
    let mut api = Service::open(root.path(), 10_000).unwrap();
    assert_eq!(
        call(&mut api, Command::Themes)["current"],
        serde_json::to_value(theme).unwrap()
    );
}
#[test]
fn new_window_animation_styles_validate_through_theme_api() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    for style in ["zoom", "fade", "zoom_fade"] {
        let theme = ThemePreferences {
            animation_show_style: style.into(),
            animation_hide_style: style.into(),
            ..Default::default()
        };
        call(&mut api, Command::SetTheme { theme });
        assert_eq!(
            call(&mut api, Command::Themes)["current"]["animation_show_style"],
            style
        );
    }
}
#[test]
fn scrollbar_preferences_default_and_persist_independently() {
    let legacy: ThemePreferences = serde_json::from_str("{}").unwrap();
    assert!(!legacy.scrollbar_glass);
    assert!(legacy.scrollbar_always_visible);
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    let theme = ThemePreferences {
        scrollbar_glass: true,
        scrollbar_always_visible: false,
        ..Default::default()
    };
    call(
        &mut api,
        Command::SetTheme {
            theme: theme.clone(),
        },
    );
    drop(api);
    let mut api = Service::open(root.path(), 10_000).unwrap();
    assert_eq!(
        call(&mut api, Command::Themes)["current"],
        serde_json::to_value(theme).unwrap()
    );
}
#[test]
fn experimental_glass_tabs_are_opt_in_and_persist() {
    let legacy: ThemePreferences = serde_json::from_str("{}").unwrap();
    assert!(!legacy.experimental_glass_tabs);
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    let theme = ThemePreferences {
        preset: Preset::BlueExperimental,
        experimental_glass_tabs: true,
        ..Default::default()
    };
    call(&mut api, Command::SetTheme { theme });
    drop(api);
    let mut api = Service::open(root.path(), 10_000).unwrap();
    assert_eq!(
        call(&mut api, Command::Themes)["current"]["experimental_glass_tabs"],
        true
    );
}
#[test]
fn video_seam_smoothing_defaults_on_and_persists_independently() {
    let legacy: ThemePreferences = serde_json::from_str("{}").unwrap();
    assert!(legacy.video_seam_smoothing);
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let theme = ThemePreferences {
        video_seam_smoothing: false,
        ..Default::default()
    };
    call(&mut api, Command::SetTheme { theme });
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(
        call(&mut api, Command::Themes)["current"]["video_seam_smoothing"],
        false
    );
}
#[test]
fn background_video_playback_is_opt_in_and_persists() {
    let legacy: ThemePreferences = serde_json::from_str("{}").unwrap();
    assert!(!legacy.video_play_hidden);
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10_000).unwrap();
    let theme = ThemePreferences {
        video_play_hidden: true,
        ..Default::default()
    };
    call(&mut api, Command::SetTheme { theme });
    drop(api);
    let mut api = Service::open(root.path(), 10_000).unwrap();
    assert_eq!(
        call(&mut api, Command::Themes)["current"]["video_play_hidden"],
        true
    );
}
#[test]
fn selection_color_is_independent_persistent_and_validated() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let old: ThemePreferences = serde_json::from_str("{}").unwrap();
    assert_eq!(old.selection, None);
    assert_eq!(old.selection_intensity, 35);
    let theme = ThemePreferences {
        preset: Preset::BlueExperimental,
        accent: Some([240, 50, 70]),
        selection: Some([40, 230, 100]),
        selection_intensity: 70,
        ..Default::default()
    };
    call(
        &mut api,
        Command::SetTheme {
            theme: theme.clone(),
        },
    );
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(
        call(&mut api, Command::Themes)["current"],
        serde_json::to_value(&theme).unwrap()
    );
    let mut invalid = theme.clone();
    invalid.selection_intensity = 101;
    let response = api.execute(Request {
        api_version: 1,
        request_id: "invalid-selection".into(),
        command: Command::SetTheme { theme: invalid },
    });
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "invalid_settings");
    assert_eq!(
        call(&mut api, Command::Themes)["current"],
        serde_json::to_value(theme).unwrap()
    );
    let mut reset = old;
    reset.preset = Preset::BlueExperimental;
    call(&mut api, Command::SetTheme { theme: reset });
    assert!(call(&mut api, Command::Themes)["current"]["selection"].is_null());
}
fn call(api: &mut Service, c: Command) -> Value {
    let r = api.execute(Request {
        api_version: 1,
        request_id: "themes".into(),
        command: c,
    });
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
#[test]
fn theme_is_optional_persistent_and_does_not_change_behavior_preferences() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let original = call(&mut api, Command::GetSettings);
    let interaction = call(&mut api, Command::Interaction);
    let catalog = call(&mut api, Command::Themes);
    assert_eq!(catalog["current"]["preset"], "blue_experimental");
    assert_eq!(catalog["current"]["lily_background"], 1);
    assert!(catalog["current"]["video_background"].is_null());
    assert_eq!(catalog["themes"][1]["experimental"], true);
    let pref = ThemePreferences {
        preset: Preset::BlueExperimental,
        accent: Some([98, 146, 240]),
        decoration: true,
        lily_background: None,
        custom_background: None,
        glass: false,
        compact_header: false,
        optical_strength: 0,
        ..Default::default()
    };
    call(
        &mut api,
        Command::SetTheme {
            theme: pref.clone(),
        },
    );
    let mut changed = call(&mut api, Command::GetSettings);
    changed["theme"] = original["theme"].clone();
    assert_eq!(changed, original);
    assert_eq!(call(&mut api, Command::Interaction), interaction);
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(
        call(&mut api, Command::Themes)["current"],
        serde_json::to_value(pref).unwrap()
    );
    call(
        &mut api,
        Command::SetTheme {
            theme: Default::default(),
        },
    );
    let reset = call(&mut api, Command::GetSettings);
    assert_eq!(reset["theme"]["preset"], "standard");
    let old: katran::settings::Settings = serde_json::from_value(serde_json::json!({})).unwrap();
    assert_eq!(old.theme.preset, Preset::BlueExperimental);
    assert!(
        serde_json::from_value::<ThemePreferences>(serde_json::json!({"preset":"remote_url"}))
            .is_err()
    );
}

#[test]
fn lily_and_notice_preferences_are_validated_and_persisted() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let mut settings: katran::settings::Settings =
        serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
    settings.theme.lily_background = Some(5);
    settings.sort_notifications = false;
    call(
        &mut api,
        Command::SetSettings {
            settings: settings.clone(),
        },
    );
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(
        call(&mut api, Command::GetSettings)["sort_notifications"],
        false
    );
    assert_eq!(
        call(&mut api, Command::Themes)["current"]["lily_background"],
        5
    );
    settings.theme.lily_background = Some(6);
    assert!(settings.validate(0).is_err());
}

#[test]
fn experimental_window_preferences_roundtrip_through_api() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let theme = ThemePreferences {
        preset: Preset::BlueExperimental,
        glass: true,
        lily_background: Some(2),
        compact_header: true,
        optical_strength: 0,
        ..Default::default()
    };
    call(
        &mut api,
        Command::SetTheme {
            theme: theme.clone(),
        },
    );
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(
        call(&mut api, Command::Themes)["current"],
        serde_json::to_value(theme).unwrap()
    );
}

#[test]
fn restart_is_a_host_intent_and_preserves_settings_and_interaction() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let before = call(&mut api, Command::GetSettings);
    let interaction = call(&mut api, Command::Interaction);
    assert_eq!(
        call(&mut api, Command::RestartApplication)["restart_requested"],
        true
    );
    assert_eq!(call(&mut api, Command::GetSettings), before);
    assert_eq!(call(&mut api, Command::Interaction), interaction);
}

#[test]
fn custom_background_import_select_and_reopen_preserve_managed_copy_and_presets() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("my-background.png");
    image::RgbaImage::new(8, 6).save(&path).unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let item = call(
        &mut api,
        Command::ImportThemeBackground { path: path.clone() },
    );
    let id = item["id"].as_str().unwrap().to_owned();
    assert_eq!(
        call(
            &mut api,
            Command::ImportThemeBackground { path: path.clone() }
        )["id"],
        id
    );
    let theme = ThemePreferences {
        preset: Preset::BlueExperimental,
        custom_background: Some(id.clone()),
        glass: true,
        ..Default::default()
    };
    call(&mut api, Command::SetTheme { theme });
    std::fs::remove_file(path).unwrap();
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(
        call(&mut api, Command::Themes)["current"]["custom_background"],
        id
    );
    assert!(
        std::path::Path::new(
            call(&mut api, Command::GetThemeBackground { id: id.clone() })["path"]
                .as_str()
                .unwrap()
        )
        .exists()
    );
    assert_eq!(
        call(&mut api, Command::ListThemeBackgrounds)["backgrounds"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    call(
        &mut api,
        Command::SetTheme {
            theme: ThemePreferences {
                preset: Preset::BlueExperimental,
                lily_background: Some(3),
                ..Default::default()
            },
        },
    );
    assert_eq!(
        call(&mut api, Command::Themes)["current"]["lily_background"],
        3
    );
    let r = api.execute(Request {
        api_version: 1,
        request_id: "invalid".into(),
        command: Command::GetThemeBackground {
            id: "../outside".into(),
        },
    });
    assert_eq!(r["ok"], false);
}

#[test]
fn selected_image_action_copies_and_applies_background_through_sync_and_job_api() {
    for worker in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let png = source.path().join("image.png");
        image::RgbaImage::new(8, 6).save(&png).unwrap();
        let bytes = std::fs::read(png).unwrap();
        let mut api = Service::open(root.path(), 10000).unwrap();
        let e = call(
            &mut api,
            Command::IngestBytes {
                name: "my.png".into(),
                bytes: bytes.clone(),
                source: "synthetic".into(),
            },
        );
        let id = e["id"].as_i64().unwrap();
        call(&mut api, Command::SetVisible { ids: vec![id] });
        call(
            &mut api,
            Command::Select {
                id,
                mode: katran::interaction::SelectionMode::Replace,
            },
        );
        assert!(
            call(&mut api, Command::Actions)
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == "use_as_background")
        );
        let result = if worker {
            let job = call(
                &mut api,
                Command::BeginJob {
                    command: Box::new(Command::RunAction {
                        id: "use_as_background".into(),
                    }),
                },
            );
            let job_id = serde_json::from_value(job["job_id"].clone()).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                let status = call(&mut api, Command::PollJob { job_id });
                if status["state"] == "succeeded" {
                    break status["result"].clone();
                }
                assert!(std::time::Instant::now() < deadline, "{status}");
                std::thread::yield_now();
            }
        } else {
            call(
                &mut api,
                Command::RunAction {
                    id: "use_as_background".into(),
                },
            )
        };
        assert_eq!(result["background_applied"], true);
        assert_eq!(
            call(&mut api, Command::Themes)["current"]["custom_background"],
            result["id"]
        );
        assert_eq!(std::fs::read(e["path"].as_str().unwrap()).unwrap(), bytes);
    }
}

#[test]
fn cancelled_background_job_never_commits_a_managed_copy() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("bg.png");
    image::RgbaImage::new(8, 6).save(&path).unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let started = call(
        &mut api,
        Command::BeginJob {
            command: Box::new(Command::ImportThemeBackground { path }),
        },
    );
    let job_id = serde_json::from_value(started["job_id"].clone()).unwrap();
    assert_eq!(started["state"], "queued");
    call(&mut api, Command::CancelJob { job_id });
    assert!(
        call(&mut api, Command::ListThemeBackgrounds)["backgrounds"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn image_action_availability_is_metadata_only_but_execution_checks_actual_bytes() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let png = source.path().join("image.png");
    image::RgbaImage::new(8, 6).save(&png).unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let e = call(
        &mut api,
        Command::IngestBytes {
            name: "stale.png".into(),
            bytes: std::fs::read(&png).unwrap(),
            source: "synthetic".into(),
        },
    );
    let id = e["id"].as_i64().unwrap();
    call(&mut api, Command::SetVisible { ids: vec![id] });
    call(
        &mut api,
        Command::Select {
            id,
            mode: katran::interaction::SelectionMode::Replace,
        },
    );
    image::RgbaImage::from_pixel(8, 6, image::Rgba([30u8, 90, 180, 255]))
        .save(&png)
        .unwrap();
    std::fs::write(e["path"].as_str().unwrap(), std::fs::read(png).unwrap()).unwrap();
    assert!(
        call(&mut api, Command::Actions)
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "use_as_background")
    );
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "stale".into(),
        command: Command::UseSelectedImageAsBackground,
    });
    assert_eq!(reply["error"]["code"], "source_changed");
    assert_eq!(
        call(&mut api, Command::Themes)["current"]["preset"],
        "blue_experimental"
    );
}
#[test]
fn optical_strength_is_optional_bounded_and_persistent() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(
        call(&mut api, Command::Themes)["current"]["optical_strength"],
        0
    );
    let theme = ThemePreferences {
        preset: Preset::BlueExperimental,
        optical_strength: 45,
        ..Default::default()
    };
    call(
        &mut api,
        Command::SetTheme {
            theme: theme.clone(),
        },
    );
    let invalid = ThemePreferences {
        optical_strength: 101,
        ..theme
    };
    let reply = api.execute(Request {
        api_version: 1,
        request_id: "optical-invalid".into(),
        command: Command::SetTheme { theme: invalid },
    });
    assert_eq!(reply["ok"], false);
    assert_eq!(
        call(&mut api, Command::Themes)["current"]["optical_strength"],
        45
    );
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(
        call(&mut api, Command::Themes)["current"]["optical_strength"],
        45
    );
}

#[test]
fn optical_components_default_validate_and_persist_independently() {
    let legacy: ThemePreferences =
        serde_json::from_value(serde_json::json!({"optical_strength":40})).unwrap();
    assert_eq!(
        (
            legacy.optical_refraction,
            legacy.optical_blur,
            legacy.optical_highlight,
            legacy.optical_chromatic
        ),
        (100, 100, 100, 100)
    );
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    let theme = ThemePreferences {
        optical_strength: 70,
        optical_refraction: 0,
        optical_blur: 57,
        optical_highlight: 125,
        optical_chromatic: 200,
        ..Default::default()
    };
    call(
        &mut api,
        Command::SetTheme {
            theme: theme.clone(),
        },
    );
    let original = call(&mut api, Command::Themes)["current"].clone();
    for bad in [
        ThemePreferences {
            optical_refraction: 201,
            ..theme.clone()
        },
        ThemePreferences {
            optical_blur: 201,
            ..theme.clone()
        },
        ThemePreferences {
            optical_highlight: 201,
            ..theme.clone()
        },
        ThemePreferences {
            optical_chromatic: 201,
            ..theme.clone()
        },
    ] {
        let reply = api.execute(Request {
            api_version: 1,
            request_id: "invalid-optics".into(),
            command: Command::SetTheme { theme: bad },
        });
        assert_eq!(reply["ok"], false, "{reply}");
        assert_eq!(call(&mut api, Command::Themes)["current"], original);
    }
    let mut off = theme.clone();
    off.optical_strength = 0;
    call(&mut api, Command::SetTheme { theme: off });
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    let off = call(&mut api, Command::Themes)["current"].clone();
    assert_eq!(off["optical_strength"], 0);
    assert_eq!(off["optical_refraction"], 0);
    assert_eq!(off["optical_blur"], 57);
    assert_eq!(off["optical_highlight"], 125);
    assert_eq!(off["optical_chromatic"], 200);
    call(
        &mut api,
        Command::SetTheme {
            theme: theme.clone(),
        },
    );
    drop(api);
    let mut api = Service::open(root.path(), 10000).unwrap();
    assert_eq!(call(&mut api, Command::Themes)["current"], original);
}
