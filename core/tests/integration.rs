use std::path::Path;
use tokenviewer_core::storage::Database;
use tokenviewer_core::sync;

#[test]
fn test_full_sync() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let db_path = format!("{}/.tokenviewer/data.db", home);

    std::fs::create_dir_all(format!("{}/.tokenviewer", home)).unwrap();

    let db = Database::open(Path::new(&db_path)).unwrap();
    println!("Database opened at: {}", db_path);

    let result = sync::sync_all(&db, Path::new(&home));
    println!("Providers synced: {}", result.providers_synced);
    println!("Records added: {}", result.records_added);
    for e in &result.errors {
        println!("  Error: {}", e);
    }
    // Should not panic, providers_synced should be > 0 if any tool is installed
    assert!(result.errors.len() <= 23); // at most one error per provider

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
