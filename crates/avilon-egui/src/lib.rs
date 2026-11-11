//! Host-styled action rows and shortcut keycaps. No actions are executed here.
pub mod optics;

use egui::{self, Color32};

/// Host-supplied fonts and colors for keycaps.
#[derive(Clone, Debug)]
pub struct KeyCapsStyle {
    pub font: egui::FontId,
    pub text_color: Color32,
    pub fill: Color32,
    pub stroke: egui::Stroke,
    pub corner_radius: u8,
}

/// Host-supplied label, selection and keycap appearance.
#[derive(Clone, Debug)]
pub struct ActionStyle {
    pub label_font: egui::FontId,
    pub label_color: Color32,
    pub selection_fill: Color32,
    pub keycaps: KeyCapsStyle,
}

pub struct KeyCaps {
    keys: Vec<std::sync::Arc<egui::Galley>>,
    pub size: egui::Vec2,
}
impl KeyCaps {
    const PAD: egui::Vec2 = egui::vec2(5.0, 2.0);
    const GAP: f32 = 3.0;
    pub fn measure(ui: &egui::Ui, text: &str, style: &KeyCapsStyle) -> Self {
        let keys: Vec<_> = text
            .split_whitespace()
            .map(|key| {
                ui.painter()
                    .layout_no_wrap(key.to_owned(), style.font.clone(), style.text_color)
            })
            .collect();
        let width = keys
            .iter()
            .map(|g| g.size().x + 2.0 * Self::PAD.x)
            .sum::<f32>()
            + keys.len().saturating_sub(1) as f32 * Self::GAP;
        let height = keys
            .iter()
            .map(|g| g.size().y + 2.0 * Self::PAD.y)
            .fold(0.0, f32::max);
        Self {
            keys,
            size: egui::vec2(width, height),
        }
    }
    pub fn measure_fitting(
        ui: &egui::Ui,
        text: &str,
        max_width: f32,
        style: &KeyCapsStyle,
    ) -> Self {
        let measured = Self::measure(ui, text, style);
        if measured.size.x <= max_width {
            return measured;
        }
        let mut job = egui::text::LayoutJob::simple(
            text.to_owned(),
            style.font.clone(),
            style.text_color,
            (max_width - 2.0 * Self::PAD.x).max(0.0),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let size = galley.size() + 2.0 * Self::PAD;
        Self {
            keys: vec![galley],
            size,
        }
    }
    pub fn paint(&self, ui: &egui::Ui, region: egui::Rect, style: &KeyCapsStyle) {
        let painter = ui
            .painter()
            .with_clip_rect(ui.clip_rect().intersect(region));
        let mut x = region.right() - self.size.x;
        for galley in &self.keys {
            let size = galley.size() + 2.0 * Self::PAD;
            let rect =
                egui::Rect::from_min_size(egui::pos2(x, region.center().y - size.y / 2.0), size);
            painter.rect(
                rect,
                style.corner_radius,
                style.fill,
                style.stroke,
                egui::StrokeKind::Inside,
            );
            painter.galley(
                rect.center() - galley.size() / 2.0,
                galley.clone(),
                style.text_color,
            );
            x += size.x + Self::GAP;
        }
    }
}

pub struct ActionPresentation<'a> {
    pub label: &'a str,
    pub icon: egui_lucide::Lucide,
    pub primary_shortcut: &'a str,
    pub full_shortcut_tooltip: &'a str,
}
pub struct ActionRow<'a>(ActionPresentation<'a>, Option<egui::TextureId>, ActionStyle);
pub fn primary_shortcut(shortcut: &str) -> &str {
    shortcut.split('/').next().unwrap_or("").trim()
}

pub struct RowRegions {
    pub icon: egui::Rect,
    pub label: egui::Rect,
    pub keys: egui::Rect,
}
impl<'a> ActionRow<'a> {
    pub fn new(presentation: ActionPresentation<'a>, style: ActionStyle) -> Self {
        Self(presentation, None, style)
    }
    pub fn with_texture(mut self, texture: Option<egui::TextureId>) -> Self {
        self.1 = texture;
        self
    }
    pub fn shortcut_budget(row_width: f32) -> f32 {
        ((row_width - 42.0).max(0.0) * 0.45).max(10.0)
    }
    pub fn regions(row: egui::Rect, key_width: f32) -> RowRegions {
        let left = (row.left() + 33.0).min(row.right());
        let right = (row.right() - 9.0).max(left);
        let available = right - left;
        // Ordinary bindings retain their intrinsic width. Oversized user bindings
        // are ellipsized inside one bounded keycap; full text is a tooltip.
        let width = key_width.min(available);
        RowRegions {
            icon: egui::Rect::from_center_size(
                egui::pos2(row.left() + 17.0, row.center().y),
                egui::vec2(16.0, 16.0),
            ),
            label: egui::Rect::from_min_max(
                egui::pos2(left, row.top()),
                egui::pos2((right - width - 6.0).max(left), row.bottom()),
            ),
            keys: egui::Rect::from_min_max(
                egui::pos2(right - width, row.top()),
                egui::pos2(right, row.bottom()),
            ),
        }
    }
    pub fn show(self, ui: &mut egui::Ui, selected: bool) -> egui::Response {
        let caps = KeyCaps::measure_fitting(
            ui,
            self.0.primary_shortcut,
            Self::shortcut_budget(ui.available_width()),
            &self.2.keycaps,
        );
        let font = self.2.label_font.clone();
        let height = 38.0_f32
            .max(caps.size.y + 6.0)
            .max(ui.fonts_mut(|f| f.row_height(&font)) + 6.0);
        let (row, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::click(),
        );
        let regions = Self::regions(row, caps.size.x);
        if selected || response.hovered() {
            ui.painter().rect_filled(row, 7.0, self.2.selection_fill);
        }
        if let Some(texture) = self.1 {
            egui::Image::new((texture, egui::vec2(16.0, 16.0))).paint_at(ui, regions.icon);
        } else {
            self.0.icon.size(16.0).image().paint_at(ui, regions.icon);
        }
        // Single-line truncation uses egui's actual font layout and same vertical anchor.
        let mut job = egui::text::LayoutJob::simple(
            self.0.label.to_owned(),
            self.2.label_font.clone(),
            self.2.label_color,
            regions.label.width(),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter()
            .with_clip_rect(ui.clip_rect().intersect(regions.label))
            .galley(
                egui::pos2(regions.label.left(), row.center().y - galley.size().y / 2.0),
                galley,
                self.2.label_color,
            );
        caps.paint(ui, regions.keys, &self.2.keycaps);
        response.on_hover_text(self.0.full_shortcut_tooltip)
    }
}
