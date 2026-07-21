use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use tokenviewer_core::parsers::{codex, kiro, parse_all, workbuddy};
use tokenviewer_core::storage::Database;
use tokenviewer_core::sync::sync_all;

#[derive(Debug, Clone)]
struct Expectation {
    model: &'static str,
    total_tokens: u64,
    conversation_count: u32,
}

#[test]
fn provider_parser_matrix_matches_reference_shapes() {
    let home = temp_home();
    seed_provider_fixtures(&home);

    let cursors = HashMap::new();
    let results = parse_all(&home, &cursors);
    let by_source: HashMap<String, _> =
        results.into_iter().map(|r| (r.source.clone(), r)).collect();

    let cases: [(&str, Expectation); 25] = [
        (
            "claude",
            Expectation {
                model: "claude-3-7-sonnet",
                total_tokens: 58,
                conversation_count: 0,
            },
        ),
        (
            "codex",
            Expectation {
                model: "openai",
                total_tokens: 18,
                conversation_count: 1,
            },
        ),
        (
            "cursor",
            Expectation {
                model: "cursor-1",
                total_tokens: 20,
                conversation_count: 1,
            },
        ),
        (
            "gemini",
            Expectation {
                model: "gemini-2.5-pro",
                total_tokens: 37,
                conversation_count: 1,
            },
        ),
        (
            "kiro",
            Expectation {
                model: "kiro-agent",
                total_tokens: 20,
                conversation_count: 1,
            },
        ),
        (
            "opencode",
            Expectation {
                model: "gpt-4.1-mini",
                total_tokens: 22,
                conversation_count: 1,
            },
        ),
        (
            "openclaw",
            Expectation {
                model: "claude-4",
                total_tokens: 15,
                conversation_count: 1,
            },
        ),
        (
            "everycode",
            Expectation {
                model: "openai",
                total_tokens: 18,
                conversation_count: 1,
            },
        ),
        (
            "hermes",
            Expectation {
                model: "hermes-3",
                total_tokens: 25,
                conversation_count: 1,
            },
        ),
        (
            "copilot",
            Expectation {
                model: "gpt-4.1",
                total_tokens: 30,
                conversation_count: 1,
            },
        ),
        (
            "kimi",
            Expectation {
                model: "kimi-for-coding",
                total_tokens: 36,
                conversation_count: 1,
            },
        ),
        (
            "grok",
            Expectation {
                model: "grok-build",
                total_tokens: 40,
                conversation_count: 1,
            },
        ),
        (
            "antigravity",
            Expectation {
                model: "gemini-2.5-flash",
                total_tokens: 14,
                conversation_count: 1,
            },
        ),
        (
            "roocode",
            Expectation {
                model: "provider:anthropic",
                total_tokens: 26,
                conversation_count: 1,
            },
        ),
        (
            "kilocode",
            Expectation {
                model: "provider:anthropic",
                total_tokens: 26,
                conversation_count: 1,
            },
        ),
        (
            "kilocli",
            Expectation {
                model: "gpt-4o-mini",
                total_tokens: 22,
                conversation_count: 1,
            },
        ),
        (
            "zed",
            Expectation {
                model: "claude-opus-4.1",
                total_tokens: 14,
                conversation_count: 1,
            },
        ),
        (
            "goose",
            Expectation {
                model: "gpt-5",
                total_tokens: 25,
                conversation_count: 1,
            },
        ),
        (
            "ohmypi",
            Expectation {
                model: "omp-unknown",
                total_tokens: 19,
                conversation_count: 1,
            },
        ),
        (
            "pi",
            Expectation {
                model: "pi-unknown",
                total_tokens: 19,
                conversation_count: 1,
            },
        ),
        (
            "craft",
            Expectation {
                model: "craft-1",
                total_tokens: 20,
                conversation_count: 1,
            },
        ),
        (
            "codebuddy",
            Expectation {
                model: "codebuddy-2",
                total_tokens: 37,
                conversation_count: 1,
            },
        ),
        (
            "workbuddy",
            Expectation {
                model: "workbuddy-quota",
                total_tokens: 400,
                conversation_count: 1,
            },
        ),
        (
            "mimocode",
            Expectation {
                model: "mimo-auto",
                total_tokens: 18,
                conversation_count: 0,
            },
        ),
        (
            "zcode",
            Expectation {
                model: "GLM-5.2",
                total_tokens: 21,
                conversation_count: 0,
            },
        ),
    ];

    assert_eq!(
        by_source.len(),
        cases.len(),
        "expected one result per registered provider"
    );

    for (source, expected) in cases {
        let result = by_source
            .get(source)
            .unwrap_or_else(|| panic!("missing parse result for {source}"));
        assert_eq!(
            result.records.len(),
            1,
            "expected exactly one aggregated record for {source}"
        );

        let record = &result.records[0];
        let expected_record_source = match source {
            "everycode" => "every-code",
            "kilocli" => "kilo-cli",
            "pi" => "pi",
            _ => source,
        };
        assert_eq!(
            record.source, expected_record_source,
            "source mismatch for {source}"
        );
        assert_eq!(record.model, expected.model, "model mismatch for {source}");
        assert_eq!(
            record.total_tokens, expected.total_tokens,
            "token mismatch for {source}"
        );
        assert_eq!(
            record.conversation_count, expected.conversation_count,
            "conversation count mismatch for {source}"
        );
    }
}

#[test]
fn zcode_running_rows_are_reprocessed_after_completion() {
    let home = temp_home();
    let db_path = home.join(".zcode/cli/db/db.sqlite");
    init_zcode_schema(&db_path);
    insert_zcode_row(
        &db_path,
        "zcode-completed",
        "req-zcode-completed",
        "sess-zcode-1",
        "main_turn",
        "builtin:bigmodel-start-plan",
        "GLM-5.2",
        "completed",
        1735689600000_i64,
        10,
        6,
        2,
        3,
        1,
    );
    insert_zcode_row(
        &db_path,
        "zcode-running",
        "req-zcode-running",
        "sess-zcode-1",
        "main_turn",
        "builtin:bigmodel-start-plan",
        "GLM-5.2",
        "running",
        1735689660000_i64,
        7,
        4,
        1,
        2,
        0,
    );

    let (first_records, cursor_json) =
        tokenviewer_core::parsers::zcode::parse(&home, None).expect("first zcode parse");
    assert_eq!(first_records.len(), 1);
    assert_eq!(first_records[0].total_tokens, 21);

    update_zcode_row_status(&db_path, "zcode-running", "completed");

    let (second_records, _) = tokenviewer_core::parsers::zcode::parse(&home, Some(&cursor_json))
        .expect("second zcode parse");
    assert_eq!(second_records.len(), 1);
    assert_eq!(second_records[0].total_tokens, 14);
}

#[test]
fn zcode_same_millisecond_rows_use_id_tiebreaker() {
    let home = temp_home();
    let db_path = home.join(".zcode/cli/db/db.sqlite");
    init_zcode_schema(&db_path);
    insert_zcode_row(
        &db_path,
        "zcode-1",
        "req-zcode-1",
        "sess-zcode-1",
        "main_turn",
        "builtin:bigmodel-start-plan",
        "GLM-5.2",
        "completed",
        1735689600000_i64,
        10,
        6,
        2,
        3,
        1,
    );

    let (first_records, cursor_json) =
        tokenviewer_core::parsers::zcode::parse(&home, None).expect("first zcode parse");
    assert_eq!(first_records.len(), 1);
    assert_eq!(first_records[0].total_tokens, 21);

    insert_zcode_row(
        &db_path,
        "zcode-2",
        "req-zcode-2",
        "sess-zcode-1",
        "main_turn",
        "builtin:bigmodel-start-plan",
        "GLM-5.2",
        "completed",
        1735689600000_i64,
        4,
        3,
        1,
        2,
        0,
    );

    let (second_records, _) = tokenviewer_core::parsers::zcode::parse(&home, Some(&cursor_json))
        .expect("second zcode parse");
    assert_eq!(second_records.len(), 1);
    assert_eq!(second_records[0].total_tokens, 10);
}

#[test]
fn codex_incremental_sync_keeps_model_context() {
    let home = temp_home();
    let file = home.join(".codex/sessions/2026/01/01/rollout-1.jsonl");

    write_text(
        &file,
        concat!(
            "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"model_provider\":\"openai\",\"model\":\"gpt-4.1\"}}\n",
            "{\"timestamp\":\"2026-01-01T00:01:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":10,\"output_tokens\":20,\"cached_input_tokens\":1,\"cache_creation_input_tokens\":2,\"reasoning_output_tokens\":3}}}}"
        ),
    );

    let (first_records, cursor_json) = codex::parse(&home, None).expect("first codex parse");
    assert_eq!(first_records.len(), 1);
    assert_eq!(first_records[0].model, "gpt-4.1");

    sleep(Duration::from_millis(1100));

    write_text(
        &file,
        concat!(
            "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"model_provider\":\"openai\",\"model\":\"gpt-4.1\"}}\n",
            "{\"timestamp\":\"2026-01-01T00:01:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":10,\"output_tokens\":20,\"cached_input_tokens\":1,\"cache_creation_input_tokens\":2,\"reasoning_output_tokens\":3}}}}\n",
            "{\"timestamp\":\"2026-01-01T00:02:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":15,\"output_tokens\":30,\"cached_input_tokens\":1,\"cache_creation_input_tokens\":2,\"reasoning_output_tokens\":3}}}}"
        ),
    );

    let (second_records, _) = codex::parse(&home, Some(&cursor_json)).expect("second codex parse");
    assert_eq!(second_records.len(), 1);
    assert_eq!(second_records[0].model, "gpt-4.1");
    assert_ne!(second_records[0].model, "unknown");
}

/// Kiro CLI v3 session directory: ~/.kiro/sessions/<workspace-hash>/sess_<uuid>/
/// with session.json (metadata incl. modelId) + messages.jsonl (event log).
fn write_kiro_v3_session_json(home: &Path, workspace_hash: &str, session_id: &str, model_id: &str) {
    let path = home
        .join(".kiro/sessions")
        .join(workspace_hash)
        .join(format!("sess_{}", session_id))
        .join("session.json");
    write_text(
        &path,
        &format!(
            r#"{{"schemaVersion":"1.0.0","id":"sess_{session_id}","title":"test","agentMode":"vibe","workspacePaths":["/tmp"],"createdAt":"2026-07-21T00:00:00.000Z","lastModifiedAt":"2026-07-21T00:00:00.000Z","modelId":"{model_id}","autopilot":true,"status":"idle"}}"#,
            session_id = session_id,
            model_id = model_id
        ),
    );
}

fn kiro_v3_messages_path(home: &Path, workspace_hash: &str, session_id: &str) -> PathBuf {
    home.join(".kiro/sessions")
        .join(workspace_hash)
        .join(format!("sess_{}", session_id))
        .join("messages.jsonl")
}

#[test]
fn kiro_v3_session_parses_turn_into_char_estimated_usage() {
    let home = temp_home();
    write_kiro_v3_session_json(&home, "hash1", "aaaa", "gpt-5.6-sol");
    let messages_path = kiro_v3_messages_path(&home, "hash1", "aaaa");

    // One turn: turn_start -> user (16 chars) -> assistant Say (8 chars) ->
    // tool_call (args ~10 chars) -> tool_result (~6 chars) -> turn_end.
    write_text(
        &messages_path,
        concat!(
            "{\"id\":\"1\",\"timestamp\":\"2026-07-21T02:47:47.050Z\",\"payload\":{\"type\":\"turn_start\",\"executionId\":\"exec-1\"}}\n",
            "{\"id\":\"2\",\"timestamp\":\"2026-07-21T02:47:48.000Z\",\"payload\":{\"type\":\"user\",\"content\":\"1234567890123456\"}}\n",
            "{\"id\":\"3\",\"timestamp\":\"2026-07-21T02:47:49.000Z\",\"payload\":{\"type\":\"assistant\",\"operationType\":\"Say\",\"content\":\"12345678\",\"executionId\":\"exec-1\"}}\n",
            "{\"id\":\"4\",\"timestamp\":\"2026-07-21T02:47:50.000Z\",\"payload\":{\"type\":\"tool_call\",\"toolName\":\"shell\",\"args\":{\"command\":\"1234\"},\"executionId\":\"exec-1\"}}\n",
            "{\"id\":\"5\",\"timestamp\":\"2026-07-21T02:47:51.000Z\",\"payload\":{\"type\":\"tool_result\",\"content\":\"123456\",\"executionId\":\"exec-1\"}}\n",
            "{\"id\":\"6\",\"timestamp\":\"2026-07-21T02:47:52.000Z\",\"payload\":{\"type\":\"turn_end\",\"stopReason\":\"end_turn\",\"executionId\":\"exec-1\"}}"
        ),
    );

    let (records, _cursor_json) = kiro::parse(&home, None).expect("kiro v3 parse");
    assert_eq!(records.len(), 1, "expected exactly one usage record per turn");
    let rec = &records[0];
    assert_eq!(rec.source, "kiro");
    assert_eq!(rec.model, "gpt-5.6-sol");
    // input = user(16 chars) + tool_result(6 chars) = 22 chars -> 22/4 = 5 tokens
    assert_eq!(rec.input_tokens, 5);
    // output = assistant(8 chars) + tool_call args (`{"command":"1234"}` compact = 18 chars) -> 8/4 + 18/4 = 2 + 4 = 6 tokens
    assert_eq!(rec.output_tokens, 6);
    assert_eq!(rec.total_tokens, rec.input_tokens + rec.output_tokens);
}

#[test]
fn kiro_v3_turn_end_and_usage_summary_do_not_double_count() {
    let home = temp_home();
    write_kiro_v3_session_json(&home, "hash1", "bbbb", "claude-sonnet-4.6");
    let messages_path = kiro_v3_messages_path(&home, "hash1", "bbbb");

    // turn_end and usage_summary share the same executionId and arrive
    // back-to-back — must only produce one record, not two.
    write_text(
        &messages_path,
        concat!(
            "{\"id\":\"1\",\"timestamp\":\"2026-07-21T02:47:47.050Z\",\"payload\":{\"type\":\"turn_start\",\"executionId\":\"exec-1\"}}\n",
            "{\"id\":\"2\",\"timestamp\":\"2026-07-21T02:47:48.000Z\",\"payload\":{\"type\":\"user\",\"content\":\"12345678\"}}\n",
            "{\"id\":\"3\",\"timestamp\":\"2026-07-21T02:47:49.000Z\",\"payload\":{\"type\":\"assistant\",\"operationType\":\"Say\",\"content\":\"12345678\",\"executionId\":\"exec-1\"}}\n",
            "{\"id\":\"4\",\"timestamp\":\"2026-07-21T02:47:50.000Z\",\"payload\":{\"type\":\"turn_end\",\"stopReason\":\"end_turn\",\"executionId\":\"exec-1\"}}\n",
            "{\"id\":\"5\",\"timestamp\":\"2026-07-21T02:47:50.100Z\",\"payload\":{\"type\":\"usage_summary\",\"promptTurnSummaries\":[{\"unit\":\"credit\",\"usage\":1.2,\"usedTools\":[]}],\"elapsedTime\":100,\"status\":\"success\",\"executionId\":\"exec-1\"}}"
        ),
    );

    let (records, _) = kiro::parse(&home, None).expect("kiro v3 parse");
    assert_eq!(
        records.len(),
        1,
        "turn_end + usage_summary for the same turn must yield exactly one record"
    );
}

#[test]
fn kiro_v3_incremental_sync_only_counts_new_turns() {
    let home = temp_home();
    write_kiro_v3_session_json(&home, "hash1", "cccc", "gpt-5.6-sol");
    let messages_path = kiro_v3_messages_path(&home, "hash1", "cccc");

    write_text(
        &messages_path,
        concat!(
            "{\"id\":\"1\",\"timestamp\":\"2026-07-21T02:00:00.000Z\",\"payload\":{\"type\":\"turn_start\",\"executionId\":\"exec-1\"}}\n",
            "{\"id\":\"2\",\"timestamp\":\"2026-07-21T02:00:01.000Z\",\"payload\":{\"type\":\"user\",\"content\":\"12345678\"}}\n",
            "{\"id\":\"3\",\"timestamp\":\"2026-07-21T02:00:02.000Z\",\"payload\":{\"type\":\"assistant\",\"operationType\":\"Say\",\"content\":\"12345678\",\"executionId\":\"exec-1\"}}\n",
            "{\"id\":\"4\",\"timestamp\":\"2026-07-21T02:00:03.000Z\",\"payload\":{\"type\":\"turn_end\",\"stopReason\":\"end_turn\",\"executionId\":\"exec-1\"}}\n"
        ),
    );

    let (first_records, cursor_json) = kiro::parse(&home, None).expect("first kiro v3 parse");
    assert_eq!(first_records.len(), 1);

    sleep(Duration::from_millis(1100));

    // Append a second turn — the previously-read bytes must not be re-parsed.
    let mut appended = fs::read_to_string(&messages_path).unwrap();
    appended.push_str(concat!(
        "{\"id\":\"5\",\"timestamp\":\"2026-07-21T03:00:00.000Z\",\"payload\":{\"type\":\"turn_start\",\"executionId\":\"exec-2\"}}\n",
        "{\"id\":\"6\",\"timestamp\":\"2026-07-21T03:00:01.000Z\",\"payload\":{\"type\":\"user\",\"content\":\"1234567890123456\"}}\n",
        "{\"id\":\"7\",\"timestamp\":\"2026-07-21T03:00:02.000Z\",\"payload\":{\"type\":\"assistant\",\"operationType\":\"Say\",\"content\":\"12345678\",\"executionId\":\"exec-2\"}}\n",
        "{\"id\":\"8\",\"timestamp\":\"2026-07-21T03:00:03.000Z\",\"payload\":{\"type\":\"turn_end\",\"stopReason\":\"end_turn\",\"executionId\":\"exec-2\"}}\n"
    ));
    write_text(&messages_path, &appended);

    let (second_records, _) =
        kiro::parse(&home, Some(&cursor_json)).expect("second kiro v3 parse");
    assert_eq!(
        second_records.len(),
        1,
        "incremental parse must only pick up the newly appended turn"
    );
    // input = 16 chars -> 4 tokens
    assert_eq!(second_records[0].input_tokens, 4);
}

#[test]
fn kiro_v3_and_legacy_cli_format_do_not_conflict() {
    let home = temp_home();

    // Legacy flat format: ~/.kiro/sessions/cli/<uuid>.json + .jsonl
    write_text(
        &home.join(".kiro/sessions/cli/legacy-uuid.json"),
        r#"{"session_state":{"conversation_metadata":{"user_turn_metadatas":[{"end_timestamp":"2026-07-21T01:00:00Z","message_ids":["m1"],"input_token_count":0,"output_token_count":0}]},"rts_model_state":{"model_info":{"model_id":"CLAUDE_SONNET_4_6"}}}}"#,
    );
    write_text(
        &home.join(".kiro/sessions/cli/legacy-uuid.jsonl"),
        concat!(
            "{\"kind\":\"Prompt\",\"data\":{\"message_id\":\"m1\",\"content\":[{\"kind\":\"text\",\"data\":\"hello world test\"}]}}\n",
            "{\"kind\":\"AssistantMessage\",\"data\":{\"message_id\":\"m1\",\"content\":[{\"kind\":\"text\",\"data\":\"hi there\"}]}}"
        ),
    );

    // New v3 format: ~/.kiro/sessions/<hash>/sess_<uuid>/
    write_kiro_v3_session_json(&home, "hash1", "dddd", "gpt-5.6-sol");
    write_text(
        &kiro_v3_messages_path(&home, "hash1", "dddd"),
        concat!(
            "{\"id\":\"1\",\"timestamp\":\"2026-07-21T02:00:00.000Z\",\"payload\":{\"type\":\"turn_start\",\"executionId\":\"exec-1\"}}\n",
            "{\"id\":\"2\",\"timestamp\":\"2026-07-21T02:00:01.000Z\",\"payload\":{\"type\":\"user\",\"content\":\"12345678\"}}\n",
            "{\"id\":\"3\",\"timestamp\":\"2026-07-21T02:00:02.000Z\",\"payload\":{\"type\":\"assistant\",\"operationType\":\"Say\",\"content\":\"12345678\",\"executionId\":\"exec-1\"}}\n",
            "{\"id\":\"4\",\"timestamp\":\"2026-07-21T02:00:03.000Z\",\"payload\":{\"type\":\"turn_end\",\"stopReason\":\"end_turn\",\"executionId\":\"exec-1\"}}\n"
        ),
    );

    let (records, _) = kiro::parse(&home, None).expect("kiro parse with both formats");
    assert_eq!(
        records.len(),
        2,
        "legacy cli/*.json session and v3 sess_* session must both be counted, exactly once each"
    );
    let models: Vec<&str> = records.iter().map(|r| r.model.as_str()).collect();
    assert!(models.contains(&"claude-sonnet-4.6"));
    assert!(models.contains(&"gpt-5.6-sol"));
}

#[test]
fn workbuddy_parser_falls_back_to_quota_snapshot() {
    let home = temp_home();
    write_text(
        &home.join(".antigravity_cockpit/workbuddy_accounts.json"),
        r#"{"version":"1.0","accounts":[{"id":"workbuddy_quota_only","email":"wb@example.com","last_used":1735690200}]}"#,
    );
    write_text(
        &home.join(".antigravity_cockpit/workbuddy_accounts/workbuddy_quota_only.json"),
        r#"{
  "id": "workbuddy_quota_only",
  "email": "wb@example.com",
  "quota_raw": {
    "userResource": {
      "data": {
        "Response": {
          "Data": {
            "Accounts": [
              {
                "PackageCode": "TCACA_code_002_AkiJS3ZHF5",
                "Status": 0,
                "CycleCapacitySizePrecise": "500",
                "CycleCapacityRemainPrecise": "125"
              }
            ]
          }
        }
      }
    }
  },
  "usage_updated_at": 1735690200
}"#,
    );

    let (records, _) = workbuddy::parse(&home, None).expect("workbuddy parse");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].model, "workbuddy-quota");
    assert_eq!(records[0].total_tokens, 375);
}

#[test]
fn mimocode_sync_reuses_saved_cursor() {
    let home = temp_home();
    init_mimocode_db(&home.join(".local/share/mimocode/mimocode.db"));

    let db = Database::open(&home.join("tokenviewer.db")).expect("open test db");

    let first = sync_all(&db, &home);
    assert!(
        first.errors.is_empty(),
        "first sync errors: {:?}",
        first.errors
    );

    let second = sync_all(&db, &home);
    assert!(
        second.errors.is_empty(),
        "second sync errors: {:?}",
        second.errors
    );

    let rows = db
        .aggregate_by_model("2025-01-01T00:00:00Z", "2025-01-01T01:00:00Z")
        .expect("aggregate usage");
    let mimocode = rows
        .iter()
        .find(|r| r.source == "mimocode" && r.model == "mimo-auto")
        .expect("mimocode aggregate");

    assert_eq!(mimocode.total_tokens, 18);
    assert_eq!(mimocode.input_tokens, 10);
    assert_eq!(mimocode.output_tokens, 6);
    assert_eq!(mimocode.reasoning_output_tokens, 2);
}

#[test]
fn workbuddy_sync_reuses_saved_cursor() {
    let home = temp_home();
    seed_workbuddy_fixture(&home);

    let db = Database::open(&home.join("tokenviewer.db")).expect("open test db");

    let first = sync_all(&db, &home);
    assert!(
        first.errors.is_empty(),
        "first sync errors: {:?}",
        first.errors
    );

    let second = sync_all(&db, &home);
    assert!(
        second.errors.is_empty(),
        "second sync errors: {:?}",
        second.errors
    );

    let rows = db
        .aggregate_by_model("2025-01-01T00:00:00Z", "2026-02-01T00:00:00Z")
        .expect("aggregate usage");
    let workbuddy = rows
        .iter()
        .find(|r| r.source == "workbuddy" && r.model == "workbuddy-quota")
        .expect("workbuddy aggregate");

    assert_eq!(workbuddy.total_tokens, 400);
    assert_eq!(workbuddy.input_tokens, 400);
    assert_eq!(workbuddy.conversation_count, 1);
}

fn temp_home() -> PathBuf {
    let mut dir = std::env::temp_dir();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_nanos();
    dir.push(format!(
        "tokenviewer-parser-regressions-{}-{}",
        std::process::id(),
        stamp
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn seed_provider_fixtures(home: &Path) {
    write_text(
        &home.join(platform_path(
            "Library/Application Support/Cursor/User/globalStorage/cursorDiskModel/usage.json",
            ".config/Cursor/User/globalStorage/cursorDiskModel/usage.json",
            "AppData/Roaming/Cursor/User/globalStorage/cursorDiskModel/usage.json",
        )),
        r#"[{"model":"cursor-1","inputTokens":12,"outputTokens":8,"timestamp":"2026-01-01T00:09:00Z"}]"#,
    );

    write_text(
        &home.join(".claude/projects/project-a/session.jsonl"),
        r#"{"timestamp":"2026-01-01T00:05:00Z","message":{"id":"msg_001","model":"claude-3-7-sonnet","usage":{"input_tokens":12,"output_tokens":34,"cache_read_input_tokens":5,"cache_creation_input_tokens":7}}}"#,
    );

    write_text(
        &home.join(".codex/sessions/2026/01/01/rollout-1.jsonl"),
        concat!(
            "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"model_provider\":\"openai\"}}\n",
            "{\"timestamp\":\"2026-01-01T00:01:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"input_tokens\":9,\"output_tokens\":4,\"cached_input_tokens\":1,\"cache_creation_input_tokens\":2,\"reasoning_output_tokens\":3}}}}"
        ),
    );

    write_text(
        &home.join(".code/sessions/2026/01/01/rollout-1.jsonl"),
        concat!(
            "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"model_provider\":\"openai\"}}\n",
            "{\"timestamp\":\"2026-01-01T00:01:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"input_tokens\":9,\"output_tokens\":4,\"cached_input_tokens\":1,\"cache_creation_input_tokens\":2,\"reasoning_output_tokens\":3}}}}"
        ),
    );

    write_text(
        &home.join(".gemini/tmp/test-session/chats/session-1.json"),
        r#"{"messages":[{"tokens":{"input":10,"output":20,"cached":3,"thoughts":4},"model":"gemini-2.5-pro","timestamp":"2026-01-01T00:02:00Z"}]}"#,
    );

    write_text(&home.join(".local/share/opencode/opencode.db"), "");
    init_opencode_db(
        &home.join(".local/share/opencode/opencode.db"),
        "opencode-1",
        r#"{"role":"assistant","modelID":"gpt-4.1-mini","tokens":{"input":10,"output":6,"reasoning":2,"cache":{"read":1,"write":3}},"time":{"completed":1735689720000}}"#,
    );

    write_text(
        &home.join(".openclaw/agents/agent-a/sessions/session.jsonl"),
        r#"{"type":"message","timestamp":"2026-01-01T00:03:00Z","message":{"model":"claude-4","usage":{"input":7,"output":5,"cacheRead":2,"cacheWrite":1}}}"#,
    );

    write_text(&home.join(".hermes/state.db"), "");
    init_hermes_db(&home.join(".hermes/state.db"));

    write_text(
        &home.join(".copilot/otel/export.jsonl"),
        r#"{"id":"resp-1","name":"chat completion","attributes":{"gen_ai.operation.name":"chat","gen_ai.usage.input_tokens":20,"gen_ai.usage.output_tokens":7,"gen_ai.usage.cache_read.input_tokens":3,"gen_ai.usage.cache_write.input_tokens":1,"gen_ai.usage.reasoning.output_tokens":2,"gen_ai.response.model":"gpt-4.1"},"endTime":[1735690140,0]}"#,
    );

    write_text(
        &home.join(".kimi/sessions/session-a/wire.jsonl"),
        r#"{"message":{"type":"StatusUpdate","payload":{"message_id":"kimi-1","timestamp":1735689780,"token_usage":{"input_other":13,"output":17,"input_cache_read":4,"input_cache_creation":2}}},"timestamp":"2026-01-01T00:04:00Z"}"#,
    );

    write_text(
        &home.join(".grok/sessions/session-a/updates.jsonl"),
        r#"{"params":{"_meta":{"totalTokens":40,"agentTimestampMs":1735689840000}},"timestamp":"2026-01-01T00:05:00Z"}"#,
    );

    write_text(
        &home.join(".gemini/antigravity/brain/session-a/transcript.jsonl"),
        r#"{"model":"gemini-2.5-flash","usageMetadata":{"promptTokenCount":4,"candidatesTokenCount":9,"cachedContentTokenCount":1,"totalTokenCount":14}}"#,
    );

    write_text(
        &home.join(platform_path(
            "Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent/dev_data/tokens_generated.jsonl",
            ".config/Kiro/User/globalStorage/kiro.kiroagent/dev_data/tokens_generated.jsonl",
            "AppData/Roaming/Kiro/User/globalStorage/kiro.kiroagent/dev_data/tokens_generated.jsonl",
        )),
        r#"{"model":"agent","provider":"kiro","promptTokens":13,"generatedTokens":7}"#,
    );

    write_text(
        &home.join(platform_path(
            "Library/Application Support/Code/User/globalStorage/rooveterinaryinc.roo-cline/tasks/task-a/ui_messages.json",
            ".config/Code/User/globalStorage/rooveterinaryinc.roo-cline/tasks/task-a/ui_messages.json",
            "AppData/Roaming/Code/User/globalStorage/rooveterinaryinc.roo-cline/tasks/task-a/ui_messages.json",
        )),
        r#"[{"say":"api_req_started","ts":1735689900000,"text":"{\"tokensIn\":10,\"tokensOut\":12,\"cacheReads\":3,\"cacheWrites\":1,\"inferenceProvider\":\"anthropic\"}"}]"#,
    );

    write_text(
        &home.join(platform_path(
            "Library/Application Support/Code/User/globalStorage/kilocode.kilo-code/tasks/task-a/ui_messages.json",
            ".config/Code/User/globalStorage/kilocode.kilo-code/tasks/task-a/ui_messages.json",
            "AppData/Roaming/Code/User/globalStorage/kilocode.kilo-code/tasks/task-a/ui_messages.json",
        )),
        r#"[{"say":"api_req_started","ts":1735689900000,"text":"{\"tokensIn\":10,\"tokensOut\":12,\"cacheReads\":3,\"cacheWrites\":1,\"inferenceProvider\":\"anthropic\"}"}]"#,
    );

    write_text(&home.join(".local/share/kilo/kilo.db"), "");
    init_opencode_db(
        &home.join(".local/share/kilo/kilo.db"),
        "kilocli-1",
        r#"{"role":"assistant","modelID":"gpt-4o-mini","tokens":{"input":10,"output":6,"reasoning":2,"cache":{"read":1,"write":3}},"time":{"completed":1735689720000}}"#,
    );

    write_text(
        &home.join(platform_path(
            "Library/Application Support/Zed/threads/threads.db",
            ".local/share/zed/threads/threads.db",
            "AppData/Roaming/Zed/threads/threads.db",
        )),
        "",
    );
    init_zed_db(&home.join(platform_path(
        "Library/Application Support/Zed/threads/threads.db",
        ".local/share/zed/threads/threads.db",
        "AppData/Roaming/Zed/threads/threads.db",
    )));

    write_text(
        &home.join(platform_path(
            "Library/Application Support/goose/sessions/sessions.db",
            ".local/share/goose/sessions/sessions.db",
            "AppData/Roaming/goose/sessions/sessions.db",
        )),
        "",
    );
    init_goose_db(&home.join(platform_path(
        "Library/Application Support/goose/sessions/sessions.db",
        ".local/share/goose/sessions/sessions.db",
        "AppData/Roaming/goose/sessions/sessions.db",
    )));

    write_text(
        &home.join(".omp/agent/sessions/session-a.jsonl"),
        r#"{"type":"message","id":"omp-1","message":{"role":"assistant","timestamp":1735689960000,"usage":{"input":6,"output":7,"cacheRead":1,"cacheWrite":2,"reasoningTokens":3}}}"#,
    );

    write_text(
        &home.join(".pi/agent/sessions/session-a.jsonl"),
        r#"{"type":"message","id":"pi-1","message":{"role":"assistant","timestamp":1735689960000,"usage":{"input":6,"output":7,"cacheRead":1,"cacheWrite":2,"reasoningTokens":3}}}"#,
    );

    write_text(
        &home.join(".craft-agent/workspaces/work-a/sessions/session-a/session.jsonl"),
        r#"{"tokenUsage":{"inputTokens":8,"outputTokens":9,"cacheReadTokens":2,"cacheCreationTokens":1},"model":"craft-1","lastMessageAt":1735690020000}"#,
    );

    write_text(
        &home.join(".codebuddy/projects/project-a/session.jsonl"),
        r#"{"type":"message","role":"assistant","uuid":"cb-1","providerData":{"model":"codebuddy-2","rawUsage":{"prompt_tokens":20,"completion_tokens":10,"prompt_tokens_details":{"cached_tokens":2},"completion_tokens_details":{"reasoning_tokens":4},"cache_read_input_tokens":1,"cache_creation_input_tokens":3}},"timestamp":1735690080000}"#,
    );

    seed_workbuddy_fixture(home);

    init_mimocode_db(&home.join(".local/share/mimocode/mimocode.db"));

    init_zcode_db(&home.join(".zcode/cli/db/db.sqlite"));
}

fn seed_workbuddy_fixture(home: &Path) {
    write_text(
        &home.join(".antigravity_cockpit/workbuddy_accounts.json"),
        r#"{"version":"1.0","accounts":[{"id":"workbuddy_fixture","email":"wb@example.com","last_used":1735690200}]}"#,
    );

    write_text(
        &home.join(".antigravity_cockpit/workbuddy_accounts/workbuddy_fixture.json"),
        r#"{
  "id": "workbuddy_fixture",
  "email": "wb@example.com",
  "payment_type": "pro",
  "usage_updated_at": 1735690200,
  "usage_raw": {
    "data": {
      "Response": {
        "Data": {
          "Accounts": [
            {
              "PackageCode": "TCACA_code_002_AkiJS3ZHF5",
              "PackageName": "Pro Monthly",
              "Status": 0,
              "CycleStartTime": "2025-12-01 00:00:00",
              "CycleEndTime": "2026-01-31 00:00:00",
              "CycleCapacitySizePrecise": "1000",
              "CycleCapacityRemainPrecise": "600"
            }
          ]
        }
      }
    }
  }
}"#,
    );
}

fn platform_path(macos: &str, linux: &str, windows: &str) -> String {
    if cfg!(target_os = "macos") {
        macos.to_string()
    } else if cfg!(target_os = "windows") {
        windows.to_string()
    } else {
        linux.to_string()
    }
}

fn write_text(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, text).unwrap();
}

fn init_opencode_db(db_path: &Path, id: &str, data_json: &str) {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS message (id TEXT PRIMARY KEY, data TEXT NOT NULL);",
    )
    .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO message (id, data) VALUES (?1, ?2)",
        [id, data_json],
    )
    .unwrap();
}

fn init_hermes_db(db_path: &Path) {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            model TEXT,
            started_at INTEGER NOT NULL,
            ended_at INTEGER,
            input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            cache_read_tokens INTEGER NOT NULL,
            cache_write_tokens INTEGER NOT NULL,
            reasoning_tokens INTEGER NOT NULL
        );
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO sessions (id, model, started_at, ended_at, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params!["hermes-1", "hermes-3", 1735689600_i64, Option::<i64>::Some(1735689660), 11_i64, 4_i64, 3_i64, 2_i64, 5_i64],
    )
    .unwrap();
}
fn init_zed_db(db_path: &Path) {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS threads (
            id TEXT PRIMARY KEY,
            updated_at TEXT NOT NULL,
            data_type TEXT,
            data BLOB NOT NULL
        );
        "#,
    )
    .unwrap();
    let payload = r#"{"model":{"provider":"zed.dev","model":"claude-opus-4.1"},"cumulative_token_usage":{"input_tokens":5,"output_tokens":6,"cache_read_input_tokens":1,"cache_creation_input_tokens":2}}"#;
    conn.execute(
        "INSERT OR REPLACE INTO threads (id, updated_at, data_type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            "zed-1",
            "2026-01-01T00:11:00Z",
            "chat",
            payload.as_bytes().to_vec()
        ],
    )
    .unwrap();
}

fn init_goose_db(db_path: &Path) {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            model_config_json TEXT,
            created_at TEXT NOT NULL,
            total_tokens INTEGER,
            input_tokens INTEGER,
            output_tokens INTEGER,
            accumulated_total_tokens INTEGER,
            accumulated_input_tokens INTEGER,
            accumulated_output_tokens INTEGER
        );
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO sessions (id, model_config_json, created_at, total_tokens, input_tokens, output_tokens, accumulated_total_tokens, accumulated_input_tokens, accumulated_output_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            "goose-1",
            r#"{"model_name":"gpt-5"}"#,
            "2026-01-01T00:12:00Z",
            0_i64,
            11_i64,
            9_i64,
            25_i64,
            11_i64,
            9_i64
        ],
    )
    .unwrap();
}

fn init_mimocode_db(db_path: &Path) {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS message (
            id TEXT PRIMARY KEY,
            time_created INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        "#,
    )
    .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO message (id, time_created, data) VALUES (?1, ?2, ?3)",
        rusqlite::params![
            "mimocode-1",
            1735689720000_i64,
            r#"{"role":"assistant","modelID":"mimo-auto","tokens":{"input":10,"output":6,"reasoning":2,"cache":{"read":1,"write":3}}}"#
        ],
    )
    .unwrap();
}

fn init_zcode_db(db_path: &Path) {
    init_zcode_schema(db_path);
    insert_zcode_row(
        db_path,
        "zcode-1",
        "req-zcode-1",
        "sess-zcode-1",
        "main_turn",
        "builtin:bigmodel-start-plan",
        "GLM-5.2",
        "completed",
        1735689600000_i64, // 2025-01-01 00:00:00 UTC (epoch ms)
        10,
        6,
        2,
        3,
        1,
    );
    // …plus a zero-token error row in the same bucket, which the parser must skip.
    insert_zcode_row(
        db_path,
        "zcode-2",
        "req-zcode-2",
        "sess-zcode-1",
        "main_turn",
        "builtin:bigmodel-start-plan",
        "GLM-5.2",
        "error",
        1735689600000_i64,
        0,
        0,
        0,
        0,
        0,
    );
}

fn init_zcode_schema(db_path: &Path) {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let conn = Connection::open(db_path).unwrap();
    // Faithful subset of the real `model_usage` schema — the columns the zcode
    // parser reads plus the NOT-NULL-no-default ones. FKs are off by default in
    // rusqlite, so no `session` row is required.
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS model_usage (
            id TEXT PRIMARY KEY,
            logical_request_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            query_source TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            model_id TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('running','completed','error','cancelled')),
            started_at INTEGER NOT NULL,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            reasoning_tokens INTEGER NOT NULL DEFAULT 0,
            cache_creation_input_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_input_tokens INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )
    .unwrap();
}

fn insert_zcode_row(
    db_path: &Path,
    id: &str,
    logical_request_id: &str,
    session_id: &str,
    query_source: &str,
    provider_id: &str,
    model_id: &str,
    status: &str,
    started_at: i64,
    input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    cache_creation_input_tokens: i64,
    cache_read_input_tokens: i64,
) {
    let conn = Connection::open(db_path).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO model_usage \
         (id, logical_request_id, session_id, query_source, provider_id, model_id, status, started_at, \
          input_tokens, output_tokens, reasoning_tokens, cache_creation_input_tokens, cache_read_input_tokens) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            id,
            logical_request_id,
            session_id,
            query_source,
            provider_id,
            model_id,
            status,
            started_at,
            input_tokens,
            output_tokens,
            reasoning_tokens,
            cache_creation_input_tokens,
            cache_read_input_tokens,
        ],
    )
    .unwrap();
}

fn update_zcode_row_status(db_path: &Path, id: &str, status: &str) {
    let conn = Connection::open(db_path).unwrap();
    conn.execute(
        "UPDATE model_usage SET status = ?1 WHERE id = ?2",
        rusqlite::params![status, id],
    )
    .unwrap();
}
