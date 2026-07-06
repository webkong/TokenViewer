use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::skills::models::{SkillEntry, SkillManifest};

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

        self.scan_path_inner(path, 0, &mut skills)?;
        Ok(skills)
    }

    fn scan_path_inner(
        &self,
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
                if let Ok(skill) = self.parse_skill_dir(&sub_path) {
                    skills.push(skill);
                }
                continue;
            }

            if depth < 2 {
                let _ = self.scan_path_inner(&sub_path, depth + 1, skills);
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

impl Scanner {
    /// Parse a skill directory into a SkillEntry.
    /// Reads manifest.json if present, otherwise generates a default manifest.
    fn parse_skill_dir(&self, path: &Path) -> Result<SkillEntry, String> {
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
                has_manifest: false,
            }
        };
        if manifest_path.is_file() {
            manifest.has_manifest = true;
        }

        let installed_at = chrono::Utc::now().to_rfc3339();

        Ok(SkillEntry {
            id,
            manifest,
            source_dir: path.to_string_lossy().to_string(),
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
