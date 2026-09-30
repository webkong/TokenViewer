use tokenviewer_core::storage::Database;
use tokenviewer_core::sync;

#[test]
fn test_full_sync() {
    // Integration checks must never migrate or sync the user's live database.
    let fixture = tempfile::TempDir::new().unwrap();
    let home = fixture.path();
    let db_path = home.join(".tokenviewer/data.db");

    std::fs::create_dir_all(home.join(".tokenviewer")).unwrap();

    let db = Database::open(&db_path).unwrap();
    println!("Database opened at: {}", db_path.display());

    let result = sync::sync_all(&db, home);
    println!("Agents synced: {}", result.agents_synced);
    println!("Records added: {}", result.records_added);
    for e in &result.errors {
        println!("  Error: {}", e);
    }
    // Should not panic; agents_synced should be > 0 if any tool is installed.
    assert!(result.errors.len() <= 23); // at most one error per agent

    // Verify cost computation does not panic and pricing resolves.
    let rows = db
        .aggregate_by_model("2020-01-01T00:00:00Z", "2030-01-01T00:00:00Z")
        .unwrap();
    let total_cost: f64 = rows
        .iter()
        .map(tokenviewer_core::pricing::compute_row_cost)
        .sum();
    println!("Models: {}, total cost: ${:.2}", rows.len(), total_cost);
    assert!(total_cost >= 0.0);
}
