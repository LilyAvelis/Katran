//! Persistent continuing identity queue. One bounded decoder lane; only private results commit.
use crate::*;
use std::sync::mpsc;
#[derive(Default)]
pub(crate) struct ClassificationRuntime {
    pending: Option<mpsc::Receiver<Vec<Computed>>>,
}
struct Computed {
    event: Event,
    kind: String,
    generation: u64,
    digest: Option<String>,
}
/// Called once when the single writer opens, independently of schema migrations.
pub(crate) fn recover_startup(db: &Connection) -> Result<()> {
    db.execute(
        "UPDATE classification_work SET state='queued',error=NULL WHERE state='inflight'",
        [],
    )?;
    Ok(())
}
pub(crate) fn enqueue(db: &Connection, id: i64) -> Result<()> {
    db.execute("INSERT OR IGNORE INTO classification_work(event_id,revision,kind,source_generation) SELECT id,CASE WHEN blob_hash IS NOT NULL THEN 'blob:'||blob_hash ELSE 'ref:'||reference_hash END,'clipboard',0 FROM events WHERE id=?1 AND source IN ('clipboard','clipboard_image','clipboard_file') AND (blob_hash IS NOT NULL OR reference_hash IS NOT NULL)",[id])?;
    Ok(())
}
impl ClassificationRuntime {
    #[cfg(test)]
    pub(crate) fn poll(&mut self, db: &Connection) -> Result<serde_json::Value> {
        self.poll_enabled(db, true)
    }
    pub(crate) fn poll_enabled(
        &mut self,
        db: &Connection,
        allow_new_work: bool,
    ) -> Result<serde_json::Value> {
        let mut changed = 0;
        if let Some(rx) = self.pending.as_ref() {
            match rx.try_recv() {
                Ok(results) => {
                    self.pending = None;
                    let tx = db.unchecked_transaction()?;
                    for result in results {
                        let current:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE id=?1 AND (CASE WHEN blob_hash IS NOT NULL THEN 'blob:'||blob_hash ELSE 'ref:'||reference_hash END)=?2)",params![result.event.id,result.event.revision],|r|r.get(0))?;
                        if !current {
                            continue;
                        }
                        let error = if result.digest.is_none() {
                            Some("unreadable_changed_or_unsupported")
                        } else {
                            None
                        };
                        tx.execute("UPDATE classification_work SET state=?1,digest=?2,error=?3 WHERE event_id=?4 AND revision=?5 AND kind=?6 AND source_generation=?7",params![if error.is_some(){"failed"}else{"done"},result.digest,error,result.event.id,result.event.revision,result.kind,result.generation])?;
                        if let Some(digest) = result.digest {
                            if result.kind=="source" {
       let inserted=tx.execute("INSERT OR IGNORE INTO verified_source_digests SELECT ?1,?2 WHERE EXISTS(SELECT 1 FROM source_configurations WHERE source_id='sharex' AND generation=?2 AND enabled=1)",params![digest,result.generation])?;
       if inserted>0 && tx.query_row("SELECT EXISTS(SELECT 1 FROM classification_work WHERE digest=?1 AND kind='clipboard' AND state='done')",params![digest],|r|r.get::<_,bool>(0))? {changed+=1;}
      } else if tx.query_row("SELECT EXISTS(SELECT 1 FROM verified_source_digests d JOIN source_configurations c ON c.source_id='sharex' AND c.enabled=1 AND c.generation=d.source_generation WHERE d.digest=?1)",params![digest],|r|r.get::<_,bool>(0))? {changed+=1;}
                        }
                    }
                    tx.commit()?;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    db.execute("UPDATE classification_work SET state='failed',error='worker_disconnected' WHERE state='inflight'",[])?;
                }
            }
        }
        if allow_new_work && self.pending.is_none() {
            let tasks = {
                let mut stmt=db.prepare("SELECT w.event_id,w.kind,w.source_generation FROM classification_work w WHERE w.state='queued' AND (w.kind='clipboard' OR EXISTS(SELECT 1 FROM source_configurations c WHERE c.source_id='sharex' AND c.enabled=1 AND c.generation=w.source_generation)) ORDER BY w.event_id LIMIT 8")?;
                stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, u64>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
            };
            if !tasks.is_empty() {
                let mut work = Vec::new();
                let tx = db.unchecked_transaction()?;
                for (id, kind, generation) in tasks {
                    let event = storage::get_event(&tx, id)?;
                    tx.execute("UPDATE classification_work SET state='inflight' WHERE event_id=?1 AND kind=?2 AND source_generation=?3 AND state='queued'",params![id,kind,generation])?;
                    work.push((event, kind, generation));
                }
                tx.commit()?;
                let (tx, rx) = mpsc::sync_channel(1);
                self.pending = Some(rx);
                std::thread::Builder::new()
                    .name("katran-classification".into())
                    .spawn(move || {
                        let result = work
                            .into_iter()
                            .map(|(event, kind, generation)| {
                                let digest = screenshot_identity::decode_digest(&event);
                                Computed {
                                    event,
                                    kind,
                                    generation,
                                    digest,
                                }
                            })
                            .collect();
                        let _ = tx.send(result);
                    })?;
            }
        }
        let (queued,failed):(u64,u64)=db.query_row("SELECT COALESCE((SELECT SUM(count) FROM classification_counts WHERE state IN ('queued','inflight')),0),COALESCE((SELECT count FROM classification_counts WHERE state='failed'),0)",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        Ok(
            serde_json::json!({"classification_changed":changed>0,"queued":queued,"failed":failed,"worker_active":self.pending.is_some()}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_queue_waits_until_enabled() {
        let temp = tempfile::tempdir().unwrap();
        let mut service = Service::open(temp.path(), 1000).unwrap();
        service.db.execute("INSERT INTO events(detected_at_ms,source,name,path,size,reference_hash) VALUES(0,'clipboard_file','synthetic.png','unavailable',0,'r')", []).unwrap();
        service.db.execute("INSERT INTO classification_work(event_id,revision,kind,state) VALUES(1,'ref:r','clipboard','queued')", []).unwrap();
        let paused = service
            .classification
            .poll_enabled(&service.db, false)
            .unwrap();
        assert_eq!(paused["queued"], 1);
        assert_eq!(paused["worker_active"], false);
        let resumed = service
            .classification
            .poll_enabled(&service.db, true)
            .unwrap();
        assert_eq!(resumed["worker_active"], true);
        let state: String = service
            .db
            .query_row(
                "SELECT state FROM classification_work WHERE event_id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "inflight");
    }
    #[test]
    fn idle_classification_work_is_independent_of_completed_history() {
        let mut measures = Vec::new();
        for count in [100_000i64, 1_000_000] {
            let temp = tempfile::tempdir().unwrap();
            let mut service = Service::open(temp.path(), 1000).unwrap();
            let tx = service.db.unchecked_transaction().unwrap();
            tx.execute("WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM seq WHERE n<?1) INSERT INTO events(detected_at_ms,source,name,path,size,reference_hash) SELECT 0,'test','synthetic.bin','unavailable',0,'r' FROM seq",[count]).unwrap();
            tx.execute("INSERT INTO classification_work(event_id,revision,kind,state) SELECT id,'ref:r','clipboard','done' FROM events",[]).unwrap();
            tx.commit().unwrap();
            let mut stmt=service.db.prepare("SELECT COALESCE((SELECT SUM(count) FROM classification_counts WHERE state IN ('queued','inflight')),0),COALESCE((SELECT count FROM classification_counts WHERE state='failed'),0)").unwrap();
            assert_eq!(
                stmt.query_row([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
                    .unwrap(),
                (0, 0)
            );
            measures.push(stmt.get_status(rusqlite::StatementStatus::VmStep));
            drop(stmt);
            let status = service.classification.poll(&service.db).unwrap();
            assert_eq!(status["queued"], 0);
            assert_eq!(status["worker_active"], false);
            service
                .db
                .execute(
                    "UPDATE classification_work SET state='failed' WHERE event_id=1",
                    [],
                )
                .unwrap();
            assert_eq!(
                service.classification.poll(&service.db).unwrap()["failed"],
                1
            );
            service
                .db
                .execute("DELETE FROM events WHERE id=1", [])
                .unwrap();
            assert_eq!(
                service.classification.poll(&service.db).unwrap()["failed"],
                0
            );
        }
        assert_eq!(
            measures[0], measures[1],
            "Idle counters traversed completed history: {measures:?}"
        );
    }
}
