use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
#[cfg(test)]
use std::cell::RefCell;
use std::fs;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use base64::Engine as _;
use uuid::Uuid;

use crate::skills::agent_config::expand_path;
use crate::skills::symlink::single_file_marker_path;
use crate::skills::SkillsCore;
use crate::storage::Database;

use super::archive::unpack_archive;
use super::config::{
    create_private_dir, load_config, load_or_create_identity, load_state, save_config, save_state,
    write_json_atomic, DeviceSyncPaths,
};
use super::crypto::{
    authenticate_snapshot_header, canonical_json, create_vault_metadata, decrypt_snapshot,
    encrypt_snapshot, inspect_snapshot_header, sha256_hex, sign_head,
    snapshot_header_parent_ids_present, unwrap_vault_key, verify_head, VaultKey,
};
use super::models::{
    valid_segment, AgentLinkRecord, DeviceIdentity, DeviceSyncConfig, DeviceSyncError,
    DeviceSyncErrorCode, DeviceSyncRecoverySummary, DeviceSyncState, DeviceSyncStatus,
    DeviceSyncVaultSession, Head, HeadUnsigned, Hlc, PendingContentSource, PreferenceMutation,
    PrepareApplyResponse, PreviewItem, PreviewResponse, PreviewSummary, SnapshotHeader,
    SnapshotListItem, SnapshotPayload, SyncComponent, SyncResult, SyncResultSummary,
    MAX_ENCRYPTED_SNAPSHOT_BYTES, MAX_GRAPH_BYTES, MAX_GRAPH_DEPTH, MAX_GRAPH_METADATA_BYTES,
    MAX_GRAPH_NODES, MAX_HEAD_BYTES, MAX_METADATA_BYTES, MAX_REMOTE_HEADS,
    MAX_REMOTE_LIST_OBJECTS, PREVIEW_TTL_SECONDS, PROTOCOL_VERSION,
};
use super::skill_env::SkillEnvironmentStore;
use super::snapshot::{build_snapshot_with_clock_and_db, SnapshotBuildRequest};
use super::store::{
    ConnectionReport, LocalFolderStore, ObjectKey, ObjectMeta, ObjectPrefix, ObjectStore,
    PutCondition, WebDavCredentials, WebDavStore,
};

#[derive(Debug)]
pub struct DeviceSyncEngine {
    paths: DeviceSyncPaths,
    home_dir: PathBuf,
    source_root: PathBuf,
    db_path: Option<PathBuf>,
    config: DeviceSyncConfig,
    identity: DeviceIdentity,
    state: DeviceSyncState,
    vault_key: Option<VaultKey>,
    webdav_credentials: Option<RuntimeProviderCredentials>,
    previews: HashMap<String, PreviewRecord>,
    transactions: HashMap<String, PendingTransaction>,
    recovery_block: Option<RecoveryBlock>,
    #[cfg(test)]
    fail_after_links: bool,
    #[cfg(test)]
    fail_next_state_save: bool,
}

#[derive(Debug, Clone)]
struct PreviewRecord {
    response: PreviewResponse,
    kind: PreviewKind,
    config: DeviceSyncConfig,
    expires_at: Instant,
}

#[derive(Debug, Clone)]
enum PreviewKind {
    Push {
        encrypted: Vec<u8>,
        payload: SnapshotPayload,
        enabled_agent_ids: Vec<String>,
    },
    Pull {
        payload: SnapshotPayload,
        enabled_agent_ids: Vec<String>,
    },
}

#[derive(Debug, Clone)]
struct PendingTransaction {
    id: String,
    payload: SnapshotPayload,
    config: DeviceSyncConfig,
    expected_remote_fingerprint: String,
    expected_local_fingerprint: String,
    enabled_agent_ids: Vec<String>,
    staging_root: PathBuf,
    rollback_root: PathBuf,
    recovery_root: PathBuf,
    source_root: PathBuf,
    env_path: PathBuf,
    links_path: PathBuf,
    skill_ids: Vec<String>,
    has_skills_component: bool,
    has_env_component: bool,
    has_links_component: bool,
    link_backups: Vec<LinkTargetBackup>,
    remote_sequences: BTreeMap<String, u64>,
    remote_snapshot_ids: Vec<String>,
    observed_clock: Option<Hlc>,
    pending_content_source: Option<PendingContentSource>,
    committed: bool,
    commit_result: Option<SyncResult>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TransactionJournal {
    transaction_id: String,
    source_root: PathBuf,
    staging_root: PathBuf,
    rollback_root: PathBuf,
    recovery_root: PathBuf,
    env_path: PathBuf,
    links_path: PathBuf,
    skill_ids: Vec<String>,
    #[serde(default)]
    has_skills_component: bool,
    #[serde(default)]
    has_env_component: bool,
    #[serde(default)]
    has_links_component: bool,
    #[serde(default)]
    link_backups: Vec<LinkTargetBackup>,
    #[serde(default)]
    remote_sequences: BTreeMap<String, u64>,
    #[serde(default)]
    remote_snapshot_ids: Vec<String>,
    #[serde(default)]
    observed_clock: Option<Hlc>,
    #[serde(default)]
    pending_content_source: Option<PendingContentSource>,
    snapshot_id: String,
    phase: String,
    #[serde(default)]
    committed: bool,
    /// Whether a rollback must restore user-owned paths. This is explicit for
    /// new journals because `prepared -> rollback_requested` must not restore
    /// files that were created after prepare. Missing values remain compatible
    /// with older journals and are inferred only for unambiguous phases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    restore_required: Option<bool>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct RecoveryOutcomes {
    #[serde(default)]
    rolled_back_transaction_ids: BTreeSet<String>,
    #[serde(default)]
    committed_transaction_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct LinkTargetBackup {
    target: PathBuf,
    backup_name: String,
    existed: bool,
    /// The raw symlink target is retained so recovery can distinguish a
    /// pre-existing managed link from a backup that was replaced in the
    /// recovery directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    link_target: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct RemoteHead {
    head: Head,
    meta: ObjectMeta,
}

#[derive(Debug, Clone)]
struct RemoteView {
    heads: Vec<RemoteHead>,
    snapshots: BTreeMap<String, SnapshotPayload>,
    frontier: Vec<String>,
    fingerprint: String,
    observed_clock: Option<Hlc>,
}

#[derive(Debug, Clone)]
struct RecoveryBlock {
    operation_id: Option<String>,
    recovery_path: PathBuf,
}

#[derive(Debug, Clone)]
struct RuntimeProviderCredentials {
    profile_id: String,
    provider_scope: String,
    webdav: WebDavCredentials,
}

const SNAPSHOT_HEADER_READ_BYTES: u64 = 12 + 64 * 1024;
const MAX_RECOVERY_OUTCOME_BYTES: u64 = 1024 * 1024;
const JOURNAL_PREPARED: &str = "prepared";
const JOURNAL_APPLYING: &str = "applying";
const JOURNAL_COMMITTED: &str = "committed";
const JOURNAL_ROLLBACK_REQUESTED: &str = "rollback_requested";
const JOURNAL_ROLLED_BACK: &str = "rolled_back";

const TRANSACTION_ROOT_PREFIX: &str = ".tokenviewer-device-sync-";

#[cfg(test)]
thread_local! {
    static NEXT_JOURNAL_WRITE_FAILURE: RefCell<Option<String>> = RefCell::new(None);
}

#[cfg(test)]
fn inject_next_journal_write_failure(phase: &str) {
    NEXT_JOURNAL_WRITE_FAILURE.with(|failure| {
        *failure.borrow_mut() = Some(phase.to_string());
    });
}

#[cfg(test)]
fn consume_journal_write_failure(phase: &str) -> bool {
    NEXT_JOURNAL_WRITE_FAILURE.with(|failure| {
        let mut failure = failure.borrow_mut();
        if failure.as_deref() == Some(phase) {
            *failure = None;
            true
        } else {
            false
        }
    })
}

fn detect_initial_recovery_block(
    paths: &super::config::DeviceSyncPaths,
    source_root: &Path,
) -> Option<RecoveryBlock> {
    if let Some(block) = detect_recovery_directory_block(&paths.rollback) {
        return Some(block);
    }
    detect_orphan_transaction_root(source_root)
}

fn detect_recovery_directory_block(rollback_root: &Path) -> Option<RecoveryBlock> {
    let metadata = match fs::symlink_metadata(rollback_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => {
            return Some(RecoveryBlock {
                operation_id: None,
                recovery_path: rollback_root.to_path_buf(),
            })
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Some(RecoveryBlock {
            operation_id: None,
            recovery_path: rollback_root.to_path_buf(),
        });
    }
    let mut entries = match fs::read_dir(rollback_root) {
        Ok(entries) => entries,
        Err(_) => {
            return Some(RecoveryBlock {
                operation_id: None,
                recovery_path: rollback_root.to_path_buf(),
            })
        }
    };
    let entry = match entries.next() {
        None => return None,
        Some(Ok(entry)) => entry,
        Some(Err(_)) => {
            return Some(RecoveryBlock {
                operation_id: None,
                recovery_path: rollback_root.to_path_buf(),
            })
        }
    };
    let operation_id = entry
        .file_name()
        .to_str()
        .filter(|value| valid_segment(value))
        .map(str::to_string);
    Some(RecoveryBlock {
        operation_id,
        recovery_path: entry.path(),
    })
}

fn detect_orphan_transaction_root(source_root: &Path) -> Option<RecoveryBlock> {
    let parent = source_root.parent()?;
    let metadata = match fs::symlink_metadata(parent) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => {
            return Some(RecoveryBlock {
                operation_id: None,
                recovery_path: parent.to_path_buf(),
            })
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Some(RecoveryBlock {
            operation_id: None,
            recovery_path: parent.to_path_buf(),
        });
    }
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(_) => {
            return Some(RecoveryBlock {
                operation_id: None,
                recovery_path: parent.to_path_buf(),
            })
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                return Some(RecoveryBlock {
                    operation_id: None,
                    recovery_path: parent.to_path_buf(),
                })
            }
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(operation_id) = name.strip_prefix(TRANSACTION_ROOT_PREFIX) else {
            continue;
        };
        return Some(RecoveryBlock {
            operation_id: valid_segment(operation_id).then(|| operation_id.to_string()),
            recovery_path: entry.path(),
        });
    }
    None
}

impl DeviceSyncEngine {
    pub fn new(home_dir: PathBuf, source_root: PathBuf) -> Result<Self, DeviceSyncError> {
        Self::new_with_db_option(home_dir, source_root, None)
    }

    pub fn new_with_db(home_dir: PathBuf, source_root: PathBuf, db_path: PathBuf) -> Result<Self, DeviceSyncError> {
        Self::new_with_db_option(home_dir, source_root, Some(db_path))
    }

    fn new_with_db_option(home_dir: PathBuf, source_root: PathBuf, db_path: Option<PathBuf>) -> Result<Self, DeviceSyncError> {
        let paths = DeviceSyncPaths::new(&home_dir);
        paths.ensure_root()?;
        let identity = load_or_create_identity(&paths)?;
        let mut config = load_config(&paths)?;
        if config.profile_id.is_empty() {
            config.profile_id = identity.device_id.clone();
        }
        let mut state = load_state(&paths)?;
        let state_needs_save = if state.clock.device_id.is_empty() {
            state.clock = Hlc::zero(identity.device_id.clone());
            true
        } else if state.clock.device_id != identity.device_id {
            // A previous build could have persisted the observed remote
            // device ID. Keep its logical position, but repair ownership to
            // this machine before any new local event is generated.
            state.clock.device_id = identity.device_id.clone();
            true
        } else {
            false
        };
        if state_needs_save {
            save_state(&paths, &state)?;
        }
        let recovery_block = detect_initial_recovery_block(&paths, &source_root);
        Ok(Self {
            paths,
            home_dir,
            source_root,
            db_path,
            config,
            identity,
            state,
            vault_key: None,
            webdav_credentials: None,
            previews: HashMap::new(),
            transactions: HashMap::new(),
            recovery_block,
            #[cfg(test)]
            fail_after_links: false,
            #[cfg(test)]
            fail_next_state_save: false,
        })
    }

    #[cfg(test)]
    fn inject_apply_failure_after_links(&mut self) {
        self.fail_after_links = true;
    }

    #[cfg(test)]
    fn inject_state_save_failure(&mut self) {
        self.fail_next_state_save = true;
    }

    /// Constructor used by protocol integration tests. It persists only the
    /// non-sensitive config; the vault key is still established by create/join.
    pub fn for_test(
        home_dir: &Path,
        source_root: PathBuf,
        config: DeviceSyncConfig,
    ) -> Result<Self, DeviceSyncError> {
        let mut engine = Self::new(home_dir.to_path_buf(), source_root)?;
        engine.set_config(config)?;
        Ok(engine)
    }

    pub fn config(&self) -> DeviceSyncConfig {
        self.config.clone()
    }

    pub fn identity(&self) -> DeviceIdentity {
        self.identity.clone()
    }

    pub fn state(&self) -> DeviceSyncState {
        self.state.clone()
    }

    /// Keep the sync engine aligned with the SkillsCore source root when the
    /// user changes that root through the existing Skills settings flow.
    pub fn set_source_root(&mut self, source_root: PathBuf) -> Result<(), DeviceSyncError> {
        if self.source_root == source_root {
            return Ok(());
        }
        self.ensure_source_root_change_allowed()?;
        self.source_root = source_root;
        Ok(())
    }

    pub fn ensure_source_root_change_allowed(&self) -> Result<(), DeviceSyncError> {
        if self.transactions.is_empty() {
            Ok(())
        } else {
            Err(DeviceSyncError::new(
                DeviceSyncErrorCode::OperationInProgress,
                true,
            ))
        }
    }

    pub fn status(&self) -> Result<DeviceSyncStatus, DeviceSyncError> {
        let (remote_head_fingerprint, frontier) = match self.vault_key.as_ref() {
            Some(vault_key) => {
                let remote = self.remote_view(vault_key, false)?;
                (Some(remote.fingerprint), remote.frontier)
            }
            None => (None, Vec::new()),
        };
        Ok(DeviceSyncStatus {
            config: self.config.clone(),
            identity: self.identity.clone(),
            state: self.state.clone(),
            remote_head_fingerprint,
            frontier,
            recovery_blocked: self.recovery_block.is_some(),
        })
    }

    pub fn vault_session(&self) -> Result<DeviceSyncVaultSession, DeviceSyncError> {
        let vault_key = self.require_vault_key()?;
        Ok(DeviceSyncVaultSession {
            status: self.status()?,
            master_key_b64: base64::engine::general_purpose::STANDARD.encode(vault_key.as_bytes()),
        })
    }

    pub fn set_master_key_b64(&mut self, encoded: &str) -> Result<(), DeviceSyncError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
        self.vault_key = Some(VaultKey::from_vec(bytes)?);
        Ok(())
    }

    pub fn list_snapshots(&self) -> Result<Vec<SnapshotListItem>, DeviceSyncError> {
        self.ensure_operation_allowed()?;
        let vault_key = self.require_vault_key()?;
        let remote = self.remote_view(vault_key, true)?;
        let frontier = remote.frontier.iter().collect::<HashSet<_>>();
        let mut snapshots = remote
            .snapshots
            .iter()
            .map(|(snapshot_id, payload)| {
                let updated_at = remote
                    .heads
                    .iter()
                    .filter(|item| item.head.snapshot_id == *snapshot_id)
                    .map(|item| item.head.updated_at.clone())
                    .max()
                    .unwrap_or_else(|| payload.manifest.created_at.clone());
                SnapshotListItem {
                    snapshot_id: snapshot_id.clone(),
                    device_id: payload.manifest.device_id.clone(),
                    parent_ids: payload.manifest.parent_ids.clone(),
                    created_at: payload.manifest.created_at.clone(),
                    updated_at,
                    is_frontier: frontier.contains(snapshot_id),
                }
            })
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(snapshots)
    }

    pub fn set_config(&mut self, config: DeviceSyncConfig) -> Result<(), DeviceSyncError> {
        self.ensure_operation_allowed()?;
        config.validate()?;
        let vault_changed = self.config.vault_id != config.vault_id;
        let provider_changed = provider_credential_scope(&self.config)
            != provider_credential_scope(&config)
            || self.config.profile_id != config.profile_id;
        save_config(&self.paths, &config)?;
        if vault_changed {
            self.vault_key = None;
        }
        if provider_changed {
            self.webdav_credentials = None;
        }
        self.config = config;
        Ok(())
    }

    pub fn set_webdav_credentials(
        &mut self,
        profile_id: &str,
        password: &str,
    ) -> Result<(), DeviceSyncError> {
        self.ensure_operation_allowed()?;
        if self.config.profile_id != profile_id {
            return Err(DeviceSyncError::invalid_config("credential profile_id"));
        }
        let provider = self
            .config
            .provider
            .as_ref()
            .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::CredentialMissing, false))?;
        if provider.kind != "webdav" {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ProtocolUnsupported,
                false,
            ));
        }
        let username = provider
            .username
            .as_deref()
            .ok_or_else(|| DeviceSyncError::invalid_config("provider.username"))?;
        let webdav = WebDavCredentials::new(username, password)?;
        let provider_scope = provider_credential_scope(&self.config)
            .ok_or_else(|| DeviceSyncError::invalid_config("provider.kind"))?;
        self.webdav_credentials = Some(RuntimeProviderCredentials {
            profile_id: profile_id.to_string(),
            provider_scope,
            webdav,
        });
        Ok(())
    }

    pub fn clear_provider_credentials(&mut self) -> Result<(), DeviceSyncError> {
        self.ensure_operation_allowed()?;
        self.webdav_credentials = None;
        Ok(())
    }

    pub fn test_connection(&self) -> Result<ConnectionReport, DeviceSyncError> {
        self.ensure_operation_allowed()?;
        self.ensure_enabled()?;
        let store = self.store()?;
        store.test_connection()
    }

    pub fn create_vault(&mut self, password: &str) -> Result<(), DeviceSyncError> {
        self.ensure_operation_allowed()?;
        self.ensure_enabled()?;
        let mut config = self.config.clone();
        let vault_id = config
            .vault_id
            .clone()
            .unwrap_or_else(|| "default".to_string());
        config.vault_id = Some(vault_id.clone());
        config.validate()?;
        let metadata = create_vault_metadata(&vault_id, &self.identity.device_id, password)?;
        let bytes = canonical_json(&metadata)?;
        let store = self.store_for_config(&config)?;
        let key = object_key_for_config(&config, "vault.json")?;
        let bytes_len = bytes.len() as u64;
        let mut source = Cursor::new(bytes);
        if let Err(error) = store.put(&key, &mut source, bytes_len, PutCondition::IfNoneMatch) {
            // A conditional-create failure usually means the remote vault
            // metadata is already there; confirm with a HEAD so the user gets
            // the actionable code instead of a generic staleness error.
            if error.code == DeviceSyncErrorCode::RemoteChanged
                && store.head(&key).ok().flatten().is_some()
            {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::VaultAlreadyExists,
                    false,
                ));
            }
            return Err(error);
        }
        self.vault_key = Some(unwrap_vault_key(&metadata, password)?);
        save_config(&self.paths, &config)?;
        self.config = config;
        Ok(())
    }

    pub fn join_vault(&mut self, password: &str) -> Result<(), DeviceSyncError> {
        self.ensure_operation_allowed()?;
        self.ensure_enabled()?;
        let mut config = self.config.clone();
        let vault_id = config
            .vault_id
            .clone()
            .unwrap_or_else(|| "default".to_string());
        config.vault_id = Some(vault_id.clone());
        config.validate()?;
        let store = self.store_for_config(&config)?;
        let key = object_key_for_config(&config, "vault.json")?;
        let bytes = get_object(&*store, &key, MAX_METADATA_BYTES)?;
        let metadata: super::models::VaultMetadata = serde_json::from_slice(&bytes)
            .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::VaultAuthFailed, false))?;
        if metadata.vault_id != vault_id {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::VaultAuthFailed,
                false,
            ));
        }
        let vault_key = unwrap_vault_key(&metadata, password)?;
        self.vault_key = Some(vault_key);
        save_config(&self.paths, &config)?;
        self.config = config;
        Ok(())
    }

    pub fn preview_push(
        &mut self,
        skills: &SkillsCore,
        enabled_agent_ids: &[String],
        rebuild: bool,
    ) -> Result<PreviewResponse, DeviceSyncError> {
        self.ensure_operation_allowed()?;
        self.ensure_enabled()?;
        let vault_key = self.require_vault_key()?;
        let remote = self.remote_view(vault_key, true)?;
        let parent_ids = self.validate_push_base(&remote, rebuild)?;
        let baseline = parent_ids
            .first()
            .and_then(|snapshot_id| remote.snapshots.get(snapshot_id));
        if baseline
            .is_some_and(|payload| payload.manifest.content_source != self.config.content_source)
        {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ConflictRequiresResolution,
                false,
            ));
        }
        let snapshot_id = Uuid::new_v4().to_string();
        let clock = self.next_snapshot_clock(remote.observed_clock.as_ref())?;
        let build = self.build_current_snapshot(
            skills,
            SnapshotBuildRequest {
                snapshot_id,
                vault_id: self.vault_id()?,
                device_id: self.identity.device_id.clone(),
                parent_ids,
                enabled_agent_ids: enabled_agent_ids.to_vec(),
            },
            baseline,
            Some(clock),
        )?;
        let encrypted = encrypt_snapshot(&build.payload, vault_key)?;
        // Reserve the HLC only after the complete snapshot has been built and
        // encrypted. A failed preview must leave the persisted watermark
        // untouched, while a successful preview remains monotonic across a
        // process restart before the upload is attempted.
        let previous_clock = self.state.clock.clone();
        self.state.clock = build.payload.manifest.clock.clone();
        if let Err(error) = self.persist_state() {
            self.state.clock = previous_clock;
            return Err(error);
        }
        let response = self.make_preview_response(
            "push",
            &build,
            baseline,
            remote.fingerprint.clone(),
            &build.local_fingerprint,
        )?;
        self.insert_preview(
            response.clone(),
            PreviewKind::Push {
                encrypted,
                payload: build.payload,
                enabled_agent_ids: enabled_agent_ids.to_vec(),
            },
        );
        Ok(response)
    }

    pub fn preview_pull(
        &mut self,
        skills: &SkillsCore,
        enabled_agent_ids: &[String],
    ) -> Result<PreviewResponse, DeviceSyncError> {
        self.ensure_operation_allowed()?;
        self.ensure_enabled()?;
        let vault_key = self.require_vault_key()?;
        let remote = self.remote_view(vault_key, true)?;
        if remote.frontier.len() != 1 {
            return Err(DeviceSyncError::new(
                if remote.frontier.is_empty() {
                    DeviceSyncErrorCode::VaultNotFound
                } else {
                    DeviceSyncErrorCode::ConflictRequiresResolution
                },
                false,
            ));
        }
        let remote_payload = remote
            .snapshots
            .get(&remote.frontier[0])
            .cloned()
            .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
        let local = self.build_local_fingerprint_snapshot(skills, enabled_agent_ids)?;
        let local_fingerprint = local.local_fingerprint.clone();
        let local_payload = local.payload.clone();
        let build = super::snapshot::SnapshotBuildResult {
            payload: remote_payload,
            local_fingerprint: local_fingerprint.clone(),
            warnings: Vec::new(),
        };
        let response = self.make_preview_response(
            "pull",
            &build,
            Some(&local_payload),
            remote.fingerprint.clone(),
            &local_fingerprint,
        )?;
        self.persist_observed_clock(remote.observed_clock.as_ref())?;
        self.insert_preview(
            response.clone(),
            PreviewKind::Pull {
                payload: build.payload,
                enabled_agent_ids: enabled_agent_ids.to_vec(),
            },
        );
        Ok(response)
    }

    pub fn push(
        &mut self,
        skills: &SkillsCore,
        enabled_agent_ids: &[String],
        preview_token: &str,
        rebuild: bool,
    ) -> Result<SyncResult, DeviceSyncError> {
        self.ensure_operation_allowed()?;
        self.ensure_enabled()?;
        let preview = self.take_preview(preview_token, "push")?;
        let PreviewKind::Push {
            encrypted,
            payload,
            enabled_agent_ids: preview_agent_ids,
        } = preview.kind
        else {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        };
        if self.config != preview.config {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        if enabled_agent_ids != preview_agent_ids {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        let vault_key = self.require_vault_key()?;
        let remote = self.remote_view(vault_key, true)?;
        if remote.fingerprint != preview.response.remote_head_fingerprint {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        let current = self.build_local_fingerprint_snapshot(skills, enabled_agent_ids)?;
        if current.local_fingerprint != preview.response.local_fingerprint {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        let expected_parent = self.validate_push_base(&remote, rebuild)?;
        if expected_parent != payload.manifest.parent_ids {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        let store = self.store()?;
        let snapshot_key = object_key_for_config(
            &self.config,
            &format!("snapshots/{}.tvsync", payload.manifest.snapshot_id),
        )?;
        let mut snapshot_source = Cursor::new(encrypted.clone());
        store.put(
            &snapshot_key,
            &mut snapshot_source,
            encrypted.len() as u64,
            PutCondition::IfNoneMatch,
        )?;
        let sequence = self.next_sequence(&remote);
        let unsigned = HeadUnsigned {
            format: "tokenviewer-head".to_string(),
            protocol_version: PROTOCOL_VERSION,
            vault_id: self.vault_id()?,
            device_id: self.identity.device_id.clone(),
            snapshot_id: payload.manifest.snapshot_id.clone(),
            parent_ids: payload.manifest.parent_ids.clone(),
            updated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            snapshot_sha256: sha256_hex(&encrypted),
            sequence,
        };
        let head = sign_head(&unsigned, vault_key)?;
        let head_bytes = canonical_json(&head)?;
        let head_key = object_key_for_config(
            &self.config,
            &format!("devices/{}/head.json", self.identity.device_id),
        )?;
        let existing = remote
            .heads
            .iter()
            .find(|item| item.head.device_id == self.identity.device_id);
        let condition = existing
            .and_then(|item| item.meta.etag.clone())
            .map(PutCondition::IfMatch)
            .unwrap_or(PutCondition::IfNoneMatch);
        let mut head_source = Cursor::new(head_bytes);
        let head_len = head_source.get_ref().len() as u64;
        store.put(&head_key, &mut head_source, head_len, condition)?;
        self.state.local_sequence = sequence;
        self.state.clock = Hlc::observe(
            &self.state.clock,
            Some(&payload.manifest.clock),
            self.identity.device_id.clone(),
        );
        self.state.applied_snapshot_id = Some(payload.manifest.snapshot_id.clone());
        self.record_remote_state(&remote);
        self.state
            .seen_snapshots
            .insert(payload.manifest.snapshot_id.clone());
        self.state.last_result = Some(SyncResultSummary {
            direction: "push".to_string(),
            snapshot_id: Some(payload.manifest.snapshot_id.clone()),
            completed_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            message_key: None,
        });
        self.persist_state()?;
        Ok(SyncResult {
            direction: "push".to_string(),
            snapshot_id: Some(payload.manifest.snapshot_id),
            operation_id: Uuid::new_v4().to_string(),
            warnings: Vec::new(),
        })
    }

    pub fn prepare_apply(
        &mut self,
        skills: &SkillsCore,
        preview_token: &str,
    ) -> Result<PrepareApplyResponse, DeviceSyncError> {
        self.ensure_operation_allowed()?;
        self.ensure_enabled()?;
        let preview = self.take_preview(preview_token, "pull")?;
        let PreviewKind::Pull {
            payload,
            enabled_agent_ids,
        } = preview.kind
        else {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        };
        if self.config != preview.config {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        let vault_key = self.require_vault_key()?;
        let remote = self.remote_view(vault_key, true)?;
        if remote.fingerprint != preview.response.remote_head_fingerprint {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        let local = self.build_local_fingerprint_snapshot(skills, &enabled_agent_ids)?;
        if local.local_fingerprint != preview.response.local_fingerprint {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        if payload.manifest.content_source != self.config.content_source {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ConflictRequiresResolution,
                false,
            ));
        }

        let has_skills_component = payload.components_contains(SyncComponent::Skills);
        let has_env_component = payload.components_contains(SyncComponent::SkillEnv);
        let has_links_component = payload.components_contains(SyncComponent::AgentLinks);
        let mut skill_ids = if has_skills_component {
            payload
                .manifest
                .records
                .skills
                .iter()
                .map(|record| record.skill_id.clone())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        skill_ids.sort();
        skill_ids.dedup();
        let pending_content_source = self.pending_content_source_for_apply(&payload, skills);
        let env_path = SkillEnvironmentStore::for_home(&self.home_dir())
            .path()
            .to_path_buf();
        let links_path = skills.config_dir.join("linked_skills.json");
        let link_targets = if has_links_component {
            preflight_agent_links(skills, &payload)?
        } else {
            Vec::new()
        };
        let remote_sequences = remote
            .heads
            .iter()
            .map(|item| (item.head.device_id.clone(), item.head.sequence))
            .collect::<BTreeMap<_, _>>();
        let remote_snapshot_ids = remote
            .heads
            .iter()
            .map(|item| item.head.snapshot_id.clone())
            .collect::<Vec<_>>();
        let transaction_id = Uuid::new_v4().to_string();
        let (staging_root, rollback_root, recovery_root) =
            self.transaction_directories(&transaction_id)?;
        let result: Result<PrepareApplyResponse, DeviceSyncError> = (|| {
            create_private_dir(&staging_root)?;
            create_private_dir(&rollback_root)?;
            let link_backups = backup_transaction_state(
                &rollback_root,
                &self.source_root,
                &env_path,
                &links_path,
                &skill_ids,
                &link_targets,
            )?;
            if payload.manifest.content_source == "cloud" && !payload.archive.is_empty() {
                unpack_archive(&payload.archive, &staging_root.join("archive"))?;
            }
            let transaction = PendingTransaction {
                id: transaction_id.clone(),
                payload: payload.clone(),
                config: self.config.clone(),
                expected_remote_fingerprint: preview.response.remote_head_fingerprint.clone(),
                expected_local_fingerprint: preview.response.local_fingerprint.clone(),
                enabled_agent_ids: enabled_agent_ids.clone(),
                staging_root: staging_root.clone(),
                rollback_root: rollback_root.clone(),
                recovery_root: recovery_root.clone(),
                source_root: self.source_root.clone(),
                env_path: env_path.clone(),
                links_path: links_path.clone(),
                skill_ids: skill_ids.clone(),
                has_skills_component,
                has_env_component,
                has_links_component,
                link_backups,
                remote_sequences: remote_sequences.clone(),
                remote_snapshot_ids: remote_snapshot_ids.clone(),
                observed_clock: remote.observed_clock.clone(),
                pending_content_source,
                committed: false,
                commit_result: None,
            };
            write_transaction_journal(&transaction, "prepared")?;
            self.transactions
                .insert(transaction_id.clone(), transaction);
            let preference_mutations = payload
                .manifest
                .records
                .preferences
                .as_ref()
                .map(|record| {
                    vec![PreferenceMutation {
                        key: "skillsEnabledProviders".to_string(),
                        enabled_agent_ids: record.enabled_agent_ids.clone(),
                    }]
                })
                .unwrap_or_default();
            Ok(PrepareApplyResponse {
                transaction_id: transaction_id.clone(),
                preference_mutations,
                recovery_path: Some(recovery_root.clone()),
            })
        })();
        if let Err(error) = &result {
            cleanup_transaction_paths(&staging_root, &rollback_root, &recovery_root);
            return Err(error.clone().with_operation_id(transaction_id));
        }
        result
    }

    pub fn commit_apply(
        &mut self,
        skills: &mut SkillsCore,
        transaction_id: &str,
    ) -> Result<SyncResult, DeviceSyncError> {
        self.ensure_operation_allowed()?;
        let pending = self
            .transactions
            .get(transaction_id)
            .cloned()
            .ok_or_else(|| {
                DeviceSyncError::new(DeviceSyncErrorCode::ApplyFailed, false)
                    .with_operation_id(transaction_id.to_string())
            })?;
        if pending.committed {
            return pending.commit_result.clone().ok_or_else(|| {
                DeviceSyncError::new(DeviceSyncErrorCode::ApplyFailed, false)
                    .with_operation_id(transaction_id.to_string())
            });
        }
        self.ensure_enabled()?;
        if self.config != pending.config
            || pending.payload.manifest.content_source != self.config.content_source
        {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        let vault_key = self.require_vault_key()?;
        let remote = self.remote_view(vault_key, true)?;
        if remote.fingerprint != pending.expected_remote_fingerprint {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        let current = self.build_local_fingerprint_snapshot(skills, &pending.enabled_agent_ids)?;
        if current.local_fingerprint != pending.expected_local_fingerprint {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        if pending.has_links_component {
            // The generated Agent targets can change independently of the
            // source-root fingerprint. Recheck them immediately before any
            // source/env mutation so user-owned paths remain untouched.
            preflight_agent_links(skills, &pending.payload)?;
        }
        // Keep the in-memory transaction until the complete Rust/Swift
        // protocol has acknowledged commit. A caller that fails while
        // writing preferences must still be able to invoke rollback_apply.
        let transaction = pending;
        if let Err(error) = self.apply_transaction(&transaction, skills) {
            if let Err(rollback_error) = rollback_transaction(&transaction) {
                self.recovery_block = Some(RecoveryBlock {
                    operation_id: Some(transaction.id.clone()),
                    recovery_path: transaction.recovery_root.clone(),
                });
                return Err(
                    DeviceSyncError::new(DeviceSyncErrorCode::RollbackFailed, false)
                        .with_argument("detail", rollback_error.code.as_str())
                        .with_operation_id(transaction.id.clone()),
                );
            }
            return Err(error.with_operation_id(transaction.id.clone()));
        }

        self.state.applied_snapshot_id = Some(transaction.payload.manifest.snapshot_id.clone());
        self.state.pending_content_source = transaction.pending_content_source.clone();
        self.state
            .seen_snapshots
            .insert(transaction.payload.manifest.snapshot_id.clone());
        self.state.last_result = Some(SyncResultSummary {
            direction: "pull".to_string(),
            snapshot_id: Some(transaction.payload.manifest.snapshot_id.clone()),
            completed_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            message_key: None,
        });
        self.record_remote_state_from_transaction(&transaction);
        if let Err(error) = self.persist_state() {
            // The files and the Rust journal already say committed. Never
            // restore them after this point: doing so would create the
            // forbidden combination of old files and new persisted state.
            if let Some(pending) = self.transactions.get_mut(transaction_id) {
                pending.committed = true;
                pending.commit_result = None;
            }
            self.recovery_block = Some(RecoveryBlock {
                operation_id: Some(transaction.id.clone()),
                recovery_path: transaction.recovery_root.clone(),
            });
            return Err(
                DeviceSyncError::new(DeviceSyncErrorCode::RecoveryBlocked, true)
                    .with_argument("detail", error.code.as_str())
                    .with_argument(
                        "recovery_path",
                        transaction.recovery_root.to_string_lossy(),
                    )
                    .with_operation_id(transaction.id.clone()),
            );
        }

        let result = SyncResult {
            direction: "pull".to_string(),
            snapshot_id: Some(transaction.payload.manifest.snapshot_id),
            operation_id: transaction.id,
            warnings: Vec::new(),
        };
        if let Some(pending) = self.transactions.get_mut(transaction_id) {
            pending.committed = true;
            pending.commit_result = Some(result.clone());
        }
        Ok(result)
    }

    /// Remove Rust-side transaction state only after Swift has durably marked
    /// its UserDefaults journal committed. A committed journal remains
    /// recoverable if this cleanup call is interrupted.
    pub fn finalize_apply(&mut self, transaction_id: &str) -> Result<(), DeviceSyncError> {
        let transaction = self
            .transactions
            .get(transaction_id)
            .cloned()
            .ok_or_else(|| {
                DeviceSyncError::new(DeviceSyncErrorCode::ApplyFailed, false)
                    .with_operation_id(transaction_id.to_string())
            })?;
        if !transaction.committed {
            return Err(
                DeviceSyncError::new(DeviceSyncErrorCode::ApplyFailed, false)
                    .with_operation_id(transaction.id),
            );
        }
        if let Err(error) = self.persist_recovery_outcome(transaction_id, true) {
            self.recovery_block = Some(RecoveryBlock {
                operation_id: Some(transaction.id.clone()),
                recovery_path: transaction.recovery_root.clone(),
            });
            return Err(DeviceSyncError::new(DeviceSyncErrorCode::RecoveryBlocked, true)
                .with_argument("detail", error.code.as_str())
                .with_argument(
                    "recovery_path",
                    transaction.recovery_root.to_string_lossy(),
                )
                .with_operation_id(transaction_id.to_string()));
        }
        if let Err(error) = cleanup_transaction(&transaction) {
            self.recovery_block = Some(RecoveryBlock {
                operation_id: Some(transaction.id.clone()),
                recovery_path: transaction.recovery_root.clone(),
            });
            return Err(DeviceSyncError::new(DeviceSyncErrorCode::RecoveryBlocked, true)
                .with_argument("detail", error.code.as_str())
                .with_argument(
                    "recovery_path",
                    transaction.recovery_root.to_string_lossy(),
                )
                .with_operation_id(transaction_id.to_string()));
        }
        self.transactions.remove(transaction_id);
        self.recovery_block = detect_initial_recovery_block(&self.paths, &self.source_root);
        Ok(())
    }

    pub fn rollback_apply(&mut self, transaction_id: &str) -> Result<(), DeviceSyncError> {
        let transaction = self
            .transactions
            .get(transaction_id)
            .cloned()
            .ok_or_else(|| {
                DeviceSyncError::new(DeviceSyncErrorCode::RollbackFailed, false)
                    .with_operation_id(transaction_id.to_string())
            })?;
        if transaction.committed {
            return Err(
                DeviceSyncError::new(DeviceSyncErrorCode::ApplyFailed, false)
                    .with_operation_id(transaction.id),
            );
        }
        rollback_transaction(&transaction).map_err(|error| {
            self.recovery_block = Some(RecoveryBlock {
                operation_id: Some(transaction.id.clone()),
                recovery_path: transaction.recovery_root.clone(),
            });
            DeviceSyncError::new(DeviceSyncErrorCode::RollbackFailed, false)
                .with_argument("detail", error.code.as_str())
                .with_operation_id(transaction.id.clone())
        })?;
        self.persist_recovery_outcome(transaction_id, false)
            .map_err(|error| {
                self.recovery_block = Some(RecoveryBlock {
                    operation_id: Some(transaction.id.clone()),
                    recovery_path: transaction.recovery_root.clone(),
                });
                DeviceSyncError::new(DeviceSyncErrorCode::RollbackFailed, false)
                    .with_argument("detail", error.code.as_str())
                    .with_operation_id(transaction.id.clone())
            })?;
        cleanup_transaction(&transaction).map_err(|error| {
            self.recovery_block = Some(RecoveryBlock {
                operation_id: Some(transaction.id.clone()),
                recovery_path: transaction.recovery_root.clone(),
            });
            DeviceSyncError::new(DeviceSyncErrorCode::RollbackFailed, false)
                .with_argument("detail", error.code.as_str())
                .with_operation_id(transaction.id.clone())
        })?;
        self.transactions.remove(transaction_id);
        self.recovery_block = detect_initial_recovery_block(&self.paths, &self.source_root);
        Ok(())
    }

    pub fn recover_pending_apply(&mut self) -> Result<u64, DeviceSyncError> {
        Ok(self.recover_pending_apply_detailed()?.recovered)
    }

    pub fn recover_pending_apply_detailed(
        &mut self,
    ) -> Result<DeviceSyncRecoverySummary, DeviceSyncError> {
        let result = self.recover_pending_apply_inner();
        match result {
            Ok(summary) => {
                self.recovery_block = detect_initial_recovery_block(&self.paths, &self.source_root);
                if self.recovery_block.is_some() {
                    let block = self.recovery_block.as_ref().expect("checked above");
                    return Err(recovery_error(block.operation_id.as_deref()).with_argument(
                        "recovery_path",
                        block.recovery_path.to_string_lossy(),
                    ));
                }
                Ok(summary)
            }
            Err(error) => {
                self.recovery_block = Some(RecoveryBlock {
                    operation_id: error.operation_id.clone(),
                    recovery_path: error
                        .arguments
                        .get("recovery_path")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| self.paths.rollback.clone()),
                });
                Err(error)
            }
        }
    }

    fn recover_pending_apply_inner(
        &mut self,
    ) -> Result<DeviceSyncRecoverySummary, DeviceSyncError> {
        let persisted_outcomes = self.load_recovery_outcomes()?;
        let mut summary = DeviceSyncRecoverySummary {
            recovered: 0,
            rolled_back_transaction_ids: persisted_outcomes
                .rolled_back_transaction_ids
                .iter()
                .cloned()
                .collect(),
            committed_transaction_ids: persisted_outcomes
                .committed_transaction_ids
                .iter()
                .cloned()
                .collect(),
        };
        let rollback_metadata = match fs::symlink_metadata(&self.paths.rollback) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(block) = detect_orphan_transaction_root(&self.source_root) {
                    return Err(recovery_error(block.operation_id.as_deref()).with_argument(
                        "recovery_path",
                        block.recovery_path.to_string_lossy(),
                    ));
                }
                return Ok(summary);
            }
            Err(error) => {
                return Err(
                    DeviceSyncError::new(DeviceSyncErrorCode::RollbackFailed, false)
                        .with_argument("detail", error.to_string()),
                )
            }
        };
        if rollback_metadata.file_type().is_symlink() || !rollback_metadata.is_dir() {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::RollbackFailed,
                false,
            ));
        }
        for entry in fs::read_dir(&self.paths.rollback).map_err(|error| {
            DeviceSyncError::new(DeviceSyncErrorCode::RollbackFailed, false)
                .with_argument("detail", error.to_string())
        })? {
            let entry = entry.map_err(|error| {
                DeviceSyncError::new(DeviceSyncErrorCode::RollbackFailed, false)
                    .with_argument("detail", error.to_string())
            })?;
            let entry_path = entry.path();
            let entry_operation_id = entry_path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|value| valid_segment(value))
                .map(str::to_string);
            let entry_metadata = fs::symlink_metadata(&entry_path).map_err(|error| {
                recovery_error(entry_operation_id.as_deref())
                    .with_argument("detail", error.to_string())
            })?;
            if entry_metadata.file_type().is_symlink() {
                return Err(recovery_error(entry_operation_id.as_deref()));
            }
            if !entry_metadata.is_dir() {
                continue;
            }
            let journal_path = entry_path.join("journal.json");
            let journal_metadata = match fs::symlink_metadata(&journal_path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(recovery_error(entry_operation_id.as_deref())
                        .with_argument("detail", error.to_string()))
                }
            };
            if journal_metadata.file_type().is_symlink() || !journal_metadata.is_file() {
                return Err(recovery_error(entry_operation_id.as_deref()));
            }
            let journal_bytes = fs::read(&journal_path).map_err(|error| {
                recovery_error(entry_operation_id.as_deref())
                    .with_argument("detail", error.to_string())
            })?;
            if journal_bytes.len() as u64 > MAX_RECOVERY_OUTCOME_BYTES {
                return Err(recovery_error(entry_operation_id.as_deref()));
            }
            let journal: TransactionJournal = serde_json::from_slice(&journal_bytes)
                .map_err(|_| recovery_error(entry_operation_id.as_deref()))?;
            self.validate_recovery_journal(&entry_path, &journal)?;
            match journal.phase.as_str() {
                JOURNAL_PREPARED => {
                    // Persist both rollback intent and completion before
                    // cleanup. The explicit restore marker keeps a crash
                    // between these writes from restoring post-prepare user
                    // files.
                    let requested = write_journal_phase(&journal, JOURNAL_ROLLBACK_REQUESTED)
                        .map_err(|error| {
                            error.with_operation_id(journal.transaction_id.clone())
                        })?;
                    let rolled_back = write_journal_phase(&requested, JOURNAL_ROLLED_BACK)
                        .map_err(|error| {
                            error.with_operation_id(journal.transaction_id.clone())
                        })?;
                    self.persist_recovery_outcome(&rolled_back.transaction_id, false)?;
                    add_recovery_transaction(
                        &mut summary.rolled_back_transaction_ids,
                        &journal.transaction_id,
                    );
                }
                JOURNAL_APPLYING | JOURNAL_ROLLBACK_REQUESTED => {
                    // The intent is the durable boundary: if this write
                    // fails, recovery must stop before touching user files.
                    let requested = write_journal_phase(&journal, JOURNAL_ROLLBACK_REQUESTED)
                        .map_err(|error| {
                        error.with_operation_id(journal.transaction_id.clone())
                    })?;
                    if journal_restore_required(&requested)? {
                        restore_from_journal(&requested).map_err(|error| {
                            error.with_operation_id(journal.transaction_id.clone())
                        })?;
                    }
                    let rolled_back = write_journal_phase(&requested, JOURNAL_ROLLED_BACK)
                        .map_err(|error| error.with_operation_id(journal.transaction_id.clone()))?;
                    self.persist_recovery_outcome(&rolled_back.transaction_id, false)?;
                    add_recovery_transaction(
                        &mut summary.rolled_back_transaction_ids,
                        &journal.transaction_id,
                    );
                }
                JOURNAL_ROLLED_BACK => {
                    self.persist_recovery_outcome(&journal.transaction_id, false)?;
                    add_recovery_transaction(
                        &mut summary.rolled_back_transaction_ids,
                        &journal.transaction_id,
                    );
                }
                JOURNAL_COMMITTED => {
                    self.state.applied_snapshot_id = Some(journal.snapshot_id.clone());
                    self.state.pending_content_source = journal.pending_content_source.clone();
                    record_remote_state_from_journal(
                        &mut self.state,
                        &journal,
                        &self.identity.device_id,
                    );
                    self.state
                        .seen_snapshots
                        .insert(journal.snapshot_id.clone());
                    self.state.last_result = Some(SyncResultSummary {
                        direction: "pull".to_string(),
                        snapshot_id: Some(journal.snapshot_id.clone()),
                        completed_at: chrono::Utc::now()
                            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                        message_key: None,
                    });
                    self.persist_state()
                        .map_err(|error| error.with_operation_id(journal.transaction_id.clone()))?;
                    self.persist_recovery_outcome(&journal.transaction_id, true)?;
                    add_recovery_transaction(
                        &mut summary.committed_transaction_ids,
                        &journal.transaction_id,
                    );
                }
                _ => return Err(recovery_error(Some(&journal.transaction_id))),
            }
            cleanup_journal(&journal)
                .map_err(|error| error.with_operation_id(journal.transaction_id.clone()))?;
            let _ = fs::remove_dir_all(entry_path);
            summary.recovered += 1;
        }
        if let Some(block) = detect_orphan_transaction_root(&self.source_root) {
            return Err(recovery_error(block.operation_id.as_deref()).with_argument(
                "recovery_path",
                block.recovery_path.to_string_lossy(),
            ));
        }
        Ok(summary)
    }

    fn load_recovery_outcomes(&self) -> Result<RecoveryOutcomes, DeviceSyncError> {
        let metadata = match fs::symlink_metadata(&self.paths.recovery_outcomes) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RecoveryOutcomes::default())
            }
            Err(error) => {
                return Err(recovery_error(None).with_argument("detail", error.to_string()))
            }
        };
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAX_RECOVERY_OUTCOME_BYTES
        {
            return Err(recovery_error(None));
        }
        let bytes = fs::read(&self.paths.recovery_outcomes)
            .map_err(|error| recovery_error(None).with_argument("detail", error.to_string()))?;
        let outcomes: RecoveryOutcomes =
            serde_json::from_slice(&bytes).map_err(|_| recovery_error(None))?;
        if outcomes.rolled_back_transaction_ids.len() > MAX_GRAPH_NODES
            || outcomes.committed_transaction_ids.len() > MAX_GRAPH_NODES
            || !outcomes
                .rolled_back_transaction_ids
                .is_disjoint(&outcomes.committed_transaction_ids)
            || outcomes
                .rolled_back_transaction_ids
                .iter()
                .chain(outcomes.committed_transaction_ids.iter())
                .any(|id| !valid_segment(id))
        {
            return Err(recovery_error(None));
        }
        Ok(outcomes)
    }

    fn persist_recovery_outcome(
        &self,
        transaction_id: &str,
        committed: bool,
    ) -> Result<(), DeviceSyncError> {
        if !valid_segment(transaction_id) {
            return Err(recovery_error(Some(transaction_id)));
        }
        let mut outcomes = self.load_recovery_outcomes()?;
        if committed {
            if outcomes
                .rolled_back_transaction_ids
                .contains(transaction_id)
            {
                return Err(recovery_error(Some(transaction_id)));
            }
            outcomes
                .committed_transaction_ids
                .insert(transaction_id.to_string());
        } else {
            if outcomes.committed_transaction_ids.contains(transaction_id) {
                return Err(recovery_error(Some(transaction_id)));
            }
            outcomes
                .rolled_back_transaction_ids
                .insert(transaction_id.to_string());
        }
        if outcomes.rolled_back_transaction_ids.len() > MAX_GRAPH_NODES
            || outcomes.committed_transaction_ids.len() > MAX_GRAPH_NODES
        {
            return Err(recovery_error(Some(transaction_id)));
        }
        let encoded =
            serde_json::to_vec(&outcomes).map_err(|_| recovery_error(Some(transaction_id)))?;
        if encoded.len() as u64 > MAX_RECOVERY_OUTCOME_BYTES {
            return Err(recovery_error(Some(transaction_id)));
        }
        write_json_atomic(&self.paths.recovery_outcomes, &outcomes)
            .map_err(|error| error.with_operation_id(transaction_id.to_string()))
    }

    fn validate_recovery_journal(
        &self,
        entry_path: &Path,
        journal: &TransactionJournal,
    ) -> Result<(), DeviceSyncError> {
        let operation_id =
            valid_segment(&journal.transaction_id).then(|| journal.transaction_id.clone());
        let invalid = || recovery_error(operation_id.as_deref());
        let entry_id = entry_path.file_name().and_then(|name| name.to_str());
        if entry_path.parent() != Some(self.paths.rollback.as_path())
            || entry_id != Some(journal.transaction_id.as_str())
            || !valid_segment(&journal.transaction_id)
            || !valid_segment(&journal.snapshot_id)
        {
            return Err(invalid());
        }

        let source_parent = self.source_root.parent().ok_or_else(invalid)?;
        let transaction_root = source_parent.join(format!(
            ".tokenviewer-device-sync-{}",
            journal.transaction_id
        ));
        if journal.source_root != self.source_root
            || journal.staging_root != transaction_root.join("staging")
            || journal.rollback_root != transaction_root.join("rollback")
            || journal.recovery_root != entry_path
            || journal.env_path != SkillEnvironmentStore::for_home(&self.home_dir).path()
            || journal.links_path
                != self
                    .home_dir
                    .join(".tokenviewer/skills-manager/linked_skills.json")
        {
            return Err(invalid());
        }

        if validate_journal_phase_record(journal).is_err()
            || journal.skill_ids.len() > MAX_GRAPH_NODES
            || journal
                .skill_ids
                .iter()
                .any(|skill_id| !valid_segment(skill_id))
            || journal.skill_ids.iter().collect::<BTreeSet<_>>().len() != journal.skill_ids.len()
            || journal.remote_sequences.len() > MAX_REMOTE_HEADS
            || journal
                .remote_sequences
                .keys()
                .any(|device_id| !valid_segment(device_id))
            || journal.remote_snapshot_ids.len() > MAX_GRAPH_NODES
            || journal
                .remote_snapshot_ids
                .iter()
                .any(|snapshot_id| !valid_segment(snapshot_id))
            || journal
                .remote_snapshot_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != journal.remote_snapshot_ids.len()
            || journal.observed_clock.as_ref().is_some_and(|clock| {
                clock.wall_ms < 0 || !valid_segment(&clock.device_id)
            })
        {
            return Err(invalid());
        }

        let mut backup_names = BTreeSet::new();
        let mut backup_targets = BTreeSet::new();
        if journal.link_backups.len() > MAX_GRAPH_NODES {
            return Err(invalid());
        }
        for backup in &journal.link_backups {
            if !valid_segment(&backup.backup_name)
                || !backup.target.is_absolute()
                || !path_is_within(&self.home_dir, &backup.target)
                || backup.link_target.as_ref().is_some_and(|target| {
                    !target.is_absolute() || !path_is_within(&journal.source_root, target)
                })
                || !backup_names.insert(backup.backup_name.clone())
                || !backup_targets.insert(lexically_normalize(&backup.target))
            {
                return Err(invalid());
            }
        }
        if let Some(pending) = &journal.pending_content_source {
            if pending.content_source != "git"
                || pending.skill_ids.len() > MAX_GRAPH_NODES
                || pending
                    .skill_ids
                    .iter()
                    .any(|skill_id| !valid_segment(skill_id))
            {
                return Err(invalid());
            }
        }
        for path in [
            &transaction_root,
            &journal.staging_root,
            &journal.rollback_root,
            &journal.recovery_root,
        ] {
            if let Ok(metadata) = fs::symlink_metadata(path) {
                if metadata.file_type().is_symlink() {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }

    fn apply_transaction(
        &self,
        transaction: &PendingTransaction,
        skills: &mut SkillsCore,
    ) -> Result<(), DeviceSyncError> {
        let prepared = read_transaction_journal(transaction)?;
        let applying = write_journal_phase(&prepared, JOURNAL_APPLYING)?;
        if transaction.has_skills_component
            && transaction.payload.manifest.content_source == "cloud"
        {
            let staged_skills = transaction.staging_root.join("archive/skills");
            let remote_skill_ids = transaction
                .payload
                .manifest
                .records
                .skills
                .iter()
                .filter(|record| !record.metadata.tombstone)
                .map(|record| record.skill_id.as_str())
                .collect::<HashSet<_>>();
            for skill_id in &transaction.skill_ids {
                let source = staged_skills.join(skill_id);
                let target = transaction.source_root.join(skill_id);
                let record = transaction
                    .payload
                    .manifest
                    .records
                    .skills
                    .iter()
                    .find(|record| record.skill_id == *skill_id);
                if target.is_symlink() {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::LinkTargetOccupied,
                        false,
                    ));
                }
                if remote_skill_ids.contains(skill_id.as_str()) {
                    if !source.is_dir() {
                        return Err(DeviceSyncError::new(
                            DeviceSyncErrorCode::IntegrityFailed,
                            false,
                        ));
                    }
                    if target.exists() && !target.is_dir() {
                        return Err(DeviceSyncError::new(
                            DeviceSyncErrorCode::LinkTargetOccupied,
                            false,
                        ));
                    }
                    remove_path_if_exists(&target)?;
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)
                            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
                    }
                    fs::rename(&source, &target)
                        .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
                } else if record.is_some_and(|record| record.metadata.tombstone) && target.exists()
                {
                    if !target.is_dir() {
                        return Err(DeviceSyncError::new(
                            DeviceSyncErrorCode::LinkTargetOccupied,
                            false,
                        ));
                    }
                    remove_path_if_exists(&target)?;
                }
            }
        }

        if transaction.has_env_component {
            let mut updates = BTreeMap::new();
            let mut removals = Vec::new();
            for record in &transaction.payload.manifest.records.skill_env {
                if record.metadata.tombstone {
                    removals.push(record.name.clone());
                } else {
                    updates.insert(record.name.clone(), record.value.clone());
                }
            }
            SkillEnvironmentStore::from_path(transaction.env_path.clone())
                .merge_and_write(&updates, &removals)?;
        }

        if transaction.has_links_component {
            apply_agent_links(
                skills,
                &transaction.payload.manifest.records.agent_links,
                &transaction.payload.manifest.tombstones,
                transaction.pending_content_source.is_some(),
            )?;
        }

        if transaction.payload.components_contains(SyncComponent::Usage) {
            if let Some(path) = &self.db_path {
                let database = Database::open(path)
                    .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
                database
                    .replace_usage(&transaction.payload.manifest.records.usage)
                    .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            }
        }

        #[cfg(test)]
        if self.fail_after_links {
            return Err(DeviceSyncError::apply_failed(
                "injected failure after Agent links",
            ));
        }

        write_journal_phase(&applying, JOURNAL_COMMITTED)?;
        Ok(())
    }

    fn make_preview_response(
        &self,
        direction: &str,
        build: &super::snapshot::SnapshotBuildResult,
        local_payload: Option<&SnapshotPayload>,
        remote_fingerprint: String,
        local_fingerprint: &str,
    ) -> Result<PreviewResponse, DeviceSyncError> {
        let (mut summary, mut items) = if let Some(local) = local_payload {
            diff_payloads(local, &build.payload)?
        } else {
            push_summary(&build.payload)
        };
        for warning in &build.warnings {
            summary.warnings += 1;
            items.push(PreviewItem {
                component: "skills".to_string(),
                action: "warning".to_string(),
                id: warning.path.clone(),
                detail: Some(warning.code.clone()),
                destructive: false,
            });
        }
        let token = Uuid::new_v4().to_string();
        let expires_at = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::seconds(PREVIEW_TTL_SECONDS as i64))
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        Ok(PreviewResponse {
            preview_token: token,
            direction: direction.to_string(),
            expires_at,
            remote_head_fingerprint: remote_fingerprint,
            local_fingerprint: local_fingerprint.to_string(),
            summary,
            items,
        })
    }

    fn insert_preview(&mut self, response: PreviewResponse, kind: PreviewKind) {
        self.expire_previews();
        self.previews.insert(
            response.preview_token.clone(),
            PreviewRecord {
                response,
                kind,
                config: self.config.clone(),
                expires_at: Instant::now() + Duration::from_secs(PREVIEW_TTL_SECONDS),
            },
        );
    }

    fn take_preview(
        &mut self,
        token: &str,
        direction: &str,
    ) -> Result<PreviewRecord, DeviceSyncError> {
        self.expire_previews();
        let preview = self
            .previews
            .remove(token)
            .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::StalePreview, true))?;
        if preview.response.direction != direction || preview.expires_at <= Instant::now() {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::StalePreview,
                true,
            ));
        }
        Ok(preview)
    }

    fn expire_previews(&mut self) {
        let now = Instant::now();
        self.previews.retain(|_, item| item.expires_at > now);
    }

    fn build_current_snapshot(
        &self,
        skills: &SkillsCore,
        request: SnapshotBuildRequest,
        baseline: Option<&SnapshotPayload>,
        clock: Option<Hlc>,
    ) -> Result<super::snapshot::SnapshotBuildResult, DeviceSyncError> {
        let database = self
            .db_path
            .as_ref()
            .map(|path| Database::open(path).map_err(|error| DeviceSyncError::apply_failed(error.to_string())))
            .transpose()?;
        match clock {
            Some(clock) => build_snapshot_with_clock_and_db(
                skills,
                &self.home_dir(),
                &self.config,
                &request,
                clock,
                baseline,
                database.as_ref(),
            ),
            None => super::snapshot::build_snapshot_with_db(skills, &self.home_dir(), &self.config, &request, baseline, database.as_ref()),
        }
    }

    fn build_local_fingerprint_snapshot(
        &self,
        skills: &SkillsCore,
        enabled_agent_ids: &[String],
    ) -> Result<super::snapshot::SnapshotBuildResult, DeviceSyncError> {
        self.build_current_snapshot(
            skills,
            SnapshotBuildRequest {
                snapshot_id: format!("local-{}", self.identity.device_id),
                vault_id: self.vault_id()?,
                device_id: self.identity.device_id.clone(),
                parent_ids: Vec::new(),
                enabled_agent_ids: enabled_agent_ids.to_vec(),
            },
            None,
            None,
        )
    }

    fn next_snapshot_clock(&self, observed: Option<&Hlc>) -> Result<Hlc, DeviceSyncError> {
        Hlc::receive_after(
            &self.state.clock,
            observed,
            Hlc::current_wall_ms(),
            self.identity.device_id.clone(),
        )
        .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))
    }

    fn persist_observed_clock(&mut self, observed: Option<&Hlc>) -> Result<(), DeviceSyncError> {
        let next = Hlc::observe(
            &self.state.clock,
            observed,
            self.identity.device_id.clone(),
        );
        if next == self.state.clock {
            return Ok(());
        }
        let previous = self.state.clock.clone();
        self.state.clock = next;
        if let Err(error) = self.persist_state() {
            self.state.clock = previous;
            return Err(error);
        }
        Ok(())
    }

    fn pending_content_source_for_apply(
        &self,
        payload: &SnapshotPayload,
        skills: &SkillsCore,
    ) -> Option<PendingContentSource> {
        if payload.manifest.content_source != "git" {
            return None;
        }

        let mut skill_ids = payload
            .manifest
            .records
            .skills
            .iter()
            .filter(|record| !record.metadata.tombstone)
            .map(|record| record.skill_id.clone())
            .chain(
                payload
                    .manifest
                    .records
                    .agent_links
                    .iter()
                    .filter(|record| !record.metadata.tombstone)
                    .map(|record| record.skill_id.clone()),
            )
            .collect::<BTreeSet<_>>();
        skill_ids.retain(|skill_id| !self.source_root.join(skill_id).is_dir());

        if skills.git.is_some() && skill_ids.is_empty() {
            return None;
        }

        Some(PendingContentSource {
            content_source: "git".to_string(),
            repository: payload.manifest.git_repository.clone(),
            skill_ids: skill_ids.into_iter().collect(),
        })
    }

    fn validate_push_base(
        &self,
        remote: &RemoteView,
        allow_rebuild: bool,
    ) -> Result<Vec<String>, DeviceSyncError> {
        if remote.frontier.len() > 1 {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ConflictRequiresResolution,
                false,
            ));
        }
        let frontier = remote.frontier.first().cloned();
        if frontier.is_some() && self.state.applied_snapshot_id != frontier {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ConflictRequiresResolution,
                false,
            ));
        }
        if frontier.is_none() && self.state.applied_snapshot_id.is_some() {
            if !allow_rebuild {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::RemoteChanged,
                    true,
                ));
            }
            // Rebuild mode: the remote vault holds no recognizable snapshots
            // while this device still remembers a baseline. Treat the push as
            // a fresh root instead of dead-ending the user.
        }
        Ok(frontier.into_iter().collect())
    }

    fn remote_view(
        &self,
        vault_key: &VaultKey,
        load_frontier_payload: bool,
    ) -> Result<RemoteView, DeviceSyncError> {
        let store = self.store()?;
        let prefix = object_prefix_for_config(&self.config, "devices")?;
        let mut heads = Vec::new();
        let mut cursor = None;
        let mut listed_objects = 0usize;
        let mut device_ids = HashSet::new();
        loop {
            let page = store.list(&prefix, cursor.as_deref())?;
            listed_objects = listed_objects
                .checked_add(page.objects.len())
                .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false))?;
            if listed_objects > MAX_REMOTE_LIST_OBJECTS {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ObjectTooLarge,
                    false,
                ));
            }
            for meta in page.objects {
                if meta.key.segments().last().map(String::as_str) != Some("head.json") {
                    continue;
                }
                let segments = meta.key.segments();
                if segments.len() < 3 || segments[segments.len() - 3] != "devices" {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::IntegrityFailed,
                        false,
                    ));
                }
                let path_device_id = segments[segments.len() - 2].clone();
                if !valid_segment(&path_device_id) {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::IntegrityFailed,
                        false,
                    ));
                }
                let expected_key = object_key_for_config(
                    &self.config,
                    &format!("devices/{}/head.json", path_device_id),
                )?;
                if meta.key != expected_key {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::IntegrityFailed,
                        false,
                    ));
                }
                if heads.len() >= MAX_REMOTE_HEADS {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::ObjectTooLarge,
                        false,
                    ));
                }
                let bytes = get_object(&*store, &meta.key, MAX_HEAD_BYTES)?;
                let head: Head = serde_json::from_slice(&bytes).map_err(|_| {
                    DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false)
                })?;
                validate_remote_head(&head, &path_device_id, &self.vault_id()?)?;
                if canonical_json(&head)?.as_slice() != bytes.as_slice()
                    || !device_ids.insert(head.device_id.clone())
                {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::IntegrityFailed,
                        false,
                    ));
                }
                verify_head(&head, vault_key)?;
                if let Some(previous) = self.state.max_remote_sequences.get(&head.device_id) {
                    if head.sequence < *previous
                        || (head.sequence == *previous
                            && !self.state.seen_snapshots.contains(&head.snapshot_id))
                    {
                        return Err(DeviceSyncError::new(
                            DeviceSyncErrorCode::RemoteRollbackDetected,
                            false,
                        ));
                    }
                }
                heads.push(RemoteHead { head, meta });
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        heads.sort_by(|left, right| left.head.device_id.cmp(&right.head.device_id));
        let fingerprint = sha256_hex(&canonical_json(
            &heads.iter().map(|item| &item.head).collect::<Vec<_>>(),
        )?);
        let mut snapshots = BTreeMap::new();
        let mut graph = BTreeMap::<String, Vec<String>>::new();
        let mut headers = BTreeMap::<String, SnapshotHeader>::new();
        let mut counted_metadata_objects = HashSet::new();
        let mut counted_payload_objects = HashSet::new();
        let mut metadata_bytes = 0u64;
        let mut payload_bytes = 0u64;
        let mut visiting = HashSet::new();
        let candidates = heads
            .iter()
            .map(|item| item.head.snapshot_id.clone())
            .collect::<BTreeSet<_>>();
        let mut frontier = candidates.iter().cloned().collect::<Vec<_>>();

        // Header-only traversal is enough to decide whether device heads are
        // ancestors. Full payloads are loaded only after the frontier is
        // known. Legacy v1 headers that omit `parent_ids` are the one explicit
        // exception: their authenticated payload is needed to discover the
        // graph edge.
        let mut legacy_headers = HashSet::new();
        let mut legacy_payloads = BTreeMap::new();
        let mut legacy_payload_hashes = BTreeMap::new();
        for item in &heads {
            self.collect_snapshot_header(
                &*store,
                &item.head.snapshot_id,
                0,
                Some(item.head.snapshot_sha256.as_str()),
                &mut headers,
                &mut graph,
                &mut metadata_bytes,
                &mut payload_bytes,
                &mut counted_metadata_objects,
                &mut counted_payload_objects,
                &mut visiting,
                &mut legacy_headers,
                &mut legacy_payloads,
                &mut legacy_payload_hashes,
                load_frontier_payload,
            )?;
            let header = headers
                .get(&item.head.snapshot_id)
                .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
            let parents = graph
                .get(&item.head.snapshot_id)
                .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
            // A v1 header predating parent_ids is represented as an empty
            // vector. The authenticated full payload remains the source of
            // truth for that compatibility case.
            let parents_match = if legacy_headers.contains(&item.head.snapshot_id) {
                parents == &item.head.parent_ids
            } else {
                header.parent_ids == item.head.parent_ids
            };
            if !parents_match {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::IntegrityFailed,
                    false,
                ));
            }
        }

        frontier.retain(|candidate| {
            !candidates
                .iter()
                .any(|other| other != candidate && is_ancestor(candidate, other, &graph))
        });
        for snapshot_id in &frontier {
            let frontier_heads = heads
                .iter()
                .filter(|item| item.head.snapshot_id == *snapshot_id)
                .collect::<Vec<_>>();
            let expected_hash = frontier_heads
                .first()
                .map(|item| item.head.snapshot_sha256.as_str())
                .unwrap_or_default();
            if frontier_heads
                .iter()
                .any(|item| item.head.snapshot_sha256 != expected_hash)
            {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::IntegrityFailed,
                    false,
                ));
            }
            if !load_frontier_payload {
                continue;
            }
            let payload = if legacy_payload_hashes
                .get(snapshot_id)
                .is_some_and(|hash| hash == expected_hash)
            {
                legacy_payloads.get(snapshot_id).cloned().ok_or_else(|| {
                    DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false)
                })?
            } else {
                self.load_snapshot(
                    &*store,
                    vault_key,
                    snapshot_id,
                    expected_hash,
                    &mut payload_bytes,
                    &mut counted_payload_objects,
                )?
            };
            if frontier_heads.is_empty()
                || frontier_heads.iter().any(|item| {
                    payload.manifest.device_id != item.head.device_id
                        || payload.manifest.parent_ids != item.head.parent_ids
                })
            {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::IntegrityFailed,
                    false,
                ));
            }
            snapshots.insert(snapshot_id.clone(), payload);
        }
        let observed_clock = legacy_payloads
            .values()
            .chain(snapshots.values())
            .map(|payload| &payload.manifest.clock)
            .filter(|clock| !clock.device_id.is_empty())
            .max()
            .cloned();
        Ok(RemoteView {
            heads,
            snapshots,
            frontier,
            fingerprint,
            observed_clock,
        })
    }

    fn load_snapshot(
        &self,
        store: &dyn ObjectStore,
        vault_key: &VaultKey,
        snapshot_id: &str,
        expected_hash: &str,
        payload_bytes: &mut u64,
        counted_payload_objects: &mut HashSet<String>,
    ) -> Result<SnapshotPayload, DeviceSyncError> {
        let key =
            object_key_for_config(&self.config, &format!("snapshots/{}.tvsync", snapshot_id))?;
        let encrypted = get_object(store, &key, MAX_ENCRYPTED_SNAPSHOT_BYTES).map_err(|error| {
            if error.code == DeviceSyncErrorCode::ObjectTooLarge {
                remote_object_limit_error()
            } else {
                error
            }
        })?;
        account_snapshot_payload_bytes(
            snapshot_id,
            encrypted.len() as u64,
            payload_bytes,
            counted_payload_objects,
        )?;
        if !expected_hash.is_empty() && sha256_hex(&encrypted) != expected_hash {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::IntegrityFailed,
                false,
            ));
        }
        let payload = decrypt_snapshot(&encrypted, vault_key)?;
        if payload.manifest.snapshot_id != snapshot_id
            || payload.manifest.vault_id != self.vault_id()?
            || payload.manifest.parent_ids.len() > 2
        {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::IntegrityFailed,
                false,
            ));
        }
        Ok(payload)
    }

    fn collect_snapshot_header(
        &self,
        store: &dyn ObjectStore,
        snapshot_id: &str,
        depth: usize,
        root_expected_hash: Option<&str>,
        headers: &mut BTreeMap<String, SnapshotHeader>,
        graph: &mut BTreeMap<String, Vec<String>>,
        metadata_bytes: &mut u64,
        payload_bytes: &mut u64,
        counted_metadata_objects: &mut HashSet<String>,
        counted_payload_objects: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
        legacy_headers: &mut HashSet<String>,
        legacy_payloads: &mut BTreeMap<String, SnapshotPayload>,
        legacy_payload_hashes: &mut BTreeMap<String, String>,
        verify_payload: bool,
    ) -> Result<(), DeviceSyncError> {
        if depth > MAX_GRAPH_DEPTH {
            return Err(remote_history_limit_error());
        }
        if graph.contains_key(snapshot_id) {
            return Ok(());
        }
        // Use an explicit DFS stack so a legitimate long linear history does
        // not consume the Rust call stack. `visiting` is the active DFS path;
        // seeing an active node is a real cycle, while `graph` represents an
        // already authenticated/shared ancestor and is safe to reuse.
        let mut stack = vec![(snapshot_id.to_string(), depth, false)];
        while let Some((current_id, current_depth, leaving)) = stack.pop() {
            if leaving {
                visiting.remove(&current_id);
                continue;
            }
            if current_depth > MAX_GRAPH_DEPTH {
                return Err(remote_history_limit_error());
            }
            if graph.contains_key(&current_id) {
                continue;
            }
            if visiting.contains(&current_id) {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ArchiveUnsafe,
                    false,
                ));
            }
            if graph.len() >= MAX_GRAPH_NODES {
                return Err(remote_history_limit_error());
            }

            visiting.insert(current_id.clone());
            let key =
                object_key_for_config(&self.config, &format!("snapshots/{}.tvsync", current_id))?;
            let mut prefix = Vec::new();
            let meta = store
                .get_prefix(&key, SNAPSHOT_HEADER_READ_BYTES, &mut prefix)
                .map_err(|error| {
                    if error.code == DeviceSyncErrorCode::ObjectTooLarge {
                        remote_object_limit_error()
                    } else {
                        error
                    }
                })?;
            if meta.size > MAX_ENCRYPTED_SNAPSHOT_BYTES {
                return Err(remote_object_limit_error());
            }
            if prefix.len() as u64 > SNAPSHOT_HEADER_READ_BYTES {
                return Err(remote_metadata_limit_error());
            }
            if counted_metadata_objects.insert(current_id.clone()) {
                *metadata_bytes = metadata_bytes.checked_add(prefix.len() as u64).ok_or_else(|| {
                    DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false)
                })?;
                if *metadata_bytes > MAX_GRAPH_METADATA_BYTES {
                    return Err(remote_metadata_limit_error());
                }
            }
            let header = inspect_snapshot_header(&prefix).map_err(|_| {
                DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false)
            })?;
            if header.snapshot_id != current_id || header.vault_id != self.vault_id()? {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::IntegrityFailed,
                    false,
                ));
            }
            let parents = if header.header_mac_b64.is_some()
                && snapshot_header_parent_ids_present(&prefix)
            {
                let authenticated = authenticate_snapshot_header(&prefix, self.require_vault_key()?)?;
                if verify_payload {
                    let _ = self.load_snapshot(
                        store,
                        self.require_vault_key()?,
                        &current_id,
                        "",
                        payload_bytes,
                        counted_payload_objects,
                    )?;
                }
                authenticated.parent_ids
            } else {
                legacy_headers.insert(current_id.clone());
                let expected_hash = if current_id == snapshot_id {
                    root_expected_hash.unwrap_or_default()
                } else {
                    ""
                };
                let payload = self.load_snapshot(
                    store,
                    self.require_vault_key()?,
                    &current_id,
                    expected_hash,
                    payload_bytes,
                    counted_payload_objects,
                )?;
                let parents = payload.manifest.parent_ids.clone();
                if !expected_hash.is_empty() {
                    legacy_payload_hashes.insert(current_id.clone(), expected_hash.to_string());
                }
                legacy_payloads.insert(current_id.clone(), payload);
                parents
            };
            headers.insert(current_id.clone(), header);
            graph.insert(current_id.clone(), parents.clone());
            stack.push((current_id, current_depth, true));
            for parent in parents.into_iter().rev() {
                if visiting.contains(&parent) {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::ArchiveUnsafe,
                        false,
                    ));
                }
                if !graph.contains_key(&parent) {
                    stack.push((parent, current_depth + 1, false));
                }
            }
        }
        Ok(())
    }

    fn record_remote_state(&mut self, remote: &RemoteView) {
        for item in &remote.heads {
            self.state
                .max_remote_sequences
                .entry(item.head.device_id.clone())
                .and_modify(|value| *value = (*value).max(item.head.sequence))
                .or_insert(item.head.sequence);
            self.state
                .seen_snapshots
                .insert(item.head.snapshot_id.clone());
        }
    }

    fn record_remote_state_from_transaction(&mut self, transaction: &PendingTransaction) {
        for (device_id, sequence) in &transaction.remote_sequences {
            self.state
                .max_remote_sequences
                .entry(device_id.clone())
                .and_modify(|value| *value = (*value).max(*sequence))
                .or_insert(*sequence);
        }
        self.state
            .seen_snapshots
            .extend(transaction.remote_snapshot_ids.iter().cloned());
        self.state
            .seen_snapshots
            .insert(transaction.payload.manifest.snapshot_id.clone());
        self.state.clock = Hlc::observe(
            &self.state.clock,
            transaction.observed_clock.as_ref(),
            self.identity.device_id.clone(),
        );
    }

    fn next_sequence(&self, remote: &RemoteView) -> u64 {
        let remote_sequence = remote
            .heads
            .iter()
            .find(|item| item.head.device_id == self.identity.device_id)
            .map(|item| item.head.sequence)
            .unwrap_or(0);
        self.state.local_sequence.max(remote_sequence) + 1
    }

    fn transaction_directories(
        &self,
        transaction_id: &str,
    ) -> Result<(PathBuf, PathBuf, PathBuf), DeviceSyncError> {
        fs::create_dir_all(&self.source_root)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        let parent = self
            .source_root
            .parent()
            .ok_or_else(|| DeviceSyncError::apply_failed("source root has no parent"))?
            .to_path_buf();
        let temp_root = parent.join(format!(".tokenviewer-device-sync-{}", transaction_id));
        let staging = temp_root.join("staging");
        let rollback = temp_root.join("rollback");
        let recovery = self.paths.rollback.join(transaction_id);
        create_private_dir(&temp_root)?;
        if let Err(error) = create_private_dir(&recovery) {
            let _ = fs::remove_dir_all(&temp_root);
            return Err(error);
        }
        Ok((staging, rollback, recovery))
    }

    fn store(&self) -> Result<Box<dyn ObjectStore>, DeviceSyncError> {
        self.store_for_config(&self.config)
    }

    fn store_for_config(
        &self,
        config: &DeviceSyncConfig,
    ) -> Result<Box<dyn ObjectStore>, DeviceSyncError> {
        let provider = config
            .provider
            .as_ref()
            .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::CredentialMissing, false))?;
        match provider.kind.as_str() {
            "local_folder" => {
                let root = provider
                    .local_root
                    .clone()
                    .ok_or_else(|| DeviceSyncError::invalid_config("provider.local_root"))?;
                Ok(Box::new(LocalFolderStore::new(root)?))
            }
            "webdav" => {
                let endpoint = provider
                    .endpoint
                    .as_deref()
                    .ok_or_else(|| DeviceSyncError::invalid_config("provider.endpoint"))?;
                let provider_scope = provider_credential_scope(config)
                    .ok_or_else(|| DeviceSyncError::invalid_config("provider.kind"))?;
                let credentials = self
                    .webdav_credentials
                    .as_ref()
                    .filter(|credentials| {
                        credentials.profile_id == config.profile_id
                            && credentials.provider_scope == provider_scope
                    })
                    .map(|credentials| credentials.webdav.clone())
                    .ok_or_else(|| {
                        DeviceSyncError::new(DeviceSyncErrorCode::CredentialMissing, false)
                    })?;
                let store = if provider.insecure {
                    WebDavStore::new_allowing_http(
                        endpoint,
                        &provider.remote_prefix,
                        credentials,
                    )?
                } else {
                    WebDavStore::new(endpoint, &provider.remote_prefix, credentials)?
                };
                Ok(Box::new(store))
            }
            _ => Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ProtocolUnsupported,
                false,
            )),
        }
    }

    fn require_vault_key(&self) -> Result<&VaultKey, DeviceSyncError> {
        self.vault_key
            .as_ref()
            .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::CredentialMissing, false))
    }

    /// Status and recovery are intentionally available while blocked, but all
    /// normal sync mutations must stop until the pending local transaction is
    /// resolved. Keep this check in the Rust engine as well as in Swift so a
    /// bridge caller cannot bypass the recovery gate.
    pub fn ensure_operation_allowed(&self) -> Result<(), DeviceSyncError> {
        let Some(block) = &self.recovery_block else {
            return Ok(());
        };
        let error = DeviceSyncError::new(DeviceSyncErrorCode::RecoveryBlocked, true)
            .with_argument("recovery_path", block.recovery_path.to_string_lossy());
        let error = if let Some(operation_id) = &block.operation_id {
            error.with_operation_id(operation_id.clone())
        } else {
            error
        };
        Err(error)
    }

    fn ensure_enabled(&self) -> Result<(), DeviceSyncError> {
        self.config.validate()?;
        if !self.config.enabled {
            return Err(DeviceSyncError::invalid_config("device sync is disabled"));
        }
        Ok(())
    }

    fn vault_id(&self) -> Result<String, DeviceSyncError> {
        self.config
            .vault_id
            .clone()
            .ok_or_else(|| DeviceSyncError::invalid_config("vault_id"))
    }

    fn home_dir(&self) -> PathBuf {
        self.home_dir.clone()
    }

    fn persist_state(&mut self) -> Result<(), DeviceSyncError> {
        #[cfg(test)]
        if self.fail_next_state_save {
            self.fail_next_state_save = false;
            return Err(DeviceSyncError::apply_failed(
                "injected state save failure",
            ));
        }
        save_state(&self.paths, &self.state)
    }
}

fn object_key_for_config(
    config: &DeviceSyncConfig,
    suffix: &str,
) -> Result<ObjectKey, DeviceSyncError> {
    let mut path = config
        .provider
        .as_ref()
        .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::CredentialMissing, false))?
        .remote_prefix
        .clone();
    if !path.is_empty() {
        path.push('/');
    }
    path.push_str(
        config
            .vault_id
            .as_deref()
            .ok_or_else(|| DeviceSyncError::invalid_config("vault_id"))?,
    );
    path.push('/');
    path.push_str(suffix);
    ObjectKey::from_path(&path)
}

/// Identifies the non-secret provider configuration to which an in-memory
/// credential belongs. Keeping this separate from the password prevents a
/// credential from surviving a profile, endpoint, prefix, or username change.
fn provider_credential_scope(config: &DeviceSyncConfig) -> Option<String> {
    let provider = config.provider.as_ref()?;
    (provider.kind == "webdav").then(|| {
        format!(
            "webdav\n{}\n{}\n{}\n{}",
            provider.endpoint.as_deref().unwrap_or_default(),
            provider.remote_prefix,
            provider.username.as_deref().unwrap_or_default(),
            provider.insecure,
        )
    })
}

fn object_prefix_for_config(
    config: &DeviceSyncConfig,
    suffix: &str,
) -> Result<ObjectPrefix, DeviceSyncError> {
    let mut path = config
        .provider
        .as_ref()
        .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::CredentialMissing, false))?
        .remote_prefix
        .clone();
    if !path.is_empty() {
        path.push('/');
    }
    path.push_str(
        config
            .vault_id
            .as_deref()
            .ok_or_else(|| DeviceSyncError::invalid_config("vault_id"))?,
    );
    path.push('/');
    path.push_str(suffix);
    ObjectPrefix::from_path(&path)
}

fn get_object(
    store: &dyn ObjectStore,
    key: &ObjectKey,
    max_bytes: u64,
) -> Result<Vec<u8>, DeviceSyncError> {
    let mut bytes = Vec::new();
    store.get_bounded(key, max_bytes, &mut bytes)?;
    Ok(bytes)
}

fn validate_remote_head(
    head: &Head,
    path_device_id: &str,
    vault_id: &str,
) -> Result<(), DeviceSyncError> {
    let invalid = || DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false);
    if head.format != "tokenviewer-head"
        || head.protocol_version != PROTOCOL_VERSION
        || head.vault_id != vault_id
        || head.device_id != path_device_id
        || !valid_segment(&head.vault_id)
        || !valid_segment(&head.device_id)
        || !valid_segment(&head.snapshot_id)
        || head.parent_ids.len() > 2
        || head
            .parent_ids
            .iter()
            .any(|parent| !valid_segment(parent) || parent == &head.snapshot_id)
        || head.parent_ids.iter().collect::<BTreeSet<_>>().len() != head.parent_ids.len()
        || !is_sha256_hex(&head.snapshot_sha256)
        || head.sequence == 0
        || head.updated_at.len() > MAX_HEAD_BYTES as usize
        || head.updated_at.chars().any(char::is_control)
        || chrono::DateTime::parse_from_rfc3339(&head.updated_at).is_err()
    {
        return Err(invalid());
    }
    let mac = base64::engine::general_purpose::STANDARD
        .decode(&head.mac_b64)
        .map_err(|_| invalid())?;
    if mac.len() != 32 {
        return Err(invalid());
    }
    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn remote_history_limit_error() -> DeviceSyncError {
    DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false).with_argument(
        "detail",
        "remote history exceeds the safety limit; compact or apply retention before retrying",
    )
}

fn remote_metadata_limit_error() -> DeviceSyncError {
    DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false).with_argument(
        "detail",
        "remote graph metadata exceeds the safety limit; compact or apply retention before retrying",
    )
}

fn remote_payload_limit_error() -> DeviceSyncError {
    DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false).with_argument(
        "detail",
        "downloaded remote snapshot payloads exceed the safety limit; apply retention before retrying",
    )
}

fn remote_object_limit_error() -> DeviceSyncError {
    DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false).with_argument(
        "detail",
        "a remote snapshot object exceeds the per-object safety limit",
    )
}

fn account_snapshot_payload_bytes(
    snapshot_id: &str,
    downloaded_bytes: u64,
    payload_bytes: &mut u64,
    counted_payload_objects: &mut HashSet<String>,
) -> Result<(), DeviceSyncError> {
    if counted_payload_objects.insert(snapshot_id.to_string()) {
        *payload_bytes = payload_bytes
            .checked_add(downloaded_bytes)
            .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false))?;
    }
    if *payload_bytes > MAX_GRAPH_BYTES {
        return Err(remote_payload_limit_error());
    }
    Ok(())
}

fn recovery_error(operation_id: Option<&str>) -> DeviceSyncError {
    let error = DeviceSyncError::new(DeviceSyncErrorCode::RollbackFailed, false);
    operation_id.map_or(error.clone(), |id| error.with_operation_id(id.to_string()))
}

fn add_recovery_transaction(transaction_ids: &mut Vec<String>, transaction_id: &str) {
    if !transaction_ids.iter().any(|id| id == transaction_id) {
        transaction_ids.push(transaction_id.to_string());
    }
}

fn is_ancestor(ancestor: &str, descendant: &str, graph: &BTreeMap<String, Vec<String>>) -> bool {
    let mut queue = VecDeque::from([(descendant.to_string(), 0usize)]);
    let mut visited = HashSet::new();
    while let Some((current, depth)) = queue.pop_front() {
        if depth >= MAX_GRAPH_DEPTH || !visited.insert(current.clone()) {
            continue;
        }
        if let Some(parents) = graph.get(&current) {
            for parent in parents {
                if parent == ancestor {
                    return true;
                }
                queue.push_back((parent.clone(), depth + 1));
            }
        }
    }
    false
}

fn push_summary(payload: &SnapshotPayload) -> (PreviewSummary, Vec<PreviewItem>) {
    let mut summary = PreviewSummary::default();
    let mut items = Vec::new();
    for record in &payload.manifest.records.skills {
        let deleted = record.metadata.tombstone;
        if deleted {
            summary.skills.deleted += 1;
        } else {
            summary.skills.added += 1;
        }
        items.push(PreviewItem {
            component: "skills".to_string(),
            action: if deleted { "delete" } else { "add" }.to_string(),
            id: record.skill_id.clone(),
            detail: None,
            destructive: deleted,
        });
    }
    for record in &payload.manifest.records.agent_links {
        let deleted = record.metadata.tombstone;
        if deleted {
            summary.agent_links.deleted += 1;
        } else {
            summary.agent_links.added += 1;
        }
        items.push(PreviewItem {
            component: "agent_links".to_string(),
            action: if deleted { "delete" } else { "add" }.to_string(),
            id: format!("{}/{}", record.agent_id, record.skill_id),
            detail: None,
            destructive: deleted,
        });
    }
    for record in &payload.manifest.records.skill_env {
        let deleted = record.metadata.tombstone;
        if deleted {
            summary.skill_env.deleted += 1;
        } else {
            summary.skill_env.added += 1;
        }
        items.push(PreviewItem {
            component: "skill_env".to_string(),
            action: if deleted { "delete" } else { "add" }.to_string(),
            id: record.name.clone(),
            detail: (!deleted).then(|| "value will be restored".to_string()),
            destructive: deleted,
        });
    }
    if let Some(record) = &payload.manifest.records.preferences {
        if record.metadata.tombstone {
            summary.preferences.deleted += 1;
        } else {
            summary.preferences.updated += 1;
        }
        items.push(PreviewItem {
            component: "preferences".to_string(),
            action: if record.metadata.tombstone {
                "delete"
            } else {
                "update"
            }
            .to_string(),
            id: record.metadata.record_id.clone(),
            detail: None,
            destructive: record.metadata.tombstone,
        });
    }
    if !payload.manifest.records.usage.is_empty() {
        summary.usage.added = payload.manifest.records.usage.len() as u64;
        items.push(preview_item("usage", "replace", "parsed_usage", true));
    }
    (summary, items)
}

fn diff_payloads(
    local: &SnapshotPayload,
    remote: &SnapshotPayload,
) -> Result<(PreviewSummary, Vec<PreviewItem>), DeviceSyncError> {
    let mut summary = PreviewSummary::default();
    let mut items = Vec::new();
    let local_skills = local
        .manifest
        .records
        .skills
        .iter()
        .map(|record| (record.skill_id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let remote_skills = remote
        .manifest
        .records
        .skills
        .iter()
        .map(|record| (record.skill_id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    for id in local_skills
        .keys()
        .chain(remote_skills.keys())
        .collect::<BTreeSet<_>>()
    {
        match (local_skills.get(id), remote_skills.get(id)) {
            (None, Some(right)) if right.metadata.tombstone => {}
            (None, Some(_)) => {
                summary.skills.added += 1;
                items.push(preview_item("skills", "add", id, false));
            }
            (Some(_), None) => {
                summary.skills.skipped += 1;
                items.push(preview_item("skills", "skipped", id, false));
            }
            (Some(left), Some(right)) if right.metadata.tombstone && !left.metadata.tombstone => {
                summary.skills.deleted += 1;
                items.push(preview_item("skills", "delete", id, true));
            }
            (Some(left), Some(right))
                if left.skill_id != right.skill_id
                    || left.files != right.files
                    || left.metadata.tombstone != right.metadata.tombstone =>
            {
                summary.skills.updated += 1;
                items.push(preview_item("skills", "update", id, true));
            }
            _ => {}
        }
    }
    let local_links = local
        .manifest
        .records
        .agent_links
        .iter()
        .filter(|record| !record.metadata.tombstone)
        .map(|record| (record.agent_id.clone(), record.skill_id.clone()))
        .collect::<BTreeSet<_>>();
    let remote_links = remote
        .manifest
        .records
        .agent_links
        .iter()
        .filter(|record| !record.metadata.tombstone)
        .map(|record| (record.agent_id.clone(), record.skill_id.clone()))
        .collect::<BTreeSet<_>>();
    let remote_link_tombstones = remote
        .manifest
        .records
        .agent_links
        .iter()
        .filter(|record| record.metadata.tombstone)
        .map(|record| (record.agent_id.clone(), record.skill_id.clone()))
        .collect::<BTreeSet<_>>();
    for (agent, skill) in remote_links.difference(&local_links) {
        summary.agent_links.added += 1;
        items.push(preview_item(
            "agent_links",
            "add",
            &format!("{}/{}", agent, skill),
            false,
        ));
    }
    for (agent, skill) in local_links.difference(&remote_links) {
        if remote_link_tombstones.contains(&(agent.clone(), skill.clone())) {
            summary.agent_links.deleted += 1;
        } else {
            summary.agent_links.skipped += 1;
        }
        items.push(preview_item(
            "agent_links",
            if remote_link_tombstones.contains(&(agent.clone(), skill.clone())) {
                "delete"
            } else {
                "skipped"
            },
            &format!("{}/{}", agent, skill),
            remote_link_tombstones.contains(&(agent.clone(), skill.clone())),
        ));
    }
    let local_env = local
        .manifest
        .records
        .skill_env
        .iter()
        .map(|record| (record.name.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let remote_env = remote
        .manifest
        .records
        .skill_env
        .iter()
        .filter(|record| !record.metadata.tombstone)
        .map(|record| (record.name.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let remote_env_tombstones = remote
        .manifest
        .records
        .skill_env
        .iter()
        .filter(|record| record.metadata.tombstone)
        .map(|record| record.name.clone())
        .collect::<BTreeSet<_>>();
    for id in local_env
        .keys()
        .chain(remote_env.keys())
        .collect::<BTreeSet<_>>()
    {
        match (local_env.get(id), remote_env.get(id)) {
            (None, Some(_)) => {
                summary.skill_env.added += 1;
                items.push(preview_item("skill_env", "add", id, false));
            }
            (Some(_), None) => {
                let explicit_delete = remote_env_tombstones.contains(id);
                if explicit_delete {
                    summary.skill_env.deleted += 1;
                } else {
                    summary.skill_env.skipped += 1;
                }
                items.push(preview_item(
                    "skill_env",
                    if explicit_delete { "delete" } else { "skipped" },
                    id,
                    explicit_delete,
                ));
            }
            (Some(left), Some(right))
                if left.value != right.value
                    || left.metadata.tombstone != right.metadata.tombstone =>
            {
                summary.skill_env.updated += 1;
                items.push(preview_item("skill_env", "update", id, true));
            }
            _ => {}
        }
    }
    let local_preferences = local
        .manifest
        .records
        .preferences
        .as_ref()
        .map(|record| (&record.enabled_agent_ids, record.metadata.tombstone));
    let remote_preferences = remote
        .manifest
        .records
        .preferences
        .as_ref()
        .map(|record| (&record.enabled_agent_ids, record.metadata.tombstone));
    if local_preferences != remote_preferences {
        summary.preferences.updated += 1;
        items.push(preview_item(
            "preferences",
            "update",
            "skillsEnabledProviders",
            false,
        ));
    }
    if local.manifest.records.usage != remote.manifest.records.usage {
        summary.usage.updated = remote.manifest.records.usage.len() as u64;
        items.push(preview_item("usage", "replace", "parsed_usage", true));
    }
    Ok((summary, items))
}

fn preview_item(component: &str, action: &str, id: &str, destructive: bool) -> PreviewItem {
    PreviewItem {
        component: component.to_string(),
        action: action.to_string(),
        id: id.to_string(),
        detail: None,
        destructive,
    }
}

fn preflight_agent_links(
    skills: &SkillsCore,
    payload: &SnapshotPayload,
) -> Result<Vec<PathBuf>, DeviceSyncError> {
    let mut incoming = BTreeMap::<String, BTreeSet<String>>::new();
    for record in &payload.manifest.records.agent_links {
        if record.metadata.tombstone {
            continue;
        }
        incoming
            .entry(record.agent_id.clone())
            .or_default()
            .insert(record.skill_id.clone());
    }

    let mut targets = BTreeSet::new();
    for agent in skills.registry.all() {
        if !agent.is_installed {
            continue;
        }
        let mut affected_skill_ids = agent.linked_skills.iter().cloned().collect::<BTreeSet<_>>();
        if let Some(skill_ids) = incoming.get(&agent.source) {
            affected_skill_ids.extend(skill_ids.iter().cloned());
        }
        if affected_skill_ids.is_empty() {
            continue;
        }

        let target_base = expand_path(&agent.skills_path)
            .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::LinkTargetOccupied, false))?;
        match agent.link_type {
            crate::skills::models::LinkType::SingleFile => {
                // All skills for a SingleFile agent share one generated
                // target. Validate it once and back up the target plus its
                // ownership marker as one logical unit.
                let skill_id = affected_skill_ids
                    .iter()
                    .next()
                    .expect("non-empty affected skill set");
                if !targets.insert(target_base.clone()) {
                    // Multiple registered Agents may intentionally share one
                    // SingleFile destination. It is one ownership boundary,
                    // so checking it again can reject the same valid pair
                    // under a different Agent record.
                    continue;
                }
                targets.insert(single_file_marker_path(&target_base));
                skills
                    .symlink
                    .ensure_device_sync_target_available(&agent, skill_id)
                    .map_err(|_| {
                        DeviceSyncError::new(DeviceSyncErrorCode::LinkTargetOccupied, false)
                    })?;
            }
            crate::skills::models::LinkType::Directory
            | crate::skills::models::LinkType::Overlay => {
                for skill_id in affected_skill_ids {
                    skills
                        .symlink
                        .ensure_device_sync_target_available(&agent, &skill_id)
                        .map_err(|_| {
                            DeviceSyncError::new(DeviceSyncErrorCode::LinkTargetOccupied, false)
                        })?;
                    targets.insert(target_base.join(skill_id));
                }
            }
        }
    }
    Ok(targets.into_iter().collect())
}

fn backup_transaction_state(
    rollback_root: &Path,
    source_root: &Path,
    env_path: &Path,
    links_path: &Path,
    skill_ids: &[String],
    link_targets: &[PathBuf],
) -> Result<Vec<LinkTargetBackup>, DeviceSyncError> {
    let backup_skills = rollback_root.join("skills");
    create_private_dir(&backup_skills)?;
    for skill_id in skill_ids {
        let source = source_root.join(skill_id);
        let destination = backup_skills.join(skill_id);
        if source.exists() || source.is_symlink() {
            if source.is_symlink() {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::LinkTargetOccupied,
                    false,
                ));
            }
            copy_path(&source, &destination)?;
        } else {
            fs::write(backup_skills.join(format!("{}.absent", skill_id)), b"")
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        }
    }
    backup_file_with_marker(env_path, &rollback_root.join("skill-env.sh"))?;
    backup_file_with_marker(links_path, &rollback_root.join("linked_skills.json"))?;
    backup_link_targets(rollback_root, link_targets)
}

fn backup_link_targets(
    rollback_root: &Path,
    targets: &[PathBuf],
) -> Result<Vec<LinkTargetBackup>, DeviceSyncError> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }
    let backup_root = rollback_root.join("agent-links");
    create_private_dir(&backup_root)?;
    let mut backups = Vec::with_capacity(targets.len());
    for (index, target) in targets.iter().enumerate() {
        let backup_name = format!("target-{index:06}");
        let backup = backup_root.join(&backup_name);
        let (existed, link_target) = match fs::symlink_metadata(target) {
            Ok(metadata) if metadata.file_type().is_symlink() => (
                true,
                Some(
                    fs::read_link(target)
                        .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?,
                ),
            ),
            Ok(_) => (true, None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (false, None),
            Err(error) => return Err(DeviceSyncError::apply_failed(error.to_string())),
        };
        if existed {
            copy_path(target, &backup)?;
        }
        backups.push(LinkTargetBackup {
            target: target.clone(),
            backup_name,
            existed,
            link_target,
        });
    }
    Ok(backups)
}

fn backup_file_with_marker(source: &Path, destination: &Path) -> Result<(), DeviceSyncError> {
    match fs::symlink_metadata(source) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(DeviceSyncError::new(
                DeviceSyncErrorCode::LinkTargetOccupied,
                false,
            ))
        }
        Ok(_) => fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::write(destination.with_extension("absent"), b"")
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))
        }
        Err(error) => Err(DeviceSyncError::apply_failed(error.to_string())),
    }
}

fn copy_path(source: &Path, destination: &Path) -> Result<(), DeviceSyncError> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(source)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, destination)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(target, destination)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
    } else if metadata.is_dir() {
        fs::create_dir_all(destination)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        for entry in fs::read_dir(source)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?
        {
            let entry = entry.map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            copy_path(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else {
        fs::copy(source, destination)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<(), DeviceSyncError> {
    if path.is_symlink() {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::LinkTargetOccupied,
            false,
        ));
    }
    if path.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
    } else if path.exists() {
        fs::remove_file(path).map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
    }
    Ok(())
}

fn apply_agent_links(
    skills: &mut SkillsCore,
    records: &[AgentLinkRecord],
    tombstones: &[super::models::Tombstone],
    allow_missing_sources: bool,
) -> Result<(), DeviceSyncError> {
    let mut mapping = BTreeMap::<String, Vec<String>>::new();
    for record in records {
        if record.metadata.tombstone {
            continue;
        }
        mapping
            .entry(record.agent_id.clone())
            .or_default()
            .push(record.skill_id.clone());
    }
    for skill_ids in mapping.values_mut() {
        skill_ids.sort();
        skill_ids.dedup();
    }
    let tombstone_record_ids = tombstones
        .iter()
        .filter(|tombstone| tombstone.component == "agent_links")
        .map(|tombstone| tombstone.record_id.as_str())
        .collect::<HashSet<_>>();
    let tombstoned_links = records
        .iter()
        .filter(|record| {
            record.metadata.tombstone
                || tombstone_record_ids.contains(record.metadata.record_id.as_str())
        })
        .map(|record| (record.agent_id.clone(), record.skill_id.clone()))
        .collect::<HashSet<_>>();
    let old_agents = skills.registry.all();
    for agent in &old_agents {
        if !agent.is_installed {
            continue;
        }
        if let Some(skill_ids) = mapping.get(&agent.source) {
            for skill_id in skill_ids {
                if allow_missing_sources && !skills.source_root.join(skill_id).is_dir() {
                    continue;
                }
                skills
                    .symlink
                    .ensure_device_sync_target_available(agent, skill_id)
                    .map_err(|_| {
                        DeviceSyncError::new(DeviceSyncErrorCode::LinkTargetOccupied, false)
                    })?;
            }
        }
        for skill_id in &agent.linked_skills {
            if tombstoned_links.contains(&(agent.source.clone(), skill_id.clone())) {
                skills
                    .symlink
                    .ensure_device_sync_target_available(agent, skill_id)
                    .map_err(|_| {
                        DeviceSyncError::new(DeviceSyncErrorCode::LinkTargetOccupied, false)
                    })?;
            }
        }
    }

    // A record set is scoped state, not proof that every omitted local link was
    // deleted. Keep local links unless the snapshot carries an explicit
    // tombstone, while still allowing a future protocol version to remove them
    // deterministically.
    for agent in &old_agents {
        let entry = mapping.entry(agent.source.clone()).or_default();
        for skill_id in &agent.linked_skills {
            if !tombstoned_links.contains(&(agent.source.clone(), skill_id.clone()))
                && !entry.contains(skill_id)
            {
                entry.push(skill_id.clone());
            }
        }
        entry.sort();
        entry.dedup();
    }

    for agent in &old_agents {
        if agent.is_installed && agent.link_type != crate::skills::models::LinkType::SingleFile {
            for skill_id in &agent.linked_skills {
                if tombstoned_links.contains(&(agent.source.clone(), skill_id.clone())) {
                    skills
                        .symlink
                        .remove_skill_link(agent, skill_id)
                        .map_err(|_| {
                            DeviceSyncError::new(DeviceSyncErrorCode::LinkTargetOccupied, false)
                        })?;
                }
            }
        }
    }

    let previous_linked_by_agent = old_agents
        .iter()
        .map(|agent| (agent.source.clone(), !agent.linked_skills.is_empty()))
        .collect::<HashMap<_, _>>();
    skills
        .registry
        .replace_linked_skills(mapping)
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::ApplyFailed, false))?;
    for agent in skills.registry.all() {
        if !agent.is_installed {
            continue;
        }
        match agent.link_type {
            crate::skills::models::LinkType::SingleFile => {
                let available_skill_ids = agent
                    .linked_skills
                    .iter()
                    .filter(|skill_id| skills.source_root.join(skill_id).is_dir())
                    .cloned()
                    .collect::<Vec<_>>();
                if !agent.linked_skills.is_empty()
                    || previous_linked_by_agent
                        .get(&agent.source)
                        .copied()
                        .unwrap_or(false)
                {
                    if !allow_missing_sources
                        || available_skill_ids.len() == agent.linked_skills.len()
                    {
                        skills
                            .symlink
                            .rebuild_single_file(&agent, &available_skill_ids)
                            .map_err(|_| {
                                DeviceSyncError::new(DeviceSyncErrorCode::ApplyFailed, false)
                            })?;
                    }
                }
            }
            crate::skills::models::LinkType::Directory
            | crate::skills::models::LinkType::Overlay => {
                for skill_id in &agent.linked_skills {
                    if allow_missing_sources && !skills.source_root.join(skill_id).is_dir() {
                        continue;
                    }
                    skills
                        .symlink
                        .create_skill_link(&agent, skill_id)
                        .map_err(|_| {
                            DeviceSyncError::new(DeviceSyncErrorCode::ApplyFailed, false)
                        })?;
                }
            }
        }
    }
    Ok(())
}

fn journal_for(transaction: &PendingTransaction, phase: &str) -> TransactionJournal {
    TransactionJournal {
        transaction_id: transaction.id.clone(),
        source_root: transaction.source_root.clone(),
        staging_root: transaction.staging_root.clone(),
        rollback_root: transaction.rollback_root.clone(),
        recovery_root: transaction.recovery_root.clone(),
        env_path: transaction.env_path.clone(),
        links_path: transaction.links_path.clone(),
        skill_ids: transaction.skill_ids.clone(),
        has_skills_component: transaction.has_skills_component,
        has_env_component: transaction.has_env_component,
        has_links_component: transaction.has_links_component,
        link_backups: transaction.link_backups.clone(),
        remote_sequences: transaction.remote_sequences.clone(),
        remote_snapshot_ids: transaction.remote_snapshot_ids.clone(),
        observed_clock: transaction.observed_clock.clone(),
        pending_content_source: transaction.pending_content_source.clone(),
        snapshot_id: transaction.payload.manifest.snapshot_id.clone(),
        phase: phase.to_string(),
        committed: phase == JOURNAL_COMMITTED,
        restore_required: Some(phase != JOURNAL_PREPARED),
    }
}

fn record_remote_state_from_journal(
    state: &mut DeviceSyncState,
    journal: &TransactionJournal,
    local_device_id: &str,
) {
    for (device_id, sequence) in &journal.remote_sequences {
        state
            .max_remote_sequences
            .entry(device_id.clone())
            .and_modify(|value| *value = (*value).max(*sequence))
            .or_insert(*sequence);
    }
    state
        .seen_snapshots
        .extend(journal.remote_snapshot_ids.iter().cloned());
    state.clock = Hlc::observe(
        &state.clock,
        journal.observed_clock.as_ref(),
        local_device_id.to_string(),
    );
}

fn write_transaction_journal(
    transaction: &PendingTransaction,
    phase: &str,
) -> Result<(), DeviceSyncError> {
    if phase != JOURNAL_PREPARED {
        return Err(recovery_error(Some(&transaction.id)).with_argument(
            "detail",
            "a transaction journal must start in prepared phase",
        ));
    }
    let journal = journal_for(transaction, phase);
    validate_journal_phase_record(&journal)?;
    write_journal(&journal)
}

fn write_journal_phase(
    journal: &TransactionJournal,
    phase: &str,
) -> Result<TransactionJournal, DeviceSyncError> {
    validate_journal_phase_record(journal)?;
    if !journal_phase_transition_allowed(journal.phase.as_str(), phase) {
        return Err(recovery_error(Some(&journal.transaction_id)).with_argument(
            "detail",
            format!(
                "invalid journal phase transition: {} -> {}",
                journal.phase, phase
            ),
        ));
    }
    let mut next = journal.clone();
    next.phase = phase.to_string();
    next.committed = phase == JOURNAL_COMMITTED;
    next.restore_required = Some(match phase {
        JOURNAL_PREPARED => false,
        JOURNAL_APPLYING | JOURNAL_COMMITTED => true,
        JOURNAL_ROLLBACK_REQUESTED => {
            if journal.phase == JOURNAL_PREPARED {
                false
            } else {
                journal_restore_required(journal)?
            }
        }
        JOURNAL_ROLLED_BACK => journal_restore_required(journal)?,
        _ => {
            return Err(recovery_error(Some(&journal.transaction_id)).with_argument(
                "detail",
                "unknown journal phase",
            ))
        }
    });
    validate_journal_phase_record(&next)?;
    write_journal(&next)?;
    Ok(next)
}

fn write_journal(journal: &TransactionJournal) -> Result<(), DeviceSyncError> {
    #[cfg(test)]
    if consume_journal_write_failure(&journal.phase) {
        return Err(recovery_error(Some(&journal.transaction_id)).with_argument(
            "detail",
            "injected journal write failure",
        ));
    }
    // The recovery copy is the sole authority. The rollback directory holds
    // backups only; writing a second journal there could leave two durable
    // phases that disagree after a partial failure.
    write_json_atomic(&journal.recovery_root.join("journal.json"), journal)
}

fn read_transaction_journal(
    transaction: &PendingTransaction,
) -> Result<TransactionJournal, DeviceSyncError> {
    let journal_path = transaction.recovery_root.join("journal.json");
    let bytes = fs::read(&journal_path).map_err(|error| {
        recovery_error(Some(&transaction.id)).with_argument("detail", error.to_string())
    })?;
    if bytes.len() as u64 > MAX_RECOVERY_OUTCOME_BYTES {
        return Err(recovery_error(Some(&transaction.id)));
    }
    let journal: TransactionJournal = serde_json::from_slice(&bytes).map_err(|_| {
        recovery_error(Some(&transaction.id)).with_argument("detail", "invalid transaction journal")
    })?;
    if journal.transaction_id != transaction.id
        || journal.source_root != transaction.source_root
        || journal.staging_root != transaction.staging_root
        || journal.rollback_root != transaction.rollback_root
        || journal.recovery_root != transaction.recovery_root
        || journal.env_path != transaction.env_path
        || journal.links_path != transaction.links_path
        || journal.skill_ids != transaction.skill_ids
        || journal.has_skills_component != transaction.has_skills_component
        || journal.has_env_component != transaction.has_env_component
        || journal.has_links_component != transaction.has_links_component
        || journal.link_backups != transaction.link_backups
        || journal.remote_sequences != transaction.remote_sequences
        || journal.remote_snapshot_ids != transaction.remote_snapshot_ids
        || journal.observed_clock != transaction.observed_clock
        || journal.pending_content_source != transaction.pending_content_source
        || journal.snapshot_id != transaction.payload.manifest.snapshot_id
    {
        return Err(recovery_error(Some(&transaction.id)));
    }
    validate_journal_phase_record(&journal)?;
    Ok(journal)
}

fn journal_phase_transition_allowed(current: &str, next: &str) -> bool {
    current == next
        || matches!(
            (current, next),
            (JOURNAL_PREPARED, JOURNAL_APPLYING)
                | (JOURNAL_PREPARED, JOURNAL_ROLLBACK_REQUESTED)
                | (JOURNAL_APPLYING, JOURNAL_COMMITTED)
                | (JOURNAL_APPLYING, JOURNAL_ROLLBACK_REQUESTED)
                | (JOURNAL_ROLLBACK_REQUESTED, JOURNAL_ROLLED_BACK)
        )
}

fn validate_journal_phase_record(
    journal: &TransactionJournal,
) -> Result<(), DeviceSyncError> {
    let known = matches!(
        journal.phase.as_str(),
        JOURNAL_PREPARED
            | JOURNAL_APPLYING
            | JOURNAL_COMMITTED
            | JOURNAL_ROLLBACK_REQUESTED
            | JOURNAL_ROLLED_BACK
    );
    let committed_matches = (journal.phase == JOURNAL_COMMITTED) == journal.committed;
    let restore_marker_is_valid = match journal.restore_required {
        Some(false) => journal.phase != JOURNAL_APPLYING && journal.phase != JOURNAL_COMMITTED,
        Some(true) => journal.phase != JOURNAL_PREPARED,
        None => journal.phase != JOURNAL_ROLLBACK_REQUESTED,
    };
    if known && committed_matches && restore_marker_is_valid {
        Ok(())
    } else {
        Err(recovery_error(Some(&journal.transaction_id)).with_argument(
            "detail",
            "invalid journal phase record",
        ))
    }
}

fn journal_restore_required(journal: &TransactionJournal) -> Result<bool, DeviceSyncError> {
    if let Some(value) = journal.restore_required {
        return Ok(value);
    }
    match journal.phase.as_str() {
        JOURNAL_PREPARED | JOURNAL_ROLLED_BACK => Ok(false),
        JOURNAL_APPLYING | JOURNAL_COMMITTED => Ok(true),
        JOURNAL_ROLLBACK_REQUESTED => Err(recovery_error(Some(&journal.transaction_id)).with_argument(
            "detail",
            "rollback intent has no restore marker",
        )),
        _ => Err(recovery_error(Some(&journal.transaction_id))),
    }
}

/// Persist the rollback intent before touching any user-owned path. The
/// operation is idempotent so a caller can retry it after a partial failure.
fn rollback_transaction(transaction: &PendingTransaction) -> Result<(), DeviceSyncError> {
    let journal = read_transaction_journal(transaction)?;
    match journal.phase.as_str() {
        JOURNAL_COMMITTED => {
            return Err(recovery_error(Some(&transaction.id)).with_argument(
                "detail",
                "a committed transaction cannot be rolled back",
            ));
        }
        JOURNAL_ROLLED_BACK => return Ok(()),
        JOURNAL_PREPARED | JOURNAL_APPLYING | JOURNAL_ROLLBACK_REQUESTED => {}
        _ => return Err(recovery_error(Some(&transaction.id))),
    }

    let requested = write_journal_phase(&journal, JOURNAL_ROLLBACK_REQUESTED)?;
    if journal_restore_required(&requested)? {
        restore_from_journal(&requested)?;
    }
    write_journal_phase(&requested, JOURNAL_ROLLED_BACK)?;
    Ok(())
}

fn restore_from_journal(journal: &TransactionJournal) -> Result<(), DeviceSyncError> {
    validate_restore_backups(journal)?;
    restore_link_targets(journal).map_err(|error| restore_failure(journal, error))?;
    let backup_skills = journal.rollback_root.join("skills");
    for skill_id in &journal.skill_ids {
        let target = journal.source_root.join(skill_id);
        remove_path_if_exists(&target)
            .map_err(|error| restore_failure(journal, error))?;
        let backup = backup_skills.join(skill_id);
        if backup.exists() || backup.is_symlink() {
            copy_path(&backup, &target).map_err(|error| restore_failure(journal, error))?;
        }
    }
    restore_file_with_marker(
        &journal.rollback_root.join("skill-env.sh"),
        &journal.env_path,
    )
    .map_err(|error| restore_failure(journal, error))?;
    restore_file_with_marker(
        &journal.rollback_root.join("linked_skills.json"),
        &journal.links_path,
    )
    .map_err(|error| restore_failure(journal, error))?;
    Ok(())
}

fn restore_failure(journal: &TransactionJournal, error: DeviceSyncError) -> DeviceSyncError {
    DeviceSyncError::new(DeviceSyncErrorCode::RollbackFailed, false)
        .with_argument("detail", error.code.as_str())
        .with_operation_id(journal.transaction_id.clone())
}

fn backup_metadata(path: &Path, journal: &TransactionJournal) -> Result<Option<fs::Metadata>, DeviceSyncError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(restore_failure(
            journal,
            DeviceSyncError::apply_failed(error.to_string()),
        )),
    }
}

fn require_backup_directory(
    path: &Path,
    journal: &TransactionJournal,
) -> Result<(), DeviceSyncError> {
    let Some(metadata) = backup_metadata(path, journal)? else {
        return Err(restore_failure(
            journal,
            DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false),
        ));
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(restore_failure(
            journal,
            DeviceSyncError::new(DeviceSyncErrorCode::ArchiveUnsafe, false),
        ));
    }
    Ok(())
}

fn validate_file_backup_pair(
    backup: &Path,
    journal: &TransactionJournal,
) -> Result<(), DeviceSyncError> {
    let absent = backup.with_extension("absent");
    let backup_meta = backup_metadata(backup, journal)?;
    let absent_meta = backup_metadata(&absent, journal)?;
    match (backup_meta, absent_meta) {
        (Some(metadata), None)
            if !metadata.file_type().is_symlink() && metadata.is_file() => Ok(()),
        (None, Some(metadata))
            if !metadata.file_type().is_symlink() && metadata.is_file() && metadata.len() == 0 => {
            Ok(())
        }
        _ => Err(restore_failure(
            journal,
            DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false),
        )),
    }
}

fn validate_restore_backups(journal: &TransactionJournal) -> Result<(), DeviceSyncError> {
    require_backup_directory(&journal.rollback_root, journal)?;
    let backup_skills = journal.rollback_root.join("skills");
    require_backup_directory(&backup_skills, journal)?;
    for skill_id in &journal.skill_ids {
        let backup = backup_skills.join(skill_id);
        let absent = backup_skills.join(format!("{skill_id}.absent"));
        let backup_meta = backup_metadata(&backup, journal)?;
        let absent_meta = backup_metadata(&absent, journal)?;
        match (backup_meta, absent_meta) {
            (Some(metadata), None)
                if !metadata.file_type().is_symlink()
                    && (metadata.is_file() || metadata.is_dir()) => {}
            (None, Some(metadata))
                if !metadata.file_type().is_symlink()
                    && metadata.is_file()
                    && metadata.len() == 0 => {}
            _ => {
                return Err(restore_failure(
                    journal,
                    DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false),
                ))
            }
        }
    }
    validate_file_backup_pair(&journal.rollback_root.join("skill-env.sh"), journal)?;
    validate_file_backup_pair(&journal.rollback_root.join("linked_skills.json"), journal)?;

    if !journal.link_backups.is_empty() {
        let backup_root = journal.rollback_root.join("agent-links");
        require_backup_directory(&backup_root, journal)?;
        for backup in &journal.link_backups {
            let source = backup_root.join(&backup.backup_name);
            let metadata = backup_metadata(&source, journal)?;
            if backup.existed {
                let Some(metadata) = metadata else {
                    return Err(restore_failure(
                        journal,
                        DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false),
                    ));
                };
                if metadata.file_type().is_symlink() {
                    let expected_target = backup.link_target.as_ref().ok_or_else(|| {
                        restore_failure(
                            journal,
                            DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false),
                        )
                    })?;
                    let actual_target = fs::read_link(&source).map_err(|error| {
                        restore_failure(
                            journal,
                            DeviceSyncError::apply_failed(error.to_string()),
                        )
                    })?;
                    if &actual_target != expected_target {
                        return Err(restore_failure(
                            journal,
                            DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false),
                        ));
                    }
                } else if backup.link_target.is_some()
                    || (!metadata.is_file() && !metadata.is_dir())
                {
                    return Err(restore_failure(
                        journal,
                        DeviceSyncError::new(DeviceSyncErrorCode::ArchiveUnsafe, false),
                    ));
                }
            } else if metadata.is_some() || backup.link_target.is_some() {
                return Err(restore_failure(
                    journal,
                    DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false),
                ));
            }
        }
    }
    Ok(())
}

fn restore_link_targets(journal: &TransactionJournal) -> Result<(), DeviceSyncError> {
    for backup in journal.link_backups.iter().rev() {
        remove_path_for_restore(&backup.target)?;
        if !backup.existed {
            continue;
        }
        let source = journal
            .rollback_root
            .join("agent-links")
            .join(&backup.backup_name);
        if !source.exists() && !source.is_symlink() {
            return Err(DeviceSyncError::apply_failed(
                "Agent link rollback backup is missing",
            ));
        }
        if let Some(parent) = backup.target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        }
        copy_path(&source, &backup.target)?;
    }
    Ok(())
}

fn remove_path_for_restore(path: &Path) -> Result<(), DeviceSyncError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(DeviceSyncError::apply_failed(error.to_string())),
    };
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path).map_err(|error| DeviceSyncError::apply_failed(error.to_string()))
    } else {
        fs::remove_file(path).map_err(|error| DeviceSyncError::apply_failed(error.to_string()))
    }
}

fn restore_file_with_marker(backup: &Path, target: &Path) -> Result<(), DeviceSyncError> {
    let absent = backup.with_extension("absent");
    if absent.exists() {
        if let Ok(metadata) = fs::symlink_metadata(target) {
            if metadata.file_type().is_symlink() || metadata.is_file() {
                fs::remove_file(target)
                    .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            } else {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::LinkTargetOccupied,
                    false,
                ));
            }
        }
        return Ok(());
    }
    if backup.is_file() {
        if let Ok(metadata) = fs::symlink_metadata(target) {
            if metadata.file_type().is_symlink() || metadata.is_file() {
                fs::remove_file(target)
                    .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            } else {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::LinkTargetOccupied,
                    false,
                ));
            }
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        }
        fs::copy(backup, target)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
    }
    Ok(())
}

fn cleanup_transaction(transaction: &PendingTransaction) -> Result<(), DeviceSyncError> {
    cleanup_journal(&journal_for(transaction, "committed"))
}

fn cleanup_journal(journal: &TransactionJournal) -> Result<(), DeviceSyncError> {
    let source_transaction_root = journal
        .staging_root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| journal.staging_root.clone());
    remove_dir_if_exists(&source_transaction_root)?;
    remove_dir_if_exists(&journal.recovery_root)
}

fn cleanup_transaction_paths(staging: &Path, rollback: &Path, recovery: &Path) {
    let source_transaction_root = staging
        .parent()
        .or_else(|| rollback.parent())
        .unwrap_or(staging);
    let _ = fs::remove_dir_all(source_transaction_root);
    let _ = fs::remove_dir_all(recovery);
}

fn remove_dir_if_exists(path: &Path) -> Result<(), DeviceSyncError> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DeviceSyncError::apply_failed(error.to_string())),
    }
}

fn lexically_normalize(path: &Path) -> PathBuf {
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

fn path_is_within(root: &Path, path: &Path) -> bool {
    lexically_normalize(path).starts_with(lexically_normalize(root))
}

trait SnapshotPayloadExt {
    fn components_contains(&self, component: SyncComponent) -> bool;
}

impl SnapshotPayloadExt for SnapshotPayload {
    fn components_contains(&self, component: SyncComponent) -> bool {
        self.manifest.components.contains_key(component.as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Read, Write};

    use tempfile::TempDir;

    use super::*;
    use crate::device_sync::crypto::sign_snapshot_header;
    use crate::device_sync::models::{Hlc, RecordMetadata, SnapshotManifest};
    use crate::device_sync::models::TVSYNC_MAGIC;
    use crate::device_sync::store::{
        ConnectionReport, DeleteCondition, ObjectPage, ObjectStore, StoreCapabilities,
    };
    use crate::skills::models::LinkType;
    use crate::storage::Database;

    struct HeaderGraphStore {
        vault_key: VaultKey,
        total_nodes: usize,
        padded_prefix: bool,
    }

    impl ObjectStore for HeaderGraphStore {
        fn capabilities(&self) -> StoreCapabilities {
            StoreCapabilities {
                conditional_put: false,
                conditional_delete: false,
                list: true,
                delete: false,
            }
        }

        fn test_connection(&self) -> Result<ConnectionReport, DeviceSyncError> {
            Ok(ConnectionReport {
                provider: "test".to_string(),
                writable: false,
            })
        }

        fn head(&self, _key: &ObjectKey) -> Result<Option<ObjectMeta>, DeviceSyncError> {
            Ok(None)
        }

        fn get_bounded(
            &self,
            _key: &ObjectKey,
            _max_bytes: u64,
            _sink: &mut dyn Write,
        ) -> Result<ObjectMeta, DeviceSyncError> {
            Err(DeviceSyncError::new(
                DeviceSyncErrorCode::InternalError,
                false,
            ))
        }

        fn get_prefix(
            &self,
            key: &ObjectKey,
            _max_bytes: u64,
            sink: &mut dyn Write,
        ) -> Result<ObjectMeta, DeviceSyncError> {
            let file_name = key.segments().last().expect("graph object filename");
            let snapshot_id = file_name
                .strip_suffix(".tvsync")
                .expect("graph object suffix");
            let index = snapshot_id
                .strip_prefix("graph-")
                .expect("graph object prefix")
                .parse::<usize>()
                .expect("graph object index");
            let parent_ids = if index + 1 < self.total_nodes {
                vec![format!("graph-{index:05}", index = index + 1)]
            } else {
                Vec::new()
            };
            let mut header = SnapshotHeader {
                format: "tokenviewer-tvsync".to_string(),
                protocol_version: PROTOCOL_VERSION,
                vault_id: "vault-test".to_string(),
                object_type: "snapshot".to_string(),
                snapshot_id: snapshot_id.to_string(),
                parent_ids,
                payload_sha256: "00".repeat(32),
                header_mac_b64: None,
            };
            header.header_mac_b64 = Some(sign_snapshot_header(&header, &self.vault_key)?);
            let header_bytes = canonical_json(&header)?;
            let mut prefix = Vec::with_capacity(
                TVSYNC_MAGIC.len() + 4 + header_bytes.len(),
            );
            prefix.extend_from_slice(TVSYNC_MAGIC);
            prefix.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
            prefix.extend_from_slice(&header_bytes);
            if self.padded_prefix {
                prefix.resize(SNAPSHOT_HEADER_READ_BYTES as usize, 0);
            }
            sink.write_all(&prefix)
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            Ok(ObjectMeta {
                key: key.clone(),
                size: if self.padded_prefix {
                    MAX_ENCRYPTED_SNAPSHOT_BYTES
                } else {
                    prefix.len() as u64
                },
                etag: None,
            })
        }

        fn put(
            &self,
            _key: &ObjectKey,
            _source: &mut dyn Read,
            _len: u64,
            _condition: PutCondition,
        ) -> Result<ObjectMeta, DeviceSyncError> {
            Err(DeviceSyncError::new(
                DeviceSyncErrorCode::InternalError,
                false,
            ))
        }

        fn list(
            &self,
            _prefix: &ObjectPrefix,
            _cursor: Option<&str>,
        ) -> Result<ObjectPage, DeviceSyncError> {
            Ok(ObjectPage {
                objects: Vec::new(),
                next_cursor: None,
            })
        }

        fn delete(
            &self,
            _key: &ObjectKey,
            _condition: DeleteCondition,
        ) -> Result<(), DeviceSyncError> {
            Err(DeviceSyncError::new(
                DeviceSyncErrorCode::InternalError,
                false,
            ))
        }

        fn create_prefix(&self, _prefix: &ObjectPrefix) -> Result<(), DeviceSyncError> {
            Ok(())
        }
    }

    fn graph_test_engine(
        home: &TempDir,
        remote: &TempDir,
        key: VaultKey,
    ) -> DeviceSyncEngine {
        let source_root = home.path().join(".tokenviewer/skills");
        fs::create_dir_all(&source_root).unwrap();
        let mut config = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
        config.enabled = true;
        let mut engine =
            DeviceSyncEngine::for_test(home.path(), source_root, config).unwrap();
        engine.vault_key = Some(key);
        engine
    }

    #[test]
    fn new_devices_create_and_join_the_default_vault_without_a_known_id() {
        let remote = TempDir::new().unwrap();
        let home_a = TempDir::new().unwrap();
        let home_b = TempDir::new().unwrap();
        let source_a = home_a.path().join(".tokenviewer/skills");
        let source_b = home_b.path().join(".tokenviewer/skills");
        fs::create_dir_all(&source_a).unwrap();
        fs::create_dir_all(&source_b).unwrap();

        let mut config_a = DeviceSyncConfig::for_local_test(remote.path(), "placeholder");
        config_a.enabled = true;
        config_a.vault_id = None;
        let mut config_b = config_a.clone();
        config_b.profile_id = "profile-b".to_string();

        let mut engine_a = DeviceSyncEngine::for_test(home_a.path(), source_a, config_a).unwrap();
        let mut engine_b = DeviceSyncEngine::for_test(home_b.path(), source_b, config_b).unwrap();
        engine_a.create_vault("shared-password").unwrap();
        engine_b.join_vault("shared-password").unwrap();

        assert_eq!(engine_a.config().vault_id.as_deref(), Some("default"));
        assert_eq!(engine_b.config().vault_id.as_deref(), Some("default"));
        assert_eq!(
            engine_a.vault_key.as_ref().unwrap().as_bytes(),
            engine_b.vault_key.as_ref().unwrap().as_bytes()
        );
    }

    #[test]
    fn graph_metadata_node_budget_is_independent_from_payload_budget() {
        let home = TempDir::new().unwrap();
        let remote = TempDir::new().unwrap();
        let engine = graph_test_engine(&home, &remote, VaultKey::from_bytes([31; 32]));
        let store = HeaderGraphStore {
            vault_key: VaultKey::from_bytes([31; 32]),
            total_nodes: MAX_GRAPH_NODES + 1,
            padded_prefix: false,
        };
        let mut headers = BTreeMap::new();
        let mut graph = BTreeMap::new();
        let mut metadata_bytes = 0;
        let mut payload_bytes = 0;
        let mut counted_metadata_objects = HashSet::new();
        let mut counted_payload_objects = HashSet::new();
        let mut visiting = HashSet::new();
        let mut legacy_headers = HashSet::new();
        let mut legacy_payloads = BTreeMap::new();
        let mut legacy_payload_hashes = BTreeMap::new();

        let error = engine
            .collect_snapshot_header(
                &store,
                "graph-00000",
                0,
                None,
                &mut headers,
                &mut graph,
                &mut metadata_bytes,
                &mut payload_bytes,
                &mut counted_metadata_objects,
                &mut counted_payload_objects,
                &mut visiting,
                &mut legacy_headers,
                &mut legacy_payloads,
                &mut legacy_payload_hashes,
                false,
            )
            .unwrap_err();
        assert_eq!(error.code, DeviceSyncErrorCode::ObjectTooLarge);
        assert!(error
            .arguments
            .get("detail")
            .is_some_and(|detail| detail.contains("remote history")));
    }

    #[test]
    fn graph_metadata_byte_budget_counts_only_header_prefixes() {
        let home = TempDir::new().unwrap();
        let remote = TempDir::new().unwrap();
        let engine = graph_test_engine(&home, &remote, VaultKey::from_bytes([32; 32]));
        let store = HeaderGraphStore {
            vault_key: VaultKey::from_bytes([32; 32]),
            total_nodes: (MAX_GRAPH_METADATA_BYTES / SNAPSHOT_HEADER_READ_BYTES) as usize + 1,
            padded_prefix: true,
        };
        let mut headers = BTreeMap::new();
        let mut graph = BTreeMap::new();
        let mut metadata_bytes = 0;
        let mut payload_bytes = 0;
        let mut counted_metadata_objects = HashSet::new();
        let mut counted_payload_objects = HashSet::new();
        let mut visiting = HashSet::new();
        let mut legacy_headers = HashSet::new();
        let mut legacy_payloads = BTreeMap::new();
        let mut legacy_payload_hashes = BTreeMap::new();

        let error = engine
            .collect_snapshot_header(
                &store,
                "graph-00000",
                0,
                None,
                &mut headers,
                &mut graph,
                &mut metadata_bytes,
                &mut payload_bytes,
                &mut counted_metadata_objects,
                &mut counted_payload_objects,
                &mut visiting,
                &mut legacy_headers,
                &mut legacy_payloads,
                &mut legacy_payload_hashes,
                false,
            )
            .unwrap_err();
        assert_eq!(error.code, DeviceSyncErrorCode::ObjectTooLarge);
        assert!(error
            .arguments
            .get("detail")
            .is_some_and(|detail| detail.contains("graph metadata")));
    }

    #[test]
    fn payload_budget_counts_downloaded_objects_once_and_returns_actionable_error() {
        let mut payload_bytes = MAX_GRAPH_BYTES - 1;
        let mut counted = HashSet::new();
        account_snapshot_payload_bytes("snapshot-a", 1, &mut payload_bytes, &mut counted)
            .unwrap();
        account_snapshot_payload_bytes("snapshot-a", MAX_GRAPH_BYTES, &mut payload_bytes, &mut counted)
            .unwrap();
        let error = account_snapshot_payload_bytes(
            "snapshot-b",
            1,
            &mut payload_bytes,
            &mut counted,
        )
        .unwrap_err();
        assert_eq!(error.code, DeviceSyncErrorCode::ObjectTooLarge);
        assert!(error
            .arguments
            .get("detail")
            .is_some_and(|detail| detail.contains("payload")));
    }

    #[test]
    fn rollback_intent_write_failure_leaves_the_applying_phase_durable() {
        let dir = TempDir::new().unwrap();
        let transaction_id = "transaction-journal-failure";
        let recovery_root = dir
            .path()
            .join(".tokenviewer/device-sync/rollback")
            .join(transaction_id);
        fs::create_dir_all(&recovery_root).unwrap();
        let journal = TransactionJournal {
            transaction_id: transaction_id.to_string(),
            source_root: dir.path().join("skills"),
            staging_root: dir.path().join("transaction/staging"),
            rollback_root: dir.path().join("transaction/rollback"),
            recovery_root: recovery_root.clone(),
            env_path: dir.path().join("skill-env.sh"),
            links_path: dir.path().join("linked_skills.json"),
            skill_ids: Vec::new(),
            has_skills_component: false,
            has_env_component: false,
            has_links_component: false,
            link_backups: Vec::new(),
            remote_sequences: BTreeMap::new(),
            remote_snapshot_ids: Vec::new(),
            observed_clock: None,
            pending_content_source: None,
            snapshot_id: "snapshot-journal-failure".to_string(),
            phase: JOURNAL_APPLYING.to_string(),
            committed: false,
            restore_required: Some(true),
        };
        write_json_atomic(&recovery_root.join("journal.json"), &journal).unwrap();
        inject_next_journal_write_failure(JOURNAL_ROLLBACK_REQUESTED);

        let error = write_journal_phase(&journal, JOURNAL_ROLLBACK_REQUESTED).unwrap_err();
        assert_eq!(error.code, DeviceSyncErrorCode::RollbackFailed);
        let persisted: TransactionJournal = serde_json::from_slice(
            &fs::read(recovery_root.join("journal.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted.phase, JOURNAL_APPLYING);
        assert!(!persisted.committed);
    }

    fn run_link_apply(link_type: LinkType) {
        let remote = TempDir::new().unwrap();
        let home_a = TempDir::new().unwrap();
        let home_b = TempDir::new().unwrap();
        let source_a = home_a.path().join(".tokenviewer/skills");
        let source_b = home_b.path().join(".tokenviewer/skills");
        let target_b = match &link_type {
            LinkType::SingleFile => home_b.path().join("agent/AGENT.md"),
            LinkType::Directory | LinkType::Overlay => home_b.path().join("agent/skills"),
        };
        fs::create_dir_all(source_a.join("alpha")).unwrap();
        fs::create_dir_all(&source_b).unwrap();
        fs::write(source_a.join("alpha/SKILL.md"), "# Remote Alpha\n").unwrap();
        let mut config_a = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
        config_a.enabled = true;
        let mut config_b = config_a.clone();
        config_b.profile_id = "profile-b".to_string();
        let mut engine_a =
            DeviceSyncEngine::for_test(home_a.path(), source_a.clone(), config_a).unwrap();
        let mut engine_b =
            DeviceSyncEngine::for_test(home_b.path(), source_b.clone(), config_b).unwrap();
        let db_a = Database::open(&home_a.path().join(".tokenviewer/data.db")).unwrap();
        let db_b = Database::open(&home_b.path().join(".tokenviewer/data.db")).unwrap();
        let mut skills_a = SkillsCore::new(
            &db_a,
            source_a.clone(),
            home_a.path().join(".tokenviewer/skills-manager"),
        )
        .unwrap();
        let mut skills_b = SkillsCore::new(
            &db_b,
            source_b,
            home_b.path().join(".tokenviewer/skills-manager"),
        )
        .unwrap();
        let configure = |skills: &mut SkillsCore, target: &std::path::Path| {
            skills
                .registry
                .set_override(
                    "codex",
                    Some(target.to_string_lossy().to_string()),
                    Some(link_type.clone()),
                )
                .unwrap();
            skills
                .registry
                .set_installed_for_test("codex", true)
                .unwrap();
        };
        let target_a = match &link_type {
            LinkType::SingleFile => home_a.path().join("agent/AGENT.md"),
            LinkType::Directory | LinkType::Overlay => home_a.path().join("agent/skills"),
        };
        configure(&mut skills_a, &target_a);
        configure(&mut skills_b, &target_b);
        skills_a.registry.link_skill("codex", "alpha").unwrap();

        engine_a.create_vault("link-strategy-password").unwrap();
        engine_b.join_vault("link-strategy-password").unwrap();
        let push = engine_a.preview_push(&skills_a, &[], false).unwrap();
        engine_a.push(&skills_a, &[], &push.preview_token, false).unwrap();
        let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
        let transaction = engine_b
            .prepare_apply(&skills_b, &pull.preview_token)
            .unwrap();
        let result = engine_b
            .commit_apply(&mut skills_b, &transaction.transaction_id)
            .unwrap();
        assert_eq!(
            engine_b
                .commit_apply(&mut skills_b, &transaction.transaction_id)
                .unwrap(),
            result
        );

        match link_type {
            LinkType::SingleFile => {
                let content = fs::read_to_string(&target_b).unwrap();
                assert!(content.contains("# Remote Alpha"));
                assert!(!content.contains("# Local managed content"));
            }
            LinkType::Overlay => {
                let link = target_b.join("alpha/SKILL.md");
                assert!(link.is_symlink());
                assert_eq!(fs::read_to_string(link).unwrap(), "# Remote Alpha\n");
            }
            LinkType::Directory => unreachable!(),
        }
    }

    #[test]
    fn single_file_apply_creates_an_unoccupied_target() {
        run_link_apply(LinkType::SingleFile);
    }

    #[test]
    fn overlay_apply_creates_links_into_restored_skill() {
        run_link_apply(LinkType::Overlay);
    }

    #[test]
    fn tombstoned_agent_link_does_not_preflight_an_unowned_single_file() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        let config_dir = dir.path().join("skills-manager");
        let target = dir.path().join("agent/AGENT.md");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "user-owned\n").unwrap();

        let db = Database::open(&dir.path().join("data.db")).unwrap();
        let mut skills = SkillsCore::new(&db, source_root, config_dir).unwrap();
        skills
            .registry
            .set_override(
                "codex",
                Some(target.to_string_lossy().to_string()),
                Some(LinkType::SingleFile),
            )
            .unwrap();
        skills
            .registry
            .set_installed_for_test("codex", true)
            .unwrap();

        let mut manifest = SnapshotManifest::new(
            "snapshot-test".to_string(),
            "vault-test".to_string(),
            "device-test".to_string(),
            Vec::new(),
            "cloud".to_string(),
            BTreeMap::new(),
        );
        manifest.records.agent_links = vec![AgentLinkRecord {
            agent_id: "codex".to_string(),
            skill_id: "alpha".to_string(),
            metadata: RecordMetadata {
                record_id: "agent-link-codex-alpha".to_string(),
                hlc: Hlc {
                    wall_ms: 1,
                    counter: 0,
                    device_id: "device-test".to_string(),
                },
                last_modified_by: "device-test".to_string(),
                tombstone: true,
            },
        }];
        let payload = SnapshotPayload {
            manifest,
            archive: Vec::new(),
        };

        assert!(preflight_agent_links(&skills, &payload).is_ok());
        assert_eq!(fs::read_to_string(target).unwrap(), "user-owned\n");
    }

    #[test]
    fn recovery_rejects_a_journal_for_another_source_root() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        let outside_source = dir.path().join("outside-source");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(outside_source.join("alpha")).unwrap();
        fs::write(outside_source.join("alpha/SKILL.md"), "must remain\n").unwrap();
        let mut engine = DeviceSyncEngine::new(dir.path().to_path_buf(), source_root).unwrap();

        let transaction_id = "transaction-a";
        let recovery_root = dir
            .path()
            .join(".tokenviewer/device-sync/rollback")
            .join(transaction_id);
        fs::create_dir_all(&recovery_root).unwrap();
        let journal = TransactionJournal {
            transaction_id: transaction_id.to_string(),
            source_root: outside_source.clone(),
            staging_root: outside_source.join("staging"),
            rollback_root: outside_source.join("rollback"),
            recovery_root: recovery_root.clone(),
            env_path: outside_source.join("skill-env.sh"),
            links_path: outside_source.join("linked_skills.json"),
            skill_ids: vec!["alpha".to_string()],
            has_skills_component: true,
            has_env_component: false,
            has_links_component: false,
            link_backups: Vec::new(),
            remote_sequences: BTreeMap::new(),
            remote_snapshot_ids: Vec::new(),
            observed_clock: None,
            pending_content_source: None,
            snapshot_id: "snapshot-a".to_string(),
            phase: "applying".to_string(),
            committed: false,
            restore_required: None,
        };
        fs::write(
            recovery_root.join("journal.json"),
            serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();

        let error = engine.recover_pending_apply().unwrap_err();
        assert_eq!(error.code, DeviceSyncErrorCode::RollbackFailed);
        assert_eq!(
            fs::read_to_string(outside_source.join("alpha/SKILL.md")).unwrap(),
            "must remain\n"
        );
    }

    #[test]
    fn recovery_blocks_an_orphan_source_transaction_root() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        fs::create_dir_all(&source_root).unwrap();
        let orphan_root = dir
            .path()
            .join(format!("{TRANSACTION_ROOT_PREFIX}orphan-transaction"));
        fs::create_dir_all(orphan_root.join("rollback")).unwrap();

        let mut engine = DeviceSyncEngine::new(dir.path().to_path_buf(), source_root).unwrap();
        assert!(engine.status().unwrap().recovery_blocked);
        let error = engine.recover_pending_apply().unwrap_err();
        assert_eq!(error.code, DeviceSyncErrorCode::RollbackFailed);
        assert_eq!(
            error.arguments.get("recovery_path"),
            Some(&orphan_root.to_string_lossy().to_string())
        );
        assert!(orphan_root.exists());
    }

    #[test]
    fn recovery_rejects_an_applying_journal_with_a_missing_skill_backup() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        fs::create_dir_all(&source_root).unwrap();
        let mut engine = DeviceSyncEngine::new(dir.path().to_path_buf(), source_root.clone()).unwrap();

        let transaction_id = "transaction-missing-backup";
        let transaction_root = source_root
            .parent()
            .unwrap()
            .join(format!("{TRANSACTION_ROOT_PREFIX}{transaction_id}"));
        let recovery_root = dir
            .path()
            .join(".tokenviewer/device-sync/rollback")
            .join(transaction_id);
        fs::create_dir_all(transaction_root.join("staging")).unwrap();
        fs::create_dir_all(transaction_root.join("rollback/skills")).unwrap();
        fs::create_dir_all(&recovery_root).unwrap();

        let journal = TransactionJournal {
            transaction_id: transaction_id.to_string(),
            source_root: source_root.clone(),
            staging_root: transaction_root.join("staging"),
            rollback_root: transaction_root.join("rollback"),
            recovery_root: recovery_root.clone(),
            env_path: dir.path().join(".tokenviewer/skill-env.sh"),
            links_path: dir
                .path()
                .join(".tokenviewer/skills-manager/linked_skills.json"),
            skill_ids: vec!["alpha".to_string()],
            has_skills_component: true,
            has_env_component: false,
            has_links_component: false,
            link_backups: Vec::new(),
            remote_sequences: BTreeMap::new(),
            remote_snapshot_ids: Vec::new(),
            observed_clock: None,
            pending_content_source: None,
            snapshot_id: "snapshot-missing-backup".to_string(),
            phase: JOURNAL_APPLYING.to_string(),
            committed: false,
            restore_required: Some(true),
        };
        write_json_atomic(&recovery_root.join("journal.json"), &journal).unwrap();

        let error = engine.recover_pending_apply().unwrap_err();
        assert_eq!(error.code, DeviceSyncErrorCode::RollbackFailed);
        assert_eq!(error.operation_id.as_deref(), Some(transaction_id));
        assert!(recovery_root.join("journal.json").exists());
        assert!(transaction_root.exists());
    }

    #[test]
    fn recovery_rejects_a_journal_with_a_mismatched_transaction_id() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        fs::create_dir_all(&source_root).unwrap();
        let mut engine =
            DeviceSyncEngine::new(dir.path().to_path_buf(), source_root.clone()).unwrap();

        let entry_id = "transaction-a";
        let recovery_root = dir
            .path()
            .join(".tokenviewer/device-sync/rollback")
            .join(entry_id);
        fs::create_dir_all(&recovery_root).unwrap();
        let journal = TransactionJournal {
            transaction_id: "transaction-b".to_string(),
            source_root: source_root.clone(),
            staging_root: source_root
                .parent()
                .unwrap()
                .join(".tokenviewer-device-sync-transaction-a/staging"),
            rollback_root: source_root
                .parent()
                .unwrap()
                .join(".tokenviewer-device-sync-transaction-a/rollback"),
            recovery_root: recovery_root.clone(),
            env_path: dir.path().join(".tokenviewer/skill-env.sh"),
            links_path: dir
                .path()
                .join(".tokenviewer/skills-manager/linked_skills.json"),
            skill_ids: Vec::new(),
            has_skills_component: false,
            has_env_component: false,
            has_links_component: false,
            link_backups: Vec::new(),
            remote_sequences: BTreeMap::new(),
            remote_snapshot_ids: Vec::new(),
            observed_clock: None,
            pending_content_source: None,
            snapshot_id: "snapshot-a".to_string(),
            phase: "committed".to_string(),
            committed: true,
            restore_required: None,
        };
        fs::write(
            recovery_root.join("journal.json"),
            serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();

        let error = engine.recover_pending_apply().unwrap_err();
        assert_eq!(error.code, DeviceSyncErrorCode::RollbackFailed);
        assert!(recovery_root.join("journal.json").exists());
    }

    #[test]
    fn recovery_of_prepared_transaction_does_not_remove_post_prepare_user_files() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("skills");
        fs::create_dir_all(&source_root).unwrap();
        let mut engine =
            DeviceSyncEngine::new(dir.path().to_path_buf(), source_root.clone()).unwrap();

        let transaction_id = "transaction-prepared";
        let transaction_root = source_root
            .parent()
            .unwrap()
            .join(format!(".tokenviewer-device-sync-{transaction_id}"));
        let recovery_root = dir
            .path()
            .join(".tokenviewer/device-sync/rollback")
            .join(transaction_id);
        fs::create_dir_all(transaction_root.join("staging")).unwrap();
        fs::create_dir_all(transaction_root.join("rollback/skills")).unwrap();
        fs::create_dir_all(&recovery_root).unwrap();

        let journal = TransactionJournal {
            transaction_id: transaction_id.to_string(),
            source_root: source_root.clone(),
            staging_root: transaction_root.join("staging"),
            rollback_root: transaction_root.join("rollback"),
            recovery_root: recovery_root.clone(),
            env_path: dir.path().join(".tokenviewer/skill-env.sh"),
            links_path: dir
                .path()
                .join(".tokenviewer/skills-manager/linked_skills.json"),
            skill_ids: vec!["new-skill".to_string()],
            has_skills_component: true,
            has_env_component: false,
            has_links_component: false,
            link_backups: Vec::new(),
            remote_sequences: BTreeMap::new(),
            remote_snapshot_ids: Vec::new(),
            observed_clock: None,
            pending_content_source: None,
            snapshot_id: "snapshot-prepared".to_string(),
            phase: "prepared".to_string(),
            committed: false,
            restore_required: None,
        };
        write_json_atomic(&recovery_root.join("journal.json"), &journal).unwrap();

        let user_file = source_root.join("new-skill/SKILL.md");
        fs::create_dir_all(user_file.parent().unwrap()).unwrap();
        fs::write(&user_file, "created after prepare\n").unwrap();

        let summary = engine.recover_pending_apply_detailed().unwrap();
        assert_eq!(summary.recovered, 1);
        assert_eq!(summary.rolled_back_transaction_ids, vec![transaction_id]);
        assert_eq!(
            fs::read_to_string(&user_file).unwrap(),
            "created after prepare\n"
        );
        assert!(!recovery_root.exists());
        assert!(!transaction_root.exists());
    }

    #[test]
    fn apply_failure_restores_generated_agent_links() {
        let remote = TempDir::new().unwrap();
        let home_a = TempDir::new().unwrap();
        let home_b = TempDir::new().unwrap();
        let source_a = home_a.path().join(".tokenviewer/skills");
        let source_b = home_b.path().join(".tokenviewer/skills");
        fs::create_dir_all(source_a.join("alpha")).unwrap();
        fs::create_dir_all(source_b.join("alpha")).unwrap();
        fs::write(source_a.join("alpha/SKILL.md"), "remote\n").unwrap();
        fs::write(source_b.join("alpha/SKILL.md"), "local\n").unwrap();

        let mut config_a = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
        config_a.enabled = true;
        let mut config_b = config_a.clone();
        config_b.profile_id = "profile-b".to_string();
        let mut engine_a =
            DeviceSyncEngine::for_test(home_a.path(), source_a.clone(), config_a).unwrap();
        let mut engine_b =
            DeviceSyncEngine::for_test(home_b.path(), source_b.clone(), config_b).unwrap();
        let db_a = Database::open(&home_a.path().join(".tokenviewer/data.db")).unwrap();
        let db_b = Database::open(&home_b.path().join(".tokenviewer/data.db")).unwrap();
        let mut skills_a = SkillsCore::new(
            &db_a,
            source_a.clone(),
            home_a.path().join(".tokenviewer/skills-manager"),
        )
        .unwrap();
        let mut skills_b = SkillsCore::new(
            &db_b,
            source_b.clone(),
            home_b.path().join(".tokenviewer/skills-manager"),
        )
        .unwrap();

        let codex_target_a = home_a.path().join("agent/codex-skills");
        let codex_target_b = home_b.path().join("agent/codex-skills");
        skills_a
            .registry
            .set_override(
                "codex",
                Some(codex_target_a.to_string_lossy().to_string()),
                Some(LinkType::Directory),
            )
            .unwrap();
        skills_b
            .registry
            .set_override(
                "codex",
                Some(codex_target_b.to_string_lossy().to_string()),
                Some(LinkType::Directory),
            )
            .unwrap();
        skills_a
            .registry
            .set_installed_for_test("codex", true)
            .unwrap();
        skills_b
            .registry
            .set_installed_for_test("codex", true)
            .unwrap();
        skills_b.registry.link_skill("codex", "alpha").unwrap();
        let codex_agent_b = skills_b.registry.find("codex").unwrap();
        skills_b
            .symlink
            .create_skill_link(&codex_agent_b, "alpha")
            .unwrap();
        skills_a.registry.link_skill("codex", "alpha").unwrap();

        skills_a
            .registry
            .set_override(
                "cursor",
                Some(
                    home_a
                        .path()
                        .join("agent/cursor-skills")
                        .to_string_lossy()
                        .to_string(),
                ),
                Some(LinkType::Directory),
            )
            .unwrap();
        skills_b
            .registry
            .set_override(
                "cursor",
                Some(
                    home_b
                        .path()
                        .join("agent/cursor-skills")
                        .to_string_lossy()
                        .to_string(),
                ),
                Some(LinkType::Directory),
            )
            .unwrap();
        skills_a
            .registry
            .set_installed_for_test("cursor", true)
            .unwrap();
        skills_b
            .registry
            .set_installed_for_test("cursor", true)
            .unwrap();
        skills_a.registry.link_skill("cursor", "alpha").unwrap();

        engine_a.create_vault("link-rollback-password").unwrap();
        engine_b.join_vault("link-rollback-password").unwrap();
        let push = engine_a.preview_push(&skills_a, &[], false).unwrap();
        engine_a.push(&skills_a, &[], &push.preview_token, false).unwrap();
        let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
        let transaction = engine_b
            .prepare_apply(&skills_b, &pull.preview_token)
            .unwrap();
        engine_b.inject_apply_failure_after_links();
        assert_eq!(
            engine_b
                .commit_apply(&mut skills_b, &transaction.transaction_id)
                .unwrap_err()
                .code,
            DeviceSyncErrorCode::ApplyFailed
        );
        assert_eq!(
            fs::read_to_string(source_b.join("alpha/SKILL.md")).unwrap(),
            "local\n"
        );
        assert!(codex_target_b.join("alpha").is_symlink());
        assert!(!home_b.path().join("agent/cursor-skills/alpha").exists());
        assert!(skills_b.registry.is_skill_linked("codex", "alpha"));
        assert!(!skills_b.registry.is_skill_linked("cursor", "alpha"));
    }

    #[test]
    fn apply_failure_restores_generated_single_file_pair() {
        let remote = TempDir::new().unwrap();
        let home_a = TempDir::new().unwrap();
        let home_b = TempDir::new().unwrap();
        let source_a = home_a.path().join(".tokenviewer/skills");
        let source_b = home_b.path().join(".tokenviewer/skills");
        let target_a = home_a.path().join("agent/AGENT.md");
        let target_b = home_b.path().join("agent/AGENT.md");
        let marker_b = home_b
            .path()
            .join("agent/AGENT.md.tokenviewer-managed.json");
        fs::create_dir_all(source_a.join("alpha")).unwrap();
        fs::create_dir_all(source_b.join("alpha")).unwrap();
        fs::write(source_a.join("alpha/SKILL.md"), "remote\n").unwrap();
        fs::write(source_b.join("alpha/SKILL.md"), "local\n").unwrap();

        let mut config_a = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
        config_a.enabled = true;
        let mut config_b = config_a.clone();
        config_b.profile_id = "profile-b".to_string();
        let mut engine_a =
            DeviceSyncEngine::for_test(home_a.path(), source_a.clone(), config_a).unwrap();
        let mut engine_b =
            DeviceSyncEngine::for_test(home_b.path(), source_b.clone(), config_b).unwrap();
        let db_a = Database::open(&home_a.path().join(".tokenviewer/data.db")).unwrap();
        let db_b = Database::open(&home_b.path().join(".tokenviewer/data.db")).unwrap();
        let mut skills_a = SkillsCore::new(
            &db_a,
            source_a.clone(),
            home_a.path().join(".tokenviewer/skills-manager"),
        )
        .unwrap();
        let mut skills_b = SkillsCore::new(
            &db_b,
            source_b.clone(),
            home_b.path().join(".tokenviewer/skills-manager"),
        )
        .unwrap();
        for (skills, target) in [(&mut skills_a, &target_a), (&mut skills_b, &target_b)] {
            skills
                .registry
                .set_override(
                    "codex",
                    Some(target.to_string_lossy().to_string()),
                    Some(LinkType::SingleFile),
                )
                .unwrap();
            skills
                .registry
                .set_installed_for_test("codex", true)
                .unwrap();
            skills.registry.link_skill("codex", "alpha").unwrap();
        }
        let agent_b = skills_b.registry.find("codex").unwrap();
        skills_b
            .symlink
            .rebuild_single_file(&agent_b, &["alpha".to_string()])
            .unwrap();
        let original_target = fs::read(&target_b).unwrap();
        let original_marker = fs::read(&marker_b).unwrap();

        engine_a
            .create_vault("single-file-rollback-password")
            .unwrap();
        engine_b
            .join_vault("single-file-rollback-password")
            .unwrap();
        let push = engine_a.preview_push(&skills_a, &[], false).unwrap();
        engine_a.push(&skills_a, &[], &push.preview_token, false).unwrap();
        let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
        let transaction = engine_b
            .prepare_apply(&skills_b, &pull.preview_token)
            .unwrap();
        engine_b.inject_apply_failure_after_links();
        assert_eq!(
            engine_b
                .commit_apply(&mut skills_b, &transaction.transaction_id)
                .unwrap_err()
                .code,
            DeviceSyncErrorCode::ApplyFailed
        );

        assert_eq!(fs::read(&target_b).unwrap(), original_target);
        assert_eq!(fs::read(&marker_b).unwrap(), original_marker);
        assert_eq!(
            fs::read_to_string(source_b.join("alpha/SKILL.md")).unwrap(),
            "local\n"
        );
        assert!(skills_b.registry.is_skill_linked("codex", "alpha"));
        engine_b
            .rollback_apply(&transaction.transaction_id)
            .unwrap();
    }
}
