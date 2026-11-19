/// Remove tables introduced after v12 before a test recreates an old catalogue.
pub fn remove_later_tables(db: &rusqlite::Connection) {
    db.execute_batch(
        "DROP TABLE bull_members;
         DROP TABLE translated_images;
         DROP TABLE desktop_scoped_assignments;
         DROP TABLE tag_scoped_assignments;
         DROP TABLE tag_scoped_slots;",
    )
    .unwrap();
}
