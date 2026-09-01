use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::Engine;
use uuid::Uuid;

use super::models::{
    valid_environment_name, DeviceSyncError, DeviceSyncErrorCode, MAX_ENVIRONMENT_VALUE_BYTES,
    MAX_ENVIRONMENT_VARIABLES,
};

pub const MANAGED_HEADER: &str =
    "# Managed by TokenViewer. Edit values in TokenViewer to avoid conflicts.";
pub const VALUE_PREFIX: &str = "# tokenviewer-value ";

/// The Rust implementation intentionally mirrors SkillEnvironmentManager's
/// managed file format. Values are parsed from metadata lines, never from the
/// executable shell statements.
pub struct SkillEnvironmentStore {
    path: PathBuf,
}

impl SkillEnvironmentStore {
    pub fn for_home(home_dir: &Path) -> Self {
        Self {
            path: home_dir.join(".tokenviewer").join("skill-env.sh"),
        }
    }

    pub fn from_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&self) -> Result<BTreeMap<String, String>, DeviceSyncError> {
        read_values(&self.path)
    }

    pub fn merge_and_write(
        &self,
        updates: &BTreeMap<String, String>,
        removals: &[String],
    ) -> Result<(), DeviceSyncError> {
        let mut values = self.read()?;
        for name in removals {
            if !valid_environment_name(name) {
                return Err(DeviceSyncError::invalid_config("environment name"));
            }
            values.remove(name);
        }
        for (name, value) in updates {
            validate_value(name, value)?;
            values.insert(name.clone(), value.clone());
        }
        write_values(&self.path, &values)
    }
}

pub fn read_values(path: &Path) -> Result<BTreeMap<String, String>, DeviceSyncError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::LinkTargetOccupied,
                false,
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BTreeMap::new());
        }
        Err(error) => {
            return Err(
                DeviceSyncError::new(DeviceSyncErrorCode::ApplyFailed, false)
                    .with_argument("detail", error.to_string()),
            );
        }
    }
    let content = fs::read_to_string(path).map_err(|error| {
        DeviceSyncError::new(DeviceSyncErrorCode::ApplyFailed, false)
            .with_argument("detail", error.to_string())
    })?;
    let mut values = BTreeMap::new();
    for line in content
        .lines()
        .filter(|line| line.starts_with(VALUE_PREFIX))
    {
        let payload = &line[VALUE_PREFIX.len()..];
        let Some((name, encoded)) = payload.split_once(' ') else {
            continue;
        };
        if !valid_environment_name(name) {
            continue;
        }
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| DeviceSyncError::invalid_config("skill-env.sh encoding"))?;
        if decoded.len() > MAX_ENVIRONMENT_VALUE_BYTES {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ObjectTooLarge,
                false,
            ));
        }
        let value = String::from_utf8(decoded)
            .map_err(|_| DeviceSyncError::invalid_config("skill-env.sh value"))?;
        if values.len() >= MAX_ENVIRONMENT_VARIABLES && !values.contains_key(name) {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ObjectTooLarge,
                false,
            ));
        }
        values.insert(name.to_string(), value);
    }
    Ok(values)
}

pub fn write_values(path: &Path, values: &BTreeMap<String, String>) -> Result<(), DeviceSyncError> {
    if values.len() > MAX_ENVIRONMENT_VARIABLES {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ObjectTooLarge,
            false,
        ));
    }
    for (name, value) in values {
        validate_value(name, value)?;
    }

    let parent = path
        .parent()
        .ok_or_else(|| DeviceSyncError::apply_failed("environment path has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
    set_mode(parent, 0o700)?;
    let temporary = parent.join(format!(".skill-env.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        set_mode(&temporary, 0o600)?;
        let mut lines = vec![MANAGED_HEADER.to_string()];
        for (name, value) in values {
            let encoded = base64::engine::general_purpose::STANDARD.encode(value.as_bytes());
            lines.push(format!("{}{} {}", VALUE_PREFIX, name, encoded));
            lines.push(format!("export {}={}", name, shell_quote(value)));
        }
        lines.push(String::new());
        file.write_all(lines.join("\n").as_bytes())
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        file.sync_all()
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        // A user can replace the managed path while the temporary file is
        // being prepared. Never let the final rename take over that symlink.
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::LinkTargetOccupied,
                    false,
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(DeviceSyncError::apply_failed(error.to_string()));
            }
        }
        fs::rename(&temporary, path)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        set_mode(path, 0o600)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn validate_value(name: &str, value: &str) -> Result<(), DeviceSyncError> {
    if !valid_environment_name(name) {
        return Err(DeviceSyncError::invalid_config("environment name"));
    }
    if value.as_bytes().len() > MAX_ENVIRONMENT_VALUE_BYTES || value.contains('\0') {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ObjectTooLarge,
            false,
        ));
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn set_mode(path: &Path, mode: u32) -> Result<(), DeviceSyncError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn round_trips_values_in_the_existing_managed_format() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("skill-env.sh");
        let values = BTreeMap::from([
            ("ALPHA_KEY".to_string(), "a'b\nvalue".to_string()),
            ("BETA".to_string(), "two".to_string()),
        ]);
        write_values(&path, &values).unwrap();
        assert_eq!(read_values(&path).unwrap(), values);
        assert!(fs::read_to_string(path)
            .unwrap()
            .contains("export ALPHA_KEY='a'\"'\"'b\nvalue'"));
    }

    #[test]
    #[cfg(unix)]
    fn refuses_to_replace_a_symlink_target() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("skill-env.sh");
        let protected = dir.path().join("protected.sh");
        fs::write(&protected, "user-owned\n").unwrap();
        std::os::unix::fs::symlink(&protected, &path).unwrap();

        let values = BTreeMap::from([("TOKEN".to_string(), "replacement".to_string())]);
        let error = write_values(&path, &values).unwrap_err();

        assert_eq!(error.code, DeviceSyncErrorCode::LinkTargetOccupied);
        assert_eq!(fs::read_to_string(&protected).unwrap(), "user-owned\n");
        assert_eq!(fs::read_link(&path).unwrap(), protected);
    }
}
