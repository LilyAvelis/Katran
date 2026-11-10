//! Pure recognition and export of single HTTP(S) links. No network or storage effects.
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkFragment {
    /// Exact captured value, including Unicode, query and fragment.
    pub url: String,
}

/// A fragment is a link only when the entire input is a single absolute HTTP(S)
/// URL. Do not trim prose or whitespace and accidentally change classification.
pub fn recognize(input: &str) -> Option<LinkFragment> {
    if input.is_empty()
        || input
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control() || ch == '\\')
    {
        return None;
    }
    let parsed = Url::parse(input).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    // Reject authority shorthand such as http:example.com, which Url::parse
    // otherwise repairs. Only explicit hierarchical URLs are classified.
    let scheme_end = input.find(':')?;
    let authority = input[scheme_end + 1..].strip_prefix("//")?;
    if authority.is_empty() || authority.starts_with(['/', '?', '#']) {
        return None;
    }
    Some(LinkFragment { url: input.into() })
}

/// Every export validates again, including fragments deserialized from clients.
fn validated(links: &[LinkFragment]) -> std::result::Result<(), &'static str> {
    if links.iter().any(|link| recognize(&link.url).is_none()) {
        return Err("invalid_link");
    }
    Ok(())
}
fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}
fn markdown_label(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '[' | ']' | '*' | '_' | '`' | '<' | '>') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}
/// Exact link values, one per LF-terminated line; empty input yields empty text.
pub fn serialize_txt(links: &[LinkFragment]) -> std::result::Result<String, &'static str> {
    validated(links)?;
    Ok(links.iter().map(|link| format!("{}\n", link.url)).collect())
}
/// Markdown links retain the original display and use an escaped URL destination.
pub fn serialize_markdown(links: &[LinkFragment]) -> std::result::Result<String, &'static str> {
    validated(links)?;
    Ok(links
        .iter()
        .map(|link| {
            let destination = link.url.replace('<', "%3C").replace('>', "%3E");
            format!("[{}](<{}>)\n", markdown_label(&link.url), destination)
        })
        .collect())
}
/// One deterministic CRLF Windows InternetShortcut file per link.
pub fn serialize_internet_shortcuts(
    links: &[LinkFragment],
) -> std::result::Result<Vec<(String, String)>, &'static str> {
    validated(links)?;
    Ok(links
        .iter()
        .enumerate()
        .map(|(index, link)| {
            (
                format!("link-{:03}.url", index + 1),
                format!("[InternetShortcut]\r\nURL={}\r\n", link.url),
            )
        })
        .collect())
}
/// Netscape bookmark interchange format with escaped attribute and text values.
pub fn serialize_bookmarks_html(
    links: &[LinkFragment],
) -> std::result::Result<String, &'static str> {
    validated(links)?;
    let mut result = String::from(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks</H1>\n<DL><p>\n",
    );
    for link in links {
        let value = html_escape(&link.url);
        result.push_str(&format!("    <DT><A HREF=\"{value}\">{value}</A>\n"));
    }
    result.push_str("</DL><p>\n");
    Ok(result)
}

/// Prepare a validated exact URL as a managed blob with explicit per-event type.
pub(crate) fn prepare(
    root: &std::path::Path,
    quota: u64,
    url: &str,
    source: &str,
    origin: &crate::origin::Origin,
    references: &[std::path::PathBuf],
) -> crate::Result<crate::storage::PreparedBlob> {
    if url.len() > crate::text_fragments::MAX_TEXT_BYTES || recognize(url).is_none() {
        return Err(crate::ApiError::new(
            "invalid_link",
            "Expected one exact HTTP(S) URL up to 1 MiB",
        ));
    }
    let name = format!(
        "Link-{}-{}.txt",
        crate::now_ms(),
        NEXT_LINK.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let mut receipt = crate::storage::prepare_bytes(
        root,
        quota,
        crate::storage::ByteIngest {
            name: &name,
            source,
            bytes: url.as_bytes(),
            origin,
        },
        references,
    )?;
    receipt.mark_link(url.to_owned());
    Ok(receipt)
}
static NEXT_LINK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) fn read_event(event: &crate::Event) -> crate::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    if event.kind != crate::EventKind::LinkFragment {
        return Err(crate::ApiError::new("not_link", "Item is not a link"));
    }
    crate::open_file::validate(event)?;
    let mut bytes = Vec::new();
    std::fs::File::open(&event.path)?
        .take((crate::text_fragments::MAX_TEXT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > crate::text_fragments::MAX_TEXT_BYTES
        || event.revision != format!("blob:{:x}", Sha256::digest(&bytes))
    {
        return Err(crate::ApiError::new(
            "source_changed",
            "Link content changed",
        ));
    }
    let url = String::from_utf8(bytes)
        .map_err(|_| crate::ApiError::new("invalid_link", "Link is not UTF-8"))?;
    if recognize(&url).is_none() {
        return Err(crate::ApiError::new(
            "invalid_link",
            "Stored link is invalid",
        ));
    }
    Ok(url)
}
impl crate::Service {
    pub(crate) fn get_link(&self, id: i64) -> crate::Result<serde_json::Value> {
        let event = self.get(id)?;
        let url = read_event(&event)?;
        Ok(serde_json::json!({"id":id,"revision":event.revision,"url":url}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognize_exact_single_url_without_repairing_prose_or_schemes() {
        for text in [
            "https://пример.рф/путь?q=один&x=2#якорь",
            "HTTP://example.com/a?x=1#frag",
        ] {
            assert_eq!(recognize(text).unwrap().url, text);
        }
        for text in [
            "",
            "hello https://example.com",
            "https://example.com\nnext",
            " https://example.com",
            "https://example.com ",
            "https://example.com/a\tb",
            "javascript:alert(1)",
            "file:///x",
            "http:example.com",
            "https:///",
            "https:///example.com",
            "https://example.com\\evil",
            "https://",
        ] {
            assert!(recognize(text).is_none(), "{text:?}");
        }
    }
    #[test]
    fn exporters_escape_inputs_and_generate_unique_shortcut_names() {
        let links = vec![
            recognize("https://example.com/a?x=1&y=\"<tag>\"#🐄").unwrap(),
            recognize("https://example.com/a?x=1&y=\"<tag>\"#🐄").unwrap(),
        ];
        let shortcuts = serialize_internet_shortcuts(&links).unwrap();
        assert_eq!(shortcuts[0].0, "link-001.url");
        assert_eq!(shortcuts[1].0, "link-002.url");
        assert!(shortcuts[0].1.contains("#🐄\r\n"));
        let html = serialize_bookmarks_html(&links).unwrap();
        assert!(html.contains("&amp;y=&quot;&lt;tag&gt;&quot;"));
        let md = serialize_markdown(&links).unwrap();
        assert!(md.contains("%3Ctag%3E"));
        assert_eq!(serialize_txt(&links).unwrap().lines().count(), 2);
        let forged = [LinkFragment {
            url: "javascript:alert(1)".into(),
        }];
        assert_eq!(serialize_bookmarks_html(&forged), Err("invalid_link"));
        assert_eq!(serialize_internet_shortcuts(&forged), Err("invalid_link"));
    }
}
