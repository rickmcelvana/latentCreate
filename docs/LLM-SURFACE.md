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

## 12. Lyric generation -- verified 2026-08-25

Captured against the same install as section 11 (Ollama 0.32.15, `gemma4:12b-32k`), with an
assembled system prompt of the shape ARCHITECTURE 6 specifies and a filled-in brief
(theme, style tags, mood, `V-C-V-C-B-C`, language, POV, duration). Section 11 measured the
*test call*; this section measures the thing the phase is actually built on.

### 12.1 WARNING A full song is 99% chain-of-thought by default

One generation, `max_tokens: 2000`, nothing else set:

| measure | value |
|---|---|
| `finish_reason` | `length` |
| `completion_tokens` | 2000 (the entire budget) |
| content | **85 characters** |
| reasoning | **7458 characters** |
| first content delta | **44.08 s** into a 44.65 s stream |

The song was cut off eight words in, and for **44 of the 45 seconds the document stayed
empty**. Three consequences, none of which survive being guessed at:

- **A "generous" token budget is not a budget for lyrics.** 2000 tokens bought no song. The
  budget cannot be sized against the length of a song, because the song is not what the
  tokens are spent on.
- **`ChatDelta::Reasoning` is the only proof of life for most of the stream.** T-108 typed it
  so it could be kept *out* of the document; the lyrics UI must additionally render it *as
  status*, or a working generation is indistinguishable from a hung one for 44 seconds.
- **`finish_reason: length` is a first-class outcome**, not a detail. Saving that 85-character
  fragment as version 1 without saying it was truncated is the bug this section exists to
  prevent.

### 12.2 WARNING `think: false` is accepted and ignored; `reasoning_effort: "none"` works

Same prompt (a four-line chorus), `max_tokens: 300`, one variable changed at a time:

| request field | elapsed | `finish_reason` | content | reasoning |
|---|---|---|---|---|
| *(none)* | 6.30 s | `length` | 0 chars | 1039 chars |
| `"think": false` | 6.01 s | `length` | 0 chars | 1034 chars |
| `"reasoning_effort": "low"` | 5.97 s | `length` | 0 chars | 1034 chars |
| `"reasoning_effort": "none"` | **0.95 s** | **`stop`** | **176 chars** | **0 chars** |

`think` is Ollama's own native-API switch, and over the OpenAI-compatible endpoint it is
**silently dropped** -- no error, no warning, byte-for-byte the baseline behaviour. A client
that sets it believes thinking is off while paying for every thinking token. `"low"` is
likewise not honoured by this model. Only `"none"` flips it, and when it does the same
request is **6.6x faster and actually answers**.

### 12.3 The same song with reasoning off

`max_tokens: 2000`, `reasoning_effort: "none"`, two runs:

| measure | run 1 | run 2 |
|---|---|---|
| elapsed | 8.80 s | 8.18 s |
| first content delta | 0.40 s | 0.53 s |
| `finish_reason` | `stop` | `stop` |
| `completion_tokens` | 422 | 383 |
| content | 1767 chars | 1490 chars |
| reasoning | 0 chars | 0 chars |

**A complete `V-C-V-C-B-C` song is roughly 400 completion tokens.** With reasoning on, five
times that budget produced nothing usable.

**The policy this supports, and its limit.** `reasoning_effort` is verified against Ollama
only. Whether an arbitrary OpenAI-compatible server ignores an unknown field (as Ollama does
with `think`) or rejects the request is **not verified** -- LM Studio, llama.cpp and vLLM were
not tested. So the field is sent only when the model is *known* to think, and that fact has
exactly one source: the Ollama enrichment layer's `thinks` flag, already collected by the
wizard's LLM step (section 11.1). Against any endpoint the app cannot enrich, capabilities are
unknown, `thinks` is null, and nothing extra is sent. The unverified path is therefore never
taken rather than defended.

### 12.4 The model breaks the profile's own lyric rule, reliably

`ace-step-1.5-turbo`'s `lyrics_contract.notes` says vocal-style cues belong in tags, not
lyrics, and the assembled system prompt said so in as many words. Every capture put them in
the lyrics anyway:

```
[inst]
[Driving synthwave bassline enters, heavy rever_b, 80s gated drums]

[Verse]
Packed the shadows in a cardboard box
...
[Bridge]
[Vocal style: ethereal, airy]
```

Those bracketed lines are not structure tags; ACE-Step will read them as words to sing.
One capture also dropped a Hangul character into an English lyric. **The structure-tag
validator is therefore load-bearing, not decorative** -- the system prompt demonstrably
does not enforce the contract, so something after generation has to notice. What it must
notice is a bracketed token that is *not* a structure tag, which is a different check from
"are the required sections present".

### 12.5 WARNING Forbidding a behaviour in the prompt does not stop it

The assembled prompt of section 12 was run against `gemma4:12b-32k` **14 times** while
writing T-202, with `reasoning_effort: "none"` and the default brief, counting the
bracketed blocks that are *not* one of the profile's declared structure tags -- the
production and vocal-style directions the profile's own `lyrics_contract` says do not
belong in lyrics (section 12.4).

Three prompt variants, same brief, same budget, same model:

| variant | worked example | "do not write production directions" rule | runs | stray direction blocks (mean) |
|---|---|---|---|---|
| A | yes | **no** | 8 | **3.4** |
| B | no | **no** | 8 | 5.0 |
| C | yes/no | **yes** | 6 | 5.7 |

**The rule that forbids the behaviour is followed by the runs with the most of it.**
Per-run counts ranged 0 to 10 on identical prompts, so the ordering between A and B is
inside the noise -- but the rule never helped in any grouping, and adding it moved the
mean the wrong way. Naming the forbidden thing appears to prime it.

Two conclusions, both acted on in T-202:

- **The prompt does not carry an anti-direction rule**, and `create-core::lyrics` has a
  test whose only job is to stop one being re-added on intuition. The profile's own
  `lyrics_contract` note stays, because it is the profile author's words about their
  model rather than an instruction this app invented.
- **The lint is the only defence that works** (T-203). No prompt variant got stray
  directions to zero, and most runs had several.

One behaviour *was* stable across all 14 runs, and it is what structure linting should
be built on:

**Counted over all 13 saved generations** (`testdata/lyrics/` holds two of them), against
the default `V-C-V-C-B-C` brief and the ACE-Step profile:

| what | result |
|---|---|
| requested order correct (as a subsequence) | **13 of 13** |
| extra song sections beyond the six requested | **1 in 9 files, 0 in the other 4** |
| what the extra section always was | **`[Outro]`**, in every one of the 9 |
| `[inst]` markers | 0 to 3 per file, in 7 of 13 |
| stray direction blocks | 46 in total; 10 of 13 files had at least one, worst file 8 |
| declared tags written in a form other than the profile's | **0 of 99** |

So **"the requested sections appear, in order" is a check a lyric can pass** -- every real
generation passed it. **"and nothing else" is a check most lyrics fail**, over one
`[Outro]` the user very likely wants, so it is information rather than a finding.

The last row is worth its own note: the model writes the profile's tags exactly as the
prompt lists them, never numbered. **Numbering tolerance in the lint is therefore for the
user's own text, not the model's** -- the shipped ACE-Step template writes `[Verse 1]`
(MCP-SURFACE 15.2), so a lyric pasted from there, or typed by a songwriter out of habit,
is the case that needs it.

## 13. The first non-Ollama endpoint -- observed 2026-08-27

Every capture above section 12 was taken against Ollama, local or its cloud tags. T-301b gave
the wizard an endpoint field, and the first hosted OpenAI-compatible endpoint was connected
the same day. **This is a producer click-through, not an instrumented run** -- no timings or
token counts were taken, so what follows is recorded as observation, and section 13.1 is the
measurement it argues for.

Endpoint: a hosted OpenAI-compatible API (provider not yet recorded here). Model id
`qwen3.8-flash`. What happened:

- **`/v1/models` listed the catalogue and the test call returned.** The step's whole path
  works against something that is not Ollama, which had never been exercised before.
- **The model reasoned at length**, and the reasoning was rendered as status text above the
  lyric editor -- the proof-of-life behaviour from the 2026-08-25 decision, working as
  intended against a provider it was never tested on.
- **The lyrics arrived clean in the editor.** No reasoning text leaked into the document.

**The typed-delta split held against a second provider, and that is the finding.** `ChatDelta`
separates `Content` from `Reasoning` (section 2), and both spellings are read because clients
that know only one have shipped the bug of dropping the other (section 3). Until now that was
a rule justified by one vendor's wire format. A different vendor's stream fed the same code
path and the user's document received only content. **Which spelling this endpoint used was
not captured** -- it works either way, which is the point, but 13.1 should record it.

### 13.1 MEASURED 2026-08-27 -- QwenCloud honours `reasoning_effort: "none"`

Provider: **QwenCloud** (Alibaba DashScope international,
`https://dashscope-intl.aliyuncs.com/compatible-mode/v1`), model `qwen3.8-flash`. Same
assembled lyric brief, same 1260-token budget, one run each, back to back. Harness:
`cargo test -p app -- --ignored reasoning_effort --nocapture`.

| | A: no field (**what the app sends today**) | B: `reasoning_effort: "none"` |
|---|---|---|
| Total | **35.03 s** | **4.37 s** |
| First content delta | **33.12 s** | **1.13 s** |
| Lyric characters | 1031 | 910 |
| Reasoning characters | **9419** | **0** |
| Completion tokens | **2771** | **235** |
| `finish_reason` | `stop` | `stop` |

**It is honoured** -- the first of the three possibilities 13.1 previously listed, and the
consequential one. **29x faster to the first character, 8x faster overall, and 11.8x fewer
completion tokens.**

⚠ **On a hosted endpoint the current rule costs money, not just time.** Every lyric
generation against QwenCloud bills 2771 completion tokens where 235 would do, and produces a
song no worse for it. On Ollama the same rule cost only patience (12.1). That is a different
argument than the one the rule was written against.

Both runs returned a complete, structured song, so this is not a quality trade -- B's lyric
is 910 characters against A's 1031, and both stopped cleanly.

*Unexplained, recorded rather than theorised:* `prompt_tokens` differed between the two runs
(334 for A, 298 for B) on identical messages. Something server-side varies with the field;
nothing in this app changed.

### 13.2 The spelling is `reasoning_content`, and that rule just earned its keep

The raw SSE carries **`reasoning_content`**, never `reasoning`. Usage frame present, as
section 5 describes.

`reasoning_content` is the DeepSeek/older-vLLM spelling; every capture before today used
Ollama's `reasoning`. **A client reading only `reasoning` would have decoded none of those
9419 characters** -- not as an error, but as 33 seconds of a stream that appears to carry
nothing, which is precisely the "indistinguishable from a hang" failure the 2026-08-25
proof-of-life decision named. The 2026-08-24 rule to read both spellings was written from
documentation, defensively, with no provider in hand that needed it. **The first hosted
endpoint this app ever connected to needed it.**

### 13.3 MEASURED -- what a rejection looks like, and what gets silently ignored

Probed against QwenCloud, tiny prompts, `max_tokens: 16`. Harness:
`cargo test -p app -- --ignored rejection_shapes --nocapture`.

| Sent | Result |
|---|---|
| `reasoning_effort: "none"`, reasoning model | **200** |
| `reasoning_effort: "banana"` (invalid value) | **400** |
| `latentcreate_not_a_real_param: true` (unknown field) | **200** -- silently ignored |
| `reasoning_effort: "none"`, older `qwen3.5-27b` | **200**, *and it reasoned anyway* |
| `reasoning_effort: "none"`, `deepseek-v3.2` (another vendor, same gateway) | **200** |
| any field, `qwen3.7-text-embedding` | **404** `model_not_supported` -- unrelated to the field |

**A rejection is a 400 naming the field**, and it is precise:

```json
{"error":{"message":"'reasoning_effort' must be one of: 'none', 'minimal', 'low',
 'medium', 'high', 'xhigh', 'max'","type":"invalid_request_error","param":null,
 "code":"invalid_parameter_error"}}
```

`code: "invalid_parameter_error"` with the field name in `message`. That the endpoint
validates the *value* is itself the proof it genuinely supports the field, rather than
tolerating it.

⚠ **An unknown parameter is accepted and ignored, not rejected.** `latentcreate_not_a_real_param`
came back 200. So the failure this app has been guarding against -- an endpoint erroring on
`reasoning_effort` because it does not know it -- **is not how at least this gateway behaves**.
The 400 above was for an invalid *value* of a *known* field, which is a different thing.
The guarded-against case remains possible on a stricter endpoint; it is no longer the
expected one.

### 13.4 WARNING Acceptance is per endpoint; **honouring is per model**

The finding that shapes any fix. On **one** endpoint, with the **same** field:

- `qwen3.8-flash` -- **honoured**. Zero reasoning characters, 235 completion tokens (13.1).
- `qwen3.5-27b` -- **accepted and ignored**. HTTP 200, and the response carried
  `reasoning_content` regardless: it thought anyway.

So "does this endpoint accept the field" and "will this model stop reasoning" are two
questions, and only the first is worth asking. **Accepted-but-ignored costs nothing** -- the
request succeeds and behaviour is what it would have been. The only outcome that hurts is a
rejection, and that is detectable by 13.3's shape.

Consequence for the design: a probe needs to answer **one** question -- *does sending this
field make the request fail?* -- and its answer is per **endpoint**, not per model. Trying to
discover which models honour it would be a much larger surface for no benefit, since sending
it to a model that ignores it is free.

Also captured: usage carries `completion_tokens_details.reasoning_tokens` (41 on a
16-token greeting), which is a cheaper measure of thinking than counting delta characters.
