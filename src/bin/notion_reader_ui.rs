use eframe::egui::{self, Color32, RichText, Stroke};
use katran::{
    notion_reader::{Block, Cell, ReferenceKind},
    notion_rich::Document,
};

pub(super) fn show(ui: &mut egui::Ui, blocks: &[Block]) {
    for (index, block) in blocks.iter().enumerate() {
        ui.push_id(index, |ui| match block {
            Block::Markdown(document) => {
                show_document(ui, document, false);
            }
            Block::Callout {
                icon,
                color,
                children,
            } => {
                let border = callout_color(color.as_deref());
                let fill = Color32::from_gray(32).lerp_to_gamma(border, 0.19);
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, border))
                    .corner_radius(6)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_top(|ui| {
                            if let Some(icon) = icon
                                .as_deref()
                                .filter(|icon| icon.chars().count() <= 12 && !icon.contains("://"))
                            {
                                ui.label(RichText::new(icon).size(16.0));
                            }
                            ui.vertical(|ui| show(ui, children));
                        });
                    });
            }
            Block::Toggle { title, children } => {
                let title = if title.trim().is_empty() {
                    super::tr("Раскрыть блок", "Expand block")
                } else {
                    title
                };
                egui::CollapsingHeader::new(title)
                    .id_salt("notion-toggle")
                    .show(ui, |ui| show(ui, children));
            }
            Block::Table { rows } => show_table(ui, rows),
            Block::Reference { kind, title, url } => {
                let label = match kind {
                    ReferenceKind::Database => super::tr("База Notion", "Notion database"),
                    ReferenceKind::Page => super::tr("Страница Notion", "Notion page"),
                };
                egui::Frame::new()
                    .fill(Color32::from_gray(45))
                    .corner_radius(6)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.weak(label);
                            let title = if title.is_empty() { label } else { title };
                            if let Some(url) = url {
                                ui.hyperlink_to(RichText::new(title).strong(), url);
                            } else {
                                ui.label(RichText::new(title).strong());
                            }
                        });
                    });
            }
        });
    }
}

fn show_document(ui: &mut egui::Ui, document: &Document, header: bool) {
    for line in &document.lines {
        if line.rule {
            ui.separator();
            continue;
        }
        ui.horizontal_top(|ui| {
            let indent = (line.indent.min(12) * 12) as f32;
            if indent > 0.0 {
                ui.add_space(indent);
            }
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = (ui.available_width() - 8.0).max(20.0);
            for run in &line.runs {
                let style = &run.style;
                let size = match line.heading {
                    0 => 15.0,
                    level => 26.0 - f32::from(level) * 1.6,
                };
                let color = style.color.as_deref().map_or_else(
                    || {
                        if style.link {
                            Color32::from_rgb(153, 198, 240)
                        } else if header || style.bold {
                            Color32::WHITE
                        } else {
                            Color32::from_gray(228)
                        }
                    },
                    callout_color_for_text,
                );
                let underline = if style.underline || style.link {
                    Stroke::new(1.0, color)
                } else {
                    Stroke::NONE
                };
                job.append(
                    &run.text,
                    0.0,
                    egui::TextFormat {
                        font_id: if style.code {
                            egui::FontId::monospace(size)
                        } else if style.bold || header || line.heading != 0 {
                            egui::FontId::new(
                                size,
                                egui::FontFamily::Name(crate::ui_fonts::BOLD_FAMILY.into()),
                            )
                        } else {
                            egui::FontId::proportional(size)
                        },
                        color,
                        background: style
                            .color
                            .as_deref()
                            .filter(|color| color.ends_with("_bg"))
                            .map_or(Color32::TRANSPARENT, |color| {
                                callout_color(Some(color)).gamma_multiply(0.32)
                            }),
                        italics: style.italic,
                        underline,
                        strikethrough: if style.strike {
                            Stroke::new(1.0, color)
                        } else {
                            Stroke::NONE
                        },
                        ..Default::default()
                    },
                );
            }
            if line.quote || line.code {
                egui::Frame::new()
                    .fill(Color32::from_gray(if line.code { 48 } else { 40 }))
                    .inner_margin(5)
                    .corner_radius(4)
                    .show(ui, |ui| {
                        ui.label(job);
                    });
            } else {
                ui.label(job);
            }
        });
    }
}

fn callout_color_for_text(color: &str) -> Color32 {
    if color.ends_with("_bg") {
        Color32::from_gray(240)
    } else {
        callout_color(Some(color))
    }
}

fn callout_color(color: Option<&str>) -> Color32 {
    let color = color.unwrap_or("gray").trim_end_matches("_bg");
    let (r, g, b) = match color {
        "brown" => (173, 125, 99),
        "orange" => (213, 145, 93),
        "yellow" => (211, 178, 90),
        "green" => (109, 166, 124),
        "blue" => (105, 159, 205),
        "purple" => (156, 132, 204),
        "pink" => (201, 132, 166),
        "red" => (209, 121, 117),
        _ => (139, 140, 145),
    };
    Color32::from_rgb(r, g, b)
}

fn show_table(ui: &mut egui::Ui, rows: &[Vec<Cell>]) {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return;
    }
    let cell_width = (ui.available_width() / columns as f32 - 10.0).clamp(128.0, 250.0);
    egui::ScrollArea::horizontal()
        .id_salt("notion-table")
        .show(ui, |ui| {
            egui::Grid::new("cells")
                .striped(true)
                .spacing(egui::vec2(1.0, 1.0))
                .show(ui, |ui| {
                    for row in rows {
                        for cell in row {
                            let header = cell.header;
                            let fill = Color32::from_gray(if header { 61 } else { 36 });
                            egui::Frame::new()
                                .fill(fill)
                                .inner_margin(5)
                                .show(ui, |ui| {
                                    ui.set_width(cell_width);
                                    show_document(ui, &cell.document, header);
                                });
                        }
                        ui.end_row();
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notion_reader_paints_callout_table_and_collapsed_toggle() {
        let source = "This is <b>Bold text</b> and <i>Italic text</i>.\n\n<p />\n\n\
            <database url=\"https://www.notion.so/example\" data-source-url=\"collection://example\">\nFixture database\n</database>\n\n\
            <callout icon=\"💡\" color=\"blue_bg\">\nReader callout\n</callout>\n\n\
            | Table heading | Count |\n| --- | --- |\n| Visible value | 42 |\n\n\
            <toggle>\nOpen notes\n\nHidden child\n</toggle>";
        let blocks = katran::notion_reader::parse(source).unwrap();
        let context = egui::Context::default();
        crate::ui_fonts::install(&context);
        let mut output = egui::FullOutput::default();
        for _ in 0..2 {
            output = context.run_ui(egui::RawInput::default(), |ui| show(ui, &blocks));
            output.textures_delta.clear();
        }
        fn rendered(shape: &egui::Shape, expected: &str) -> bool {
            match shape {
                egui::Shape::Text(text) => text.galley.job.text.contains(expected),
                egui::Shape::Vec(shapes) => shapes.iter().any(|shape| rendered(shape, expected)),
                _ => false,
            }
        }
        for expected in [
            "Reader callout",
            "Table heading",
            "Visible value",
            "Open notes",
            "Bold text",
            "Italic text",
            "Fixture database",
        ] {
            assert!(
                output.shapes.iter().any(|s| rendered(&s.shape, expected)),
                "{expected}"
            );
        }
        assert!(
            !output
                .shapes
                .iter()
                .any(|s| rendered(&s.shape, "Hidden child"))
        );
        for hidden in ["<b>", "<i>", "<p />", "<database", "data-source-url"] {
            assert!(
                !output.shapes.iter().any(|s| rendered(&s.shape, hidden)),
                "{hidden}"
            );
        }
    }
}
