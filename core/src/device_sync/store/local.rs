use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd};

use uuid::Uuid;

use super::{
    ConnectionReport, DeleteCondition, ObjectKey, ObjectMeta, ObjectPage, ObjectPrefix,
    ObjectStore, PutCondition, StoreCapabilities,
};
use crate::device_sync::config::create_private_dir;
use crate::device_sync::crypto::sha256_hex;
use crate::device_sync::models::{
    DeviceSyncError, DeviceSyncErrorCode, MAX_ENCRYPTED_SNAPSHOT_BYTES,
};

#[derive(Debug, Clone)]
pub struct LocalFolderStore {
    root: PathBuf,
}

struct OpenedObject {
    file: File,
    metadata: fs::Metadata,
}

impl LocalFolderStore {
    pub fn new(root: PathBuf) -> Result<Self, DeviceSyncError> {
        match fs::symlink_metadata(&root) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ArchiveUnsafe,
                    false,
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(DeviceSyncError::invalid_config("local store root"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                create_private_dir(&root)?;
            }
            Err(error) => return Err(DeviceSyncError::apply_failed(error.to_string())),
        }
        if !root.is_dir() {
            return Err(DeviceSyncError::invalid_config("local store root"));
        }
        let root = fs::canonicalize(&root)
            .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::ArchiveUnsafe, false))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, key: &ObjectKey) -> Result<PathBuf, DeviceSyncError> {
        let path = self.root.join(key.segments().iter().collect::<PathBuf>());
        validate_real_parent_path(&self.root, &path)?;
        Ok(path)
    }

    fn metadata_for(&self, key: &ObjectKey) -> Result<Option<ObjectMeta>, DeviceSyncError> {
        let Some(mut opened) = self.open_object(key)? else {
            return Ok(None);
        };
        let etag = file_etag(&mut opened.file)?;
        if !metadata_matches(
            &opened.metadata,
            &opened
                .file
                .metadata()
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?,
        ) {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::RemoteChanged,
                true,
            ));
        }
        Ok(Some(ObjectMeta {
            key: key.clone(),
            size: opened.metadata.len(),
            etag: Some(etag),
        }))
    }

    fn open_object(&self, key: &ObjectKey) -> Result<Option<OpenedObject>, DeviceSyncError> {
        let path = self.path_for(key)?;
        let path_metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(DeviceSyncError::apply_failed(error.to_string())),
        };
        if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ArchiveUnsafe,
                false,
            ));
        }
        if path_metadata.len() > MAX_ENCRYPTED_SNAPSHOT_BYTES {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ObjectTooLarge,
                false,
            ));
        }
        let file = match open_regular_file_nofollow(&self.root, &path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ArchiveUnsafe,
                    false,
                ))
            }
        };
        let opened_metadata = file
            .metadata()
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        if !opened_metadata.is_file() || !metadata_matches(&path_metadata, &opened_metadata) {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::RemoteChanged,
                true,
            ));
        }
        Ok(Some(OpenedObject {
            file,
            metadata: opened_metadata,
        }))
    }

    fn read_object(&self, key: &ObjectKey) -> Result<Vec<u8>, DeviceSyncError> {
        let Some(mut opened) = self.open_object(key)? else {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::VaultNotFound,
                false,
            ));
        };
        let object_size = opened.metadata.len();
        let mut data = Vec::with_capacity(object_size.min(1024 * 1024) as usize);
        opened
            .file
            .read_to_end(&mut data)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        if !metadata_matches(
            &opened.metadata,
            &opened
                .file
                .metadata()
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?,
        ) {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::RemoteChanged,
                true,
            ));
        }
        if data.len() as u64 != object_size {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::RemoteChanged,
                true,
            ));
        }
        Ok(data)
    }

    fn ensure_parent(&self, path: &Path) -> Result<(), DeviceSyncError> {
        let parent = path
            .parent()
            .ok_or_else(|| DeviceSyncError::invalid_config("object path parent"))?;
        ensure_real_directory_tree(&self.root, parent)?;
        set_mode(parent, 0o700)?;
        Ok(())
    }
}

impl ObjectStore for LocalFolderStore {
    fn capabilities(&self) -> StoreCapabilities {
        StoreCapabilities {
            conditional_put: true,
            conditional_delete: true,
            list: true,
            delete: true,
        }
    }

    fn test_connection(&self) -> Result<ConnectionReport, DeviceSyncError> {
        create_private_dir(&self.root)?;
        Ok(ConnectionReport {
            provider: "local_folder".to_string(),
            writable: true,
        })
    }

    fn head(&self, key: &ObjectKey) -> Result<Option<ObjectMeta>, DeviceSyncError> {
        self.metadata_for(key)
    }

    fn get_bounded(
        &self,
        key: &ObjectKey,
        max_bytes: u64,
        sink: &mut dyn Write,
    ) -> Result<ObjectMeta, DeviceSyncError> {
        let Some(mut opened) = self.open_object(key)? else {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::VaultNotFound,
                false,
            ));
        };
        if opened.metadata.len() > max_bytes {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ObjectTooLarge,
                false,
            ));
        }
        let mut total = 0u64;
        let mut buffer = [0u8; 32 * 1024];
        loop {
            let count = opened
                .file
                .read(&mut buffer)
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false))?;
            if total > max_bytes {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ObjectTooLarge,
                    false,
                ));
            }
            sink.write_all(&buffer[..count])
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        }
        if total != opened.metadata.len() {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::IntegrityFailed,
                false,
            ));
        }
        if !metadata_matches(
            &opened.metadata,
            &opened
                .file
                .metadata()
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?,
        ) {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::RemoteChanged,
                true,
            ));
        }
        Ok(ObjectMeta {
            key: key.clone(),
            size: opened.metadata.len(),
            etag: None,
        })
    }

    fn get_prefix(
        &self,
        key: &ObjectKey,
        max_bytes: u64,
        sink: &mut dyn Write,
    ) -> Result<ObjectMeta, DeviceSyncError> {
        let Some(mut opened) = self.open_object(key)? else {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::VaultNotFound,
                false,
            ));
        };
        let object_size = opened.metadata.len();
        if !opened.metadata.is_file() {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::RemoteChanged,
                true,
            ));
        }
        let mut remaining = max_bytes;
        let mut buffer = [0u8; 32 * 1024];
        while remaining > 0 {
            let chunk_size = (buffer.len() as u64).min(remaining) as usize;
            let count = opened
                .file
                .read(&mut buffer[..chunk_size])
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            if count == 0 {
                break;
            }
            sink.write_all(&buffer[..count])
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            remaining -= count as u64;
        }
        if !metadata_matches(
            &opened.metadata,
            &opened
                .file
                .metadata()
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?,
        ) {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::RemoteChanged,
                true,
            ));
        }
        Ok(ObjectMeta {
            key: key.clone(),
            size: object_size,
            etag: None,
        })
    }

    fn put(
        &self,
        key: &ObjectKey,
        source: &mut dyn Read,
        len: u64,
        condition: PutCondition,
    ) -> Result<ObjectMeta, DeviceSyncError> {
        if len > MAX_ENCRYPTED_SNAPSHOT_BYTES {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ObjectTooLarge,
                false,
            ));
        }
        let mut data = Vec::with_capacity(len.min(1024 * 1024) as usize);
        source
            .take(len.saturating_add(1))
            .read_to_end(&mut data)
            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
        if data.len() as u64 != len {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::IntegrityFailed,
                false,
            ));
        }

        let existing = self.metadata_for(key)?;
        let is_if_none_match = matches!(&condition, PutCondition::IfNoneMatch);
        match &condition {
            PutCondition::Any => {}
            PutCondition::IfNoneMatch => {
                if let Some(existing) = existing {
                    let old_data = self.read_object(key)?;
                    if old_data == data {
                        return Ok(existing);
                    }
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::ImmutableObjectConflict,
                        false,
                    ));
                }
            }
            PutCondition::IfMatch(expected) => {
                let Some(existing) = existing else {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::RemoteChanged,
                        true,
                    ));
                };
                if existing.etag.as_deref() != Some(expected.as_str()) {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::RemoteChanged,
                        true,
                    ));
                }
            }
        }

        let path = self.path_for(key)?;
        self.ensure_parent(&path)?;
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() || metadata.is_dir() {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ArchiveUnsafe,
                    false,
                ));
            }
        }
        let temporary = path
            .parent()
            .ok_or_else(|| DeviceSyncError::invalid_config("object path parent"))?
            .join(format!(
                ".{}.{}.tmp",
                path.file_name().unwrap_or_default().to_string_lossy(),
                Uuid::new_v4()
            ));
        let mut raced_existing = None;
        let result: Result<(), super::super::models::DeviceSyncError> = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            set_mode(&temporary, 0o600)?;
            file.write_all(&data)
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            file.sync_all()
                .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            if is_if_none_match {
                match fs::hard_link(&temporary, &path) {
                    Ok(()) => {
                        fs::remove_file(&temporary)
                            .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        let existing = self.metadata_for(key)?.ok_or_else(|| {
                            DeviceSyncError::new(DeviceSyncErrorCode::RemoteChanged, true)
                        })?;
                        let old_data = self.read_object(key)?;
                        let _ = fs::remove_file(&temporary);
                        if old_data == data {
                            raced_existing = Some(existing);
                            return Ok(());
                        }
                        return Err(DeviceSyncError::new(
                            DeviceSyncErrorCode::ImmutableObjectConflict,
                            false,
                        ));
                    }
                    Err(error) => {
                        return Err(DeviceSyncError::apply_failed(error.to_string()));
                    }
                }
            } else {
                // Local filesystems do not provide an ETag-aware rename. Read
                // the target again immediately before replacing it so a
                // concurrent update observed after the initial check is not
                // silently overwritten.
                if let PutCondition::IfMatch(expected) = &condition {
                    let current = self.metadata_for(key)?;
                    if current.as_ref().and_then(|meta| meta.etag.as_deref())
                        != Some(expected.as_str())
                    {
                        return Err(DeviceSyncError::new(
                            DeviceSyncErrorCode::RemoteChanged,
                            true,
                        ));
                    }
                }
                fs::rename(&temporary, &path)
                    .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        if let Some(existing) = raced_existing {
            return Ok(existing);
        }
        Ok(ObjectMeta {
            key: key.clone(),
            size: data.len() as u64,
            etag: Some(sha256_hex(&data)),
        })
    }

    fn list(
        &self,
        prefix: &ObjectPrefix,
        cursor: Option<&str>,
    ) -> Result<ObjectPage, DeviceSyncError> {
        let start = cursor
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| DeviceSyncError::invalid_config("object list cursor"))?;
        let mut keys = Vec::new();
        for entry in walkdir::WalkDir::new(&self.root).follow_links(false) {
            let entry =
                entry.map_err(|_| DeviceSyncError::apply_failed("unable to list local store"))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(&self.root)
                .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::ArchiveUnsafe, false))?;
            let segments = relative
                .components()
                .map(|component| match component {
                    Component::Normal(value) => {
                        value.to_str().map(str::to_string).ok_or_else(|| {
                            DeviceSyncError::new(DeviceSyncErrorCode::ArchiveUnsafe, false)
                        })
                    }
                    _ => Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::ArchiveUnsafe,
                        false,
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?;
            if segments.starts_with(prefix.segments()) {
                keys.push(ObjectKey::from_segments(segments)?);
            }
        }
        keys.sort_by_key(|key| key.to_string());
        let page_size = 1_000usize;
        let end = (start + page_size).min(keys.len());
        let objects = keys[start.min(keys.len())..end]
            .iter()
            .filter_map(|key| self.metadata_for(key).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ObjectPage {
            objects,
            next_cursor: (end < keys.len()).then(|| end.to_string()),
        })
    }

    fn delete(&self, key: &ObjectKey, condition: DeleteCondition) -> Result<(), DeviceSyncError> {
        let existing = self.metadata_for(key)?;
        let Some(existing) = existing else {
            return Ok(());
        };
        if let DeleteCondition::IfMatch(expected) = condition {
            if existing.etag.as_deref() != Some(expected.as_str()) {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::RemoteChanged,
                    true,
                ));
            }
        }
        let path = self.path_for(key)?;
        fs::remove_file(path).map_err(|error| DeviceSyncError::apply_failed(error.to_string()))
    }

    fn create_prefix(&self, prefix: &ObjectPrefix) -> Result<(), DeviceSyncError> {
        let path = if prefix.segments().is_empty() {
            self.root.clone()
        } else {
            let key = ObjectKey::from_segments(prefix.segments().to_vec())?;
            self.path_for(&key)?
        };
        ensure_real_directory_tree(&self.root, &path)?;
        self.ensure_parent(&path)?;
        set_mode(&path, 0o700)
    }
}

fn file_etag(file: &mut File) -> Result<String, DeviceSyncError> {
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
    Ok(sha256_hex(&data))
}

fn validate_real_parent_path(root: &Path, path: &Path) -> Result<(), DeviceSyncError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::ArchiveUnsafe, false))?;
    let components = relative.components().collect::<Vec<_>>();
    let mut current = root.to_path_buf();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        let Component::Normal(name) = component else {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ArchiveUnsafe,
                false,
            ));
        };
        current.push(name);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(DeviceSyncError::apply_failed(error.to_string())),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ArchiveUnsafe,
                false,
            ));
        }
    }
    Ok(())
}

fn ensure_real_directory_tree(root: &Path, path: &Path) -> Result<(), DeviceSyncError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::ArchiveUnsafe, false))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ArchiveUnsafe,
                false,
            ));
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::ArchiveUnsafe,
                        false,
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|error| DeviceSyncError::apply_failed(error.to_string()))?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::ArchiveUnsafe,
                        false,
                    ));
                }
            }
            Err(error) => return Err(DeviceSyncError::apply_failed(error.to_string())),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn open_regular_file_nofollow(root: &Path, path: &Path) -> std::io::Result<File> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    let components = relative.components().collect::<Vec<_>>();
    if components.is_empty() {
        return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
    }
    let root_name = CString::new(root.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    let root_fd = unsafe {
        libc::open(
            root_name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if root_fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut directory = unsafe { File::from_raw_fd(root_fd) };
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
        };
        let name = CString::new(name.as_bytes())
            .map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
        let is_last = index + 1 == components.len();
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if is_last { 0 } else { libc::O_DIRECTORY };
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let opened = unsafe { File::from_raw_fd(fd) };
        if is_last {
            return Ok(opened);
        }
        directory = opened;
    }
    Err(std::io::Error::from_raw_os_error(libc::EINVAL))
}

#[cfg(not(unix))]
fn open_regular_file_nofollow(_root: &Path, path: &Path) -> std::io::Result<File> {
    File::open(path)
}

fn metadata_matches(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    if left.len() != right.len() || left.modified().ok() != right.modified().ok() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return left.dev() == right.dev() && left.ino() == right.ino();
    }
    #[cfg(not(unix))]
    {
        true
    }
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
