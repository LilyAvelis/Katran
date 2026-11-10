//! Bounded, conservative Markdown projection. Source spans always refer to the exact input.
use std::ops::Range;
pub const MAX_CHARS: usize = 65_536;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Heading(u8),
    ListItem,
    CodeFence,
    Paragraph,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineKind {
    Text,
    Emphasis,
    Strong,
    Code,
    Link { destination: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inline {
    pub kind: InlineKind,
    pub span: Range<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub kind: Kind,
    pub span: Range<usize>,
    pub content: Range<usize>,
    pub inline: Vec<Inline>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub source: String,
    pub blocks: Vec<Block>,
}

/// Invalid or unsupported Markdown remains ordinary source text. No HTML is interpreted.
pub fn parse(source: &str) -> Result<Document, &'static str> {
    if source.chars().count() > MAX_CHARS {
        return Err("Markdown preview exceeds 65536 characters");
    }
    let mut blocks = Vec::new();
    let mut offset = 0;
    let mut lines = source.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        let start = offset;
        offset += line.len();
        let plain = line.trim_end_matches(['\r', '\n']);
        let trimmed = plain.trim_start();
        let indent = plain.len() - trimmed.len();
        let (kind, content_start, content_end) = if trimmed.starts_with("```") {
            let mut closed = false;
            while let Some(next) = lines.peek() {
                let close = next.trim_start().starts_with("```");
                offset += next.len();
                lines.next();
                if close {
                    closed = true;
                    break;
                }
            }
            if closed {
                let body_start = start + line.len();
                let segment = &source[body_start..offset];
                let body_end = segment
                    .rfind("```")
                    .map(|i| body_start + i)
                    .unwrap_or(offset);
                (Kind::CodeFence, body_start, body_end)
            } else {
                (Kind::Paragraph, start, offset)
            }
        } else {
            let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
            if (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
                (
                    Kind::Heading(hashes as u8),
                    start + indent + hashes + 1,
                    start + plain.len(),
                )
            } else if ["- ", "* ", "+ "]
                .iter()
                .any(|prefix| trimmed.starts_with(prefix))
            {
                (Kind::ListItem, start + indent + 2, start + plain.len())
            } else {
                (Kind::Paragraph, start, start + plain.len())
            }
        };
        let inline = if kind == Kind::CodeFence {
            Vec::new()
        } else {
            parse_inline(source, content_start..content_end)
        };
        blocks.push(Block {
            kind,
            span: start..offset,
            content: content_start..content_end,
            inline,
        });
    }
    Ok(Document {
        source: source.into(),
        blocks,
    })
}
fn parse_inline(source: &str, range: Range<usize>) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut cursor = range.start;
    let mut plain = cursor;
    let mut probes = 0;
    while cursor < range.end {
        let rest = &source[cursor..range.end];
        if matches!(rest.as_bytes()[0], b'*' | b'`' | b'[') {
            probes += 1;
            if probes > 128 {
                break;
            }
        }
        let found = if let Some(tail) = rest.strip_prefix("**") {
            tail.find("**")
                .filter(|i| *i > 0)
                .map(|end| (InlineKind::Strong, 2, end, 2))
        } else if let Some(tail) = rest.strip_prefix('*') {
            tail.find('*')
                .filter(|i| *i > 0)
                .map(|end| (InlineKind::Emphasis, 1, end, 1))
        } else if let Some(tail) = rest.strip_prefix('`') {
            tail.find('`')
                .filter(|i| *i > 0)
                .map(|end| (InlineKind::Code, 1, end, 1))
        } else if cursor > range.start && source.as_bytes()[cursor - 1] == b'!' {
            None
        } else if let Some(tail) = rest.strip_prefix('[') {
            tail.find("](")
                .and_then(|end| tail[end + 2..].find(')').map(|close| (end, close)))
                .filter(|(end, _)| *end > 0)
                .map(|(end, close)| {
                    (
                        InlineKind::Link {
                            destination: tail[end + 2..end + 2 + close].into(),
                        },
                        1,
                        end,
                        close + 3,
                    )
                })
        } else {
            None
        };
        if let Some((kind, prefix, len, suffix)) = found {
            if plain < cursor {
                out.push(Inline {
                    kind: InlineKind::Text,
                    span: plain..cursor,
                });
            }
            let begin = cursor + prefix;
            out.push(Inline {
                kind,
                span: begin..begin + len,
            });
            cursor = begin + len + suffix;
            plain = cursor;
        } else {
            cursor += rest.chars().next().unwrap().len_utf8();
        }
    }
    if plain < range.end {
        out.push(Inline {
            kind: InlineKind::Text,
            span: plain..range.end,
        });
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_source_spans_cover_every_byte_and_unclosed_syntax_stays_literal() {
        let source = "# Заголовок 🐄\r\n- **жирный** и *курсив* [ссылка](https://example.com)\n```rs\nlet x = 1;\n```\nнепарный *текст\n<script>alert(1)</script>";
        let doc = parse(source).unwrap();
        assert_eq!(doc.source, source);
        assert_eq!(
            doc.blocks
                .iter()
                .map(|b| &doc.source[b.span.clone()])
                .collect::<String>(),
            source
        );
        assert_eq!(doc.blocks[0].kind, Kind::Heading(1));
        assert_eq!(doc.blocks[1].kind, Kind::ListItem);
        assert!(
            doc.blocks[1]
                .inline
                .iter()
                .any(|i| i.kind == InlineKind::Strong && &doc.source[i.span.clone()] == "жирный")
        );
        assert_eq!(doc.blocks[2].kind, Kind::CodeFence);
        assert_eq!(
            &doc.source[doc.blocks[3].content.clone()],
            "непарный *текст"
        );
        assert!(doc.source[doc.blocks[4].content.clone()].contains("<script>"));
    }
    #[test]
    fn bounds_and_unclosed_fence_fall_back_to_source() {
        assert!(parse(&"x".repeat(MAX_CHARS + 1)).is_err());
        let text = "```\nнезакрытый код";
        let doc = parse(text).unwrap();
        assert_eq!(doc.blocks[0].kind, Kind::Paragraph);
        assert_eq!(&doc.source[doc.blocks[0].content.clone()], text);
    }
}
