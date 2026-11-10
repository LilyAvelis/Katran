//! Safe Markdown display: paints parsed text only, never executes HTML or fetches assets.
use eframe::egui::{self, Color32, RichText};
use katran::markdown::{Document, InlineKind, Kind};

pub(super) fn show(ui: &mut egui::Ui, doc: &Document) {
    for block in &doc.blocks {
        match block.kind {
            Kind::CodeFence => {
                egui::Frame::new()
                    .fill(Color32::from_gray(48))
                    .corner_radius(5)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.label(RichText::new(&doc.source[block.content.clone()]).monospace());
                    });
            }
            _ => {
                if block.kind == Kind::ListItem {
                    ui.horizontal(|ui| {
                        ui.label("•");
                        inline(ui, doc, block);
                    });
                } else {
                    inline(ui, doc, block);
                }
            }
        }
    }
}
fn inline(ui: &mut egui::Ui, doc: &Document, block: &katran::markdown::Block) {
    let size = match block.kind {
        Kind::Heading(level) => 25.0 - f32::from(level) * 1.5,
        _ => 15.0,
    };
    let mut layout = egui::text::LayoutJob::default();
    layout.wrap.max_width = ui.available_width();
    for span in &block.inline {
        let (font, color, italics, underline) = match &span.kind {
            InlineKind::Strong => (
                egui::FontId::proportional(size + 1.0),
                Color32::from_gray(240),
                false,
                false,
            ),
            InlineKind::Emphasis => (
                egui::FontId::proportional(size),
                Color32::from_gray(225),
                true,
                false,
            ),
            InlineKind::Code => (
                egui::FontId::monospace(size),
                Color32::from_gray(225),
                false,
                false,
            ),
            InlineKind::Link { .. } => (
                egui::FontId::proportional(size),
                Color32::from_rgb(143, 197, 238),
                false,
                true,
            ),
            InlineKind::Text => (
                egui::FontId::proportional(size),
                Color32::from_gray(225),
                false,
                false,
            ),
        };
        layout.append(
            &doc.source[span.span.clone()],
            0.0,
            egui::TextFormat {
                font_id: font,
                color,
                italics,
                underline: if underline {
                    egui::Stroke::new(1.0, color)
                } else {
                    egui::Stroke::NONE
                },
                ..Default::default()
            },
        );
    }
    if block.inline.is_empty() {
        ui.add_space(size * 0.6);
    } else {
        ui.label(layout);
    }
}
