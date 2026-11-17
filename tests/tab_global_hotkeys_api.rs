use katran::{
    Command, Request, Service,
    interaction::ShelfTab,
    overlay::{Host, OverlayError, SummonReceipt, Target, WorkArea},
    settings::Settings,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};
fn invoke(api: &mut Service, command: Command) -> Value {
    api.execute(Request {
        api_version: 1,
        request_id: "tab-hotkeys".into(),
        command,
    })
}
fn call(api: &mut Service, command: Command) -> Value {
    let reply = invoke(api, command);
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}
fn assign(api: &mut Service, tab: ShelfTab, binding: Option<&str>) -> Value {
    invoke(
        api,
        Command::SetTabGlobalHotkey {
            tab,
            binding: binding.map(str::to_owned),
        },
    )
}
#[test]
fn bindings_are_canonical_scoped_persistent_and_conflicts_are_atomic() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    assert!(
        call(&mut api, Command::GetSettings)
            .get("tab_hotkeys")
            .is_none()
    );
    assert_eq!(
        assign(&mut api, ShelfTab::Text, Some("shift ctrl v"))["ok"],
        true
    );
    assert_eq!(
        call(&mut api, Command::GetSettings)["tab_hotkeys"]["text"],
        "Ctrl+Shift+V"
    );
    assert_eq!(
        assign(&mut api, ShelfTab::Downloads, Some("super+ctrl+5"))["ok"],
        true
    );
    assert_eq!(
        call(&mut api, Command::GetSettings)["tab_hotkeys"]["downloads"],
        "Win+Ctrl+5"
    );
    assert_eq!(
        assign(&mut api, ShelfTab::Desktop, Some("Win Space"))["ok"],
        true
    );
    assert_eq!(
        call(&mut api, Command::GetSettings)["tab_hotkeys"]["desktop"],
        "Win+Space"
    );
    let duplicate = assign(&mut api, ShelfTab::Images, Some("CTRL+V+SHIFT"));
    assert_eq!(duplicate["error"]["code"], "hotkey_conflict");
    assert_eq!(
        assign(&mut api, ShelfTab::Images, Some("F5"))["error"]["code"],
        "invalid_hotkey"
    );
    let mut settings: Settings =
        serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
    settings.overlay_hotkey = "Ctrl Shift V".into();
    assert_eq!(
        invoke(&mut api, Command::SetSettings { settings })["error"]["code"],
        "hotkey_conflict"
    );
    let mut settings: Settings =
        serde_json::from_value(call(&mut api, Command::GetSettings)).unwrap();
    settings
        .tab_hotkeys
        .insert(ShelfTab::Images, "shift+ctrl+v".into());
    assert_eq!(
        invoke(&mut api, Command::SetSettings { settings })["error"]["code"],
        "hotkey_conflict"
    );
    assert_eq!(
        call(&mut api, Command::GetSettings)["tab_hotkeys"]["text"],
        "Ctrl+Shift+V"
    );
    drop(api);
    let mut api = Service::open(root.path(), 100_000).unwrap();
    assert_eq!(
        call(&mut api, Command::GetSettings)["tab_hotkeys"]["text"],
        "Ctrl+Shift+V"
    );
    assert_eq!(assign(&mut api, ShelfTab::Text, None)["ok"], true);
    assign(&mut api, ShelfTab::Downloads, None);
    assign(&mut api, ShelfTab::Desktop, None);
    assert!(
        call(&mut api, Command::GetSettings)
            .get("tab_hotkeys")
            .is_none()
    );
}
struct Fake(Arc<Mutex<u64>>);
impl Host for Fake {
    fn foreground(&mut self) -> katran::overlay::Result<u64> {
        Ok(*self.0.lock().unwrap())
    }
    fn identity(&mut self, id: u64) -> katran::overlay::Result<Target> {
        match id {
            1 | 2 => Ok(Target {
                hwnd: id,
                pid: 1,
                process_started: 1,
            }),
            7 => Ok(Target {
                hwnd: id,
                pid: 7,
                process_started: 7,
            }),
            _ => Err(OverlayError::new("unavailable", "missing")),
        }
    }
    fn work_area(&mut self, _: u64) -> katran::overlay::Result<WorkArea> {
        Ok(WorkArea {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        })
    }
    fn wait_modifiers_released(&mut self) -> katran::overlay::Result<()> {
        Ok(())
    }
    fn activate(&mut self, _: u64) -> katran::overlay::Result<()> {
        Ok(())
    }
    fn send_paste(&mut self, _: Target, _: u32) -> katran::overlay::Result<()> {
        Ok(())
    }
}
#[test]
fn summon_is_show_only_checks_disabled_and_preserves_previous_capture() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let foreground = Arc::new(Mutex::new(7));
    api.set_overlay_host(Box::new(Fake(foreground.clone())));
    let capture = SummonReceipt {
        previous: Target {
            hwnd: 7,
            pid: 7,
            process_started: 7,
        },
        work_area: WorkArea {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        },
    };
    let disabled = invoke(
        &mut api,
        Command::SummonTabOverlay {
            tab: ShelfTab::Text,
            owner_window: 1,
            capture: Some(capture.clone()),
        },
    );
    assert_eq!(disabled["error"]["code"], "tab_disabled");
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    let shown = call(
        &mut api,
        Command::SummonTabOverlay {
            tab: ShelfTab::Text,
            owner_window: 1,
            capture: Some(capture),
        },
    );
    assert_eq!(shown["visible"], true);
    assert_eq!(shown["tab"], "text");
    *foreground.lock().unwrap() = 2;
    let shown_again = call(
        &mut api,
        Command::SummonTabOverlay {
            tab: ShelfTab::Images,
            owner_window: 1,
            capture: None,
        },
    );
    assert_eq!(shown_again["visible"], true);
    assert_eq!(shown_again["tab"], "images");
    assert_eq!(call(&mut api, Command::Interaction)["tab"], "images");
    // Validation reaches the retained previous target; later copy may fail for an empty selection.
    let paste = invoke(&mut api, Command::BeginPastePrevious { owner_window: 1 });
    assert_ne!(paste["error"]["code"], "no_previous_target", "{paste}");
    *foreground.lock().unwrap() = 7;
    let denied = invoke(
        &mut api,
        Command::SummonTabOverlay {
            tab: ShelfTab::Text,
            owner_window: 1,
            capture: None,
        },
    );
    assert_eq!(denied["error"]["code"], "foreground_changed");
    assert_eq!(call(&mut api, Command::Interaction)["tab"], "images");
}

#[test]
fn global_summon_restores_tab_view_and_closes_settings() {
    use katran::interaction::{SelectionMode, View};
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100_000).unwrap();
    let foreground = Arc::new(Mutex::new(2));
    api.set_overlay_host(Box::new(Fake(foreground)));
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: ShelfTab::Text,
            enabled: true,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Text,
        },
    );
    call(
        &mut api,
        Command::SetView {
            view: View::Details,
        },
    );
    call(
        &mut api,
        Command::SetTab {
            tab: ShelfTab::Images,
        },
    );
    call(
        &mut api,
        Command::SetView {
            view: View::Gallery,
        },
    );
    call(&mut api, Command::SetVisible { ids: vec![111] });
    call(
        &mut api,
        Command::Select {
            id: 111,
            mode: SelectionMode::Replace,
        },
    );
    call(&mut api, Command::ShowSettings);
    assert_eq!(call(&mut api, Command::Interaction)["settings_open"], true);
    let result = call(
        &mut api,
        Command::SummonTabOverlay {
            tab: ShelfTab::Text,
            owner_window: 1,
            capture: None,
        },
    );
    assert_eq!(result["visible"], true);
    let state = call(&mut api, Command::Interaction);
    assert_eq!(state["tab"], "text");
    assert_eq!(state["view"], "details");
    assert_eq!(state["settings_open"], false);
    assert_eq!(state["selected"], serde_json::json!([]));
    assert_eq!(state["cursor"], Value::Null);
    call(
        &mut api,
        Command::SummonTabOverlay {
            tab: ShelfTab::Images,
            owner_window: 1,
            capture: None,
        },
    );
    assert_eq!(call(&mut api, Command::Interaction)["view"], "gallery");
    assert_eq!(call(&mut api, Command::WindowVisibility)["visible"], true);
}
