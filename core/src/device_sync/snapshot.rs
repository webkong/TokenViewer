use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::skills::SkillsCore;

use super::archive::{build_skill_archive, ArchiveWarning};
use super::crypto::{canonical_json, sha256_hex};
use super::models::{
    valid_environment_name, valid_segment, AgentLinkRecord, ComponentSummary, DeviceSyncConfig,
    DeviceSyncError, DeviceSyncErrorCode, EnvironmentRecord, GitRepositoryHint, Hlc,
    PreferencesRecord, RecordMetadata, ScopeMode, SkillFileRecord, SkillRecord, SnapshotManifest,
    SnapshotPayload, SnapshotRecords, SyncComponent, Tombstone, MAX_ENVIRONMENT_VALUE_BYTES,
    MAX_ENVIRONMENT_VARIABLES,
};
use super::skill_env::SkillEnvironmentStore;

#[derive(Debug, Clone)]
pub struct SnapshotBuildRequest {
    pub snapshot_id: String,
    pub vault_id: String,
    pub device_id: String,
    pub parent_ids: Vec<String>,
    pub enabled_agent_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SnapshotBuildResult {
    pub payload: SnapshotPayload,
    pub local_fingerprint: String,
    pub warnings: Vec<ArchiveWarning>,
}

pub fn build_snapshot(
    skills: &SkillsCore,
    home_dir: &Path,
    config: &DeviceSyncConfig,
    request: &SnapshotBuildRequest,
    baseline: Option<&SnapshotPayload>,
) -> Result<SnapshotBuildResult, DeviceSyncError> {
    build_snapshot_with_clock(
        skills,
        home_dir,
        config,
        request,
        Hlc::now(request.device_id.clone()),
        baseline,
    )
}

pub fn build_snapshot_with_clock(
    skills: &SkillsCore,
    home_dir: &Path,
    config: &DeviceSyncConfig,
    request: &SnapshotBuildRequest,
    clock: Hlc,
    baseline: Option<&SnapshotPayload>,
) -> Result<SnapshotBuildResult, DeviceSyncError> {
    config.validate()?;
    if request.parent_ids.len() > 2
        || !valid_segment(&request.snapshot_id)
        || !valid_segment(&request.vault_id)
        || !valid_segment(&request.device_id)
    {
        return Err(DeviceSyncError::invalid_config("snapshot identity"));
    }
    if clock.device_id != request.device_id || clock.wall_ms < 0 {
        return Err(DeviceSyncError::invalid_config("snapshot clock"));
    }

    let scanned = skills
        .scanner
        .scan_all()
        .map_err(|_| DeviceSyncError::apply_failed("unable to scan Skills"))?;
    let mut scanned_by_id = BTreeMap::new();
    for skill in scanned {
        if valid_segment(&skill.id) {
            scanned_by_id.insert(skill.id.clone(), skill);
        }
    }
    let selected_skill_ids = select_skill_ids(config, &scanned_by_id);
    let mut archive = Vec::new();
    let mut archive_records = Vec::new();
    let mut archive_warnings = Vec::new();
    let mut expanded_bytes = 0u64;
    if config.content_source == "cloud" && config.has_component(SyncComponent::Skills) {
        let built = build_skill_archive(skills.scanner.source_root(), &selected_skill_ids)?;
        archive = built.bytes;
        archive_records = built.records;
        expanded_bytes = built.expanded_bytes;
        archive_warnings = built.warnings;
    }

    let mut records = SnapshotRecords::default();
    let mut components = BTreeMap::new();

    let skill_records = build_skill_records(
        &selected_skill_ids,
        &archive_records,
        &clock,
        &request.device_id,
    );
    if config.has_component(SyncComponent::Skills) {
        components.insert(
            "skills".to_string(),
            ComponentSummary {
                sha256: sha256_hex(&archive),
                bytes: archive.len() as u64,
                entries: archive_records.len() as u64,
                records: skill_records.len() as u64,
            },
        );
        records.skills = skill_records;
    }

    if config.has_component(SyncComponent::AgentLinks) {
        records.agent_links = build_agent_link_records(
            skills,
            &selected_skill_ids,
            &clock,
            &request.device_id,
            config.content_source == "git",
        );
        components.insert(
            "agent_links".to_string(),
            component_summary(&records.agent_links, records.agent_links.len() as u64)?,
        );
    }

    if config.has_component(SyncComponent::SkillEnv) {
        records.skill_env = build_environment_records(
            skills,
            home_dir,
            config,
            &selected_skill_ids,
            &scanned_by_id,
            &clock,
            &request.device_id,
        )?;
        components.insert(
            "skill_env".to_string(),
            component_summary(&records.skill_env, records.skill_env.len() as u64)?,
        );
    }

    if config.has_component(SyncComponent::Preferences) {
        let enabled_agent_ids = normalize_agent_ids(&request.enabled_agent_ids)?;
        records.preferences = Some(PreferencesRecord {
            enabled_agent_ids,
            metadata: metadata("preferences", &clock, &request.device_id),
        });
        components.insert(
            "preferences".to_string(),
            component_summary(&records.preferences, 1)?,
        );
    }

    let mut manifest = SnapshotManifest::new(
        request.snapshot_id.clone(),
        request.vault_id.clone(),
        request.device_id.clone(),
        request.parent_ids.clone(),
        config.content_source.clone(),
        components,
    );
    manifest.clock = clock.clone();
    manifest.records = records;
    manifest.limits.archive_bytes = archive.len() as u64;
    manifest.limits.expanded_bytes = expanded_bytes;
    manifest.git_repository = (config.content_source == "git")
        .then(|| git_repository_hint(skills))
        .flatten();
    if let Some(baseline) = baseline {
        preserve_unchanged_metadata(&mut manifest.records, &baseline.manifest.records);
        append_tombstones(&mut manifest, baseline, config, &clock, &request.device_id);
        refresh_component_summaries(&mut manifest)?;
    }

    let local_fingerprint = local_fingerprint(&manifest)?;
    Ok(SnapshotBuildResult {
        payload: SnapshotPayload { manifest, archive },
        local_fingerprint,
        warnings: archive_warnings,
    })
}

fn preserve_unchanged_metadata(current: &mut SnapshotRecords, baseline: &SnapshotRecords) {
    for record in &mut current.skills {
        if let Some(previous) = baseline
            .skills
            .iter()
            .find(|previous| previous.skill_id == record.skill_id && !previous.metadata.tombstone)
        {
            if previous.files == record.files {
                record.metadata = previous.metadata.clone();
            }
        }
    }
    for record in &mut current.agent_links {
        if let Some(previous) = baseline.agent_links.iter().find(|previous| {
            previous.agent_id == record.agent_id
                && previous.skill_id == record.skill_id
                && !previous.metadata.tombstone
        }) {
            record.metadata = previous.metadata.clone();
        }
    }
    for record in &mut current.skill_env {
        if let Some(previous) = baseline
            .skill_env
            .iter()
            .find(|previous| previous.name == record.name && !previous.metadata.tombstone)
        {
            if previous.value == record.value
                && previous.referenced_by_skill_ids == record.referenced_by_skill_ids
            {
                record.metadata = previous.metadata.clone();
            }
        }
    }
    if let (Some(current), Some(previous)) =
        (current.preferences.as_mut(), baseline.preferences.as_ref())
    {
        if !previous.metadata.tombstone && previous.enabled_agent_ids == current.enabled_agent_ids {
            current.metadata = previous.metadata.clone();
        }
    }
}

pub fn local_fingerprint(manifest: &SnapshotManifest) -> Result<String, DeviceSyncError> {
    // HLC metadata changes every time a snapshot is built. A local baseline
    // must describe the portable state, otherwise a push can never reuse its
    // preview and a pull always appears to update an unchanged record.
    let value = serde_json::json!({
        "content_source": &manifest.content_source,
        "components": manifest.components.keys().collect::<Vec<_>>(),
        "git_repository": &manifest.git_repository,
        "records": {
            "skills": manifest.records.skills.iter().filter(|record| !record.metadata.tombstone).map(|record| serde_json::json!({
                "skill_id": &record.skill_id,
                "files": &record.files,
                "tombstone": record.metadata.tombstone,
            })).collect::<Vec<_>>(),
            "agent_links": manifest.records.agent_links.iter().filter(|record| !record.metadata.tombstone).map(|record| serde_json::json!({
                "agent_id": &record.agent_id,
                "skill_id": &record.skill_id,
                "tombstone": record.metadata.tombstone,
            })).collect::<Vec<_>>(),
            "skill_env": manifest.records.skill_env.iter().filter(|record| !record.metadata.tombstone).map(|record| serde_json::json!({
                "name": &record.name,
                "value": &record.value,
                "referenced_by_skill_ids": &record.referenced_by_skill_ids,
                "tombstone": record.metadata.tombstone,
            })).collect::<Vec<_>>(),
            "preferences": manifest.records.preferences.as_ref().filter(|record| !record.metadata.tombstone).map(|record| serde_json::json!({
                "enabled_agent_ids": &record.enabled_agent_ids,
                "tombstone": record.metadata.tombstone,
            })),
        }
    });
    Ok(sha256_hex(&canonical_json(&value)?))
}

fn select_skill_ids(
    config: &DeviceSyncConfig,
    scanned: &BTreeMap<String, crate::skills::models::SkillEntry>,
) -> Vec<String> {
    if !config.has_component(SyncComponent::Skills)
        && !config.has_component(SyncComponent::SkillEnv)
        && config.content_source != "git"
    {
        return Vec::new();
    }
    match config.skill_scope.mode {
        ScopeMode::All => scanned.keys().cloned().collect(),
        ScopeMode::Selected => config
            .skill_scope
            .skill_ids
            .iter()
            .filter(|id| scanned.contains_key(*id))
            .cloned()
            .collect(),
    }
}

fn build_skill_records(
    skill_ids: &[String],
    archive_records: &[SkillFileRecord],
    clock: &Hlc,
    device_id: &str,
) -> Vec<SkillRecord> {
    skill_ids
        .iter()
        .filter_map(|skill_id| {
            let prefix = format!("skills/{}/", skill_id);
            let files = archive_records
                .iter()
                .filter(|record| {
                    record.path == format!("skills/{}", skill_id)
                        || record.path.starts_with(&prefix)
                })
                .cloned()
                .collect::<Vec<_>>();
            (!files.is_empty()).then(|| SkillRecord {
                skill_id: skill_id.clone(),
                metadata: metadata(&format!("skill/{}", skill_id), clock, device_id),
                files,
            })
        })
        .collect()
}

fn build_agent_link_records(
    skills: &SkillsCore,
    selected_skill_ids: &[String],
    clock: &Hlc,
    device_id: &str,
    include_unselected: bool,
) -> Vec<AgentLinkRecord> {
    let selected = selected_skill_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut records = Vec::new();
    for agent in skills.registry.all() {
        if !valid_segment(&agent.source) {
            continue;
        }
        for skill_id in agent.linked_skills {
            if !valid_segment(&skill_id) || (!include_unselected && !selected.contains(&skill_id)) {
                continue;
            }
            records.push(AgentLinkRecord {
                agent_id: agent.source.clone(),
                skill_id: skill_id.clone(),
                metadata: metadata(
                    &format!("agent-link/{}/{}", agent.source, skill_id),
                    clock,
                    device_id,
                ),
            });
        }
    }
    records.sort_by(|left, right| {
        left.agent_id
            .cmp(&right.agent_id)
            .then_with(|| left.skill_id.cmp(&right.skill_id))
    });
    records
}

fn build_environment_records(
    skills: &SkillsCore,
    home_dir: &Path,
    config: &DeviceSyncConfig,
    selected_skill_ids: &[String],
    scanned: &BTreeMap<String, crate::skills::models::SkillEntry>,
    clock: &Hlc,
    device_id: &str,
) -> Result<Vec<EnvironmentRecord>, DeviceSyncError> {
    let selected = selected_skill_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut references = BTreeMap::<String, BTreeSet<String>>::new();
    for skill_id in selected {
        if let Some(skill) = scanned.get(&skill_id) {
            for variable in &skill.manifest.environment_variables {
                if valid_environment_name(&variable.name) {
                    references
                        .entry(variable.name.clone())
                        .or_default()
                        .insert(skill_id.clone());
                }
            }
        }
    }
    let requested = match config.environment_scope.mode {
        ScopeMode::All => references.keys().cloned().collect::<BTreeSet<_>>(),
        ScopeMode::Selected => config
            .environment_scope
            .names
            .iter()
            .filter(|name| valid_environment_name(name))
            .cloned()
            .collect::<BTreeSet<_>>(),
    };
    let values = SkillEnvironmentStore::for_home(home_dir).read()?;
    let mut records = Vec::new();
    for name in requested {
        let Some(value) = values.get(&name) else {
            continue;
        };
        if value.as_bytes().len() > MAX_ENVIRONMENT_VALUE_BYTES {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ObjectTooLarge,
                false,
            ));
        }
        let Some(referenced_by_skill_ids) = references.get(&name) else {
            continue;
        };
        records.push(EnvironmentRecord {
            name: name.clone(),
            value: value.clone(),
            referenced_by_skill_ids: referenced_by_skill_ids.iter().cloned().collect(),
            metadata: metadata(&format!("environment/{}", name), clock, device_id),
        });
    }
    if records.len() > MAX_ENVIRONMENT_VARIABLES {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ObjectTooLarge,
            false,
        ));
    }
    let _ = skills;
    Ok(records)
}

fn append_tombstones(
    manifest: &mut SnapshotManifest,
    baseline: &SnapshotPayload,
    config: &DeviceSyncConfig,
    clock: &Hlc,
    device_id: &str,
) {
    // Keep configured selected IDs in scope after a local Skill is deleted,
    // so a missing scan entry can still produce a tombstone.
    let selected_skills = config
        .skill_scope
        .skill_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let skill_is_in_scope = |skill_id: &str| {
        config.content_source == "git"
            || config.skill_scope.mode == ScopeMode::All
            || selected_skills.contains(skill_id)
    };

    if config.has_component(SyncComponent::Skills) {
        let current_ids = manifest
            .records
            .skills
            .iter()
            .map(|record| record.skill_id.clone())
            .collect::<BTreeSet<_>>();
        let current_record_ids = manifest
            .records
            .skills
            .iter()
            .map(|record| record.metadata.record_id.clone())
            .collect::<BTreeSet<_>>();
        let mut retained_ids = current_ids.clone();
        for record in &baseline.manifest.records.skills {
            if !skill_is_in_scope(&record.skill_id) || !retained_ids.insert(record.skill_id.clone())
            {
                continue;
            }
            let metadata = if record.metadata.tombstone {
                record.metadata.clone()
            } else {
                tombstone_metadata(&record.metadata, clock, device_id)
            };
            manifest.records.skills.push(SkillRecord {
                skill_id: record.skill_id.clone(),
                metadata: metadata.clone(),
                files: Vec::new(),
            });
            add_tombstone(&mut manifest.tombstones, "skills", &metadata);
        }
        carry_unmatched_tombstones(
            &mut manifest.tombstones,
            &baseline.manifest.tombstones,
            "skills",
            &current_record_ids,
        );
    }

    if config.has_component(SyncComponent::AgentLinks) {
        let current_ids = manifest
            .records
            .agent_links
            .iter()
            .map(|record| (record.agent_id.clone(), record.skill_id.clone()))
            .collect::<BTreeSet<_>>();
        let current_record_ids = manifest
            .records
            .agent_links
            .iter()
            .map(|record| record.metadata.record_id.clone())
            .collect::<BTreeSet<_>>();
        let mut retained_ids = current_ids.clone();
        for record in &baseline.manifest.records.agent_links {
            if !skill_is_in_scope(&record.skill_id)
                || !retained_ids.insert((record.agent_id.clone(), record.skill_id.clone()))
            {
                continue;
            }
            let metadata = if record.metadata.tombstone {
                record.metadata.clone()
            } else {
                tombstone_metadata(&record.metadata, clock, device_id)
            };
            manifest.records.agent_links.push(AgentLinkRecord {
                agent_id: record.agent_id.clone(),
                skill_id: record.skill_id.clone(),
                metadata: metadata.clone(),
            });
            add_tombstone(&mut manifest.tombstones, "agent_links", &metadata);
        }
        carry_unmatched_tombstones(
            &mut manifest.tombstones,
            &baseline.manifest.tombstones,
            "agent_links",
            &current_record_ids,
        );
    }

    if config.has_component(SyncComponent::SkillEnv) {
        let current_ids = manifest
            .records
            .skill_env
            .iter()
            .map(|record| record.name.clone())
            .collect::<BTreeSet<_>>();
        let current_record_ids = manifest
            .records
            .skill_env
            .iter()
            .map(|record| record.metadata.record_id.clone())
            .collect::<BTreeSet<_>>();
        let mut retained_ids = current_ids.clone();
        for record in &baseline.manifest.records.skill_env {
            if !environment_is_in_scope(record, config) || !retained_ids.insert(record.name.clone())
            {
                continue;
            }
            let metadata = if record.metadata.tombstone {
                record.metadata.clone()
            } else {
                tombstone_metadata(&record.metadata, clock, device_id)
            };
            manifest.records.skill_env.push(EnvironmentRecord {
                name: record.name.clone(),
                value: String::new(),
                referenced_by_skill_ids: record.referenced_by_skill_ids.clone(),
                metadata: metadata.clone(),
            });
            add_tombstone(&mut manifest.tombstones, "skill_env", &metadata);
        }
        carry_unmatched_tombstones(
            &mut manifest.tombstones,
            &baseline.manifest.tombstones,
            "skill_env",
            &current_record_ids,
        );
    }
}

fn environment_is_in_scope(record: &EnvironmentRecord, config: &DeviceSyncConfig) -> bool {
    let name_is_selected = config.environment_scope.mode == ScopeMode::All
        || config
            .environment_scope
            .names
            .iter()
            .any(|name| name == &record.name);
    if !name_is_selected {
        return false;
    }
    config.environment_scope.mode == ScopeMode::Selected
        || config.content_source == "git"
        || config.skill_scope.mode == ScopeMode::All
        || record
            .referenced_by_skill_ids
            .iter()
            .any(|skill_id| config.skill_scope.skill_ids.contains(skill_id))
}

fn tombstone_metadata(previous: &RecordMetadata, clock: &Hlc, device_id: &str) -> RecordMetadata {
    RecordMetadata {
        record_id: previous.record_id.clone(),
        hlc: clock.clone(),
        last_modified_by: device_id.to_string(),
        tombstone: true,
    }
}

fn add_tombstone(tombstones: &mut Vec<Tombstone>, component: &str, metadata: &RecordMetadata) {
    if tombstones
        .iter()
        .any(|item| item.component == component && item.record_id == metadata.record_id)
    {
        return;
    }
    tombstones.push(Tombstone {
        component: component.to_string(),
        record_id: metadata.record_id.clone(),
        hlc: metadata.hlc.clone(),
        last_modified_by: metadata.last_modified_by.clone(),
    });
}

fn carry_unmatched_tombstones(
    target: &mut Vec<Tombstone>,
    baseline: &[Tombstone],
    component: &str,
    current_record_ids: &BTreeSet<String>,
) {
    for tombstone in baseline.iter().filter(|item| item.component == component) {
        if current_record_ids.contains(&tombstone.record_id) {
            continue;
        }
        if !target.iter().any(|item| {
            item.component == tombstone.component && item.record_id == tombstone.record_id
        }) {
            target.push(tombstone.clone());
        }
    }
}

fn refresh_component_summaries(manifest: &mut SnapshotManifest) -> Result<(), DeviceSyncError> {
    for (name, summary) in &mut manifest.components {
        match name.as_str() {
            "skills" => {
                summary.records = manifest.records.skills.len() as u64;
            }
            "agent_links" => {
                let bytes = canonical_json(&manifest.records.agent_links)?;
                summary.bytes = bytes.len() as u64;
                summary.sha256 = sha256_hex(&bytes);
                summary.records = manifest.records.agent_links.len() as u64;
            }
            "skill_env" => {
                let bytes = canonical_json(&manifest.records.skill_env)?;
                summary.bytes = bytes.len() as u64;
                summary.sha256 = sha256_hex(&bytes);
                summary.records = manifest.records.skill_env.len() as u64;
            }
            "preferences" => {
                let bytes = canonical_json(&manifest.records.preferences)?;
                summary.bytes = bytes.len() as u64;
                summary.sha256 = sha256_hex(&bytes);
                summary.records = u64::from(manifest.records.preferences.is_some());
            }
            _ => {}
        }
    }
    Ok(())
}

fn component_summary<T: serde::Serialize>(
    value: &T,
    record_count: u64,
) -> Result<ComponentSummary, DeviceSyncError> {
    let bytes = canonical_json(value)?;
    Ok(ComponentSummary {
        sha256: sha256_hex(&bytes),
        bytes: bytes.len() as u64,
        entries: 0,
        records: record_count,
    })
}

fn metadata(record_id: &str, clock: &Hlc, device_id: &str) -> RecordMetadata {
    RecordMetadata {
        record_id: sha256_hex(record_id.as_bytes()),
        hlc: clock.clone(),
        last_modified_by: device_id.to_string(),
        tombstone: false,
    }
}

fn normalize_agent_ids(ids: &[String]) -> Result<Vec<String>, DeviceSyncError> {
    let mut normalized = ids.to_vec();
    normalized.sort();
    normalized.dedup();
    if normalized.iter().any(|id| !valid_segment(id)) {
        return Err(DeviceSyncError::invalid_config("enabled agent ids"));
    }
    Ok(normalized)
}

fn git_repository_hint(skills: &SkillsCore) -> Option<GitRepositoryHint> {
    let repository = git2::Repository::discover(&skills.source_root).ok();
    let remote_url = skills
        .git_remote_url
        .clone()
        .or_else(|| {
            repository
                .as_ref()?
                .find_remote("origin")
                .ok()?
                .url()
                .map(str::to_string)
        })
        .and_then(|url| sanitize_git_remote_url(&url));
    let commit_oid = repository.and_then(|repo| {
        repo.head()
            .ok()
            .and_then(|head| head.target().map(|oid| oid.to_string()))
    });
    if remote_url.is_none() && commit_oid.is_none() && skills.git_platform.is_none() {
        return None;
    }
    Some(GitRepositoryHint {
        provider: skills.git_platform.clone(),
        branch: if skills.git_branch.trim().is_empty() {
            "main".to_string()
        } else {
            skills.git_branch.clone()
        },
        remote_url,
        commit_oid,
    })
}

fn sanitize_git_remote_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.contains('\0') || raw.chars().any(char::is_control) {
        return None;
    }
    if let Ok(mut parsed) = url::Url::parse(raw) {
        if !matches!(
            parsed.scheme(),
            "http" | "https" | "ssh" | "git" | "git+ssh"
        ) || parsed.host_str().is_none()
            || parsed.set_username("").is_err()
            || parsed.set_password(None).is_err()
        {
            return None;
        }
        // Query strings and fragments are not part of repository identity and
        // are a common place for tokens to be embedded in copied Git URLs.
        parsed.set_query(None);
        parsed.set_fragment(None);
        return Some(parsed.to_string());
    }
    let (_, suffix) = raw.split_once('@')?;
    let colon = suffix.rfind(':')?;
    let host = &suffix[..colon];
    let path = &suffix[colon + 1..];
    if host.is_empty()
        || path.is_empty()
        || host.contains('/')
        || host.contains('\\')
        || host.contains('?')
        || host.contains('#')
        || path.contains('?')
        || path.contains('#')
        || host.chars().any(char::is_whitespace)
        || path.chars().any(char::is_whitespace)
    {
        return None;
    }
    let normalized_path = path.trim_start_matches('/');
    (!normalized_path.is_empty()).then(|| format!("ssh://{host}/{normalized_path}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{local_fingerprint, preserve_unchanged_metadata, sanitize_git_remote_url};
    use crate::device_sync::models::{
        AgentLinkRecord, ComponentSummary, EnvironmentRecord, Hlc, PreferencesRecord,
        RecordMetadata, SkillRecord, SnapshotManifest, SnapshotRecords,
    };

    #[test]
    fn strips_git_remote_credentials() {
        assert_eq!(
            sanitize_git_remote_url("https://user:password@example.com/team/repo.git"),
            Some("https://example.com/team/repo.git".to_string())
        );
        assert_eq!(
            sanitize_git_remote_url("git@example.com:team/repo.git"),
            Some("ssh://example.com/team/repo.git".to_string())
        );
        assert_eq!(
            sanitize_git_remote_url("https://example.com/team/repo.git?token=secret#readme"),
            Some("https://example.com/team/repo.git".to_string())
        );
        assert_eq!(sanitize_git_remote_url("file:///tmp/skills"), None);
        assert_eq!(sanitize_git_remote_url("/tmp/skills"), None);
    }

    #[test]
    fn local_fingerprint_ignores_volatile_record_metadata() {
        let mut first = SnapshotManifest::new(
            "snapshot-a".to_string(),
            "vault-a".to_string(),
            "device-a".to_string(),
            Vec::new(),
            "cloud".to_string(),
            BTreeMap::from([("agent_links".to_string(), ComponentSummary::empty())]),
        );
        first.records.agent_links.push(AgentLinkRecord {
            agent_id: "codex".to_string(),
            skill_id: "alpha".to_string(),
            metadata: RecordMetadata {
                record_id: "stable-id".to_string(),
                hlc: Hlc {
                    wall_ms: 1,
                    counter: 0,
                    device_id: "device-a".to_string(),
                },
                last_modified_by: "device-a".to_string(),
                tombstone: false,
            },
        });
        let mut second = first.clone();
        second.records.agent_links[0].metadata.hlc.wall_ms = 2;
        second.records.agent_links[0].metadata.last_modified_by = "device-b".to_string();

        assert_eq!(
            local_fingerprint(&first).unwrap(),
            local_fingerprint(&second).unwrap()
        );

        second.records.agent_links[0].skill_id = "beta".to_string();
        assert_ne!(
            local_fingerprint(&first).unwrap(),
            local_fingerprint(&second).unwrap()
        );
    }

    #[test]
    fn unchanged_records_keep_their_metadata_across_snapshot_builds() {
        let previous_metadata = RecordMetadata {
            record_id: "stable-record".to_string(),
            hlc: Hlc {
                wall_ms: 100,
                counter: 2,
                device_id: "device-a".to_string(),
            },
            last_modified_by: "device-a".to_string(),
            tombstone: false,
        };
        let next_metadata = RecordMetadata {
            record_id: "new-record".to_string(),
            hlc: Hlc {
                wall_ms: 101,
                counter: 0,
                device_id: "device-a".to_string(),
            },
            last_modified_by: "device-a".to_string(),
            tombstone: false,
        };
        let baseline = SnapshotRecords {
            skills: vec![SkillRecord {
                skill_id: "alpha".to_string(),
                metadata: previous_metadata.clone(),
                files: Vec::new(),
            }],
            agent_links: vec![AgentLinkRecord {
                agent_id: "codex".to_string(),
                skill_id: "alpha".to_string(),
                metadata: previous_metadata.clone(),
            }],
            skill_env: vec![EnvironmentRecord {
                name: "ALPHA_KEY".to_string(),
                value: "value".to_string(),
                referenced_by_skill_ids: vec!["alpha".to_string()],
                metadata: previous_metadata.clone(),
            }],
            preferences: Some(PreferencesRecord {
                enabled_agent_ids: vec!["codex".to_string()],
                metadata: previous_metadata.clone(),
            }),
        };
        let mut current = SnapshotRecords {
            skills: vec![SkillRecord {
                skill_id: "alpha".to_string(),
                metadata: next_metadata.clone(),
                files: Vec::new(),
            }],
            agent_links: vec![AgentLinkRecord {
                agent_id: "codex".to_string(),
                skill_id: "alpha".to_string(),
                metadata: next_metadata.clone(),
            }],
            skill_env: vec![EnvironmentRecord {
                name: "ALPHA_KEY".to_string(),
                value: "value".to_string(),
                referenced_by_skill_ids: vec!["alpha".to_string()],
                metadata: next_metadata.clone(),
            }],
            preferences: Some(PreferencesRecord {
                enabled_agent_ids: vec!["codex".to_string()],
                metadata: next_metadata,
            }),
        };

        preserve_unchanged_metadata(&mut current, &baseline);

        assert_eq!(current.skills[0].metadata, previous_metadata);
        assert_eq!(
            current.agent_links[0].metadata,
            baseline.agent_links[0].metadata
        );
        assert_eq!(
            current.skill_env[0].metadata,
            baseline.skill_env[0].metadata
        );
        assert_eq!(
            current.preferences.as_ref().unwrap().metadata,
            baseline.preferences.as_ref().unwrap().metadata
        );
    }
}
