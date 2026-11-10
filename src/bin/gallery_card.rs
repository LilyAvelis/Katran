//! Afio gallery_view/layout pattern: allocate once, fit/paint into fixed regions.
use eframe::egui::{self, Color32, Rect, Vec2};
pub const GAP: f32 = 12.0;
pub fn fit_rect(cell: Rect, natural: Vec2) -> Rect {
    let scale = (cell.width() / natural.x.max(1.0))
        .min(cell.height() / natural.y.max(1.0))
        .max(0.0);
    Rect::from_center_size(cell.center(), natural * scale)
}
#[derive(Clone, Copy)]
pub struct Geometry {
    pub image: Rect,
    pub name: Rect,
    pub origin: Rect,
}
pub fn geometry(card: Rect) -> Geometry {
    let inner = card.shrink(10.0);
    let image = Rect::from_min_size(
        inner.min,
        Vec2::new(inner.width(), (inner.height() - 54.0).max(1.0)),
    );
    let name = Rect::from_min_size(
        egui::pos2(inner.left(), image.bottom() + 8.0),
        Vec2::new(inner.width(), 22.0),
    );
    let origin = Rect::from_min_size(
        egui::pos2(inner.left(), name.bottom() + 4.0),
        Vec2::new(inner.width(), 20.0),
    );
    Geometry {
        image,
        name,
        origin,
    }
}
pub fn text(ui: &egui::Ui, rect: Rect, value: &str, size: f32, color: Color32) {
    let mut job = egui::text::LayoutJob::simple_singleline(
        value.to_owned(),
        egui::FontId::proportional(size),
        color,
    );
    job.wrap.max_width = rect.width();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    ui.painter_at(rect).galley(
        egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_titles_and_sources_are_clipped_at_multiple_display_scales() {
        for scale in [1.0, 1.5, 2.0] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(scale);
            let mut regions = Vec::new();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let card = Rect::from_min_size(egui::pos2(20.0, 20.0), Vec2::new(280.0, 334.0));
                let g = geometry(card);
                for rect in [g.name, g.origin] {
                    regions.push(rect);
                    text(
                        ui,
                        rect,
                        &"Очень длинное название Zen · example.com/".repeat(40),
                        14.0,
                        Color32::WHITE,
                    );
                }
            });
            let text_shapes: Vec<_> = output
                .shapes
                .iter()
                .filter(|s| matches!(s.shape, egui::Shape::Text(_)))
                .collect();
            assert_eq!(text_shapes.len(), 2);
            for (shape, rect) in text_shapes.into_iter().zip(regions) {
                assert!(rect.expand(0.01).contains_rect(shape.clip_rect));
                if let egui::Shape::Text(t) = &shape.shape {
                    assert!(t.galley.size().x <= rect.width() + 0.1);
                    assert!(t.galley.size().y <= rect.height() + 0.1);
                }
            }
            output.textures_delta.clear();
        }
    }
    #[test]
    fn cards_keep_image_name_origin_separate_at_all_scales() {
        for width in [280.0, 600.0, 940.0, 1600.0] {
            for count in [1usize, 2, 4, 8]
                .into_iter()
                .filter(|count| width / *count as f32 >= 96.0)
            {
                let w = (width - 16.0 - GAP * (count - 1) as f32) / count as f32;
                let h = w + 54.0;
                for column in 0..count {
                    let r = Rect::from_min_size(
                        egui::pos2(column as f32 * (w + GAP), 0.0),
                        Vec2::new(w, h),
                    );
                    let g = geometry(r);
                    for zone in [g.image, g.name, g.origin] {
                        assert!(r.contains_rect(zone));
                    }
                    assert!(g.image.bottom() < g.name.top());
                    assert!(g.name.bottom() < g.origin.top());
                    assert!(r.right() <= width);
                    for natural in [
                        Vec2::new(20.0, 1000.0),
                        Vec2::new(2000.0, 20.0),
                        Vec2::splat(400.0),
                    ] {
                        assert!(
                            g.image
                                .expand(0.01)
                                .contains_rect(fit_rect(g.image, natural))
                        );
                    }
                }
            }
        }
    }
}
