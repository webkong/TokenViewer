use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};

use crate::skills::agent_registry::expand_path;
use crate::skills::models::{AgentConfig, LinkType};
use crate::skills::scanner::Scanner;

pub struct SymlinkManager {
    source_root: PathBuf,
}

impl SymlinkManager {
    pub fn new(source_root: PathBuf) -> Self {
        Self { source_root }
    }

    /// Create a symlink for a specific skill to a specific agent.
    pub fn create_skill_link(
        &self,
        agent: &AgentConfig,
        skill_id: &str,
    ) -> Result<(), String> {
        let source = self.source_root.join(skill_id);
        if !source.exists() {
            return Err(format!("Skill source does not exist: {}", source.display()));
        }

        let target_base = expand_path(&agent.skills_path)?;

        match agent.link_type {
            LinkType::Directory => self.link_directory(&source, &target_base, skill_id),
            LinkType::SingleFile => self.link_single_file(&source, &target_base),
            LinkType::Overlay => self.link_overlay(&source, &target_base, skill_id),
        }
    }

    /// Remove a symlink for a specific skill from a specific agent.
    pub fn remove_skill_link(
        &self,
        agent: &AgentConfig,
        skill_id: &str,
    ) -> Result<(), String> {
        let target_base = expand_path(&agent.skills_path)?;

        match agent.link_type {
            LinkType::Directory => {
                let link_path = target_base.join(skill_id);
                self.remove_link_if_exists(&link_path)
            }
            LinkType::SingleFile => {
                // SingleFile mode generates a merged file; remove it
                if target_base.is_symlink() || target_base.is_file() {
                    fs::remove_file(&target_base)
                        .map_err(|e| format!("Failed to remove single file {}: {}", target_base.display(), e))?;
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
                            fs::remove_file(&path).ok();
                        }
                    }
                    // Remove the overlay directory if empty
                    fs::remove_dir(&overlay_dir).ok();
                }
                Ok(())
            }
        }
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
            fs::rename(&target, &backup)
                .map_err(|e| format!("Failed to backup {} to {}: {}", target.display(), backup.display(), e))?;
            eprintln!("Backed up existing directory: {} -> {}", target.display(), backup.display());
        } else if target.is_symlink() {
            fs::remove_file(&target)
                .map_err(|e| format!("Failed to remove existing symlink {}: {}", target.display(), e))?;
        }

        // Ensure parent directory exists
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dir {}: {}", parent.display(), e))?;
        }

        unix_fs::symlink(source, &target)
            .map_err(|e| format!("Failed to create symlink {} -> {}: {}", target.display(), source.display(), e))?;

        Ok(())
    }

    /// SingleFile strategy: merge all SKILL.md files into one.
    fn link_single_file(&self, source: &Path, target: &Path) -> Result<(), String> {
        let mut content = String::new();

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
        fs::create_dir_all(&overlay_dir)
            .map_err(|e| format!("Failed to create overlay dir {}: {}", overlay_dir.display(), e))?;

        for entry in fs::read_dir(source)
            .map_err(|e| format!("Failed to read source dir {}: {}", source.display(), e))?
            .flatten()
        {
            let source_file = entry.path();
            let link_path = overlay_dir.join(entry.file_name());

            // Remove existing symlink if present
            if link_path.is_symlink() {
                fs::remove_file(&link_path).ok();
            }

            // Don't overwrite real files
            if link_path.exists() && !link_path.is_symlink() {
                continue;
            }

            unix_fs::symlink(&source_file, &link_path)
                .map_err(|e| format!("Failed to create overlay symlink {} -> {}: {}", link_path.display(), source_file.display(), e))?;
        }

        Ok(())
    }

    /// Helper: remove a symlink path if it exists.
    fn remove_link_if_exists(&self, path: &Path) -> Result<(), String> {
        if path.is_symlink() {
            fs::remove_file(path)
                .map_err(|e| format!("Failed to remove symlink {}: {}", path.display(), e))?;
        }
        Ok(())
    }

    /// Organize a single skill: move from agent directory to source_root, create symlink at original location.
    pub fn organize_skill(
        &self,
        agent: &AgentConfig,
        skill_id: &str,
    ) -> Result<(), String> {
        let target_base = expand_path(&agent.skills_path)?;
        let source_dir = target_base.join(skill_id);

        if !source_dir.exists() {
            return Err(format!("Skill directory not found: {}", source_dir.display()));
        }

        // Don't organize if it's already a symlink
        if source_dir.is_symlink() {
            return Ok(());
        }

        let dest_dir = self.source_root.join(skill_id);

        // If destination already exists, skip
        if dest_dir.exists() {
            return Ok(());
        }

        // Create destination parent
        if let Some(parent) = dest_dir.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dir: {}", e))?;
        }

        // Move directory
        fs::rename(&source_dir, &dest_dir)
            .map_err(|e| format!("Failed to move {} to {}: {}", source_dir.display(), dest_dir.display(), e))?;

        // Create symlink at original location
        unix_fs::symlink(&dest_dir, &source_dir)
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

                match self.organize_skill(agent, &skill.id) {
                    Ok(()) => {
                        organized.push((skill.id.clone(), agent.id.clone()));
                    }
                    Err(e) => {
                        eprintln!("Failed to organize skill {} from {}: {}", skill.id, agent.id, e);
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
            return Err(format!("Source directory not found: {}", source_dir.display()));
        }

        if source_dir.is_symlink() {
            return Err("Source directory is a symlink, not a real directory".to_string());
        }

        let target_base = expand_path(&source_agent.skills_path)?;
        let target_dir = target_base.join(skill_id);

        // Remove symlink at agent's location
        if target_dir.is_symlink() {
            fs::remove_file(&target_dir)
                .map_err(|e| format!("Failed to remove symlink at {}: {}", target_dir.display(), e))?;
        }

        // Remove broken symlinks from other agents
        for agent_id in other_linked_agents {
            if agent_id == &source_agent.id {
                continue;
            }
            // We need to find the agent's skills path, but we don't have the full AgentConfig here.
            // Instead, we'll handle this in the FFI layer which has access to the registry.
        }

        // Move directory back
        if target_dir.exists() {
            return Err(format!("Target directory already exists: {}", target_dir.display()));
        }

        if let Some(parent) = target_dir.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent dir: {}", e))?;
        }

        fs::rename(&source_dir, &target_dir)
            .map_err(|e| format!("Failed to move {} to {}: {}", source_dir.display(), target_dir.display(), e))?;

        Ok(())
    }
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
        assert_eq!(
            fs::read_link(&link_path).unwrap(),
            skill_dir
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
}
