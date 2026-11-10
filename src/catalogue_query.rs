//! SQL shelf projection. Visibility, identity classification, search and cursor
//! are resolved before LIMIT; only the returned page is decoded into events.
use crate::{ApiError, Event, Result, interaction::ShelfTab, storage};
use rusqlite::{Connection, params};

pub(crate) fn query(
    db: &Connection,
    tab: ShelfTab,
    needle: &str,
    limit: u32,
    after: Option<i64>,
) -> Result<Vec<Event>> {
    if !(1..=1000).contains(&limit) || after.is_some_and(|id| id <= 0) {
        return Err(ApiError::new(
            "invalid_argument",
            "Expected limit 1..1000 and positive cursor",
        ));
    }
    let ordinary = matches!(tab, ShelfTab::All | ShelfTab::Images);
    let source = match tab {
        ShelfTab::Screenshots => "sharex",
        ShelfTab::Downloads => "downloads",
        _ => "",
    };
    let mut statement = db.prepare_cached(&sql())?;
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

// Keep ORDER BY on the compound statement: wrapping this UNION in a SELECT
// causes SQLite to materialize and sort the entire history before LIMIT.
fn sql() -> String {
    let search = |app: &str| {
        format!(
            "(?5='' OR katran_contains(e.name,?5) OR katran_contains(e.source,?5) OR katran_contains({app},?5) OR katran_contains(CASE WHEN json_valid(e.origin_json) THEN COALESCE(json_extract(e.origin_json,'$.source_domain'),'') ELSE '' END,?5))"
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
          e.reference_hash,e.delete_policy_json,p.available AS projected_available
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
                   NULL AS projected_available
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
