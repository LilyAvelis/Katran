use rusqlite::Connection;

use katran::pinned_order::{Direction, ensure_schema, generation, move_one, rank, swap};

fn schema(db: &Connection) {
    db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE events(id INTEGER PRIMARY KEY); INSERT INTO events VALUES(1),(2),(3),(4); CREATE TABLE katran_properties(event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,pinned INTEGER NOT NULL); INSERT INTO katran_properties VALUES(1,1),(2,1),(3,0);").unwrap();
}
fn ids(db: &Connection) -> Vec<i64> {
    db.prepare("SELECT event_id FROM katran_pinned_order ORDER BY position,event_id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn migration_movement_restart_and_membership() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let db = Connection::open(&path).unwrap();
    schema(&db);
    ensure_schema(&db).unwrap();
    assert_eq!(ids(&db), vec![2, 1]);
    let original = generation(&db).unwrap();
    assert!(!move_one(&db, 2, Direction::Up).unwrap().moved);
    assert_eq!(generation(&db).unwrap(), original);
    assert!(move_one(&db, 1, Direction::Up).unwrap().moved);
    assert_eq!(ids(&db), vec![1, 2]);
    assert_eq!(
        move_one(&db, 3, Direction::Down).unwrap_err().code,
        "not_pinned"
    );
    assert_eq!(
        move_one(&db, 99, Direction::Up).unwrap_err().code,
        "not_found"
    );
    db.execute("UPDATE katran_properties SET pinned=1 WHERE event_id=3", [])
        .unwrap();
    assert_eq!(ids(&db), vec![1, 2, 3]);
    db.execute("UPDATE katran_properties SET pinned=0 WHERE event_id=2", [])
        .unwrap();
    assert_eq!(rank(&db, 2).unwrap(), None);
    db.execute("UPDATE katran_properties SET pinned=1 WHERE event_id=2", [])
        .unwrap();
    assert_eq!(ids(&db), vec![1, 3, 2]);
    drop(db);
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    ensure_schema(&db).unwrap();
    assert_eq!(ids(&db), vec![1, 3, 2]);
    let before = generation(&db).unwrap();
    db.execute("DELETE FROM events WHERE id=3", []).unwrap();
    assert_eq!(ids(&db), vec![1, 2]);
    assert!(generation(&db).unwrap() > before);
    assert!(move_one(&db, 1, Direction::Down).unwrap().moved);
    assert_eq!(ids(&db), vec![2, 1]);
}

#[test]
fn swap_visible_neighbors_across_hidden_pin() {
    let db = Connection::open_in_memory().unwrap();
    schema(&db);
    db.execute("UPDATE katran_properties SET pinned=1 WHERE event_id=3", [])
        .unwrap();
    ensure_schema(&db).unwrap();
    assert_eq!(ids(&db), vec![3, 2, 1]);
    let before = generation(&db).unwrap();
    let result = swap(&db, 3, 1).unwrap();
    assert!(result.moved);
    assert_eq!(result.generation, before + 1);
    assert_eq!(ids(&db), vec![1, 2, 3]); // 2 is hidden in the active view.
    let no_op = swap(&db, 1, 1).unwrap();
    assert!(!no_op.moved);
    assert_eq!(no_op.generation, result.generation);
    assert_eq!(swap(&db, 1, 4).unwrap_err().code, "not_pinned");
    assert_eq!(swap(&db, 1, 99).unwrap_err().code, "not_found");
    assert_eq!(ids(&db), vec![1, 2, 3]);
    assert_eq!(generation(&db).unwrap(), result.generation);
}
