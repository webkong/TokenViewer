use std::collections::{BTreeMap, HashMap, HashSet};
#[cfg(unix)]
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd};

use sha2::{Digest, Sha256};
use tar::{Builder, EntryType, Header};

use super::models::{
    valid_segment, DeviceSyncError, DeviceSyncErrorCode, SkillFileKind, SkillFileRecord,
    MAX_ARCHIVE_ENTRIES, MAX_COMPRESSION_RATIO, MAX_EXPANDED_BYTES, MAX_RELATIVE_PATH_BYTES,
    MAX_SINGLE_FILE_BYTES,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveWarning {
    pub code: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveBuild {
    pub bytes: Vec<u8>,
    pub records: Vec<SkillFileRecord>,
    pub warnings: Vec<ArchiveWarning>,
    pub expanded_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveInspection {
    pub records: Vec<SkillFileRecord>,
    pub expanded_bytes: u64,
}

#[derive(Debug, Clone)]
enum EntryPlanKind {
    File,
    Directory,
    Symlink(String),
}

#[derive(Debug, Clone)]
struct EntryPlan {
    source: PathBuf,
    path: String,
    kind: EntryPlanKind,
    mode: u32,
    identity: Option<FileIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    size: u64,
    modified_ns: Option<u128>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl FileIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            size: metadata.len(),
            modified_ns: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos()),
            #[cfg(unix)]
            device: {
                use std::os::unix::fs::MetadataExt;
                metadata.dev()
            },
            #[cfg(unix)]
            inode: {
                use std::os::unix::fs::MetadataExt;
                metadata.ino()
            },
        }
    }
}

/// Phase 1 archive ADR: GNU tar through the `tar` crate is the sole wire
/// archive format. Entries are sorted, metadata is normalized, timestamps and
/// ownership are zeroed, and extraction never delegates unsafe paths to the
/// crate's convenience unpacker.
pub fn build_skill_archive(
    source_root: &Path,
    skill_ids: &[String],
) -> Result<ArchiveBuild, DeviceSyncError> {
    build_skill_archive_with_hook(source_root, skill_ids, &mut |_| {})
}

fn build_skill_archive_with_hook(
    source_root: &Path,
    skill_ids: &[String],
    before_entry: &mut dyn FnMut(&EntryPlan),
) -> Result<ArchiveBuild, DeviceSyncError> {
    let root = fs::canonicalize(source_root)
        .map_err(|_| DeviceSyncError::archive_unsafe("source root is unavailable"))?;
    let mut plans = Vec::new();
    let mut warnings = Vec::new();
    let mut seen_skill_ids = HashSet::new();
    let selected_skill_ids = skill_ids.iter().cloned().collect::<HashSet<_>>();

    for skill_id in skill_ids {
        if !valid_segment(skill_id) || !seen_skill_ids.insert(skill_id.clone()) {
            return Err(DeviceSyncError::archive_unsafe(
                "invalid or duplicate skill id",
            ));
        }
        let skill_path = root.join(skill_id);
        let metadata = fs::symlink_metadata(&skill_path).map_err(|_| {
            DeviceSyncError::archive_unsafe(format!("skill is unavailable: {}", skill_id))
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(DeviceSyncError::archive_unsafe(
                "skill root must be a real directory",
            ));
        }
        collect_directory(
            &root,
            &skill_path,
            &format!("skills/{}", skill_id),
            &selected_skill_ids,
            &mut plans,
            &mut warnings,
        )?;
    }

    plans.sort_by(|left, right| left.path.cmp(&right.path));
    validate_plan_paths(&plans)?;

    let mut bytes = Vec::new();
    let mut records = Vec::new();
    let mut expanded_bytes = 0u64;

    {
        let mut builder = Builder::new(&mut bytes);
        for plan in plans {
            before_entry(&plan);
            if records.len() as u64 >= MAX_ARCHIVE_ENTRIES {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ObjectTooLarge,
                    false,
                ));
            }
            validate_plan_source(&root, &plan)?;
            let (entry_type, content, link_target, size) = match &plan.kind {
                EntryPlanKind::File => {
                    let file = open_regular_file_nofollow(&root, &plan.source)?;
                    let opened_metadata = file.metadata().map_err(|_| {
                        DeviceSyncError::archive_unsafe("skill file metadata unavailable")
                    })?;
                    let opened_identity = FileIdentity::from_metadata(&opened_metadata);
                    if plan.identity != Some(opened_identity)
                        || opened_identity.size != plan.identity.map_or(0, |identity| identity.size)
                    {
                        return Err(DeviceSyncError::archive_unsafe(
                            "skill file changed during archive scan",
                        ));
                    }
                    if opened_identity.size > MAX_SINGLE_FILE_BYTES {
                        return Err(DeviceSyncError::new(
                            DeviceSyncErrorCode::ObjectTooLarge,
                            false,
                        ));
                    }
                    let mut data =
                        Vec::with_capacity(opened_identity.size.min(1024 * 1024) as usize);
                    (&file)
                        .take(MAX_SINGLE_FILE_BYTES + 1)
                        .read_to_end(&mut data)
                        .map_err(|_| DeviceSyncError::archive_unsafe("skill file is unreadable"))?;
                    let final_metadata = file.metadata().map_err(|_| {
                        DeviceSyncError::archive_unsafe("skill file metadata unavailable")
                    })?;
                    let final_identity = FileIdentity::from_metadata(&final_metadata);
                    if final_identity != opened_identity
                        || data.len() as u64 != opened_identity.size
                    {
                        if data.len() as u64 > MAX_SINGLE_FILE_BYTES
                            || final_identity.size > MAX_SINGLE_FILE_BYTES
                        {
                            return Err(DeviceSyncError::new(
                                DeviceSyncErrorCode::ObjectTooLarge,
                                false,
                            ));
                        }
                        return Err(DeviceSyncError::archive_unsafe(
                            "skill file changed during archive read",
                        ));
                    }
                    if data.len() as u64 > MAX_SINGLE_FILE_BYTES {
                        return Err(DeviceSyncError::new(
                            DeviceSyncErrorCode::ObjectTooLarge,
                            false,
                        ));
                    }
                    expanded_bytes =
                        expanded_bytes
                            .checked_add(data.len() as u64)
                            .ok_or_else(|| {
                                DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false)
                            })?;
                    if expanded_bytes > MAX_EXPANDED_BYTES {
                        return Err(DeviceSyncError::new(
                            DeviceSyncErrorCode::ObjectTooLarge,
                            false,
                        ));
                    }
                    (EntryType::Regular, data, None, opened_identity.size)
                }
                EntryPlanKind::Directory => {
                    let metadata = fs::symlink_metadata(&plan.source).map_err(|_| {
                        DeviceSyncError::archive_unsafe("skill directory changed during scan")
                    })?;
                    if metadata.file_type().is_symlink() || !metadata.is_dir() {
                        return Err(DeviceSyncError::archive_unsafe(
                            "skill directory changed during scan",
                        ));
                    }
                    (EntryType::Directory, Vec::new(), None, 0)
                }
                EntryPlanKind::Symlink(target) => {
                    let metadata = fs::symlink_metadata(&plan.source).map_err(|_| {
                        DeviceSyncError::archive_unsafe("skill symlink changed during scan")
                    })?;
                    if !metadata.file_type().is_symlink()
                        || fs::read_link(&plan.source)
                            .ok()
                            .and_then(|path| path.to_str().map(str::to_string))
                            .as_deref()
                            != Some(target.as_str())
                    {
                        return Err(DeviceSyncError::archive_unsafe(
                            "skill symlink changed during scan",
                        ));
                    }
                    (EntryType::Symlink, Vec::new(), Some(target.clone()), 0)
                }
            };

            let mut header = Header::new_gnu();
            header.set_entry_type(entry_type);
            header.set_size(size);
            header.set_mode(plan.mode);
            header.set_mtime(0);
            header.set_uid(0);
            header.set_gid(0);
            header.set_username("")?;
            header.set_groupname("")?;
            header.set_path(&plan.path).map_err(|_| {
                DeviceSyncError::archive_unsafe("archive path cannot be represented safely")
            })?;
            header.set_cksum();

            if let Some(target) = &link_target {
                builder
                    .append_link(&mut header, &plan.path, target)
                    .map_err(|_| DeviceSyncError::archive_unsafe("unable to write symlink"))?;
            } else {
                builder
                    .append_data(&mut header, &plan.path, Cursor::new(content.as_slice()))
                    .map_err(|_| DeviceSyncError::archive_unsafe("unable to write archive"))?;
            }

            let (kind, hash, link_target) = match (&plan.kind, content.as_slice()) {
                (EntryPlanKind::File, content) => (SkillFileKind::File, sha256_hex(content), None),
                (EntryPlanKind::Directory, _) => (SkillFileKind::Directory, String::new(), None),
                (EntryPlanKind::Symlink(target), _) => {
                    (SkillFileKind::Symlink, String::new(), Some(target.clone()))
                }
            };
            records.push(SkillFileRecord {
                path: plan.path,
                kind,
                size,
                sha256: hash,
                mode: plan.mode,
                link_target,
            });
        }

        builder
            .finish()
            .map_err(|_| DeviceSyncError::archive_unsafe("unable to finish archive"))?;
    }
    Ok(ArchiveBuild {
        bytes,
        records,
        warnings,
        expanded_bytes,
    })
}

pub fn unpack_archive(bytes: &[u8], destination: &Path) -> Result<ArchiveBuild, DeviceSyncError> {
    let inspection = inspect_archive(bytes)?;
    if bytes.len() as u64 > MAX_EXPANDED_BYTES {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ObjectTooLarge,
            false,
        ));
    }
    fs::create_dir_all(destination)
        .map_err(|_| DeviceSyncError::archive_unsafe("staging directory unavailable"))?;
    set_mode(destination, 0o700)?;

    let mut archive = tar::Archive::new(Cursor::new(bytes));
    let mut records = Vec::new();
    let mut symlinks = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut expanded_bytes = 0u64;

    for entry_result in archive
        .entries()
        .map_err(|_| DeviceSyncError::archive_unsafe("invalid tar archive"))?
    {
        let entry =
            entry_result.map_err(|_| DeviceSyncError::archive_unsafe("invalid tar entry"))?;
        if records.len() as u64 >= MAX_ARCHIVE_ENTRIES {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ObjectTooLarge,
                false,
            ));
        }
        let path = safe_archive_path(
            &entry
                .path()
                .map_err(|_| DeviceSyncError::archive_unsafe("archive path is invalid"))?,
        )?;
        let collision_key = path.to_lowercase();
        if !seen_paths.insert(collision_key) {
            return Err(DeviceSyncError::archive_unsafe(
                "duplicate or case-colliding path",
            ));
        }
        let target = destination.join(&path);
        let entry_type = entry.header().entry_type();
        let mode = normalize_mode(entry.header().mode().unwrap_or(0o644), entry_type);

        if entry_type == EntryType::Directory {
            ensure_real_parent(destination, &target)?;
            if target.exists() || target.is_symlink() {
                if !target.is_dir() || target.is_symlink() {
                    return Err(DeviceSyncError::archive_unsafe(
                        "directory target is occupied",
                    ));
                }
            } else {
                fs::create_dir(&target)
                    .map_err(|_| DeviceSyncError::archive_unsafe("unable to create directory"))?;
            }
            set_mode(&target, mode)?;
            records.push(SkillFileRecord {
                path,
                kind: SkillFileKind::Directory,
                size: 0,
                sha256: String::new(),
                mode,
                link_target: None,
            });
            continue;
        }

        if entry_type == EntryType::Regular {
            let declared_size = entry.size();
            if declared_size > MAX_SINGLE_FILE_BYTES {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ObjectTooLarge,
                    false,
                ));
            }
            expanded_bytes = expanded_bytes
                .checked_add(declared_size)
                .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false))?;
            if expanded_bytes > MAX_EXPANDED_BYTES {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ObjectTooLarge,
                    false,
                ));
            }
            ensure_real_parent(destination, &target)?;
            if target.exists() || target.is_symlink() {
                return Err(DeviceSyncError::archive_unsafe("file target is occupied"));
            }
            let mut data = Vec::with_capacity(declared_size.min(1024 * 1024) as usize);
            entry
                .take(MAX_SINGLE_FILE_BYTES + 1)
                .read_to_end(&mut data)
                .map_err(|_| DeviceSyncError::archive_unsafe("unable to read file entry"))?;
            if data.len() as u64 != declared_size {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::IntegrityFailed,
                    false,
                ));
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|_| DeviceSyncError::archive_unsafe("unable to create staged file"))?;
            file.write_all(&data)
                .map_err(|_| DeviceSyncError::archive_unsafe("unable to write staged file"))?;
            file.sync_all()
                .map_err(|_| DeviceSyncError::archive_unsafe("unable to sync staged file"))?;
            set_mode(&target, mode)?;
            records.push(SkillFileRecord {
                path,
                kind: SkillFileKind::File,
                size: declared_size,
                sha256: sha256_hex(&data),
                mode,
                link_target: None,
            });
            continue;
        }

        if entry_type == EntryType::Symlink {
            let link_target = entry
                .link_name()
                .map_err(|_| DeviceSyncError::archive_unsafe("symlink target is invalid"))?
                .ok_or_else(|| DeviceSyncError::archive_unsafe("symlink target is missing"))?;
            let link_target = link_target
                .to_str()
                .ok_or_else(|| DeviceSyncError::archive_unsafe("symlink target is not UTF-8"))?
                .to_string();
            validate_relative_link(&path, &link_target)?;
            ensure_real_parent(destination, &target)?;
            if target.exists() || target.is_symlink() {
                return Err(DeviceSyncError::archive_unsafe(
                    "symlink target path is occupied",
                ));
            }
            symlinks.push((target, path.clone(), link_target.clone(), mode));
            records.push(SkillFileRecord {
                path,
                kind: SkillFileKind::Symlink,
                size: 0,
                sha256: String::new(),
                mode,
                link_target: Some(link_target),
            });
            continue;
        }

        return Err(DeviceSyncError::archive_unsafe(
            "unsupported tar entry type",
        ));
    }

    let destination_canonical = fs::canonicalize(destination)
        .map_err(|_| DeviceSyncError::archive_unsafe("staging directory unavailable"))?;
    for (target, path, link_target, mode) in symlinks {
        #[cfg(unix)]
        std::os::unix::fs::symlink(&link_target, &target)
            .map_err(|_| DeviceSyncError::archive_unsafe("unable to create staged symlink"))?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&link_target, &target)
            .map_err(|_| DeviceSyncError::archive_unsafe("unable to create staged symlink"))?;
        let resolved = fs::canonicalize(&target)
            .map_err(|_| DeviceSyncError::archive_unsafe("staged symlink is dangling"))?;
        if !resolved.starts_with(&destination_canonical) || !resolved.is_file() {
            return Err(DeviceSyncError::archive_unsafe(
                "staged symlink leaves source root",
            ));
        }
        let _ = mode;
        let _ = path;
    }

    if bytes.is_empty() && expanded_bytes > 0 {
        return Err(DeviceSyncError::archive_unsafe("invalid empty archive"));
    }
    validate_archive_compression_ratio(bytes.len() as u64, expanded_bytes)?;

    if records != inspection.records || expanded_bytes != inspection.expanded_bytes {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    }

    Ok(ArchiveBuild {
        bytes: bytes.to_vec(),
        records,
        warnings: Vec::new(),
        expanded_bytes,
    })
}

/// Inspect a tar archive without creating files. This is used before a
/// snapshot is accepted so the manifest can be checked against the actual
/// archive entries before any staging directory is modified.
pub fn inspect_archive(bytes: &[u8]) -> Result<ArchiveInspection, DeviceSyncError> {
    if bytes.len() as u64 > MAX_EXPANDED_BYTES {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ObjectTooLarge,
            false,
        ));
    }

    let mut archive = tar::Archive::new(Cursor::new(bytes));
    let mut records = Vec::new();
    let mut symlinks = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut expanded_bytes = 0u64;

    for entry_result in archive
        .entries()
        .map_err(|_| DeviceSyncError::archive_unsafe("invalid tar archive"))?
    {
        let entry =
            entry_result.map_err(|_| DeviceSyncError::archive_unsafe("invalid tar entry"))?;
        if records.len() as u64 >= MAX_ARCHIVE_ENTRIES {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ObjectTooLarge,
                false,
            ));
        }
        let path = safe_archive_path(
            &entry
                .path()
                .map_err(|_| DeviceSyncError::archive_unsafe("archive path is invalid"))?,
        )?;
        if !seen_paths.insert(path.to_lowercase()) {
            return Err(DeviceSyncError::archive_unsafe(
                "duplicate or case-colliding path",
            ));
        }
        let entry_type = entry.header().entry_type();
        let mode = normalize_mode(entry.header().mode().unwrap_or(0o644), entry_type);

        if entry_type == EntryType::Directory {
            records.push(SkillFileRecord {
                path,
                kind: SkillFileKind::Directory,
                size: 0,
                sha256: String::new(),
                mode,
                link_target: None,
            });
            continue;
        }

        if entry_type == EntryType::Regular {
            let declared_size = entry.size();
            if declared_size > MAX_SINGLE_FILE_BYTES {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ObjectTooLarge,
                    false,
                ));
            }
            expanded_bytes = expanded_bytes
                .checked_add(declared_size)
                .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false))?;
            if expanded_bytes > MAX_EXPANDED_BYTES {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ObjectTooLarge,
                    false,
                ));
            }
            let mut data = Vec::with_capacity(declared_size.min(1024 * 1024) as usize);
            entry
                .take(MAX_SINGLE_FILE_BYTES + 1)
                .read_to_end(&mut data)
                .map_err(|_| DeviceSyncError::archive_unsafe("unable to read file entry"))?;
            if data.len() as u64 != declared_size {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::IntegrityFailed,
                    false,
                ));
            }
            records.push(SkillFileRecord {
                path,
                kind: SkillFileKind::File,
                size: declared_size,
                sha256: sha256_hex(&data),
                mode,
                link_target: None,
            });
            continue;
        }

        if entry_type == EntryType::Symlink {
            let link_target = entry
                .link_name()
                .map_err(|_| DeviceSyncError::archive_unsafe("symlink target is invalid"))?
                .ok_or_else(|| DeviceSyncError::archive_unsafe("symlink target is missing"))?;
            let link_target = link_target
                .to_str()
                .ok_or_else(|| DeviceSyncError::archive_unsafe("symlink target is not UTF-8"))?
                .to_string();
            validate_relative_link(&path, &link_target)?;
            symlinks.push((path.clone(), link_target.clone()));
            records.push(SkillFileRecord {
                path,
                kind: SkillFileKind::Symlink,
                size: 0,
                sha256: String::new(),
                mode,
                link_target: Some(link_target),
            });
            continue;
        }

        return Err(DeviceSyncError::archive_unsafe(
            "unsupported tar entry type",
        ));
    }

    if bytes.is_empty() && expanded_bytes > 0 {
        return Err(DeviceSyncError::archive_unsafe("invalid empty archive"));
    }
    validate_archive_compression_ratio(bytes.len() as u64, expanded_bytes)?;

    validate_archive_layout(&records)?;
    validate_archive_symlinks(&records, &symlinks)?;
    Ok(ArchiveInspection {
        records,
        expanded_bytes,
    })
}

fn validate_archive_layout(records: &[SkillFileRecord]) -> Result<(), DeviceSyncError> {
    let kinds = records
        .iter()
        .map(|record| (record.path.as_str(), &record.kind))
        .collect::<HashMap<_, _>>();
    for record in records {
        let parts = record.path.split('/').collect::<Vec<_>>();
        for index in 1..parts.len() {
            let parent = parts[..index].join("/");
            if kinds
                .get(parent.as_str())
                .is_some_and(|kind| !matches!(kind, SkillFileKind::Directory))
            {
                return Err(DeviceSyncError::archive_unsafe(
                    "archive parent is not a directory",
                ));
            }
        }
    }
    Ok(())
}

fn validate_archive_symlinks(
    records: &[SkillFileRecord],
    symlinks: &[(String, String)],
) -> Result<(), DeviceSyncError> {
    let by_path = records
        .iter()
        .map(|record| (record.path.as_str(), record))
        .collect::<BTreeMap<_, _>>();
    for (path, _) in symlinks {
        let mut current = path.clone();
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(current.clone()) {
                return Err(DeviceSyncError::archive_unsafe("symlink cycle"));
            }
            let record = by_path
                .get(current.as_str())
                .ok_or_else(|| DeviceSyncError::archive_unsafe("symlink target is missing"))?;
            match record.kind {
                SkillFileKind::File => break,
                SkillFileKind::Directory => {
                    return Err(DeviceSyncError::archive_unsafe(
                        "symlink target is not a file",
                    ))
                }
                SkillFileKind::Symlink => {
                    let target = record.link_target.as_deref().ok_or_else(|| {
                        DeviceSyncError::archive_unsafe("symlink target is missing")
                    })?;
                    current = normalize_link_target(&record.path, target)?;
                }
            }
        }
    }
    Ok(())
}

fn collect_directory(
    root: &Path,
    source: &Path,
    archive_path: &str,
    selected_skill_ids: &HashSet<String>,
    plans: &mut Vec<EntryPlan>,
    warnings: &mut Vec<ArchiveWarning>,
) -> Result<(), DeviceSyncError> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|_| DeviceSyncError::archive_unsafe("unable to inspect skill entry"))?;
    if metadata.file_type().is_symlink() {
        return Err(DeviceSyncError::archive_unsafe(
            "skill root cannot be a symlink",
        ));
    }
    plans.push(EntryPlan {
        source: source.to_path_buf(),
        path: archive_path.to_string(),
        kind: EntryPlanKind::Directory,
        mode: normalize_mode(metadata_mode(&metadata), EntryType::Directory),
        identity: None,
    });

    let mut children = fs::read_dir(source)
        .map_err(|_| DeviceSyncError::archive_unsafe("unable to read skill directory"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| DeviceSyncError::archive_unsafe("unable to read skill directory"))?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let file_name_os = child.file_name();
        let file_name = file_name_os
            .to_str()
            .ok_or_else(|| DeviceSyncError::archive_unsafe("skill path is not UTF-8"))?;
        if is_denied_component(file_name) {
            continue;
        }
        let child_archive_path = format!("{}/{}", archive_path, file_name);
        validate_relative_path(&child_archive_path)?;
        let child_path = child.path();
        let child_metadata = fs::symlink_metadata(&child_path)
            .map_err(|_| DeviceSyncError::archive_unsafe("unable to inspect skill entry"))?;
        let file_type = child_metadata.file_type();
        if file_type.is_symlink() {
            let target = fs::read_link(&child_path)
                .map_err(|_| DeviceSyncError::archive_unsafe("unable to read symlink"))?;
            let target_text = target
                .to_str()
                .ok_or_else(|| DeviceSyncError::archive_unsafe("symlink target is not UTF-8"))?;
            if target.is_absolute() {
                warnings.push(ArchiveWarning {
                    code: "absolute_symlink".to_string(),
                    path: child_archive_path,
                });
                continue;
            }
            let resolved =
                match fs::canonicalize(child_path.parent().unwrap_or(source).join(&target)) {
                    Ok(path) => path,
                    Err(_) => {
                        warnings.push(ArchiveWarning {
                            code: "dangling_symlink".to_string(),
                            path: child_archive_path,
                        });
                        continue;
                    }
                };
            if !resolved.starts_with(root) {
                warnings.push(ArchiveWarning {
                    code: "external_symlink".to_string(),
                    path: child_archive_path,
                });
                continue;
            }
            let dependency_skill_id = resolved
                .strip_prefix(root)
                .ok()
                .and_then(|relative| relative.components().next())
                .and_then(|component| component.as_os_str().to_str());
            if dependency_skill_id.is_none_or(|skill_id| !selected_skill_ids.contains(skill_id)) {
                warnings.push(ArchiveWarning {
                    code: "unselected_skill_dependency".to_string(),
                    path: child_archive_path,
                });
                continue;
            }
            if !resolved.is_file() {
                warnings.push(ArchiveWarning {
                    code: "external_symlink".to_string(),
                    path: child_archive_path,
                });
                continue;
            }
            plans.push(EntryPlan {
                source: child_path,
                path: child_archive_path,
                kind: EntryPlanKind::Symlink(target_text.to_string()),
                mode: 0o777,
                identity: None,
            });
        } else if file_type.is_dir() {
            collect_directory(
                root,
                &child_path,
                &child_archive_path,
                selected_skill_ids,
                plans,
                warnings,
            )?;
        } else if file_type.is_file() {
            if child_metadata.len() > MAX_SINGLE_FILE_BYTES {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ObjectTooLarge,
                    false,
                ));
            }
            plans.push(EntryPlan {
                source: child_path,
                path: child_archive_path,
                kind: EntryPlanKind::File,
                mode: normalize_mode(metadata_mode(&child_metadata), EntryType::Regular),
                identity: Some(FileIdentity::from_metadata(&child_metadata)),
            });
        } else {
            warnings.push(ArchiveWarning {
                code: "unsupported_file_type".to_string(),
                path: child_archive_path,
            });
        }
        if plans.len() as u64 > MAX_ARCHIVE_ENTRIES {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ObjectTooLarge,
                false,
            ));
        }
    }
    Ok(())
}

fn validate_plan_source(root: &Path, plan: &EntryPlan) -> Result<(), DeviceSyncError> {
    validate_real_parent_path(root, &plan.source)?;
    let canonical = fs::canonicalize(&plan.source)
        .map_err(|_| DeviceSyncError::archive_unsafe("skill entry changed during scan"))?;
    if !canonical.starts_with(root) {
        return Err(DeviceSyncError::archive_unsafe(
            "skill entry escaped source root",
        ));
    }
    Ok(())
}

fn open_regular_file_nofollow(root: &Path, path: &Path) -> Result<File, DeviceSyncError> {
    #[cfg(unix)]
    {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| DeviceSyncError::archive_unsafe("skill entry escaped source root"))?;
        let components = relative.components().collect::<Vec<_>>();
        if components.is_empty() {
            return Err(DeviceSyncError::archive_unsafe("skill file path is empty"));
        }

        let root_name = CString::new(root.as_os_str().as_bytes())
            .map_err(|_| DeviceSyncError::archive_unsafe("skill root is invalid"))?;
        let root_fd = unsafe {
            libc::open(
                root_name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if root_fd < 0 {
            return Err(DeviceSyncError::archive_unsafe("skill root is unavailable"));
        }
        let mut directory = unsafe { File::from_raw_fd(root_fd) };
        for (index, component) in components.iter().enumerate() {
            let Component::Normal(name) = component else {
                return Err(DeviceSyncError::archive_unsafe("invalid skill path"));
            };
            let name = CString::new(name.as_bytes())
                .map_err(|_| DeviceSyncError::archive_unsafe("skill path is invalid"))?;
            let is_last = index + 1 == components.len();
            let flags = libc::O_RDONLY
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW
                | if is_last { 0 } else { libc::O_DIRECTORY };
            let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err(DeviceSyncError::archive_unsafe("skill file is unreadable"));
            }
            let opened = unsafe { File::from_raw_fd(fd) };
            if is_last {
                return Ok(opened);
            }
            directory = opened;
        }
        Err(DeviceSyncError::archive_unsafe("skill file is unreadable"))
    }
    #[cfg(not(unix))]
    {
        let _ = root;
        File::open(path).map_err(|_| DeviceSyncError::archive_unsafe("skill file is unreadable"))
    }
}

fn validate_real_parent_path(root: &Path, path: &Path) -> Result<(), DeviceSyncError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| DeviceSyncError::archive_unsafe("skill entry escaped source root"))?;
    let mut current = root.to_path_buf();
    let components = relative.components().collect::<Vec<_>>();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        let Component::Normal(name) = component else {
            return Err(DeviceSyncError::archive_unsafe("invalid skill parent path"));
        };
        current.push(name);
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| DeviceSyncError::archive_unsafe("skill parent changed during scan"))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(DeviceSyncError::archive_unsafe(
                "skill parent escaped source root",
            ));
        }
    }
    Ok(())
}

fn validate_plan_paths(plans: &[EntryPlan]) -> Result<(), DeviceSyncError> {
    let mut seen = HashSet::new();
    for plan in plans {
        validate_relative_path(&plan.path)?;
        if !seen.insert(plan.path.to_lowercase()) {
            return Err(DeviceSyncError::archive_unsafe(
                "duplicate or case-colliding path",
            ));
        }
    }
    Ok(())
}

fn safe_archive_path(path: &Path) -> Result<String, DeviceSyncError> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => components.push(
                value
                    .to_str()
                    .ok_or_else(|| DeviceSyncError::archive_unsafe("archive path is not UTF-8"))?,
            ),
            _ => return Err(DeviceSyncError::archive_unsafe("archive traversal path")),
        }
    }
    let path = components.join("/");
    validate_relative_path(&path)?;
    if components.first().copied() != Some("skills") {
        return Err(DeviceSyncError::archive_unsafe(
            "archive entry outside skills",
        ));
    }
    if components.get(1).is_none_or(|skill| !valid_segment(skill)) {
        return Err(DeviceSyncError::archive_unsafe(
            "archive skill id is invalid",
        ));
    }
    Ok(path)
}

fn validate_relative_path(path: &str) -> Result<(), DeviceSyncError> {
    if path.is_empty()
        || path.len() > MAX_RELATIVE_PATH_BYTES
        || path.contains('\0')
        || path.contains('\\')
        || path.chars().any(char::is_control)
    {
        return Err(DeviceSyncError::archive_unsafe(
            "archive path length or NUL",
        ));
    }
    let parsed = Path::new(path);
    if parsed.is_absolute() {
        return Err(DeviceSyncError::archive_unsafe("absolute archive path"));
    }
    for component in parsed.components() {
        match component {
            Component::Normal(value) => {
                if value.to_str().is_none()
                    || value == "."
                    || value == ".."
                    || value.to_string_lossy().chars().any(char::is_control)
                {
                    return Err(DeviceSyncError::archive_unsafe("invalid archive component"));
                }
            }
            _ => return Err(DeviceSyncError::archive_unsafe("archive traversal path")),
        }
    }
    Ok(())
}

fn validate_relative_link(path: &str, link_target: &str) -> Result<(), DeviceSyncError> {
    normalize_link_target(path, link_target).map(|_| ())
}

fn validate_archive_compression_ratio(
    compressed_bytes: u64,
    expanded_bytes: u64,
) -> Result<(), DeviceSyncError> {
    if compressed_bytes == 0 {
        if expanded_bytes == 0 {
            return Ok(());
        }
        return Err(DeviceSyncError::archive_unsafe(
            "archive compression ratio exceeded",
        ));
    }
    let maximum_expanded = compressed_bytes
        .checked_mul(MAX_COMPRESSION_RATIO)
        .ok_or_else(|| DeviceSyncError::archive_unsafe("archive compression ratio overflow"))?;
    if expanded_bytes > maximum_expanded {
        return Err(DeviceSyncError::archive_unsafe(
            "archive compression ratio exceeded",
        ));
    }
    Ok(())
}

fn normalize_link_target(path: &str, link_target: &str) -> Result<String, DeviceSyncError> {
    if link_target.is_empty()
        || link_target.len() > MAX_RELATIVE_PATH_BYTES
        || link_target.contains('\0')
        || link_target.contains('\\')
        || link_target.chars().any(char::is_control)
    {
        return Err(DeviceSyncError::archive_unsafe("invalid symlink target"));
    }
    let target = Path::new(link_target);
    if target.is_absolute() {
        return Err(DeviceSyncError::archive_unsafe("absolute symlink target"));
    }
    let mut stack = path.split('/').map(str::to_string).collect::<Vec<_>>();
    stack.pop();
    if stack.len() < 2 {
        return Err(DeviceSyncError::archive_unsafe(
            "symlink source is outside skills",
        ));
    }
    for component in link_target.split('/') {
        match component {
            "" => return Err(DeviceSyncError::archive_unsafe("invalid symlink target")),
            "." => {}
            ".." if stack.len() > 2 => {
                stack.pop();
            }
            ".." => {
                return Err(DeviceSyncError::archive_unsafe(
                    "symlink leaves archive root",
                ))
            }
            value => stack.push(value.to_string()),
        }
    }
    let normalized = stack.join("/");
    if normalized.split('/').next() != Some("skills")
        || normalized
            .split('/')
            .nth(1)
            .is_none_or(|skill| !valid_segment(skill))
    {
        return Err(DeviceSyncError::archive_unsafe(
            "symlink leaves archive root",
        ));
    }
    Ok(normalized)
}

fn ensure_real_parent(root: &Path, path: &Path) -> Result<(), DeviceSyncError> {
    let parent = path
        .parent()
        .ok_or_else(|| DeviceSyncError::archive_unsafe("archive parent missing"))?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| DeviceSyncError::archive_unsafe("archive parent escaped root"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(DeviceSyncError::archive_unsafe("invalid archive parent"));
        };
        current.push(name);
        if current.exists() || current.is_symlink() {
            if !current.is_dir() || current.is_symlink() {
                return Err(DeviceSyncError::archive_unsafe(
                    "archive parent is a symlink",
                ));
            }
        } else {
            fs::create_dir(&current)
                .map_err(|_| DeviceSyncError::archive_unsafe("unable to create archive parent"))?;
            set_mode(&current, 0o700)?;
        }
    }
    Ok(())
}

fn is_denied_component(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".git"
        || lower == ".ds_store"
        || lower == "thumbs.db"
        || lower == "sessions"
        || lower == "session"
        || lower == "logs"
        || lower == "log"
        || lower == "cache"
        || lower == "download-cache"
        || lower == "downloads"
        || lower.starts_with("data.db")
        || lower.ends_with(".swp")
        || lower.ends_with(".swo")
        || lower.ends_with('~')
        || lower.ends_with(".crash")
        || lower.ends_with(".ips")
        || lower.ends_with(".profraw")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{:02x}", byte)).collect()
}

fn metadata_mode(metadata: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return metadata.mode();
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0o644
    }
}

fn normalize_mode(mode: u32, entry_type: EntryType) -> u32 {
    if entry_type == EntryType::Directory {
        0o755
    } else if entry_type == EntryType::Symlink {
        0o777
    } else if mode & 0o111 != 0 {
        0o755
    } else {
        0o644
    }
}

fn set_mode(path: &Path, mode: u32) -> Result<(), DeviceSyncError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|_| DeviceSyncError::archive_unsafe("unable to set staged permissions"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn source_with_file() -> (TempDir, PathBuf) {
        let directory = TempDir::new().unwrap();
        let file = directory.path().join("alpha/payload");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"original").unwrap();
        (directory, file)
    }

    #[test]
    fn archive_rejects_a_file_replaced_by_an_external_symlink_after_scan() {
        let (directory, file) = source_with_file();
        let mut swapped = false;
        let error =
            build_skill_archive_with_hook(directory.path(), &["alpha".to_string()], &mut |plan| {
                if !swapped && plan.path.ends_with("/payload") {
                    swapped = true;
                    fs::remove_file(&file).unwrap();
                    #[cfg(unix)]
                    std::os::unix::fs::symlink("/etc/passwd", &file).unwrap();
                }
            })
            .unwrap_err();

        #[cfg(unix)]
        assert_eq!(error.code, DeviceSyncErrorCode::ArchiveUnsafe);
    }

    #[test]
    fn archive_rejects_file_growth_after_scan() {
        let (directory, file) = source_with_file();
        let mut swapped = false;
        let error =
            build_skill_archive_with_hook(directory.path(), &["alpha".to_string()], &mut |plan| {
                if !swapped && plan.path.ends_with("/payload") {
                    swapped = true;
                    let mut output = OpenOptions::new().append(true).open(&file).unwrap();
                    output.write_all(b" grew").unwrap();
                }
            })
            .unwrap_err();

        assert_eq!(error.code, DeviceSyncErrorCode::ArchiveUnsafe);
    }

    #[test]
    fn archive_rejects_file_type_change_after_scan() {
        let (directory, file) = source_with_file();
        let mut swapped = false;
        let error =
            build_skill_archive_with_hook(directory.path(), &["alpha".to_string()], &mut |plan| {
                if !swapped && plan.path.ends_with("/payload") {
                    swapped = true;
                    fs::remove_file(&file).unwrap();
                    fs::create_dir(&file).unwrap();
                }
            })
            .unwrap_err();

        assert_eq!(error.code, DeviceSyncErrorCode::ArchiveUnsafe);
    }
}
