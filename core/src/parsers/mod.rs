pub mod utils;
pub mod claude;
pub mod codex;
pub mod cursor;
pub mod gemini;
pub mod kiro;
pub mod opencode;
pub mod openclaw;
pub mod everycode;
pub mod hermes;
pub mod copilot;
pub mod kimi;
pub mod grok;
pub mod antigravity;
pub mod roocode;
pub mod kilocode;
pub mod kilocli;
pub mod zed;
pub mod goose;
pub mod ohmypi;
pub mod pi;
pub mod craft;
pub mod codebuddy;
pub mod workbuddy;
pub mod mimocode;

use std::collections::HashMap;
use std::path::Path;

use rayon::prelude::*;

use crate::models::UsageRecord;

pub struct ParseResult {
    pub source: String,
    pub records: Vec<UsageRecord>,
    pub new_cursor: String,
}

type ParserFn = fn(&Path, Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>>;

/// Returns all registered parsers as (source_name, parse_fn).
fn all_parsers() -> Vec<(&'static str, ParserFn)> {
    vec![
        ("claude", claude::parse),
        ("codex", codex::parse),
        ("cursor", cursor::parse),
        ("gemini", gemini::parse),
        ("kiro", kiro::parse),
        ("opencode", opencode::parse),
        ("openclaw", openclaw::parse),
        ("everycode", everycode::parse),
        ("hermes", hermes::parse),
        ("copilot", copilot::parse),
        ("kimi", kimi::parse),
        ("grok", grok::parse),
        ("antigravity", antigravity::parse),
        ("roocode", roocode::parse),
        ("kilocode", kilocode::parse),
        ("kilocli", kilocli::parse),
        ("zed", zed::parse),
        ("goose", goose::parse),
        ("ohmypi", ohmypi::parse),
        ("pi", pi::parse),
        ("craft", craft::parse),
        ("codebuddy", codebuddy::parse),
        ("workbuddy", workbuddy::parse),
        ("mimocode", mimocode::parse),
    ]
}

pub fn all_parser_sources() -> Vec<&'static str> {
    all_parsers().into_iter().map(|(source, _)| source).collect()
}

/// Parse all providers in parallel. `cursors` maps source name -> cursor JSON string.
pub fn parse_all(home_dir: &Path, cursors: &HashMap<String, String>) -> Vec<ParseResult> {
    all_parsers()
        .into_par_iter()
        .map(|(source, parser_fn)| {
            let cursor_data = cursors.get(source).map(|s| s.as_str());
            match parser_fn(home_dir, cursor_data) {
                Ok((records, new_cursor)) => ParseResult {
                    source: source.to_string(),
                    records,
                    new_cursor,
                },
                Err(e) => {
                    eprintln!("tokenviewer: parser '{}' failed: {}", source, e);
                    ParseResult {
                        source: source.to_string(),
                        records: vec![],
                        new_cursor: cursor_data.unwrap_or("{}").to_string(),
                    }
                }
            }
        })
        .collect()
}
