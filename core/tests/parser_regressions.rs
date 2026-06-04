use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use tokenviewer_core::parsers::{codex, parse_all};

#[derive(Debug, Clone)]
struct Expectation {
    model: &'static str,
    total_tokens: u64,
}

#[test]
fn provider_parser_matrix_matches_reference_shapes() {
    let home = temp_home();
    seed_provider_fixtures(&home);

    let cursors = HashMap::new();
    let results = parse_all(&home, &cursors);
    let by_source: HashMap<String, _> = results.into_iter().map(|r| (r.source.clone(), r)).collect();

    let cases: [(&str, Expectation); 22] = [
        ("claude", Expectation { model: "claude-3-7-sonnet", total_tokens: 58 }),
        ("codex", Expectation { model: "openai", total_tokens: 18 }),
        ("cursor", Expectation { model: "cursor-1", total_tokens: 20 }),
        ("gemini", Expectation { model: "gemini-2.5-pro", total_tokens: 37 }),
        ("kiro", Expectation { model: "kiro-agent", total_tokens: 20 }),
        ("opencode", Expectation { model: "gpt-4.1-mini", total_tokens: 22 }),
        ("openclaw", Expectation { model: "claude-4", total_tokens: 15 }),
        ("everycode", Expectation { model: "openai", total_tokens: 18 }),
        ("hermes", Expectation { model: "hermes-3", total_tokens: 25 }),
        ("copilot", Expectation { model: "gpt-4.1", total_tokens: 30 }),
        ("kimi", Expectation { model: "kimi-for-coding", total_tokens: 36 }),
        ("grok", Expectation { model: "grok-build", total_tokens: 40 }),
        ("antigravity", Expectation { model: "gemini-2.5-flash", total_tokens: 14 }),
        ("roocode", Expectation { model: "provider:anthropic", total_tokens: 26 }),
        ("kilocode", Expectation { model: "provider:anthropic", total_tokens: 26 }),
        ("kilocli", Expectation { model: "gpt-4o-mini", total_tokens: 22 }),
        ("zed", Expectation { model: "claude-opus-4.1", total_tokens: 14 }),
        ("goose", Expectation { model: "gpt-5", total_tokens: 25 }),
        ("ohmypi", Expectation { model: "omp-unknown", total_tokens: 19 }),
        ("pi", Expectation { model: "pi-unknown", total_tokens: 19 }),
        ("craft", Expectation { model: "craft-1", total_tokens: 20 }),
        ("codebuddy", Expectation { model: "codebuddy-2", total_tokens: 37 }),
    ];

    assert_eq!(by_source.len(), cases.len(), "expected one result per registered provider");

    for (source, expected) in cases {
        let result = by_source.get(source).unwrap_or_else(|| panic!("missing parse result for {source}"));
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
        assert_eq!(record.source, expected_record_source, "source mismatch for {source}");
        assert_eq!(record.model, expected.model, "model mismatch for {source}");
        assert_eq!(record.total_tokens, expected.total_tokens, "token mismatch for {source}");
        assert_eq!(record.conversation_count, 1, "conversation count mismatch for {source}");
    }
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

fn temp_home() -> PathBuf {
    let mut dir = std::env::temp_dir();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_nanos();
    dir.push(format!("tokenviewer-parser-regressions-{}-{}", std::process::id(), stamp));
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
        r#"{"timestamp":"2026-01-01T00:05:00Z","message":{"model":"claude-3-7-sonnet","usage":{"input_tokens":12,"output_tokens":34,"cache_read_input_tokens":5,"cache_creation_input_tokens":7}}}"#,
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

    write_text(
        &home.join(".local/share/opencode/opencode.db"),
        "",
    );
    init_opencode_db(
        &home.join(".local/share/opencode/opencode.db"),
        "opencode-1",
        r#"{"role":"assistant","modelID":"gpt-4.1-mini","tokens":{"input":10,"output":6,"reasoning":2,"cache":{"read":1,"write":3}},"time":{"completed":1735689720000}}"#,
    );

    write_text(
        &home.join(".openclaw/agents/agent-a/sessions/session.jsonl"),
        r#"{"type":"message","timestamp":"2026-01-01T00:03:00Z","message":{"model":"claude-4","usage":{"input":7,"output":5,"cacheRead":2,"cacheWrite":1}}}"#,
    );

    write_text(
        &home.join(".hermes/state.db"),
        "",
    );
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

    write_text(
        &home.join(".local/share/kilo/kilo.db"),
        "",
    );
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
        r#"{"type":"message","role":"assistant","uuid":"cb-1","providerData":{"model":"codebuddy-2","rawUsage":{"prompt_tokens":20,"completion_tokens":10,"prompt_tokens_details":{"cached_tokens":2,"reasoning_tokens":4},"cache_read_input_tokens":1,"cache_creation_input_tokens":3}},"timestamp":1735690080000}"#,
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
        rusqlite::params!["zed-1", "2026-01-01T00:11:00Z", "chat", payload.as_bytes().to_vec()],
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
