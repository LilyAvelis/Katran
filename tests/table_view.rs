use katran::{
    preview_view::{Point, Size},
    table_view::State,
    *,
};
use serde_json::{Value, json};

fn call(api: &mut Service, command: Command) -> Value {
    let r = api.execute(Request {
        api_version: 1,
        request_id: "table-view".into(),
        command,
    });
    assert_eq!(r["ok"], true, "{r}");
    r["result"].clone()
}
#[test]
fn pointer_anchor_pan_bounds_fit_and_invalid_inputs() {
    let viewport = Size {
        width: 500.,
        height: 400.,
    };
    let content = Size {
        width: 2000.,
        height: 1000.,
    };
    let mut state = State::default();
    state
        .zoom(2., Point { x: 100., y: 80. }, viewport, content)
        .unwrap();
    assert_eq!(state.factor, 2.);
    assert!(((state.scroll.x + 100.) / state.factor - 100.).abs() < 0.001);
    assert!(((state.scroll.y + 80.) / state.factor - 80.).abs() < 0.001);
    state
        .pan(
            Point {
                x: -10000.,
                y: -10000.,
            },
            viewport,
            content,
        )
        .unwrap();
    assert_eq!(state.scroll.x, 3500.);
    assert_eq!(state.scroll.y, 1600.);
    let before = serde_json::to_value(state).unwrap();
    assert!(
        state
            .zoom(f32::NAN, Point::default(), viewport, content)
            .is_err()
    );
    assert!(
        state
            .pan(
                Point {
                    x: f32::INFINITY,
                    y: 0.
                },
                viewport,
                content
            )
            .is_err()
    );
    assert_eq!(serde_json::to_value(state).unwrap(), before);
    state.fit(viewport, content).unwrap();
    assert_eq!(state.factor, 0.25);
    assert_eq!(state.scroll.x, 0.);
    assert_eq!(state.geometry(viewport, content).unwrap()["width"], 500.);
    state
        .zoom(0.00001, Point::default(), viewport, content)
        .unwrap();
    assert_eq!(state.factor, katran::table_view::MIN_FACTOR);
    assert_eq!(state.scroll.y, 0.);
}
#[test]
fn dispatcher_table_zoom_keeps_selection_and_resets_between_items() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 10000).unwrap();
    call(
        &mut api,
        Command::SetTabEnabled {
            tab: interaction::ShelfTab::Tables,
            enabled: true,
        },
    );
    let mut ids = vec![];
    for s in ["first", "second"] {
        let event = call(
            &mut api,
            Command::IngestTable {
                rows: vec![vec![s.into()]],
                source: "test".into(),
            },
        );
        ids.push(event["id"].as_i64().unwrap());
    }
    call(&mut api, Command::SetVisible { ids: ids.clone() });
    call(
        &mut api,
        Command::Select {
            id: ids[0],
            mode: interaction::SelectionMode::Replace,
        },
    );
    let before = call(&mut api, Command::TogglePreview);
    let viewport = Size {
        width: 500.,
        height: 400.,
    };
    let content = Size {
        width: 2000.,
        height: 1000.,
    };
    let zoom = call(
        &mut api,
        Command::ZoomTableView {
            multiplier: 2.,
            anchor: Point { x: 100., y: 80. },
            viewport,
            content,
        },
    );
    assert_eq!(zoom["table_view"]["factor"], 2.);
    assert_eq!(zoom["selected"], before["selected"]);
    assert_eq!(zoom["preview"], ids[0]);
    assert_eq!(
        call(&mut api, Command::TableViewGeometry { viewport, content })["scroll"],
        json!({"x":100.,"y":80.})
    );
    let next = call(&mut api, Command::NavigatePreview { delta: 1 });
    assert_eq!(next["preview"], ids[1]);
    assert_eq!(next["table_view"]["factor"], 1.);
    call(&mut api, Command::FitTableView { viewport, content });
    assert_eq!(
        call(&mut api, Command::ResetTableView)["table_view"]["factor"],
        1.
    );
    call(&mut api, Command::ClosePreview);
    let r = api.execute(Request {
        api_version: 1,
        request_id: "closed".into(),
        command: Command::TableViewGeometry { viewport, content },
    });
    assert_eq!(r["error"]["code"], "preview_closed");
}
