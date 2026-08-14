use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::skills::models::{SkillEntry, SkillEnvironmentVariable, SkillManifest};

pub struct Scanner {
    source_root: PathBuf,
}

impl Scanner {
    pub fn new(source_root: PathBuf) -> Self {
        Self { source_root }
    }

    pub fn source_root(&self) -> &PathBuf {
        &self.source_root
    }

    /// Scan a single directory for skills (any path, not just source_root).
    pub fn scan_path(&self, path: &Path) -> Result<Vec<SkillEntry>, String> {
        let mut skills = Vec::new();

        if !path.exists() || !path.is_dir() {
            return Ok(skills);
        }

        self.scan_path_inner(path, path, 0, &mut skills)?;
        Ok(skills)
    }

    fn scan_path_inner(
        &self,
        scan_root: &Path,
        path: &Path,
        depth: usize,
        skills: &mut Vec<SkillEntry>,
    ) -> Result<(), String> {
        let entries =
            fs::read_dir(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let sub_path = entry.path();
            if !sub_path.is_dir() {
                continue;
            }

            if let Some(name) = sub_path.file_name().and_then(|n| n.to_str()) {
                if matches!(name, ".git" | "node_modules" | "target" | "DerivedData") {
                    continue;
                }
            }

            if Self::validate_skill_dir(&sub_path) {
                if let Ok(skill) = self.parse_skill_dir(scan_root, &sub_path) {
                    skills.push(skill);
                }
                continue;
            }

            if depth < 2 {
                let _ = self.scan_path_inner(scan_root, &sub_path, depth + 1, skills);
            }
        }

        Ok(())
    }

    /// Scan all skill directories under source_root (one level deep).
    /// Returns all valid SkillEntry objects.
    pub fn scan_all(&self) -> Result<Vec<SkillEntry>, String> {
        self.scan_path(&self.source_root)
    }

    /// Detect new skills by comparing against a set of known skill IDs.
    pub fn detect_new(&self, known: &HashSet<String>) -> Result<Vec<SkillEntry>, String> {
        let all = self.scan_all()?;
        let new: Vec<SkillEntry> = all.into_iter().filter(|s| !known.contains(&s.id)).collect();
        Ok(new)
    }

    /// Validate that a directory contains SKILL.md.
    /// manifest.json is optional — if missing, a default manifest is generated.
    pub fn validate_skill_dir(path: &Path) -> bool {
        path.join("SKILL.md").is_file() || path.join("skill.md").is_file()
    }
}

/// Extract description from SKILL.md (first paragraph after title, max 200 chars).
pub fn extract_description(skill_md_path: &Path) -> String {
    let content = match fs::read_to_string(skill_md_path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let mut desc = String::new();
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut skipped_title = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip YAML frontmatter
        if !frontmatter_done {
            if trimmed == "---" {
                if !in_frontmatter {
                    in_frontmatter = true;
                    continue;
                } else {
                    in_frontmatter = false;
                    frontmatter_done = true;
                    continue;
                }
            }
            if in_frontmatter {
                continue;
            }
        }

        // Skip markdown title lines (# ...)
        if !skipped_title && trimmed.starts_with('#') {
            skipped_title = true;
            continue;
        }
        skipped_title = true;

        // Skip empty lines at the start
        if trimmed.is_empty() && desc.is_empty() {
            continue;
        }

        if trimmed.is_empty() && !desc.is_empty() {
            // Empty line after content - stop
            break;
        }

        if !desc.is_empty() {
            desc.push(' ');
        }
        desc.push_str(trimmed);
    }

    // Truncate to ~200 chars, breaking at word boundary
    if desc.len() > 200 {
        let mut end = 200;
        while end > 0 && !desc.as_bytes()[end].is_ascii_whitespace() {
            end -= 1;
        }
        if end == 0 {
            end = 200;
        }
        desc.truncate(end);
        desc.push_str("...");
    }

    desc
}

/// Extract environment-variable declarations from SKILL.md frontmatter.
///
/// The Agent Skills specification does not define a structured environment
/// variable field. It does allow arbitrary metadata, so accept only extension
/// keys that explicitly identify themselves as `env`/`environment`, such as
/// `environment_variables`, `required-env`, or `metadata.env`.
pub fn extract_environment_variables(skill_md_path: &Path) -> Vec<SkillEnvironmentVariable> {
    let content = match fs::read_to_string(skill_md_path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Vec::new();
    }

    let mut frontmatter = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        frontmatter.push(line);
    }

    let Some((section_index, section_indent)) =
        frontmatter.iter().enumerate().find_map(|(index, line)| {
            is_environment_frontmatter_key(line).then_some((index, leading_whitespace_count(line)))
        })
    else {
        return Vec::new();
    };

    let mut variables = Vec::new();
    let mut current: Option<SkillEnvironmentVariable> = None;
    if let Some((_, inline_value)) = frontmatter[section_index].split_once(':') {
        collect_inline_frontmatter_environment_variables(inline_value, &mut variables);
    }

    for line in frontmatter.into_iter().skip(section_index + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_whitespace_count(line) <= section_indent {
            break;
        }

        if let Some(item) = trimmed.strip_prefix("- ") {
            if let Some(variable) = current.take() {
                push_valid_environment_variable(&mut variables, variable);
            }
            let mut variable = SkillEnvironmentVariable {
                name: String::new(),
                default_value: String::new(),
                required: false,
                note: String::new(),
                secret: false,
                inferred: false,
            };
            if let Some(value) = item.strip_prefix("name:") {
                variable.name = yaml_scalar(value);
            } else {
                variable.name = yaml_scalar(item);
            }
            current = Some(variable);
            continue;
        }

        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            if is_valid_environment_variable_name(key)
                && key == key.to_ascii_uppercase()
                && !matches!(key, "NAME" | "REQUIRED" | "NOTE" | "SECRET")
            {
                if let Some(variable) = current.take() {
                    push_valid_environment_variable(&mut variables, variable);
                }
                let value = yaml_scalar(value);
                current = Some(SkillEnvironmentVariable {
                    name: key.to_string(),
                    default_value: if value.is_empty() || value.eq_ignore_ascii_case("null") {
                        String::new()
                    } else {
                        value
                    },
                    required: false,
                    note: String::new(),
                    secret: false,
                    inferred: false,
                });
                continue;
            }
        }

        if let Some(variable) = current.as_mut() {
            if let Some(value) = trimmed.strip_prefix("name:") {
                variable.name = yaml_scalar(value);
            } else if let Some(value) = trimmed
                .strip_prefix("default:")
                .or_else(|| trimmed.strip_prefix("default_value:"))
                .or_else(|| trimmed.strip_prefix("default-value:"))
                .or_else(|| trimmed.strip_prefix("defaultValue:"))
                .or_else(|| trimmed.strip_prefix("value:"))
            {
                variable.default_value = yaml_scalar(value);
            } else if let Some(value) = trimmed.strip_prefix("required:") {
                variable.required = yaml_scalar(value).eq_ignore_ascii_case("true");
            } else if let Some(value) = trimmed.strip_prefix("note:") {
                variable.note = yaml_scalar(value);
            } else if let Some(value) = trimmed.strip_prefix("secret:") {
                variable.secret = yaml_scalar(value).eq_ignore_ascii_case("true");
            }
        }
    }

    if let Some(variable) = current {
        push_valid_environment_variable(&mut variables, variable);
    }
    variables
}

fn collect_inline_frontmatter_environment_variables(
    value: &str,
    variables: &mut Vec<SkillEnvironmentVariable>,
) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    let value = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(value);
    for name in value.split(',').map(yaml_scalar) {
        push_valid_environment_variable(
            variables,
            SkillEnvironmentVariable {
                name,
                default_value: String::new(),
                required: false,
                note: String::new(),
                secret: false,
                inferred: false,
            },
        );
    }
}

fn is_environment_frontmatter_key(line: &str) -> bool {
    let trimmed = line.trim();
    let Some((key, _)) = trimmed.split_once(':') else {
        return false;
    };
    let tokens: Vec<&str> = key
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    tokens.iter().any(|token| {
        token.eq_ignore_ascii_case("env")
            || token.eq_ignore_ascii_case("envs")
            || token.eq_ignore_ascii_case("environment")
            || token.eq_ignore_ascii_case("environments")
    })
}

fn leading_whitespace_count(value: &str) -> usize {
    value
        .chars()
        .take_while(|character| character.is_whitespace())
        .count()
}

fn yaml_scalar(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let first = value.as_bytes()[0];
        let last = value.as_bytes()[value.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn push_valid_environment_variable(
    variables: &mut Vec<SkillEnvironmentVariable>,
    mut variable: SkillEnvironmentVariable,
) {
    if !is_valid_environment_variable_name(&variable.name) {
        return;
    }
    if !variable.secret {
        let uppercase = variable.name.to_ascii_uppercase();
        variable.secret = ["KEY", "TOKEN", "SECRET", "PASSWORD", "CREDENTIAL"]
            .iter()
            .any(|marker| uppercase.contains(marker));
    }
    if let Some(existing) = variables
        .iter_mut()
        .find(|existing| existing.name == variable.name)
    {
        if existing.default_value.is_empty() {
            existing.default_value = variable.default_value;
        }
        if existing.note.is_empty() {
            existing.note = variable.note;
        }
        existing.required |= variable.required;
        existing.secret |= variable.secret;
        existing.inferred &= variable.inferred;
    } else {
        variables.push(variable);
    }
}

/// Infer optional environment variables from the Skill instructions.
///
/// Explicit Environment/ENV sections are always scanned. Other sections are
/// scanned only when they contain a strong environment-variable signal such as
/// `$NAME`, `${NAME}`, or an inline-code assignment like `NAME=value`, and only
/// compound `UPPER_SNAKE_CASE` names are inferred from those implicit sections.
/// This lets descriptive sections such as "Storage" declare related variables
/// while avoiding unrelated uppercase words elsewhere in the document.
pub fn infer_environment_variables(skill_md_path: &Path) -> Vec<SkillEnvironmentVariable> {
    let content = match fs::read_to_string(skill_md_path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };
    let mut names = Vec::new();
    let mut defaults = HashMap::new();
    let mut section_lines = Vec::new();
    let mut environment_heading_level: Option<usize> = None;
    for line in markdown_body(&content).lines() {
        if let Some((level, title)) = markdown_heading(line) {
            collect_environment_names_from_section(
                &section_lines,
                environment_heading_level.is_some(),
                &mut names,
                &mut defaults,
            );
            section_lines.clear();
            if environment_heading_level.is_some_and(|active| level <= active) {
                environment_heading_level = None;
            }
            if is_environment_heading(title) {
                environment_heading_level = Some(level);
            }
            continue;
        }
        section_lines.push(line);
    }
    collect_environment_names_from_section(
        &section_lines,
        environment_heading_level.is_some(),
        &mut names,
        &mut defaults,
    );

    names
        .into_iter()
        .filter(|name| !is_standard_environment_variable(name))
        .map(|name| {
            let uppercase = name.to_ascii_uppercase();
            SkillEnvironmentVariable {
                default_value: defaults.remove(&name).unwrap_or_default(),
                name,
                required: false,
                note: String::new(),
                secret: ["KEY", "TOKEN", "SECRET", "PASSWORD", "CREDENTIAL"]
                    .iter()
                    .any(|marker| uppercase.contains(marker)),
                inferred: true,
            }
        })
        .collect()
}

fn collect_environment_names_from_section(
    lines: &[&str],
    is_explicit_environment_section: bool,
    names: &mut Vec<String>,
    defaults: &mut HashMap<String, String>,
) {
    let mut strong_names = Vec::new();
    for line in lines {
        collect_dollar_environment_names(line, &mut strong_names);
        collect_assigned_inline_environment_names(line, &mut strong_names);
    }
    if !is_explicit_environment_section && strong_names.is_empty() {
        return;
    }

    let mut section_names = Vec::new();
    for line in lines {
        collect_dollar_environment_names(line, &mut section_names);
        collect_inline_code_environment_names(line, &mut section_names);
        collect_leading_environment_name(line, &mut section_names);
    }
    for name in section_names {
        if is_explicit_environment_section || name.contains('_') {
            add_environment_name(&name, names);
        }
    }
    for line in lines {
        collect_environment_defaults(line, defaults);
    }
}

fn collect_environment_defaults(line: &str, defaults: &mut HashMap<String, String>) {
    let mut remainder = line;
    while let Some(start) = remainder.find("${") {
        remainder = &remainder[start + 2..];
        let Some(end) = remainder.find('}') else {
            break;
        };
        let expression = &remainder[..end];
        remainder = &remainder[end + 1..];
        let Some((name, value)) = expression
            .split_once(":-")
            .or_else(|| expression.split_once(":="))
            .or_else(|| expression.split_once('-'))
            .or_else(|| expression.split_once('='))
        else {
            continue;
        };
        insert_environment_default(name, value, defaults);
    }

    let mut code = line;
    while let Some(start) = code.find('`') {
        code = &code[start + 1..];
        let Some(end) = code.find('`') else {
            break;
        };
        collect_assignment_default(code[..end].trim(), defaults);
        code = &code[end + 1..];
    }

    let candidate = line
        .trim()
        .trim_start_matches(|character: char| {
            character == '-' || character == '*' || character == '+' || character.is_whitespace()
        })
        .strip_prefix("export ")
        .unwrap_or_else(|| {
            line.trim().trim_start_matches(|character: char| {
                character == '-'
                    || character == '*'
                    || character == '+'
                    || character.is_whitespace()
            })
        });
    collect_assignment_default(candidate, defaults);
}

fn collect_assignment_default(value: &str, defaults: &mut HashMap<String, String>) {
    let Some((name, value)) = value.split_once('=') else {
        return;
    };
    insert_environment_default(name.trim(), value.trim(), defaults);
}

fn insert_environment_default(name: &str, value: &str, defaults: &mut HashMap<String, String>) {
    if !is_valid_environment_variable_name(name)
        || name != name.to_ascii_uppercase()
        || is_standard_environment_variable(name)
    {
        return;
    }
    let value = yaml_scalar(value)
        .trim_end_matches(|character: char| character == ',' || character == ';')
        .to_string();
    if value.is_empty() || value.starts_with("$(") {
        return;
    }
    defaults.entry(name.to_string()).or_insert(value);
}

fn markdown_body(content: &str) -> &str {
    let Some(rest) = content.strip_prefix("---") else {
        return content;
    };
    let Some(end) = rest.find("\n---") else {
        return content;
    };
    &rest[end + 4..]
}

fn markdown_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let level = trimmed
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if level == 0 || level > 6 {
        return None;
    }
    let title = trimmed[level..].trim();
    (!title.is_empty()).then_some((level, title))
}

fn is_environment_heading(title: &str) -> bool {
    let normalized = title
        .trim_matches(|character: char| !character.is_alphanumeric())
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "env"
            | "envs"
            | "environment"
            | "environments"
            | "environment variable"
            | "environment variables"
            | "env variable"
            | "env variables"
    ) || normalized.contains("环境变量")
}

fn collect_dollar_environment_names(line: &str, names: &mut Vec<String>) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }
        index += 1;
        if index < bytes.len() && bytes[index] == b'{' {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && (bytes[index] == b'_' || bytes[index].is_ascii_alphanumeric())
        {
            index += 1;
        }
        if start < index {
            add_environment_name(&line[start..index], names);
        }
    }
}

fn collect_inline_code_environment_names(line: &str, names: &mut Vec<String>) {
    let mut remainder = line;
    while let Some(start) = remainder.find('`') {
        remainder = &remainder[start + 1..];
        let Some(end) = remainder.find('`') else {
            break;
        };
        let token = remainder[..end].trim();
        remainder = &remainder[end + 1..];

        let token = token
            .strip_prefix("${")
            .or_else(|| token.strip_prefix('$'))
            .unwrap_or(token);
        let name: String = token
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        let suffix = &token[name.len()..];
        if !name.is_empty()
            && (suffix.is_empty()
                || suffix.starts_with('=')
                || suffix.starts_with(':')
                || suffix.starts_with('}'))
        {
            add_environment_name(&name, names);
        }
    }
}

fn collect_assigned_inline_environment_names(line: &str, names: &mut Vec<String>) {
    let mut remainder = line;
    while let Some(start) = remainder.find('`') {
        remainder = &remainder[start + 1..];
        let Some(end) = remainder.find('`') else {
            break;
        };
        let token = remainder[..end].trim();
        remainder = &remainder[end + 1..];

        let name: String = token
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        if !name.is_empty() && token[name.len()..].starts_with('=') {
            add_environment_name(&name, names);
        }
    }
}

fn collect_leading_environment_name(line: &str, names: &mut Vec<String>) {
    let candidate = line
        .trim()
        .trim_start_matches(|character: char| {
            character == '-' || character == '*' || character == '+' || character.is_whitespace()
        })
        .strip_prefix("export ")
        .unwrap_or_else(|| {
            line.trim().trim_start_matches(|character: char| {
                character == '-'
                    || character == '*'
                    || character == '+'
                    || character.is_whitespace()
            })
        });
    let name: String = candidate
        .trim_start_matches('`')
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect();
    if !name.is_empty() {
        add_environment_name(&name, names);
    }
}

fn add_environment_name(name: &str, names: &mut Vec<String>) {
    if is_valid_environment_variable_name(name)
        && name == name.to_ascii_uppercase()
        && name.chars().any(|character| character.is_ascii_uppercase())
        && !names.iter().any(|existing| existing == name)
    {
        names.push(name.to_string());
    }
}

fn is_standard_environment_variable(name: &str) -> bool {
    matches!(
        name,
        "HOME"
            | "PATH"
            | "PWD"
            | "OLDPWD"
            | "SHELL"
            | "USER"
            | "LOGNAME"
            | "TMPDIR"
            | "SHLVL"
            | "TERM"
            | "LANG"
            | "LC_ALL"
            | "DISPLAY"
            | "HOSTNAME"
            | "UID"
            | "EUID"
            | "PPID"
            | "RANDOM"
            | "SECONDS"
            | "LINENO"
    )
}

fn is_valid_environment_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

impl Scanner {
    /// Parse a skill directory into a SkillEntry.
    /// Reads manifest.json if present, otherwise generates a default manifest.
    fn parse_skill_dir(&self, scan_root: &Path, path: &Path) -> Result<SkillEntry, String> {
        let id = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("Invalid directory name: {}", path.display()))?
            .to_string();

        let manifest_path = path.join("manifest.json");
        let mut manifest = if manifest_path.is_file() {
            let manifest_content = fs::read_to_string(&manifest_path)
                .map_err(|e| format!("Failed to read {}: {}", manifest_path.display(), e))?;
            let m: SkillManifest = serde_json::from_str(&manifest_content)
                .map_err(|e| format!("Failed to parse {}: {}", manifest_path.display(), e))?;
            m
        } else {
            // Generate default manifest from directory name.
            // has_manifest stays false (the serde default) — not user-authored.
            SkillManifest {
                name: id.clone(),
                description: format!("{} skill", id),
                tags: Vec::new(),
                compatible_agents: vec!["*".to_string()],
                version: "unknown".to_string(),
                environment_variables: Vec::new(),
                has_manifest: false,
            }
        };
        if manifest_path.is_file() {
            manifest.has_manifest = true;
        }
        let declared_variables = std::mem::take(&mut manifest.environment_variables);
        for variable in declared_variables {
            push_valid_environment_variable(&mut manifest.environment_variables, variable);
        }
        let skill_md_path = ["SKILL.md", "skill.md"]
            .iter()
            .map(|name| path.join(name))
            .find(|candidate| candidate.is_file());
        if let Some(skill_md_path) = skill_md_path {
            for variable in extract_environment_variables(&skill_md_path) {
                push_valid_environment_variable(&mut manifest.environment_variables, variable);
            }
            for variable in infer_environment_variables(&skill_md_path) {
                push_valid_environment_variable(&mut manifest.environment_variables, variable);
            }
        }

        let installed_at = chrono::Utc::now().to_rfc3339();
        let relative_path = path
            .strip_prefix(scan_root)
            .unwrap_or(path)
            .components()
            .filter_map(|component| component.as_os_str().to_str().map(str::to_string))
            .collect();

        Ok(SkillEntry {
            id,
            manifest,
            source_dir: path.to_string_lossy().to_string(),
            relative_path,
            installed_at,
            agent_ids: Vec::new(),
            is_built_in: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_skill(dir: &Path, name: &str, desc: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();

        // Create manifest.json
        let manifest = serde_json::json!({
            "name": name,
            "description": desc,
            "tags": ["test"],
            "compatible_agents": ["*"],
            "version": "1.0.0"
        });
        fs::write(
            skill_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        // Create SKILL.md
        fs::write(skill_dir.join("SKILL.md"), "# Test Skill\n").unwrap();
    }

    #[test]
    fn test_scan_empty_directory() {
        let dir = TempDir::new().unwrap();
        let scanner = Scanner::new(dir.path().to_path_buf());
        let skills = scanner.scan_all().unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_scan_with_skills() {
        let dir = TempDir::new().unwrap();
        create_test_skill(dir.path(), "code-review", "Review code");
        create_test_skill(dir.path(), "commit-msg", "Write commits");

        let scanner = Scanner::new(dir.path().to_path_buf());
        let skills = scanner.scan_all().unwrap();

        assert_eq!(skills.len(), 2);
        assert!(skills.iter().any(|s| s.id == "code-review"));
        assert!(skills.iter().any(|s| s.id == "commit-msg"));
    }

    #[test]
    fn test_scan_skips_invalid_dirs() {
        let dir = TempDir::new().unwrap();

        // Valid skill
        create_test_skill(dir.path(), "valid-skill", "Valid");

        // Missing manifest.json (now valid - uses default manifest)
        let no_manifest = dir.path().join("no-manifest");
        fs::create_dir_all(&no_manifest).unwrap();
        fs::write(no_manifest.join("SKILL.md"), "# No manifest\n").unwrap();

        // Missing SKILL.md (still invalid)
        let missing_skill = dir.path().join("no-skill-md");
        fs::create_dir_all(&missing_skill).unwrap();
        fs::write(
            missing_skill.join("manifest.json"),
            r#"{"name":"no-skill","description":"x","tags":[],"compatible_agents":["*"],"version":"1.0"}"#,
        )
        .unwrap();

        // Hidden containers such as Codex .system are scanned.
        let hidden = dir.path().join(".hidden");
        create_test_skill(&hidden, ".hidden-skill", "Hidden");

        // Git internals are ignored.
        let git_dir = dir.path().join(".git");
        create_test_skill(&git_dir, "not-a-skill", "Git internals");

        // File (not directory)
        fs::write(dir.path().join("some-file.txt"), "not a skill").unwrap();

        let scanner = Scanner::new(dir.path().to_path_buf());
        let skills = scanner.scan_all().unwrap();

        assert_eq!(skills.len(), 3); // valid-skill + no-manifest + hidden container skill
        assert!(skills.iter().any(|s| s.id == "valid-skill"));
        assert!(skills.iter().any(|s| s.id == "no-manifest"));
        assert!(skills.iter().any(|s| s.id == ".hidden-skill"));
        assert!(!skills.iter().any(|s| s.id == "not-a-skill"));
    }

    #[test]
    fn test_scan_nested_system_skills() {
        let dir = TempDir::new().unwrap();
        let system = dir.path().join(".system");
        create_test_skill(&system, "imagegen", "Generate images");

        let scanner = Scanner::new(dir.path().to_path_buf());
        let skills = scanner.scan_all().unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "imagegen");
        assert_eq!(skills[0].relative_path, vec![".system", "imagegen"]);
    }

    #[test]
    fn test_scan_preserves_nested_bundle_path() {
        let dir = TempDir::new().unwrap();
        let bundle_skills = dir.path().join("team-operating-system").join("skills");
        create_test_skill(&bundle_skills, "team-plan", "Plan team work");

        let scanner = Scanner::new(dir.path().to_path_buf());
        let skills = scanner.scan_all().unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(
            skills[0].relative_path,
            vec!["team-operating-system", "skills", "team-plan"]
        );
    }

    #[test]
    fn test_scan_extracts_environment_variables_from_frontmatter() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("web-collector");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: web-collector
dependencies:
  environment_variables:
    - name: OPENAI_API_KEY
      default: sk-example
      required: true
      note: "API access"
    CACHE_DIR: /tmp/web-collector-cache
---
# Web Collector
"#,
        )
        .unwrap();

        let scanner = Scanner::new(dir.path().to_path_buf());
        let skills = scanner.scan_all().unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(
            skills[0].manifest.environment_variables,
            vec![
                SkillEnvironmentVariable {
                    name: "OPENAI_API_KEY".to_string(),
                    default_value: "sk-example".to_string(),
                    required: true,
                    note: "API access".to_string(),
                    secret: true,
                    inferred: false,
                },
                SkillEnvironmentVariable {
                    name: "CACHE_DIR".to_string(),
                    default_value: "/tmp/web-collector-cache".to_string(),
                    required: false,
                    note: String::new(),
                    secret: false,
                    inferred: false,
                },
            ]
        );
    }

    #[test]
    fn test_scan_infers_environment_variables_from_skill_instructions() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("self-improving-agent");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: self-improving-agent
---
# Storage

`TokenViewer` is a product name, not an environment variable.

## 存储目录

1. `SELF_IMPROVEMENT_HOME`: explicit storage directory.
2. `SELF_IMPROVEMENT_SCOPE=project`: use project storage.
3. `SELF_IMPROVEMENT_PROJECT_ROOT`: explicit project root.
4. `${AGENTS_HOME:-$HOME/.agents}/learnings/`.
5. `$SINGLE` is a shell-local placeholder, not a compound environment key.

## Other configuration

`NOT_AN_ENVIRONMENT_VARIABLE` is outside the Environment section.
"#,
        )
        .unwrap();

        let scanner = Scanner::new(dir.path().to_path_buf());
        let skills = scanner.scan_all().unwrap();
        let variables = &skills[0].manifest.environment_variables;

        assert_eq!(
            variables
                .iter()
                .map(|variable| variable.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "SELF_IMPROVEMENT_HOME",
                "SELF_IMPROVEMENT_SCOPE",
                "SELF_IMPROVEMENT_PROJECT_ROOT",
                "AGENTS_HOME"
            ]
        );
        assert!(variables.iter().all(|variable| variable.inferred));
        assert!(!variables.iter().any(|variable| variable.name == "SINGLE"));
        assert_eq!(
            variables
                .iter()
                .find(|variable| variable.name == "SELF_IMPROVEMENT_SCOPE")
                .map(|variable| variable.default_value.as_str()),
            Some("project")
        );
        assert_eq!(
            variables
                .iter()
                .find(|variable| variable.name == "AGENTS_HOME")
                .map(|variable| variable.default_value.as_str()),
            Some("$HOME/.agents")
        );
        assert!(variables
            .iter()
            .find(|variable| variable.name == "SELF_IMPROVEMENT_HOME")
            .is_some_and(|variable| variable.default_value.is_empty()));
    }

    #[test]
    fn test_scan_accepts_explicit_env_frontmatter_extension_key() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("env-extension");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: env-extension
metadata:
  required-env:
    - name: SERVICE_API_KEY
      required: true
---
# Instructions
"#,
        )
        .unwrap();

        let scanner = Scanner::new(dir.path().to_path_buf());
        let skills = scanner.scan_all().unwrap();
        assert_eq!(
            skills[0].manifest.environment_variables,
            vec![SkillEnvironmentVariable {
                name: "SERVICE_API_KEY".to_string(),
                default_value: String::new(),
                required: true,
                note: String::new(),
                secret: true,
                inferred: false,
            }]
        );
    }

    #[test]
    fn test_detect_new_skills() {
        let dir = TempDir::new().unwrap();
        create_test_skill(dir.path(), "existing", "Already known");
        create_test_skill(dir.path(), "new-one", "New skill");

        let scanner = Scanner::new(dir.path().to_path_buf());

        let mut known = HashSet::new();
        known.insert("existing".to_string());

        let new_skills = scanner.detect_new(&known).unwrap();
        assert_eq!(new_skills.len(), 1);
        assert_eq!(new_skills[0].id, "new-one");
    }

    #[test]
    fn test_validate_skill_dir() {
        let dir = TempDir::new().unwrap();

        let valid_dir = dir.path().join("valid");
        fs::create_dir_all(&valid_dir).unwrap();
        fs::write(valid_dir.join("manifest.json"), "{}").unwrap();
        fs::write(valid_dir.join("SKILL.md"), "# Skill").unwrap();

        assert!(Scanner::validate_skill_dir(&valid_dir));

        // A directory with only SKILL.md (no manifest) is now valid
        let skill_only = dir.path().join("skill-only");
        fs::create_dir_all(&skill_only).unwrap();
        fs::write(skill_only.join("SKILL.md"), "# Skill only\n").unwrap();
        assert!(Scanner::validate_skill_dir(&skill_only));

        // A directory with neither SKILL.md nor manifest is invalid
        let empty_dir = dir.path().join("empty");
        fs::create_dir_all(&empty_dir).unwrap();
        assert!(!Scanner::validate_skill_dir(&empty_dir));
    }

    #[test]
    fn test_scan_parses_manifest_fields() {
        let dir = TempDir::new().unwrap();
        create_test_skill(dir.path(), "refactor", "Refactor code safely");

        let scanner = Scanner::new(dir.path().to_path_buf());
        let skills = scanner.scan_all().unwrap();

        assert_eq!(skills.len(), 1);
        let skill = &skills[0];
        assert_eq!(skill.manifest.name, "refactor");
        assert_eq!(skill.manifest.description, "Refactor code safely");
        assert_eq!(skill.manifest.version, "1.0.0");
        assert!(!skill.installed_at.is_empty());
    }

    /// End-to-end regression test: a manifest-less skill lives in source_root,
    /// and an agent dir holds a symlink pointing back at it (post-organize
    /// state). Scanning source_root first then the agent dir must NOT narrow
    /// compatible_agents from ["*"] to the agent's id. This is the exact
    /// scenario that core/src/ffi.rs guards with the source_root_ids HashSet.
    #[test]
    #[cfg(unix)]
    fn test_global_skill_symlinked_to_agent_keeps_wildcard() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("shared-skills");
        let agent_skills = dir.path().join(".codex").join("skills");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&agent_skills).unwrap();

        // Create a manifest-less skill in source_root (the "global" skill).
        let global_skill = source_root.join("code-review");
        fs::create_dir_all(&global_skill).unwrap();
        fs::write(global_skill.join("SKILL.md"), "# Code Review\n").unwrap();

        // Create a symlink inside the agent dir pointing back to it
        // (mimics the state after organize_skill moves + symlinks).
        let agent_link = agent_skills.join("code-review");
        std::os::unix::fs::symlink(&global_skill, &agent_link).unwrap();

        // 1) Scan source_root to discover global skills (this populates the
        //    source_root_ids set in the real FFI path).
        let scanner = Scanner::new(source_root.clone());
        let root_skills = scanner.scan_all().unwrap();
        let root_ids: HashSet<String> = root_skills.iter().map(|s| s.id.clone()).collect();
        assert!(root_ids.contains("code-review"));

        // 2) Scan the agent dir — the scanner follows the symlink and returns
        //    the same skill id.
        let agent_scanner = Scanner::new(source_root.clone());
        let mut agent_skills_found = agent_scanner.scan_path(&agent_skills).unwrap();
        assert_eq!(agent_skills_found.len(), 1);
        let mut skill = agent_skills_found.pop().unwrap();
        assert_eq!(skill.id, "code-review");

        // 3) Apply the same merge logic as ffi.rs scan_skills_for_agents:
        //    skip merge for skills already registered in source_root.
        let agent_id = "codex";
        if !root_ids.contains(&skill.id) {
            skill.manifest.merge_compatible_agent(agent_id);
        }

        // The global skill's wildcard must survive.
        assert_eq!(skill.manifest.compatible_agents, vec!["*".to_string()]);
        assert!(!skill.manifest.has_manifest);
    }

    /// End-to-end inverse test: a manifest-less skill discovered ONLY via an
    /// agent dir (never in source_root) must accumulate the agent into its
    /// compatible_agents, so the UI correctly marks it agent-scoped.
    #[test]
    fn test_agent_only_skill_accumulates_via_scan() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("shared-skills");
        let agent_skills = dir.path().join(".codex").join("skills");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&agent_skills).unwrap();

        // A manifest-less skill that lives ONLY in the agent dir.
        let agent_skill_dir = agent_skills.join("codex-only-thing");
        fs::create_dir_all(&agent_skill_dir).unwrap();
        fs::write(agent_skill_dir.join("SKILL.md"), "# Codex-only\n").unwrap();

        // source_root scan finds nothing global.
        let scanner = Scanner::new(source_root.clone());
        let root_skills = scanner.scan_all().unwrap();
        let root_ids: HashSet<String> = root_skills.iter().map(|s| s.id.clone()).collect();

        // Agent scan discovers the skill.
        let agent_scanner = Scanner::new(source_root.clone());
        let mut agent_skills_found = agent_scanner.scan_path(&agent_skills).unwrap();
        assert_eq!(agent_skills_found.len(), 1);
        let mut skill = agent_skills_found.pop().unwrap();

        // Apply the same merge guard as ffi.rs.
        let agent_id = "codex";
        if !root_ids.contains(&skill.id) {
            skill.manifest.merge_compatible_agent(agent_id);
        }

        // The agent-only skill should be scoped to its agent.
        assert_eq!(skill.manifest.compatible_agents, vec!["codex".to_string()]);
    }
}
