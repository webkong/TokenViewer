use std::path::Path;

use super::codex;
use crate::models::UsageRecord;

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    codex::parse_codex_format(home_dir, cursor_data, ".code/sessions", "every-code")
}
