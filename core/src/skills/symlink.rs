use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::skills::agent_config::{expand_path, AgentConfig};
use crate::skills::models::LinkType;
use crate::skills::scanner::Scanner;

pub struct SymlinkManager {
    source_root: PathBuf,
}

impl SymlinkManager {
    pub fn new(source_root: PathBuf) -> Self {
        Self { source_root }
    }

    /// Create a symlink for a specific skill to a specific agent.
    pub fn create_skill_link(&self, agent: &AgentConfig, skill_id: &str) -> Result<(), String> {
        let source = self.source_root.join(skill_id);
        if !source.exists() {
            return Err(format!(
                "Skill source does not exist: {}. Organize this skill to the global library before linking it to an agent.",
                source.display()
            ));
        }

        let target_base = expand_path(&agent.skills_path)?;

        match agent.link_type {
            LinkType::Directory => self.link_directory(&source, &target_base, skill_id),
            LinkType::SingleFile => self.link_single_file(&source, &target_base),
            LinkType::Overlay => self.link_overlay(&source, &target_base, skill_id),
        }
    }

    /// Retarget every real symlink under the registered Agent Skill paths that
    /// points into `old_root`. This intentionally scans the filesystem instead
    /// of relying on linked_skills.json, because links created by other Skill
    /// managers must keep working when the shared library is moved.
    pub fn retarget_source_root_links(
        &self,
        old_root: &Path,
        new_root: &Path,
        agents: &[AgentConfig],
    ) -> Result<usize, String> {
        let old_root = normalize_path(old_root);
        let new_root = normalize_path(new_root);
        if old_root == new_root {
            return Ok(0);
        }

        let mut visited_links = HashSet::new();
        let mut updated = 0;
        for agent in agents {
            if agent.link_type == LinkType::SingleFile {
                continue;
            }
            let skills_path = expand_path(&agent.skills_path)?;
            if !skills_path.is_dir() {
                continue;
            }

            for entry in walkdir::WalkDir::new(&skills_path).follow_links(false) {
                let entry = entry.map_err(|e| {
                    format!(
                        "Failed to scan Agent Skill path {}: {}",
                        skills_path.display(),
                        e
                    )
                })?;
                if !entry.file_type().is_symlink() {
                    continue;
                }
                let link_path = entry.path().to_path_buf();
                if !visited_links.insert(link_path.clone()) {
                    continue;
                }
                let raw_target = fs::read_link(&link_path).map_err(|e| {
                    format!("Failed to read symlink {}: {}", link_path.display(), e)
                })?;
                let resolved_target = if raw_target.is_absolute() {
                    normalize_path(&raw_target)
                } else {
                    normalize_path(
                        &link_path
                            .parent()
                            .unwrap_or_else(|| Path::new("/"))
                            .join(&raw_target),
                    )
                };
                let relative_target = match resolved_target.strip_prefix(&old_root) {
                    Ok(relative) => relative,
                    Err(_) => continue,
                };
                let new_target = new_root.join(relative_target);
                // A link that was already broken before the directory move cannot
                // be repaired safely. Leave it untouched rather than guessing.
                if !new_target.exists() {
                    continue;
                }

                replace_symlink_atomically(&link_path, &new_target)?;
                updated += 1;
            }
        }
        Ok(updated)
    }

    /// Remove a symlink for a specific skill from a specific agent.
    pub fn remove_skill_link(&self, agent: &AgentConfig, skill_id: &str) -> Result<(), String> {
        let target_base = expand_path(&agent.skills_path)?;

        match agent.link_type {
            LinkType::Directory => {
                let link_path = target_base.join(skill_id);
                self.remove_link_if_exists(&link_path)
            }
            LinkType::SingleFile => {
                // SingleFile mode generates a merged file; remove it
                if target_base.is_symlink() || target_base.is_file() {
                    fs::remove_file(&target_base).map_err(|e| {
                        format!(
                            "Failed to remove single file {}: {}",
                            target_base.display(),
                            e
                        )
                    })?;
                }
                Ok(())
            }
            LinkType::Overlay => {
                let overlay_dir = target_base.join(skill_id);
                if overlay_dir.exists() {
                    for entry in fs::read_dir(&overlay_dir)
                        .map_err(|e| format!("Failed to read overlay dir: {}", e))?
                        .flatten()
                    {
                        let path = entry.path();
                        if path.is_symlink() {
                            remove_symlink(&path).ok();
                        }
                    }
                    // Remove the overlay directory if empty
                    fs::remove_dir(&overlay_dir).ok();
                }
                Ok(())
            }
        }
    }

    /// Rebuild a SingleFile agent from the skills that remain linked to it.
    pub fn rebuild_single_file(
        &self,
        agent: &AgentConfig,
        skill_ids: &[String],
    ) -> Result<(), String> {
        if agent.link_type != LinkType::SingleFile {
            return Err(format!("Agent {} is not a SingleFile agent", agent.source));
        }
        let target = expand_path(&agent.skills_path)?;
        if skill_ids.is_empty() {
            if target.is_symlink() || target.is_file() {
                fs::remove_file(&target).map_err(|e| {
                    format!("Failed to remove single file {}: {}", target.display(), e)
                })?;
            }
            return Ok(());
        }

        let sources = skill_ids
            .iter()
            .map(|skill_id| self.source_root.join(skill_id))
            .collect::<Vec<_>>();
        for source in &sources {
            if !source.is_dir() {
                return Err(format!("Skill source does not exist: {}", source.display()));
            }
        }
        self.write_single_file(&sources, &target)
    }

    /// Remove all symlinks for an agent (all linked skills).
    pub fn remove_all_links(&self, agent: &AgentConfig) -> Result<(), String> {
        let skill_ids: Vec<String> = agent.linked_skills.clone();
        for skill_id in &skill_ids {
            // Ignore individual errors during bulk removal
            self.remove_skill_link(agent, skill_id).ok();
        }
        Ok(())
    }

    // ── Link strategies ──

    /// Directory strategy: symlink the entire skill directory.
    fn link_directory(
        &self,
        source: &Path,
        target_base: &Path,
        skill_id: &str,
    ) -> Result<(), String> {
        let target = target_base.join(skill_id);

        // Backup existing non-symlink
        if target.exists() && !target.is_symlink() {
            let backup = target.with_extension("bak");
            fs::rename(&target, &backup).map_err(|e| {
                format!(
                    "Failed to backup {} to {}: {}",
                    target.display(),
                    backup.display(),
                    e
                )
            })?;
            eprintln!(
                "Backed up existing directory: {} -> {}",
                target.display(),
                backup.display()
            );
        } else if target.is_symlink() {
            remove_symlink(&target).map_err(|e| {
                format!(
                    "Failed to remove existing symlink {}: {}",
                    target.display(),
                    e
                )
            })?;
        }

        // Ensure parent directory exists
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dir {}: {}", parent.display(), e))?;
        }

        create_symlink(source, &target).map_err(|e| {
            format!(
                "Failed to create symlink {} -> {}: {}",
                target.display(),
                source.display(),
                e
            )
        })?;

        Ok(())
    }

    /// SingleFile strategy: merge all SKILL.md files into one.
    fn link_single_file(&self, source: &Path, target: &Path) -> Result<(), String> {
        self.write_single_file(&[source.to_path_buf()], target)
    }

    fn write_single_file(&self, sources: &[PathBuf], target: &Path) -> Result<(), String> {
        let mut content = String::new();

        for source in sources {
            for entry in walkdir::WalkDir::new(source)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_name() == "SKILL.md" {
                    let file_content = fs::read_to_string(entry.path())
                        .map_err(|e| format!("Failed to read {}: {}", entry.path().display(), e))?;
                    content.push_str(&file_content);
                    content.push_str("\n\n---\n\n");
                }
            }
        }

        // Ensure parent directory exists
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dir {}: {}", parent.display(), e))?;
        }

        // Backup existing file
        if target.exists() {
            let backup = target.with_extension("bak");
            fs::rename(target, &backup)
                .map_err(|e| format!("Failed to backup {}: {}", target.display(), e))?;
        }

        fs::write(target, content)
            .map_err(|e| format!("Failed to write merged file {}: {}", target.display(), e))?;

        Ok(())
    }

    /// Overlay strategy: symlink individual files into a subdirectory.
    fn link_overlay(
        &self,
        source: &Path,
        target_base: &Path,
        skill_id: &str,
    ) -> Result<(), String> {
        let overlay_dir = target_base.join(skill_id);
        fs::create_dir_all(&overlay_dir).map_err(|e| {
            format!(
                "Failed to create overlay dir {}: {}",
                overlay_dir.display(),
                e
            )
        })?;

        for entry in fs::read_dir(source)
            .map_err(|e| format!("Failed to read source dir {}: {}", source.display(), e))?
            .flatten()
        {
            let source_file = entry.path();
            let link_path = overlay_dir.join(entry.file_name());

            // Remove existing symlink if present
            if link_path.is_symlink() {
                remove_symlink(&link_path).ok();
            }

            // Don't overwrite real files
            if link_path.exists() && !link_path.is_symlink() {
                continue;
            }

            create_symlink(&source_file, &link_path).map_err(|e| {
                format!(
                    "Failed to create overlay symlink {} -> {}: {}",
                    link_path.display(),
                    source_file.display(),
                    e
                )
            })?;
        }

        Ok(())
    }

    /// Helper: remove a symlink path if it exists.
    fn remove_link_if_exists(&self, path: &Path) -> Result<(), String> {
        if path.is_symlink() {
            remove_symlink(path)
                .map_err(|e| format!("Failed to remove symlink {}: {}", path.display(), e))?;
        }
        Ok(())
    }

    /// Resolve the on-disk directory for `skill_id` under `agent`'s skills root.
    /// Tries the direct `skills_path/skill_id` join first (fast path), then falls
    /// back to scanning the agent's skills tree for a nested match (e.g. Codex's
    /// `.system/<skill>` layout). Generic across all agents.
    pub fn resolve_agent_skill_dir(
        &self,
        agent: &AgentConfig,
        skill_id: &str,
        scanner: &Scanner,
    ) -> Result<PathBuf, String> {
        let target_base = expand_path(&agent.skills_path)?;
        let direct_dir = target_base.join(skill_id);
        if direct_dir.exists() {
            return Ok(direct_dir);
        }

        let skills = scanner.scan_path(&target_base)?;
        skills
            .into_iter()
            .find(|skill| skill.id == skill_id)
            .map(|skill| PathBuf::from(skill.source_dir))
            .ok_or_else(|| {
                format!(
                    "Skill directory not found for {} under {}",
                    skill_id,
                    target_base.display()
                )
            })
    }

    /// Organize a single skill: move from agent directory to source_root, create symlink at original location.
    pub fn organize_skill(&self, agent: &AgentConfig, skill_id: &str) -> Result<(), String> {
        let target_base = expand_path(&agent.skills_path)?;
        let source_dir = target_base.join(skill_id);
        self.organize_skill_from_source(skill_id, &source_dir)
    }

    /// Organize a single skill, resolving its real on-disk location first via
    /// `resolve_agent_skill_dir` (so nested layouts like `.system/<skill>` work).
    pub fn organize_skill_resolved(
        &self,
        agent: &AgentConfig,
        skill_id: &str,
        scanner: &Scanner,
    ) -> Result<(), String> {
        let source_dir = self.resolve_agent_skill_dir(agent, skill_id, scanner)?;
        self.organize_skill_from_source(skill_id, &source_dir)
    }

    pub fn organize_skill_from_source(
        &self,
        skill_id: &str,
        source_dir: &Path,
    ) -> Result<(), String> {
        if !source_dir.exists() {
            return Err(format!(
                "Skill directory not found: {}",
                source_dir.display()
            ));
        }

        // Don't organize if it's already a symlink
        if source_dir.is_symlink() {
            return Ok(());
        }

        let dest_dir = self.source_root.join(skill_id);

        // If the shared skill already exists, keep it as the canonical copy and
        // replace the agent-side directory with a link.
        if dest_dir.exists() {
            let target_base = source_dir
                .parent()
                .ok_or_else(|| format!("Invalid skill path: {}", source_dir.display()))?;
            let target_name = source_dir
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("Invalid skill path: {}", source_dir.display()))?;
            return self.link_directory(&dest_dir, target_base, target_name);
        }

        // Create destination parent
        if let Some(parent) = dest_dir.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dir: {}", e))?;
        }

        // Move directory
        fs::rename(&source_dir, &dest_dir).map_err(|e| {
            format!(
                "Failed to move {} to {}: {}",
                source_dir.display(),
                dest_dir.display(),
                e
            )
        })?;

        // Create symlink at original location
        create_symlink(&dest_dir, &source_dir)
            .map_err(|e| format!("Failed to create symlink: {}", e))?;

        Ok(())
    }

    /// Organize all skills from all agents: move real directories to source_root, leave symlinks.
    pub fn organize_all(
        &self,
        agents: &[AgentConfig],
        scanner: &Scanner,
    ) -> Result<Vec<(String, String)>, String> {
        let mut organized = Vec::new();

        for agent in agents {
            let target_base = match expand_path(&agent.skills_path) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if !target_base.exists() {
                continue;
            }

            // Scan for skills in this agent's directory
            let skills = scanner.scan_path(&target_base)?;

            for skill in skills {
                let source_dir = PathBuf::from(&skill.source_dir);

                // Skip if already a symlink
                if source_dir.is_symlink() {
                    continue;
                }

                match self.organize_skill_resolved(agent, &skill.id, scanner) {
                    Ok(()) => {
                        organized.push((skill.id.clone(), agent.source.clone()));
                    }
                    Err(e) => {
                        eprintln!(
                            "Failed to organize skill {} from {}: {}",
                            skill.id, agent.source, e
                        );
                    }
                }
            }
        }

        Ok(organized)
    }

    /// Restore an organized skill back to its original agent directory.
    /// Removes the symlink at the agent's location, moves the real directory
    /// back from source_root, and removes broken symlinks from other agents.
    pub fn restore_skill(
        &self,
        skill_id: &str,
        source_agent: &AgentConfig,
        other_linked_agents: &[String],
    ) -> Result<(), String> {
        let source_dir = self.source_root.join(skill_id);

        if !source_dir.exists() {
            return Err(format!(
                "Source directory not found: {}",
                source_dir.display()
            ));
        }

        if source_dir.is_symlink() {
            return Err("Source directory is a symlink, not a real directory".to_string());
        }

        let target_base = expand_path(&source_agent.skills_path)?;
        let target_dir = target_base.join(skill_id);

        // Remove symlink at agent's location
        if target_dir.is_symlink() {
            remove_symlink(&target_dir).map_err(|e| {
                format!(
                    "Failed to remove symlink at {}: {}",
                    target_dir.display(),
                    e
                )
            })?;
        }

        // Remove broken symlinks from other agents
        for agent_id in other_linked_agents {
            if agent_id == &source_agent.source {
                continue;
            }
            // We need to find the agent's skills path, but we don't have the full AgentConfig here.
            // Instead, we'll handle this in the FFI layer which has access to the registry.
        }

        // Move directory back
        if target_dir.exists() {
            return Err(format!(
                "Target directory already exists: {}",
                target_dir.display()
            ));
        }

        if let Some(parent) = target_dir.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dir: {}", e))?;
        }

        fs::rename(&source_dir, &target_dir).map_err(|e| {
            format!(
                "Failed to move {} to {}: {}",
                source_dir.display(),
                target_dir.display(),
                e
            )
        })?;

        Ok(())
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(unix)]
fn create_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

fn remove_symlink(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        if path.is_dir() {
            fs::remove_dir(path)
        } else {
            fs::remove_file(path)
        }
    }
    #[cfg(not(windows))]
    {
        fs::remove_file(path)
    }
}

#[cfg(windows)]
fn create_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        std::os::windows::fs::symlink_dir(source, target)
    } else {
        std::os::windows::fs::symlink_file(source, target)
    }
}

fn replace_symlink_atomically(link_path: &Path, new_target: &Path) -> Result<(), String> {
    let file_name = link_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("skill");
    let temporary = link_path.with_file_name(format!(
        ".{}.tokenviewer-relink-{}",
        file_name,
        uuid::Uuid::new_v4()
    ));
    create_symlink(new_target, &temporary).map_err(|e| {
        format!(
            "Failed to prepare replacement symlink {} -> {}: {}",
            link_path.display(),
            new_target.display(),
            e
        )
    })?;
    #[cfg(windows)]
    if let Err(error) = remove_symlink(link_path) {
        let _ = remove_symlink(&temporary);
        return Err(format!(
            "Failed to remove old symlink {}: {}",
            link_path.display(),
            error
        ));
    }
    if let Err(error) = fs::rename(&temporary, link_path) {
        let _ = remove_symlink(&temporary);
        return Err(format!(
            "Failed to replace symlink {} -> {}: {}",
            link_path.display(),
            new_target.display(),
            error
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_directory_symlink() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        fs::create_dir_all(&source_root).unwrap();

        let skill_dir = source_root.join("code-review");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Review\n").unwrap();

        let target_base = dir.path().join("agent-dir");
        fs::create_dir_all(&target_base).unwrap();

        let manager = SymlinkManager::new(source_root.clone());

        let agent = AgentConfig::custom(
            "test-agent",
            "Test Agent",
            &target_base.to_string_lossy(),
            LinkType::Directory,
        );

        manager.create_skill_link(&agent, "code-review").unwrap();

        let link_path = target_base.join("code-review");
        assert!(link_path.is_symlink());
        assert_eq!(fs::read_link(&link_path).unwrap(), skill_dir);
    }

    #[test]
    #[cfg(unix)]
    fn retargets_registered_and_unregistered_links_after_source_root_move() {
        let dir = TempDir::new().unwrap();
        let old_root = dir.path().join("old-skills");
        let new_root = dir.path().join("new-skills");
        let agent_root = dir.path().join("agent-skills");
        for skill_id in ["registered", "external"] {
            let skill = old_root.join(skill_id);
            fs::create_dir_all(&skill).unwrap();
            fs::write(skill.join("SKILL.md"), format!("# {}\n", skill_id)).unwrap();
        }
        fs::create_dir_all(&agent_root).unwrap();
        std::os::unix::fs::symlink(old_root.join("registered"), agent_root.join("registered"))
            .unwrap();
        std::os::unix::fs::symlink(
            Path::new("../old-skills/external"),
            agent_root.join("external"),
        )
        .unwrap();
        std::os::unix::fs::symlink(old_root.join("missing"), agent_root.join("missing")).unwrap();

        fs::rename(&old_root, &new_root).unwrap();
        let agent = AgentConfig::custom(
            "test-agent",
            "Test Agent",
            &agent_root.to_string_lossy(),
            LinkType::Directory,
        );
        let manager = SymlinkManager::new(new_root.clone());

        let updated = manager
            .retarget_source_root_links(&old_root, &new_root, &[agent])
            .unwrap();

        assert_eq!(updated, 2);
        assert_eq!(
            fs::read_link(agent_root.join("registered")).unwrap(),
            new_root.join("registered")
        );
        assert_eq!(
            fs::read_link(agent_root.join("external")).unwrap(),
            new_root.join("external")
        );
        assert_eq!(
            fs::read_link(agent_root.join("missing")).unwrap(),
            old_root.join("missing")
        );
    }

    #[test]
    fn test_remove_directory_symlink() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        fs::create_dir_all(&source_root).unwrap();

        let skill_dir = source_root.join("code-review");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Review\n").unwrap();

        let target_base = dir.path().join("agent-dir");
        fs::create_dir_all(&target_base).unwrap();

        let manager = SymlinkManager::new(source_root);

        let agent = AgentConfig::custom(
            "test-agent",
            "Test Agent",
            &target_base.to_string_lossy(),
            LinkType::Directory,
        );

        manager.create_skill_link(&agent, "code-review").unwrap();
        assert!(target_base.join("code-review").is_symlink());

        manager.remove_skill_link(&agent, "code-review").unwrap();
        assert!(!target_base.join("code-review").exists());
    }

    #[test]
    fn test_single_file_merge() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        fs::create_dir_all(&source_root).unwrap();

        // Create two skills
        let skill1 = source_root.join("skill-a");
        fs::create_dir_all(&skill1).unwrap();
        fs::write(skill1.join("SKILL.md"), "Content A\n").unwrap();

        let skill2 = source_root.join("skill-b");
        fs::create_dir_all(&skill2).unwrap();
        fs::write(skill2.join("SKILL.md"), "Content B\n").unwrap();

        let target_file = dir.path().join("merged-instructions.md");

        let manager = SymlinkManager::new(source_root);

        let agent = AgentConfig::custom(
            "single-file-agent",
            "Single File Agent",
            &target_file.to_string_lossy(),
            LinkType::SingleFile,
        );

        manager.create_skill_link(&agent, "skill-a").unwrap();
        let content = fs::read_to_string(&target_file).unwrap();
        assert!(content.contains("Content A"));

        // Note: SingleFile link targets the file directly, not a subdirectory
    }

    #[test]
    fn test_backup_existing_directory() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        fs::create_dir_all(&source_root).unwrap();

        let skill_dir = source_root.join("code-review");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# New\n").unwrap();

        let target_base = dir.path().join("agent-dir");
        fs::create_dir_all(&target_base).unwrap();

        // Create existing real directory (not symlink)
        let existing_dir = target_base.join("code-review");
        fs::create_dir_all(&existing_dir).unwrap();
        fs::write(existing_dir.join("old-file.txt"), "old").unwrap();

        let manager = SymlinkManager::new(source_root);

        let agent = AgentConfig::custom(
            "test-agent",
            "Test Agent",
            &target_base.to_string_lossy(),
            LinkType::Directory,
        );

        manager.create_skill_link(&agent, "code-review").unwrap();

        // New symlink should exist
        assert!(target_base.join("code-review").is_symlink());

        // Backup should exist
        let backup = target_base.join("code-review.bak");
        assert!(backup.exists());
        assert!(backup.join("old-file.txt").exists());
    }

    #[test]
    fn test_organize_nested_skill_from_source() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        let nested_root = dir.path().join("agent-dir").join(".system");
        fs::create_dir_all(&nested_root).unwrap();

        let nested_skill = nested_root.join("code-review");
        fs::create_dir_all(&nested_skill).unwrap();
        fs::write(nested_skill.join("SKILL.md"), "# Review\n").unwrap();

        let manager = SymlinkManager::new(source_root.clone());
        manager
            .organize_skill_from_source("code-review", &nested_skill)
            .unwrap();

        let canonical_skill = source_root.join("code-review");
        assert!(canonical_skill.exists());
        assert!(nested_skill.is_symlink());
        assert_eq!(fs::read_link(&nested_skill).unwrap(), canonical_skill);
    }

    #[test]
    fn test_organize_existing_shared_skill_links_agent_copy() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        fs::create_dir_all(&source_root).unwrap();

        let canonical_skill = source_root.join("code-review");
        fs::create_dir_all(&canonical_skill).unwrap();
        fs::write(canonical_skill.join("SKILL.md"), "# Shared\n").unwrap();

        let agent_root = dir.path().join("agent-dir");
        let agent_skill = agent_root.join("code-review");
        fs::create_dir_all(&agent_skill).unwrap();
        fs::write(agent_skill.join("SKILL.md"), "# Agent Copy\n").unwrap();

        let manager = SymlinkManager::new(source_root.clone());
        manager
            .organize_skill_from_source("code-review", &agent_skill)
            .unwrap();

        assert!(agent_skill.is_symlink());
        assert_eq!(fs::read_link(&agent_skill).unwrap(), canonical_skill);
        assert!(agent_root.join("code-review.bak").exists());
    }

    /// Any agent with a nested skills layout (not just Codex's `.system/`) must be
    /// organizable via `organize_all`, which relies on the scanner-backed resolver.
    #[test]
    fn test_organize_all_handles_nested_layout_for_generic_agent() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("shared-skills");
        fs::create_dir_all(&source_root).unwrap();

        // Simulate a generic agent ("some-agent") that nests skills under a
        // subdirectory, analogous to Codex's `.system/<skill>` layout.
        let agent_root = dir.path().join("some-agent-dir");
        let nested_skill = agent_root.join("bundled").join("formatter");
        fs::create_dir_all(&nested_skill).unwrap();
        fs::write(nested_skill.join("SKILL.md"), "# Formatter\n").unwrap();

        let manager = SymlinkManager::new(source_root.clone());
        let scanner = Scanner::new(source_root.clone());

        let agent = AgentConfig::custom(
            "some-agent",
            "Some Agent",
            &agent_root.to_string_lossy(),
            LinkType::Directory,
        );

        let organized = manager.organize_all(&[agent], &scanner).unwrap();

        assert_eq!(
            organized,
            vec![("formatter".to_string(), "some-agent".to_string())]
        );
        let shared_skill = source_root.join("formatter");
        assert!(shared_skill.exists());
        assert!(nested_skill.is_symlink());
        assert_eq!(fs::read_link(&nested_skill).unwrap(), shared_skill);
    }
}
