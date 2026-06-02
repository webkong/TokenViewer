use std::path::Path;

use crate::models::UsageRecord;
use super::roocode;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    roocode::parse_ui_messages(home_dir, cursor_data, "kilocode.kilo-code", "kilocode")
}
