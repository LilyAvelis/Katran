//! Bounded, lossless table model and side-effect-free interchange formats.
use serde::{Deserialize, Serialize};

pub const MAX_CONTENT_BYTES: usize = 1024 * 1024;
pub const MAX_ROWS: usize = 10_000;
pub const MAX_COLUMNS: usize = 256;
pub const MAX_CELLS: usize = 100_000;
// Input overhead also has a bound; canonical markup can be larger than cell content.
pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub rows: Vec<Vec<String>>,
}
impl Table {
    pub fn validate(&self) -> Result<(), String> {
        if self.rows.is_empty() || self.rows.len() > MAX_ROWS {
            return Err("Table must contain 1..10000 rows".into());
        }
        let width = self.rows[0].len();
        if width == 0 || width > MAX_COLUMNS {
            return Err("Table must contain 1..256 columns".into());
        }
        if self
            .rows
            .len()
            .checked_mul(width)
            .is_none_or(|n| n > MAX_CELLS)
        {
            return Err("Table exceeds 100000 cells".into());
        }
        let mut total = 0usize;
        for row in &self.rows {
            if row.len() != width {
                return Err("Table must be rectangular".into());
            }
            for cell in row {
                if cell.contains('\0') {
                    return Err("Table cells cannot contain NUL".into());
                }
                total = total.checked_add(cell.len()).ok_or("Table size overflow")?;
                if total > MAX_CONTENT_BYTES {
                    return Err("Table cell content exceeds 1 MiB".into());
                }
            }
        }
        Ok(())
    }
}

/// Parse Excel-style TSV. Single-column text is accepted only for explicit imports.
pub fn parse_tsv(input: &str, allow_single_column: bool) -> Result<Table, String> {
    parse_delimited(input, '\t', allow_single_column)
}
/// Parse RFC 4180 CSV, accepting an optional UTF-8 BOM.
pub fn parse_csv(input: &str) -> Result<Table, String> {
    parse_delimited(input.strip_prefix('\u{feff}').unwrap_or(input), ',', true)
}
fn parse_delimited(
    input: &str,
    delimiter: char,
    allow_single_column: bool,
) -> Result<Table, String> {
    if input.len() > MAX_INPUT_BYTES {
        return Err("TSV input exceeds size limit".into());
    }
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut closed = false;
    let mut at_start = true;
    let mut separators = 0usize;
    let mut content_bytes = 0usize;
    let mut cells = 0usize;
    let mut ended_row = false;
    while i < bytes.len() {
        let ch = input[i..].chars().next().unwrap();
        if quoted {
            if ch == '"' {
                if bytes.get(i + 1) == Some(&b'"') {
                    field.push('"');
                    content_bytes += 1;
                    i += 2;
                } else {
                    quoted = false;
                    closed = true;
                    i += 1;
                }
            } else {
                if ch == '\0' {
                    return Err("Table cells cannot contain NUL".into());
                }
                field.push(ch);
                content_bytes += ch.len_utf8();
                i += ch.len_utf8();
            }
        } else if at_start && ch == '"' {
            quoted = true;
            at_start = false;
            ended_row = false;
            i += 1;
        } else if ch == delimiter || ch == '\n' || ch == '\r' {
            cells += 1;
            if cells > MAX_CELLS {
                return Err("Table exceeds 100000 cells".into());
            }
            row.push(std::mem::take(&mut field));
            if row.len() > MAX_COLUMNS {
                return Err("Table exceeds 256 columns".into());
            }
            at_start = true;
            closed = false;
            if ch == delimiter {
                separators += 1;
                ended_row = false;
                i += 1;
            } else {
                if ch == '\r' && bytes.get(i + 1) == Some(&b'\n') {
                    i += 2;
                } else {
                    i += 1;
                }
                rows.push(std::mem::take(&mut row));
                if rows.len() > MAX_ROWS {
                    return Err("Table exceeds 10000 rows".into());
                }
                ended_row = true;
            }
        } else {
            if closed || ch == '"' || ch == '\0' {
                return Err("Malformed TSV field".into());
            }
            field.push(ch);
            content_bytes += ch.len_utf8();
            i += ch.len_utf8();
            at_start = false;
            ended_row = false;
        }
        if content_bytes > MAX_CONTENT_BYTES {
            return Err("Table cell content exceeds 1 MiB".into());
        }
    }
    if quoted {
        return Err("Unclosed quoted TSV field".into());
    }
    if !ended_row || rows.is_empty() {
        row.push(field);
        rows.push(row);
    }
    if !allow_single_column && separators == 0 {
        return Err("Not tabular clipboard text".into());
    }
    let table = Table { rows };
    table.validate()?;
    Ok(table)
}

/// Detect actual table elements, ignoring strings in comments, scripts and attributes.
/// The cheap scan avoids building a DOM for ordinary HTML.
pub fn has_html_table(input: &str) -> bool {
    if input.len() > MAX_INPUT_BYTES {
        return false;
    }
    let candidate = input
        .as_bytes()
        .windows(6)
        .any(|w| w.eq_ignore_ascii_case(b"<table"));
    if !candidate {
        return false;
    }
    let document = scraper::Html::parse_fragment(input);
    let selector = scraper::Selector::parse("table").expect("static selector");
    document.select(&selector).next().is_some() && !has_visible_text_outside_tables(&document)
}

fn has_visible_text_outside_tables(document: &scraper::Html) -> bool {
    document.tree.root().descendants().any(|node| {
        let scraper::Node::Text(text) = node.value() else {
            return false;
        };
        if text.trim().is_empty() {
            return false;
        }
        !node.ancestors().any(|ancestor| {
            ancestor.value().as_element().is_some_and(|element| {
                matches!(
                    element.name(),
                    "table" | "script" | "style" | "head" | "title" | "template" | "noscript"
                )
            })
        })
    })
}

/// Parse one complete HTML table; markup is interpreted by HTML5ever through scraper.
/// Spans preserve the anchor and leave covered positions empty. Nested or multiple
/// tables and spans extending past the supplied rows are rejected.
pub fn parse_html(input: &str) -> Result<Table, String> {
    use scraper::{ElementRef, Html, Selector};
    if input.len() > MAX_INPUT_BYTES {
        return Err("HTML input exceeds size limit".into());
    }
    if input.contains('\0') {
        return Err("HTML input cannot contain NUL".into());
    }
    let document = Html::parse_fragment(input);
    if has_visible_text_outside_tables(&document) {
        return Err("HTML contains visible text outside the table".into());
    }
    let table_selector = Selector::parse("table").map_err(|_| "Invalid table selector")?;
    let mut tables = document.select(&table_selector);
    let table = tables.next().ok_or("HTML contains no complete table")?;
    if tables.next().is_some() {
        return Err("Nested or multiple HTML tables are unsupported".into());
    }
    let row_selector = Selector::parse("tr").map_err(|_| "Invalid row selector")?;
    let rows: Vec<_> = table.select(&row_selector).take(MAX_ROWS + 1).collect();
    if rows.is_empty() || rows.len() > MAX_ROWS {
        return Err("HTML table must contain 1..10000 rows".into());
    }
    let mut grid: Vec<Vec<Option<String>>> = vec![Vec::new(); rows.len()];
    let mut width = 0usize;
    let mut total = 0usize;
    for (y, row) in rows.iter().enumerate() {
        let mut x = 0usize;
        for cell in row
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|element| matches!(element.value().name(), "td" | "th"))
        {
            while grid[y].get(x).is_some_and(Option::is_some) {
                x += 1;
            }
            let span = |name: &str, max: usize| -> Result<usize, String> {
                match cell.attr(name) {
                    None => Ok(1),
                    Some(value) => {
                        let value = value
                            .trim()
                            .parse::<usize>()
                            .map_err(|_| format!("Invalid {name}"))?;
                        if value == 0 || value > max {
                            return Err(format!("Invalid {name}"));
                        }
                        Ok(value)
                    }
                }
            };
            let colspan = span("colspan", MAX_COLUMNS)?;
            let rowspan = span("rowspan", MAX_ROWS)?;
            let right = x.checked_add(colspan).ok_or("HTML span overflow")?;
            let bottom = y.checked_add(rowspan).ok_or("HTML span overflow")?;
            if right > MAX_COLUMNS || bottom > rows.len() {
                return Err("HTML cell span exceeds table bounds".into());
            }
            width = width.max(right);
            if width.checked_mul(rows.len()).is_none_or(|n| n > MAX_CELLS) {
                return Err("Table exceeds 100000 cells".into());
            }
            let mut value = String::new();
            let mut generated = false;
            html_cell_text(cell, &mut value, &mut generated, 0)?;
            if generated && value.ends_with('\n') {
                value.pop();
            }
            total = total
                .checked_add(value.len())
                .ok_or("Table size overflow")?;
            if total > MAX_CONTENT_BYTES {
                return Err("Table cell content exceeds 1 MiB".into());
            }
            for (target_y, line) in grid.iter_mut().enumerate().take(bottom).skip(y) {
                line.resize_with(right.max(line.len()), || None);
                for (target_x, slot) in line.iter_mut().enumerate().take(right).skip(x) {
                    if slot.is_some() {
                        return Err("HTML cell spans overlap".into());
                    }
                    *slot = Some(if target_y == y && target_x == x {
                        value.clone()
                    } else {
                        String::new()
                    });
                }
            }
            x = right;
        }
    }
    if width == 0 {
        return Err("HTML table has no cells".into());
    }
    let rows = grid
        .into_iter()
        .map(|mut line| {
            line.resize_with(width, || None);
            line.into_iter().map(Option::unwrap_or_default).collect()
        })
        .collect();
    let table = Table { rows };
    table.validate()?;
    Ok(table)
}
fn html_cell_text(
    element: scraper::ElementRef<'_>,
    output: &mut String,
    generated: &mut bool,
    depth: usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err("HTML nesting exceeds limit".into());
    }
    for child in element.children() {
        match child.value() {
            scraper::Node::Text(text) => {
                output.push_str(text);
                *generated = false;
            }
            scraper::Node::Element(_) => {
                let child = scraper::ElementRef::wrap(child).unwrap();
                let name = child.value().name();
                if matches!(
                    name,
                    "script" | "style" | "img" | "svg" | "iframe" | "object"
                ) {
                    continue;
                }
                if name == "br" {
                    output.push('\n');
                    *generated = false;
                } else {
                    let block = matches!(
                        name,
                        "p" | "div" | "li" | "section" | "article" | "blockquote" | "pre"
                    );
                    if block && !output.is_empty() && !output.ends_with('\n') {
                        output.push('\n');
                    }
                    html_cell_text(child, output, generated, depth + 1)?;
                    if block && !output.ends_with('\n') {
                        output.push('\n');
                        *generated = true;
                    }
                }
            }
            _ => {}
        }
        if output.len() > MAX_CONTENT_BYTES {
            return Err("Table cell content exceeds 1 MiB".into());
        }
    }
    Ok(())
}

fn quote_field(value: &str, delimiter: char) -> String {
    if value.contains([delimiter, '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.into()
    }
}
fn delimited(table: &Table, delimiter: char, line_end: &str) -> Result<String, String> {
    table.validate()?;
    let mut output = table
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| quote_field(cell, delimiter))
                .collect::<Vec<_>>()
                .join(&delimiter.to_string())
        })
        .collect::<Vec<_>>()
        .join(line_end);
    // A terminator distinguishes a final empty record from end-of-input.
    output.push_str(line_end);
    Ok(output)
}
pub fn to_tsv(table: &Table) -> Result<String, String> {
    delimited(table, '\t', "\r\n")
}
pub fn to_csv_bom(table: &Table) -> Result<Vec<u8>, String> {
    let content = delimited(table, ',', "\r\n")?;
    let mut output = vec![0xef, 0xbb, 0xbf];
    output.extend_from_slice(content.as_bytes());
    Ok(output)
}
fn escape_html(text: &str) -> String {
    let mut result = String::new();
    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            '\n' => result.push_str("<br>"),
            '\r' => result.push_str("&#13;"),
            _ => result.push(ch),
        }
    }
    result
}
pub fn to_html(table: &Table) -> Result<String, String> {
    table.validate()?;
    let mut result = String::from("<table>");
    for row in &table.rows {
        result.push_str("<tr>");
        for cell in row {
            result.push_str("<td>");
            result.push_str(&escape_html(cell));
            result.push_str("</td>");
        }
        result.push_str("</tr>");
    }
    result.push_str("</table>");
    Ok(result)
}

/// Decode byte offsets, never treating UTF-8 character positions as bytes.
/// Header keys and numeric fields are ASCII; SourceURL may contain UTF-8.
pub fn extract_cf_html(data: &[u8]) -> Result<String, String> {
    const MAX_HEADER: usize = 16 * 1024;
    if data.len() > MAX_INPUT_BYTES {
        return Err("CF_HTML exceeds size limit".into());
    }
    let end = data
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(data.len());
    if data[end..].iter().any(|byte| *byte != 0) {
        return Err("CF_HTML contains embedded NUL".into());
    }
    let data = &data[..end];
    let boundary = data
        .iter()
        .position(|byte| *byte == b'<')
        .ok_or("CF_HTML has no HTML boundary")?;
    if boundary > MAX_HEADER {
        return Err("CF_HTML header exceeds limit".into());
    }
    let header =
        std::str::from_utf8(&data[..boundary]).map_err(|_| "CF_HTML header must be ASCII")?;
    let mut start_html = None;
    let mut end_html = None;
    let mut start_fragment = None;
    let mut end_fragment = None;
    let mut header_end = 0usize;
    for segment in header.split_inclusive(['\r', '\n']) {
        let line = segment.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            header_end += segment.len();
            continue;
        }
        let (key, value) = match line.split_once(':') {
            Some(pair) => pair,
            None => break,
        };
        if !key.is_ascii() || (key != "SourceURL" && !value.is_ascii()) {
            return Err("Invalid CF_HTML header".into());
        }
        let slot = match key {
            "StartHTML" => Some(&mut start_html),
            "EndHTML" => Some(&mut end_html),
            "StartFragment" => Some(&mut start_fragment),
            "EndFragment" => Some(&mut end_fragment),
            "Version" | "SourceURL" | "StartSelection" | "EndSelection" => None,
            _ => break,
        };
        if let Some(slot) = slot {
            if slot.is_some() {
                return Err("Duplicate CF_HTML offset".into());
            }
            *slot = Some(
                value
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| "Invalid CF_HTML offset")?,
            );
        }
        header_end += segment.len();
    }
    let bound = |value: Option<i64>, fallback: usize| -> Result<usize, String> {
        match value {
            Some(-1) => Ok(fallback),
            Some(n) if n >= 0 => usize::try_from(n).map_err(|_| "CF_HTML offset overflow".into()),
            _ => Err("Missing or invalid CF_HTML offset".into()),
        }
    };
    let html_start = bound(start_html, header_end)?;
    let html_end = bound(end_html, data.len())?;
    let start = bound(start_fragment, usize::MAX)?;
    let finish = bound(end_fragment, usize::MAX)?;
    if header_end > html_start
        || html_start > start
        || start > finish
        || finish > html_end
        || html_end > data.len()
    {
        return Err("CF_HTML offsets are outside document".into());
    }
    std::str::from_utf8(&data[start..finish])
        .map(str::to_owned)
        .map_err(|_| "CF_HTML fragment splits UTF-8".into())
}
pub fn encode_cf_html(fragment: &str) -> Result<Vec<u8>, String> {
    if fragment.len() > MAX_INPUT_BYTES {
        return Err("HTML fragment exceeds size limit".into());
    }
    const TEMPLATE: &str = "Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n";
    const BEFORE: &str = "<html><body><!--StartFragment-->";
    const AFTER: &str = "<!--EndFragment--></body></html>";
    let start_html = TEMPLATE.len();
    let start_fragment = start_html + BEFORE.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + AFTER.len();
    let header = format!(
        "Version:1.0\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n"
    );
    let mut result = Vec::with_capacity(end_html);
    result.extend_from_slice(header.as_bytes());
    result.extend_from_slice(BEFORE.as_bytes());
    result.extend_from_slice(fragment.as_bytes());
    result.extend_from_slice(AFTER.as_bytes());
    Ok(result)
}
