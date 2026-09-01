use std::collections::BTreeSet;
use std::fmt;
use std::io::{Cursor, Read};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::archive::inspect_archive;
use super::models::{
    valid_environment_name, valid_segment, DeviceSyncError, DeviceSyncErrorCode, Head,
    HeadUnsigned, KdfParameters, KeyWrap, SkillFileKind, SnapshotHeader, SnapshotManifest,
    SnapshotPayload, VaultMetadata, MAX_ARCHIVE_ENTRIES, MAX_COMPRESSION_RATIO,
    MAX_ENCRYPTED_SNAPSHOT_BYTES, MAX_EXPANDED_BYTES, MAX_MANIFEST_BYTES, MAX_MANIFEST_COMPONENTS,
    MAX_MANIFEST_RECORDS, MAX_RELATIVE_PATH_BYTES, MAX_SINGLE_FILE_BYTES, PROTOCOL_VERSION,
    TVSYNC_MAGIC,
};

const ARGON2_MEMORY_KIB: u32 = 65_536;
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_PARALLELISM: u32 = 1;
const ARGON2_OUTPUT_BYTES: usize = 32;
const VAULT_SALT_BYTES: usize = 16;
const SNAPSHOT_NONCE_BYTES: usize = 24;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const PAYLOAD_LENGTH_BYTES: usize = 8;

type HmacSha256 = Hmac<Sha256>;

pub struct SecretString(Zeroizing<Vec<u8>>);

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString(REDACTED)")
    }
}

impl SecretString {
    pub fn as_str(&self) -> Result<&str, DeviceSyncError> {
        std::str::from_utf8(self.0.as_slice())
            .map_err(|_| DeviceSyncError::invalid_config("secret is not UTF-8"))
    }
}

impl<'de> serde::Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Ok(Self(Zeroizing::new(value.into_bytes())))
    }
}

#[derive(PartialEq, Eq)]
pub struct VaultKey(Zeroizing<Vec<u8>>);

impl fmt::Debug for VaultKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VaultKey(REDACTED)")
    }
}

impl VaultKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes.to_vec()))
    }

    pub fn from_vec(bytes: Vec<u8>) -> Result<Self, DeviceSyncError> {
        if bytes.len() != ARGON2_OUTPUT_BYTES {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::IntegrityFailed,
                false,
            ));
        }
        Ok(Self(Zeroizing::new(bytes)))
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

/// Serialize JSON according to the deterministic subset of RFC 8785 used by
/// the protocol. Protocol models contain no floating-point values; rejecting
/// them here prevents two runtimes from choosing different number spellings.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, DeviceSyncError> {
    let value = serde_json::to_value(value)
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
    let mut output = Vec::new();
    write_canonical_value(&value, &mut output)?;
    Ok(output)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{:02x}", byte)).collect()
}

pub fn create_vault_metadata(
    vault_id: &str,
    _device_id: &str,
    password: &str,
) -> Result<VaultMetadata, DeviceSyncError> {
    if !valid_segment(vault_id) {
        return Err(DeviceSyncError::invalid_config("vault_id"));
    }
    if password.is_empty() {
        return Err(DeviceSyncError::invalid_config("password"));
    }
    let mut salt = [0u8; VAULT_SALT_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    let vmk = random_key();
    let wrapping_key = derive_password_key(password, &salt)?;
    let mut nonce = [0u8; SNAPSHOT_NONCE_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let cipher = XChaCha20Poly1305::new_from_slice(wrapping_key.as_slice())
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: vmk.as_slice(),
                aad: &vault_aad(vault_id),
            },
        )
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;

    Ok(VaultMetadata {
        format: "tokenviewer-vault".to_string(),
        protocol_version: PROTOCOL_VERSION,
        vault_id: vault_id.to_string(),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        kdf: KdfParameters {
            name: "argon2id".to_string(),
            version: 19,
            salt_b64: base64::engine::general_purpose::STANDARD.encode(salt),
            memory_kib: ARGON2_MEMORY_KIB,
            iterations: ARGON2_ITERATIONS,
            parallelism: ARGON2_PARALLELISM,
        },
        key_wrap: KeyWrap {
            algorithm: "xchacha20-poly1305".to_string(),
            nonce_b64: base64::engine::general_purpose::STANDARD.encode(nonce),
            ciphertext_b64: base64::engine::general_purpose::STANDARD.encode(ciphertext),
        },
    })
}

pub fn unwrap_vault_key(
    metadata: &VaultMetadata,
    password: &str,
) -> Result<VaultKey, DeviceSyncError> {
    if password.is_empty() {
        return Err(vault_auth_error());
    }
    if metadata.format != "tokenviewer-vault"
        || metadata.protocol_version != PROTOCOL_VERSION
        || !valid_segment(&metadata.vault_id)
        || metadata.kdf.name != "argon2id"
        || metadata.kdf.version != 19
        || metadata.kdf.memory_kib != ARGON2_MEMORY_KIB
        || metadata.kdf.iterations != ARGON2_ITERATIONS
        || metadata.kdf.parallelism != ARGON2_PARALLELISM
        || metadata.key_wrap.algorithm != "xchacha20-poly1305"
    {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ProtocolUnsupported,
            false,
        ));
    }
    let salt = base64::engine::general_purpose::STANDARD
        .decode(&metadata.kdf.salt_b64)
        .map_err(|_| vault_auth_error())?;
    let nonce = base64::engine::general_purpose::STANDARD
        .decode(&metadata.key_wrap.nonce_b64)
        .map_err(|_| vault_auth_error())?;
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(&metadata.key_wrap.ciphertext_b64)
        .map_err(|_| vault_auth_error())?;
    if salt.len() != VAULT_SALT_BYTES || nonce.len() != SNAPSHOT_NONCE_BYTES {
        return Err(vault_auth_error());
    }
    let wrapping_key = derive_password_key(password, &salt).map_err(|_| vault_auth_error())?;
    let cipher = XChaCha20Poly1305::new_from_slice(wrapping_key.as_slice())
        .map_err(|_| vault_auth_error())?;
    let vmk = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: ciphertext.as_slice(),
                aad: &vault_aad(&metadata.vault_id),
            },
        )
        .map_err(|_| vault_auth_error())?;
    VaultKey::from_vec(vmk).map_err(|_| vault_auth_error())
}

pub fn encrypt_snapshot(
    payload: &SnapshotPayload,
    vault_key: &VaultKey,
) -> Result<Vec<u8>, DeviceSyncError> {
    validate_manifest(&payload.manifest, &payload.archive)?;
    let manifest_bytes = canonical_json(&payload.manifest)?;
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES
        || payload.archive.len() as u64 > MAX_EXPANDED_BYTES
    {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ObjectTooLarge,
            false,
        ));
    }
    let plaintext_len = PAYLOAD_LENGTH_BYTES
        .checked_add(manifest_bytes.len())
        .and_then(|value| value.checked_add(payload.archive.len()))
        .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false))?;
    let mut plaintext = Vec::with_capacity(plaintext_len);
    plaintext.extend_from_slice(&(manifest_bytes.len() as u64).to_be_bytes());
    plaintext.extend_from_slice(&manifest_bytes);
    plaintext.extend_from_slice(&payload.archive);
    if plaintext.len() as u64 > max_snapshot_plaintext_bytes() {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ObjectTooLarge,
            false,
        ));
    }
    let payload_hash = sha256_hex(&plaintext);
    let header = SnapshotHeader {
        format: "tokenviewer-tvsync".to_string(),
        protocol_version: PROTOCOL_VERSION,
        vault_id: payload.manifest.vault_id.clone(),
        object_type: "snapshot".to_string(),
        snapshot_id: payload.manifest.snapshot_id.clone(),
        parent_ids: payload.manifest.parent_ids.clone(),
        payload_sha256: payload_hash,
        header_mac_b64: None,
    };
    let mut header = header;
    header.header_mac_b64 = Some(sign_snapshot_header(&header, vault_key)?);
    verify_snapshot_header(&header, vault_key)?;
    let header_bytes = canonical_json(&header)?;
    if header_bytes.len() > MAX_HEADER_BYTES {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ObjectTooLarge,
            false,
        ));
    }
    let compressed = zstd::stream::encode_all(Cursor::new(plaintext.as_slice()), 3)
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;
    validate_compression_ratio(compressed.len() as u64, plaintext.len() as u64)?;
    let mut nonce = [0u8; SNAPSHOT_NONCE_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let snapshot_key = derive_context_key(
        vault_key,
        &format!(
            "tokenviewer/snapshot/v{}/{}/{}",
            PROTOCOL_VERSION, header.vault_id, header.snapshot_id
        ),
    )?;
    let cipher = XChaCha20Poly1305::new_from_slice(snapshot_key.as_slice())
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: compressed.as_slice(),
                aad: &snapshot_aad(&header)?,
            },
        )
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;
    let total_len = TVSYNC_MAGIC
        .len()
        .checked_add(4)
        .and_then(|value| value.checked_add(header_bytes.len()))
        .and_then(|value| value.checked_add(nonce.len()))
        .and_then(|value| value.checked_add(ciphertext.len()))
        .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false))?;
    if total_len as u64 > MAX_ENCRYPTED_SNAPSHOT_BYTES {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ObjectTooLarge,
            false,
        ));
    }
    let mut output = Vec::with_capacity(total_len);
    output.extend_from_slice(TVSYNC_MAGIC);
    output.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
    output.extend_from_slice(&header_bytes);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

pub fn decrypt_snapshot(
    encrypted: &[u8],
    vault_key: &VaultKey,
) -> Result<SnapshotPayload, DeviceSyncError> {
    if encrypted.len() as u64 > MAX_ENCRYPTED_SNAPSHOT_BYTES
        || encrypted.len() < TVSYNC_MAGIC.len() + 4 + SNAPSHOT_NONCE_BYTES + 16
        || &encrypted[..TVSYNC_MAGIC.len()] != TVSYNC_MAGIC
    {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    }
    let (header, mut offset) = parse_snapshot_header(encrypted)?;
    if header.header_mac_b64.is_some() {
        verify_snapshot_header(&header, vault_key)?;
    }
    let header_has_parent_ids = header_parent_ids_present(&encrypted[..offset]);
    let nonce_end = offset
        .checked_add(SNAPSHOT_NONCE_BYTES)
        .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
    if nonce_end > encrypted.len() || encrypted.len() - nonce_end < 16 {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    }
    let nonce = &encrypted[offset..nonce_end];
    offset = nonce_end;
    let ciphertext = &encrypted[offset..];
    let snapshot_key = derive_context_key(
        vault_key,
        &format!(
            "tokenviewer/snapshot/v{}/{}/{}",
            PROTOCOL_VERSION, header.vault_id, header.snapshot_id
        ),
    )?;
    let cipher = XChaCha20Poly1305::new_from_slice(snapshot_key.as_slice())
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;
    let compressed = cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &if header_has_parent_ids {
                    snapshot_aad(&header)?
                } else {
                    legacy_snapshot_aad(&header)?
                },
            },
        )
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::AuthenticationFailed, false))?;
    let compressed_len = compressed.len() as u64;
    let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(compressed.as_slice()))
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::AuthenticationFailed, false))?;
    let mut plaintext = Zeroizing::new(Vec::new());
    let mut buffer = [0u8; 32 * 1024];
    loop {
        let count = decoder
            .read(&mut buffer)
            .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::AuthenticationFailed, false))?;
        if count == 0 {
            break;
        }
        let next_len = (plaintext.len() as u64)
            .checked_add(count as u64)
            .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false))?;
        if next_len > max_snapshot_plaintext_bytes() {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::ObjectTooLarge,
                false,
            ));
        }
        plaintext.extend_from_slice(&buffer[..count]);
        validate_compression_ratio(compressed_len, plaintext.len() as u64)?;
    }
    // An empty zstd stream does not enter the read loop. Validate once more
    // after EOF so compressed_len == 0 and every other boundary use the same
    // protocol rule as the publisher.
    validate_compression_ratio(compressed_len, plaintext.len() as u64)?;
    if sha256_hex(&plaintext) != header.payload_sha256 {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    }
    if plaintext.len() < PAYLOAD_LENGTH_BYTES {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    }
    let manifest_len = u64::from_be_bytes(
        plaintext[..PAYLOAD_LENGTH_BYTES]
            .try_into()
            .expect("manifest length is eight bytes"),
    );
    if manifest_len > MAX_MANIFEST_BYTES {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    }
    let manifest_end = PAYLOAD_LENGTH_BYTES
        .checked_add(manifest_len as usize)
        .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
    if manifest_end > plaintext.len() {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    }
    let manifest_bytes = &plaintext[PAYLOAD_LENGTH_BYTES..manifest_end];
    let raw_manifest: serde_json::Value = serde_json::from_slice(manifest_bytes)
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
    let manifest: SnapshotManifest = serde_json::from_value(raw_manifest.clone())
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
    let canonical_manifest = canonical_json(&manifest).as_deref() == Ok(manifest_bytes);
    let legacy_manifest =
        !canonical_manifest && legacy_manifest_without_clock(&raw_manifest, &manifest)?;
    if !canonical_manifest && !legacy_manifest
        || manifest.protocol_version != PROTOCOL_VERSION
        || manifest.format != "tokenviewer-snapshot"
        || manifest.snapshot_id != header.snapshot_id
        || manifest.vault_id != header.vault_id
        || (header_has_parent_ids && manifest.parent_ids != header.parent_ids)
    {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    }
    let archive = plaintext[manifest_end..].to_vec();
    validate_manifest(&manifest, &archive)?;
    verify_component_hashes(&manifest, &archive)?;
    Ok(SnapshotPayload { manifest, archive })
}

/// Parse and validate the unauthenticated framing header without reading the
/// encrypted payload. The caller must still authenticate a payload before
/// applying any fields from it.
pub fn inspect_snapshot_header(encrypted_prefix: &[u8]) -> Result<SnapshotHeader, DeviceSyncError> {
    parse_snapshot_header(encrypted_prefix).map(|(header, _)| header)
}

/// Authenticate a v1 header before using its parent list for graph
/// traversal. Older headers do not carry a MAC and must be fully decrypted by
/// the caller instead.
pub fn authenticate_snapshot_header(
    encrypted_prefix: &[u8],
    vault_key: &VaultKey,
) -> Result<SnapshotHeader, DeviceSyncError> {
    let (header, _) = parse_snapshot_header(encrypted_prefix).map_err(|_| {
        DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false)
    })?;
    verify_snapshot_header(&header, vault_key)?;
    Ok(header)
}

fn parse_snapshot_header(encrypted: &[u8]) -> Result<(SnapshotHeader, usize), DeviceSyncError> {
    let invalid = || DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false);
    if encrypted.len() < TVSYNC_MAGIC.len() + 4 || &encrypted[..TVSYNC_MAGIC.len()] != TVSYNC_MAGIC
    {
        return Err(invalid());
    }
    let length_start = TVSYNC_MAGIC.len();
    let header_len = u32::from_be_bytes(
        encrypted[length_start..length_start + 4]
            .try_into()
            .expect("header length is four bytes"),
    ) as usize;
    if header_len == 0 || header_len > MAX_HEADER_BYTES {
        return Err(invalid());
    }
    let header_start = length_start + 4;
    let header_end = header_start.checked_add(header_len).ok_or_else(invalid)?;
    if header_end > encrypted.len() {
        return Err(invalid());
    }
    let header_bytes = &encrypted[header_start..header_end];
    let header: SnapshotHeader = serde_json::from_slice(header_bytes).map_err(|_| invalid())?;
    let canonical_header = canonical_json(&header)?;
    if canonical_header.as_slice() != header_bytes {
        // v1 snapshots written before parent_ids was added are still valid;
        // accept their canonical representation only when that field is the
        // sole missing field.
        let raw: serde_json::Value = serde_json::from_slice(header_bytes).map_err(|_| invalid())?;
        if raw
            .as_object()
            .is_none_or(|object| object.contains_key("parent_ids"))
            || canonical_json(&raw)?.as_slice() != header_bytes
        {
            return Err(invalid());
        }
    }
    if header.format != "tokenviewer-tvsync"
        || header.object_type != "snapshot"
        || header.protocol_version != PROTOCOL_VERSION
        || !valid_segment(&header.vault_id)
        || !valid_segment(&header.snapshot_id)
        || !is_hash(&header.payload_sha256)
        || header.parent_ids.len() > 2
        || header
            .parent_ids
            .iter()
            .any(|parent| !valid_segment(parent) || parent == &header.snapshot_id)
        || header.parent_ids.iter().collect::<BTreeSet<_>>().len() != header.parent_ids.len()
    {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ProtocolUnsupported,
            false,
        ));
    }
    Ok((header, header_end))
}

fn header_parent_ids_present(encrypted: &[u8]) -> bool {
    let Some(length_bytes) = encrypted.get(TVSYNC_MAGIC.len()..TVSYNC_MAGIC.len() + 4) else {
        return false;
    };
    let Ok(length_bytes) = <[u8; 4]>::try_from(length_bytes) else {
        return false;
    };
    let header_len = u32::from_be_bytes(length_bytes) as usize;
    let start = TVSYNC_MAGIC.len() + 4;
    let end = start.checked_add(header_len).unwrap_or(usize::MAX);
    if end > encrypted.len() {
        return false;
    }
    serde_json::from_slice::<serde_json::Value>(&encrypted[start..end])
        .ok()
        .and_then(|value| value.get("parent_ids").cloned())
        .is_some()
}

/// Return whether the v1 framing header explicitly carried `parent_ids`.
/// Older v1 objects are decoded with an empty vector, so graph traversal must
/// authenticate their payload before it can discover the real parents.
pub fn snapshot_header_parent_ids_present(encrypted: &[u8]) -> bool {
    header_parent_ids_present(encrypted)
}

fn legacy_manifest_without_clock(
    raw_manifest: &serde_json::Value,
    manifest: &SnapshotManifest,
) -> Result<bool, DeviceSyncError> {
    let Some(raw_object) = raw_manifest.as_object() else {
        return Ok(false);
    };
    if raw_object.contains_key("clock") {
        return Ok(false);
    }
    let mut expected = serde_json::to_value(manifest)
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
    let Some(expected_object) = expected.as_object_mut() else {
        return Ok(false);
    };
    expected_object.remove("clock");
    let expected = serde_json::Value::Object(expected_object.clone());
    Ok(canonical_json(raw_manifest)?.as_slice() == canonical_json(&expected)?.as_slice())
}

fn snapshot_header_mac_input(header: &SnapshotHeader) -> Result<Vec<u8>, DeviceSyncError> {
    canonical_json(&serde_json::json!({
        "magic_b64": base64::engine::general_purpose::STANDARD.encode(TVSYNC_MAGIC),
        "format": header.format,
        "protocol_version": header.protocol_version,
        "vault_id": header.vault_id,
        "object_type": header.object_type,
        "snapshot_id": header.snapshot_id,
        "parent_ids": header.parent_ids,
        "payload_sha256": header.payload_sha256,
    }))
}

pub(crate) fn sign_snapshot_header(
    header: &SnapshotHeader,
    vault_key: &VaultKey,
) -> Result<String, DeviceSyncError> {
    let mac_key = derive_context_key(vault_key, "tokenviewer/snapshot-header/v1")?;
    let canonical = snapshot_header_mac_input(header)?;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(mac_key.as_slice())
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;
    mac.update(&canonical);
    Ok(base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()))
}

fn verify_snapshot_header(
    header: &SnapshotHeader,
    vault_key: &VaultKey,
) -> Result<(), DeviceSyncError> {
    let Some(encoded) = header.header_mac_b64.as_deref() else {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    };
    let actual = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
    if actual.len() != 32 {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    }
    let expected = base64::engine::general_purpose::STANDARD
        .decode(sign_snapshot_header(header, vault_key)?)
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
    if !constant_time_eq(&actual, &expected) {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::IntegrityFailed,
            false,
        ));
    }
    Ok(())
}

fn max_snapshot_plaintext_bytes() -> u64 {
    MAX_EXPANDED_BYTES
        .checked_add(MAX_MANIFEST_BYTES)
        .and_then(|value| value.checked_add(PAYLOAD_LENGTH_BYTES as u64))
        .unwrap_or(u64::MAX)
}

/// The ratio applies to the complete framed plaintext, including its manifest
/// length prefix. Keeping the rule identical on both sides prevents an object
/// accepted by the publisher from being rejected by the reader.
fn validate_compression_ratio(
    compressed_len: u64,
    expanded_len: u64,
) -> Result<(), DeviceSyncError> {
    let maximum_expanded = compressed_len
        .checked_mul(MAX_COMPRESSION_RATIO)
        .ok_or_else(|| DeviceSyncError::new(DeviceSyncErrorCode::ObjectTooLarge, false))?;
    if compressed_len == 0 || expanded_len > maximum_expanded {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::ObjectTooLarge,
            false,
        ));
    }
    Ok(())
}

fn verify_component_hashes(
    manifest: &SnapshotManifest,
    archive: &[u8],
) -> Result<(), DeviceSyncError> {
    for (name, summary) in &manifest.components {
        let (byte_len, digest) = match name.as_str() {
            "skills" => (archive.len() as u64, sha256_hex(archive)),
            "agent_links" => {
                let bytes = canonical_json(&manifest.records.agent_links)?;
                (bytes.len() as u64, sha256_hex(&bytes))
            }
            "skill_env" => {
                let bytes = canonical_json(&manifest.records.skill_env)?;
                (bytes.len() as u64, sha256_hex(&bytes))
            }
            "preferences" => {
                let bytes = canonical_json(&manifest.records.preferences)?;
                (bytes.len() as u64, sha256_hex(&bytes))
            }
            // Unknown component names remain forward-compatible. The whole
            // manifest is authenticated, and older clients simply cannot
            // apply a component they do not understand.
            _ => continue,
        };
        if !is_hash(&summary.sha256) || summary.bytes != byte_len || digest != summary.sha256 {
            return Err(DeviceSyncError::new(
                DeviceSyncErrorCode::IntegrityFailed,
                false,
            ));
        }
    }
    Ok(())
}

fn validate_manifest(manifest: &SnapshotManifest, archive: &[u8]) -> Result<(), DeviceSyncError> {
    let archive_bytes = archive.len() as u64;
    let invalid = || DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false);
    if manifest.format != "tokenviewer-snapshot"
        || manifest.protocol_version != PROTOCOL_VERSION
        || !valid_segment(&manifest.snapshot_id)
        || !valid_segment(&manifest.vault_id)
        || !valid_segment(&manifest.device_id)
        || (manifest.content_source != "cloud" && manifest.content_source != "git")
        || manifest.parent_ids.len() > 2
        || manifest
            .parent_ids
            .iter()
            .any(|parent| !valid_segment(parent) || parent == &manifest.snapshot_id)
        || manifest.parent_ids.iter().collect::<BTreeSet<_>>().len() != manifest.parent_ids.len()
        || manifest.limits.archive_bytes != archive_bytes
        || manifest.limits.archive_bytes > MAX_EXPANDED_BYTES
        || manifest.limits.expanded_bytes > MAX_EXPANDED_BYTES
        || (manifest.content_source == "git" && archive_bytes != 0)
        || (manifest.content_source == "cloud" && manifest.git_repository.is_some())
        || manifest.components.len() > MAX_MANIFEST_COMPONENTS
    {
        return Err(invalid());
    }

    let total_records = manifest
        .records
        .skills
        .len()
        .saturating_add(manifest.records.agent_links.len())
        .saturating_add(manifest.records.skill_env.len())
        .saturating_add(usize::from(manifest.records.preferences.is_some()))
        .saturating_add(manifest.tombstones.len());
    if total_records > MAX_MANIFEST_RECORDS {
        return Err(invalid());
    }

    let known_components = ["skills", "agent_links", "skill_env", "preferences"];
    for (name, summary) in &manifest.components {
        if name.is_empty()
            || name.len() > MAX_RELATIVE_PATH_BYTES
            || summary.bytes > MAX_MANIFEST_BYTES
            || summary.entries > MAX_ARCHIVE_ENTRIES
            || summary.records > MAX_MANIFEST_RECORDS as u64
        {
            return Err(invalid());
        }
        if known_components.contains(&name.as_str()) {
            if !is_hash(&summary.sha256) {
                return Err(invalid());
            }
        }
    }
    for name in &known_components {
        let expected_records = match *name {
            "skills" => manifest.records.skills.len() as u64,
            "agent_links" => manifest.records.agent_links.len() as u64,
            "skill_env" => manifest.records.skill_env.len() as u64,
            "preferences" => u64::from(manifest.records.preferences.is_some()),
            _ => unreachable!(),
        };
        let has_records = expected_records > 0;
        match manifest.components.get(*name) {
            Some(summary) if summary.records == expected_records => {}
            Some(_) => return Err(invalid()),
            None if has_records => return Err(invalid()),
            None => {}
        }
    }
    if !manifest.components.contains_key("skills") && archive_bytes != 0 {
        return Err(invalid());
    }
    if manifest
        .components
        .get("skills")
        .is_some_and(|summary| summary.entries > MAX_ARCHIVE_ENTRIES)
    {
        return Err(invalid());
    }

    if let Some(summary) = manifest.components.get("skills") {
        let inspection = if archive_bytes == 0 {
            super::archive::ArchiveInspection {
                records: Vec::new(),
                expanded_bytes: 0,
            }
        } else {
            inspect_archive(archive)?
        };
        let mut expected_files = manifest
            .records
            .skills
            .iter()
            .filter(|record| !record.metadata.tombstone)
            .flat_map(|record| record.files.iter().cloned())
            .collect::<Vec<_>>();
        let mut actual_files = inspection.records;
        expected_files.sort_by(|left, right| left.path.cmp(&right.path));
        actual_files.sort_by(|left, right| left.path.cmp(&right.path));
        if expected_files != actual_files
            || summary.entries != actual_files.len() as u64
            || manifest.limits.expanded_bytes != inspection.expanded_bytes
        {
            return Err(invalid());
        }
    } else if manifest.limits.expanded_bytes != 0 {
        return Err(invalid());
    }

    let mut skill_ids = BTreeSet::new();
    for record in &manifest.records.skills {
        if !valid_segment(&record.skill_id)
            || !validate_record_metadata(&record.metadata)
            || !skill_ids.insert(record.skill_id.clone())
            || (record.metadata.tombstone && !record.files.is_empty())
            || (!record.metadata.tombstone && record.files.is_empty())
            || record.files.len() as u64 > MAX_ARCHIVE_ENTRIES
        {
            return Err(invalid());
        }
        let mut paths = BTreeSet::new();
        for file in &record.files {
            if !validate_skill_file(file, &record.skill_id)
                || !paths.insert(file.path.to_lowercase())
            {
                return Err(invalid());
            }
        }
    }

    let mut link_ids = BTreeSet::new();
    for record in &manifest.records.agent_links {
        if !valid_segment(&record.agent_id)
            || !valid_segment(&record.skill_id)
            || !validate_record_metadata(&record.metadata)
            || !link_ids.insert((record.agent_id.clone(), record.skill_id.clone()))
        {
            return Err(invalid());
        }
    }

    let mut environment_names = BTreeSet::new();
    for record in &manifest.records.skill_env {
        if !valid_environment_name(&record.name)
            || !validate_record_metadata(&record.metadata)
            || !environment_names.insert(record.name.clone())
            || record.value.as_bytes().len() > super::models::MAX_ENVIRONMENT_VALUE_BYTES
            || record.value.contains('\0')
            || record.referenced_by_skill_ids.len() > MAX_ARCHIVE_ENTRIES as usize
            || record
                .referenced_by_skill_ids
                .iter()
                .any(|skill_id| !valid_segment(skill_id))
            || record
                .referenced_by_skill_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != record.referenced_by_skill_ids.len()
        {
            return Err(invalid());
        }
    }

    if let Some(record) = &manifest.records.preferences {
        if !validate_record_metadata(&record.metadata) {
            return Err(invalid());
        }
        let mut enabled_ids = BTreeSet::new();
        if record
            .enabled_agent_ids
            .iter()
            .any(|agent_id| !valid_segment(agent_id) || !enabled_ids.insert(agent_id))
        {
            return Err(invalid());
        }
    }

    let mut tombstone_ids = BTreeSet::new();
    for tombstone in &manifest.tombstones {
        if !known_components.contains(&tombstone.component.as_str())
            || !valid_segment(&tombstone.record_id)
            || !validate_hlc(&tombstone.hlc)
            || !valid_segment(&tombstone.last_modified_by)
            || !tombstone_ids.insert((tombstone.component.clone(), tombstone.record_id.clone()))
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn validate_record_metadata(metadata: &super::models::RecordMetadata) -> bool {
    valid_segment(&metadata.record_id)
        && validate_hlc(&metadata.hlc)
        && valid_segment(&metadata.last_modified_by)
}

fn validate_hlc(hlc: &super::models::Hlc) -> bool {
    hlc.wall_ms >= 0 && valid_segment(&hlc.device_id)
}

fn validate_skill_file(file: &super::models::SkillFileRecord, skill_id: &str) -> bool {
    let root = format!("skills/{}", skill_id);
    let prefix = format!("{}/", root);
    let is_root = file.path == root;
    if file.path.len() > MAX_RELATIVE_PATH_BYTES
        || file.path.contains('\\')
        || file.path.chars().any(char::is_control)
        || (!is_root && !file.path.starts_with(&prefix))
        || (is_root && !matches!(file.kind, SkillFileKind::Directory))
        || file
            .path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return false;
    }
    match file.kind {
        SkillFileKind::File => {
            file.size <= MAX_SINGLE_FILE_BYTES
                && file.link_target.is_none()
                && is_hash(&file.sha256)
        }
        SkillFileKind::Directory => {
            file.size == 0 && file.sha256.is_empty() && file.link_target.is_none()
        }
        SkillFileKind::Symlink => {
            file.size == 0
                && file.sha256.is_empty()
                && file
                    .link_target
                    .as_deref()
                    .is_some_and(|target| validate_link_target(&file.path, target))
        }
    }
}

fn validate_link_target(path: &str, target: &str) -> bool {
    if target.is_empty()
        || target.len() > MAX_RELATIVE_PATH_BYTES
        || target.starts_with('/')
        || target.contains('\0')
        || target.contains('\\')
        || target.chars().any(char::is_control)
    {
        return false;
    }
    let mut stack = path.split('/').map(str::to_string).collect::<Vec<_>>();
    if stack.pop().is_none() || stack.len() < 2 {
        return false;
    }
    for part in target.split('/') {
        match part {
            "" => return false,
            "." => {}
            ".." if stack.len() > 2 => {
                stack.pop();
            }
            ".." => return false,
            value => stack.push(value.to_string()),
        }
    }
    stack.first().map(String::as_str) == Some("skills")
        && stack.get(1).is_some_and(|skill_id| valid_segment(skill_id))
}

fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn sign_head(unsigned: &HeadUnsigned, vault_key: &VaultKey) -> Result<Head, DeviceSyncError> {
    let mac_key = derive_context_key(vault_key, "tokenviewer/head/v1")?;
    let canonical = canonical_json(unsigned)?;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(mac_key.as_slice())
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;
    mac.update(&canonical);
    let mac_b64 = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    Ok(Head {
        format: unsigned.format.clone(),
        protocol_version: unsigned.protocol_version,
        vault_id: unsigned.vault_id.clone(),
        device_id: unsigned.device_id.clone(),
        snapshot_id: unsigned.snapshot_id.clone(),
        parent_ids: unsigned.parent_ids.clone(),
        updated_at: unsigned.updated_at.clone(),
        snapshot_sha256: unsigned.snapshot_sha256.clone(),
        sequence: unsigned.sequence,
        mac_b64,
    })
}

pub fn verify_head(head: &Head, vault_key: &VaultKey) -> Result<(), DeviceSyncError> {
    let expected = sign_head(&head.unsigned(), vault_key)?;
    let actual = base64::engine::general_purpose::STANDARD
        .decode(&head.mac_b64)
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::AuthenticationFailed, false))?;
    let expected = base64::engine::general_purpose::STANDARD
        .decode(expected.mac_b64)
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;
    if actual.len() != expected.len() || !constant_time_eq(&actual, &expected) {
        return Err(DeviceSyncError::new(
            DeviceSyncErrorCode::AuthenticationFailed,
            false,
        ));
    }
    Ok(())
}

fn derive_password_key(password: &str, salt: &[u8]) -> Result<Zeroizing<Vec<u8>>, DeviceSyncError> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        Some(ARGON2_OUTPUT_BYTES),
    )
    .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let password_bytes = Zeroizing::new(password.as_bytes().to_vec());
    let mut output = Zeroizing::new(vec![0u8; ARGON2_OUTPUT_BYTES]);
    argon
        .hash_password_into(password_bytes.as_slice(), salt, output.as_mut_slice())
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;
    Ok(output)
}

fn derive_context_key(
    vault_key: &VaultKey,
    context: &str,
) -> Result<Zeroizing<Vec<u8>>, DeviceSyncError> {
    let hkdf = Hkdf::<Sha256>::new(None, vault_key.as_bytes());
    let mut output = Zeroizing::new(vec![0u8; ARGON2_OUTPUT_BYTES]);
    hkdf.expand(context.as_bytes(), output.as_mut_slice())
        .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::InternalError, false))?;
    Ok(output)
}

fn random_key() -> Zeroizing<Vec<u8>> {
    let mut key = Zeroizing::new(vec![0u8; ARGON2_OUTPUT_BYTES]);
    rand::rngs::OsRng.fill_bytes(key.as_mut_slice());
    key
}

fn vault_aad(vault_id: &str) -> Vec<u8> {
    format!("tokenviewer/vault/v{}/{}", PROTOCOL_VERSION, vault_id).into_bytes()
}

fn snapshot_aad(header: &SnapshotHeader) -> Result<Vec<u8>, DeviceSyncError> {
    canonical_json(&serde_json::json!({
        "magic": String::from_utf8_lossy(TVSYNC_MAGIC),
        "protocol_version": header.protocol_version,
        "vault_id": header.vault_id,
        "object_type": header.object_type,
        "snapshot_id": header.snapshot_id,
        "parent_ids": header.parent_ids,
    }))
}

fn legacy_snapshot_aad(header: &SnapshotHeader) -> Result<Vec<u8>, DeviceSyncError> {
    canonical_json(&serde_json::json!({
        "magic": String::from_utf8_lossy(TVSYNC_MAGIC),
        "protocol_version": header.protocol_version,
        "vault_id": header.vault_id,
        "object_type": header.object_type,
        "snapshot_id": header.snapshot_id,
    }))
}

fn vault_auth_error() -> DeviceSyncError {
    DeviceSyncError::new(DeviceSyncErrorCode::VaultAuthFailed, false)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= left.get(index).copied().unwrap_or(0) as usize
            ^ right.get(index).copied().unwrap_or(0) as usize;
    }
    difference == 0
}

fn write_canonical_value(
    value: &serde_json::Value,
    output: &mut Vec<u8>,
) -> Result<(), DeviceSyncError> {
    match value {
        serde_json::Value::Null => output.extend_from_slice(b"null"),
        serde_json::Value::Bool(value) => {
            output.extend_from_slice(if *value { b"true" } else { b"false" })
        }
        serde_json::Value::Number(number) => {
            if !number.is_i64() && !number.is_u64() {
                return Err(DeviceSyncError::invalid_config("floating-point value"));
            }
            output.extend_from_slice(number.to_string().as_bytes());
        }
        serde_json::Value::String(value) => {
            let encoded = serde_json::to_string(value)
                .map_err(|_| DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false))?;
            output.extend_from_slice(encoded.as_bytes());
        }
        serde_json::Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                write_canonical_value(value, output)?;
            }
            output.push(b']');
        }
        serde_json::Value::Object(values) => {
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort_by(|left, right| {
                left.encode_utf16()
                    .cmp(right.encode_utf16())
                    .then_with(|| left.cmp(right))
            });
            output.push(b'{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                let encoded = serde_json::to_string(key).map_err(|_| {
                    DeviceSyncError::new(DeviceSyncErrorCode::IntegrityFailed, false)
                })?;
                output.extend_from_slice(encoded.as_bytes());
                output.push(b':');
                write_canonical_value(&values[*key], output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::device_sync::models::Hlc;

    #[test]
    fn generated_vault_metadata_round_trips() {
        let metadata = create_vault_metadata("vault", "device", "password").unwrap();
        let key = unwrap_vault_key(&metadata, "password").unwrap();
        assert_eq!(key.as_bytes().len(), 32);
        assert_eq!(
            unwrap_vault_key(&metadata, "different").unwrap_err().code,
            DeviceSyncErrorCode::VaultAuthFailed
        );
    }

    #[test]
    fn empty_vault_password_is_rejected() {
        assert_eq!(
            create_vault_metadata("vault", "device", "")
                .unwrap_err()
                .code,
            DeviceSyncErrorCode::InvalidConfig
        );
        let metadata = create_vault_metadata("vault", "device", "password").unwrap();
        assert_eq!(
            unwrap_vault_key(&metadata, "").unwrap_err().code,
            DeviceSyncErrorCode::VaultAuthFailed
        );
    }

    #[test]
    fn reads_a_v1_snapshot_without_parent_ids_or_clock_fields() {
        let key = VaultKey::from_bytes([9; 32]);
        let mut manifest = SnapshotManifest::new(
            "snapshot-v1".to_string(),
            "vault-v1".to_string(),
            "device-v1".to_string(),
            vec!["parent-v1".to_string()],
            "cloud".to_string(),
            BTreeMap::new(),
        );
        manifest.limits.archive_bytes = 0;
        manifest.limits.expanded_bytes = 0;
        let mut raw_manifest = serde_json::to_value(&manifest).unwrap();
        raw_manifest.as_object_mut().unwrap().remove("clock");
        let manifest_bytes = canonical_json(&raw_manifest).unwrap();
        let mut plaintext = Vec::with_capacity(PAYLOAD_LENGTH_BYTES + manifest_bytes.len());
        plaintext.extend_from_slice(&(manifest_bytes.len() as u64).to_be_bytes());
        plaintext.extend_from_slice(&manifest_bytes);

        let header = SnapshotHeader {
            format: "tokenviewer-tvsync".to_string(),
            protocol_version: PROTOCOL_VERSION,
            vault_id: "vault-v1".to_string(),
            object_type: "snapshot".to_string(),
            snapshot_id: "snapshot-v1".to_string(),
            parent_ids: Vec::new(),
            payload_sha256: sha256_hex(&plaintext),
            header_mac_b64: None,
        };
        let mut raw_header = serde_json::to_value(&header).unwrap();
        raw_header.as_object_mut().unwrap().remove("parent_ids");
        let header_bytes = canonical_json(&raw_header).unwrap();
        let compressed = zstd::stream::encode_all(Cursor::new(&plaintext), 3).unwrap();
        let mut nonce = [0u8; SNAPSHOT_NONCE_BYTES];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let snapshot_key =
            derive_context_key(&key, "tokenviewer/snapshot/v1/vault-v1/snapshot-v1").unwrap();
        let cipher = XChaCha20Poly1305::new_from_slice(snapshot_key.as_slice()).unwrap();
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &compressed,
                    aad: &legacy_snapshot_aad(&header).unwrap(),
                },
            )
            .unwrap();
        let mut fixture = Vec::new();
        fixture.extend_from_slice(TVSYNC_MAGIC);
        fixture.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
        fixture.extend_from_slice(&header_bytes);
        fixture.extend_from_slice(&nonce);
        fixture.extend_from_slice(&ciphertext);

        let decoded = decrypt_snapshot(&fixture, &key).unwrap();
        assert_eq!(decoded.manifest.snapshot_id, "snapshot-v1");
        assert_eq!(decoded.manifest.parent_ids, vec!["parent-v1"]);
        assert_eq!(decoded.manifest.clock, Hlc::default());
        assert!(decoded.archive.is_empty());
    }
}
