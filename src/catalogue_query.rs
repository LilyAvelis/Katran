//! SQL shelf projection. Visibility, identity classification, search and cursor
//! are resolved before LIMIT; only the returned page is decoded into events.
use crate::{ApiError, Event, Result, interaction::ShelfTab, storage};
use rusqlite::{Connection, params};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct OrderedCursor {
    version: u8,
    tab: ShelfTab,
    query: String,
    sort: crate::sorting::SortSpec,
    generation: String,
    missing: bool,
    key: CursorKey,
    name: String,
    id: i64,
}

#[derive(serde::Deserialize, serde::Serialize)]
enum CursorKey {
    Integer(i64),
    Text(String),
    Blob(Vec<u8>),
}
impl CursorKey {
    fn value(&self) -> rusqlite::types::Value {
        match self {
            Self::Integer(v) => (*v).into(),
            Self::Text(v) => v.clone().into(),
            Self::Blob(v) => v.clone().into(),
        }
    }
}

/// The SQL projection filters the entire source before keyset ordering/LIMIT.
/// Only page rows are decoded; original paths are never accessed here.
#[allow(clippy::too_many_arguments)]
pub(crate) fn query_sorted_slice(
    db: &Connection,
    tab: ShelfTab,
    needle: &str,
    limit: u32,
    cursor: Option<&str>,
    sort: crate::sorting::SortSpec,
    pinned: bool,
    tag: Option<&str>,
    tag_slot: Option<u8>,
    view_generation: u64,
    features: (bool, bool, bool, bool),
) -> Result<(Vec<Event>, Option<String>)> {
    if db.is_autocommit() {
        let transaction = db.unchecked_transaction()?;
        let result = query_sorted_slice_inner(
            &transaction,
            tab,
            needle,
            limit,
            cursor,
            sort,
            pinned,
            tag,
            tag_slot,
            view_generation,
            features,
        );
        transaction.commit()?;
        return result;
    }
    query_sorted_slice_inner(
        db,
        tab,
        needle,
        limit,
        cursor,
        sort,
        pinned,
        tag,
        tag_slot,
        view_generation,
        features,
    )
}

#[allow(clippy::too_many_arguments)]
fn query_sorted_slice_inner(
    db: &Connection,
    tab: ShelfTab,
    needle: &str,
    limit: u32,
    cursor: Option<&str>,
    sort: crate::sorting::SortSpec,
    pinned: bool,
    tag: Option<&str>,
    tag_slot: Option<u8>,
    view_generation: u64,
    features: (bool, bool, bool, bool),
) -> Result<(Vec<Event>, Option<String>)> {
    let (images_enabled, text_enabled, links_enabled, tables_enabled) = features;
    use crate::sorting::SortMode;
    if !(1..=1000).contains(&limit) {
        return Err(ApiError::new("invalid_argument", "Expected limit 1..1000"));
    }
    let sort = sort.normalized();
    let sources: String = db.query_row("SELECT COALESCE(group_concat(source_id||':'||generation||':'||enabled,';'),'') FROM (SELECT * FROM source_configurations ORDER BY source_id)", [], |r| r.get(0))?;
    let pinned_generation = if sort.mode == SortMode::Pinned {
        crate::pinned_order::generation(db)?
    } else {
        0
    };
    let tag_generation = if sort.mode == SortMode::Tags {
        tag_sort_generation(db, tab)?
    } else {
        String::new()
    };
    let generation = format!(
        "{sources}:{images_enabled}:{text_enabled}:{links_enabled}:{tables_enabled}:{pinned}:{tag:?}:{tag_slot:?}:{view_generation}:{pinned_generation}:{tag_generation}"
    );
    let after = cursor
        .map(|value| {
            if value.len() > 131072 {
                return Err(ApiError::new("invalid_cursor", "Cursor too large"));
            }
            let value: OrderedCursor =
                serde_json::from_str(value).map_err(|e| ApiError::new("invalid_cursor", e))?;
            if value.version != 1
                || value.tab != tab
                || value.query != needle
                || value.sort != sort
                || value.generation != generation
                || value.id <= 0
            {
                return Err(ApiError::new(
                    "stale_cursor",
                    "Sort, filter or source changed; restart browsing",
                ));
            }
            Ok(value)
        })
        .transpose()?;
    let ordinary = matches!(
        tab,
        ShelfTab::All
            | ShelfTab::Images
            | ShelfTab::Text
            | ShelfTab::Links
            | ShelfTab::Tables
            | ShelfTab::Pods
            | ShelfTab::Files
            | ShelfTab::Translate
    );
    let source = match tab {
        ShelfTab::Screenshots => "sharex",
        ShelfTab::Downloads => "downloads",
        _ => "",
    };
    if sort.mode == SortMode::Arrival {
        // Preserve the indexed UNION merge path for the default large-history
        // view. No outer computed-key sort/materialization is needed for IDs.
        let mut sql = slice_projection(
            pinned,
            tag,
            tag_slot,
            tab,
            projection_sql(
                tab,
                images_enabled,
                text_enabled,
                links_enabled,
                tables_enabled,
            ),
        );
        if sort.reverse {
            sql = sql
                .replace("e.id <= ?4", "e.id >= ?4")
                .replace("p.event_id <= ?4", "p.event_id >= ?4")
                .replace("ORDER BY id DESC", "ORDER BY id ASC");
        }
        let bound = after
            .as_ref()
            .map_or(if sort.reverse { 0 } else { i64::MAX }, |c| {
                if sort.reverse {
                    c.id.saturating_add(1)
                } else {
                    c.id - 1
                }
            });
        let mut statement = db.prepare_cached(&sql)?;
        let mut events = statement
            .query_map(
                params![
                    ordinary,
                    tab == ShelfTab::Images,
                    source,
                    bound,
                    needle,
                    limit + 1
                ],
                |r| storage::event_row_with_availability(r, r.get(10)?),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let more = events.len() > limit as usize;
        events.truncate(limit as usize);
        let next = if more {
            events
                .last()
                .map(|event| {
                    serde_json::to_string(&OrderedCursor {
                        version: 1,
                        tab,
                        query: needle.into(),
                        sort,
                        generation,
                        missing: false,
                        key: CursorKey::Integer(event.id),
                        name: crate::sorting::name_key(&event.name),
                        id: event.id,
                    })
                })
                .transpose()
                .map_err(|e| ApiError::new("invalid_cursor", e))?
        } else {
            None
        };
        return Ok((events, next));
    }
    let projection = slice_projection(
        pinned,
        tag,
        tag_slot,
        tab,
        projection_sql(
            tab,
            images_enabled,
            text_enabled,
            links_enabled,
            tables_enabled,
        ),
    );
    let projection = projection
        .trim()
        .strip_suffix("ORDER BY id DESC LIMIT ?6")
        .expect("known projection tail");
    let key = match sort.mode {
        SortMode::Arrival => "p.id",
        SortMode::Tags => "COALESCE(a.slot_number,10)",
        SortMode::Pinned => "CASE WHEN po.event_id IS NOT NULL THEN po.position ELSE -p.id END",
        SortMode::Name => "katran_name_key(p.name)",
        SortMode::Extension => "katran_extension_key(p.name)",
        SortMode::Natural => "katran_natural_key(p.name)",
        SortMode::Modified => "m.modified_at_ms",
        SortMode::Size => "p.size",
        SortMode::Shuffle => "katran_shuffle_key(p.id,?7)",
        SortMode::Origin => {
            "katran_origin_key(CASE WHEN json_valid(p.origin_json) THEN json_extract(p.origin_json,'$.source_domain') END,CASE WHEN json_valid(p.origin_json) THEN json_extract(p.origin_json,'$.source_app') END,p.source)"
        }
    };
    let descending = (sort.mode == SortMode::Arrival) != sort.reverse;
    let relation = if descending { "<" } else { ">" };
    // Missing primary values always last; COALESCE supplies only a cursor value,
    // never a fabricated modification date/provenance.
    let (rank_join, missing) = if sort.mode == SortMode::Pinned {
        (
            "LEFT JOIN katran_pinned_order po ON po.event_id=p.id",
            "po.event_id IS NULL".to_owned(),
        )
    } else if sort.mode == SortMode::Tags {
        let scope = crate::tag_service::scope(tab)?;
        (
            match scope {
                "images" => {
                    "LEFT JOIN tag_scoped_assignments a ON a.event_id=p.id AND a.scope='images'"
                }
                "screenshots" => {
                    "LEFT JOIN tag_scoped_assignments a ON a.event_id=p.id AND a.scope='screenshots' AND EXISTS(SELECT 1 FROM katran_properties tp WHERE tp.event_id=p.id AND tp.pinned=1)"
                }
                "downloads" => {
                    "LEFT JOIN tag_scoped_assignments a ON a.event_id=p.id AND a.scope='downloads' AND EXISTS(SELECT 1 FROM katran_properties tp WHERE tp.event_id=p.id AND tp.pinned=1)"
                }
                "text" => {
                    "LEFT JOIN tag_scoped_assignments a ON a.event_id=p.id AND a.scope='text' AND EXISTS(SELECT 1 FROM katran_properties tp WHERE tp.event_id=p.id AND tp.pinned=1)"
                }
                "links" => {
                    "LEFT JOIN tag_scoped_assignments a ON a.event_id=p.id AND a.scope='links' AND EXISTS(SELECT 1 FROM katran_properties tp WHERE tp.event_id=p.id AND tp.pinned=1)"
                }
                "tables" => {
                    "LEFT JOIN tag_scoped_assignments a ON a.event_id=p.id AND a.scope='tables' AND EXISTS(SELECT 1 FROM katran_properties tp WHERE tp.event_id=p.id AND tp.pinned=1)"
                }
                "pods" => {
                    "LEFT JOIN tag_scoped_assignments a ON a.event_id=p.id AND a.scope='pods' AND EXISTS(SELECT 1 FROM katran_properties tp WHERE tp.event_id=p.id AND tp.pinned=1)"
                }
                "files" => {
                    "LEFT JOIN tag_scoped_assignments a ON a.event_id=p.id AND a.scope='files' AND EXISTS(SELECT 1 FROM katran_properties tp WHERE tp.event_id=p.id AND tp.pinned=1)"
                }
                "translate" => {
                    "LEFT JOIN tag_scoped_assignments a ON a.event_id=p.id AND a.scope='translate' AND EXISTS(SELECT 1 FROM katran_properties tp WHERE tp.event_id=p.id AND tp.pinned=1)"
                }
                _ => {
                    return Err(ApiError::new(
                        "folder_has_no_tags",
                        "Folder has no tag slots",
                    ));
                }
            },
            "0".to_owned(),
        )
    } else {
        ("", format!("{key} IS NULL"))
    };
    let tag_sort = sort.mode == SortMode::Tags;
    let (cursor_order, order_by) = if tag_sort {
        (
            "sort_key>?10 OR (sort_key=?10 AND id<?12)".to_owned(),
            "missing ASC,sort_key ASC,id DESC".to_owned(),
        )
    } else {
        (
            format!(
                "sort_key {relation} ?10 OR (sort_key=?10 AND (name_key {relation} ?11 OR (name_key=?11 AND id {relation} ?12)))"
            ),
            if descending {
                "missing ASC,sort_key DESC,name_key DESC,id DESC".to_owned()
            } else {
                "missing ASC,sort_key ASC,name_key ASC,id ASC".to_owned()
            },
        )
    };
    let sql = format!(
        "WITH projection AS ({projection}), keyed AS (SELECT p.*,({missing}) AS missing,COALESCE({key},0) AS sort_key,katran_name_key(p.name) AS name_key FROM projection p JOIN events m ON m.id=p.id {rank_join}) SELECT * FROM keyed WHERE ?8=0 OR missing>?9 OR (missing=?9 AND ({cursor_order})) ORDER BY {order_by} LIMIT ?6"
    );
    let mut statement = db.prepare_cached(&sql)?;
    let mut rows = statement.query(params![
        ordinary,
        tab == ShelfTab::Images,
        source,
        i64::MAX,
        needle,
        limit + 1,
        sort.seed as i64,
        after.is_some(),
        after.as_ref().is_some_and(|c| c.missing),
        after.as_ref().map_or(0.into(), |c| c.key.value()),
        after.as_ref().map_or("", |c| c.name.as_str()),
        after.as_ref().map_or(0, |c| c.id)
    ])?;
    let mut events = Vec::new();
    let mut last = None;
    let mut more = false;
    while let Some(row) = rows.next()? {
        if events.len() == limit as usize {
            more = true;
            break;
        }
        let event = storage::event_row_with_availability(row, row.get(10)?)?;
        let key = match row.get_ref("sort_key")? {
            rusqlite::types::ValueRef::Integer(v) => CursorKey::Integer(v),
            rusqlite::types::ValueRef::Text(v) => {
                CursorKey::Text(String::from_utf8_lossy(v).into_owned())
            }
            rusqlite::types::ValueRef::Blob(v) => CursorKey::Blob(v.to_vec()),
            _ => {
                return Err(ApiError::new(
                    "invalid_sort_key",
                    "Unsupported catalogue key",
                ));
            }
        };
        last = Some(OrderedCursor {
            version: 1,
            tab,
            query: needle.into(),
            sort,
            generation: generation.clone(),
            missing: row.get("missing")?,
            key,
            name: row.get("name_key")?,
            id: event.id,
        });
        events.push(event);
    }
    let next = if more {
        last.map(|c| serde_json::to_string(&c))
            .transpose()
            .map_err(|e| ApiError::new("invalid_cursor", e))?
    } else {
        None
    };
    Ok((events, next))
}

/// Bind a Tags cursor to both the active definitions/reorder revision and the
/// complete assignment map. This detects updates made by another Service
/// sharing the profile database, even when this Service's view generation is
/// unchanged.
fn tag_sort_generation(db: &Connection, tab: ShelfTab) -> Result<String> {
    use sha2::{Digest, Sha256};
    let scope = crate::tag_service::scope(tab)?;
    let slots = crate::tag_slots::list_slots(db, scope)
        .map_err(|error| ApiError::new("storage_error", error))?;
    let revision = crate::tag_slots::revision(db, scope, &slots)
        .map_err(|error| ApiError::new("storage_error", error))?;
    let mut digest = Sha256::new();
    digest.update(revision.as_bytes());
    let mut statement = db.prepare(
        "SELECT event_id,slot_number FROM tag_scoped_assignments WHERE scope=?1 ORDER BY event_id",
    )?;
    let mut rows = statement.query([scope])?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let slot: u8 = row.get(1)?;
        digest.update(id.to_be_bytes());
        digest.update([slot]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
pub(crate) fn query(
    db: &Connection,
    tab: ShelfTab,
    needle: &str,
    limit: u32,
    after: Option<i64>,
) -> Result<Vec<Event>> {
    query_features(db, tab, needle, limit, after, (true, true, true, true))
}

pub(crate) fn query_features(
    db: &Connection,
    tab: ShelfTab,
    needle: &str,
    limit: u32,
    after: Option<i64>,
    features: (bool, bool, bool, bool),
) -> Result<Vec<Event>> {
    let (images_enabled, text_enabled, links_enabled, tables_enabled) = features;
    if !(1..=1000).contains(&limit) || after.is_some_and(|id| id <= 0) {
        return Err(ApiError::new(
            "invalid_argument",
            "Expected limit 1..1000 and positive cursor",
        ));
    }
    let ordinary = matches!(
        tab,
        ShelfTab::All
            | ShelfTab::Images
            | ShelfTab::Text
            | ShelfTab::Links
            | ShelfTab::Tables
            | ShelfTab::Pods
            | ShelfTab::Files
            | ShelfTab::Translate
    );
    let source = match tab {
        ShelfTab::Screenshots => "sharex",
        ShelfTab::Downloads => "downloads",
        _ => "",
    };
    let sql = projection_sql(
        tab,
        images_enabled,
        text_enabled,
        links_enabled,
        tables_enabled,
    );
    let mut statement = db.prepare_cached(&sql)?;
    Ok(statement
        .query_map(
            params![
                ordinary,
                tab == ShelfTab::Images,
                source,
                after.map_or(i64::MAX, |id| id - 1),
                needle,
                limit
            ],
            |row| storage::event_row_with_availability(row, row.get(10)?),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

fn search_predicate(app: &str, additional: &str) -> String {
    format!(
        "(?5='' OR EXISTS(SELECT 1 FROM text_fragments t JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE t.event_id=e.id AND katran_contains(c.content,?5)) OR EXISTS(SELECT 1 FROM link_fragments l JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE l.event_id=e.id AND katran_contains(c.content,?5)) OR EXISTS(SELECT 1 FROM table_fragments t JOIN table_contents c ON c.blob_hash=e.blob_hash WHERE t.event_id=e.id AND katran_contains(c.content,?5)) OR katran_contains(e.name,?5) OR katran_contains(e.source,?5) OR katran_contains({app},?5) OR katran_contains(CASE WHEN json_valid(e.origin_json) THEN COALESCE(json_extract(e.origin_json,'$.source_domain'),'') ELSE '' END,?5) {additional})"
    )
}

pub(crate) const SCREENSHOT_SOURCES: &str = "e.source IN ('sharex','snipping_tool')";
pub(crate) const NON_FOLDER_IMAGE_SOURCES: &str =
    "e.source NOT IN ('sharex','snipping_tool','downloads')";

pub(crate) fn visible_clipboard_images() -> String {
    format!(
        r#"NOT ({image} AND e.source IN ('clipboard','clipboard_image','clipboard_file')
                  AND lower(CASE WHEN json_valid(e.origin_json) THEN COALESCE(json_extract(e.origin_json,'$.source_app'),'') ELSE '' END)='sharex')
              AND NOT EXISTS (
                  SELECT 1 FROM verified_screenshot_matches m
                  JOIN source_configurations c ON c.source_id='sharex' AND c.enabled=1 AND c.generation=m.source_generation
                  WHERE m.clipboard_id=e.id AND m.clipboard_revision=
                    CASE WHEN e.blob_hash IS NOT NULL THEN 'blob:'||e.blob_hash
                         WHEN e.reference_hash IS NOT NULL THEN 'ref:'||e.reference_hash ELSE '' END
              )
              AND NOT EXISTS (
                  SELECT 1 FROM classification_work w
                  WHERE w.event_id=e.id AND w.kind='clipboard' AND w.state='done'
                    AND w.revision=CASE WHEN e.blob_hash IS NOT NULL THEN 'blob:'||e.blob_hash WHEN e.reference_hash IS NOT NULL THEN 'ref:'||e.reference_hash ELSE '' END
                    AND (EXISTS (
                        SELECT 1 FROM verified_source_digests d
                        JOIN source_configurations c ON c.source_id='sharex' AND c.enabled=1 AND c.generation=d.source_generation
                        WHERE d.digest=w.digest
                    ) OR {snipping_match})
              )"#,
        image = image_shelf_predicate(),
        snipping_match = crate::classification_runtime::snipping_match("w.digest"),
    )
}

fn slice_projection(
    pinned: bool,
    tag: Option<&str>,
    tag_slot: Option<u8>,
    tab: ShelfTab,
    sql: String,
) -> String {
    if !pinned && tag_slot.is_none() {
        return sql;
    }
    let category = match tab {
        ShelfTab::All => "0".to_owned(),
        ShelfTab::Screenshots => SCREENSHOT_SOURCES.into(),
        ShelfTab::Downloads => "e.source='downloads'".into(),
        ShelfTab::Text => text_shelf_predicate().into(),
        ShelfTab::Pages => "0".into(),
        ShelfTab::Pods => pods_shelf_predicate().into(),
        ShelfTab::Files => files_shelf_predicate().into(),
        ShelfTab::Desktop => "0".into(),
        ShelfTab::Translate => translated_image_predicate(),
        ShelfTab::Links => link_shelf_predicate().into(),
        ShelfTab::Tables => "EXISTS(SELECT 1 FROM table_fragments f WHERE f.event_id=e.id)".into(),
        ShelfTab::Images => format!(
            "{} AND NOT {TRANSLATION_MEMBERSHIP} AND {NON_FOLDER_IMAGE_SOURCES} AND NOT EXISTS(SELECT 1 FROM files_pointers f WHERE f.event_id=e.id) AND NOT EXISTS(SELECT 1 FROM text_fragments f WHERE f.event_id=e.id) AND NOT EXISTS(SELECT 1 FROM link_fragments f WHERE f.event_id=e.id) AND NOT EXISTS(SELECT 1 FROM table_fragments f WHERE f.event_id=e.id) AND {}",
            image_shelf_predicate(),
            visible_clipboard_images()
        ),
    };
    // Literal quoting is restricted to this optional tag value; all search and
    // cursor values remain bound parameters in the shared SQL pipeline.
    let tag_filter = tag
        .map(|t| {
            format!(
                "AND EXISTS(SELECT 1 FROM katran_tags t WHERE t.event_id=e.id AND t.tag_key='{}')",
                t.replace('\'', "''")
            )
        })
        .unwrap_or_default();
    let scope = crate::tag_service::scope(tab).unwrap_or("");
    let slot_filter = tag_slot
        .map(|number| format!("AND EXISTS(SELECT 1 FROM tag_scoped_assignments a WHERE a.event_id=e.id AND a.scope='{scope}' AND a.slot_number={number})"))
        .unwrap_or_default();
    let from = if !pinned && tab == ShelfTab::Images {
        "events e LEFT JOIN katran_properties k ON k.event_id=e.id"
    } else {
        "katran_properties k JOIN events e ON e.id=k.event_id AND k.pinned=1"
    };
    format!("SELECT e.id,e.detected_at_ms,e.source,e.name,e.path,e.size,e.blob_hash,e.origin_json,e.reference_hash,e.delete_policy_json,
      CASE WHEN json_extract(e.delete_policy_json,'$.kind')='live_original' THEN EXISTS(SELECT 1 FROM source_projection p JOIN source_configurations c ON c.source_id=p.source_id AND c.generation=p.generation AND c.enabled=1 WHERE p.event_id=e.id AND p.available=1) ELSE NULL END AS projected_available,
      COALESCE((SELECT preview FROM text_fragments f WHERE f.event_id=e.id),(SELECT preview FROM link_fragments f WHERE f.event_id=e.id),(SELECT preview FROM table_fragments f WHERE f.event_id=e.id)) AS text_preview,
      EXISTS(SELECT 1 FROM link_fragments f WHERE f.event_id=e.id) AS is_link,EXISTS(SELECT 1 FROM table_fragments f WHERE f.event_id=e.id) AS is_table,e.modified_at_ms
      FROM {from} WHERE {category} AND NOT EXISTS(SELECT 1 FROM bull_members bm WHERE bm.member_id=e.id) AND NOT EXISTS(SELECT 1 FROM page_moves pm WHERE pm.event_id=e.id) AND e.id <= ?4 {tag_filter} {slot_filter}
      AND (?1 OR ?2 OR ?3='' OR ?3!='') AND {} ORDER BY id DESC LIMIT ?6", search_predicate("CASE WHEN json_valid(e.origin_json) THEN COALESCE(json_extract(e.origin_json,'$.source_app'),'') ELSE '' END", "OR katran_contains(COALESCE(k.display_name,''),?5)"))
}

fn projection_sql(
    tab: ShelfTab,
    images_enabled: bool,
    text_enabled: bool,
    links_enabled: bool,
    tables_enabled: bool,
) -> String {
    let mut sql = sql();
    if !images_enabled && !matches!(tab, ShelfTab::Translate | ShelfTab::Files) {
        sql = sql.replace(
            "AND e.legacy_source_ref=0",
            &format!(
                "AND e.legacy_source_ref=0 AND NOT {}",
                image_shelf_predicate()
            ),
        );
    }
    if tab == ShelfTab::Text {
        sql = sql.replace(
            "AND e.legacy_source_ref=0",
            &format!("AND e.legacy_source_ref=0 AND {}", text_shelf_predicate()),
        );
        sql = sql.replace("WHERE ?2=0", "WHERE 0 AND ?2=0");
    }
    if tab == ShelfTab::Pods {
        sql = sql.replace(
            "AND e.legacy_source_ref=0",
            &format!("AND e.legacy_source_ref=0 AND {}", pods_shelf_predicate()),
        );
        sql = sql.replace("WHERE ?2=0", "WHERE 0 AND ?2=0");
    }
    if tab == ShelfTab::Files {
        sql = sql.replace(
            "AND e.legacy_source_ref=0",
            &format!("AND e.legacy_source_ref=0 AND {}", files_shelf_predicate()),
        );
        sql = sql.replace("WHERE ?2=0", "WHERE 0 AND ?2=0");
    } else {
        sql = sql.replace(
            "AND e.legacy_source_ref=0",
            &format!(
                "AND e.legacy_source_ref=0 AND NOT {}",
                files_shelf_predicate()
            ),
        );
    }
    if tab == ShelfTab::Translate {
        sql = sql.replace(
            "AND e.legacy_source_ref=0",
            &format!(
                "AND e.legacy_source_ref=0 AND {}",
                translated_image_predicate()
            ),
        );
        sql = sql.replace("WHERE ?2=0", "WHERE 0 AND ?2=0");
    } else if tab == ShelfTab::Images {
        sql = sql.replace(
            "AND e.legacy_source_ref=0",
            &format!("AND e.legacy_source_ref=0 AND NOT {TRANSLATION_MEMBERSHIP}"),
        );
    }
    if tab == ShelfTab::Links {
        sql = sql.replace(
            "AND e.legacy_source_ref=0",
            &format!("AND e.legacy_source_ref=0 AND {}", link_shelf_predicate()),
        );
        sql = sql.replace("WHERE ?2=0", "WHERE 0 AND ?2=0");
    }
    if !links_enabled {
        sql = sql.replace(
            "AND e.legacy_source_ref=0",
            &format!(
                "AND e.legacy_source_ref=0 AND NOT {}",
                link_shelf_predicate()
            ),
        );
    }
    if tab == ShelfTab::Tables {
        sql=sql.replace("AND e.legacy_source_ref=0", "AND e.legacy_source_ref=0 AND EXISTS(SELECT 1 FROM table_fragments t WHERE t.event_id=e.id)");
        sql = sql.replace("WHERE ?2=0", "WHERE 0 AND ?2=0");
    }
    if !tables_enabled {
        sql=sql.replace("AND e.legacy_source_ref=0","AND e.legacy_source_ref=0 AND NOT EXISTS(SELECT 1 FROM table_fragments t WHERE t.event_id=e.id)");
    }
    if !text_enabled && tab != ShelfTab::Pods {
        sql=sql.replace("AND e.legacy_source_ref=0", &format!("AND e.legacy_source_ref=0 AND NOT EXISTS(SELECT 1 FROM text_fragments t WHERE t.event_id=e.id) AND NOT {}", text_shelf_predicate()));
    }
    sql
}

pub(crate) const TRANSLATION_MEMBERSHIP: &str =
    "EXISTS(SELECT 1 FROM translated_images t WHERE t.event_id=e.id)";

pub(crate) fn link_shelf_predicate() -> &'static str {
    "(EXISTS(SELECT 1 FROM link_fragments f WHERE f.event_id=e.id) OR (e.blob_hash IS NOT NULL AND e.source='link_collection' AND lower(substr(e.name,-5))='.bull'))"
}

pub(crate) fn files_shelf_predicate() -> &'static str {
    "(EXISTS(SELECT 1 FROM files_pointers f WHERE f.event_id=e.id) OR (e.blob_hash IS NOT NULL AND (e.source='bs_extracted_file' OR (e.source='bs_collection' AND lower(substr(e.name,-3))='.bs') OR (e.source IN ('file_bull_collection','file_image_collection') AND lower(substr(e.name,-5))='.bull'))))"
}

pub(crate) fn text_shelf_predicate() -> &'static str {
    "(EXISTS(SELECT 1 FROM text_fragments f JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE f.event_id=e.id AND (e.source='notion_reader' OR katran_is_pod(c.content)=0)) OR (e.blob_hash IS NOT NULL AND e.source='text_collection' AND lower(substr(e.name,-5))='.bull'))"
}

pub(crate) fn pods_shelf_predicate() -> &'static str {
    "(e.source<>'notion_reader' AND EXISTS(SELECT 1 FROM text_fragments f JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE f.event_id=e.id AND katran_is_pod(c.content)=1))"
}

/// Ordinary image collections share the Images shelf, but remain managed blobs.
pub(crate) fn image_shelf_predicate() -> String {
    format!(
        "({} OR (e.blob_hash IS NOT NULL AND e.source='image_collection' AND lower(substr(e.name,-5))='.bull'))",
        crate::file_types::sql_image_predicate()
    )
}

pub(crate) fn translated_image_predicate() -> String {
    format!(
        "e.blob_hash IS NOT NULL AND ({} OR (e.source='image_translation_collection' AND substr(e.name,-5)='.bull')) AND {TRANSLATION_MEMBERSHIP}",
        crate::file_types::sql_image_predicate()
    )
}

// Keep ORDER BY on the compound statement: wrapping this UNION in a SELECT
// causes SQLite to materialize and sort the entire history before LIMIT.
fn sql() -> String {
    // Separate source branches bind all leading browse-index columns. CROSS
    // JOIN keeps projection first; an events-first plan scans ordinary history
    // just to discover that the live source has no rows.
    let live = |source: crate::source_policy::SourceId, shelf: &str, additional: &str| {
        let app = source.display_name();
        let source = source.as_str();
        format!(
            r#"
        SELECT p.event_id AS id,e.detected_at_ms,e.source,e.name,e.path,e.size,e.blob_hash,
          json_set(CASE WHEN json_valid(e.origin_json) THEN e.origin_json ELSE '{{}}' END,'$.source_app','{app}'),
          e.reference_hash,e.delete_policy_json,p.available AS projected_available,NULL AS text_preview,0 AS is_link,0 AS is_table,e.modified_at_ms
        FROM source_projection p CROSS JOIN events e ON e.id=p.event_id
        WHERE ?2=0 AND (?3='' OR ?3='{shelf}') AND p.source_id='{source}' AND p.available=1
          AND NOT EXISTS(SELECT 1 FROM bull_members bm WHERE bm.member_id=e.id) AND NOT EXISTS(SELECT 1 FROM page_moves pm WHERE pm.event_id=e.id)
          AND p.generation=(SELECT generation FROM source_configurations WHERE source_id='{source}' AND enabled=1)
          AND p.event_id <= ?4
          AND json_extract(e.delete_policy_json,'$.kind')='live_original'
          AND json_extract(e.delete_policy_json,'$.source_id')=p.source_id
          AND json_extract(e.delete_policy_json,'$.generation')=p.generation
          AND {search}
          {additional}
    "#,
            search = search_predicate(&format!("'{app}'"), "")
        )
    };
    format!(
        r#"
            SELECT e.id,e.detected_at_ms,e.source,e.name,e.path,e.size,e.blob_hash,e.origin_json,e.reference_hash,e.delete_policy_json,
                   NULL AS projected_available,COALESCE((SELECT preview FROM text_fragments t WHERE t.event_id=e.id),(SELECT preview FROM link_fragments l WHERE l.event_id=e.id),(SELECT preview FROM table_fragments t WHERE t.event_id=e.id)) AS text_preview, EXISTS(SELECT 1 FROM link_fragments l WHERE l.event_id=e.id) AS is_link,EXISTS(SELECT 1 FROM table_fragments t WHERE t.event_id=e.id) AS is_table,e.modified_at_ms
            FROM events e
            WHERE ?1 AND e.id <= ?4 AND e.legacy_source_ref=0
              AND NOT EXISTS(SELECT 1 FROM bull_members bm WHERE bm.member_id=e.id) AND NOT EXISTS(SELECT 1 FROM page_moves pm WHERE pm.event_id=e.id)
              AND {ordinary_search}
              AND json_extract(e.delete_policy_json,'$.kind')='shelf_only'
              AND (?2=0 OR {image})
              AND {clipboards}
            UNION ALL
            {sharex}
            UNION ALL
            {snipping_tool}
            UNION ALL
            {downloads}
        ORDER BY id DESC LIMIT ?6
    "#,
        image = image_shelf_predicate(),
        ordinary_search = search_predicate(
            "CASE WHEN json_valid(e.origin_json) THEN COALESCE(json_extract(e.origin_json,'$.source_app'),'') ELSE '' END",
            "",
        ),
        clipboards = visible_clipboard_images(),
        sharex = live(crate::source_policy::SourceId::Sharex, "sharex", ""),
        snipping_tool = live(
            crate::source_policy::SourceId::SnippingTool,
            "sharex",
            "AND NOT EXISTS (SELECT 1 FROM source_projection x JOIN source_configurations c
                ON c.source_id=x.source_id AND c.generation=x.generation AND c.enabled=1
                WHERE x.source_id='sharex' AND x.path_key=p.path_key AND x.available=1)"
        ),
        downloads = live(crate::source_policy::SourceId::Downloads, "downloads", "")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Service;
    use rusqlite::StatementStatus;

    fn fixture() -> (tempfile::TempDir, Service) {
        let root = tempfile::tempdir().unwrap();
        let service = Service::open(root.path(), crate::DEFAULT_QUOTA).unwrap();
        (root, service)
    }
    fn event(
        db: &Connection,
        name: &str,
        source: &str,
        policy: serde_json::Value,
        legacy: bool,
        app: Option<&str>,
    ) -> i64 {
        db.execute("INSERT INTO events(detected_at_ms,source,name,path,size,origin_json,reference_hash,delete_policy_json,legacy_source_ref) VALUES(0,?1,?2,?3,1,?4,'revision',?5,?6)",params![source,name,format!("missing-synthetic/{name}"),serde_json::json!({"source_app":app}).to_string(),policy.to_string(),legacy]).unwrap();
        db.last_insert_rowid()
    }
    fn ordinary(db: &Connection, name: &str, source: &str) -> i64 {
        event(
            db,
            name,
            source,
            serde_json::json!({"kind":"shelf_only"}),
            false,
            None,
        )
    }
    fn live(db: &Connection, name: &str, source: &str, generation: i64, available: bool) -> i64 {
        let id = event(
            db,
            name,
            source,
            serde_json::json!({"kind":"live_original","source_id":source,"generation":generation,"mode":if source=="sharex" {"permanent"} else {"recycle"}}),
            false,
            None,
        );
        db.execute(
            "INSERT INTO source_projection VALUES(?1,?2,?3,?4,?5)",
            params![source, generation, name, id, available],
        )
        .unwrap();
        id
    }
    fn ids(
        db: &Connection,
        tab: ShelfTab,
        query_text: &str,
        limit: u32,
        after: Option<i64>,
    ) -> Vec<i64> {
        query(db, tab, query_text, limit, after)
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect()
    }
    #[test]
    fn translate_shows_only_published_managed_images() {
        let (_dir, mut service) = fixture();
        let source = ordinary(&service.db, "plain.png", "image_translation");
        let result = crate::storage::ingest_bytes(
            &mut service.db,
            &service.root,
            service.quota,
            crate::storage::ByteIngest {
                name: "result.png",
                source: "fixture",
                bytes: b"synthetic managed image",
                origin: &crate::origin::Origin::default(),
            },
        )
        .unwrap();
        assert!(ids(&service.db, ShelfTab::Translate, "", 10, None).is_empty());
        service
            .db
            .execute(
                "INSERT INTO translated_images VALUES (?1), (?2)",
                [source, result],
            )
            .unwrap();
        assert_eq!(
            ids(&service.db, ShelfTab::Translate, "", 10, None),
            vec![result]
        );
        assert!(ids(&service.db, ShelfTab::Translate, "unmatched", 10, None).is_empty());
    }
    #[test]
    fn collection_members_are_hidden_before_paging_and_remain_addressable() {
        let (_dir, service) = fixture();
        let db = &service.db;
        let visible = ordinary(db, "visible.png", "fixture");
        let member = ordinary(db, "hidden.png", "fixture");
        let collection = ordinary(db, "album.bull", "fixture");
        db.execute(
            "INSERT INTO bull_members(collection_id,member_id,position) VALUES(?1,?2,0)",
            params![collection, member],
        )
        .unwrap();
        db.execute(
            "INSERT INTO katran_properties(event_id,pinned) VALUES(?1,1)",
            [member],
        )
        .unwrap();
        db.execute(
            "INSERT INTO katran_tags(event_id,tag_key,tag_display) VALUES(?1,'hidden','hidden')",
            [member],
        )
        .unwrap();
        assert_eq!(ids(db, ShelfTab::Images, "", 1, None), vec![visible]);
        assert_eq!(service.get(member).unwrap().id, member);
        let (pinned, _) = query_sorted_slice(
            db,
            ShelfTab::Images,
            "",
            10,
            None,
            crate::sorting::SortSpec::default(),
            true,
            None,
            None,
            0,
            (true, true, true, true),
        )
        .unwrap();
        assert!(pinned.is_empty());
        db.execute(
            "DELETE FROM bull_members WHERE collection_id=?1",
            [collection],
        )
        .unwrap();
        assert_eq!(
            ids(db, ShelfTab::Images, "", 2, None),
            vec![member, visible]
        );
    }
    #[test]
    fn membership_is_current_projection_not_provenance_or_path_and_never_stats_live_paths() {
        let (_root, s) = fixture();
        let db = &s.db;
        db.execute_batch("INSERT INTO source_configurations VALUES('sharex',2,'synthetic-sharex',1),('downloads',1,'synthetic-downloads',1)").unwrap();
        let generic = ordinary(db, "generic.png", "sharex");
        let _legacy = event(
            db,
            "legacy.png",
            "sharex",
            serde_json::json!({"kind":"shelf_only"}),
            true,
            None,
        );
        let _old = live(db, "old.png", "sharex", 1, true);
        let screenshot = live(db, "current.png", "sharex", 2, true);
        let _missing = live(db, "unavailable.png", "sharex", 2, false);
        let download = live(db, "download.png", "downloads", 1, true);
        assert_eq!(
            ids(db, ShelfTab::All, "", 100, None),
            vec![download, screenshot, generic]
        );
        assert_eq!(ids(db, ShelfTab::Images, "", 100, None), vec![generic]);
        assert_eq!(
            ids(db, ShelfTab::Screenshots, "", 100, None),
            vec![screenshot]
        );
        assert_eq!(ids(db, ShelfTab::Downloads, "", 100, None), vec![download]);
        let page = query(db, ShelfTab::Screenshots, "", 100, None).unwrap();
        assert!(
            page[0].available,
            "projection availability must not stat an absent live original"
        );
        assert_eq!(page[0].origin.source_app.as_deref(), Some("ShareX folder"));
        // A replacement updates one path projection; its previous arrival remains
        // addressable by ID but no longer belongs to this live view.
        let newer = event(
            db,
            "current.png",
            "sharex",
            serde_json::json!({"kind":"live_original","source_id":"sharex","generation":2,"mode":"permanent"}),
            false,
            None,
        );
        db.execute(
            "UPDATE source_projection SET event_id=?1 WHERE event_id=?2",
            params![newer, screenshot],
        )
        .unwrap();
        assert_eq!(ids(db, ShelfTab::Screenshots, "", 100, None), vec![newer]);
        db.execute(
            "UPDATE source_configurations SET generation=3 WHERE source_id='sharex'",
            [],
        )
        .unwrap();
        assert!(ids(db, ShelfTab::Screenshots, "", 100, None).is_empty());
        db.execute(
            "UPDATE source_configurations SET enabled=0 WHERE source_id='downloads'",
            [],
        )
        .unwrap();
        assert_eq!(ids(db, ShelfTab::All, "", 100, None), vec![generic]);
    }
    #[test]
    fn classification_and_literal_unicode_search_precede_limit_and_cursor() {
        let (_root, s) = fixture();
        let db = &s.db;
        db.execute_batch("INSERT INTO source_configurations VALUES('sharex',2,'synthetic',1)")
            .unwrap();
        let a = ordinary(db, "Привет%_[один].png", "clipboard_image");
        let b = ordinary(db, "Привет%_[два].png", "clipboard_image");
        let stale = ordinary(db, "Привет%_[stale].png", "clipboard_image");
        let _classified = ordinary(db, "Привет%_[matched].png", "clipboard_image");
        db.execute(
            "INSERT INTO verified_screenshot_matches VALUES(?1,'ref:revision',2)",
            [_classified],
        )
        .unwrap();
        db.execute(
            "INSERT INTO verified_screenshot_matches VALUES(?1,'ref:obsolete',2)",
            [stale],
        )
        .unwrap();
        let _known = event(
            db,
            "Привет%_[known].png",
            "clipboard_file",
            serde_json::json!({"kind":"shelf_only"}),
            false,
            Some("sHaReX"),
        );
        let _not_literal = ordinary(db, "ПриветXX[other].png", "clipboard_image");
        assert_eq!(
            ids(db, ShelfTab::Images, "ПРИВЕТ%_[", 2, None),
            vec![stale, b]
        );
        assert_eq!(ids(db, ShelfTab::Images, "ПРИВЕТ%_[", 2, Some(b)), vec![a]);
        assert_eq!(ids(db, ShelfTab::All, "ПРИВЕТ%_[", 2, None), vec![stale, b]);
        db.execute(
            "UPDATE source_configurations SET generation=3 WHERE source_id='sharex'",
            [],
        )
        .unwrap();
        assert_eq!(
            ids(db, ShelfTab::Images, "ПРИВЕТ%_[matched]", 1, None),
            vec![_classified]
        );
        assert!(query(db, ShelfTab::All, "", 0, None).is_err());
        assert!(query(db, ShelfTab::All, "", 1, Some(0)).is_err());
    }
    fn seed(db: &Connection, count: i64) {
        db.execute("WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM seq WHERE n<?1) INSERT INTO events(detected_at_ms,source,name,path,size,origin_json,reference_hash) SELECT 0,'clipboard_image','synthetic-'||n||'.png','missing-synthetic-path-'||n,1,'{}','revision' FROM seq",[count]).unwrap();
    }
    #[test]
    fn fixed_page_work_is_bounded_at_twenty_and_hundred_thousand_events() {
        let mut steps = Vec::new();
        for count in [20_000, 100_000] {
            let (_root, s) = fixture();
            seed(&s.db, count);
            let started = std::time::Instant::now();
            let events = query(&s.db, ShelfTab::All, "", 1000, None).unwrap();
            assert_eq!(events.len(), 1000);
            assert_eq!(events[0].id, count);
            let plan =
                s.db.prepare(&("EXPLAIN QUERY PLAN ".to_owned() + &sql()))
                    .unwrap()
                    .query_map(params![true, false, "", i64::MAX, "", 1000], |r| {
                        r.get::<_, String>(3)
                    })
                    .unwrap()
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .unwrap();
            assert!(
                !plan.iter().any(|line| line.contains("TEMP B-TREE")),
                "{plan:?}"
            );
            let mut stmt = s.db.prepare(&sql()).unwrap();
            let rows = stmt
                .query_map(params![true, false, "", i64::MAX, "", 1000], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(rows.len(), 1000);
            let work = stmt.get_status(StatementStatus::VmStep);
            eprintln!(
                "{count} history / 1000 page: {:?}, {work} SQL steps",
                started.elapsed()
            );
            steps.push(work);
        }
        assert!(
            steps[1] <= steps[0] + 1000,
            "fixed page SQL work grew with unrelated history: {steps:?}"
        );
        assert!(
            steps[1] < 200_000,
            "page query unexpectedly scans history: {steps:?}"
        );
    }
    #[test]
    fn mixed_live_pages_and_generations_have_bounded_sql_work() {
        let mut work = Vec::new();
        for count in [20_000, 100_000] {
            let (_root, s) = fixture();
            seed(&s.db, count);
            s.db.execute_batch("INSERT INTO source_configurations VALUES('sharex',2,'synthetic',1),('downloads',2,'synthetic',1);
                UPDATE events SET source=CASE id%3 WHEN 0 THEN 'sharex' WHEN 1 THEN 'downloads' ELSE 'clipboard_image' END;
                UPDATE events SET delete_policy_json=json_object('kind','live_original','source_id',source,'generation',CASE WHEN id<10000 THEN 1 ELSE 2 END,'mode',CASE source WHEN 'sharex' THEN 'permanent' ELSE 'recycle' END) WHERE source IN ('sharex','downloads');
                INSERT INTO source_projection SELECT source,json_extract(delete_policy_json,'$.generation'),path,id,1 FROM events WHERE source IN ('sharex','downloads');").unwrap();
            let mut per_tab = Vec::new();
            for (tab, ordinary, image, source) in [
                (ShelfTab::All, true, false, ""),
                (ShelfTab::Images, true, true, ""),
                (ShelfTab::Screenshots, false, false, "sharex"),
                (ShelfTab::Downloads, false, false, "downloads"),
            ] {
                let page = query(&s.db, tab, "", 1000, None).unwrap();
                assert_eq!(page.len(), 1000);
                assert!(page.windows(2).all(|w| w[0].id > w[1].id));
                if !source.is_empty() {
                    assert!(page.iter().all(|e| e.source == source && e.available));
                }
                let mut statement = s.db.prepare(&sql()).unwrap();
                let rows = statement
                    .query_map(params![ordinary, image, source, i64::MAX, "", 1000], |r| {
                        r.get::<_, i64>(0)
                    })
                    .unwrap()
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .unwrap();
                assert_eq!(rows, page.iter().map(|e| e.id).collect::<Vec<_>>());
                per_tab.push(statement.get_status(StatementStatus::VmStep));
                let next = query(&s.db, tab, "", 1000, Some(page.last().unwrap().id)).unwrap();
                assert!(next.iter().all(|e| e.id < page.last().unwrap().id));
            }
            eprintln!(
                "{count} mixed history: {per_tab:?} VM steps for 1000 rows per All/Images/Screenshots/Downloads"
            );
            work.push(per_tab);
        }
        for tab in 0..4 {
            assert!(work[1][tab] <= work[0][tab] + 1000, "{work:?}");
            assert!(work[1][tab] < 200_000, "{work:?}");
        }
    }
    #[test]
    fn browse_dispatcher_keeps_exact_ids_and_selection_across_pages() {
        let (_root, mut s) = fixture();
        let a = ordinary(&s.db, "alpha.png", "manual");
        let b = ordinary(&s.db, "beta.png", "manual");
        let request = |command| crate::Request {
            api_version: crate::API_VERSION,
            request_id: "synthetic".into(),
            command,
        };
        assert!(
            s.execute(request(crate::Command::SetTab {
                tab: ShelfTab::Images
            }))["ok"]
                .as_bool()
                .unwrap()
        );
        let first = s.execute(request(crate::Command::BrowseShelf {
            query: "".into(),
            limit: 1,
            after_id: None,
        }));
        assert_eq!(first["result"]["events"][0]["id"], b);
        assert_eq!(first["result"]["next_cursor"], b);
        s.execute(request(crate::Command::SetVisible { ids: vec![b] }));
        s.execute(request(crate::Command::Select {
            id: b,
            mode: crate::interaction::SelectionMode::Replace,
        }));
        let next = s.execute(request(crate::Command::BrowseShelf {
            query: "alpha".into(),
            limit: 1,
            after_id: Some(b),
        }));
        assert_eq!(next["result"]["events"][0]["id"], a);
        assert_eq!(s.interaction.selected, vec![b]);
    }
}
