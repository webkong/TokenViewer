use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::models::{
    valid_segment, DeviceIdentity, DeviceSyncConfig, DeviceSyncError, DeviceSyncErrorCode,
    DeviceSyncState, CONFIG_SCHEMA_VERSION, MAX_GRAPH_NODES, MAX_REMOTE_HEADS,
};

#[derive(Debug, Clone)]
pub struct DeviceSyncPaths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub device: PathBuf,
    pub state: PathBuf,
    pub staging: PathBuf,
    pub rollback: PathBuf,
    pub recovery_outcomes: PathBuf,
}

impl DeviceSyncPaths {
    pub fn new(home_dir: &Path) -> Self {
        let root = home_dir.join(".tokenviewer").join("device-sync");
        Self {
            config: root.join("config.json"),
            device: root.join("device.json"),
            state: root.join("state.json"),
            staging: root.join("staging"),
            rollback: root.join("rollback"),
            recovery_outcomes: root.join("recovery-outcomes.json"),
            root,
        }
    }

    pub fn ensure_root(&self) -> Result<(), DeviceSyncError> {
        create_private_dir(&self.root)
    }
}

pub fn load_config(paths: &DeviceSyncPaths) -> Result<DeviceSyncConfig, DeviceSyncError> {
    if !paths.config.exists() {
        return Ok(DeviceSyncConfig::default());
    }
    let config: DeviceSyncConfig = read_json(&paths.config)?;
    if config.schema_version != CONFIG_SCHEMA_VERSION {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ProtocolUnsupported,
            false,
        ));
    }
    Ok(config)
}

pub fn save_config(
    paths: &DeviceSyncPaths,
    config: &DeviceSyncConfig,
) -> Result<(), DeviceSyncError> {
    config.validate()?;
    paths.ensure_root()?;
    write_json_atomic(&paths.config, config)
}

pub fn load_or_create_identity(paths: &DeviceSyncPaths) -> Result<DeviceIdentity, DeviceSyncError> {
    if paths.device.exists() {
        let identity: DeviceIdentity = read_json(&paths.device)?;
        if identity.schema_version != CONFIG_SCHEMA_VERSION || !valid_segment(&identity.device_id) {
            return Err(DeviceSyncError::invalid_config("device.json"));
        }
        return Ok(identity);
    }

    paths.ensure_root()?;
    let identity = DeviceIdentity {
        schema_version: CONFIG_SCHEMA_VERSION,
        device_id: Uuid::new_v4().to_string(),
        display_name: "This Mac".to_string(),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    };
    write_json_atomic(&paths.device, &identity)?;
    Ok(identity)
}

pub fn load_state(paths: &DeviceSyncPaths) -> Result<DeviceSyncState, DeviceSyncError> {
    if !paths.state.exists() {
        return Ok(DeviceSyncState {
            schema_version: CONFIG_SCHEMA_VERSION,
            ..DeviceSyncState::default()
        });
    }
    let mut state: DeviceSyncState = read_json(&paths.state)?;
    if state.schema_version == 0 {
        state.schema_version = CONFIG_SCHEMA_VERSION;
    }
    if state.schema_version != CONFIG_SCHEMA_VERSION {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ProtocolUnsupported,
            false,
        ));
    }
    if state.max_remote_sequences.len() > MAX_REMOTE_HEADS
        || state.seen_snapshots.len() > MAX_GRAPH_NODES
        || state
            .max_remote_sequences
            .keys()
            .any(|device_id| !valid_segment(device_id))
        || state
            .seen_snapshots
            .iter()
            .any(|snapshot_id| !valid_segment(snapshot_id))
        || state
            .applied_snapshot_id
            .as_deref()
            .is_some_and(|snapshot_id| !valid_segment(snapshot_id))
        || (!state.clock.device_id.is_empty() && !valid_segment(&state.clock.device_id))
        || state.clock.wall_ms < 0
    {
        return Err(DeviceSyncError::invalid_config("state.json"));
    }
    Ok(state)
}

pub fn save_state(paths: &DeviceSyncPaths, state: &DeviceSyncState) -> Result<(), DeviceSyncError> {
    if state.schema_version != CONFIG_SCHEMA_VERSION {
        return Err(DeviceSyncError::invalid_config("state.schema_version"));
    }
    paths.ensure_root()?;
    write_json_atomic(&paths.state, state)
}

pub fn create_private_dir(path: &Path) -> Result<(), DeviceSyncError> {
    fs::create_dir_all(path).map_err(|error| {
        DeviceSyncError::apply_failed(format!("unable to create local state directory: {}", error))
    })?;
    set_private_permissions(path)?;
    Ok(())
}

pub fn write_json_atomic<T: serde::Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), DeviceSyncError> {
    let parent = path
        .parent()
        .ok_or_else(|| DeviceSyncError::apply_failed("state path has no parent"))?;
    create_private_dir(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        Uuid::new_v4()
    ));
    let data = serde_json::to_vec_pretty(value)
        .map_err(|_| DeviceSyncError::apply_failed("failed to encode local state"))?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        set_private_permissions(&temporary)?;
        file.write_all(&data)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        file.sync_all()
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        fs::rename(&temporary, path)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        set_private_permissions(path)?;
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sync_directory(path: &Path) -> Result<(), DeviceSyncError> {
    #[cfg(unix)]
    {
        let directory = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        directory
            .sync_all()
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
    }
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, DeviceSyncError> {
    let data =
        fs::read(path).map_err(|error| DeviceSyncError::invalid_config(error.to_string()))?;
    serde_json::from_slice(&data).map_err(|_| DeviceSyncError::invalid_config("invalid JSON"))
}

fn set_private_permissions(path: &Path) -> Result<(), DeviceSyncError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata =
            fs::metadata(path).map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        let mode = if metadata.is_dir() { 0o700 } else { 0o600 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
    }
    Ok(())
}
