# LLM-SURFACE.md — verified LLM wire formats

**Status: verified live 2026-08-24** against Ollama 0.32.15 on `127.0.0.1:11434` — both the
OpenAI-compatible surface (1-7) and Ollama's own API (8-9) — cross-checked
against the OpenAI Python SDK's own chunk type. Authoritative over model documentation and
over docs/RESEARCH.md, exactly as [MCP-SURFACE.md](MCP-SURFACE.md) is for comfy-mcp. Read
before touching `llm-bridge`.

Raw captures live in [testdata/llm/](../testdata/llm/) and are replayed in the crate's
tests, so nothing in CI needs a running model.

**How this was verified.** `curl` against a live endpoint for every shape below;
`cargo metadata` for licences; the OpenAI SDK source for the canonical field set. The
streaming client was then written, compiled, and run against the live endpoint before any
of it entered a brief — `test_live_stream_returns_content_separated_from_reasoning`
(`cargo test -p llm-bridge -- --ignored`) is that check, kept as a permanent live smoke
test rather than thrown away.

---

## 1. `GET /v1/models` — the model list

```json
{"object":"list","data":[
  {"id":"kimi-k3:cloud","object":"model","created":1787613495,"owned_by":"library"},
  {"id":"gemma4:12b-32k","object":"model","created":1784893439,"owned_by":"library"}]}
```

Only `id` is worth reading. `created` is a **file mtime on local servers**, not a release
date, so it must not be presented as one. `owned_by` is `"library"` for every local model.

The wizard's model picker (T-112) reads this list; wire order is Ollama's recency order,
which is a more useful default than alphabetical.

---

## 2. ⚠ `delta.reasoning` — the finding that shapes the whole crate

**A model this app recommends for lyrics spends most of its output on chain-of-thought,
in a field OpenAI does not document.**

Prompt: `Reply with exactly: tulip`. Model: `gemma4:12b-it-qat`. Result:

| | characters |
|---|---|
| `delta.reasoning` | **163** |
| `delta.content` | **5** (`tulip`) |

Frames look like this — note `"content":""` sitting *beside* a non-empty `reasoning`:

```json
{"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":"The"},"finish_reason":null}]}
{"choices":[{"index":0,"delta":{"content":"","reasoning":" user"},"finish_reason":null}]}
...
{"choices":[{"index":0,"delta":{"content":"tul"},"finish_reason":null}]}
{"choices":[{"index":0,"delta":{"content":"ip"},"finish_reason":null}]}
{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}
```

Three consequences, all of them correctness rather than polish:

1. **Reasoning must never be appended to lyrics.** A client that concatenates every text
   field puts 163 characters of the model's deliberation into the user's song.
2. **Reasoning must not be discarded either.** For the 40 frames before content starts,
   a client that only watches `content` shows a frozen UI on a working stream. It is
   status text — "thinking…" — not output.
3. **Presence is not text.** `"content":""` is sent on nearly every frame, so
   `delta.content.is_some()` is true throughout a stream carrying no content at all.

This is not limited to models that advertise reasoning: `gemma4:12b-it-qat`, an ordinary
instruct model, does it, and it is the model docs/MODELS.md recommends for lyric writing.

---

## 3. ⚠ Two spellings: `reasoning` and `reasoning_content`

The ecosystem is split, and a client that knows only one silently drops the thinking
stream on half of it:

| Field | Used by |
|---|---|
| `reasoning` | Ollama, OpenRouter, current vLLM |
| `reasoning_content` | DeepSeek, older vLLM |

Neither is in OpenAI's own `ChoiceDelta`, whose documented fields are `content`, `role`,
`refusal`, `tool_calls`, `function_call`. Both must be read and merged. Real clients have
shipped this bug — see the vLLM/litellm/strands-agents issues linked from the session log.

**`refusal` matters for the same reason:** when a model declines, the text lands there and
`content` stays empty. A client watching only `content` shows a blank answer with no
explanation.

---

## 4. Errors — and one that is not JSON

| Case | Status | Body |
|---|---|---|
| Unknown model | 404 | `{"error":{"message":"model 'no-such-model:99b' not found","type":"not_found_error","param":null,"code":null}}` |
| Missing `messages` | 400 | `{"error":{"message":"[] is too short - 'messages'","type":"invalid_request_error","param":null,"code":null}}` |
| **Wrong path** (base URL without `/v1`) | 404 | `404 page not found` — **plain text** |

The envelope matches OpenAI's documented shape; `param` and `code` were null on every
capture. But the third row is the trap: **an error body is not necessarily JSON.** A user
who pastes `http://127.0.0.1:11434` instead of `.../v1` gets plain text, and a client that
insists on the envelope reports "expected value at line 1 column 1" instead of the one
sentence that would let them fix it. Decode the envelope, fall back to the raw body.

---

## 5. `stream_options.include_usage` — and the frame with no choices

Requesting usage appends one final frame **before** `[DONE]`:

```json
{"id":"chatcmpl-221","object":"chat.completion.chunk","model":"gemma4:12b-it-qat",
 "choices":[],"usage":{"prompt_tokens":24,"completion_tokens":10,"total_tokens":34}}
```

`choices` is `[]`. The OpenAI SDK types it `List[Choice]` with no guarantee of length, so
this is spec-conformant rather than an Ollama quirk — and `chunk.choices[0]` fails on the
last frame of every metered stream. `usage` is `Optional`; it is absent unless requested.

`finish_reason` (`"stop"`, `"length"`, `"content_filter"`, `"tool_calls"`,
`"function_call"`) arrives on its own frame whose `delta` is `{}`.

**Budget note:** `max_tokens` counts reasoning tokens. A 10-token budget on the capture
above was spent entirely on chain-of-thought and returned `finish_reason: "length"` with
zero content — so a lyrics request needs headroom for thinking, not just for lyrics.

---

## 6. SSE framing

Standard `text/event-stream`. What the decoder must handle, none of it optional:

- **Events split across reads.** A frame arrives in as many TCP segments as the network
  feels like; the parser buffers until a blank line.
- **Multi-byte characters split across reads.** Buffer **bytes**, decode UTF-8 only once a
  whole event has arrived. Lyrics are written in 50+ languages; decoding each read on its
  own corrupts exactly those characters.
- **Comment heartbeats.** Lines beginning `:` — OpenRouter sends `: OPENROUTER PROCESSING`
  while a model warms up. Parsing one as JSON fails the whole stream.
- **`\r\n\r\n` as well as `\n\n`.** Proxies rewrite line endings; a decoder that knows only
  `\n\n` buffers the entire response and emits nothing.
- **Repeated `data:` lines** join with a newline, per the SSE spec.
- **`data: [DONE]`** terminates an OpenAI-style stream. It is not JSON.

---

## 7. Dependencies added, with licences

`cargo metadata`, not memory. All permissive; nothing copyleft (CONVENTIONS).

| Crate | Version | Licence |
|---|---|---|
| `reqwest` | 0.13.4 | MIT OR Apache-2.0 |
| `rustls` | 0.23.43 | Apache-2.0 OR ISC OR MIT |
| `rustls-native-certs` | (via `rustls` feature) | Apache-2.0 OR ISC OR MIT |
| `hyper-rustls` | 0.27.9 | Apache-2.0 OR ISC OR MIT |
| `aws-lc-rs` | 1.18.0 | ISC AND (Apache-2.0 OR ISC) |
| `ring` | 0.17.14 | Apache-2.0 AND ISC |
| `bytes` | 1.12.1 | MIT |
| `futures-core` / `futures-util` | 0.3.34 | MIT OR Apache-2.0 |

⚠ **reqwest 0.13 renamed its TLS features.** There is no `rustls-tls` or
`rustls-tls-native-roots`; the feature is plain **`rustls`**, and it pulls
`rustls-native-certs`, so the OS trust store is used rather than a bundled root list. The
0.12-era names fail to resolve outright — caught by compiling, which is the only reason
this file does not repeat them.

No OpenSSL enters the tree, so Linux CI needs no `libssl-dev`.

---

## 8. Ollama's native API — what the OpenAI shape cannot say

Verified against **Ollama 0.32.15**, 2026-08-24. Fixture:
[testdata/llm/ollama-tags.json](../testdata/llm/ollama-tags.json).

`GET /api/tags` returns, per model, what `/v1/models` has nowhere to put:

```json
{"name":"gemma4:12b-it-qat","size":7151003754,
 "details":{"family":"gemma4","families":["gemma4"],"parameter_size":"11.9B",
            "quantization_level":"Q4_0","context_length":262144},
 "capabilities":["completion","tools","thinking","vision"]}
```

**`capabilities` is the payload.** Three facts follow from it that the app cannot get
otherwise:

1. **`completion` marks a model that can chat at all.** `nomic-embed-text` reports
   `["embedding"]` and nothing else — yet `/v1/models` lists it identically to a chat
   model. Without this, an embedding model sits in the lyric picker and fails only at
   generation time.
2. **`thinking` marks a model that will emit `delta.reasoning`** (section 2). It is the
   only way to know *before* generating that part of the token budget goes to
   chain-of-thought. On this install every completion model had it, including both
   `gemma4:12b` variants.
3. `tools` / `vision` / `audio` are listed too, unused by the lyric flow.

**⚠ `remote_host` is a privacy fact.** Present only on cloud entries
(`"https://ollama.com"`), it means generation happens on someone else's hardware — the
user's unreleased lyrics leave the machine. The UI must say so wherever a model is chosen.
Their `size` is a **stub manifest** (308 bytes for a 2.81T model), so it must never be
shown as disk usage.

**Three decode traps, all captured:**

| Trap | Why it bites |
|---|---|
| `families` arrives as JSON **`null`** on cloud entries | `#[serde(default)]` on `Vec<String>` rejects an explicit null, so the entire model list fails to decode the moment a user signs in to Ollama's cloud. Needs `Option<Vec<String>>`. |
| `parameter_size` is **not normalised** | One install reported `"1t"`, `"1T"`, `"756b"`, `"2.81T"` — and `""` for one model. A label to display, never a number to sort or parse. |
| `parent_model`, `format`, `family` are **empty strings** on cloud entries | Absent-vs-empty is not distinguished; treat empty as unknown. |

**`GET /api/show` is not for lists.** It returns **68 KB for one model** — a 667-entry
tensor manifest, 43 `model_info` keys, and the full 10 KB licence text — against **5.7 KB
for all twelve models** from `/api/tags`. Building a picker from `/api/show` would be
~825 KB of JSON for one screen. It is a details panel call, whose real value is `license`
(CONVENTIONS requires per-model licence terms wherever a model is chosen) and `requires`,
the minimum Ollama version.

`GET /api/version` returns `{"version":"0.32.15"}`, and doubles as the probe for "is this
actually Ollama" — LM Studio and vLLM also serve `/v1` but answer this with a 404.

`GET /api/ps` lists loaded models; it returned `{"models":[]}` with nothing warm, and its
loaded shape was **not captured**.

---

## 9. `POST /api/pull` — NDJSON, and a failure that returns 200

Verified by pulling a real 46 MB model, 2026-08-24. Full capture:
[testdata/llm/ollama-pull.ndjson](../testdata/llm/ollama-pull.ndjson) (23 frames).

**Different framing from the chat stream:** newline-delimited JSON, no `data:` prefix, no
blank-line delimiter. The SSE decoder does not apply; pull needs its own line-based one.

```
{"status":"pulling manifest"}
{"status":"pulling 797b70c4edf8","digest":"sha256:797b...","total":45949216}
{"status":"pulling 797b70c4edf8","digest":"sha256:797b...","total":45949216,"completed":236265}
{"status":"verifying sha256 digest"}
{"status":"writing manifest"}
{"status":"success"}
```

**⚠ A failed pull answers HTTP 200.** The failure is a frame in the body:

```
{"status":"pulling manifest"}
{"error":"pull model manifest: file does not exist"}
```

This is comfy-mcp's `Ok(is_error: true)` in a different protocol (MCP-SURFACE 8): a client
that checks the status code reports a failed download as a success, and the user is left
looking for a model that was never fetched. The error frame carries **no `status` field**.

**`completed` is absent, not zero.** Of 23 frames, 19 carried `digest` and `total` but only
**11** carried `completed` — a layer's first frame arrives before any bytes land. Typing it
`u64` with a serde default reports "0 bytes fetched", which is indistinguishable from a
stalled download. Absent-with-a-total means *started*; no total at all (manifest, verify,
success) means there is nothing to draw a bar for.

Terminal status is `"success"`. **Never call this without the user asking** — models are
gigabytes of someone else's bandwidth and disk (phase-1 T-112).

---

## 10. What is *not* verified

- **OpenAI, Anthropic and OpenRouter proper.** Everything above was captured from Ollama.
  The envelope and chunk shapes match the OpenAI SDK's own types, but no authenticated
  request to a paid endpoint was made, so 401/429 bodies and rate-limit headers are
  **unverified**. Verify before writing a provider that depends on them.
- **`tool_calls` / `function_call` deltas.** Not used by the lyric flow, not captured.
- **`/api/ps` with a model loaded.** Captured empty only.
- **Cancellation mid-pull.** (Pull of an already-installed model *is* verified -- see
  `test_live_pull_of_an_installed_model_reaches_success`, which runs green against
  `all-minilm`.)
- **Non-streaming `POST /v1/chat/completions`** beyond the single capture in section 11.4.
  The app streams, so this path is not exercised in anger.
## 11. The setup wizard's LLM step -- verified 2026-08-25

Captured against Ollama 0.32.15 with 13 models installed (2 embedding-only, 8 remote).

### 11.1 WARNING `/v1/models` carries no capability data at all

The OpenAI-compatible list returns exactly four keys per row:

```json
{ "id": "gemma4:12b-32k", "object": "model", "created": 1787..., "owned_by": "library" }
```

Thirteen ids came back, and **`all-minilm:latest` and `nomic-embed-text:latest` are listed
indistinguishably from chat models**. A picker built on this list offers the user two models
that cannot answer a chat request at all; the failure then surfaces at lyric-generation time,
far from the screen where the choice was made.

`/api/tags` answers the same 13 models **with** a `capabilities` array, which is the entire
reason `ollama_native` exists (section 8). `can_chat()` is `capabilities` containing
`completion`.

### 11.2 WARNING The privacy fact is invisible over the OpenAI-compatible API

Eight of the thirteen run on Ollama's servers. Over `/v1/models` there is **no way to tell** --
the only hint is the `:cloud` suffix in the id, which is a naming convention, not a contract.
`/api/tags` reports `remote_host` (`"https://ollama.com"`, sometimes with an explicit `:443`),
and that is the only reliable signal.

This matters more than the capability filtering. latentCreate's premise is that generation
happens on the user's own hardware; choosing a remote model means **unreleased lyrics leave the
machine**. A wizard that cannot say so is misleading by omission.

**Consequence:** against a non-Ollama OpenAI-compatible endpoint neither fact is knowable. The
UI must say the capabilities are unknown -- it must never present unknown as "local" or as
"can chat".

### 11.3 A missing `/v1` is a 404 with a plain-text body

The likeliest user misconfiguration. `http://127.0.0.1:11434/models` returns HTTP 404 with the
body `404 page not found` -- **not JSON**. T-108's `http_error()` already falls back to the raw
body for exactly this, but the wizard should recognise it and suggest adding `/v1` rather than
relaying a bare 404.

### 11.4 WARNING A thinking model spends the token budget on reasoning first

Section 2 recorded this for generation, where a 10-token budget produced no lyrics at all. It bites the **test call** just as hard, and here are the numbers for it. Asking `gemma4:12b-32k` to "Reply with exactly: ok":

| `max_tokens` | `finish_reason` | `content` | `reasoning` |
|---|---|---|---|
| 20 | `length` | `""` | 68 chars, truncated |
| 400 | `stop` | `"ok"` | 108 chars |

A budget that looks generous for a two-letter answer is consumed entirely by chain-of-thought,
and the endpoint returns **empty content on a perfectly healthy connection**. A test call that
asserts non-empty content reports failure on a working setup.

**Therefore the test call's success criterion is a well-formed response, not non-empty
content.** Reasoning-only proves the endpoint answers. Round trip was 0.75 s with the model
already resident; a cold model must load first, so the UI needs a spinner and a generous
timeout, not a fast failure.

Note also that the non-streaming response carries `reasoning` on `message`, the same split the
streaming deltas use (sections 2 and 3).

### 11.5 Recommendation matching cannot be equality

`docs/MODELS.md` suggests Gemma 4 12B. This install has **two** of them --
`gemma4:12b-32k` and `gemma4:12b-it-qat` -- and **neither is named `gemma4:12b`**. Matching is
by prefix on the id, and because more than one can match, the preselect must be deterministic
(lowest id wins) and must never override a model the user has already chosen.
