mod local;

pub use local::LocalFolderStore;

use std::fmt;
use std::io::{Read, Write};

use super::models::{
    valid_segment, DeviceSyncError, DeviceSyncErrorCode, MAX_OBJECT_KEY_BYTES,
    MAX_OBJECT_KEY_SEGMENTS,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectKey(Vec<String>);

impl ObjectKey {
    pub fn from_path(path: &str) -> Result<Self, DeviceSyncError> {
        let segments = path.split('/').map(str::to_string).collect::<Vec<_>>();
        Self::from_segments(segments)
    }

    pub fn from_segments(segments: Vec<String>) -> Result<Self, DeviceSyncError> {
        if segments.is_empty()
            || segments.len() > MAX_OBJECT_KEY_SEGMENTS
            || segments.iter().any(|segment| !valid_segment(segment))
            || segments.join("/").len() > MAX_OBJECT_KEY_BYTES
        {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::InvalidConfig,
                false,
            ));
        }
        Ok(Self(segments))
    }

    pub fn segments(&self) -> &[String] {
        &self.0
    }

    pub fn join(&self, child: &str) -> Result<Self, DeviceSyncError> {
        let mut segments = self.0.clone();
        segments.extend(child.split('/').map(str::to_string));
        Self::from_segments(segments)
    }
}

impl fmt::Display for ObjectKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0.join("/"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectPrefix(Vec<String>);

impl ObjectPrefix {
    pub fn root() -> Self {
        Self(Vec::new())
    }

    pub fn from_path(path: &str) -> Result<Self, DeviceSyncError> {
        if path.is_empty() {
            return Ok(Self::root());
        }
        Ok(Self(ObjectKey::from_path(path)?.0))
    }

    pub fn segments(&self) -> &[String] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
    pub key: ObjectKey,
    pub size: u64,
    pub etag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectPage {
    pub objects: Vec<ObjectMeta>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionReport {
    pub provider: String,
    pub writable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutCondition {
    Any,
    IfNoneMatch,
    IfMatch(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteCondition {
    Any,
    IfMatch(String),
}

pub trait ObjectStore: Send + Sync {
    fn capabilities(&self) -> StoreCapabilities;
    fn test_connection(&self) -> Result<ConnectionReport, DeviceSyncError>;
    fn head(&self, key: &ObjectKey) -> Result<Option<ObjectMeta>, DeviceSyncError>;
    fn get_bounded(
        &self,
        key: &ObjectKey,
        max_bytes: u64,
        sink: &mut dyn Write,
    ) -> Result<ObjectMeta, DeviceSyncError>;
    /// Read at most `max_bytes` from an object while retaining the object's
    /// complete metadata. Stores with range support should override this;
    /// the default is conservative and is suitable for small test stores.
    fn get_prefix(
        &self,
        key: &ObjectKey,
        max_bytes: u64,
        sink: &mut dyn Write,
    ) -> Result<ObjectMeta, DeviceSyncError> {
        self.get_bounded(key, max_bytes, sink)
    }
    fn put(
        &self,
        key: &ObjectKey,
        source: &mut dyn Read,
        len: u64,
        condition: PutCondition,
    ) -> Result<ObjectMeta, DeviceSyncError>;
    fn list(
        &self,
        prefix: &ObjectPrefix,
        cursor: Option<&str>,
    ) -> Result<ObjectPage, DeviceSyncError>;
    fn delete(&self, key: &ObjectKey, condition: DeleteCondition) -> Result<(), DeviceSyncError>;
    fn create_prefix(&self, prefix: &ObjectPrefix) -> Result<(), DeviceSyncError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreCapabilities {
    pub conditional_put: bool,
    pub conditional_delete: bool,
    pub list: bool,
    pub delete: bool,
}
