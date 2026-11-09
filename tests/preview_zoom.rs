use katran::{
    Command, Request, Service,
    preview_view::{Point, Size},
};
fn call(api: &mut Service, command: Command) -> serde_json::Value {
    api.execute(Request {
        api_version: 1,
        request_id: "zoom".into(),
        command,
    })
}
const SIZE: Size = Size {
    width: 400.0,
    height: 400.0,
};
#[test]
fn zoom_anchor_bounds_pan_and_reset_through_dispatcher() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100).unwrap();
    assert_eq!(
        call(
            &mut api,
            Command::PreviewGeometry {
                viewport: SIZE,
                fitted: SIZE
            }
        )["error"]["code"],
        "preview_closed"
    );
    call(&mut api, Command::SetVisible { ids: vec![1, 2] });
    call(&mut api, Command::OpenCurrent);
    let anchor = Point { x: 300.0, y: 250.0 };
    call(
        &mut api,
        Command::ZoomPreview {
            multiplier: 2.0,
            anchor,
            viewport: SIZE,
            fitted: SIZE,
        },
    );
    let geometry = call(
        &mut api,
        Command::PreviewGeometry {
            viewport: SIZE,
            fitted: SIZE,
        },
    );
    let g = &geometry["result"];
    assert_eq!(g["left"], -300.0);
    assert_eq!(g["top"], -250.0);
    // The same normalized image point stays under the pointer.
    assert_eq!(
        (300.0 - g["left"].as_f64().unwrap()) / g["width"].as_f64().unwrap(),
        0.75
    );
    let before = call(&mut api, Command::Interaction);
    assert_eq!(
        call(
            &mut api,
            Command::ZoomPreview {
                multiplier: f32::NAN,
                anchor,
                viewport: SIZE,
                fitted: SIZE
            }
        )["ok"],
        false
    );
    assert_eq!(call(&mut api, Command::Interaction), before);
    call(
        &mut api,
        Command::ZoomPreview {
            multiplier: 1e20,
            anchor: Point { x: 200.0, y: 200.0 },
            viewport: SIZE,
            fitted: SIZE,
        },
    );
    let state = call(
        &mut api,
        Command::PanPreview {
            delta: Point { x: 1e20, y: -1e20 },
            viewport: SIZE,
            fitted: SIZE,
        },
    );
    assert_eq!(state["result"]["preview_view"]["factor"], 16.0);
    assert_eq!(state["result"]["preview_view"]["offset"]["x"], 3000.0);
    assert_eq!(state["result"]["preview_view"]["offset"]["y"], -3000.0);
    let state = call(&mut api, Command::NavigatePreview { delta: 1 });
    assert_eq!(state["result"]["preview_view"]["factor"], 1.0);
    call(
        &mut api,
        Command::ZoomPreview {
            multiplier: 2.0,
            anchor,
            viewport: SIZE,
            fitted: SIZE,
        },
    );
    call(&mut api, Command::TogglePreview);
    let state = call(&mut api, Command::TogglePreview);
    assert_eq!(state["result"]["preview_view"]["factor"], 1.0);
    let state = call(
        &mut api,
        Command::ZoomPreview {
            multiplier: 0.001,
            anchor,
            viewport: SIZE,
            fitted: SIZE,
        },
    );
    assert_eq!(state["result"]["preview_view"]["factor"], 1.0);
}

#[test]
fn center_zoom_keeps_center_and_geometry_reconstrains_after_resize() {
    let root = tempfile::tempdir().unwrap();
    let mut api = Service::open(root.path(), 100).unwrap();
    call(&mut api, Command::SetVisible { ids: vec![1] });
    call(&mut api, Command::OpenCurrent);
    let center = Point { x: 200.0, y: 200.0 };
    let state = call(
        &mut api,
        Command::ZoomPreview {
            multiplier: 2.0,
            anchor: center,
            viewport: SIZE,
            fitted: SIZE,
        },
    );
    assert_eq!(state["result"]["preview_view"]["offset"]["x"], 0.0);
    assert_eq!(state["result"]["preview_view"]["offset"]["y"], 0.0);
    call(
        &mut api,
        Command::PanPreview {
            delta: Point { x: 150.0, y: 150.0 },
            viewport: SIZE,
            fitted: SIZE,
        },
    );
    let wide = Size {
        width: 1000.0,
        height: 400.0,
    };
    let g = call(
        &mut api,
        Command::PreviewGeometry {
            viewport: wide,
            fitted: SIZE,
        },
    );
    assert_eq!(g["result"]["offset"]["x"], 0.0);
    assert_eq!(g["result"]["offset"]["y"], 150.0);
    let invalid = Size {
        width: 0.0,
        height: 400.0,
    };
    assert_eq!(
        call(
            &mut api,
            Command::PanPreview {
                delta: center,
                viewport: invalid,
                fitted: SIZE
            }
        )["error"]["code"],
        "invalid_argument"
    );
    let state = call(&mut api, Command::SetVisible { ids: vec![] });
    assert!(state["result"]["preview"].is_null());
    assert_eq!(state["result"]["preview_view"]["factor"], 1.0);
}
