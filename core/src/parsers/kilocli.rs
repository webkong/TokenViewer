use std::path::Path;

use super::opencode;
use crate::models::UsageRecord;

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let db_path = home_dir.join(".local/share/kilo/kilo.db");
    opencode::parse_opencode_db(&db_path, cursor_data, "kilo-cli")
}
