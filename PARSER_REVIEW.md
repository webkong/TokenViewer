# Parser Review — Alignment with Original TokenViewer (rollout.js)

Reviewed all 22 parsers against the canonical `src/lib/rollout.js`. Most had wrong
paths/schemas/field names and were rewritten. Summary below.

## Rewritten (were incorrect)

| Provider | Bug found | Fix |
|---|---|---|
| **codex** | Read top-level `usage`; real format is `event_msg` → `payload.info.{total,last}_token_usage`. `input` must subtract `cached`. | Event-stream parse, cumulative delta, model from `turn_context`/`session_meta`. ✅ verified 65 records, 4 models |
| **everycode** | Path `.every-code/sessions` | Correct path is `.code/sessions`, source `every-code`, shares codex logic |
| **gemini** | Read API-shape `usageMetadata.promptTokenCount`; real session file is `messages[].tokens.{input,output,cached,thoughts,tool}` | Rewritten with per-file cumulative delta |
| **grok** | 50/50 split, treated each line as delta | 80/20 split, `totalTokens` cumulative high-watermark via `cursor.delta` |
| **codebuddy** | Used Claude `message.usage` format | OpenAI-style `providerData.rawUsage`, `input = prompt_tokens - cacheRead` |
| **opencode** | SQL `SELECT model, input_tokens...` → "no such column" | `SELECT id,data FROM message WHERE json_extract(data,'$.role')='assistant'`, parse `tokens.{input,output,reasoning,cache.{read,write}}` ✅ verified 3 records |
| **kilocli** | Wrong columns | Shares opencode `message`/`data` schema, path `~/.local/share/kilo/kilo.db` |
| **openclaw** | Assumed SQLite `sessions.db` | Actually JSONL `~/.openclaw/agents/**/sessions/*.jsonl`, `message.usage.{input,output,cacheRead,cacheWrite}` |
| **hermes** | SQL `model,input,output,created_at` | Real: `sessions(model,started_at,ended_at,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,reasoning_tokens)`, epoch seconds, cumulative delta |
| **goose** | Wrong table `messages` | `sessions` with `accumulated_*` cols, model from `model_config_json`, reasoning = total−in−out |
| **zed** | Wrong table/path | `threads/threads.db`, BLOB `data` JSON, only `zed.dev` provider, `cumulative_token_usage`, delta |
| **kimi** | `usage.prompt_tokens` | `message.type=="StatusUpdate"` → `payload.token_usage.{input_other,output,input_cache_read,input_cache_creation}` |
| **ohmypi / pi** | `usage.input_tokens/prompt_tokens` | `usage.{input,output,cacheRead,cacheWrite,reasoningTokens}`, dedup by `entry.id` |
| **craft** | Generic session usage | `workspaces/**/sessions/**/session.jsonl` header `tokenUsage.{inputTokens,...}`, cumulative delta |
| **roocode** | `apiMetrics.inputTokens` | `msg.say=="api_req_started"`, parse `msg.text` JSON → `{tokensIn,tokensOut,cacheReads,cacheWrites}` |
| **kilocode** | guessed ext IDs | `kilocode.kilo-code`, shares roocode ui_messages logic |
| **copilot** | nested `resourceSpans` | flat OTEL records, `attributes["gen_ai.usage.*"]`, `input -= cache_read` |

## Already correct / minor

- **claude** — fixed earlier (`message.usage`, family-prefix pricing). ✅ 121 records
- **kiro** — `tokens_generated.jsonl` reader works. ✅ 1 record

## Known limitation

- **cursor** — requires remote API call (`cursor.com/api`) + JWT from `state.vscdb`.
  Account-level, network-dependent. Returns empty (no local token store). Documented
  as out-of-scope for the offline native core.

## Verification (this machine)

`cargo test --test integration`: **4 providers, 190 records, $910.19**
- claude: 121 records, 316M tokens
- codex: 65 records, 436M tokens (gpt-5.5 / gpt-5.4-mini / gpt-5.3-codex / qwen3-coder-next)
- kiro: 1 record, 26M tokens
- opencode: 3 records

Providers without local data on this machine (gemini, grok, kimi, antigravity, etc.)
match the spec but could not be runtime-validated here.

## Infrastructure added

`FileCursor` gained `snapshots: HashMap<String,[u64;5]>` + `delta(key, cur)` for
cumulative-total sources, and `seen_ids` + `mark_seen(id)` for dedup — mirroring
rollout.js's per-session snapshot and seenIds patterns.
