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
    let generation = format!(
        "{sources}:{images_enabled}:{text_enabled}:{links_enabled}:{tables_enabled}:{pinned}:{tag:?}:{view_generation}"
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
    let direction = if descending { "DESC" } else { "ASC" };
    let relation = if descending { "<" } else { ">" };
    // Missing primary values always last; COALESCE supplies only a cursor value,
    // never a fabricated modification date/provenance.
    let sql = format!(
        "WITH projection AS ({projection}), keyed AS (SELECT p.*,({key} IS NULL) AS missing,COALESCE({key},0) AS sort_key,katran_name_key(p.name) AS name_key FROM projection p JOIN events m ON m.id=p.id) SELECT * FROM keyed WHERE ?8=0 OR missing>?9 OR (missing=?9 AND (sort_key {relation} ?10 OR (sort_key=?10 AND (name_key {relation} ?11 OR (name_key=?11 AND id {relation} ?12))))) ORDER BY missing ASC,sort_key {direction},name_key {direction},id {direction} LIMIT ?6"
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

fn slice_projection(pinned: bool, tag: Option<&str>, tab: ShelfTab, sql: String) -> String {
    if !pinned {
        return sql;
    }
    let category = match tab {
        ShelfTab::All => "0".to_owned(),
        ShelfTab::Screenshots => "e.source='sharex'".into(),
        ShelfTab::Downloads => "e.source='downloads'".into(),
        ShelfTab::Text => "EXISTS(SELECT 1 FROM text_fragments f JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE f.event_id=e.id AND katran_is_pod(c.content)=0)".into(),
        ShelfTab::Pods => "EXISTS(SELECT 1 FROM text_fragments f JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE f.event_id=e.id AND katran_is_pod(c.content)=1)".into(),
        ShelfTab::Links => "EXISTS(SELECT 1 FROM link_fragments f WHERE f.event_id=e.id)".into(),
        ShelfTab::Tables => "EXISTS(SELECT 1 FROM table_fragments f WHERE f.event_id=e.id)".into(),
        ShelfTab::Images => format!("{} AND e.source NOT IN ('sharex','downloads') AND NOT EXISTS(SELECT 1 FROM text_fragments f WHERE f.event_id=e.id) AND NOT EXISTS(SELECT 1 FROM link_fragments f WHERE f.event_id=e.id) AND NOT EXISTS(SELECT 1 FROM table_fragments f WHERE f.event_id=e.id)",crate::file_types::sql_image_predicate()),
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
    format!("SELECT e.id,e.detected_at_ms,e.source,e.name,e.path,e.size,e.blob_hash,e.origin_json,e.reference_hash,e.delete_policy_json,
      CASE WHEN json_extract(e.delete_policy_json,'$.kind')='live_original' THEN EXISTS(SELECT 1 FROM source_projection p JOIN source_configurations c ON c.source_id=p.source_id AND c.generation=p.generation AND c.enabled=1 WHERE p.event_id=e.id AND p.available=1) ELSE NULL END AS projected_available,
      COALESCE((SELECT preview FROM text_fragments f WHERE f.event_id=e.id),(SELECT preview FROM link_fragments f WHERE f.event_id=e.id),(SELECT preview FROM table_fragments f WHERE f.event_id=e.id)) AS text_preview,
      EXISTS(SELECT 1 FROM link_fragments f WHERE f.event_id=e.id) AS is_link,EXISTS(SELECT 1 FROM table_fragments f WHERE f.event_id=e.id) AS is_table,e.modified_at_ms
      FROM katran_properties k JOIN events e ON e.id=k.event_id WHERE k.pinned=1 AND {category} AND e.id <= ?4 {tag_filter}
      AND (?1 OR ?2 OR ?3='' OR ?3!='') AND (?5='' OR katran_contains(e.name,?5) OR katran_contains(COALESCE(k.display_name,''),?5)) ORDER BY id DESC LIMIT ?6")
}

fn projection_sql(
    tab: ShelfTab,
    images_enabled: bool,
    text_enabled: bool,
    links_enabled: bool,
    tables_enabled: bool,
) -> String {
    let mut sql = sql();
    if !images_enabled {
        sql = sql.replace(
            "AND e.legacy_source_ref=0",
            &format!(
                "AND e.legacy_source_ref=0 AND NOT {}",
                crate::file_types::sql_image_predicate()
            ),
        );
    }
    if tab == ShelfTab::Text {
        sql=sql.replace("AND e.legacy_source_ref=0", "AND e.legacy_source_ref=0 AND EXISTS(SELECT 1 FROM text_fragments t JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE t.event_id=e.id AND katran_is_pod(c.content)=0)");
        sql = sql.replace("WHERE ?2=0", "WHERE 0 AND ?2=0");
    }
    if tab == ShelfTab::Pods {
        sql=sql.replace("AND e.legacy_source_ref=0", "AND e.legacy_source_ref=0 AND EXISTS(SELECT 1 FROM text_fragments t JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE t.event_id=e.id AND katran_is_pod(c.content)=1)");
        sql = sql.replace("WHERE ?2=0", "WHERE 0 AND ?2=0");
    }
    if tab == ShelfTab::Links {
        sql=sql.replace("AND e.legacy_source_ref=0", "AND e.legacy_source_ref=0 AND EXISTS(SELECT 1 FROM link_fragments l WHERE l.event_id=e.id)");
        sql = sql.replace("WHERE ?2=0", "WHERE 0 AND ?2=0");
    }
    if !links_enabled {
        sql=sql.replace("AND e.legacy_source_ref=0", "AND e.legacy_source_ref=0 AND NOT EXISTS(SELECT 1 FROM link_fragments l WHERE l.event_id=e.id)");
    }
    if tab == ShelfTab::Tables {
        sql=sql.replace("AND e.legacy_source_ref=0", "AND e.legacy_source_ref=0 AND EXISTS(SELECT 1 FROM table_fragments t WHERE t.event_id=e.id)");
        sql = sql.replace("WHERE ?2=0", "WHERE 0 AND ?2=0");
    }
    if !tables_enabled {
        sql=sql.replace("AND e.legacy_source_ref=0","AND e.legacy_source_ref=0 AND NOT EXISTS(SELECT 1 FROM table_fragments t WHERE t.event_id=e.id)");
    }
    if !text_enabled && tab != ShelfTab::Pods {
        sql=sql.replace("AND e.legacy_source_ref=0", "AND e.legacy_source_ref=0 AND NOT EXISTS(SELECT 1 FROM text_fragments t WHERE t.event_id=e.id)");
    }
    sql
}

// Keep ORDER BY on the compound statement: wrapping this UNION in a SELECT
// causes SQLite to materialize and sort the entire history before LIMIT.
fn sql() -> String {
    let search = |app: &str| {
        format!(
            "(?5='' OR EXISTS(SELECT 1 FROM text_fragments t JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE t.event_id=e.id AND katran_contains(c.content,?5)) OR EXISTS(SELECT 1 FROM link_fragments l JOIN text_contents c ON c.blob_hash=e.blob_hash WHERE l.event_id=e.id AND katran_contains(c.content,?5)) OR EXISTS(SELECT 1 FROM table_fragments t JOIN table_contents c ON c.blob_hash=e.blob_hash WHERE t.event_id=e.id AND katran_contains(c.content,?5)) OR katran_contains(e.name,?5) OR katran_contains(e.source,?5) OR katran_contains({app},?5) OR katran_contains(CASE WHEN json_valid(e.origin_json) THEN COALESCE(json_extract(e.origin_json,'$.source_domain'),'') ELSE '' END,?5))"
        )
    };
    // Separate source branches bind all leading browse-index columns. CROSS
    // JOIN keeps projection first; an events-first plan scans ordinary history
    // just to discover that the live source has no rows.
    let live = |source: &str, app: &str| {
        format!(
            r#"
        SELECT p.event_id AS id,e.detected_at_ms,e.source,e.name,e.path,e.size,e.blob_hash,
          json_set(CASE WHEN json_valid(e.origin_json) THEN e.origin_json ELSE '{{}}' END,'$.source_app','{app}'),
          e.reference_hash,e.delete_policy_json,p.available AS projected_available,NULL AS text_preview,0 AS is_link,0 AS is_table,e.modified_at_ms
        FROM source_projection p CROSS JOIN events e ON e.id=p.event_id
        WHERE ?2=0 AND (?3='' OR ?3='{source}') AND p.source_id='{source}' AND p.available=1
          AND p.generation=(SELECT generation FROM source_configurations WHERE source_id='{source}' AND enabled=1)
          AND p.event_id <= ?4
          AND json_extract(e.delete_policy_json,'$.kind')='live_original'
          AND json_extract(e.delete_policy_json,'$.source_id')=p.source_id
          AND json_extract(e.delete_policy_json,'$.generation')=p.generation
          AND {search}
    "#,
            search = search(&format!("'{app}'"))
        )
    };
    format!(
        r#"
            SELECT e.id,e.detected_at_ms,e.source,e.name,e.path,e.size,e.blob_hash,e.origin_json,e.reference_hash,e.delete_policy_json,
                   NULL AS projected_available,COALESCE((SELECT preview FROM text_fragments t WHERE t.event_id=e.id),(SELECT preview FROM link_fragments l WHERE l.event_id=e.id),(SELECT preview FROM table_fragments t WHERE t.event_id=e.id)) AS text_preview, EXISTS(SELECT 1 FROM link_fragments l WHERE l.event_id=e.id) AS is_link,EXISTS(SELECT 1 FROM table_fragments t WHERE t.event_id=e.id) AS is_table,e.modified_at_ms
            FROM events e
            WHERE ?1 AND e.id <= ?4 AND e.legacy_source_ref=0
              AND {ordinary_search}
              AND json_extract(e.delete_policy_json,'$.kind')='shelf_only'
              AND (?2=0 OR {image})
              AND NOT ({image} AND e.source IN ('clipboard','clipboard_image','clipboard_file')
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
                  JOIN verified_source_digests d ON d.digest=w.digest
                  JOIN source_configurations c ON c.source_id='sharex' AND c.enabled=1 AND c.generation=d.source_generation
                  WHERE w.event_id=e.id AND w.kind='clipboard' AND w.state='done'
                    AND w.revision=CASE WHEN e.blob_hash IS NOT NULL THEN 'blob:'||e.blob_hash WHEN e.reference_hash IS NOT NULL THEN 'ref:'||e.reference_hash ELSE '' END
              )
            UNION ALL
            {sharex}
            UNION ALL
            {downloads}
        ORDER BY id DESC LIMIT ?6
    "#,
        image = crate::file_types::sql_image_predicate(),
        ordinary_search = search(
            "CASE WHEN json_valid(e.origin_json) THEN COALESCE(json_extract(e.origin_json,'$.source_app'),'') ELSE '' END"
        ),
        sharex = live("sharex", "ShareX folder"),
        downloads = live("downloads", "Downloads")
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
