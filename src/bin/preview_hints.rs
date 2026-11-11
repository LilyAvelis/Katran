//! Footer hints for the active preview. Event types are supplied by the shelf;
//! Folder itself can contain several kinds of file.
use serde_json::Value;

pub(super) fn text(state: &Value, events: &[Value]) -> Option<&'static str> {
    let id = state["preview"].as_i64()?;
    let event = events
        .iter()
        .find(|event| event["id"].as_i64() == Some(id))?;
    let kind = event["kind"].as_str().unwrap_or("");
    if state["tab"] == "pods" && kind == "text_fragment" {
        return Some("Space — назад · ← → — листать");
    }
    if kind == "table_fragment" {
        return Some(
            "Space — назад · ← → — листать · колесо — масштаб · тащить — переместить · Ctrl+← — Таблица · Ctrl+→ — CSV",
        );
    }
    if kind == "link_fragment" {
        return Some("Space — назад · ← → — листать");
    }
    let markdown =
        katran::file_types::is_markdown(std::path::Path::new(event["name"].as_str().unwrap_or("")));
    if kind == "text_fragment" || markdown {
        return Some("Space — назад · ← → — листать · Ctrl+← — Markdown · Ctrl+→ — Исходный текст");
    }
    if state["preview_view"]["factor"]
        .as_f64()
        .is_some_and(|factor| factor > 1.0)
    {
        Some("ЛКМ — сдвиг · колесо — масштаб · Space — назад · ← → — листать")
    } else {
        Some("Тащить — экспорт · колесо — увеличить · Space — назад · ← → — листать")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn chooses_preview_hint_by_event_kind_and_zoom() {
        let events = vec![
            json!({"id":1,"kind":"image","name":"a.png"}),
            json!({"id":2,"kind":"text_fragment"}),
            json!({"id":3,"kind":"link_fragment"}),
            json!({"id":4,"kind":"table_fragment"}),
            json!({"id":-1,"kind":"file","name":"a.md"}),
        ];
        assert_eq!(text(&json!({"preview":null}), &events), None);
        assert_eq!(text(&json!({"preview":999}), &events), None);
        assert!(
            text(&json!({"preview":1,"preview_view":{"factor":1}}), &events)
                .unwrap()
                .contains("экспорт")
        );
        assert!(
            text(&json!({"preview":1,"preview_view":{"factor":2}}), &events)
                .unwrap()
                .contains("сдвиг")
        );
        assert!(
            text(&json!({"preview":2,"tab":"text"}), &events)
                .unwrap()
                .contains("Markdown")
        );
        assert!(
            !text(&json!({"preview":2,"tab":"pods"}), &events)
                .unwrap()
                .contains("Markdown")
        );
        assert!(
            !text(&json!({"preview":3}), &events)
                .unwrap()
                .contains("Markdown")
        );
        assert!(
            text(&json!({"preview":4}), &events)
                .unwrap()
                .contains("CSV")
        );
        assert!(
            text(&json!({"preview":-1,"tab":"folder"}), &events)
                .unwrap()
                .contains("Markdown")
        );
    }
}

/// Paint only in the free footer segment, leaving left/right controls untouched.
pub(super) fn paint(ui: &eframe::egui::Ui, region: eframe::egui::Rect, hint: &str) {
    use eframe::egui;
    if region.width() < 8.0 || !region.is_positive() {
        return;
    }
    let font = egui::FontId::proportional(11.0);
    let color = ui.visuals().weak_text_color();
    let mut job = egui::text::LayoutJob::simple_singleline(hint.to_owned(), font, color);
    job.wrap.max_width = region.width();
    job.wrap.max_rows = 1;
    job.wrap.overflow_character = Some('…');
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter_at(region)
        .galley(region.center() - galley.size() / 2.0, galley, color);
    ui.interact(
        region,
        egui::Id::new("preview-footer-hint"),
        egui::Sense::hover(),
    )
    .on_hover_text(hint);
}
#[cfg(test)]
mod paint_tests {
    use super::*;
    use eframe::egui;
    #[test]
    fn footer_text_is_centered_and_clipped_to_free_segment() {
        for width in [80.0, 260.0, 700.0] {
            let ctx = egui::Context::default();
            crate::ui_style::install(&ctx);
            let region =
                egui::Rect::from_min_size(egui::pos2(120.0, 10.0), egui::vec2(width, 30.0));
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 100.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    paint(
                        ui,
                        region,
                        "Space — назад · ← → — листать · колесо — масштаб · тащить — экспорт",
                    )
                },
            );
            output.textures_delta.clear();
            for clipped in output.shapes {
                if let egui::Shape::Text(text) = clipped.shape {
                    assert!(region.contains_rect(clipped.clip_rect));
                    let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                    assert!((bounds.center().x - region.center().x).abs() < 0.1);
                    assert!(
                        region
                            .expand(0.1)
                            .contains_rect(bounds.intersect(clipped.clip_rect))
                    );
                }
            }
        }
    }
}
