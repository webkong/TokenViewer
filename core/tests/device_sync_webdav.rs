use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tokenviewer_core::device_sync::models::{
    DeviceSyncConfig, DeviceSyncErrorCode, ProviderConfig,
};
use tokenviewer_core::device_sync::store::{
    DeleteCondition, ObjectKey, ObjectPrefix, ObjectStore, PutCondition, WebDavCredentials,
    WebDavStore,
};

#[derive(Clone, Copy)]
enum RangeMode {
    Honor,
    Ignore,
}

#[derive(Default)]
struct MockState {
    objects: BTreeMap<String, StoredObject>,
    collections: BTreeSet<String>,
    methods: Vec<String>,
    next_etag: u64,
    reject_auth: bool,
    suppress_etag: bool,
    forced_status: HashMap<String, u16>,
}

struct StoredObject {
    body: Vec<u8>,
    etag: String,
}

struct MockWebDav {
    endpoint: String,
    state: Arc<Mutex<MockState>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockWebDav {
    fn new(range_mode: RangeMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(MockState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        if let Some(request) = read_request(&mut stream) {
                            handle_request(&mut stream, request, &thread_state, range_mode);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            endpoint: format!("http://{address}/dav/"),
            state,
            stop,
            thread: Some(thread),
        }
    }

    fn set_reject_auth(&self, reject: bool) {
        self.state.lock().unwrap().reject_auth = reject;
    }

    fn force_status(&self, method: &str, status: Option<u16>) {
        let mut state = self.state.lock().unwrap();
        if let Some(status) = status {
            state.forced_status.insert(method.to_string(), status);
        } else {
            state.forced_status.remove(method);
        }
    }

    fn set_suppress_etag(&self, suppress: bool) {
        self.state.lock().unwrap().suppress_etag = suppress;
    }

    fn methods(&self) -> Vec<String> {
        self.state.lock().unwrap().methods.clone()
    }
}

impl Drop for MockWebDav {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let wake_address = self
            .endpoint
            .strip_prefix("http://")
            .and_then(|value| value.strip_suffix("/dav/"));
        if let Some(address) = wake_address {
            let _ = TcpStream::connect(address);
        }
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

struct Request {
    method: String,
    target: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut bytes = Vec::new();
    let header_end;
    loop {
        let mut chunk = [0u8; 4096];
        let count = stream.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = index + 4;
            break;
        }
        if bytes.len() > 64 * 1024 {
            return None;
        }
    }

    let header_text = std::str::from_utf8(&bytes[..header_end]).ok()?;
    let mut lines = header_text.split("\r\n");
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_string();
    let target = request_line.next()?.to_string();
    let mut headers = HashMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':')?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    if headers.get("expect").is_some_and(|value| value.eq_ignore_ascii_case("100-continue")) {
        stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").ok()?;
    }

    let length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while bytes.len() < header_end + length {
        let mut chunk = [0u8; 4096];
        let count = stream.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Some(Request {
        method,
        target,
        headers,
        body: bytes[header_end..header_end + length].to_vec(),
    })
}

fn handle_request(
    stream: &mut TcpStream,
    request: Request,
    state: &Arc<Mutex<MockState>>,
    range_mode: RangeMode,
) {
    let method = request.method.to_ascii_uppercase();
    let path = request
        .target
        .split_once('?')
        .map(|(path, _)| path)
        .unwrap_or(&request.target);

    let (reject_auth, forced_status) = {
        let mut state = state.lock().unwrap();
        state.methods.push(method.clone());
        (state.reject_auth, state.forced_status.get(&method).copied())
    };
    if reject_auth {
        respond(stream, 401, &[], b"");
        return;
    }
    if let Some(status) = forced_status {
        respond(stream, status, &[], b"");
        return;
    }

    let Some(segments) = decode_path_segments(path) else {
        respond(stream, 400, &[], b"");
        return;
    };
    if segments.first().map(String::as_str) != Some("dav") {
        respond(stream, 404, &[], b"");
        return;
    }
    let key = segments[1..].join("/");
    match method.as_str() {
        "OPTIONS" => respond(
            stream,
            200,
            &[
                ("DAV", "1, 2"),
                (
                    "Allow",
                    "OPTIONS, PROPFIND, HEAD, GET, PUT, DELETE, MKCOL",
                ),
            ],
            b"",
        ),
        "MKCOL" => handle_mkcol(stream, state, &key),
        "HEAD" => handle_head(stream, state, &key),
        "GET" => handle_get(stream, state, &key, &request, range_mode),
        "PUT" => handle_put(stream, state, &key, &request),
        "DELETE" => handle_delete(stream, state, &key, &request),
        "PROPFIND" => handle_propfind(stream, state, &key, &request),
        _ => respond(stream, 405, &[], b""),
    }
}

fn handle_mkcol(stream: &mut TcpStream, state: &Arc<Mutex<MockState>>, key: &str) {
    let mut state = state.lock().unwrap();
    if state.objects.contains_key(key) {
        respond(stream, 405, &[], b"");
    } else if state.collections.contains(key) {
        respond(stream, 405, &[], b"");
    } else {
        state.collections.insert(key.to_string());
        respond(stream, 201, &[], b"");
    }
}

fn handle_head(stream: &mut TcpStream, state: &Arc<Mutex<MockState>>, key: &str) {
    let state = state.lock().unwrap();
    let Some(object) = state.objects.get(key) else {
        respond(stream, 404, &[], b"");
        return;
    };
    let content_length = object.body.len().to_string();
    if state.suppress_etag {
        respond(stream, 200, &[("Content-Length", content_length.as_str())], b"");
    } else {
        respond(
            stream,
            200,
            &[
                ("Content-Length", content_length.as_str()),
                ("ETag", object.etag.as_str()),
            ],
            b"",
        );
    }
}

fn handle_get(
    stream: &mut TcpStream,
    state: &Arc<Mutex<MockState>>,
    key: &str,
    request: &Request,
    range_mode: RangeMode,
) {
    let state = state.lock().unwrap();
    let Some(object) = state.objects.get(key) else {
        respond(stream, 404, &[], b"");
        return;
    };
    let Some(range) = request.headers.get("range") else {
        if state.suppress_etag {
            respond(stream, 200, &[], &object.body);
        } else {
            respond(
                stream,
                200,
                &[("ETag", object.etag.as_str())],
                &object.body,
            );
        }
        return;
    };
    if matches!(range_mode, RangeMode::Ignore) {
        if state.suppress_etag {
            respond(stream, 200, &[], &object.body);
        } else {
            respond(
                stream,
                200,
                &[("ETag", object.etag.as_str())],
                &object.body,
            );
        }
        return;
    }
    let Some((start, requested_end)) = parse_range(range) else {
        respond(stream, 416, &[], b"");
        return;
    };
    if start >= object.body.len() {
        respond(stream, 416, &[], b"");
        return;
    }
    let end = requested_end.min(object.body.len() - 1);
    let body = &object.body[start..=end];
    let content_range = format!("bytes {start}-{end}/{}", object.body.len());
    if state.suppress_etag {
        respond(
            stream,
            206,
            &[("Content-Range", content_range.as_str())],
            body,
        );
    } else {
        respond(
            stream,
            206,
            &[
                ("Content-Range", content_range.as_str()),
                ("ETag", object.etag.as_str()),
            ],
            body,
        );
    }
}

fn handle_put(
    stream: &mut TcpStream,
    state: &Arc<Mutex<MockState>>,
    key: &str,
    request: &Request,
) {
    let mut state = state.lock().unwrap();
    if let Some(expected) = request.headers.get("if-none-match") {
        if expected == "*" && state.objects.contains_key(key) {
            respond(stream, 412, &[], b"");
            return;
        }
    }
    if let Some(expected) = request.headers.get("if-match") {
        if state.objects.get(key).map(|object| object.etag.as_str()) != Some(expected.as_str()) {
            respond(stream, 412, &[], b"");
            return;
        }
    }
    state.next_etag += 1;
    let etag = format!("\"{}\"", state.next_etag);
    state.objects.insert(
        key.to_string(),
        StoredObject {
            body: request.body.clone(),
            etag: etag.clone(),
        },
    );
    if state.suppress_etag {
        respond(stream, 201, &[], b"");
    } else {
        respond(stream, 201, &[("ETag", etag.as_str())], b"");
    }
}

fn handle_delete(
    stream: &mut TcpStream,
    state: &Arc<Mutex<MockState>>,
    key: &str,
    request: &Request,
) {
    let mut state = state.lock().unwrap();
    let Some(object) = state.objects.get(key) else {
        respond(stream, 404, &[], b"");
        return;
    };
    if let Some(expected) = request.headers.get("if-match") {
        if object.etag != *expected {
            respond(stream, 412, &[], b"");
            return;
        }
    }
    state.objects.remove(key);
    respond(stream, 204, &[], b"");
}

fn handle_propfind(
    stream: &mut TcpStream,
    state: &Arc<Mutex<MockState>>,
    key: &str,
    request: &Request,
) {
    let state = state.lock().unwrap();
    let target_is_collection = is_collection(&state, key);
    let target_exists = state.objects.contains_key(key) || target_is_collection || key.is_empty();
    if !target_exists {
        respond(stream, 404, &[], b"");
        return;
    }

    let depth = request
        .headers
        .get("depth")
        .map(String::as_str)
        .unwrap_or("0");
    let mut entries = BTreeMap::new();
    entries.insert(key.to_string(), target_is_collection);
    if depth == "1" {
        let prefix = if key.is_empty() {
            String::new()
        } else {
            format!("{key}/")
        };
        for object_key in state.objects.keys() {
            if let Some(child) = immediate_child(object_key, &prefix) {
                let collection = is_collection(&state, &child);
                entries.entry(child).or_insert(collection);
            }
        }
        for collection in &state.collections {
            if let Some(child) = immediate_child(collection, &prefix) {
                entries.entry(child).or_insert(true);
            }
        }
    } else if depth == "infinity" {
        // Recursive listing: every descendant under the target, files and
        // collections alike, as a real WebDAV server returns for Depth:infinity
        // and as LocalFolderStore::list walks the whole subtree.
        let prefix = if key.is_empty() {
            String::new()
        } else {
            format!("{key}/")
        };
        for object_key in state.objects.keys() {
            if object_key.starts_with(&prefix) {
                entries.entry(object_key.clone()).or_insert(false);
            }
        }
        for collection in &state.collections {
            if collection.starts_with(&prefix) {
                entries.entry(collection.clone()).or_insert(true);
            }
        }
    }

    let mut xml = String::from("<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\">");
    for (entry_key, collection) in entries {
        xml.push_str(&propfind_entry(&state, &entry_key, collection));
    }
    xml.push_str("</d:multistatus>");
    respond(
        stream,
        207,
        &[("Content-Type", "application/xml")],
        xml.as_bytes(),
    );
}

fn propfind_entry(state: &MockState, key: &str, collection: bool) -> String {
    let href = href_for_key(key, collection);
    let mut entry = format!(
        "<d:response><d:href>{href}</d:href><d:propstat><d:prop>"
    );
    if let Some(object) = state.objects.get(key) {
        entry.push_str(&format!(
            "<d:getcontentlength>{}</d:getcontentlength>",
            object.body.len()
        ));
        if !state.suppress_etag {
            entry.push_str(&format!(
                "<d:getetag>{}</d:getetag>",
                xml_escape(&object.etag)
            ));
        }
    }
    if collection {
        entry.push_str("<d:resourcetype><d:collection/></d:resourcetype>");
    } else {
        entry.push_str("<d:resourcetype/>");
    }
    entry.push_str("</d:prop></d:propstat></d:response>");
    entry
}

fn respond(stream: &mut TcpStream, status: u16, headers: &[(&str, &str)], body: &[u8]) {
    let reason = match status {
        201 => "Created",
        204 => "No Content",
        207 => "Multi-Status",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        412 => "Precondition Failed",
        416 => "Range Not Satisfiable",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    };
    let has_content_length = headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("Content-Length"));
    let mut response = format!("HTTP/1.1 {status} {reason}\r\nConnection: close\r\n");
    if !has_content_length {
        response.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    for (name, value) in headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("\r\n");
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.write_all(body);
}

fn decode_path_segments(path: &str) -> Option<Vec<String>> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(percent_decode)
        .collect()
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            decoded.push((hex(bytes[index + 1])? << 4) | hex(bytes[index + 2])?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn encode_path_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z'
            | b'A'..=b'Z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~' => char::from(byte).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn href_for_key(key: &str, collection: bool) -> String {
    let mut href = String::from("/dav/");
    href.push_str(
        &key.split('/')
            .filter(|segment| !segment.is_empty())
            .map(encode_path_segment)
            .collect::<Vec<_>>()
            .join("/"),
    );
    if collection {
        href.push('/');
    }
    href
}

fn is_collection(state: &MockState, key: &str) -> bool {
    state.collections.contains(key)
        || state
            .objects
            .keys()
            .any(|object| object.starts_with(&format!("{key}/")))
}

fn immediate_child(value: &str, prefix: &str) -> Option<String> {
    let child = value.strip_prefix(prefix)?;
    if child.is_empty() {
        return None;
    }
    Some(
        child
            .split_once('/')
            .map(|(name, _)| format!("{prefix}{name}"))
            .unwrap_or_else(|| value.to_string()),
    )
}

fn parse_range(value: &str) -> Option<(usize, usize)> {
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()?))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn store(endpoint: &str) -> WebDavStore {
    store_with_prefix(endpoint, "sync")
}

fn store_with_prefix(endpoint: &str, remote_prefix: &str) -> WebDavStore {
    WebDavStore::new_for_test(
        endpoint,
        remote_prefix,
        WebDavCredentials::new("user@example.com", "app-password").unwrap(),
    )
    .unwrap()
}

#[test]
fn webdav_object_store_contract_covers_all_operations() {
    let server = MockWebDav::new(RangeMode::Honor);
    let store = store(&server.endpoint);
    let prefix = ObjectPrefix::from_path("sync/vault").unwrap();
    store.create_prefix(&prefix).unwrap();

    let key = ObjectKey::from_path("sync/vault/file name.txt").unwrap();
    let body = b"0123456789abcdef";
    let mut source = &body[..];
    let created = store
        .put(&key, &mut source, body.len() as u64, PutCondition::IfNoneMatch)
        .unwrap();
    assert_eq!(created.size, body.len() as u64);
    assert!(created.etag.is_some());

    let duplicate = store.put(
        &key,
        &mut &body[..],
        body.len() as u64,
        PutCondition::IfNoneMatch,
    );
    assert_eq!(duplicate.unwrap_err().code, DeviceSyncErrorCode::RemoteChanged);

    let headed = store.head(&key).unwrap().unwrap();
    assert_eq!(headed.size, body.len() as u64);
    assert_eq!(headed.etag, created.etag);

    let mut downloaded = Vec::new();
    let downloaded_meta = store.get_bounded(&key, 64, &mut downloaded).unwrap();
    assert_eq!(downloaded, body);
    assert_eq!(downloaded_meta.size, body.len() as u64);

    let mut prefix_bytes = Vec::new();
    let prefix_meta = store.get_prefix(&key, 4, &mut prefix_bytes).unwrap();
    assert_eq!(prefix_bytes, b"0123");
    assert_eq!(prefix_meta.size, body.len() as u64);

    let page = store.list(&prefix, None).unwrap();
    assert_eq!(page.objects.len(), 1);
    assert_eq!(page.objects[0].key, key);
    // The ObjectStore list contract is recursive (LocalFolderStore::list walks
    // every descendant), so listing the broad `sync` prefix returns the nested
    // object rather than only direct children.
    let root_page = store
        .list(&ObjectPrefix::from_path("sync").unwrap(), None)
        .unwrap();
    assert_eq!(root_page.objects.len(), 1);
    assert_eq!(root_page.objects[0].key, key);

    let mut replacement = &b"replacement"[..];
    let replacement_len = replacement.len() as u64;
    let wrong = store.put(
        &key,
        &mut replacement,
        replacement_len,
        PutCondition::IfMatch("\"wrong\"".to_string()),
    );
    assert_eq!(wrong.unwrap_err().code, DeviceSyncErrorCode::RemoteChanged);

    let current = store.head(&key).unwrap().unwrap();
    let mut replacement = &b"replacement"[..];
    let replacement_len = replacement.len() as u64;
    let updated = store
        .put(
            &key,
            &mut replacement,
            replacement_len,
            PutCondition::IfMatch(current.etag.unwrap()),
        )
        .unwrap();
    store
        .delete(&key, DeleteCondition::IfMatch(updated.etag.unwrap()))
        .unwrap();
    assert!(store.head(&key).unwrap().is_none());

    let methods = server.methods();
    for method in ["MKCOL", "PUT", "HEAD", "GET", "PROPFIND", "DELETE"] {
        assert!(methods.iter().any(|actual| actual == method), "missing {method}");
    }
}

#[test]
fn webdav_list_treats_a_missing_prefix_as_empty_before_the_first_push() {
    let server = MockWebDav::new(RangeMode::Honor);
    let store = store(&server.endpoint);
    let prefix = ObjectPrefix::from_path("sync/devices").unwrap();

    let page = store.list(&prefix, None).unwrap();

    assert!(page.objects.is_empty());
    assert!(page.next_cursor.is_none());
    assert!(server.methods().iter().any(|method| method == "PROPFIND"));
}

#[test]
fn webdav_creates_a_multi_segment_remote_prefix_without_escaping_it() {
    let server = MockWebDav::new(RangeMode::Honor);
    let store = store_with_prefix(&server.endpoint, "team/tokenviewer");
    let key = ObjectKey::from_path("team/tokenviewer/snapshots/one").unwrap();
    let body = b"snapshot";

    store
        .put(
            &key,
            &mut &body[..],
            body.len() as u64,
            PutCondition::IfNoneMatch,
        )
        .unwrap();

    let mut downloaded = Vec::new();
    store.get_bounded(&key, 64, &mut downloaded).unwrap();
    assert_eq!(downloaded, body);
}

#[test]
fn webdav_range_ignore_is_rejected_without_writing_a_prefix() {
    let server = MockWebDav::new(RangeMode::Ignore);
    let store = store(&server.endpoint);
    let key = ObjectKey::from_path("sync/object").unwrap();
    let body = b"0123456789abcdef";
    store
        .put(
            &key,
            &mut &body[..],
            body.len() as u64,
            PutCondition::IfNoneMatch,
        )
        .unwrap();

    let mut sink = Vec::new();
    let error = store.get_prefix(&key, 4, &mut sink).unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::ProtocolUnsupported);
    assert!(error
        .arguments
        .get("detail")
        .is_some_and(|detail| detail.contains("range")));
    assert!(sink.is_empty());
}

#[test]
fn webdav_status_mapping_and_auth_errors_are_structured() {
    let server = MockWebDav::new(RangeMode::Honor);
    let store = store(&server.endpoint);
    let key = ObjectKey::from_path("sync/object").unwrap();

    server.set_reject_auth(true);
    let error = store.head(&key).unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::AuthenticationFailed);
    assert!(!format!("{error:?}").contains("mock password"));
    server.set_reject_auth(false);

    server.force_status("GET", Some(429));
    let error = store.get_bounded(&key, 16, &mut Vec::new()).unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::RateLimited);
    server.force_status("GET", Some(503));
    let error = store.get_bounded(&key, 16, &mut Vec::new()).unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::NetworkUnreachable);
    server.force_status("GET", None);

    let error = store.get_bounded(&key, 16, &mut Vec::new()).unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::VaultNotFound);
}

#[test]
fn webdav_directory_conflicts_do_not_replace_an_object() {
    let server = MockWebDav::new(RangeMode::Honor);
    let store = store(&server.endpoint);
    let key = ObjectKey::from_path("sync/file").unwrap();
    let body = b"original";
    store
        .put(
            &key,
            &mut &body[..],
            body.len() as u64,
            PutCondition::IfNoneMatch,
        )
        .unwrap();

    let prefix = ObjectPrefix::from_path("sync/file/child").unwrap();
    let error = store.create_prefix(&prefix).unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::ImmutableObjectConflict);
    let mut downloaded = Vec::new();
    store.get_bounded(&key, 32, &mut downloaded).unwrap();
    assert_eq!(downloaded, body);
}

#[test]
fn webdav_if_match_rejects_server_without_strong_etag() {
    let server = MockWebDav::new(RangeMode::Honor);
    server.set_suppress_etag(true);
    let store = store(&server.endpoint);
    let key = ObjectKey::from_path("sync/object").unwrap();
    let original = b"original";
    store
        .put(
            &key,
            &mut &original[..],
            original.len() as u64,
            PutCondition::IfNoneMatch,
        )
        .unwrap();

    let current = store.head(&key).unwrap().unwrap();
    assert!(current.etag.as_deref().is_some_and(|etag| etag.starts_with("sha256:")));
    let replacement = b"replacement";
    let error = store
        .put(
            &key,
            &mut &replacement[..],
            replacement.len() as u64,
            PutCondition::IfMatch(current.etag.unwrap()),
        )
        .unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::ProtocolUnsupported);

    let mut downloaded = Vec::new();
    store.get_bounded(&key, 64, &mut downloaded).unwrap();
    assert_eq!(downloaded, original);
}

#[test]
fn webdav_list_returns_nested_objects_matching_local_store_semantics() {
    let server = MockWebDav::new(RangeMode::Honor);
    let store = store(&server.endpoint);
    let devices = ObjectPrefix::from_path("sync/devices").unwrap();
    store.create_prefix(&devices).unwrap();

    let head_key = ObjectKey::from_path("sync/devices/device-a/head.json").unwrap();
    let body = b"{\"head\":true}";
    store
        .put(
            &head_key,
            &mut &body[..],
            body.len() as u64,
            PutCondition::IfNoneMatch,
        )
        .unwrap();

    let page = store.list(&devices, None).unwrap();
    // The engine's remote_view lists the `devices` prefix and expects the
    // nested <device-id>/head.json objects (LocalFolderStore::list walks the
    // whole subtree). A Depth:1 PROPFIND only returns direct children, so a
    // complying WebDAV list must recurse to discover existing heads.
    assert!(
        page.objects.iter().any(|meta| meta.key == head_key),
        "list must return nested head.json objects under the devices prefix"
    );
}

#[test]
fn webdav_list_yields_a_usable_etag_when_propfind_omits_getetag() {
    let server = MockWebDav::new(RangeMode::Honor);
    server.set_suppress_etag(true);
    let store = store(&server.endpoint);
    let devices = ObjectPrefix::from_path("sync/devices").unwrap();
    store.create_prefix(&devices).unwrap();

    let key = ObjectKey::from_path("sync/devices/device-a/head.json").unwrap();
    let body = b"{\"device\":\"a\"}";
    store
        .put(
            &key,
            &mut &body[..],
            body.len() as u64,
            PutCondition::IfNoneMatch,
        )
        .unwrap();

    let page = store.list(&devices, None).unwrap();
    assert_eq!(page.objects.len(), 1);
    // PROPFIND omitted getetag, but list must still yield a stable etag so a
    // later conditional PUT can use If-Match instead of falling back to
    // If-None-Match on an object that already exists (which would 412).
    let listed = &page.objects[0];
    assert!(
        listed.etag.is_some(),
        "PROPFIND without getetag must not produce a None etag"
    );
    assert_eq!(store.head(&key).unwrap().unwrap().etag, listed.etag);
}

#[test]
fn webdav_connection_test_performs_authenticated_write_read_and_delete() {
    let server = MockWebDav::new(RangeMode::Honor);
    let store = store(&server.endpoint);

    let report = store.test_connection().unwrap();
    assert!(report.writable);
    let methods = server.methods();
    for method in ["MKCOL", "PUT", "GET", "DELETE"] {
        assert!(methods.iter().any(|actual| actual == method), "missing {method}");
    }

    server.set_reject_auth(true);
    let error = store.test_connection().unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::AuthenticationFailed);
}

#[test]
fn webdav_connection_test_distinguishes_a_missing_remote_directory_from_a_vault() {
    let server = MockWebDav::new(RangeMode::Honor);
    server.force_status("MKCOL", Some(405));
    server.force_status("PROPFIND", Some(404));
    let store = store(&server.endpoint);

    let error = store.test_connection().unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::RemoteDirectoryUnavailable);
    assert_eq!(
        error.arguments.get("operation").map(String::as_str),
        Some("propfind")
    );
    assert_eq!(
        error.arguments.get("http_status").map(String::as_str),
        Some("404")
    );
}

#[test]
fn webdav_http_requires_explicit_insecure_opt_in() {
    let mut config = DeviceSyncConfig::default();
    config.enabled = true;
    config.profile_id = "profile-http".to_string();
    config.provider = Some(ProviderConfig {
        kind: "webdav".to_string(),
        endpoint: Some("http://dav.example.com/".to_string()),
        remote_prefix: "tokenviewer-sync".to_string(),
        local_root: None,
        username: Some("user".to_string()),
        bucket: None,
        region: None,
        path_style: None,
        insecure: false,
    });

    assert_eq!(
        config.validate().unwrap_err().code,
        DeviceSyncErrorCode::InvalidConfig
    );
    config.provider.as_mut().unwrap().insecure = true;
    config.validate().unwrap();
}
