use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;

pub const PROTOCOL_VERSION: u32 = 1;
pub const CONFIG_SCHEMA_VERSION: u32 = 1;
pub const TVSYNC_MAGIC: &[u8; 8] = b"TVSYNC\0\0";
pub const MAX_SINGLE_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_ENCRYPTED_SNAPSHOT_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum canonical manifest bytes carried inside a snapshot plaintext.
pub const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_ARCHIVE_ENTRIES: u64 = 20_000;
pub const MAX_RELATIVE_PATH_BYTES: usize = 1_024;
pub const MAX_COMPRESSION_RATIO: u64 = 100;
pub const MAX_ENVIRONMENT_VARIABLES: usize = 1_000;
pub const MAX_ENVIRONMENT_VALUE_BYTES: usize = 64 * 1024;
pub const MAX_MANIFEST_RECORDS: usize = 20_000;
pub const MAX_MANIFEST_COMPONENTS: usize = 32;
pub const MAX_METADATA_BYTES: u64 = 64 * 1024;
pub const MAX_HEAD_BYTES: u64 = MAX_METADATA_BYTES;
pub const MAX_GRAPH_NODES: usize = 10_000;
/// A linear history may be as deep as the graph node budget. The explicit
/// depth bound remains a defense for malformed graphs, while no longer
/// rejecting an ordinary sequence of more than 256 pushes.
pub const MAX_GRAPH_DEPTH: usize = MAX_GRAPH_NODES;
/// Budget for the bytes actually read from authenticated graph headers. This
/// is separate from complete snapshot payload bytes so large ancestors do not
/// consume the payload budget during metadata-only traversal.
pub const MAX_GRAPH_METADATA_BYTES: u64 = 16 * 1024 * 1024;
/// Budget for complete encrypted snapshot payloads downloaded during a
/// frontier read or Apply. Keep the legacy name as the v1 wire/API constant.
pub const MAX_GRAPH_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_OBJECT_KEY_BYTES: usize = 4 * 1024;
pub const MAX_OBJECT_KEY_SEGMENTS: usize = 64;
pub const MAX_REMOTE_HEADS: usize = 1_024;
pub const MAX_REMOTE_LIST_OBJECTS: usize = 10_000;
pub const PREVIEW_TTL_SECONDS: u64 = 10 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceSyncErrorCode {
    InvalidConfig,
    CredentialMissing,
    AuthenticationFailed,
    NetworkUnreachable,
    RateLimited,
    ProtocolUnsupported,
    RemoteDirectoryUnavailable,
    VaultNotFound,
    VaultAuthFailed,
    VaultAlreadyExists,
    ObjectTooLarge,
    ArchiveUnsafe,
    IntegrityFailed,
    OperationInProgress,
    StalePreview,
    RemoteChanged,
    ConflictRequiresResolution,
    LinkTargetOccupied,
    ApplyFailed,
    RollbackFailed,
    PartialFailure,
    ImmutableObjectConflict,
    RemoteRollbackDetected,
    RecoveryBlocked,
    InternalError,
}

impl DeviceSyncErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidConfig => "invalid_config",
            Self::CredentialMissing => "credential_missing",
            Self::AuthenticationFailed => "authentication_failed",
            Self::NetworkUnreachable => "network_unreachable",
            Self::RateLimited => "rate_limited",
            Self::ProtocolUnsupported => "protocol_unsupported",
            Self::RemoteDirectoryUnavailable => "remote_directory_unavailable",
            Self::VaultNotFound => "vault_not_found",
            Self::VaultAuthFailed => "vault_auth_failed",
            Self::VaultAlreadyExists => "vault_already_exists",
            Self::ObjectTooLarge => "object_too_large",
            Self::ArchiveUnsafe => "archive_unsafe",
            Self::IntegrityFailed => "integrity_failed",
            Self::OperationInProgress => "operation_in_progress",
            Self::StalePreview => "stale_preview",
            Self::RemoteChanged => "remote_changed",
            Self::ConflictRequiresResolution => "conflict_requires_resolution",
            Self::LinkTargetOccupied => "link_target_occupied",
            Self::ApplyFailed => "apply_failed",
            Self::RollbackFailed => "rollback_failed",
            Self::PartialFailure => "partial_failure",
            Self::ImmutableObjectConflict => "immutable_object_conflict",
            Self::RemoteRollbackDetected => "remote_rollback_detected",
            Self::RecoveryBlocked => "recovery_blocked",
            Self::InternalError => "internal_error",
        }
    }

    pub fn message_key(self) -> &'static str {
        match self {
            Self::InvalidConfig => "deviceSync.error.invalidConfig",
            Self::CredentialMissing => "deviceSync.error.credentialMissing",
            Self::AuthenticationFailed => "deviceSync.error.authenticationFailed",
            Self::NetworkUnreachable => "deviceSync.error.networkUnreachable",
            Self::RateLimited => "deviceSync.error.rateLimited",
            Self::ProtocolUnsupported => "deviceSync.error.protocolUnsupported",
            Self::RemoteDirectoryUnavailable => "deviceSync.error.remoteDirectoryUnavailable",
            Self::VaultNotFound => "deviceSync.error.vaultNotFound",
            Self::VaultAuthFailed => "deviceSync.error.vaultAuthFailed",
            Self::VaultAlreadyExists => "deviceSync.error.vaultAlreadyExists",
            Self::ObjectTooLarge => "deviceSync.error.objectTooLarge",
            Self::ArchiveUnsafe => "deviceSync.error.archiveUnsafe",
            Self::IntegrityFailed => "deviceSync.error.integrityFailed",
            Self::OperationInProgress => "deviceSync.error.operationInProgress",
            Self::StalePreview => "deviceSync.error.stalePreview",
            Self::RemoteChanged => "deviceSync.error.remoteChanged",
            Self::ConflictRequiresResolution => "deviceSync.error.conflictRequiresResolution",
            Self::LinkTargetOccupied => "deviceSync.error.linkTargetOccupied",
            Self::ApplyFailed => "deviceSync.error.applyFailed",
            Self::RollbackFailed => "deviceSync.error.rollbackFailed",
            Self::PartialFailure => "deviceSync.error.partialFailure",
            Self::ImmutableObjectConflict => "deviceSync.error.immutableObjectConflict",
            Self::RemoteRollbackDetected => "deviceSync.error.remoteRollbackDetected",
            Self::RecoveryBlocked => "deviceSync.error.recoveryBlocked",
            Self::InternalError => "deviceSync.error.internal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceSyncError {
    pub code: DeviceSyncErrorCode,
    pub message_key: String,
    #[serde(default)]
    pub arguments: BTreeMap<String, String>,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
}

impl DeviceSyncError {
    pub fn new(code: DeviceSyncErrorCode, retryable: bool) -> Self {
        Self {
            code,
            message_key: code.message_key().to_string(),
            arguments: BTreeMap::new(),
            retryable,
            operation_id: None,
        }
    }

    pub fn with_argument(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.arguments.insert(name.into(), value.into());
        self
    }

    pub fn with_operation_id(mut self, operation_id: impl Into<String>) -> Self {
        self.operation_id = Some(operation_id.into());
        self
    }

    pub fn invalid_config(message: impl Into<String>) -> Self {
        Self::new(DeviceSyncErrorCode::InvalidConfig, false).with_argument("detail", message)
    }

    pub fn archive_unsafe(message: impl Into<String>) -> Self {
        Self::new(DeviceSyncErrorCode::ArchiveUnsafe, false).with_argument("detail", message)
    }

    pub fn apply_failed(message: impl Into<String>) -> Self {
        Self::new(DeviceSyncErrorCode::ApplyFailed, false).with_argument("detail", message)
    }
}

impl fmt::Display for DeviceSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.code.as_str())
    }
}

impl std::error::Error for DeviceSyncError {}

impl From<std::io::Error> for DeviceSyncError {
    fn from(error: std::io::Error) -> Self {
        Self::apply_failed(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncComponent {
    Skills,
    AgentLinks,
    SkillEnv,
    Preferences,
    Usage,
}

impl SyncComponent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Skills => "skills",
            Self::AgentLinks => "agent_links",
            Self::SkillEnv => "skill_env",
            Self::Preferences => "preferences",
            Self::Usage => "usage",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeMode {
    All,
    Selected,
}

impl Default for ScopeMode {
    fn default() -> Self {
        Self::Selected
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SkillScope {
    #[serde(default)]
    pub mode: ScopeMode,
    #[serde(default)]
    pub skill_ids: Vec<String>,
}

impl Default for SkillScope {
    fn default() -> Self {
        Self {
            mode: ScopeMode::Selected,
            skill_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnvironmentScope {
    #[serde(default)]
    pub mode: ScopeMode,
    #[serde(default)]
    pub names: Vec<String>,
}

impl Default for EnvironmentScope {
    fn default() -> Self {
        Self {
            mode: ScopeMode::Selected,
            names: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderConfig {
    pub kind: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub remote_prefix: String,
    #[serde(default)]
    pub local_root: Option<PathBuf>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub path_style: Option<bool>,
    #[serde(default)]
    pub insecure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceSyncConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub enabled: bool,
    pub profile_id: String,
    #[serde(default)]
    pub vault_id: Option<String>,
    #[serde(default)]
    pub provider: Option<ProviderConfig>,
    #[serde(default = "default_content_source")]
    pub content_source: String,
    #[serde(default)]
    pub components: Vec<SyncComponent>,
    #[serde(default)]
    pub skill_scope: SkillScope,
    #[serde(default)]
    pub environment_scope: EnvironmentScope,
    #[serde(default)]
    pub auto_sync: bool,
}

fn default_schema_version() -> u32 {
    CONFIG_SCHEMA_VERSION
}

fn default_content_source() -> String {
    "cloud".to_string()
}

impl Default for DeviceSyncConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            enabled: false,
            profile_id: String::new(),
            vault_id: None,
            provider: None,
            content_source: default_content_source(),
            components: vec![SyncComponent::Skills, SyncComponent::AgentLinks, SyncComponent::SkillEnv, SyncComponent::Preferences, SyncComponent::Usage],
            skill_scope: SkillScope::default(),
            environment_scope: EnvironmentScope::default(),
            auto_sync: false,
        }
    }
}

impl DeviceSyncConfig {
    pub fn for_local_test(remote_root: &std::path::Path, vault_id: &str) -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            enabled: false,
            profile_id: "profile-a".to_string(),
            vault_id: Some(vault_id.to_string()),
            provider: Some(ProviderConfig {
                kind: "local_folder".to_string(),
                endpoint: None,
                remote_prefix: "tokenviewer-sync".to_string(),
                local_root: Some(remote_root.to_path_buf()),
                username: None,
                bucket: None,
                region: None,
                path_style: None,
                insecure: false,
            }),
            content_source: "cloud".to_string(),
            components: vec![SyncComponent::Skills, SyncComponent::AgentLinks],
            skill_scope: SkillScope {
                mode: ScopeMode::All,
                skill_ids: Vec::new(),
            },
            environment_scope: EnvironmentScope::default(),
            auto_sync: false,
        }
    }

    pub fn has_component(&self, component: SyncComponent) -> bool {
        self.components.iter().any(|item| *item == component)
    }

    pub fn validate(&self) -> Result<(), DeviceSyncError> {
        if self.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ProtocolUnsupported,
                false,
            ));
        }
        if !valid_segment(&self.profile_id) {
            return Err(DeviceSyncError::invalid_config("profile_id"));
        }
        if let Some(vault_id) = &self.vault_id {
            if !valid_segment(vault_id) {
                return Err(DeviceSyncError::invalid_config("vault_id"));
            }
        }
        if self.content_source != "cloud" && self.content_source != "git" {
            return Err(DeviceSyncError::invalid_config("content_source"));
        }
        if let Some(provider) = self.provider.as_ref() {
            match provider.kind.as_str() {
                "local_folder" => {
                    if provider.local_root.is_none() {
                        return Err(DeviceSyncError::invalid_config("provider.local_root"));
                    }
                    if !valid_remote_prefix(&provider.remote_prefix) {
                        return Err(DeviceSyncError::invalid_config("provider.remote_prefix"));
                    }
                }
                "webdav" => {
                    let endpoint = provider
                        .endpoint
                        .as_deref()
                        .ok_or_else(|| DeviceSyncError::invalid_config("provider.endpoint"))?;
                    crate::device_sync::store::validate_webdav_endpoint(
                        endpoint,
                        provider.insecure,
                    )?;
                    let username = provider
                        .username
                        .as_deref()
                        .ok_or_else(|| DeviceSyncError::invalid_config("provider.username"))?;
                    if username.is_empty()
                        || username.contains(':')
                        || username.chars().any(char::is_control)
                    {
                        return Err(DeviceSyncError::invalid_config("provider.username"));
                    }
                    if !valid_remote_prefix(&provider.remote_prefix) {
                        return Err(DeviceSyncError::invalid_config("provider.remote_prefix"));
                    }
                }
                _ => {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::ProtocolUnsupported,
                        false,
                    ));
                }
            }
        } else if self.enabled {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::CredentialMissing,
                false,
            ));
        }
        if self.components.len() > 5
            || self
                .components
                .iter()
                .enumerate()
                .any(|(index, component)| self.components[..index].contains(component))
        {
            return Err(DeviceSyncError::invalid_config("components"));
        }
        let mut skill_ids = BTreeSet::new();
        for skill_id in &self.skill_scope.skill_ids {
            if !valid_segment(skill_id) || !skill_ids.insert(skill_id) {
                return Err(DeviceSyncError::invalid_config("skill_scope.skill_ids"));
            }
        }
        let mut environment_names = BTreeSet::new();
        for name in &self.environment_scope.names {
            if !valid_environment_name(name) || !environment_names.insert(name) {
                return Err(DeviceSyncError::invalid_config("environment_scope.names"));
            }
        }
        Ok(())
    }
}

pub fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains('\0')
        && !value.chars().any(char::is_control)
        && value.len() <= MAX_RELATIVE_PATH_BYTES
}

pub fn valid_remote_prefix(value: &str) -> bool {
    value.is_empty() || (value.len() <= MAX_OBJECT_KEY_BYTES && value.split('/').all(valid_segment))
}

pub fn valid_environment_name(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_RELATIVE_PATH_BYTES {
        return false;
    }
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first == '_' || first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceIdentity {
    pub schema_version: u32,
    pub device_id: String,
    pub display_name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct DeviceSyncState {
    pub schema_version: u32,
    #[serde(default)]
    pub applied_snapshot_id: Option<String>,
    #[serde(default)]
    pub local_sequence: u64,
    #[serde(default)]
    pub max_remote_sequences: BTreeMap<String, u64>,
    #[serde(default)]
    pub seen_snapshots: BTreeSet<String>,
    #[serde(default)]
    pub last_result: Option<SyncResultSummary>,
    #[serde(default)]
    pub pending_content_source: Option<PendingContentSource>,
    /// Per-device HLC watermark. Missing in early v1 state files and lazily
    /// initialized from the persisted device identity.
    #[serde(default)]
    pub clock: Hlc,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct SyncResultSummary {
    pub direction: String,
    pub snapshot_id: Option<String>,
    pub completed_at: String,
    pub message_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PendingContentSource {
    pub content_source: String,
    #[serde(default)]
    pub repository: Option<GitRepositoryHint>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default, PartialOrd, Ord,
)]
pub struct Hlc {
    pub wall_ms: i64,
    pub counter: u64,
    pub device_id: String,
}

impl Hlc {
    pub fn zero(device_id: impl Into<String>) -> Self {
        Self {
            wall_ms: 0,
            counter: 0,
            device_id: device_id.into(),
        }
    }

    pub fn current_wall_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(i64::MAX as u128) as i64
    }

    pub fn now(device_id: impl Into<String>) -> Self {
        Self {
            wall_ms: Self::current_wall_ms(),
            counter: 0,
            device_id: device_id.into(),
        }
    }

    /// Advance a device-local HLC without mutating the old watermark. This is
    /// intentionally fallible at the absolute integer boundary instead of
    /// wrapping or saturating into a value that would sort incorrectly.
    pub fn next_after(previous: &Self, wall_ms: i64, device_id: impl Into<String>) -> Option<Self> {
        let device_id = device_id.into();
        let wall_ms = wall_ms.max(0);
        if wall_ms > previous.wall_ms {
            return Some(Self {
                wall_ms,
                counter: 0,
                device_id,
            });
        }
        if previous.counter < u64::MAX {
            return Some(Self {
                wall_ms: previous.wall_ms,
                counter: previous.counter + 1,
                device_id,
            });
        }
        Some(Self {
            wall_ms: previous.wall_ms.checked_add(1)?,
            counter: 0,
            device_id,
        })
    }

    /// Apply a remote timestamp to the local HLC using the standard receive
    /// rule. The returned clock always keeps the local device identity; a
    /// remote device ID is only an ordering input.
    pub fn receive_after(
        previous: &Self,
        remote: Option<&Self>,
        wall_ms: i64,
        device_id: impl Into<String>,
    ) -> Option<Self> {
        let device_id = device_id.into();
        let now = wall_ms.max(0);
        let remote = remote.filter(|clock| !clock.device_id.is_empty() && clock.wall_ms >= 0);
        let remote_wall = remote.map(|clock| clock.wall_ms).unwrap_or(0);
        let next_wall = previous.wall_ms.max(remote_wall).max(now);

        let counter = if next_wall == previous.wall_ms && next_wall == remote_wall {
            previous
                .counter
                .max(remote.map(|clock| clock.counter).unwrap_or(0))
                .checked_add(1)
        } else if next_wall == previous.wall_ms {
            previous.counter.checked_add(1)
        } else if next_wall == remote_wall {
            remote
                .map(|clock| clock.counter.checked_add(1))
                .flatten()
        } else {
            Some(0)
        };

        match counter {
            Some(counter) => Some(Self {
                wall_ms: next_wall,
                counter,
                device_id,
            }),
            None => Some(Self {
                wall_ms: next_wall.checked_add(1)?,
                counter: 0,
                device_id,
            }),
        }
    }

    /// Record a remote timestamp without creating a local event. This advances
    /// the local watermark to the observed logical time while retaining the
    /// local device identity; the next local event must use `receive_after`.
    pub fn observe(previous: &Self, remote: Option<&Self>, device_id: impl Into<String>) -> Self {
        let device_id = device_id.into();
        let remote = remote.filter(|clock| !clock.device_id.is_empty() && clock.wall_ms >= 0);
        let Some(remote) = remote else {
            return Self {
                wall_ms: previous.wall_ms,
                counter: previous.counter,
                device_id,
            };
        };
        let wall_ms = previous.wall_ms.max(remote.wall_ms);
        let counter = if wall_ms == previous.wall_ms && wall_ms == remote.wall_ms {
            previous.counter.max(remote.counter)
        } else if wall_ms == previous.wall_ms {
            previous.counter
        } else {
            remote.counter
        };
        Self {
            wall_ms,
            counter,
            device_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ComponentSummary {
    pub sha256: String,
    pub bytes: u64,
    pub entries: u64,
    #[serde(default)]
    pub records: u64,
}

impl ComponentSummary {
    pub fn empty() -> Self {
        Self {
            sha256: String::new(),
            bytes: 0,
            entries: 0,
            records: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecordMetadata {
    pub record_id: String,
    pub hlc: Hlc,
    pub last_modified_by: String,
    #[serde(default)]
    pub tombstone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillFileKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SkillFileRecord {
    pub path: String,
    pub kind: SkillFileKind,
    pub size: u64,
    pub sha256: String,
    pub mode: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SkillRecord {
    pub skill_id: String,
    pub metadata: RecordMetadata,
    pub files: Vec<SkillFileRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentLinkRecord {
    pub agent_id: String,
    pub skill_id: String,
    pub metadata: RecordMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnvironmentRecord {
    pub name: String,
    pub value: String,
    pub referenced_by_skill_ids: Vec<String>,
    pub metadata: RecordMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PreferencesRecord {
    pub enabled_agent_ids: Vec<String>,
    pub metadata: RecordMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct SnapshotRecords {
    #[serde(default)]
    pub skills: Vec<SkillRecord>,
    #[serde(default)]
    pub agent_links: Vec<AgentLinkRecord>,
    #[serde(default)]
    pub skill_env: Vec<EnvironmentRecord>,
    #[serde(default)]
    pub preferences: Option<PreferencesRecord>,
    #[serde(default)]
    pub usage: Vec<crate::models::UsageRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tombstone {
    pub component: String,
    pub record_id: String,
    pub hlc: Hlc,
    pub last_modified_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotLimits {
    pub archive_bytes: u64,
    pub expanded_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotManifest {
    pub format: String,
    pub protocol_version: u32,
    pub snapshot_id: String,
    pub vault_id: String,
    pub device_id: String,
    #[serde(default)]
    pub parent_ids: Vec<String>,
    pub created_at: String,
    /// Added after the first v1 snapshots. Missing values decode to the
    /// protocol default and are kept compatible by the authenticated reader.
    #[serde(default)]
    pub clock: Hlc,
    pub content_source: String,
    pub components: BTreeMap<String, ComponentSummary>,
    #[serde(default)]
    pub tombstones: Vec<Tombstone>,
    pub limits: SnapshotLimits,
    #[serde(default)]
    pub records: SnapshotRecords,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_repository: Option<GitRepositoryHint>,
}

impl SnapshotManifest {
    pub fn new(
        snapshot_id: String,
        vault_id: String,
        device_id: String,
        parent_ids: Vec<String>,
        content_source: String,
        components: BTreeMap<String, ComponentSummary>,
    ) -> Self {
        Self {
            format: "tokenviewer-snapshot".to_string(),
            protocol_version: PROTOCOL_VERSION,
            snapshot_id,
            vault_id,
            device_id: device_id.clone(),
            parent_ids,
            created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            clock: Hlc::now(device_id),
            content_source,
            components,
            tombstones: Vec::new(),
            limits: SnapshotLimits {
                archive_bytes: 0,
                expanded_bytes: 0,
            },
            records: SnapshotRecords::default(),
            git_repository: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitRepositoryHint {
    pub provider: Option<String>,
    pub branch: String,
    pub remote_url: Option<String>,
    pub commit_oid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotPayload {
    pub manifest: SnapshotManifest,
    pub archive: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VaultMetadata {
    pub format: String,
    pub protocol_version: u32,
    pub vault_id: String,
    pub created_at: String,
    pub kdf: KdfParameters,
    pub key_wrap: KeyWrap,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KdfParameters {
    pub name: String,
    pub version: u32,
    pub salt_b64: String,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KeyWrap {
    pub algorithm: String,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Head {
    pub format: String,
    pub protocol_version: u32,
    pub vault_id: String,
    pub device_id: String,
    pub snapshot_id: String,
    pub parent_ids: Vec<String>,
    pub updated_at: String,
    pub snapshot_sha256: String,
    pub sequence: u64,
    pub mac_b64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HeadUnsigned {
    pub format: String,
    pub protocol_version: u32,
    pub vault_id: String,
    pub device_id: String,
    pub snapshot_id: String,
    pub parent_ids: Vec<String>,
    pub updated_at: String,
    pub snapshot_sha256: String,
    pub sequence: u64,
}

impl Head {
    pub fn unsigned(&self) -> HeadUnsigned {
        HeadUnsigned {
            format: self.format.clone(),
            protocol_version: self.protocol_version,
            vault_id: self.vault_id.clone(),
            device_id: self.device_id.clone(),
            snapshot_id: self.snapshot_id.clone(),
            parent_ids: self.parent_ids.clone(),
            updated_at: self.updated_at.clone(),
            snapshot_sha256: self.snapshot_sha256.clone(),
            sequence: self.sequence,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct ChangeCounts {
    pub added: u64,
    pub updated: u64,
    pub deleted: u64,
    pub conflicts: u64,
    pub skipped: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct PreviewSummary {
    pub skills: ChangeCounts,
    pub agent_links: ChangeCounts,
    pub skill_env: ChangeCounts,
    pub preferences: ChangeCounts,
    #[serde(default)]
    pub usage: ChangeCounts,
    pub warnings: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PreviewItem {
    pub component: String,
    pub action: String,
    pub id: String,
    pub detail: Option<String>,
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PreviewResponse {
    pub preview_token: String,
    pub direction: String,
    pub expires_at: String,
    pub remote_head_fingerprint: String,
    pub local_fingerprint: String,
    pub summary: PreviewSummary,
    pub items: Vec<PreviewItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyncResult {
    pub direction: String,
    pub snapshot_id: Option<String>,
    pub operation_id: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PrepareApplyResponse {
    pub transaction_id: String,
    pub preference_mutations: Vec<PreferenceMutation>,
    /// Local-only recovery directory; it contains no credentials or payload
    /// secrets and lets Swift associate its journal with Rust recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PreferenceMutation {
    pub key: String,
    pub enabled_agent_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct DeviceSyncRecoverySummary {
    pub recovered: u64,
    #[serde(default)]
    pub rolled_back_transaction_ids: Vec<String>,
    #[serde(default)]
    pub committed_transaction_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceSyncStatus {
    pub config: DeviceSyncConfig,
    pub identity: DeviceIdentity,
    pub state: DeviceSyncState,
    pub remote_head_fingerprint: Option<String>,
    pub frontier: Vec<String>,
    #[serde(default)]
    pub recovery_blocked: bool,
}

/// The master key is returned only for the create/join call so Swift can place
/// it in the dedicated Keychain item. It is never part of config or status.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceSyncVaultSession {
    pub status: DeviceSyncStatus,
    pub master_key_b64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotHeader {
    pub format: String,
    pub protocol_version: u32,
    pub vault_id: String,
    pub object_type: String,
    pub snapshot_id: String,
    /// Added as an optional-on-read v1 field so graph traversal can inspect
    /// parentage without downloading and decrypting the complete snapshot.
    #[serde(default)]
    pub parent_ids: Vec<String>,
    pub payload_sha256: String,
    /// HMAC over the complete graph-relevant header. Optional for old v1
    /// objects; those objects require full payload authentication before their
    /// parent list can be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_mac_b64: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotListItem {
    pub snapshot_id: String,
    pub device_id: String,
    pub parent_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_frontier: bool,
}
