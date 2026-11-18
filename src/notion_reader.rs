//! Bounded, read-only Notion Markdown v2 projection. Unknown markup remains text.
//! No tag in this module executes code or fetches an external resource.
use crate::{markdown, notion_rich};

const MAX_DEPTH: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    Database,
    Page,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Markdown(notion_rich::Document),
    Callout {
        icon: Option<String>,
        color: Option<String>,
        children: Vec<Block>,
    },
    Toggle {
        title: String,
        children: Vec<Block>,
    },
    Table {
        rows: Vec<Vec<Cell>>,
    },
    Reference {
        kind: ReferenceKind,
        title: String,
        url: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub header: bool,
    pub document: notion_rich::Document,
}

impl Cell {
    fn new(text: String, header: bool) -> Self {
        let document = notion_rich::parse(&text);
        Self {
            text: decode_entities(&text),
            header,
            document,
        }
    }
}

pub fn parse(source: &str) -> Result<Vec<Block>, &'static str> {
    if source.chars().count() > markdown::MAX_CHARS {
        return Err("Markdown preview exceeds 65536 characters");
    }
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    parse_lines(&lines, 0)
}
fn flush(pending: &mut String, out: &mut Vec<Block>) {
    if !pending.trim().is_empty() {
        out.push(Block::Markdown(notion_rich::parse(pending)));
    }
    pending.clear();
}
fn fence_marker(line: &str) -> Option<usize> {
    let t = line.trim_start();
    let n = t.bytes().take_while(|&b| b == b'`').count();
    (n >= 3).then_some(n)
}
fn opening<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let text = line.trim();
    let tail = text.strip_prefix('<')?.strip_prefix(name)?;
    if !tail.starts_with('>') && !tail.starts_with(char::is_whitespace) {
        return None;
    }
    let attrs = tail.strip_suffix('>')?;
    if attrs.ends_with('/') || attrs.contains('<') || attrs.contains('>') {
        return None;
    }
    Some(attrs)
}
fn close(line: &str, name: &str) -> bool {
    line.trim() == format!("</{name}>")
}
/// Recognize matching nested container tags only outside fenced code.
fn container_end(lines: &[&str], start: usize, name: &str) -> Option<usize> {
    let mut stack = vec![name];
    let mut fence = None;
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        if let Some(size) = fence_marker(line) {
            if fence.is_none() {
                fence = Some(size);
            } else if size >= fence.unwrap() {
                fence = None;
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        if let Some(open) = ["callout", "toggle"]
            .into_iter()
            .find(|n| opening(line, n).is_some())
        {
            if stack.len() >= MAX_DEPTH {
                return None;
            }
            stack.push(open);
        } else if ["callout", "toggle"].into_iter().any(|n| close(line, n)) {
            if !close(line, stack.pop()?) {
                return None;
            }
            if stack.is_empty() {
                return Some(i);
            }
        }
    }
    None
}
pub(crate) fn attribute(attrs: &str, wanted: &str) -> Option<String> {
    let mut rest = attrs.trim();
    while !rest.is_empty() {
        let end = rest.find(|c: char| c.is_whitespace() || c == '=')?;
        let key = &rest[..end];
        rest = rest[end..].trim_start();
        rest = rest.strip_prefix('=')?.trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        rest = &rest[quote.len_utf8()..];
        let end = rest.find(quote)?;
        let value = &rest[..end];
        if key == wanted {
            return Some(decode_entities(value));
        }
        rest = rest[end + quote.len_utf8()..].trim_start();
    }
    None
}
fn valid_notion_url(value: &str) -> Option<String> {
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) || value.contains('\\') {
        return None;
    }
    let authority = value.split_once("://")?.1.split(['/', '?', '#']).next()?;
    if authority.contains('@') {
        return None;
    }
    let parsed = url::Url::parse(value).ok()?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    let host = parsed.host_str()?;
    if !(host == "notion.so"
        || host == "notion.com"
        || host.ends_with(".notion.so")
        || host.ends_with(".notion.com"))
    {
        return None;
    }
    Some(value.to_owned())
}
fn escaped_at(text: &str, position: usize) -> bool {
    text.as_bytes()[..position]
        .iter()
        .rev()
        .take_while(|&&b| b == b'\\')
        .count()
        % 2
        == 1
}
fn find_unescaped(text: &str, needle: &str) -> Option<usize> {
    text.match_indices(needle)
        .map(|(index, _)| index)
        .find(|&index| !escaped_at(text, index))
}
/// Recognize a tag outside inline code and escaped literal markup.
fn find_reference(line: &str) -> Option<(usize, ReferenceKind, &str, usize)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut code = None;
    while i < bytes.len() {
        if bytes[i] == b'`' && !escaped_at(line, i) {
            let start = i;
            while i < bytes.len() && bytes[i] == b'`' {
                i += 1;
            }
            let count = i - start;
            if code == Some(count) {
                code = None;
            } else if code.is_none() {
                code = Some(count);
            }
            continue;
        }
        if bytes[i] == b'<' && code.is_none() && !escaped_at(line, i) {
            for (name, kind) in [
                ("database", ReferenceKind::Database),
                ("page", ReferenceKind::Page),
            ] {
                if let Some((attrs, tail)) = tag(&line[i..], name) {
                    if !attrs.trim_end().ends_with('/') {
                        return Some((i, kind, attrs, line.len() - tail.len()));
                    }
                }
            }
        }
        i += 1;
    }
    None
}
fn parse_lines(lines: &[&str], depth: usize) -> Result<Vec<Block>, &'static str> {
    let mut out = Vec::new();
    let mut pending = String::new();
    let mut i = 0;
    let mut offset = 0;
    let mut fence = None;
    while i < lines.len() {
        let line = &lines[i][offset..];
        if offset == 0
            && let Some(size) = fence_marker(line)
        {
            if fence.is_none() {
                fence = Some(size);
            } else if size >= fence.unwrap() {
                fence = None;
            }
            pending.push_str(line);
            i += 1;
            continue;
        }
        if fence.is_some() {
            pending.push_str(line);
            i += 1;
            continue;
        }
        let structural = offset == 0
            && (tag(line.trim_start(), "table").is_some()
                || opening(line, "callout").is_some()
                || opening(line, "toggle").is_some());
        if let Some((position, kind, attrs, after_open)) = if structural {
            None
        } else {
            find_reference(line)
        } {
            let name = match kind {
                ReferenceKind::Database => "database",
                ReferenceKind::Page => "page",
            };
            let closing = format!("</{name}>");
            let mut end_line = i;
            let mut segment = &line[after_open..];
            let mut body = String::new();
            let mut end_offset = None;
            loop {
                if let Some(end) = find_unescaped(segment, &closing) {
                    body.push_str(&segment[..end]);
                    end_offset = Some(if end_line == i {
                        offset + after_open + end + closing.len()
                    } else {
                        end + closing.len()
                    });
                    break;
                }
                body.push_str(segment);
                end_line += 1;
                if end_line >= lines.len() || fence_marker(lines[end_line]).is_some() {
                    break;
                }
                segment = lines[end_line];
            }
            if let Some(end_offset) = end_offset
                && !body.contains("<page")
                && !body.contains("<database")
            {
                pending.push_str(&line[..position]);
                flush(&mut pending, &mut out);
                let title = decode_entities(body.trim());
                let url = attribute(attrs, "url").and_then(|url| valid_notion_url(&url));
                out.push(Block::Reference { kind, title, url });
                i = end_line;
                offset = end_offset;
                if offset >= lines[i].len() {
                    i += 1;
                    offset = 0;
                }
                continue;
            }
            // An unclosed or nested unknown reference stays literal, without
            // repeatedly rescanning a potentially adversarial suffix.
            pending.push_str(line);
            for remaining in &lines[i + 1..] {
                pending.push_str(remaining);
            }
            break;
        }
        if matches!(line.trim(), "<p />" | "<p/>") {
            pending.push('\n');
            i += 1;
            offset = 0;
            continue;
        }
        if offset == 0 && depth < MAX_DEPTH {
            let container = ["callout", "toggle"]
                .into_iter()
                .find_map(|name| opening(line, name).map(|attrs| (name, attrs)));
            if let Some((name, attrs)) = container
                && let Some(end) = container_end(lines, i, name)
            {
                flush(&mut pending, &mut out);
                let mut body = &lines[i + 1..end];
                if name == "callout" {
                    out.push(Block::Callout {
                        icon: attribute(attrs, "icon"),
                        color: attribute(attrs, "color"),
                        children: parse_lines(body, depth + 1)?,
                    });
                } else {
                    while body.first().is_some_and(|line| line.trim().is_empty()) {
                        body = &body[1..];
                    }
                    let title = body
                        .first()
                        .map_or("", |line| line.trim_end_matches(['\r', '\n']))
                        .trim()
                        .to_owned();
                    if !body.is_empty() {
                        body = &body[1..];
                    }
                    out.push(Block::Toggle {
                        title: if title == "<p />" {
                            String::new()
                        } else {
                            title
                        },
                        children: parse_lines(body, depth + 1)?,
                    });
                }
                i = end + 1;
                offset = 0;
                continue;
            }
        }
        if offset == 0 && tag(line.trim_start(), "table").is_some() {
            let end = lines[i..].iter().position(|line| line.contains("</table>"));
            if let Some(end) = end {
                let end = i + end;
                let raw = lines[i..=end].concat();
                if let Some(rows) = styled_table(&raw) {
                    flush(&mut pending, &mut out);
                    out.push(Block::Table { rows });
                    i = end + 1;
                    offset = 0;
                    continue;
                }
                // Do not repeatedly rescan a malformed nested table or hide it.
                pending.push_str(&raw);
                i = end + 1;
                offset = 0;
                continue;
            }
            for line in &lines[i..] {
                pending.push_str(line);
            }
            break;
        }
        if offset == 0
            && i + 1 < lines.len()
            && let Some(head) = pipe_cells(line)
            && let Some(rule) = pipe_cells(lines[i + 1])
            && head.len() == rule.len()
            && !head.is_empty()
            && rule.iter().all(|cell| delimiter(cell))
        {
            flush(&mut pending, &mut out);
            let mut rows = vec![head.into_iter().map(|text| Cell::new(text, true)).collect()];
            i += 2;
            while i < lines.len() {
                let Some(cells) = pipe_cells(lines[i]) else {
                    break;
                };
                rows.push(
                    cells
                        .into_iter()
                        .map(|text| Cell::new(text, false))
                        .collect(),
                );
                i += 1;
            }
            out.push(Block::Table { rows });
            offset = 0;
            continue;
        }
        pending.push_str(line);
        i += 1;
        offset = 0;
    }
    flush(&mut pending, &mut out);
    Ok(out)
}
fn pipe_cells(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    if !line.contains('|') {
        return None;
    }
    let line = line.strip_prefix('|').unwrap_or(line);
    let mut cells = Vec::new();
    let mut part = String::new();
    let mut chars = line.chars().peekable();
    let mut last_was_delimiter = false;
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'|') {
            chars.next();
            part.push('|');
            last_was_delimiter = false;
        } else if c == '|' {
            cells.push(part.trim().to_owned());
            part.clear();
            last_was_delimiter = true;
        } else {
            part.push(c);
            last_was_delimiter = false;
        }
    }
    if !last_was_delimiter || !part.trim().is_empty() {
        cells.push(part.trim().to_owned());
    }
    Some(cells)
}
fn delimiter(value: &str) -> bool {
    let value = value.trim().trim_start_matches(':').trim_end_matches(':');
    value.len() >= 3 && value.bytes().all(|b| b == b'-')
}
pub(crate) fn decode_entities(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        rest = &rest[index..];
        let known = [
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
            ("&apos;", "'"),
        ];
        if let Some((encoded, decoded)) = known
            .into_iter()
            .find(|(encoded, _)| rest.starts_with(encoded))
        {
            out.push_str(decoded);
            rest = &rest[encoded.len()..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}
/// Return the end offset of an opening tag while respecting quoted attributes.
pub(crate) fn tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in text.char_indices() {
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), c) if q == c => quote = None,
            (None, '>') => return Some(i + 1),
            _ => {}
        }
    }
    None
}
fn tag<'a>(input: &'a str, expected: &str) -> Option<(&'a str, &'a str)> {
    let tail = input.strip_prefix('<')?.strip_prefix(expected)?;
    if !tail.starts_with('>') && !tail.starts_with(char::is_whitespace) {
        return None;
    }
    let end = tag_end(input)?;
    Some((&input[1 + expected.len()..end - 1], &input[end..]))
}
fn styled_table(raw: &str) -> Option<Vec<Vec<Cell>>> {
    let raw = raw.trim();
    let (attrs, after_open) = tag(raw, "table")?;
    let mut body = after_open.strip_suffix("</table>")?.trim();
    let header_row = attribute(attrs, "header-row").as_deref() == Some("true");
    let header_column = attribute(attrs, "header-column").as_deref() == Some("true");
    // Column widths and colours are display hints; rows/cells carry the content.
    if body.starts_with("<colgroup") {
        let (_, after_group) = tag(body, "colgroup")?;
        let end = after_group.find("</colgroup>")?;
        let mut columns = after_group[..end].trim();
        while !columns.is_empty() {
            let (attrs, rest) = tag(columns, "col")?;
            if !attrs.trim_end().ends_with('/') {
                return None;
            }
            columns = rest.trim_start();
        }
        body = after_group[end + "</colgroup>".len()..].trim_start();
    }
    let mut rows = Vec::new();
    while !body.is_empty() {
        let (_, tail) = tag(body, "tr")?;
        let end = tail.find("</tr>")?;
        let mut row_body = tail[..end].trim();
        let mut row = Vec::new();
        while !row_body.is_empty() {
            let (name, header) = if row_body.starts_with("<th") {
                ("th", true)
            } else if row_body.starts_with("<td") {
                ("td", false)
            } else {
                return None;
            };
            let (_, tail) = tag(row_body, name)?;
            let closing = format!("</{name}>");
            let end = tail.find(&closing)?;
            let value = tail[..end]
                .replace("<br />", "\n")
                .replace("<br/>", "\n")
                .replace("<br>", "\n");
            row.push(Cell::new(
                value,
                header || (header_row && rows.is_empty()) || (header_column && row.is_empty()),
            ));
            row_body = tail[end + closing.len()..].trim_start();
        }
        if row.is_empty() {
            return None;
        }
        rows.push(row);
        body = tail[end + "</tr>".len()..].trim_start();
    }
    (!rows.is_empty()).then_some(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_callout_and_toggle_keep_first_lines() {
        let source = "<callout icon=\"💡\" color=\"yellow_bg\">\nBody first\n\n<toggle>\nTitle\n\n**child**\n</toggle>\n</callout>\n";
        let blocks = parse(source).unwrap();
        let Block::Callout {
            icon,
            color,
            children,
        } = &blocks[0]
        else {
            panic!()
        };
        assert_eq!(icon.as_deref(), Some("💡"));
        assert_eq!(color.as_deref(), Some("yellow_bg"));
        assert!(matches!(&children[0], Block::Markdown(doc) if doc.source.contains("Body first")));
        assert!(
            matches!(&children[1], Block::Toggle {title,children} if title == "Title" && matches!(&children[0], Block::Markdown(doc) if doc.source.contains("**child**")))
        );
    }
    #[test]
    fn plain_and_styled_tables_keep_cells() {
        let plain = parse("| A | B |\n| --- | :---: |\n| x\\|y | z |\n").unwrap();
        let Block::Table { rows } = &plain[0] else {
            panic!()
        };
        assert!(rows[0][0].header);
        assert_eq!(rows[1][0].text, "x|y");
        let styled = parse("<table header-row=\"true\" header-column=\"true\">\n<colgroup><col width=\"180\" /></colgroup>\n<tr><td>Hi<br />there</td><th color=\"blue\">A &amp; B</th></tr>\n<tr><td>x</td><td>y</td></tr>\n</table>\n").unwrap();
        let Block::Table { rows } = &styled[0] else {
            panic!()
        };
        assert_eq!(rows[0][0].text, "Hi\nthere");
        assert_eq!(rows[0][1].text, "A & B");
        assert!(rows[0][0].header && rows[1][0].header && !rows[1][1].header);
    }
    #[test]
    fn fenced_and_unknown_markup_remains_literal() {
        let source = "```\n<callout>\nnot a callout\n</callout>\n```\n<script>alert(1)</script>\n";
        let blocks = parse(source).unwrap();
        assert!(matches!(&blocks[0], Block::Markdown(doc) if doc.source == source));
        let malformed = "<callout>\nunclosed\n";
        assert!(
            matches!(&parse(malformed).unwrap()[0], Block::Markdown(doc) if doc.source == malformed)
        );
        assert!(parse(&"x".repeat(markdown::MAX_CHARS + 1)).is_err());
        let malformed_table =
            "<table>\n<colgroup>do not hide me</colgroup>\n<tr><td>x</td></tr>\n</table>\n";
        assert!(
            matches!(&parse(malformed_table).unwrap()[0], Block::Markdown(doc) if doc.source == malformed_table)
        );
        assert!(
            matches!(&parse("<table><tr><td>x</td></tr></table>").unwrap()[0], Block::Table { rows } if rows[0][0].text == "x")
        );
    }
    #[test]
    fn parses_multiline_and_inline_references_with_safe_destinations() {
        let source = "<database url=\"https://work.notion.so/db\" data-source-url=\"https://evil.test/ds\">\nMy database\n</database>\nBefore <page url=\"https://www.notion.com/page?a=1&amp;b=2\">My page</page> after\n";
        let blocks = parse(source).unwrap();
        assert!(
            matches!(&blocks[0], Block::Reference {kind: ReferenceKind::Database, title, url} if title == "My database" && url.as_deref() == Some("https://work.notion.so/db"))
        );
        assert!(matches!(&blocks[1], Block::Markdown(doc) if doc.source.contains("Before ")));
        assert!(
            matches!(&blocks[2], Block::Reference {kind: ReferenceKind::Page, title, url} if title == "My page" && url.as_deref() == Some("https://www.notion.com/page?a=1&b=2"))
        );
        assert!(matches!(&blocks[3], Block::Markdown(doc) if doc.source.contains(" after")));
    }
    #[test]
    fn reference_urls_reject_credentials_wrong_hosts_and_non_https() {
        for url in [
            "http://notion.so/x",
            "https://notion.so.evil.test/x",
            "https://evilnotion.com/x",
            "https://user@notion.so/x",
            "https://@notion.so/x",
            "https://notion.so\\@evil.test",
            "javascript:alert(1)",
            "https://notion.so/hello world",
        ] {
            assert!(valid_notion_url(url).is_none(), "{url}");
        }
        for url in [
            "https://notion.so/x",
            "https://notion.com/x",
            "https://team.notion.so/x",
            "https://a.b.notion.com/x",
        ] {
            assert_eq!(valid_notion_url(url).as_deref(), Some(url));
        }
        let blocks = parse("<database data-source-url=\"https://notion.so/source\">\nTitle\n</database>\n<page url=\"https://evil.test\">Label</page>\n").unwrap();
        assert!(
            matches!(&blocks[0], Block::Reference { url: None, title, .. } if title == "Title")
        );
        assert!(
            matches!(&blocks[1], Block::Reference { url: None, title, .. } if title == "Label")
        );
    }
    #[test]
    fn references_are_inert_in_escaped_and_code_text_and_empty_paragraph_is_whitespace() {
        let source = "\\<page url=\"https://notion.so/x\">escaped</page>\n`<page url=\"https://notion.so/x\">code</page>`\n```\n<page url=\"https://notion.so/x\">fenced</page>\n<p />\n```\n<p />\n";
        let blocks = parse(source).unwrap();
        assert_eq!(blocks.len(), 1);
        assert!(
            matches!(&blocks[0], Block::Markdown(doc) if doc.source.contains("escaped</page>") && doc.source.contains("code</page>") && doc.source.contains("fenced</page>") && doc.source.contains("<p />\n```" ) && !doc.source.ends_with("<p />\n"))
        );
    }
}
