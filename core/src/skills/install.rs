use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use git2::{build::RepoBuilder, Cred, FetchOptions, RemoteCallbacks};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::skills::models::{SkillInstallCandidate, SkillInstallRequest, SkillInstallResponse};
use crate::skills::scanner::Scanner;

pub struct SkillInstaller {
    source_root: PathBuf,
    config_dir: PathBuf,
}

impl SkillInstaller {
    pub fn new(source_root: PathBuf, config_dir: PathBuf) -> Self {
        Self {
            source_root,
            config_dir,
        }
    }

    pub fn install(&self, req: SkillInstallRequest) -> Result<SkillInstallResponse, String> {
        fs::create_dir_all(&self.source_root).map_err(|e| {
            format!(
                "Failed to create skills source root {}: {}",
                self.source_root.display(),
                e
            )
        })?;

        let temp_root =
            std::env::temp_dir().join(format!("tokenviewer-skill-install-{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_root)
            .map_err(|e| format!("Failed to create temp dir {}: {}", temp_root.display(), e))?;
        let _cleanup = TempDirCleanup(temp_root.clone());

        if req.source_type == "git" && req.selected_skill_ids.is_empty() {
            let raw = req
                .git_url
                .as_deref()
                .ok_or_else(|| "Enter a valid Git URL.".to_string())?;
            let git_source = GitInstallSource::parse(raw);
            if git_source.is_github_directory() {
                let candidates = match fetch_github_directory_candidates(
                    &git_source,
                    req.github_token.as_deref(),
                ) {
                    Ok(candidates) => candidates,
                    Err(GitHubDirectoryError::RateLimited) => {
                        discover_github_candidates_with_sparse_clone(
                            &git_source,
                            req.github_token.as_deref(),
                            &temp_root,
                        )?
                    }
                    Err(GitHubDirectoryError::Other(message)) => return Err(message),
                };
                return Ok(SkillInstallResponse::selection_required(candidates));
            }
        }

        let prepared = self.prepare_source(&req, &temp_root)?;
        let candidates = find_skill_dirs(&prepared.root_dir)?;
        if candidates.is_empty() {
            return Err("No SKILL.md or skill.md was found in the selected source.".to_string());
        }

        let candidate_models = candidates_to_models(&candidates)?;
        if req.selected_skill_ids.is_empty() && candidate_models.len() > 1 {
            return Ok(SkillInstallResponse::selection_required(candidate_models));
        }

        let selected = if req.selected_skill_ids.is_empty() {
            candidates
        } else {
            let selected_ids: HashSet<&str> =
                req.selected_skill_ids.iter().map(String::as_str).collect();
            candidates
                .into_iter()
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .map(|id| selected_ids.contains(id))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>()
        };

        if selected.is_empty() {
            return Err("No selected skills to install.".to_string());
        }

        let mut seen = HashSet::new();
        let mut planned = Vec::new();
        for skill_dir in selected {
            let skill_id = skill_dir
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("Invalid skill folder name: {}", skill_dir.display()))?
                .to_string();
            validate_skill_id(&skill_id)?;
            if !seen.insert(skill_id.clone()) {
                return Err(format!("Duplicate skill id in source: {}", skill_id));
            }

            let destination = self.source_root.join(&skill_id);
            if same_path(&skill_dir, &destination) {
                planned.push((skill_id, skill_dir, destination, true));
                continue;
            }
            if destination.exists() && !req.replace_existing {
                return Err(format!(
                    "Skill \"{}\" already exists. Enable replace to overwrite it.",
                    skill_id
                ));
            }
            planned.push((skill_id, skill_dir, destination, false));
        }

        let mut installed = Vec::new();
        for (skill_id, skill_dir, destination, already_in_place) in planned {
            if !already_in_place {
                if destination.exists() {
                    fs::remove_dir_all(&destination).map_err(|e| {
                        format!("Failed to replace {}: {}", destination.display(), e)
                    })?;
                }
                copy_dir_recursive(&skill_dir, &destination)?;
            }
            self.record_install(
                &skill_id,
                &req.source_type,
                &prepared.source_value,
                &destination,
            )?;
            installed.push(skill_id);
        }

        Ok(SkillInstallResponse::installed(installed))
    }

    fn prepare_source(
        &self,
        req: &SkillInstallRequest,
        temp_root: &Path,
    ) -> Result<PreparedSource, String> {
        match req.source_type.as_str() {
            "folder" => {
                let path = req
                    .path
                    .as_deref()
                    .ok_or_else(|| "Select a valid folder.".to_string())?;
                Ok(PreparedSource {
                    root_dir: expand_tilde(path),
                    source_value: path.to_string(),
                })
            }
            "zip" => {
                let path = req
                    .path
                    .as_deref()
                    .ok_or_else(|| "Select a valid ZIP file.".to_string())?;
                let status = Command::new("/usr/bin/ditto")
                    .args(["-x", "-k", path])
                    .arg(temp_root)
                    .status()
                    .map_err(|e| format!("Failed to extract ZIP: {}", e))?;
                if !status.success() {
                    return Err("Failed to extract ZIP archive.".to_string());
                }
                Ok(PreparedSource {
                    root_dir: temp_root.to_path_buf(),
                    source_value: path.to_string(),
                })
            }
            "git" => {
                let raw = req
                    .git_url
                    .as_deref()
                    .ok_or_else(|| "Enter a valid Git URL.".to_string())?;
                let git_source = GitInstallSource::parse(raw);

                if git_source.is_github_directory() && !req.selected_skill_ids.is_empty() {
                    for skill_id in &req.selected_skill_ids {
                        validate_skill_id(skill_id)?;
                    }
                    let clone_dir = temp_root.join("repo");
                    sparse_clone_selected_skills(
                        &git_source,
                        &req.selected_skill_ids,
                        &clone_dir,
                        req.github_token.as_deref(),
                    )?;
                    let root_dir = git_source
                        .subpath
                        .iter()
                        .fold(clone_dir, |path, component| path.join(component));
                    return Ok(PreparedSource {
                        root_dir,
                        source_value: raw.to_string(),
                    });
                }

                let cache_dir = git_cache_dir(raw);
                let clone_dir = cache_dir.join("repo");

                // First pass (candidate discovery) refreshes the cache. The
                // second pass (install selected skills) reuses it so selecting
                // one skill does not clone the same repository again.
                if req.selected_skill_ids.is_empty() && cache_dir.exists() {
                    let _ = fs::remove_dir_all(&cache_dir);
                }
                if !clone_dir.exists() {
                    fs::create_dir_all(&cache_dir).map_err(|e| {
                        format!(
                            "Failed to create Git install cache {}: {}",
                            cache_dir.display(),
                            e
                        )
                    })?;
                    let mut builder = RepoBuilder::new();
                    let mut fetch_options = FetchOptions::new();
                    fetch_options.depth(1);
                    if git_source.github_repo.is_some() {
                        if let Some(token) = validated_github_token(req.github_token.as_deref())? {
                            let mut callbacks = RemoteCallbacks::new();
                            callbacks.credentials(move |_url, _username, _allowed| {
                                Cred::userpass_plaintext("x-access-token", &token)
                            });
                            fetch_options.remote_callbacks(callbacks);
                        }
                    }
                    builder.fetch_options(fetch_options);
                    if let Some(branch) = git_source.branch.as_deref() {
                        builder.branch(branch);
                    }
                    builder
                        .clone(&git_source.clone_url, &clone_dir)
                        .map_err(|e| format!("Failed to clone Git repository: {}", e))?;
                }
                let root_dir = git_source
                    .subpath
                    .iter()
                    .fold(clone_dir, |path, component| path.join(component));
                Ok(PreparedSource {
                    root_dir,
                    source_value: raw.to_string(),
                })
            }
            other => Err(format!("Unsupported install source type: {}", other)),
        }
    }

    fn record_install(
        &self,
        skill_id: &str,
        source_type: &str,
        source: &str,
        destination: &Path,
    ) -> Result<(), String> {
        let metadata_path = self.metadata_path();
        if let Some(parent) = metadata_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create install metadata dir {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }

        let mut metadata = if metadata_path.is_file() {
            let data = fs::read_to_string(&metadata_path).unwrap_or_default();
            serde_json::from_str::<InstallMetadata>(&data).unwrap_or_default()
        } else {
            InstallMetadata::default()
        };

        metadata.skills.insert(
            skill_id.to_string(),
            InstallRecord {
                id: skill_id.to_string(),
                installed_at: chrono::Utc::now().to_rfc3339(),
                source_type: source_type.to_string(),
                source: source.to_string(),
                destination: destination.to_string_lossy().to_string(),
            },
        );

        self.write_install_metadata(&metadata_path, &metadata)
    }

    pub(crate) fn remove_install_record(&self, skill_id: &str) -> Result<(), String> {
        let metadata_path = self.metadata_path();
        if !metadata_path.is_file() {
            return Ok(());
        }

        let data = fs::read_to_string(&metadata_path).map_err(|e| {
            format!(
                "Failed to read install metadata {}: {}",
                metadata_path.display(),
                e
            )
        })?;
        let mut metadata = serde_json::from_str::<InstallMetadata>(&data).map_err(|e| {
            format!(
                "Failed to parse install metadata {}: {}",
                metadata_path.display(),
                e
            )
        })?;
        if metadata.skills.remove(skill_id).is_none() {
            return Ok(());
        }
        if metadata.skills.is_empty() {
            fs::remove_file(&metadata_path).map_err(|e| {
                format!(
                    "Failed to remove empty install metadata {}: {}",
                    metadata_path.display(),
                    e
                )
            })?;
            return Ok(());
        }
        self.write_install_metadata(&metadata_path, &metadata)
    }

    pub(crate) fn prune_missing_install_records(&self) -> Result<Vec<String>, String> {
        let metadata_path = self.metadata_path();
        if !metadata_path.is_file() {
            return Ok(Vec::new());
        }

        let data = fs::read_to_string(&metadata_path).map_err(|e| {
            format!(
                "Failed to read install metadata {}: {}",
                metadata_path.display(),
                e
            )
        })?;
        let mut metadata = serde_json::from_str::<InstallMetadata>(&data).map_err(|e| {
            format!(
                "Failed to parse install metadata {}: {}",
                metadata_path.display(),
                e
            )
        })?;
        let missing = metadata
            .skills
            .iter()
            .filter(|(_, record)| !Path::new(&record.destination).exists())
            .map(|(skill_id, _)| skill_id.clone())
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(missing);
        }
        for skill_id in &missing {
            metadata.skills.remove(skill_id);
        }
        if metadata.skills.is_empty() {
            fs::remove_file(&metadata_path).map_err(|e| {
                format!(
                    "Failed to remove empty install metadata {}: {}",
                    metadata_path.display(),
                    e
                )
            })?;
        } else {
            self.write_install_metadata(&metadata_path, &metadata)?;
        }
        Ok(missing)
    }

    fn write_install_metadata(
        &self,
        metadata_path: &Path,
        metadata: &InstallMetadata,
    ) -> Result<(), String> {
        let json = serde_json::to_string_pretty(metadata)
            .map_err(|e| format!("Failed to encode install metadata: {}", e))?;
        fs::write(metadata_path, json).map_err(|e| {
            format!(
                "Failed to write install metadata {}: {}",
                metadata_path.display(),
                e
            )
        })
    }

    fn metadata_path(&self) -> PathBuf {
        let home = self
            .config_dir
            .parent()
            .map(Path::to_path_buf)
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        home.join(".tokenviewer").join("install.json")
    }
}

struct PreparedSource {
    root_dir: PathBuf,
    source_value: String,
}

struct TempDirCleanup(PathBuf);

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Serialize, Deserialize)]
struct InstallMetadata {
    version: u32,
    #[serde(default)]
    skills: HashMap<String, InstallRecord>,
}

impl Default for InstallMetadata {
    fn default() -> Self {
        Self {
            version: 1,
            skills: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct InstallRecord {
    id: String,
    installed_at: String,
    source_type: String,
    source: String,
    destination: String,
}

struct GitInstallSource {
    clone_url: String,
    branch: Option<String>,
    subpath: Vec<String>,
    github_repo: Option<GitHubRepo>,
}

struct GitHubRepo {
    owner: String,
    name: String,
}

impl GitInstallSource {
    fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let prefix = "https://github.com/";
        if !trimmed.starts_with(prefix) {
            return Self {
                clone_url: trimmed.to_string(),
                branch: None,
                subpath: Vec::new(),
                github_repo: None,
            };
        }

        let without_prefix = trimmed[prefix.len()..].trim_end_matches('/');
        let parts: Vec<&str> = without_prefix.split('/').collect();
        if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Self {
                clone_url: trimmed.to_string(),
                branch: None,
                subpath: Vec::new(),
                github_repo: None,
            };
        }

        let owner = parts[0];
        let repo = parts[1].trim_end_matches(".git");
        let is_tree_url = parts.len() >= 4 && parts[2] == "tree";
        let branch = is_tree_url.then(|| parts[3].to_string());
        let subpath = if is_tree_url {
            parts
                .iter()
                .skip(4)
                .map(|part| (*part).to_string())
                .collect()
        } else {
            Vec::new()
        };

        Self {
            clone_url: format!("https://github.com/{}/{}.git", owner, repo),
            branch,
            subpath,
            github_repo: Some(GitHubRepo {
                owner: owner.to_string(),
                name: repo.to_string(),
            }),
        }
    }

    fn is_github_directory(&self) -> bool {
        self.github_repo.is_some() && !self.subpath.is_empty()
    }

    fn relative_subpath(&self) -> PathBuf {
        self.subpath
            .iter()
            .fold(PathBuf::new(), |path, component| path.join(component))
    }
}

enum GitHubDirectoryError {
    RateLimited,
    Other(String),
}

fn fetch_github_directory_candidates(
    source: &GitInstallSource,
    github_token: Option<&str>,
) -> Result<Vec<SkillInstallCandidate>, GitHubDirectoryError> {
    let repo = source.github_repo.as_ref().ok_or_else(|| {
        GitHubDirectoryError::Other("The GitHub repository URL is invalid.".to_string())
    })?;
    let branch = source.branch.as_deref().ok_or_else(|| {
        GitHubDirectoryError::Other("The GitHub branch is missing from the URL.".to_string())
    })?;
    let api_path = source
        .subpath
        .iter()
        .map(|component| percent_encode(component))
        .collect::<Vec<_>>()
        .join("/");
    let url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
        percent_encode(&repo.owner),
        percent_encode(&repo.name),
        api_path,
        percent_encode(branch)
    );

    let token = github_token
        .map(str::trim)
        .filter(|token| !token.is_empty());
    if token.is_some_and(|token| token.chars().any(|ch| matches!(ch, '\r' | '\n'))) {
        return Err(GitHubDirectoryError::Other(
            "The configured GitHub token is invalid.".to_string(),
        ));
    }

    let mut command = Command::new("/usr/bin/curl");
    command
        .args([
            "--fail-with-body",
            "--silent",
            "--show-error",
            "--location",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--user-agent",
            "TokenViewer",
            "--header",
            "@-",
            "--write-out",
            "\nTOKENVIEWER_HTTP_STATUS:%{http_code}",
            &url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| {
        GitHubDirectoryError::Other(format!("Failed to query the GitHub directory: {}", e))
    })?;
    let mut headers =
        String::from("Accept: application/vnd.github+json\nX-GitHub-Api-Version: 2022-11-28\n");
    if let Some(token) = token {
        headers.push_str("Authorization: Bearer ");
        headers.push_str(token);
        headers.push('\n');
    }
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(headers.as_bytes()).map_err(|e| {
            GitHubDirectoryError::Other(format!("Failed to authorize the GitHub request: {}", e))
        })?;
    }
    let output = child.wait_with_output().map_err(|e| {
        GitHubDirectoryError::Other(format!("Failed to query the GitHub directory: {}", e))
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (body, http_status) = stdout
        .rsplit_once("\nTOKENVIEWER_HTTP_STATUS:")
        .map(|(body, status)| (body, status.trim().parse::<u16>().ok()))
        .unwrap_or((&stdout, None));

    if !output.status.success() {
        let api_message = github_api_error_message(body.as_bytes());
        let is_rate_limited = http_status == Some(429)
            || (http_status == Some(403)
                && api_message
                    .as_deref()
                    .is_some_and(|message| message.to_ascii_lowercase().contains("rate limit")));
        if is_rate_limited {
            return Err(GitHubDirectoryError::RateLimited);
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = api_message
            .or_else(|| (!stderr.is_empty()).then_some(stderr))
            .unwrap_or_else(|| format!("curl exited with {}", output.status));
        return Err(GitHubDirectoryError::Other(format!(
            "Failed to query the GitHub directory: {}",
            detail
        )));
    }

    github_candidates_from_json(body.as_bytes()).map_err(GitHubDirectoryError::Other)
}

fn discover_github_candidates_with_sparse_clone(
    source: &GitInstallSource,
    github_token: Option<&str>,
    temp_root: &Path,
) -> Result<Vec<SkillInstallCandidate>, String> {
    let clone_dir = temp_root.join("repo");
    let relative_root = source.relative_subpath();
    sparse_clone_paths(source, &[relative_root.clone()], &clone_dir, github_token)?;
    let root_dir = clone_dir.join(&relative_root);
    direct_directory_candidates(&root_dir, &relative_root)
}

fn direct_directory_candidates(
    root: &Path,
    relative_root: &Path,
) -> Result<Vec<SkillInstallCandidate>, String> {
    let entries = fs::read_dir(root)
        .map_err(|e| format!("Failed to read GitHub directory {}: {}", root.display(), e))?;
    let mut candidates = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(id) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        validate_skill_id(id)?;
        candidates.push(SkillInstallCandidate {
            id: id.to_string(),
            source_dir: relative_root.join(id).to_string_lossy().to_string(),
        });
    }
    candidates.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    if candidates.is_empty() {
        return Err("No skill directories were found in the GitHub directory.".to_string());
    }
    Ok(candidates)
}

fn github_candidates_from_json(data: &[u8]) -> Result<Vec<SkillInstallCandidate>, String> {
    let value: serde_json::Value = serde_json::from_slice(data)
        .map_err(|e| format!("Failed to decode the GitHub directory response: {}", e))?;
    let entries = value.as_array().ok_or_else(|| {
        let detail = value
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("GitHub did not return a directory listing");
        format!("Failed to query the GitHub directory: {}", detail)
    })?;

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries {
        if entry.get("type").and_then(serde_json::Value::as_str) != Some("dir") {
            continue;
        }
        let Some(id) = entry.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(path) = entry.get("path").and_then(serde_json::Value::as_str) else {
            continue;
        };
        validate_skill_id(id)?;
        if !seen.insert(id.to_string()) {
            return Err(format!("Duplicate skill id in source: {}", id));
        }
        candidates.push(SkillInstallCandidate {
            id: id.to_string(),
            source_dir: path.to_string(),
        });
    }
    candidates.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    if candidates.is_empty() {
        return Err("No skill directories were found in the GitHub directory.".to_string());
    }
    Ok(candidates)
}

fn github_api_error_message(data: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(data)
        .ok()?
        .get("message")?
        .as_str()
        .map(str::to_string)
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
        }
    }
    encoded
}

fn sparse_clone_selected_skills(
    source: &GitInstallSource,
    selected_skill_ids: &[String],
    clone_dir: &Path,
    github_token: Option<&str>,
) -> Result<(), String> {
    let selected_paths = selected_skill_ids
        .iter()
        .map(|skill_id| source.relative_subpath().join(skill_id))
        .collect::<Vec<_>>();
    sparse_clone_paths(source, &selected_paths, clone_dir, github_token)
}

fn sparse_clone_paths(
    source: &GitInstallSource,
    paths: &[PathBuf],
    clone_dir: &Path,
    github_token: Option<&str>,
) -> Result<(), String> {
    let mut clone = Command::new("/usr/bin/git");
    clone
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .args([
            "-c",
            "http.lowSpeedLimit=1024",
            "-c",
            "http.lowSpeedTime=30",
            "clone",
            "--depth=1",
            "--filter=blob:none",
            "--sparse",
        ]);
    if source.github_repo.is_some() {
        configure_github_auth(&mut clone, github_token)?;
    }
    if let Some(branch) = source.branch.as_deref() {
        clone.args(["--branch", branch]);
    }
    let output = clone
        .arg(&source.clone_url)
        .arg(clone_dir)
        .output()
        .map_err(|e| format!("Failed to clone the selected skills: {}", e))?;
    if !output.status.success() {
        return Err(command_failure(
            "Failed to clone the selected skills",
            &output,
        ));
    }

    let mut checkout = Command::new("/usr/bin/git");
    checkout
        .env("GIT_TERMINAL_PROMPT", "0")
        .arg("-C")
        .arg(clone_dir)
        .args(["sparse-checkout", "set"]);
    for path in paths {
        checkout.arg(path);
    }
    let output = checkout
        .output()
        .map_err(|e| format!("Failed to select skill directories: {}", e))?;
    if !output.status.success() {
        return Err(command_failure(
            "Failed to select skill directories",
            &output,
        ));
    }
    Ok(())
}

fn configure_github_auth(command: &mut Command, token: Option<&str>) -> Result<(), String> {
    if let Some(token) = validated_github_token(token)? {
        // Pass the credential through the child environment rather than argv so it is not
        // exposed in the process list. Scope the header to GitHub HTTPS requests only.
        command
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
            .env(
                "GIT_CONFIG_VALUE_0",
                format!("Authorization: Bearer {}", token),
            );
    }
    Ok(())
}

fn validated_github_token(token: Option<&str>) -> Result<Option<String>, String> {
    let token = token.map(str::trim).filter(|token| !token.is_empty());
    if token.is_some_and(|token| token.chars().any(|ch| matches!(ch, '\r' | '\n'))) {
        return Err("The configured GitHub token is invalid.".to_string());
    }
    Ok(token.map(str::to_string))
}

fn command_failure(context: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("{}: process exited with {}", context, output.status)
    } else {
        format!("{}: {}", context, stderr)
    }
}

fn find_skill_dirs(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut skills = Vec::new();
    if !root.exists() || !root.is_dir() {
        return Err(format!(
            "Source directory does not exist: {}",
            root.display()
        ));
    }
    find_skill_dirs_inner(root, 0, &mut skills)?;
    skills.sort();
    Ok(skills)
}

fn find_skill_dirs_inner(
    path: &Path,
    depth: usize,
    skills: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if Scanner::validate_skill_dir(path) {
        skills.push(path.to_path_buf());
        return Ok(());
    }
    if depth >= 2 {
        return Ok(());
    }

    let entries =
        fs::read_dir(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    for entry in entries.filter_map(Result::ok) {
        let child = entry.path();
        if !child.is_dir() {
            continue;
        }
        if let Some(name) = child.file_name().and_then(|name| name.to_str()) {
            if matches!(name, ".git" | "node_modules" | "target" | "DerivedData") {
                continue;
            }
        }
        let _ = find_skill_dirs_inner(&child, depth + 1, skills);
    }
    Ok(())
}

fn candidates_to_models(paths: &[PathBuf]) -> Result<Vec<SkillInstallCandidate>, String> {
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for path in paths {
        let id = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("Invalid skill folder name: {}", path.display()))?
            .to_string();
        validate_skill_id(&id)?;
        if !seen.insert(id.clone()) {
            return Err(format!("Duplicate skill id in source: {}", id));
        }
        candidates.push(SkillInstallCandidate {
            id,
            source_dir: path.to_string_lossy().to_string(),
        });
    }
    Ok(candidates)
}

fn validate_skill_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(format!("Invalid skill folder name: {}", id));
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|e| format!("Failed to create {}: {}", destination.display(), e))?;
    for entry in walkdir::WalkDir::new(source).min_depth(1) {
        let entry = entry.map_err(|e| format!("Failed to copy {}: {}", source.display(), e))?;
        let path = entry.path();
        let relative = path.strip_prefix(source).map_err(|e| e.to_string())?;
        if relative
            .components()
            .any(|component| component.as_os_str() == ".git")
        {
            continue;
        }
        let target = destination.join(relative);
        let file_type = entry.file_type();
        if file_type.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| format!("Failed to create {}: {}", target.display(), e))?;
        } else if file_type.is_symlink() {
            let link_target = fs::read_link(path)
                .map_err(|e| format!("Failed to read symlink {}: {}", path.display(), e))?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link_target, &target)
                .map_err(|e| format!("Failed to copy symlink {}: {}", target.display(), e))?;
        } else if file_type.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
            }
            fs::copy(path, &target).map_err(|e| {
                format!(
                    "Failed to copy {} to {}: {}",
                    path.display(),
                    target.display(),
                    e
                )
            })?;
        }
    }
    Ok(())
}

fn same_path(lhs: &Path, rhs: &Path) -> bool {
    let lhs = fs::canonicalize(lhs).unwrap_or_else(|_| lhs.to_path_buf());
    let rhs = fs::canonicalize(rhs).unwrap_or_else(|_| rhs.to_path_buf());
    lhs == rhs
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn git_cache_dir(source: &str) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.trim().hash(&mut hasher);
    std::env::temp_dir()
        .join("tokenviewer-skill-install-cache")
        .join(format!("{:016x}", hasher.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn folder_with_multiple_skills_requires_selection() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("global");
        let source = dir.path().join("bundle");
        fs::create_dir_all(source.join("a")).unwrap();
        fs::create_dir_all(source.join("b")).unwrap();
        fs::write(source.join("a").join("SKILL.md"), "# A\n").unwrap();
        fs::write(source.join("b").join("SKILL.md"), "# B\n").unwrap();

        let installer = SkillInstaller::new(source_root, dir.path().join(".agents"));
        let response = installer
            .install(SkillInstallRequest {
                source_type: "folder".to_string(),
                path: Some(source.to_string_lossy().to_string()),
                git_url: None,
                github_token: None,
                replace_existing: false,
                selected_skill_ids: Vec::new(),
            })
            .unwrap();

        assert_eq!(response.status, "selection_required");
        assert_eq!(response.candidates.len(), 2);
    }

    #[test]
    fn installs_selected_skills_and_records_metadata() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("global");
        let source = dir.path().join("bundle");
        fs::create_dir_all(source.join("a")).unwrap();
        fs::create_dir_all(source.join("b")).unwrap();
        fs::write(source.join("a").join("SKILL.md"), "# A\n").unwrap();
        fs::write(source.join("b").join("SKILL.md"), "# B\n").unwrap();

        let installer = SkillInstaller::new(source_root.clone(), dir.path().join(".agents"));
        let response = installer
            .install(SkillInstallRequest {
                source_type: "folder".to_string(),
                path: Some(source.to_string_lossy().to_string()),
                git_url: None,
                github_token: None,
                replace_existing: false,
                selected_skill_ids: vec!["b".to_string()],
            })
            .unwrap();

        assert_eq!(response.status, "installed");
        assert_eq!(response.installed_skill_ids, vec!["b".to_string()]);
        assert!(!source_root.join("a").exists());
        assert!(source_root.join("b").join("SKILL.md").exists());
        assert!(dir
            .path()
            .join(".tokenviewer")
            .join("install.json")
            .exists());
    }

    #[test]
    fn selected_git_install_reuses_discovery_cache() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("global");
        let cache_source = "https://example.test/repo.git";
        let cache_dir = git_cache_dir(cache_source);
        let _ = fs::remove_dir_all(&cache_dir);
        fs::create_dir_all(cache_dir.join("repo").join("a")).unwrap();
        fs::write(cache_dir.join("repo").join("a").join("SKILL.md"), "# A\n").unwrap();

        let installer = SkillInstaller::new(source_root.clone(), dir.path().join(".agents"));
        let response = installer
            .install(SkillInstallRequest {
                source_type: "git".to_string(),
                path: None,
                git_url: Some(cache_source.to_string()),
                github_token: None,
                replace_existing: false,
                selected_skill_ids: vec!["a".to_string()],
            })
            .unwrap();

        assert_eq!(response.status, "installed");
        assert!(source_root.join("a").join("SKILL.md").exists());
        let _ = fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn parses_github_directory_url_for_api_and_sparse_checkout() {
        let source = GitInstallSource::parse("https://github.com/stablyai/orca/tree/main/skills");

        assert_eq!(source.clone_url, "https://github.com/stablyai/orca.git");
        assert_eq!(source.branch.as_deref(), Some("main"));
        assert_eq!(source.subpath, vec!["skills"]);
        assert!(source.is_github_directory());
        let repo = source.github_repo.unwrap();
        assert_eq!(repo.owner, "stablyai");
        assert_eq!(repo.name, "orca");
    }

    #[test]
    fn parses_github_repository_url_for_authenticated_clone() {
        let source = GitInstallSource::parse("https://github.com/acme/private-skills.git");

        assert_eq!(
            source.clone_url,
            "https://github.com/acme/private-skills.git"
        );
        assert!(source.github_repo.is_some());
        assert!(!source.is_github_directory());
    }

    #[test]
    fn git_auth_uses_environment_instead_of_process_arguments() {
        let mut command = Command::new("/usr/bin/git");
        command
            .arg("clone")
            .arg("https://github.com/acme/private.git");

        configure_github_auth(&mut command, Some("secret-token")).unwrap();

        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>();
        assert!(args.iter().all(|arg| !arg.contains("secret-token")));
        let env = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().to_string(),
                    value.map(|value| value.to_string_lossy().to_string()),
                )
            })
            .collect::<HashMap<_, _>>();
        assert_eq!(
            env.get("GIT_CONFIG_VALUE_0").and_then(Option::as_deref),
            Some("Authorization: Bearer secret-token")
        );
    }

    #[test]
    fn github_directory_response_only_exposes_directories() {
        let candidates = github_candidates_from_json(
            br#"[
                {"name":"zeta","path":"skills/zeta","type":"dir"},
                {"name":"README.md","path":"skills/README.md","type":"file"},
                {"name":"alpha","path":"skills/alpha","type":"dir"}
            ]"#,
        )
        .unwrap();

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].id, "alpha");
        assert_eq!(candidates[0].source_dir, "skills/alpha");
        assert_eq!(candidates[1].id, "zeta");
    }

    #[test]
    fn sparse_fallback_exposes_only_direct_directories() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("skills");
        fs::create_dir_all(root.join("zeta")).unwrap();
        fs::create_dir_all(root.join("alpha").join("nested")).unwrap();
        fs::write(root.join("README.md"), "not a skill directory\n").unwrap();

        let candidates = direct_directory_candidates(&root, Path::new("skills")).unwrap();

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].id, "alpha");
        assert_eq!(candidates[0].source_dir, "skills/alpha");
        assert_eq!(candidates[1].id, "zeta");
    }
}
