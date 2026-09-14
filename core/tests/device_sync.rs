use std::fs;
use std::io::Cursor;

use tar::{Builder, EntryType, Header};
use tempfile::TempDir;
use tokenviewer_core::device_sync::archive::{
    build_skill_archive, inspect_archive, unpack_archive,
};
use tokenviewer_core::device_sync::config::{load_state, save_state, DeviceSyncPaths};
use tokenviewer_core::device_sync::crypto::{
    authenticate_snapshot_header, canonical_json, decrypt_snapshot, encrypt_snapshot,
    sha256_hex, unwrap_vault_key, VaultKey,
};
use tokenviewer_core::device_sync::models::{
    AgentLinkRecord, ComponentSummary, DeviceSyncConfig, DeviceSyncState, Hlc, RecordMetadata,
    ScopeMode, SkillFileKind, SkillFileRecord, SkillRecord, SkillScope, SnapshotManifest,
    SnapshotPayload, SyncComponent, CONFIG_SCHEMA_VERSION, MAX_COMPRESSION_RATIO, TVSYNC_MAGIC,
    MAX_ENCRYPTED_SNAPSHOT_BYTES, MAX_SINGLE_FILE_BYTES,
};
use tokenviewer_core::device_sync::store::{
    LocalFolderStore, ObjectKey, ObjectStore, PutCondition,
};
use tokenviewer_core::device_sync::snapshot::{build_snapshot_with_clock, SnapshotBuildRequest};
use tokenviewer_core::device_sync::{create_vault_metadata, DeviceSyncEngine, DeviceSyncErrorCode};
use tokenviewer_core::skills::SkillsCore;
use tokenviewer_core::storage::Database;

#[test]
fn hlc_remains_monotonic_across_clock_regression_and_restart() {
    let zero = Hlc::zero("device-a");
    let first = Hlc::next_after(&zero, 1_000, "device-a").unwrap();
    let same_millisecond = Hlc::next_after(&first, 1_000, "device-a").unwrap();
    let clock_regressed = Hlc::next_after(&same_millisecond, 999, "device-a").unwrap();
    assert!(zero < first);
    assert!(first < same_millisecond);
    assert!(same_millisecond < clock_regressed);

    let dir = TempDir::new().unwrap();
    let paths = DeviceSyncPaths::new(dir.path());
    let state = DeviceSyncState {
        schema_version: CONFIG_SCHEMA_VERSION,
        clock: clock_regressed.clone(),
        ..DeviceSyncState::default()
    };
    save_state(&paths, &state).unwrap();
    let restored = load_state(&paths).unwrap();
    let after_restart = Hlc::next_after(&restored.clock, 1, "device-a").unwrap();
    assert!(clock_regressed < after_restart);
}

#[test]
fn hlc_receive_merges_remote_time_without_adopting_remote_identity() {
    let local = Hlc {
        wall_ms: 1_000,
        counter: 2,
        device_id: "device-local".to_string(),
    };
    let remote = Hlc {
        wall_ms: 1_000,
        counter: 9,
        device_id: "device-remote".to_string(),
    };

    let observed = Hlc::observe(&local, Some(&remote), "device-local");
    assert_eq!(observed.device_id, "device-local");
    assert_eq!(observed.wall_ms, 1_000);
    assert_eq!(observed.counter, 9);

    let next = Hlc::receive_after(&observed, Some(&remote), 1_000, "device-local").unwrap();
    assert_eq!(next.device_id, "device-local");
    assert!(next > remote);

    let future = Hlc {
        wall_ms: 2_000,
        counter: 4,
        device_id: "device-remote".to_string(),
    };
    let after_future = Hlc::receive_after(&observed, Some(&future), 1, "device-local").unwrap();
    assert_eq!(after_future.device_id, "device-local");
    assert!(after_future > future);
}

#[test]
fn hlc_receive_counter_overflow_advances_wall_time() {
    let local = Hlc {
        wall_ms: 10,
        counter: u64::MAX,
        device_id: "device-local".to_string(),
    };
    let remote = Hlc {
        wall_ms: 10,
        counter: u64::MAX,
        device_id: "device-remote".to_string(),
    };
    let next = Hlc::receive_after(&local, Some(&remote), 10, "device-local").unwrap();
    assert_eq!(next.wall_ms, 11);
    assert_eq!(next.counter, 0);
    assert_eq!(next.device_id, "device-local");
    assert!(next > local);
    assert!(next > remote);
}

#[test]
fn persisted_remote_clock_identity_is_repaired_on_engine_start() {
    let dir = TempDir::new().unwrap();
    let source_root = dir.path().join("skills");
    fs::create_dir_all(&source_root).unwrap();
    let first = DeviceSyncEngine::new(dir.path().to_path_buf(), source_root.clone()).unwrap();
    let identity = first.identity();
    let paths = DeviceSyncPaths::new(dir.path());
    save_state(
        &paths,
        &DeviceSyncState {
            schema_version: CONFIG_SCHEMA_VERSION,
            clock: Hlc {
                wall_ms: 42,
                counter: 7,
                device_id: "device-remote".to_string(),
            },
            ..DeviceSyncState::default()
        },
    )
    .unwrap();

    let restarted = DeviceSyncEngine::new(dir.path().to_path_buf(), source_root).unwrap();
    assert_eq!(restarted.state().clock.device_id, identity.device_id);
    assert_eq!(restarted.state().clock.wall_ms, 42);
    assert_eq!(restarted.state().clock.counter, 7);
}

#[test]
fn remote_frontier_clock_is_observed_without_adopting_remote_identity() {
    let remote = TempDir::new().unwrap();
    let home_a = TempDir::new().unwrap();
    let home_b = TempDir::new().unwrap();
    let source_a = home_a.path().join(".tokenviewer/skills");
    let source_b = home_b.path().join(".tokenviewer/skills");
    fs::create_dir_all(&source_a).unwrap();
    fs::create_dir_all(&source_b).unwrap();

    let mut config_a = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
    config_a.enabled = true;
    config_a.components = vec![SyncComponent::Preferences];
    let mut config_b = config_a.clone();
    config_b.profile_id = "profile-b".to_string();
    let mut engine_a =
        DeviceSyncEngine::for_test(home_a.path(), source_a.clone(), config_a).unwrap();
    let db_a = Database::open(&home_a.path().join(".tokenviewer/data.db")).unwrap();
    let skills_a = SkillsCore::new(
        &db_a,
        source_a.clone(),
        home_a.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();

    engine_a.create_vault("remote-clock-password").unwrap();
    let identity_a = engine_a.identity();
    let paths_a = DeviceSyncPaths::new(home_a.path());
    save_state(
        &paths_a,
        &DeviceSyncState {
            schema_version: CONFIG_SCHEMA_VERSION,
            clock: Hlc {
                wall_ms: 9_000_000_000_000,
                counter: 7,
                device_id: identity_a.device_id.clone(),
            },
            ..engine_a.state()
        },
    )
    .unwrap();
    drop(engine_a);

    let mut engine_a =
        DeviceSyncEngine::new(home_a.path().to_path_buf(), source_a.clone()).unwrap();
    engine_a.join_vault("remote-clock-password").unwrap();
    let push = engine_a.preview_push(&skills_a, &[], false).unwrap();
    engine_a.push(&skills_a, &[], &push.preview_token, false).unwrap();
    let remote_clock = engine_a.state().clock;

    let mut engine_b =
        DeviceSyncEngine::for_test(home_b.path(), source_b.clone(), config_b).unwrap();
    let db_b = Database::open(&home_b.path().join(".tokenviewer/data.db")).unwrap();
    let mut skills_b = SkillsCore::new(
        &db_b,
        source_b,
        home_b.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();
    engine_b.join_vault("remote-clock-password").unwrap();

    let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
    let observed = engine_b.state().clock;
    assert_eq!(observed.device_id, engine_b.identity().device_id);
    assert_eq!(observed.wall_ms, remote_clock.wall_ms);
    assert_eq!(observed.counter, remote_clock.counter);

    let transaction = engine_b.prepare_apply(&skills_b, &pull.preview_token).unwrap();
    engine_b
        .commit_apply(&mut skills_b, &transaction.transaction_id)
        .unwrap();
    let next = engine_b.preview_push(&skills_b, &[], false).unwrap();
    let next_clock = engine_b.state().clock;
    assert_eq!(next_clock.device_id, engine_b.identity().device_id);
    assert_eq!(next_clock.wall_ms, remote_clock.wall_ms);
    assert!(next_clock.counter > remote_clock.counter);
    engine_b.push(&skills_b, &[], &next.preview_token, false).unwrap();
}

#[test]
fn canonical_json_sorts_keys_and_rejects_floating_point_values() {
    let value = serde_json::json!({"z": 1, "a": {"b": 2, "a": 1}});
    assert_eq!(
        canonical_json(&value).unwrap(),
        br#"{"a":{"a":1,"b":2},"z":1}"#
    );
    assert!(canonical_json(&serde_json::json!({"value": 1.0})).is_err());
}

#[test]
fn archive_is_deterministic_keeps_allowed_hidden_files_and_skips_external_links() {
    let dir = TempDir::new().unwrap();
    let skill = dir.path().join("alpha");
    fs::create_dir_all(skill.join(".claude")).unwrap();
    fs::create_dir_all(skill.join(".git")).unwrap();
    fs::write(skill.join("SKILL.md"), "# Alpha\n").unwrap();
    fs::write(skill.join(".claude/config"), "keep").unwrap();
    fs::write(skill.join(".git/secret"), "skip").unwrap();

    let external_file = dir.path().join("outside.txt");
    fs::write(&external_file, "outside").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&external_file, skill.join("external-link")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&external_file, skill.join("external-link")).unwrap();

    let first = build_skill_archive(dir.path(), &["alpha".to_string()]).unwrap();
    let second = build_skill_archive(dir.path(), &["alpha".to_string()]).unwrap();
    assert_eq!(first.bytes, second.bytes);
    assert!(first
        .records
        .iter()
        .any(|record| record.path.ends_with(".claude/config")));
    assert!(!first
        .records
        .iter()
        .any(|record| record.path.contains(".git")));
    assert!(first.warnings.iter().any(|warning| {
        (warning.code == "external_symlink" || warning.code == "absolute_symlink")
            && warning.path.ends_with("external-link")
    }));

    let unpack_dir = TempDir::new().unwrap();
    let unpacked = unpack_archive(&first.bytes, unpack_dir.path()).unwrap();
    assert_eq!(
        fs::read_to_string(unpack_dir.path().join("skills/alpha/.claude/config")).unwrap(),
        "keep"
    );
    assert!(!unpacked
        .records
        .iter()
        .any(|record| record.path.contains(".git")));
}

#[cfg(unix)]
#[test]
fn selected_skill_dependency_is_warned_and_skipped_during_archive_scan() {
    let dir = TempDir::new().unwrap();
    let alpha = dir.path().join("alpha");
    let beta = dir.path().join("beta");
    fs::create_dir_all(&alpha).unwrap();
    fs::create_dir_all(&beta).unwrap();
    fs::write(alpha.join("SKILL.md"), "alpha\n").unwrap();
    fs::write(beta.join("SKILL.md"), "beta\n").unwrap();
    std::os::unix::fs::symlink("../beta", alpha.join("related-skill")).unwrap();

    let built = build_skill_archive(dir.path(), &["alpha".to_string()]).unwrap();
    assert!(built
        .warnings
        .iter()
        .any(|warning| warning.code == "unselected_skill_dependency"));
    assert!(!built
        .records
        .iter()
        .any(|record| record.path.ends_with("related-skill")));
}

#[test]
fn git_content_source_builds_no_skill_archive() {
    let home = TempDir::new().unwrap();
    let source_root = home.path().join(".tokenviewer/skills");
    fs::create_dir_all(source_root.join("alpha")).unwrap();
    fs::write(source_root.join("alpha/SKILL.md"), "git-managed\n").unwrap();
    let db = Database::open(&home.path().join(".tokenviewer/data.db")).unwrap();
    let skills = SkillsCore::new(
        &db,
        source_root,
        home.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();

    let mut config = DeviceSyncConfig::for_local_test(
        &home.path().join("remote"),
        "vault-test",
    );
    config.enabled = true;
    config.content_source = "git".to_string();
    config.components = vec![
        SyncComponent::Skills,
        SyncComponent::AgentLinks,
        SyncComponent::SkillEnv,
        SyncComponent::Preferences,
    ];
    config.skill_scope = SkillScope {
        mode: ScopeMode::All,
        skill_ids: Vec::new(),
    };
    let payload = build_snapshot_with_clock(
        &skills,
        home.path(),
        &config,
        &SnapshotBuildRequest {
            snapshot_id: "git-snapshot".to_string(),
            vault_id: "vault-test".to_string(),
            device_id: "device-test".to_string(),
            parent_ids: Vec::new(),
            enabled_agent_ids: Vec::new(),
        },
        Hlc {
            wall_ms: 1,
            counter: 0,
            device_id: "device-test".to_string(),
        },
        None,
    )
    .unwrap()
    .payload;

    assert!(payload.archive.is_empty());
    assert!(payload.manifest.records.skills.is_empty());
    let key = VaultKey::from_bytes([23; 32]);
    let encrypted = encrypt_snapshot(&payload, &key).unwrap();
    assert!(decrypt_snapshot(&encrypted, &key).unwrap().archive.is_empty());
}

#[test]
fn non_frontier_header_parent_tampering_is_rejected_before_frontier_calculation() {
    let remote = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let source = home.path().join(".tokenviewer/skills");
    fs::create_dir_all(&source).unwrap();

    let mut config = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
    config.enabled = true;
    config.components = vec![SyncComponent::Preferences];
    let mut engine = DeviceSyncEngine::for_test(home.path(), source.clone(), config).unwrap();
    let db = Database::open(&home.path().join(".tokenviewer/data.db")).unwrap();
    let skills = SkillsCore::new(
        &db,
        source,
        home.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();

    engine.create_vault("ancestor-header-password").unwrap();
    let first = engine.preview_push(&skills, &[], false).unwrap();
    let first_result = engine.push(&skills, &[], &first.preview_token, false).unwrap();
    let second = engine.preview_push(&skills, &[], false).unwrap();
    engine.push(&skills, &[], &second.preview_token, false).unwrap();

    let first_snapshot_id = first_result.snapshot_id.unwrap();
    let snapshot_path = remote
        .path()
        .join("tokenviewer-sync/vault-test/snapshots")
        .join(format!("{first_snapshot_id}.tvsync"));
    let tampered = rewrite_snapshot_header(
        &fs::read(&snapshot_path).unwrap(),
        Box::new(|header| {
            header.insert(
                "parent_ids".to_string(),
                serde_json::json!(["forged-parent"]),
            );
        }),
    );
    fs::write(snapshot_path, tampered).unwrap();

    assert_eq!(
        engine.status().unwrap_err().code,
        DeviceSyncErrorCode::IntegrityFailed
    );
}

#[test]
fn encrypted_snapshot_rejects_wrong_key_and_tampering() {
    let (archive, archive_records) = test_archive();
    let manifest = test_manifest(&archive, &archive_records);
    let payload = SnapshotPayload {
        manifest: manifest.clone(),
        archive,
    };
    let first_key = VaultKey::from_bytes([7; 32]);
    let second_key = VaultKey::from_bytes([8; 32]);
    let encrypted = encrypt_snapshot(&payload, &first_key).unwrap();
    assert_eq!(
        decrypt_snapshot(&encrypted, &first_key).unwrap().manifest,
        manifest
    );
    assert_eq!(
        decrypt_snapshot(&encrypted, &second_key).unwrap_err().code,
        DeviceSyncErrorCode::IntegrityFailed
    );

    let mut tampered = encrypted.clone();
    *tampered.last_mut().unwrap() ^= 0x01;
    assert_eq!(
        decrypt_snapshot(&tampered, &first_key).unwrap_err().code,
        DeviceSyncErrorCode::AuthenticationFailed
    );
}

#[test]
fn authenticated_snapshot_header_rejects_all_graph_relevant_tampering() {
    let (archive, archive_records) = test_archive();
    let payload = SnapshotPayload {
        manifest: test_manifest(&archive, &archive_records),
        archive,
    };
    let key = VaultKey::from_bytes([17; 32]);
    let encrypted = encrypt_snapshot(&payload, &key).unwrap();

    let mut magic_tampered = encrypted.clone();
    magic_tampered[0] ^= 0x01;
    assert_eq!(
        authenticate_snapshot_header(&magic_tampered, &key)
            .unwrap_err()
            .code,
        DeviceSyncErrorCode::IntegrityFailed
    );

    for mutate in [
        Box::new(|header: &mut serde_json::Map<String, serde_json::Value>| {
            header.insert("protocol_version".to_string(), serde_json::json!(2));
        }) as Box<dyn Fn(&mut serde_json::Map<String, serde_json::Value>)>,
        Box::new(|header: &mut serde_json::Map<String, serde_json::Value>| {
            header.insert("vault_id".to_string(), serde_json::json!("other-vault"));
        }),
        Box::new(|header: &mut serde_json::Map<String, serde_json::Value>| {
            header.insert("snapshot_id".to_string(), serde_json::json!("other-snapshot"));
        }),
        Box::new(|header: &mut serde_json::Map<String, serde_json::Value>| {
            header.insert("payload_sha256".to_string(), serde_json::json!("00".repeat(32)));
        }),
        Box::new(|header: &mut serde_json::Map<String, serde_json::Value>| {
            header.insert(
                "parent_ids".to_string(),
                serde_json::json!(["parent-a", "parent-b"]),
            );
        }),
        Box::new(|header: &mut serde_json::Map<String, serde_json::Value>| {
            header.insert("header_mac_b64".to_string(), serde_json::json!("AA=="));
        }),
    ] {
        let tampered = rewrite_snapshot_header(&encrypted, mutate);
        assert_eq!(
            authenticate_snapshot_header(&tampered, &key)
                .unwrap_err()
                .code,
            DeviceSyncErrorCode::IntegrityFailed
        );
    }
}

#[test]
fn encrypted_snapshot_rejects_component_hash_mismatch() {
    let (archive, archive_records) = test_archive();
    let mut manifest = test_manifest(&archive, &archive_records);
    manifest.records.agent_links.push(AgentLinkRecord {
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
            tombstone: false,
        },
    });
    manifest.components.insert(
        "agent_links".to_string(),
        ComponentSummary {
            sha256: "00".repeat(32),
            bytes: 2,
            entries: 0,
            records: 1,
        },
    );
    let payload = SnapshotPayload { manifest, archive };
    let key = VaultKey::from_bytes([9; 32]);
    let encrypted = encrypt_snapshot(&payload, &key).unwrap();
    assert_eq!(
        decrypt_snapshot(&encrypted, &key).unwrap_err().code,
        DeviceSyncErrorCode::IntegrityFailed
    );
}

#[test]
fn encrypted_snapshot_rejects_compression_bomb_before_local_publish() {
    let remote = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let source_root = home.path().join(".tokenviewer/skills");
    fs::create_dir_all(source_root.join("alpha")).unwrap();
    fs::write(source_root.join("alpha/SKILL.md"), "# Compression test\n").unwrap();
    fs::write(
        source_root.join("alpha/payload.bin"),
        vec![b'A'; 4 * 1024 * 1024],
    )
    .unwrap();

    let mut config = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
    config.enabled = true;
    config.components = vec![SyncComponent::Skills];
    config.skill_scope.mode = ScopeMode::All;
    let mut engine = DeviceSyncEngine::for_test(home.path(), source_root.clone(), config).unwrap();
    let db = Database::open(&home.path().join(".tokenviewer/data.db")).unwrap();
    let skills = SkillsCore::new(
        &db,
        source_root,
        home.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();
    engine.create_vault("compression-bomb-password").unwrap();

    let error = engine.preview_push(&skills, &[], false).unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::ObjectTooLarge);
    assert!(walkdir::WalkDir::new(remote.path())
        .into_iter()
        .filter_map(Result::ok)
        .all(|entry| {
            !entry.file_type().is_file()
                || !entry
                    .path()
                    .components()
                    .any(|component| component.as_os_str() == "snapshots")
        }));
}

#[test]
fn encrypted_snapshot_accepts_a_payload_close_to_the_compression_ratio_limit() {
    let mut candidate = None;
    for stride in [
        8usize, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536, 2048,
    ] {
        let mut content = vec![0u8; 2 * 1024 * 1024];
        let mut random_state = 0x9e37_79b9_u32;
        for index in (0..content.len()).step_by(stride) {
            random_state ^= random_state << 13;
            random_state ^= random_state >> 17;
            random_state ^= random_state << 5;
            content[index] = (random_state >> 24) as u8;
        }
        let payload = test_payload_with_file("payload.bin", &content);
        let manifest_bytes = canonical_json(&payload.manifest).unwrap();
        let mut plaintext = Vec::new();
        plaintext.extend_from_slice(&(manifest_bytes.len() as u64).to_be_bytes());
        plaintext.extend_from_slice(&manifest_bytes);
        plaintext.extend_from_slice(&payload.archive);
        let compressed = zstd::stream::encode_all(Cursor::new(&plaintext), 3).unwrap();
        let expanded_len = plaintext.len() as u64;
        let compressed_len = compressed.len() as u64;
        if expanded_len > compressed_len * (MAX_COMPRESSION_RATIO - 20)
            && expanded_len <= compressed_len * MAX_COMPRESSION_RATIO
        {
            candidate = Some((payload, expanded_len, compressed_len));
            break;
        }
    }

    let (payload, expanded_len, compressed_len) =
        candidate.expect("expected a deterministic payload near the ratio boundary");
    assert!(expanded_len > compressed_len * (MAX_COMPRESSION_RATIO - 20));
    assert!(expanded_len <= compressed_len * MAX_COMPRESSION_RATIO);
    let key = VaultKey::from_bytes([11; 32]);
    let encrypted = encrypt_snapshot(&payload, &key).unwrap();
    assert_eq!(decrypt_snapshot(&encrypted, &key).unwrap(), payload);
}

#[test]
fn archive_rejects_an_oversized_declared_entry_before_reading_it() {
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_size(MAX_SINGLE_FILE_BYTES + 1);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    header.set_path("skills/alpha/payload").unwrap();
    header.set_cksum();
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend_from_slice(&[0u8; 1024]);

    assert_eq!(
        inspect_archive(&bytes).unwrap_err().code,
        DeviceSyncErrorCode::ObjectTooLarge
    );
}

#[test]
fn unpack_archive_rejects_traversal_without_writing_outside_destination() {
    let mut bytes = Vec::new();
    let mut builder = Builder::new(&mut bytes);
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_size(1);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    let invalid_path = b"skills/alpha/../../escape";
    header.as_mut_bytes()[..invalid_path.len()].copy_from_slice(invalid_path);
    header.as_mut_bytes()[invalid_path.len()] = 0;
    header.set_cksum();
    builder.append(&header, Cursor::new(b"x")).unwrap();
    builder.finish().unwrap();
    drop(builder);

    let destination = TempDir::new().unwrap();
    let outside = destination.path().join("escape");
    assert_eq!(
        unpack_archive(&bytes, destination.path()).unwrap_err().code,
        DeviceSyncErrorCode::ArchiveUnsafe
    );
    assert!(!outside.exists());
}

#[test]
fn vault_key_wrap_uses_password_only_for_unwrap() {
    let metadata = create_vault_metadata("vault-test", "device-test", "correct horse").unwrap();
    let key = unwrap_vault_key(&metadata, "correct horse").unwrap();
    assert_eq!(key.as_bytes().len(), 32);
    assert_eq!(
        unwrap_vault_key(&metadata, "wrong").unwrap_err().code,
        DeviceSyncErrorCode::VaultAuthFailed
    );
}

#[test]
fn local_store_enforces_immutable_objects_and_bounded_reads() {
    let dir = TempDir::new().unwrap();
    let store = LocalFolderStore::new(dir.path().to_path_buf()).unwrap();
    let key = ObjectKey::from_path("snapshots/example.tvsync").unwrap();
    store
        .put(
            &key,
            &mut Cursor::new(b"payload".to_vec()),
            7,
            PutCondition::IfNoneMatch,
        )
        .unwrap();
    store
        .put(
            &key,
            &mut Cursor::new(b"payload".to_vec()),
            7,
            PutCondition::IfNoneMatch,
        )
        .unwrap();
    assert_eq!(
        store
            .put(
                &key,
                &mut Cursor::new(b"changed".to_vec()),
                7,
                PutCondition::IfNoneMatch,
            )
            .unwrap_err()
            .code,
        DeviceSyncErrorCode::ImmutableObjectConflict
    );
    let old_meta = store.head(&key).unwrap().unwrap();
    fs::write(dir.path().join("snapshots/example.tvsync"), b"changed").unwrap();
    assert_eq!(
        store
            .put(
                &key,
                &mut Cursor::new(b"replacement".to_vec()),
                11,
                PutCondition::IfMatch(old_meta.etag.unwrap()),
            )
            .unwrap_err()
            .code,
        DeviceSyncErrorCode::RemoteChanged
    );
    let mut output = Vec::new();
    assert_eq!(
        store.get_bounded(&key, 3, &mut output).unwrap_err().code,
        DeviceSyncErrorCode::ObjectTooLarge
    );
}

#[test]
fn two_local_homes_can_push_preview_and_apply_without_plaintext_snapshot() {
    let remote = TempDir::new().unwrap();
    let home_a = TempDir::new().unwrap();
    let home_b = TempDir::new().unwrap();
    let source_a = home_a.path().join(".tokenviewer/skills");
    let source_b = home_b.path().join(".tokenviewer/skills");
    fs::create_dir_all(source_a.join("alpha")).unwrap();
    fs::create_dir_all(source_b.join("beta")).unwrap();
    fs::write(source_a.join("alpha/SKILL.md"), "# Alpha\nsecret body\n").unwrap();
    fs::write(source_b.join("beta/SKILL.md"), "# Beta\nlocal only\n").unwrap();

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
    let skills_a = SkillsCore::new(
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
    engine_a.create_vault("correct horse").unwrap();
    engine_b.join_vault("correct horse").unwrap();

    let preview = engine_a.preview_push(&skills_a, &[], false).unwrap();
    assert!(preview.summary.skills.added >= 1);
    let pushed = engine_a
        .push(&skills_a, &[], &preview.preview_token, false)
        .unwrap();
    assert!(pushed.snapshot_id.is_some());
    let remote_bytes = fs::read_dir(remote.path())
        .unwrap()
        .flat_map(|entry| walkdir::WalkDir::new(entry.unwrap().path()).into_iter())
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| fs::read(entry.path()).unwrap())
        .collect::<Vec<_>>();
    assert!(remote_bytes
        .iter()
        .all(|bytes| !bytes.windows(10).any(|window| window == b"secret body")));

    let unchanged_push = engine_a.preview_push(&skills_a, &[], false).unwrap();
    assert_eq!(unchanged_push.summary.skills.added, 0);
    assert_eq!(unchanged_push.summary.skills.updated, 0);

    let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
    let transaction = engine_b
        .prepare_apply(&skills_b, &pull.preview_token)
        .unwrap();
    engine_b
        .commit_apply(&mut skills_b, &transaction.transaction_id)
        .unwrap();
    assert_eq!(
        fs::read_to_string(source_b.join("alpha/SKILL.md")).unwrap(),
        "# Alpha\nsecret body\n"
    );
    assert_eq!(
        fs::read_to_string(source_b.join("beta/SKILL.md")).unwrap(),
        "# Beta\nlocal only\n"
    );
    assert_eq!(
        engine_b
            .state()
            .max_remote_sequences
            .get(&engine_a.identity().device_id),
        Some(&1)
    );
}

#[test]
fn linear_remote_history_beyond_256_pushes_still_supports_status_and_preview() {
    let remote = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let source = home.path().join(".tokenviewer/skills");
    fs::create_dir_all(&source).unwrap();

    let mut config = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
    config.enabled = true;
    config.components = vec![SyncComponent::Preferences];
    let mut engine = DeviceSyncEngine::for_test(home.path(), source.clone(), config).unwrap();
    let db = Database::open(&home.path().join(".tokenviewer/data.db")).unwrap();
    let skills =
        SkillsCore::new(&db, source, home.path().join(".tokenviewer/skills-manager")).unwrap();

    engine.create_vault("deep-history-password").unwrap();
    for _ in 0..257 {
        let preview = engine.preview_push(&skills, &[], false).unwrap();
        engine.push(&skills, &[], &preview.preview_token, false).unwrap();
    }

    let status = engine.status().unwrap();
    assert_eq!(status.frontier.len(), 1);
    let preview = engine.preview_pull(&skills, &[]).unwrap();
    assert!(!preview.preview_token.is_empty());
}

#[test]
fn preview_rejects_corrupted_large_ancestor_payloads() {
    let remote = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let source = home.path().join(".tokenviewer/skills");
    fs::create_dir_all(&source).unwrap();

    let mut config = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
    config.enabled = true;
    config.components = vec![SyncComponent::Preferences];
    let mut engine = DeviceSyncEngine::for_test(home.path(), source.clone(), config).unwrap();
    let db = Database::open(&home.path().join(".tokenviewer/data.db")).unwrap();
    let skills = SkillsCore::new(
        &db,
        source.clone(),
        home.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();

    engine.create_vault("large-ancestor-password").unwrap();
    let first = engine.preview_push(&skills, &[], false).unwrap();
    let first_result = engine.push(&skills, &[], &first.preview_token, false).unwrap();
    let second = engine.preview_push(&skills, &[], false).unwrap();
    let second_result = engine.push(&skills, &[], &second.preview_token, false).unwrap();

    let first_snapshot_id = first_result.snapshot_id.unwrap();
    let snapshot_path = remote
        .path()
        .join("tokenviewer-sync/vault-test/snapshots")
        .join(format!("{first_snapshot_id}.tvsync"));
    let file = fs::OpenOptions::new()
        .write(true)
        .open(&snapshot_path)
        .unwrap();
    file.set_len(MAX_ENCRYPTED_SNAPSHOT_BYTES).unwrap();

    let status = engine.status().unwrap();
    assert_eq!(status.frontier, vec![second_result.snapshot_id.unwrap()]);
    let error = engine.preview_push(&skills, &[], false).unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::ObjectTooLarge);
}

#[test]
fn apply_without_skills_component_preserves_local_skills() {
    let remote = TempDir::new().unwrap();
    let home_a = TempDir::new().unwrap();
    let home_b = TempDir::new().unwrap();
    let source_a = home_a.path().join(".tokenviewer/skills");
    let source_b = home_b.path().join(".tokenviewer/skills");
    fs::create_dir_all(source_a.join("alpha")).unwrap();
    fs::create_dir_all(source_b.join("beta")).unwrap();
    fs::write(source_a.join("alpha/SKILL.md"), "remote\n").unwrap();
    fs::write(source_b.join("beta/SKILL.md"), "local\n").unwrap();

    let mut config_a = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
    config_a.enabled = true;
    config_a.components = vec![tokenviewer_core::device_sync::models::SyncComponent::AgentLinks];
    let mut config_b = config_a.clone();
    config_b.profile_id = "profile-b".to_string();
    let mut engine_a =
        DeviceSyncEngine::for_test(home_a.path(), source_a.clone(), config_a).unwrap();
    let mut engine_b =
        DeviceSyncEngine::for_test(home_b.path(), source_b.clone(), config_b).unwrap();
    let db_a = Database::open(&home_a.path().join(".tokenviewer/data.db")).unwrap();
    let db_b = Database::open(&home_b.path().join(".tokenviewer/data.db")).unwrap();
    let skills_a = SkillsCore::new(
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

    engine_a.create_vault("component-scope-password").unwrap();
    engine_b.join_vault("component-scope-password").unwrap();
    let preview = engine_a.preview_push(&skills_a, &[], false).unwrap();
    engine_a
        .push(&skills_a, &[], &preview.preview_token, false)
        .unwrap();
    let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
    let transaction = engine_b
        .prepare_apply(&skills_b, &pull.preview_token)
        .unwrap();
    engine_b
        .commit_apply(&mut skills_b, &transaction.transaction_id)
        .unwrap();

    assert_eq!(
        fs::read_to_string(source_b.join("beta/SKILL.md")).unwrap(),
        "local\n"
    );
    assert!(!source_b.join("alpha").exists());
}

#[test]
fn commit_apply_rejects_local_changes_after_prepare() {
    let remote = TempDir::new().unwrap();
    let home_a = TempDir::new().unwrap();
    let home_b = TempDir::new().unwrap();
    let source_a = home_a.path().join(".tokenviewer/skills");
    let source_b = home_b.path().join(".tokenviewer/skills");
    fs::create_dir_all(source_a.join("alpha")).unwrap();
    fs::create_dir_all(&source_b).unwrap();
    fs::write(source_a.join("alpha/SKILL.md"), "remote\n").unwrap();

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
    let skills_a = SkillsCore::new(
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

    engine_a.create_vault("stale-commit-password").unwrap();
    engine_b.join_vault("stale-commit-password").unwrap();
    let preview = engine_a.preview_push(&skills_a, &[], false).unwrap();
    engine_a
        .push(&skills_a, &[], &preview.preview_token, false)
        .unwrap();
    let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
    let transaction = engine_b
        .prepare_apply(&skills_b, &pull.preview_token)
        .unwrap();

    fs::create_dir_all(source_b.join("local-change")).unwrap();
    fs::write(source_b.join("local-change/SKILL.md"), "keep me\n").unwrap();
    assert_eq!(
        engine_b
            .commit_apply(&mut skills_b, &transaction.transaction_id)
            .unwrap_err()
            .code,
        DeviceSyncErrorCode::StalePreview
    );
    assert!(source_b.join("local-change/SKILL.md").is_file());
    assert!(!source_b.join("alpha").exists());
    engine_b
        .rollback_apply(&transaction.transaction_id)
        .unwrap();
}

#[test]
fn apply_failure_restores_skills_and_leaves_original_environment_path_untouched() {
    let remote = TempDir::new().unwrap();
    let home_a = TempDir::new().unwrap();
    let home_b = TempDir::new().unwrap();
    let source_a = home_a.path().join(".tokenviewer/skills");
    let source_b = home_b.path().join(".tokenviewer/skills");
    fs::create_dir_all(source_a.join("alpha")).unwrap();
    fs::create_dir_all(source_b.join("alpha")).unwrap();
    fs::write(source_a.join("alpha/SKILL.md"), "remote\n").unwrap();
    fs::write(source_b.join("alpha/SKILL.md"), "local\n").unwrap();
    let env_path = home_b.path().join(".tokenviewer/skill-env.sh");

    let mut config_a = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
    config_a.enabled = true;
    config_a.components = vec![
        tokenviewer_core::device_sync::models::SyncComponent::Skills,
        tokenviewer_core::device_sync::models::SyncComponent::SkillEnv,
    ];
    config_a.environment_scope.mode = tokenviewer_core::device_sync::models::ScopeMode::All;
    let mut config_b = config_a.clone();
    config_b.profile_id = "profile-b".to_string();
    let mut engine_a =
        DeviceSyncEngine::for_test(home_a.path(), source_a.clone(), config_a).unwrap();
    let mut engine_b =
        DeviceSyncEngine::for_test(home_b.path(), source_b.clone(), config_b).unwrap();
    let db_a = Database::open(&home_a.path().join(".tokenviewer/data.db")).unwrap();
    let db_b = Database::open(&home_b.path().join(".tokenviewer/data.db")).unwrap();
    let skills_a = SkillsCore::new(
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
    engine_a.create_vault("apply-failure-password").unwrap();
    engine_b.join_vault("apply-failure-password").unwrap();
    let preview = engine_a.preview_push(&skills_a, &[], false).unwrap();
    engine_a
        .push(&skills_a, &[], &preview.preview_token, false)
        .unwrap();
    let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
    let transaction = engine_b
        .prepare_apply(&skills_b, &pull.preview_token)
        .unwrap();
    fs::create_dir_all(&env_path).unwrap();

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
    assert!(env_path.is_dir());
}

#[test]
fn occupied_skill_target_is_not_deleted_during_apply() {
    let remote = TempDir::new().unwrap();
    let home_a = TempDir::new().unwrap();
    let home_b = TempDir::new().unwrap();
    let source_a = home_a.path().join(".tokenviewer/skills");
    let source_b = home_b.path().join(".tokenviewer/skills");
    fs::create_dir_all(source_a.join("alpha")).unwrap();
    fs::create_dir_all(&source_b).unwrap();
    fs::write(source_a.join("alpha/SKILL.md"), "remote\n").unwrap();
    fs::write(source_b.join("alpha"), "user file\n").unwrap();

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
    let skills_a = SkillsCore::new(
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
    engine_a.create_vault("occupied-target-password").unwrap();
    engine_b.join_vault("occupied-target-password").unwrap();
    let preview = engine_a.preview_push(&skills_a, &[], false).unwrap();
    engine_a
        .push(&skills_a, &[], &preview.preview_token, false)
        .unwrap();
    let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
    let transaction = engine_b
        .prepare_apply(&skills_b, &pull.preview_token)
        .unwrap();

    assert_eq!(
        engine_b
            .commit_apply(&mut skills_b, &transaction.transaction_id)
            .unwrap_err()
            .code,
        DeviceSyncErrorCode::LinkTargetOccupied
    );
    assert_eq!(
        fs::read_to_string(source_b.join("alpha")).unwrap(),
        "user file\n"
    );
}

#[test]
fn pending_apply_journal_is_recovered_after_engine_restart() {
    let remote = TempDir::new().unwrap();
    let home_a = TempDir::new().unwrap();
    let home_b = TempDir::new().unwrap();
    let source_a = home_a.path().join(".tokenviewer/skills");
    let source_b = home_b.path().join(".tokenviewer/skills");
    fs::create_dir_all(source_a.join("alpha")).unwrap();
    fs::create_dir_all(&source_b).unwrap();
    fs::write(source_a.join("alpha/SKILL.md"), "remote\n").unwrap();

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
    let skills_a = SkillsCore::new(
        &db_a,
        source_a.clone(),
        home_a.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();
    let skills_b = SkillsCore::new(
        &db_b,
        source_b.clone(),
        home_b.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();
    engine_a.create_vault("recovery-password").unwrap();
    engine_b.join_vault("recovery-password").unwrap();
    let preview = engine_a.preview_push(&skills_a, &[], false).unwrap();
    engine_a
        .push(&skills_a, &[], &preview.preview_token, false)
        .unwrap();
    let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
    engine_b
        .prepare_apply(&skills_b, &pull.preview_token)
        .unwrap();
    drop(engine_b);

    let mut restarted =
        DeviceSyncEngine::new(home_b.path().to_path_buf(), source_b.clone()).unwrap();
    assert_eq!(restarted.recover_pending_apply().unwrap(), 1);
    assert!(!home_b
        .path()
        .join(".tokenviewer/device-sync/rollback")
        .read_dir()
        .unwrap()
        .next()
        .is_some());
    assert!(!source_b.join("alpha").exists());
}

#[test]
fn committed_apply_survives_state_save_failure_and_converges_after_restart() {
    let remote = TempDir::new().unwrap();
    let home_a = TempDir::new().unwrap();
    let home_b = TempDir::new().unwrap();
    let source_a = home_a.path().join(".tokenviewer/skills");
    let source_b = home_b.path().join(".tokenviewer/skills");
    fs::create_dir_all(source_a.join("alpha")).unwrap();
    fs::create_dir_all(&source_b).unwrap();
    fs::write(source_a.join("alpha/SKILL.md"), "remote\n").unwrap();

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
    let skills_a = SkillsCore::new(
        &db_a,
        source_a,
        home_a.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();
    let mut skills_b = SkillsCore::new(
        &db_b,
        source_b.clone(),
        home_b.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();

    engine_a.create_vault("committed-state-save-password").unwrap();
    engine_b.join_vault("committed-state-save-password").unwrap();
    let push = engine_a.preview_push(&skills_a, &[], false).unwrap();
    let pushed = engine_a.push(&skills_a, &[], &push.preview_token, false).unwrap();
    let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
    let transaction = engine_b
        .prepare_apply(&skills_b, &pull.preview_token)
        .unwrap();

    let paths = DeviceSyncPaths::new(home_b.path());
    fs::remove_file(&paths.state).unwrap();
    fs::create_dir(&paths.state).unwrap();
    let error = engine_b
        .commit_apply(&mut skills_b, &transaction.transaction_id)
        .unwrap_err();
    assert_eq!(error.code, DeviceSyncErrorCode::RecoveryBlocked);
    assert_eq!(
        fs::read_to_string(source_b.join("alpha/SKILL.md")).unwrap(),
        "remote\n"
    );
    let journal_path = transaction
        .recovery_path
        .as_ref()
        .expect("prepare returns a recovery path")
        .join("journal.json");
    let journal: serde_json::Value = serde_json::from_slice(&fs::read(&journal_path).unwrap()).unwrap();
    assert_eq!(journal.get("phase").and_then(serde_json::Value::as_str), Some("committed"));

    drop(engine_b);
    fs::remove_dir_all(&paths.state).unwrap();
    let mut restarted =
        DeviceSyncEngine::new(home_b.path().to_path_buf(), source_b.clone()).unwrap();
    let recovery = restarted.recover_pending_apply_detailed().unwrap();
    assert_eq!(recovery.committed_transaction_ids, vec![transaction.transaction_id]);
    assert_eq!(
        load_state(&paths).unwrap().applied_snapshot_id,
        pushed.snapshot_id
    );
    assert!(source_b.join("alpha/SKILL.md").is_file());
    assert!(fs::read_dir(&paths.rollback).unwrap().next().is_none());
}

#[test]
fn deleting_a_skill_emits_a_tombstone_and_removes_it_on_pull() {
    let remote = TempDir::new().unwrap();
    let home_a = TempDir::new().unwrap();
    let home_b = TempDir::new().unwrap();
    let source_a = home_a.path().join(".tokenviewer/skills");
    let source_b = home_b.path().join(".tokenviewer/skills");
    fs::create_dir_all(source_a.join("alpha")).unwrap();
    fs::create_dir_all(source_b.join("alpha")).unwrap();
    fs::write(source_a.join("alpha/SKILL.md"), "alpha\n").unwrap();
    fs::write(source_b.join("alpha/SKILL.md"), "alpha\n").unwrap();

    let mut config_a = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
    config_a.enabled = true;
    config_a.components = vec![SyncComponent::Skills];
    config_a.skill_scope = SkillScope {
        mode: ScopeMode::Selected,
        skill_ids: vec!["alpha".to_string()],
    };
    let mut config_b = config_a.clone();
    config_b.profile_id = "profile-b".to_string();
    let mut engine_a =
        DeviceSyncEngine::for_test(home_a.path(), source_a.clone(), config_a).unwrap();
    let mut engine_b =
        DeviceSyncEngine::for_test(home_b.path(), source_b.clone(), config_b).unwrap();
    let db_a = Database::open(&home_a.path().join(".tokenviewer/data.db")).unwrap();
    let db_b = Database::open(&home_b.path().join(".tokenviewer/data.db")).unwrap();
    let skills_a = SkillsCore::new(
        &db_a,
        source_a.clone(),
        home_a.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();
    let skills_b = SkillsCore::new(
        &db_b,
        source_b.clone(),
        home_b.path().join(".tokenviewer/skills-manager"),
    )
    .unwrap();

    engine_a.create_vault("tombstone-password").unwrap();
    engine_b.join_vault("tombstone-password").unwrap();
    let initial_status = engine_a.status().unwrap();
    assert!(initial_status.remote_head_fingerprint.is_some());
    assert!(initial_status.frontier.is_empty());
    let first = engine_a.preview_push(&skills_a, &[], false).unwrap();
    let first_result = engine_a.push(&skills_a, &[], &first.preview_token, false).unwrap();
    let pushed_status = engine_a.status().unwrap();
    assert_eq!(
        pushed_status.frontier,
        vec![first_result.snapshot_id.clone().unwrap()]
    );
    fs::remove_dir_all(source_a.join("alpha")).unwrap();

    let second = engine_a.preview_push(&skills_a, &[], false).unwrap();
    assert_eq!(second.summary.skills.deleted, 1);
    engine_a
        .push(&skills_a, &[], &second.preview_token, false)
        .unwrap();

    let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
    assert_eq!(pull.summary.skills.deleted, 1);
    let transaction = engine_b
        .prepare_apply(&skills_b, &pull.preview_token)
        .unwrap();
    let mut skills_b = skills_b;
    engine_b
        .commit_apply(&mut skills_b, &transaction.transaction_id)
        .unwrap();
    assert!(!source_b.join("alpha").exists());
}

#[test]
fn git_pull_records_pending_content_without_creating_empty_links() {
    let remote = TempDir::new().unwrap();
    let home_a = TempDir::new().unwrap();
    let home_b = TempDir::new().unwrap();
    let source_a = home_a.path().join(".tokenviewer/skills");
    let source_b = home_b.path().join(".tokenviewer/skills");
    fs::create_dir_all(source_a.join("alpha")).unwrap();
    fs::write(source_a.join("alpha/SKILL.md"), "alpha\n").unwrap();

    let mut config_a = DeviceSyncConfig::for_local_test(remote.path(), "vault-test");
    config_a.enabled = true;
    config_a.content_source = "git".to_string();
    config_a.components = vec![SyncComponent::AgentLinks, SyncComponent::Preferences];
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
    skills_a.registry.link_skill("codex", "alpha").unwrap();

    engine_a.create_vault("git-pending-password").unwrap();
    engine_b.join_vault("git-pending-password").unwrap();
    let push = engine_a.preview_push(&skills_a, &[], false).unwrap();
    engine_a.push(&skills_a, &[], &push.preview_token, false).unwrap();
    let pull = engine_b.preview_pull(&skills_b, &[]).unwrap();
    let transaction = engine_b
        .prepare_apply(&skills_b, &pull.preview_token)
        .unwrap();
    engine_b
        .commit_apply(&mut skills_b, &transaction.transaction_id)
        .unwrap();

    let pending = engine_b
        .state()
        .pending_content_source
        .expect("Git content should remain pending");
    assert_eq!(pending.content_source, "git");
    assert_eq!(pending.skill_ids, vec!["alpha"]);
    assert!(!source_b.join("alpha").exists());
    assert!(!home_b.path().join(".codex/skills/alpha").exists());
}

fn test_archive() -> (Vec<u8>, Vec<SkillFileRecord>) {
    let payload = test_payload_with_file("SKILL.md", b"archive body\n");
    let records = payload.manifest.records.skills[0].files.clone();
    (payload.archive, records)
}

fn test_payload_with_file(name: &str, content: &[u8]) -> SnapshotPayload {
    let dir = TempDir::new().unwrap();
    let skill = dir.path().join("alpha");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join(name), content).unwrap();
    let built = build_skill_archive(dir.path(), &["alpha".to_string()]).unwrap();
    let manifest = test_manifest(&built.bytes, &built.records);
    SnapshotPayload {
        manifest,
        archive: built.bytes,
    }
}

fn test_manifest(archive: &[u8], archive_records: &[SkillFileRecord]) -> SnapshotManifest {
    let expanded_bytes = archive_records
        .iter()
        .filter(|record| record.kind == SkillFileKind::File)
        .map(|record| record.size)
        .sum();
    let mut manifest = SnapshotManifest::new(
        "snapshot-test".to_string(),
        "vault-test".to_string(),
        "device-test".to_string(),
        Vec::new(),
        "cloud".to_string(),
        std::collections::BTreeMap::from([(
            "skills".to_string(),
            ComponentSummary {
                sha256: sha256_hex(archive),
                bytes: archive.len() as u64,
                entries: archive_records.len() as u64,
                records: 1,
            },
        )]),
    );
    manifest.records.skills = vec![SkillRecord {
        skill_id: "alpha".to_string(),
        metadata: RecordMetadata {
            record_id: "skill-alpha".to_string(),
            hlc: Hlc {
                wall_ms: 1,
                counter: 0,
                device_id: "device-test".to_string(),
            },
            last_modified_by: "device-test".to_string(),
            tombstone: false,
        },
        files: archive_records.to_vec(),
    }];
    manifest.limits.archive_bytes = archive.len() as u64;
    manifest.limits.expanded_bytes = expanded_bytes;
    manifest
}

fn rewrite_snapshot_header(
    encrypted: &[u8],
    mutate: Box<dyn Fn(&mut serde_json::Map<String, serde_json::Value>)>,
) -> Vec<u8> {
    let header_length_start = TVSYNC_MAGIC.len();
    let header_length = u32::from_be_bytes(
        encrypted[header_length_start..header_length_start + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    let header_start = header_length_start + 4;
    let header_end = header_start + header_length;
    let mut header: serde_json::Value = serde_json::from_slice(&encrypted[header_start..header_end]).unwrap();
    mutate(header.as_object_mut().unwrap());
    let header_bytes = canonical_json(&header).unwrap();
    let mut rewritten = Vec::new();
    rewritten.extend_from_slice(TVSYNC_MAGIC);
    rewritten.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
    rewritten.extend_from_slice(&header_bytes);
    rewritten.extend_from_slice(&encrypted[header_end..]);
    rewritten
}
