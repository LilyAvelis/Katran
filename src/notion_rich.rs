//! Read-only CommonMark and Notion inline formatting projected to native text runs.
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
    pub code: bool,
    pub link: bool,
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub style: Style,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub runs: Vec<Run>,
    pub heading: u8,
    pub indent: usize,
    pub quote: bool,
    pub rule: bool,
    pub code: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub source: String,
    pub lines: Vec<Line>,
}

#[derive(Default)]
struct Reader {
    lines: Vec<Line>,
    line: Line,
    style: Style,
    markdown_styles: Vec<Style>,
    html_styles: Vec<(&'static str, Style)>,
    lists: Vec<Option<u64>>,
    quotes: usize,
    heading: u8,
    code: bool,
}

impl Reader {
    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.line.heading = self.heading;
        self.line.indent = self.lists.len().saturating_sub(1) + self.quotes;
        self.line.quote = self.quotes != 0;
        self.line.code = self.code;
        let mut style = self.style.clone();
        style.code |= self.code;
        if let Some(last) = self.line.runs.last_mut()
            && last.style == style
        {
            last.text.push_str(text);
        } else {
            self.line.runs.push(Run {
                text: text.into(),
                style,
            });
        }
    }

    fn finish(&mut self) {
        if !self.line.runs.is_empty() || self.line.rule {
            self.lines.push(std::mem::take(&mut self.line));
        }
    }

    fn html_tag(&mut self, tag: &str) -> bool {
        let inner = tag.trim_start_matches('<').trim_end_matches('>').trim();
        let closing = inner.starts_with('/');
        let inner = inner.trim_start_matches('/');
        let name = inner
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let slot = match name {
            "b" => "b",
            "i" => "i",
            "s" => "s",
            "u" => "u",
            "span" => "span",
            "mention" => "mention",
            "br" => {
                self.text("\n");
                return true;
            }
            "p" => {
                self.finish();
                return true;
            }
            "h1" | "h2" | "h3" | "h4" => {
                self.finish();
                self.heading = if closing {
                    0
                } else {
                    name.as_bytes()[1] - b'0'
                };
                return true;
            }
            "quote" => {
                self.finish();
                self.quotes = if closing {
                    self.quotes.saturating_sub(1)
                } else {
                    self.quotes + 1
                };
                return true;
            }
            "ul" | "ol" => {
                self.finish();
                if closing {
                    self.lists.pop();
                } else {
                    self.lists.push((name == "ol").then_some(1));
                }
                return true;
            }
            "li" | "todo" => {
                self.finish();
                if !closing {
                    if name == "todo" {
                        let checked =
                            crate::notion_reader::attribute(&inner[name.len()..], "checked");
                        self.text(if checked.as_deref() == Some("true") {
                            "☑  "
                        } else {
                            "☐  "
                        });
                    } else {
                        self.item();
                    }
                }
                return true;
            }
            "date" if !closing => {
                if let Some(start) = crate::notion_reader::attribute(&inner[name.len()..], "start")
                {
                    self.text(&start);
                    if let Some(end) = crate::notion_reader::attribute(&inner[name.len()..], "end")
                    {
                        self.text(" – ");
                        self.text(&end);
                    }
                    return true;
                }
                return false;
            }
            _ => return false,
        };
        if inner.ends_with('/') {
            return true;
        }
        if closing {
            if self
                .html_styles
                .last()
                .is_some_and(|(name, _)| *name == slot)
            {
                self.style = self.html_styles.pop().unwrap().1;
                return true;
            }
            return false;
        }
        self.html_styles.push((slot, self.style.clone()));
        match slot {
            "b" => self.style.bold = true,
            "i" => self.style.italic = true,
            "s" => self.style.strike = true,
            "u" => self.style.underline = true,
            "mention" => self.style.link = true,
            "span" => {
                if let Some(color) = crate::notion_reader::attribute(&inner[name.len()..], "color")
                {
                    self.style.color = Some(color);
                }
            }
            _ => {}
        }
        true
    }

    fn item(&mut self) {
        let marker = if let Some(Some(index)) = self.lists.last_mut() {
            let marker = format!("{index}.  ");
            *index = index.saturating_add(1);
            marker
        } else {
            "•  ".into()
        };
        self.text(&marker);
    }

    fn html(&mut self, fragment: &str) {
        let mut rest = fragment;
        while let Some(start) = rest.find('<') {
            self.text(&crate::notion_reader::decode_entities(&rest[..start]));
            rest = &rest[start..];
            let Some(end) = crate::notion_reader::tag_end(rest) else {
                break;
            };
            let tag = &rest[..end];
            if !self.html_tag(tag) {
                self.text(tag);
            }
            rest = &rest[end..];
        }
        self.text(&crate::notion_reader::decode_entities(rest));
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => self.heading = level as u8,
            Event::End(TagEnd::Heading(_)) => {
                self.finish();
                self.heading = 0;
            }
            Event::Start(Tag::List(start)) => {
                self.finish();
                self.lists.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                self.finish();
                self.lists.pop();
            }
            Event::Start(Tag::Item) => {
                self.finish();
                self.item();
            }
            Event::Start(Tag::BlockQuote(_)) => {
                self.finish();
                self.quotes += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                self.finish();
                self.quotes -= 1;
            }
            Event::Start(Tag::CodeBlock(_)) => {
                self.finish();
                self.code = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                self.finish();
                self.code = false;
            }
            Event::End(TagEnd::Item | TagEnd::Paragraph) => self.finish(),
            Event::Start(
                tag @ (Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. }),
            ) => {
                self.markdown_styles.push(self.style.clone());
                match tag {
                    Tag::Emphasis => self.style.italic = true,
                    Tag::Strong => self.style.bold = true,
                    Tag::Strikethrough => self.style.strike = true,
                    Tag::Link { .. } => self.style.link = true,
                    _ => {}
                }
            }
            Event::End(
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link,
            ) => {
                if let Some(style) = self.markdown_styles.pop() {
                    self.style = style;
                }
            }
            Event::Text(text) => self.text(&text),
            Event::Code(text) => {
                let previous = self.style.code;
                self.style.code = true;
                self.text(&text);
                self.style.code = previous;
            }
            Event::InlineHtml(tag) => {
                if !self.html_tag(&tag) {
                    self.text(&tag);
                }
            }
            Event::Html(fragment) => self.html(&fragment),
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.text("\n"),
            Event::TaskListMarker(checked) => {
                if self.line.runs.len() == 1 && self.line.runs[0].text == "•  " {
                    self.line.runs.clear();
                }
                self.text(if checked { "☑  " } else { "☐  " });
            }
            Event::Rule => {
                self.finish();
                self.line.rule = true;
                self.finish();
            }
            Event::Start(Tag::Image { .. }) => self.text("▧  "),
            _ => {}
        }
    }
}

pub fn parse(source: &str) -> Document {
    let mut reader = Reader::default();
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for event in Parser::new_ext(source, options) {
        reader.event(event);
    }
    reader.finish();
    Document {
        source: source.into(),
        lines: reader.lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notion_inline_styles_are_native_and_escaped_tags_stay_literal() {
        let document = parse(
            "Это <b>важно и <i>читаемо</i></b>, <u>подчёркнуто</u>, <s>старое</s>.\n\n\\<b> &lt;i&gt; `<b>`",
        );
        let runs: Vec<_> = document.lines.iter().flat_map(|line| &line.runs).collect();
        assert!(
            runs.iter()
                .any(|run| run.text == "читаемо" && run.style.bold && run.style.italic)
        );
        assert!(
            runs.iter()
                .any(|run| run.text == "подчёркнуто" && run.style.underline && !run.style.bold)
        );
        assert!(
            runs.iter()
                .any(|run| run.text == "старое" && run.style.strike)
        );
        assert!(
            runs.iter()
                .any(|run| run.text.contains("<b> <i>") && !run.style.bold)
        );
        assert!(runs.iter().any(|run| run.text == "<b>" && run.style.code));
    }

    #[test]
    fn handles_commonmark_structures_and_preserves_literal_code() {
        let document = parse(
            "# Title\n\n> Quoted\n\n3. Third\n4. Fourth\n\n- [x] Done\n\n---\n\n```html\n<b>literal</b>\n```\n\n~~removed~~ and [label](https://example.com)",
        );
        assert_eq!(document.lines[0].heading, 1);
        assert!(document.lines.iter().any(|line| line.quote));
        assert!(document.lines.iter().any(|line| line.rule));
        let runs: Vec<_> = document.lines.iter().flat_map(|line| &line.runs).collect();
        for expected in ["3.  Third", "4.  Fourth", "☑  Done"] {
            assert!(runs.iter().any(|run| run.text == expected), "{expected}");
        }
        assert!(
            runs.iter()
                .any(|run| run.text.contains("<b>literal</b>") && run.style.code)
        );
        assert!(
            runs.iter()
                .any(|run| run.text == "removed" && run.style.strike)
        );
        assert!(runs.iter().any(|run| run.text == "label" && run.style.link));
    }
}
