use std::collections::HashSet;
use std::fmt;
use std::io::{self, Read, Write};
use std::time::Duration;

use base64::Engine as _;
use quick_xml::events::Event;
use quick_xml::escape::unescape;
use quick_xml::Reader;
use sha2::{Digest, Sha256};
use ureq::{http, Agent, AsSendBody, SendBody};
use url::Url;
use zeroize::Zeroizing;

use super::{
    ConnectionReport, DeleteCondition, ObjectKey, ObjectMeta, ObjectPage, ObjectPrefix,
    ObjectStore, PutCondition, StoreCapabilities,
};
use crate::device_sync::models::{
    valid_remote_prefix, valid_segment, DeviceSyncError, DeviceSyncErrorCode,
    MAX_ENCRYPTED_SNAPSHOT_BYTES, MAX_REMOTE_LIST_OBJECTS,
};

const MAX_PROPFIND_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ETAG_BYTES: usize = 8 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const PROPFIND_BODY: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:getcontentlength/><d:getetag/><d:resourcetype/></d:prop></d:propfind>"#;

/// Credentials retained by a WebDAV store for the duration of the process.
///
/// The password deliberately has a redacted `Debug` implementation. It is not
/// serializable and is never part of `ProviderConfig` or a sync snapshot.
#[derive(Clone, PartialEq, Eq)]
pub struct WebDavCredentials {
    username: String,
    password: Zeroizing<String>,
}

impl WebDavCredentials {
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Result<Self, DeviceSyncError> {
        let username = username.into();
        let password = password.into();
        if username.is_empty()
            || username.contains(':')
            || username.chars().any(char::is_control)
            || password.is_empty()
            || password.chars().any(char::is_control)
        {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::CredentialMissing,
                false,
            ));
        }
        Ok(Self {
            username,
            password: Zeroizing::new(password),
        })
    }

    pub fn username(&self) -> &str {
        &self.username
    }
}

impl fmt::Debug for WebDavCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebDavCredentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// Synchronous WebDAV object store used by Device Sync.
pub struct WebDavStore {
    endpoint: Url,
    endpoint_segments: Vec<String>,
    remote_prefix: ObjectPrefix,
    credentials: WebDavCredentials,
    agent: Agent,
}

impl fmt::Debug for WebDavStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebDavStore")
            .field("endpoint", &self.endpoint)
            .field("remote_prefix", &self.remote_prefix.segments())
            .field("credentials", &self.credentials)
            .finish()
    }
}

impl WebDavStore {
    /// Creates a production WebDAV store. Only HTTPS endpoints are accepted.
    pub fn new(
        endpoint: &str,
        remote_prefix: &str,
        credentials: WebDavCredentials,
    ) -> Result<Self, DeviceSyncError> {
        Self::build(endpoint, remote_prefix, credentials, false, false)
    }

    /// Creates a store that explicitly permits an unencrypted HTTP endpoint.
    pub fn new_allowing_http(
        endpoint: &str,
        remote_prefix: &str,
        credentials: WebDavCredentials,
    ) -> Result<Self, DeviceSyncError> {
        Self::build(endpoint, remote_prefix, credentials, true, false)
    }

    /// Creates a store for an explicit local HTTP mock endpoint.
    ///
    /// This constructor is intentionally separate from [`Self::new`] so a
    /// production provider cannot accidentally disable TLS validation.
    pub fn new_for_test(
        endpoint: &str,
        remote_prefix: &str,
        credentials: WebDavCredentials,
    ) -> Result<Self, DeviceSyncError> {
        Self::build(endpoint, remote_prefix, credentials, true, true)
    }

    fn build(
        endpoint: &str,
        remote_prefix: &str,
        credentials: WebDavCredentials,
        allow_http: bool,
        disable_proxy: bool,
    ) -> Result<Self, DeviceSyncError> {
        let mut endpoint = parse_endpoint(endpoint, allow_http)?;
        let endpoint_segments = decode_path_segments(endpoint.path())?;
        if !endpoint.path().ends_with('/') {
            let mut path = endpoint.path().to_string();
            if path.is_empty() {
                path.push('/');
            } else {
                path.push('/');
            }
            endpoint.set_path(&path);
        }

        if !valid_remote_prefix(remote_prefix) {
            return Err(DeviceSyncError::invalid_config("provider.remote_prefix"));
        }
        let remote_prefix = ObjectPrefix::from_path(remote_prefix)?;
        let mut agent_builder = Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .timeout_per_call(Some(REQUEST_TIMEOUT))
            .https_only(!allow_http)
            .max_redirects(0)
            .max_redirects_will_error(true)
            .allow_non_standard_methods(true);
        if disable_proxy {
            agent_builder = agent_builder.proxy(None);
        }
        let agent = agent_builder.build().new_agent();

        Ok(Self {
            endpoint,
            endpoint_segments,
            remote_prefix,
            credentials,
            agent,
        })
    }

    fn validate_key(&self, key: &ObjectKey) -> Result<(), DeviceSyncError> {
        if key
            .segments()
            .starts_with(self.remote_prefix.segments())
            && key.segments().len() > self.remote_prefix.segments().len()
        {
            Ok(())
        } else {
            Err(DeviceSyncError::invalid_config("object key outside remote prefix"))
        }
    }

    fn validate_prefix(&self, prefix: &ObjectPrefix) -> Result<(), DeviceSyncError> {
        if prefix
            .segments()
            .starts_with(self.remote_prefix.segments())
        {
            Ok(())
        } else {
            Err(DeviceSyncError::invalid_config("object prefix outside remote prefix"))
        }
    }

    fn url_for_segments(
        &self,
        segments: &[String],
        trailing_slash: bool,
    ) -> Result<Url, DeviceSyncError> {
        if !segments.starts_with(self.remote_prefix.segments()) {
            return Err(DeviceSyncError::invalid_config("object path outside remote prefix"));
        }
        let mut path = self.endpoint.path().to_string();
        if !path.ends_with('/') {
            path.push('/');
        }
        for (index, segment) in segments.iter().enumerate() {
            if !valid_segment(segment) {
                return Err(DeviceSyncError::invalid_config("object path segment"));
            }
            path.push_str(&encode_path_segment(segment));
            if index + 1 < segments.len() || trailing_slash {
                path.push('/');
            }
        }
        let mut url = self.endpoint.clone();
        url.set_path(&path);
        Ok(url)
    }

    fn url_for_key(&self, key: &ObjectKey) -> Result<Url, DeviceSyncError> {
        self.validate_key(key)?;
        self.url_for_segments(key.segments(), false)
    }

    fn url_for_prefix(&self, prefix: &ObjectPrefix) -> Result<Url, DeviceSyncError> {
        self.validate_prefix(prefix)?;
        self.url_for_segments(prefix.segments(), true)
    }

    /// Builds a collection URL on the configured remote path. Creating a
    /// multi-segment remote prefix necessarily walks its own ancestors (for
    /// example `team/tokenviewer` needs `team` first), while still forbidding
    /// creation beside or outside that configured prefix.
    fn url_for_collection_segments(&self, segments: &[String]) -> Result<Url, DeviceSyncError> {
        if !(self.remote_prefix.segments().starts_with(segments)
            || segments.starts_with(self.remote_prefix.segments()))
        {
            return Err(DeviceSyncError::invalid_config(
                "object path outside remote prefix",
            ));
        }
        let mut path = self.endpoint.path().to_string();
        if !path.ends_with('/') {
            path.push('/');
        }
        for segment in segments {
            if !valid_segment(segment) {
                return Err(DeviceSyncError::invalid_config("object path segment"));
            }
            path.push_str(&encode_path_segment(segment));
            path.push('/');
        }
        let mut url = self.endpoint.clone();
        url.set_path(&path);
        Ok(url)
    }

    fn authorization_header(&self) -> String {
        let value = format!("{}:{}", self.credentials.username, self.credentials.password.as_str());
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(value.as_bytes())
        )
    }

    fn common_headers(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Authorization", self.authorization_header()),
            ("Accept-Encoding", "identity".to_string()),
        ]
    }

    fn execute<S: AsSendBody>(
        &self,
        method: &str,
        url: &Url,
        headers: &[(&str, String)],
        body: S,
    ) -> Result<http::Response<ureq::Body>, DeviceSyncError> {
        let mut builder = http::Request::builder().method(method).uri(url.as_str());
        for (name, value) in headers {
            builder = builder.header(*name, value.as_str());
        }
        let request = builder
            .body(body)
            .map_err(|_| DeviceSyncError::invalid_config("webdav request"))?;
        let request = self
            .agent
            .configure_request(request)
            .http_status_as_error(false)
            .build();
        self.agent
            .run(request)
            .map_err(|error| map_ureq_error(&error, "request"))
    }

    fn propfind(&self, url: &Url, depth: &'static str) -> Result<Vec<DavEntry>, DeviceSyncError> {
        let mut headers = self.common_headers();
        headers.push(("Depth", depth.to_string()));
        headers.push(("Content-Type", "application/xml; charset=utf-8".to_string()));
        headers.push(("Content-Length", PROPFIND_BODY.len().to_string()));
        let response = self.execute(
            "PROPFIND",
            url,
            &headers,
            SendBody::from_reader(&mut &PROPFIND_BODY[..]),
        )?;
        let status = response.status().as_u16();
        if !is_success(status) {
            return Err(status_error("propfind", status));
        }
        let mut xml = Vec::new();
        read_response_to_sink(response, MAX_PROPFIND_BYTES, &mut xml, "propfind")?;
        parse_propfind_xml(&xml)
    }

    /// Recursively lists every file under `url` using only Depth:1 PROPFIND
    /// requests. Many real WebDAV servers (e.g. Nextcloud) reject
    /// `Depth: infinity` with 403, so the store walks one collection level at
    /// a time instead — Depth:1 is a mandatory part of the WebDAV spec.
    fn propfind_recursive(&self, url: &Url) -> Result<Vec<DavEntry>, DeviceSyncError> {
        let mut files = Vec::new();
        let mut descended: HashSet<Vec<String>> = HashSet::new();
        self.collect_files(url, &mut files, &mut descended)?;
        Ok(files)
    }

    fn collect_files(
        &self,
        url: &Url,
        files: &mut Vec<DavEntry>,
        descended: &mut HashSet<Vec<String>>,
    ) -> Result<(), DeviceSyncError> {
        let target_path = decode_path_segments(url.path())?;
        let entries = self.propfind(url, "1")?;
        let mut subdirs = Vec::new();
        for entry in entries {
            if !entry.collection {
                files.push(entry);
                continue;
            }
            let entry_url = href_to_url(&self.endpoint, &entry.href)?;
            let entry_path = decode_path_segments(entry_url.path())?;
            // Never recurse into the target collection itself, and descend each
            // child collection exactly once (decoded path guards against hrefs
            // that only differ by a trailing slash).
            if entry_path != target_path && descended.insert(entry_path.clone()) {
                subdirs.push(entry_url);
            }
        }
        for subdir in subdirs {
            self.collect_files(&subdir, files, descended)?;
        }
        Ok(())
    }

    fn head_raw(&self, key: &ObjectKey) -> Result<Option<ObjectMeta>, DeviceSyncError> {
        let url = self.url_for_key(key)?;
        let headers = self.common_headers();
        let response = self.execute("HEAD", &url, &headers, SendBody::none())?;
        let status = response.status().as_u16();
        match status {
            404 => Ok(None),
            405 | 501 => {
                drop(response);
                self.head_from_propfind(key)
            }
            status if is_success(status) => {
                let size = response_content_length(&response)?;
                let Some(size) = size else {
                    drop(response);
                    return self.head_from_propfind(key);
                };
                let etag = response_header(&response, "etag")?;
                Ok(Some(ObjectMeta {
                    key: key.clone(),
                    size,
                    etag,
                }))
            }
            status => Err(status_error("head", status)),
        }
    }

    fn head_from_propfind(&self, key: &ObjectKey) -> Result<Option<ObjectMeta>, DeviceSyncError> {
        let url = self.url_for_key(key)?;
        let entries = match self.propfind(&url, "0") {
            Ok(entries) => entries,
            Err(error) if error.code == DeviceSyncErrorCode::VaultNotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut found = None;
        for entry in entries {
            let Some(entry_key) = self.key_from_href(&entry.href)? else {
                continue;
            };
            if entry_key == *key && !entry.collection {
                if found.is_some() {
                    return Err(integrity_error("duplicate WebDAV response"));
                }
                let size = entry
                    .size
                    .ok_or_else(|| protocol_error("missing WebDAV content length"))?;
                found = Some(ObjectMeta {
                    key: key.clone(),
                    size,
                    etag: entry.etag,
                });
            }
        }
        Ok(found)
    }

    fn ensure_strong_meta(&self, meta: ObjectMeta) -> Result<ObjectMeta, DeviceSyncError> {
        if is_strong_etag(meta.etag.as_deref()) {
            return Ok(meta);
        }
        let mut sink = io::sink();
        let (full, _) = self.get_full_meta(&meta.key, &mut sink)?;
        if full.size != meta.size {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::RemoteChanged,
                true,
            ));
        }
        Ok(full)
    }

    fn get_full_meta(
        &self,
        key: &ObjectKey,
        sink: &mut dyn Write,
    ) -> Result<(ObjectMeta, [u8; 32]), DeviceSyncError> {
        let url = self.url_for_key(key)?;
        let headers = self.common_headers();
        let response = self.execute("GET", &url, &headers, SendBody::none())?;
        let status = response.status().as_u16();
        if !is_success(status) {
            return Err(status_error("get", status));
        }
        let data = read_response_to_sink(
            response,
            MAX_ENCRYPTED_SNAPSHOT_BYTES,
            sink,
            "get",
        )?;
        let etag = if is_strong_etag(data.etag.as_deref()) {
            data.etag
        } else {
            Some(format!("sha256:{}", digest_hex(&data.digest)))
        };
        Ok((
            ObjectMeta {
                key: key.clone(),
                size: data.size,
                etag,
            },
            data.digest,
        ))
    }

    fn put_once(
        &self,
        key: &ObjectKey,
        source: &mut dyn Read,
        len: u64,
        condition: Option<(&str, &str)>,
    ) -> Result<PutOutcome, DeviceSyncError> {
        let url = self.url_for_key(key)?;
        let mut headers = self.common_headers();
        headers.push(("Content-Length", len.to_string()));
        if let Some(condition) = condition {
            headers.push((condition.0, condition.1.to_string()));
        }
        let mut bounded_reader = BoundedReader::new(source, len);
        let response = self.execute(
            "PUT",
            &url,
            &headers,
            SendBody::from_reader(&mut bounded_reader),
        )?;
        let status = response.status().as_u16();
        if !is_success(status) {
            return Err(status_error("put", status));
        }
        if bounded_reader.remaining() != 0 {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::IntegrityFailed,
                false,
            ));
        }
        let etag = response_header(&response, "etag")?;
        drop(response);
        let meta = if is_strong_etag(etag.as_deref()) {
            ObjectMeta {
                key: key.clone(),
                size: len,
                etag,
            }
        } else {
            self.head(key)?.ok_or_else(|| {
                DeviceSyncError::new(DeviceSyncErrorCode::RemoteChanged, true)
            })?
        };
        Ok(PutOutcome { meta })
    }

    fn fallback_put(
        &self,
        _key: &ObjectKey,
        _source: &mut dyn Read,
        _len: u64,
        expected: &str,
    ) -> Result<ObjectMeta, DeviceSyncError> {
        Err(conditional_write_unsupported_error(expected))
    }

    fn fallback_delete(&self, _key: &ObjectKey, expected: &str) -> Result<(), DeviceSyncError> {
        Err(conditional_write_unsupported_error(expected))
    }

    fn key_from_href(&self, href: &str) -> Result<Option<ObjectKey>, DeviceSyncError> {
        let url = href_to_url(&self.endpoint, href)?;
        let path_segments = decode_path_segments(url.path())?;
        if path_segments.len() < self.endpoint_segments.len()
            || path_segments[..self.endpoint_segments.len()] != self.endpoint_segments[..]
        {
            return Err(integrity_error("WebDAV response path outside endpoint"));
        }
        let relative = &path_segments[self.endpoint_segments.len()..];
        if relative.is_empty() || relative == self.remote_prefix.segments() {
            return Ok(None);
        }
        let key = ObjectKey::from_segments(relative.to_vec())?;
        self.validate_key(&key)?;
        Ok(Some(key))
    }
}

/// Validate a production WebDAV endpoint before credentials are available.
/// This is shared by persisted provider configuration and the request layer.
pub fn validate_webdav_endpoint(
    endpoint: &str,
    allow_http: bool,
) -> Result<(), DeviceSyncError> {
    parse_endpoint(endpoint, allow_http).map(|_| ())
}

fn parse_endpoint(endpoint: &str, allow_http: bool) -> Result<Url, DeviceSyncError> {
    if endpoint.chars().any(char::is_control) {
        return Err(DeviceSyncError::invalid_config("provider.endpoint"));
    }
    let endpoint = Url::parse(endpoint)
        .map_err(|_| DeviceSyncError::invalid_config("provider.endpoint"))?;
    let scheme = endpoint.scheme();
    if (scheme != "https" && !(allow_http && scheme == "http"))
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(DeviceSyncError::invalid_config("provider.endpoint"));
    }
    decode_path_segments(endpoint.path())?;
    Ok(endpoint)
}

impl ObjectStore for WebDavStore {
    fn capabilities(&self) -> StoreCapabilities {
        StoreCapabilities {
            conditional_put: true,
            conditional_delete: true,
            list: true,
            delete: true,
        }
    }

    fn test_connection(&self) -> Result<ConnectionReport, DeviceSyncError> {
        self.create_prefix(&self.remote_prefix)?;
        let mut segments = self.remote_prefix.segments().to_vec();
        segments.push(format!("connection-probe-{}", uuid::Uuid::new_v4()));
        let key = ObjectKey::from_segments(segments)?;
        let body = b"tokenviewer-webdav-probe";
        let mut source = &body[..];
        let created = self.put(&key, &mut source, body.len() as u64, PutCondition::IfNoneMatch)?;
        let result = (|| {
            let mut downloaded = Vec::with_capacity(body.len());
            self.get_bounded(&key, body.len() as u64, &mut downloaded)?;
            if downloaded != body {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::IntegrityFailed,
                    false,
                ));
            }
            let etag = created.etag.ok_or_else(|| conditional_write_unsupported_error("missing"))?;
            if !is_strong_etag(Some(&etag)) {
                return Err(conditional_write_unsupported_error(&etag));
            }
            self.delete(&key, DeleteCondition::IfMatch(etag))?;
            Ok(ConnectionReport {
                provider: "webdav".to_string(),
                writable: true,
            })
        })();
        if result.is_err() {
            let _ = self.delete(&key, DeleteCondition::Any);
        }
        result
    }

    fn head(&self, key: &ObjectKey) -> Result<Option<ObjectMeta>, DeviceSyncError> {
        let Some(meta) = self.head_raw(key)? else {
            return Ok(None);
        };
        self.ensure_strong_meta(meta).map(Some)
    }

    fn get_bounded(
        &self,
        key: &ObjectKey,
        max_bytes: u64,
        sink: &mut dyn Write,
    ) -> Result<ObjectMeta, DeviceSyncError> {
        self.validate_key(key)?;
        let max_bytes = max_bytes.min(MAX_ENCRYPTED_SNAPSHOT_BYTES);
        let url = self.url_for_key(key)?;
        let headers = self.common_headers();
        let response = self.execute("GET", &url, &headers, SendBody::none())?;
        let status = response.status().as_u16();
        if !is_success(status) {
            return Err(status_error("get", status));
        }
        let data = read_response_to_sink(response, max_bytes, sink, "get")?;
        let etag = if is_strong_etag(data.etag.as_deref()) {
            data.etag
        } else {
            Some(format!("sha256:{}", digest_hex(&data.digest)))
        };
        Ok(ObjectMeta {
            key: key.clone(),
            size: data.size,
            etag,
        })
    }

    fn get_prefix(
        &self,
        key: &ObjectKey,
        max_bytes: u64,
        sink: &mut dyn Write,
    ) -> Result<ObjectMeta, DeviceSyncError> {
        self.validate_key(key)?;
        let max_bytes = max_bytes.min(MAX_ENCRYPTED_SNAPSHOT_BYTES);
        let url = self.url_for_key(key)?;
        let mut headers = self.common_headers();
        if max_bytes == 0 {
            let meta = self.head(key)?.ok_or_else(|| {
                DeviceSyncError::new(DeviceSyncErrorCode::VaultNotFound, false)
            })?;
            return Ok(meta);
        }
        headers.push(("Range", format!("bytes=0-{}", max_bytes - 1)));
        let response = self.execute("GET", &url, &headers, SendBody::none())?;
        let status = response.status().as_u16();
        if !is_success(status) {
            return Err(status_error("get_prefix", status));
        }
        match status {
            206 => {
                let content_range = response_header(&response, "content-range")?
                    .ok_or_else(|| protocol_error("missing WebDAV Content-Range"))?;
                let parsed = parse_content_range(&content_range)
                    .ok_or_else(|| protocol_error("invalid WebDAV Content-Range"))?;
                if parsed.start != 0 || parsed.total.is_none() {
                    return Err(protocol_error("invalid WebDAV Content-Range bounds"));
                }
                let range_length = parsed
                    .end
                    .checked_sub(parsed.start)
                    .and_then(|length| length.checked_add(1))
                    .ok_or_else(|| protocol_error("WebDAV Content-Range length overflow"))?;
                if range_length > max_bytes {
                    return Err(protocol_error("WebDAV Content-Range exceeds requested bound"));
                }
                if parsed.total.unwrap() > MAX_ENCRYPTED_SNAPSHOT_BYTES {
                    return Err(object_too_large_error("get_prefix"));
                }
                let data = read_response_to_sink(response, max_bytes, sink, "get_prefix")?;
                if data.size != range_length {
                    return Err(DeviceSyncError::new(
                        DeviceSyncErrorCode::IntegrityFailed,
                        false,
                    ));
                }
                Ok(ObjectMeta {
                    key: key.clone(),
                    size: parsed.total.unwrap(),
                    etag: data.etag,
                })
            }
            200 => {
                drop(response);
                Err(range_not_honored_error())
            }
            _ => Err(status_error("get_prefix", status)),
        }
    }

    fn put(
        &self,
        key: &ObjectKey,
        source: &mut dyn Read,
        len: u64,
        condition: PutCondition,
    ) -> Result<ObjectMeta, DeviceSyncError> {
        self.validate_key(key)?;
        if len > MAX_ENCRYPTED_SNAPSHOT_BYTES {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ObjectTooLarge,
                false,
            ));
        }
        let parent_len = key.segments().len().saturating_sub(1);
        let parent = ObjectPrefix::from_path(
            &key.segments()[..parent_len]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join("/"),
        )?;
        self.create_prefix(&parent)?;
        match condition {
            PutCondition::Any => Ok(self.put_once(key, source, len, None)?.meta),
            PutCondition::IfNoneMatch => Ok(self
                .put_once(key, source, len, Some(("If-None-Match", "*")))?
                .meta),
            PutCondition::IfMatch(expected) if is_strong_etag(Some(expected.as_str())) => Ok(self
                .put_once(key, source, len, Some(("If-Match", expected.as_str())))?
                .meta),
            PutCondition::IfMatch(expected) => self.fallback_put(key, source, len, &expected),
        }
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
        if start > MAX_REMOTE_LIST_OBJECTS {
            return Err(DeviceSyncError::invalid_config("object list cursor"));
        }
        let url = self.url_for_prefix(prefix)?;
        // List all descendants, matching LocalFolderStore::list (which walks
        // the whole subtree) and the ObjectStore contract relied on by
        // remote_view: it must surface nested <device-id>/head.json under the
        // devices prefix, not only direct children. Depth:1 recursion returns
        // every object under the prefix without relying on `Depth: infinity`,
        // which several servers reject with 403.
        let entries = match self.propfind_recursive(&url) {
            Ok(entries) => entries,
            // A list prefix does not exist until the first object under it is
            // written. Object-store semantics expose that state as an empty
            // result so the first device can publish its Head.
            Err(error) if error.code == DeviceSyncErrorCode::VaultNotFound => {
                return Ok(ObjectPage {
                    objects: Vec::new(),
                    next_cursor: None,
                });
            }
            Err(error) => return Err(error),
        };
        let mut objects = Vec::new();
        let mut seen_keys = HashSet::new();
        for entry in entries {
            let Some(key) = self.key_from_href(&entry.href)? else {
                continue;
            };
            if entry.collection {
                continue;
            }
            if !key.segments().starts_with(prefix.segments())
                || !seen_keys.insert(key.to_string())
            {
                return Err(integrity_error("duplicate or unexpected WebDAV object"));
            }
            // Prefer a strong ETag straight from PROPFIND. Some servers omit
            // getetag in Depth:1 responses (or publish only a weak ETag)
            // even though they serve a strong one on HEAD/GET. Using a missing
            // ETag downstream makes a head update fall back to
            // If-None-Match: *, which 412s for an object that already exists.
            // Fall back to head(), which rescues a real strong ETag when
            // available and otherwise synthesizes a content digest so callers
            // still see a stable meta.etag.
            let meta = if entry.size.is_some() && is_strong_etag(entry.etag.as_deref()) {
                ObjectMeta {
                    key,
                    size: entry.size.unwrap(),
                    etag: entry.etag,
                }
            } else {
                self.head(&key)?.ok_or_else(|| {
                    DeviceSyncError::new(DeviceSyncErrorCode::RemoteChanged, true)
                })?
            };
            objects.push(meta);
            if objects.len() > MAX_REMOTE_LIST_OBJECTS {
                return Err(DeviceSyncError::new(
                    DeviceSyncErrorCode::ObjectTooLarge,
                    false,
                ));
            }
        }
        objects.sort_by_key(|meta| meta.key.to_string());
        let end = (start + 1_000).min(objects.len());
        Ok(ObjectPage {
            objects: objects[start.min(objects.len())..end].to_vec(),
            next_cursor: (end < objects.len()).then(|| end.to_string()),
        })
    }

    fn delete(&self, key: &ObjectKey, condition: DeleteCondition) -> Result<(), DeviceSyncError> {
        self.validate_key(key)?;
        match condition {
            DeleteCondition::Any => {
                let url = self.url_for_key(key)?;
                let headers = self.common_headers();
                let response = self.execute("DELETE", &url, &headers, SendBody::none())?;
                let status = response.status().as_u16();
                if status == 404 || is_success(status) {
                    Ok(())
                } else {
                    Err(status_error("delete", status))
                }
            }
            DeleteCondition::IfMatch(expected)
                if is_strong_etag(Some(expected.as_str())) =>
            {
                let url = self.url_for_key(key)?;
                let mut headers = self.common_headers();
                headers.push(("If-Match", expected));
                let response = self.execute("DELETE", &url, &headers, SendBody::none())?;
                let status = response.status().as_u16();
                if status == 404 || is_success(status) {
                    Ok(())
                } else {
                    Err(status_error("delete", status))
                }
            }
            DeleteCondition::IfMatch(expected) => self.fallback_delete(key, &expected),
        }
    }

    fn create_prefix(&self, prefix: &ObjectPrefix) -> Result<(), DeviceSyncError> {
        self.validate_prefix(prefix)?;
        let mut current = Vec::new();
        for segment in prefix.segments() {
            current.push(segment.clone());
            let url = self.url_for_collection_segments(&current)?;
            let headers = self.common_headers();
            let response = self.execute("MKCOL", &url, &headers, SendBody::none())?;
            let status = response.status().as_u16();
            if is_success(status) {
                drop(response);
                continue;
            }
            if status == 405 {
                drop(response);
                self.confirm_collection(&url)?;
                continue;
            }
            return Err(status_error("create_prefix", status));
        }
        Ok(())
    }

}

impl WebDavStore {
    fn confirm_collection(&self, url: &Url) -> Result<(), DeviceSyncError> {
        let target = decode_path_segments(url.path())?;
        let entries = self.propfind(url, "0").map_err(|error| {
            if error.code == DeviceSyncErrorCode::VaultNotFound {
                DeviceSyncError::new(
                    DeviceSyncErrorCode::RemoteDirectoryUnavailable,
                    false,
                )
                .with_argument("provider", "webdav")
                .with_argument("operation", "propfind")
                .with_argument("http_status", "404")
            } else {
                error
            }
        })?;
        let mut matched = None;
        for entry in entries {
            let entry_url = href_to_url(&self.endpoint, &entry.href)?;
            if decode_path_segments(entry_url.path())? != target {
                continue;
            }
            if matched.replace(entry.collection).is_some() {
                return Err(integrity_error("duplicate WebDAV collection response"));
            }
        }
        match matched {
            Some(true) => Ok(()),
            Some(false) => Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ImmutableObjectConflict,
                false,
            )),
            None => Err(protocol_error("MKCOL 405 was not confirmed as a collection")),
        }
    }
}

struct PutOutcome {
    meta: ObjectMeta,
}

struct ResponseData {
    size: u64,
    etag: Option<String>,
    digest: [u8; 32],
}

struct BoundedReader<'a> {
    source: &'a mut dyn Read,
    remaining: u64,
}

impl<'a> BoundedReader<'a> {
    fn new(source: &'a mut dyn Read, remaining: u64) -> Self {
        Self {
            source,
            remaining,
        }
    }

    fn remaining(&self) -> u64 {
        self.remaining
    }

}

impl Read for BoundedReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let max = (self.remaining.min(buffer.len() as u64)) as usize;
        let count = self.source.read(&mut buffer[..max])?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "short request body",
            ));
        }
        self.remaining -= count as u64;
        Ok(count)
    }
}

fn read_response_to_sink(
    response: http::Response<ureq::Body>,
    max_bytes: u64,
    sink: &mut dyn Write,
    operation: &'static str,
) -> Result<ResponseData, DeviceSyncError> {
    let expected = response_content_length(&response)?;
    if expected.is_some_and(|length| length > max_bytes) {
        return Err(object_too_large_error(operation));
    }
    let etag = response_header(&response, "etag")?;
    let (_, body) = response.into_parts();
    let mut reader = body
        .into_with_config()
        .limit(max_bytes.saturating_add(1))
        .reader();
    let mut total = 0u64;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 32 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| map_body_io_error(error, operation))?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| object_too_large_error(operation))?;
        if total > max_bytes {
            return Err(object_too_large_error(operation));
        }
        hasher.update(&buffer[..count]);
        sink.write_all(&buffer[..count])
            .map_err(|_| DeviceSyncError::apply_failed("unable to write WebDAV response"))?;
    }
    if expected.is_some_and(|length| length != total) {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    }
    Ok(ResponseData {
        size: total,
        etag,
        digest: hasher.finalize().into(),
    })
}

fn parse_propfind_xml(data: &[u8]) -> Result<Vec<DavEntry>, DeviceSyncError> {
    let mut reader = Reader::from_reader(data);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut entries = Vec::new();
    let mut current: Option<DavEntry> = None;
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut seen_hrefs = HashSet::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(start)) => {
                let qualified_name = start.name();
                let name = local_name(qualified_name.as_ref());
                if name == b"response" {
                    if current.is_some() {
                        return Err(integrity_error("nested WebDAV response"));
                    }
                    current = Some(DavEntry::default());
                    stack.clear();
                } else if current.is_some() {
                    if name == b"collection" {
                        if let Some(entry) = current.as_mut() {
                            entry.collection = true;
                        }
                    }
                    if is_propfind_field(name) {
                        mark_propfind_field(current.as_mut(), name)?;
                    }
                    stack.push(name.to_vec());
                }
            }
            Ok(Event::Empty(empty)) => {
                if current.is_some() {
                    let qualified_name = empty.name();
                    let name = local_name(qualified_name.as_ref());
                    if name == b"collection" {
                        if let Some(entry) = current.as_mut() {
                            entry.collection = true;
                        }
                    } else if is_propfind_field(name) {
                        mark_propfind_field(current.as_mut(), name)?;
                    }
                }
            }
            Ok(Event::Text(text)) => {
                if let Some(field) = stack.last() {
                    let value = text
                        .decode()
                        .map_err(|_| integrity_error("invalid WebDAV XML text"))?;
                    let value = unescape(value.as_ref())
                        .map_err(|_| integrity_error("invalid WebDAV XML escape"))?
                        .into_owned();
                    set_propfind_field(current.as_mut(), field, value)?;
                }
            }
            Ok(Event::CData(text)) => {
                if let Some(field) = stack.last() {
                    let value = text
                        .decode()
                        .map_err(|_| integrity_error("invalid WebDAV XML text"))?
                        .into_owned();
                    set_propfind_field(current.as_mut(), field, value)?;
                }
            }
            Ok(Event::End(end)) => {
                let qualified_name = end.name();
                let name = local_name(qualified_name.as_ref());
                if name == b"response" {
                    let entry = current
                        .take()
                        .ok_or_else(|| integrity_error("unexpected WebDAV response end"))?
                        .finish()?;
                    if !stack.is_empty() {
                        return Err(integrity_error("malformed WebDAV response"));
                    }
                    if entry.href.is_empty() || !seen_hrefs.insert(entry.href.clone()) {
                        return Err(integrity_error("missing or duplicate WebDAV href"));
                    }
                    entries.push(entry);
                    if entries.len() > MAX_REMOTE_LIST_OBJECTS {
                        return Err(DeviceSyncError::new(
                            DeviceSyncErrorCode::ObjectTooLarge,
                            false,
                        ));
                    }
                } else if current.is_some() {
                    stack
                        .pop()
                        .filter(|field| field.as_slice() == name)
                        .ok_or_else(|| integrity_error("malformed WebDAV XML nesting"))?;
                }
            }
            Ok(Event::Eof) => break,
            Ok(Event::Decl(_) | Event::PI(_) | Event::Comment(_)) => {}
            Ok(Event::GeneralRef(reference)) => {
                if let Some(field) = stack.last() {
                    let reference = reference
                        .decode()
                        .map_err(|_| integrity_error("invalid WebDAV XML reference"))?;
                    let value = unescape(&format!("&{};", reference))
                        .map_err(|_| integrity_error("invalid WebDAV XML reference"))?
                        .into_owned();
                    set_propfind_field(current.as_mut(), field, value)?;
                }
            }
            Ok(Event::DocType(_)) => {
                return Err(integrity_error("DOCTYPE is not allowed in WebDAV XML"));
            }
            Err(_) => return Err(integrity_error("invalid WebDAV XML")),
        }
        buffer.clear();
    }
    if current.is_some() || !stack.is_empty() || entries.is_empty() {
        return Err(integrity_error("incomplete WebDAV XML response"));
    }
    Ok(entries)
}

#[derive(Default)]
struct DavEntry {
    href: String,
    content_length_text: String,
    etag_text: String,
    href_seen: bool,
    content_length_seen: bool,
    etag_seen: bool,
    size: Option<u64>,
    etag: Option<String>,
    collection: bool,
}

impl DavEntry {
    fn finish(mut self) -> Result<Self, DeviceSyncError> {
        if !self.content_length_text.trim().is_empty() {
            self.size = Some(
                self.content_length_text
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| protocol_error("invalid WebDAV content length"))?,
            );
        }
        let etag = self.etag_text.trim();
        if etag.len() > MAX_ETAG_BYTES || etag.chars().any(char::is_control) {
            return Err(integrity_error("invalid WebDAV ETag"));
        }
        self.etag = (!etag.is_empty()).then(|| etag.to_string());
        Ok(self)
    }
}

fn set_propfind_field(
    entry: Option<&mut DavEntry>,
    field: &[u8],
    value: String,
) -> Result<(), DeviceSyncError> {
    let Some(entry) = entry else {
        return Ok(());
    };
    match field {
        b"href" => entry.href.push_str(&value),
        b"getcontentlength" => entry.content_length_text.push_str(&value),
        b"getetag" => {
            entry.etag_text.push_str(&value);
        }
        _ => {}
    }
    Ok(())
}

fn is_propfind_field(name: &[u8]) -> bool {
    matches!(name, b"href" | b"getcontentlength" | b"getetag")
}

fn mark_propfind_field(
    entry: Option<&mut DavEntry>,
    field: &[u8],
) -> Result<(), DeviceSyncError> {
    let Some(entry) = entry else {
        return Ok(());
    };
    let seen = match field {
        b"href" => &mut entry.href_seen,
        b"getcontentlength" => &mut entry.content_length_seen,
        b"getetag" => &mut entry.etag_seen,
        _ => return Ok(()),
    };
    if *seen {
        return Err(integrity_error("duplicate WebDAV property"));
    }
    *seen = true;
    Ok(())
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn encode_path_segment(segment: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(*byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

fn percent_decode_segment(segment: &str) -> Result<String, DeviceSyncError> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(integrity_error("invalid percent-encoded WebDAV path"));
            }
            let high = hex_value(bytes[index + 1])
                .ok_or_else(|| integrity_error("invalid percent-encoded WebDAV path"))?;
            let low = hex_value(bytes[index + 2])
                .ok_or_else(|| integrity_error("invalid percent-encoded WebDAV path"))?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(decoded)
        .map_err(|_| integrity_error("invalid UTF-8 WebDAV path"))?;
    if !valid_segment(&decoded) {
        return Err(integrity_error("unsafe WebDAV path segment"));
    }
    Ok(decoded)
}

fn decode_path_segments(path: &str) -> Result<Vec<String>, DeviceSyncError> {
    let raw_segments = path.split('/').collect::<Vec<_>>();
    if raw_segments
        .iter()
        .enumerate()
        .any(|(index, segment)| segment.is_empty() && index > 0 && index + 1 < raw_segments.len())
    {
        return Err(integrity_error("ambiguous WebDAV path"));
    }
    raw_segments
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .map(percent_decode_segment)
        .collect()
}

fn href_to_url(endpoint: &Url, href: &str) -> Result<Url, DeviceSyncError> {
    if href.is_empty() || href.chars().any(char::is_control) {
        return Err(integrity_error("invalid WebDAV href"));
    }
    let raw_path = href
        .split_once('?')
        .map(|(path, _)| path)
        .unwrap_or(href)
        .split_once('#')
        .map(|(path, _)| path)
        .unwrap_or_else(|| {
            href.split_once('?')
                .map(|(path, _)| path)
                .unwrap_or(href)
        });
    for segment in raw_path.split('/') {
        if segment.is_empty() {
            continue;
        }
        let decoded = percent_decode_segment(segment)?;
        if decoded == "." || decoded == ".." {
            return Err(integrity_error("path traversal in WebDAV href"));
        }
    }
    let candidate = Url::parse(href).ok();
    let url = match candidate {
        Some(candidate) if !candidate.scheme().is_empty() || candidate.host_str().is_some() => {
            candidate
        }
        _ => endpoint
            .join(href)
            .map_err(|_| integrity_error("invalid WebDAV href"))?,
    };
    if url.scheme() != endpoint.scheme()
        || url.host_str().map(str::to_ascii_lowercase)
            != endpoint.host_str().map(str::to_ascii_lowercase)
        || url.port_or_known_default() != endpoint.port_or_known_default()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(integrity_error("WebDAV href outside configured endpoint"));
    }
    Ok(url)
}

fn response_header(
    response: &http::Response<ureq::Body>,
    name: &str,
) -> Result<Option<String>, DeviceSyncError> {
    let Some(value) = response.headers().get(name) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| protocol_error("invalid WebDAV response header"))?
        .trim();
    if value.len() > MAX_ETAG_BYTES || value.chars().any(char::is_control) {
        return Err(protocol_error("invalid WebDAV response header"));
    }
    Ok((!value.is_empty()).then(|| value.to_string()))
}

fn response_content_length(
    response: &http::Response<ureq::Body>,
) -> Result<Option<u64>, DeviceSyncError> {
    let Some(value) = response.headers().get("content-length") else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| protocol_error("invalid WebDAV content length"))?;
    value
        .trim()
        .parse::<u64>()
        .map(Some)
        .map_err(|_| protocol_error("invalid WebDAV content length"))
}

fn is_strong_etag(etag: Option<&str>) -> bool {
    let Some(etag) = etag.map(str::trim) else {
        return false;
    };
    !etag.is_empty()
        && !etag.starts_with("W/")
        && !is_synthetic_etag(etag)
        && !etag.chars().any(char::is_control)
}

fn is_synthetic_etag(etag: &str) -> bool {
    etag.strip_prefix("sha256:")
        .is_some_and(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn digest_hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

fn status_error(operation: &'static str, status: u16) -> DeviceSyncError {
    let (code, retryable) = match status {
        401 | 403 => (DeviceSyncErrorCode::AuthenticationFailed, false),
        404 => (DeviceSyncErrorCode::VaultNotFound, false),
        409 => (DeviceSyncErrorCode::ImmutableObjectConflict, false),
        412 => (DeviceSyncErrorCode::RemoteChanged, true),
        423 => (DeviceSyncErrorCode::RemoteChanged, true),
        408 | 429 => (DeviceSyncErrorCode::RateLimited, true),
        500..=599 => (DeviceSyncErrorCode::NetworkUnreachable, true),
        _ => (DeviceSyncErrorCode::ProtocolUnsupported, false),
    };
    DeviceSyncError::new(code, retryable)
        .with_argument("provider", "webdav")
        .with_argument("operation", operation)
        .with_argument("http_status", status.to_string())
}

fn map_ureq_error(error: &ureq::Error, operation: &'static str) -> DeviceSyncError {
    let base = match error {
        ureq::Error::BodyExceedsLimit(_) => object_too_large_error(operation),
        ureq::Error::StatusCode(status) => status_error(operation, *status),
        ureq::Error::RequireHttpsOnly(_) => {
            DeviceSyncError::invalid_config("provider.endpoint must use HTTPS")
        }
        ureq::Error::BadUri(_) | ureq::Error::Http(_) => {
            protocol_error("invalid WebDAV request")
        }
        ureq::Error::Protocol(_) | ureq::Error::BodyStalled => {
            DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false)
        }
        ureq::Error::Io(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false)
        }
        ureq::Error::Io(_)
        | ureq::Error::Timeout(_)
        | ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::RedirectFailed
        | ureq::Error::TooManyRedirects
        | ureq::Error::Tls(_)
        | ureq::Error::TlsRequired => {
            DeviceSyncError::new(DeviceSyncErrorCode::NetworkUnreachable, true)
        }
        _ => DeviceSyncError::new(DeviceSyncErrorCode::NetworkUnreachable, true),
    };
    add_transport_context(base, operation)
}

fn map_body_io_error(error: io::Error, operation: &'static str) -> DeviceSyncError {
    if let Some(inner) = error.get_ref().and_then(|value| value.downcast_ref::<ureq::Error>()) {
        return map_ureq_error(inner, operation);
    }
    add_transport_context(
        DeviceSyncError::new(DeviceSyncErrorCode::NetworkUnreachable, true),
        operation,
    )
}

fn add_transport_context(
    error: DeviceSyncError,
    operation: &'static str,
) -> DeviceSyncError {
    if error.arguments.contains_key("provider") {
        error
    } else {
        error
            .with_argument("provider", "webdav")
            .with_argument("operation", operation)
    }
}

fn object_too_large_error(operation: &'static str) -> DeviceSyncError {
    DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false)
        .with_argument("provider", "webdav")
        .with_argument("operation", operation)
}

fn range_not_honored_error() -> DeviceSyncError {
    DeviceSyncError::new(DeviceSyncErrorCode::ProtocolUnsupported, false)
        .with_argument("provider", "webdav")
        .with_argument("detail", "server did not honor the requested range")
}

fn conditional_write_unsupported_error(_etag: &str) -> DeviceSyncError {
    DeviceSyncError::new(DeviceSyncErrorCode::ProtocolUnsupported, false)
        .with_argument("provider", "webdav")
        .with_argument("detail", "WebDAV server does not provide a strong ETag for atomic updates")
}

fn protocol_error(detail: &'static str) -> DeviceSyncError {
    DeviceSyncError::new(DeviceSyncErrorCode::ProtocolUnsupported, false)
        .with_argument("provider", "webdav")
        .with_argument("detail", detail)
}

fn integrity_error(detail: &'static str) -> DeviceSyncError {
    DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false)
        .with_argument("provider", "webdav")
        .with_argument("detail", detail)
}

#[derive(Debug, PartialEq, Eq)]
struct ContentRange {
    start: u64,
    end: u64,
    total: Option<u64>,
}

fn parse_content_range(value: &str) -> Option<ContentRange> {
    let (unit, range) = value.trim().split_once(' ')?;
    if unit != "bytes" {
        return None;
    }
    let (bounds, total) = range.split_once('/')?;
    let (start, end) = bounds.split_once('-')?;
    let start = start.parse::<u64>().ok()?;
    let end = end.parse::<u64>().ok()?;
    end.checked_sub(start)?.checked_add(1)?;
    let total = if total == "*" {
        None
    } else {
        let total = total.parse::<u64>().ok()?;
        Some((total > end).then_some(total)?)
    };
    Some(ContentRange { start, end, total })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_each_path_segment_without_encoding_slashes() {
        assert_eq!(encode_path_segment("a b/%中文"), "a%20b%2F%25%E4%B8%AD%E6%96%87");
    }

    #[test]
    fn rejects_unsafe_endpoint_parts_and_requires_explicit_http_opt_in() {
        let credentials = WebDavCredentials::new("user@example.com", "app-password").unwrap();
        for endpoint in [
            "http://example.com/dav/",
            "https://user:password@example.com/dav/",
            "https://example.com/dav/?token=secret",
            "https://example.com/dav/#fragment",
        ] {
            assert!(WebDavStore::new(endpoint, "sync", credentials.clone()).is_err());
        }
        assert!(WebDavStore::new_for_test(
            "http://127.0.0.1:1/dav/",
            "sync",
            credentials
        )
        .is_ok());
        assert!(WebDavStore::new_allowing_http(
            "http://example.com/dav/",
            "sync",
            WebDavCredentials::new("user@example.com", "app-password").unwrap()
        )
        .is_ok());
    }

    #[test]
    fn parses_namespaced_propfind_and_rejects_malformed_or_duplicate_responses() {
        let xml = br#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response><d:href>/dav/sync/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
  <d:response><d:href>/dav/sync/a%20b</d:href><d:propstat><d:prop><d:getcontentlength>4</d:getcontentlength><d:getetag>W/&quot;weak&quot;</d:getetag></d:prop></d:propstat></d:response>
</d:multistatus>"#;
        let entries = parse_propfind_xml(xml).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].collection);
        assert_eq!(entries[1].size, Some(4));
        assert_eq!(entries[1].etag.as_deref(), Some("W/\"weak\""));

        let duplicate = br#"<multistatus><response><href>/dav/a</href></response><response><href>/dav/a</href></response></multistatus>"#;
        assert!(parse_propfind_xml(duplicate).is_err());
        let duplicate_property = br#"<multistatus><response><href>/dav/a</href><getetag>\"one\"</getetag><getetag>\"two\"</getetag></response></multistatus>"#;
        assert!(parse_propfind_xml(duplicate_property).is_err());
        assert!(parse_propfind_xml(b"<multistatus><response>").is_err());
    }

    #[test]
    fn parses_content_range_bounds() {
        assert_eq!(
            parse_content_range("bytes 0-63/128"),
            Some(ContentRange {
                start: 0,
                end: 63,
                total: Some(128)
            })
        );
        assert!(parse_content_range("bytes 1-63/128").is_some());
        assert!(parse_content_range("bytes 0-63/*").is_some());
        assert!(parse_content_range("bytes 63-0/128").is_none());
        assert!(parse_content_range("bytes 0-18446744073709551615/*").is_none());
    }

    #[test]
    fn maps_webdav_statuses_without_response_body_details() {
        assert_eq!(
            status_error("head", 401).code,
            DeviceSyncErrorCode::AuthenticationFailed
        );
        assert_eq!(
            status_error("put", 409).code,
            DeviceSyncErrorCode::ImmutableObjectConflict
        );
        assert_eq!(status_error("put", 412).code, DeviceSyncErrorCode::RemoteChanged);
        assert_eq!(status_error("get", 429).code, DeviceSyncErrorCode::RateLimited);
        assert_eq!(status_error("get", 503).code, DeviceSyncErrorCode::NetworkUnreachable);
    }

    #[test]
    fn credentials_debug_redacts_password() {
        let credentials = WebDavCredentials::new("user", "secret-password").unwrap();
        let debug = format!("{credentials:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("secret-password"));
    }

    #[test]
    fn uses_existing_hash_helper_for_known_payloads() {
        assert_eq!(
            crate::device_sync::crypto::sha256_hex(b""),
            digest_hex(&Sha256::digest(b"").into())
        );
    }
}
