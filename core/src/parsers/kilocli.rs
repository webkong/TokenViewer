use std::path::Path;

use super::opencode;
use super::utils::resolve_local_data_path;
use crate::models::UsageRecord;

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let db_path = resolve_local_data_path(home_dir, "kilo/kilo.db");
    opencode::parse_opencode_db(&db_path, cursor_data, "kilo-cli")
}
