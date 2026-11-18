use eframe::egui::{self, Color32, RichText, Stroke};
use katran::notion_reader::{Block, Cell};

pub(super) fn show(ui: &mut egui::Ui, blocks: &[Block]) {
    for (index, block) in blocks.iter().enumerate() {
        ui.push_id(index, |ui| match block {
            Block::Markdown(document) => {
                crate::markdown_ui::show_on(ui, document, Color32::from_gray(48));
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
        });
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
                                    let text = RichText::new(&cell.text);
                                    ui.add(
                                        egui::Label::new(if header { text.strong() } else { text })
                                            .wrap(),
                                    );
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
        let source = "<callout icon=\"💡\" color=\"blue_bg\">\nReader callout\n</callout>\n\n\
            | Table heading | Count |\n| --- | --- |\n| Visible value | 42 |\n\n\
            <toggle>\nOpen notes\n\nHidden child\n</toggle>";
        let blocks = katran::notion_reader::parse(source).unwrap();
        let context = egui::Context::default();
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
    }
}
