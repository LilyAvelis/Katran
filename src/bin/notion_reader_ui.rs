use crate::pods_ui::State as EmojiCache;
use eframe::egui::{self, Color32, RichText, Stroke};
use katran::{
    notion_reader::{Block, Cell, ReferenceKind},
    notion_rich::Document,
    settings::NotionReaderPreferences,
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub(crate) struct ReaderCopy;

impl egui::Plugin for ReaderCopy {
    fn debug_name(&self) -> &'static str {
        "Notion reader copy"
    }

    fn output_hook(&mut self, ctx: &egui::Context, output: &mut egui::FullOutput) {
        let armed = egui::Id::new(("notion-reader-copy-armed", ctx.viewport_id()));
        let Some(save_in_shelf) = ctx.data_mut(|data| data.remove_temp::<bool>(armed)) else {
            return;
        };
        let mut copied = None;
        output.platform_output.commands.retain_mut(|command| {
            if let egui::OutputCommand::CopyText(text) = command {
                let decoded = text.chars().fold(String::new(), |mut decoded, ch| {
                    if let Some(emoji) = katran::emoji_render::emoji_from_placeholder(ch) {
                        decoded.push_str(emoji);
                    } else {
                        decoded.push(ch);
                    }
                    decoded
                });
                if save_in_shelf {
                    copied = Some(decoded);
                    false
                } else {
                    *text = decoded;
                    true
                }
            } else {
                true
            }
        });
        if let Some(text) = copied {
            ctx.data_mut(|data| data.insert_temp(egui::Id::new("notion-reader-copied"), text));
            ctx.request_repaint();
        }
    }
}

pub(crate) fn arm_copy(ctx: &egui::Context) {
    arm_selection_copy(ctx, true);
}

pub(crate) fn arm_selection_copy(ctx: &egui::Context, save_in_shelf: bool) {
    if ctx.input(|input| {
        input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Copy))
    }) {
        let _ = ctx.plugin_or_default::<ReaderCopy>();
        let armed = egui::Id::new(("notion-reader-copy-armed", ctx.viewport_id()));
        ctx.data_mut(|data| data.insert_temp(armed, save_in_shelf));
    }
}

pub(crate) fn reader_column(
    ui: &mut egui::Ui,
    preferences: NotionReaderPreferences,
    render: impl FnOnce(&mut egui::Ui, f32),
) {
    let area = ui.available_rect_before_wrap();
    let width = if preferences.landscape {
        area.width()
    } else {
        area.width().min(660.0)
    };
    let size = egui::vec2(width, area.height());
    paint_reader_background(
        ui,
        egui::Rect::from_center_size(area.center(), size),
        preferences,
    );
    ui.vertical_centered(|ui| {
        ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(width);
            ui.set_max_width(width);
            render(ui, f32::from(preferences.text_percent) / 100.0);
        });
    });
}

fn paint_reader_background(ui: &egui::Ui, column: egui::Rect, prefs: NotionReaderPreferences) {
    use egui::{Pos2, Rect};
    let outer = ui.ctx().content_rect();
    let surround = if prefs.cinema {
        Color32::from_black_alpha(155)
    } else {
        Color32::TRANSPARENT
    };
    if prefs.cinema {
        let painter = ui.painter().with_clip_rect(outer);
        for rect in [
            Rect::from_min_max(outer.min, Pos2::new(outer.right(), column.top())),
            Rect::from_min_max(Pos2::new(outer.left(), column.bottom()), outer.max),
            Rect::from_min_max(Pos2::new(outer.left(), column.top()), column.left_bottom()),
            Rect::from_min_max(
                column.right_top(),
                Pos2::new(outer.right(), column.bottom()),
            ),
        ] {
            painter.rect_filled(rect, 0.0, surround);
        }
    }

    let [r, g, b] = prefs.color;
    let ink = Color32::from_rgba_unmultiplied(r, g, b, prefs.opacity);
    let halo = column.expand2(egui::vec2(42.0, 10.0)).intersect(outer);
    let xs = [halo.left(), column.left(), column.right(), halo.right()];
    let ys = [
        column.top(),
        (column.top() + 10.0).min(column.center().y),
        (column.bottom() - 10.0).max(column.center().y),
        column.bottom(),
    ];
    let mut mesh = egui::Mesh::default();
    for (y, top) in ys.into_iter().enumerate() {
        for (x, left) in xs.into_iter().enumerate() {
            let color = if (x == 1 || x == 2) && (y == 1 || y == 2) {
                ink
            } else if prefs.cinema {
                if x == 0 || x == 3 {
                    Color32::TRANSPARENT
                } else {
                    surround
                }
            } else {
                Color32::TRANSPARENT
            };
            mesh.colored_vertex(Pos2::new(left, top), color);
        }
    }
    for y in 0..3 {
        for x in 0..3 {
            let i = y * 4 + x;
            mesh.add_triangle(i, i + 1, i + 4);
            mesh.add_triangle(i + 1, i + 5, i + 4);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
pub(super) fn show(ui: &mut egui::Ui, blocks: &[Block]) {
    show_scaled(ui, blocks, 1.0, &mut EmojiCache::default());
}

pub(super) fn show_scaled(ui: &mut egui::Ui, blocks: &[Block], scale: f32, emoji: &mut EmojiCache) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let body_size = egui::TextStyle::Body.resolve(ui.style()).size * scale;
    for (index, block) in blocks.iter().enumerate() {
        ui.push_id(index, |ui| match block {
            Block::Markdown(document) => {
                show_document(ui, document, false, scale, emoji);
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
                                label(ui, emoji, icon, egui::FontId::proportional(16.0 * scale));
                            }
                            ui.vertical(|ui| show_scaled(ui, children, scale, emoji));
                        });
                    });
            }
            Block::Toggle { title, children } => {
                let title = if title.trim().is_empty() {
                    super::tr("Раскрыть блок", "Expand block")
                } else {
                    title
                };
                if title
                    .graphemes(true)
                    .any(katran::emoji_render::is_emoji_sequence)
                {
                    egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        ui.make_persistent_id("notion-toggle"),
                        false,
                    )
                    .show_header(ui, |ui| {
                        label(ui, emoji, title, egui::FontId::proportional(body_size));
                    })
                    .body(|ui| show_scaled(ui, children, scale, emoji));
                } else {
                    egui::CollapsingHeader::new(RichText::new(title).size(body_size))
                        .id_salt("notion-toggle")
                        .show(ui, |ui| show_scaled(ui, children, scale, emoji));
                }
            }
            Block::Table { rows } => show_table(ui, rows, scale, emoji),
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
                            ui.label(RichText::new(label).size(body_size).weak());
                            let title = if title.is_empty() { label } else { title };
                            if title
                                .graphemes(true)
                                .any(katran::emoji_render::is_emoji_sequence)
                            {
                                let mut line = InlineLine::default();
                                let mut format = egui::TextFormat::simple(
                                    egui::FontId::new(
                                        body_size,
                                        egui::FontFamily::Name(crate::ui_fonts::BOLD_FAMILY.into()),
                                    ),
                                    ui.visuals().text_color(),
                                );
                                if url.is_some() {
                                    format.color = ui.visuals().hyperlink_color;
                                    format.underline = Stroke::new(1.0, format.color);
                                }
                                line.append(title, format);
                                let response = line.show_with_sense(
                                    ui,
                                    emoji,
                                    if url.is_some() {
                                        egui::Sense::click()
                                    } else {
                                        egui::Sense::hover()
                                    },
                                );
                                if let Some(url) = url
                                    && response
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text(url)
                                        .clicked()
                                {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                                }
                            } else if let Some(url) = url {
                                ui.hyperlink_to(RichText::new(title).size(body_size).strong(), url);
                            } else {
                                ui.label(RichText::new(title).size(body_size).strong());
                            }
                        });
                    });
            }
        });
    }
}

pub(crate) fn label(ui: &mut egui::Ui, emoji: &mut EmojiCache, text: &str, font_id: egui::FontId) {
    let mut line = InlineLine::default();
    line.append(
        text,
        egui::TextFormat::simple(font_id, ui.visuals().text_color()),
    );
    line.show(ui, emoji);
}

#[derive(Default)]
struct InlineLine<'a> {
    job: egui::text::LayoutJob,
    emojis: Vec<(usize, &'a str, f32)>,
    chars: usize,
}

impl<'a> InlineLine<'a> {
    fn append(&mut self, text: &'a str, format: egui::TextFormat) {
        if text.is_ascii() {
            self.job.append(text, 0.0, format);
            self.chars += text.len();
            return;
        }
        let mut start = 0;
        for (offset, cluster) in text.grapheme_indices(true) {
            let Some(placeholder) = katran::emoji_render::emoji_placeholder(cluster) else {
                continue;
            };
            let plain = &text[start..offset];
            self.job.append(plain, 0.0, format.clone());
            self.chars += plain.chars().count();
            self.emojis.push((self.chars, cluster, format.font_id.size));
            self.job.append(
                &placeholder.to_string(),
                0.0,
                egui::TextFormat {
                    color: Color32::TRANSPARENT,
                    ..format.clone()
                },
            );
            self.chars += 1;
            start = offset + cluster.len();
        }
        let tail = &text[start..];
        self.job.append(tail, 0.0, format);
        self.chars += tail.chars().count();
    }

    fn show(self, ui: &mut egui::Ui, emoji: &mut EmojiCache) {
        self.show_with_sense(ui, emoji, egui::Sense::hover());
    }

    fn show_with_sense(
        self,
        ui: &mut egui::Ui,
        emoji: &mut EmojiCache,
        sense: egui::Sense,
    ) -> egui::Response {
        let label = egui::Label::new(self.job).wrap().sense(sense);
        if self.emojis.is_empty() {
            return ui.add(label);
        }
        let (pos, galley, response) = label.selectable(true).layout_in_ui(ui);
        egui::text_selection::LabelSelectionState::label_text_selection(
            ui,
            &response,
            pos,
            galley.clone(),
            ui.visuals().text_color(),
            Stroke::NONE,
        );
        visit_emoji_slots(&galley, pos, self.emojis, |cluster, rect, size| {
            emoji.paint_inline(ui, cluster, rect, size);
        });
        response
    }
}

fn visit_emoji_slots(
    galley: &egui::Galley,
    pos: egui::Pos2,
    emojis: Vec<(usize, &str, f32)>,
    mut visit: impl FnMut(&str, egui::Rect, f32),
) {
    let mut index = 0;
    let mut emojis = emojis.into_iter().peekable();
    for row in &galley.rows {
        for glyph in &row.glyphs {
            if emojis.peek().is_some_and(|(offset, _, _)| *offset == index) {
                let (_, cluster, size) = emojis.next().expect("matched inline emoji");
                let rect = glyph
                    .logical_rect()
                    .translate(pos.to_vec2() + row.pos.to_vec2());
                visit(cluster, rect, size);
            }
            index += 1;
        }
        index += usize::from(row.ends_with_newline);
    }
}

fn show_document(
    ui: &mut egui::Ui,
    document: &Document,
    header: bool,
    scale: f32,
    emoji: &mut EmojiCache,
) {
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
            let mut line_job = InlineLine::default();
            for run in &line.runs {
                let style = &run.style;
                let size = match line.heading {
                    0 => 15.0,
                    level => 26.0 - f32::from(level) * 1.6,
                } * scale;
                let inline_code = style.code && !line.code;
                let color = if inline_code {
                    Color32::from_rgb(247, 125, 116)
                } else {
                    style.color.as_deref().map_or_else(
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
                    )
                };
                let underline = if style.underline || style.link {
                    Stroke::new(1.0, color)
                } else {
                    Stroke::NONE
                };
                line_job.append(
                    &run.text,
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
                        background: if inline_code {
                            Color32::from_gray(44)
                        } else {
                            style
                                .color
                                .as_deref()
                                .filter(|color| color.ends_with("_bg"))
                                .map_or(Color32::TRANSPARENT, |color| {
                                    callout_color(Some(color)).gamma_multiply(0.32)
                                })
                        },
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
                        line_job.show(ui, emoji);
                    });
            } else {
                line_job.show(ui, emoji);
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

fn show_table(ui: &mut egui::Ui, rows: &[Vec<Cell>], scale: f32, emoji: &mut EmojiCache) {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return;
    }
    let viewport_width = ui.available_width().max(1.0);
    let cell_width = (viewport_width / columns as f32 - 10.0).clamp(128.0, 250.0);
    // Clip to the reader column before the grid asks for its full width.
    // The horizontal scroll area owns the overflow, not the surrounding page.
    let clip = ui.clip_rect().intersect(egui::Rect::from_min_max(
        ui.cursor().min,
        egui::pos2(ui.cursor().min.x + viewport_width, ui.clip_rect().max.y),
    ));
    ui.scope(|ui| {
        ui.set_max_width(viewport_width);
        ui.set_clip_rect(clip);
        egui::ScrollArea::horizontal()
            .id_salt("notion-table")
            .max_width(viewport_width)
            .auto_shrink([false, true])
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
                                        show_document(ui, &cell.document, header, scale, emoji);
                                    });
                            }
                            ui.end_row();
                        }
                    });
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_copy_expands_emoji_and_owns_only_armed_output() {
        let ctx = egui::Context::default();
        let marker = katran::emoji_render::emoji_placeholder("👩🏽‍💻").unwrap();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Copy],
                ..Default::default()
            },
            |ui| {
                arm_copy(ui.ctx());
                ui.copy_text(format!("code {marker}"));
            },
        );
        output.textures_delta.clear();
        assert!(
            output
                .platform_output
                .commands
                .iter()
                .all(|command| !matches!(command, egui::OutputCommand::CopyText(_)))
        );
        assert_eq!(
            ctx.data_mut(|data| data.remove_temp::<String>(egui::Id::new("notion-reader-copied")))
                .as_deref(),
            Some("code 👩🏽‍💻")
        );
    }

    #[test]
    fn wrapped_emoji_slots_keep_whole_sequences_and_text_punctuation() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut line = InlineLine::default();
            let format = egui::TextFormat::simple(egui::FontId::proportional(16.0), Color32::WHITE);
            line.append("© 2026 🐄\n", format.clone());
            line.append("Longer second line with a flag 🇰🇿 and 👩🏽‍💻", format);
            assert_eq!(
                line.job
                    .text
                    .chars()
                    .filter(|ch| katran::emoji_render::emoji_from_placeholder(*ch).is_some())
                    .count(),
                3
            );
            assert!(line.job.text.starts_with("© 2026"));
            line.job.wrap.max_width = 108.0;
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(line.job));
            assert!(galley.rows.len() > 2);
            let mut clusters = Vec::new();
            visit_emoji_slots(
                &galley,
                egui::Pos2::ZERO,
                line.emojis,
                |cluster, rect, _| {
                    assert!(rect.width() > 0.0);
                    clusters.push((cluster.to_owned(), rect.top()));
                },
            );
            assert_eq!(
                clusters
                    .iter()
                    .map(|(cluster, _)| cluster.as_str())
                    .collect::<Vec<_>>(),
                ["🐄", "🇰🇿", "👩🏽‍💻"]
            );
            assert!(clusters[0].1 < clusters[1].1);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn scaling_changes_reader_text_in_all_block_types() {
        let blocks = katran::notion_reader::parse(
            "Reader body\n\n| Column |\n| --- |\n| Cell |\n\n<toggle>\nToggle title\n</toggle>\n",
        )
        .unwrap();
        let context = egui::Context::default();
        crate::ui_fonts::install(&context);
        let render = |scale| {
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                show_scaled(ui, &blocks, scale, &mut EmojiCache::default())
            });
            output.textures_delta.clear();
            output
        };
        fn size(shape: &egui::Shape, text: &str) -> Option<f32> {
            match shape {
                egui::Shape::Text(value) if value.galley.job.text.contains(text) => value
                    .galley
                    .job
                    .sections
                    .first()
                    .map(|section| section.format.font_id.size),
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| size(shape, text)),
                _ => None,
            }
        }
        let normal = render(1.0);
        let large = render(1.5);
        for text in ["Reader body", "Column", "Cell", "Toggle title"] {
            let find = |output: &egui::FullOutput| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| size(&shape.shape, text))
                    .unwrap()
            };
            assert!((find(&large) - find(&normal) * 1.5).abs() < 0.01, "{text}");
        }
    }

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
