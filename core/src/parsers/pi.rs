use std::path::Path;

use crate::models::UsageRecord;
use super::ohmypi;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    ohmypi::parse_pi_session(home_dir, cursor_data, ".pi/agent/sessions", "pi", "pi-unknown")
}
