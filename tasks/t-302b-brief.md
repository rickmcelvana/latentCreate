# T-302b: discover whether an endpoint accepts `reasoning_effort`

**Depends:** T-302 | **Crate/dir:** `library`, `src-tauri`, `app`
**Files to modify:**
- `crates/library/src/config.rs`
- `src-tauri/src/llm.rs`
- `src-tauri/src/lyrics.rs`
- `src-tauri/src/optimize.rs`
- `app/src/bridge/config.ts`
- `app/src/bridge/llm.ts`
- `app/src/state/llm.ts`
- `app/src/state/llm.test.ts`
- `app/src/state/config.test.ts`  *(added at review: it builds an `LlmConfig` literal)*
- `app/src/state/lyrics.test.ts`  *(added at review: same)*
- `testdata/wire/loaded-config.json`  *(added at review: the shared wire fixture, asserted by
  `library`'s `test_wire_fixture_matches_current_types` and by `config.test.ts` -- it exists
  to catch exactly the Rust/TS drift this task creates, and it caught it)*

## Goal

Send `reasoning_effort: "none"` to every endpoint **verified to accept it**, instead of only
to endpoints the app happened to be able to enrich. The verification is a differential probe
inside the wizard's existing test call, and its answer is persisted beside the endpoint.

Why it is worth a task: T-302 measured **33.12 s -> 1.13 s** to first content and
**2771 -> 235 completion tokens** on QwenCloud, an endpoint the app never sends the field to.
On a paid endpoint the current rule bills **11.8x** the tokens for a song no better
(LLM-SURFACE 13.1).

## The evidence this design rests on

All from LLM-SURFACE 13.3 and 13.4, captured live 2026-08-27. Read those before starting.

1. **A rejection is a 400 naming the field**, with `code: "invalid_parameter_error"`. It
   exists and has been seen.
2. ⚠ **An unknown parameter is accepted and silently ignored** (a nonsense field returned
   200). The failure this app has been guarding against -- an endpoint erroring because it
   does not recognise `reasoning_effort` -- **is not how that gateway behaves**. The 400 was
   for an invalid *value* of a *known* field. Still possible elsewhere; no longer the
   expected case.
3. ⚠ **Acceptance is per endpoint; honouring is per model.** On one endpoint,
   `qwen3.8-flash` honoured the field and `qwen3.5-27b` accepted it and reasoned anyway.
   **Only acceptance is worth discovering** -- a model that ignores it costs nothing, because
   the request succeeds and behaves as it would have.

So the probe answers exactly one question: **does sending this field make the request fail?**

## Spec

### 1. `LlmConfig` gains the verified answer

In `crates/library/src/config.rs`:

```rust
/// Whether this endpoint accepts `reasoning_effort`, as verified by the
/// wizard's test call. `None` means it has never been probed.
#[serde(default)]
pub accepts_reasoning_effort: Option<bool>,
```

`#[serde(default)]` is required, not decorative: every existing `config.json` predates the
field. Add a test that a config written before this task still loads, with the field `None`.

Mirror it in `app/src/bridge/config.ts`'s `LlmConfig`.

### 2. The probe is differential, never an error-message match

Reference implementation, `rustfmt`-clean, for `src-tauri/src/llm.rs`. Adapt `run_test_call`
to whatever the existing `llm_test` body factors out -- the point is the two attempts, not
the helper's shape:

```rust
/// Does this endpoint accept `reasoning_effort`?
///
/// **A differential test, not an error-message match.** Send the field; if
/// that fails, send the identical request without it. If the second attempt
/// succeeds, the field was the difference and this endpoint rejects it.
///
/// Deliberately not parsing the provider's wording: the rejection observed on
/// QwenCloud is a 400 naming the field (LLM-SURFACE 13.3), but matching on
/// that text is the thing this repo already refuses to do for status
/// classification -- it breaks the first time a message is reworded. Two
/// requests in the failure case is a price paid once, in a wizard.
///
/// WARNING A transient failure on the first attempt that clears on the second
/// is recorded as "rejects". The consequence is that the field is not sent:
/// slower and, on a paid endpoint, dearer -- but nothing breaks, and the user
/// can run the test call again. That is the safe direction for this to fail in.
async fn probe_reasoning_effort(client: &OpenAiCompat, model: &str) -> ProbeOutcome {
    let with = run_test_call(client, model, Some("none".to_string())).await;
    if with.is_ok() {
        return ProbeOutcome {
            accepted: Some(true),
            result: with,
        };
    }
    let without = run_test_call(client, model, None).await;
    ProbeOutcome {
        accepted: if without.is_ok() { Some(false) } else { None },
        result: without,
    }
}
```

**`llm_test` returns the verdict; it does not save it.** Add `accepts_reasoning_effort:
Option<bool>` to `LlmTestResult` and its TS mirror. The command takes no `State` today and
must not start: the frontend owns config writes, through `useConfigStore`.

When both attempts fail, the verdict is `None` -- unknown, not `false`. The endpoint is
simply broken and the test call already reports that; recording a judgement about the field
from a call that never worked would be inventing data, the same rule as `Option<bool>`
capabilities (LLM-SURFACE 11.1).

### 3. Persist it, and clear it when the endpoint changes

The store's `test` action saves the verdict alongside the existing config, using the full
`llm` block per T-301b §2 so nothing is dropped by the shallow merge.

⚠ **`saveEndpoint` must clear `accepts_reasoning_effort` to `null`.** It is a fact about one
endpoint; carrying it to the next one is a stale verified-fact, which is worse than an
unverified one. Same reasoning as the optimizer override being dropped when the brief changes
(PROJECT.md, 2026-08-26): the record must describe the thing it was taken against. Add a test.

### 4. The sending rule

Reference implementation, `rustfmt`-clean, replacing the current `reasoning_effort_for` in
`src-tauri/src/lyrics.rs`:

```rust
/// Whether `reasoning_effort` may be sent, and why.
///
/// `accepts` is the endpoint's verified answer from the wizard's test call
/// (`None` = never probed). `thinks` is the pre-T-302b rule, kept as a
/// fallback so an existing Ollama user who never re-runs the test call does
/// not silently lose the suppression they have today.
///
/// **Acceptance is per endpoint, honouring is per model** (LLM-SURFACE 13.4),
/// and only acceptance matters: a model that ignores the field costs nothing,
/// because the request succeeds and behaves exactly as it would have.
pub(crate) fn reasoning_effort_for(accepts: Option<bool>, thinks: Option<bool>) -> Option<String> {
    match (accepts, thinks) {
        (Some(true), _) => Some("none".to_string()),
        (Some(false), _) => None,
        (None, Some(true)) => Some("none".to_string()),
        (None, _) => None,
    }
}
```

The `thinks` fallback is the part to get right: **a verified `false` beats it**, an absent
verdict falls back to it. Test all six combinations, and name the invariant each protects --
`test_reasoning_effort_is_sent_only_when_the_model_known_to_think` is being replaced and its
successor should say what it now guards.

### 5. Threading the flag to the two call sites

Both `lyrics.rs` and `optimize.rs` build a `ChatRequest` with `reasoning_effort_for(thinks)`
and both already call `configured_llm(config_dir)`, which returns `(base_url, model)`.

Widen it to carry the flag -- a small named struct rather than a third tuple element, because
three anonymous strings at a call site is where the wrong one gets passed:

```rust
pub(crate) struct LyricEndpoint {
    pub base_url: String,
    pub model: String,
    pub accepts_reasoning_effort: Option<bool>,
}
```

Update both callers and the existing `--ignored` measurements in `lyrics.rs`, which call
`reasoning_effort_for` directly.

## Acceptance criteria

- [ ] A `config.json` with no `accepts_reasoning_effort` loads with the field `None`.
- [ ] `reasoning_effort_for` tested across all six `(accepts, thinks)` combinations, with a
      verified `false` beating a `thinks: true`.
- [ ] ⚠ **corrected at review.** As written this asked for the probe to be tested against a
      mock. **`OpenAiCompat` opens a real socket and exposes no injectable transport**, so a
      test driving `probe_reasoning_effort` can only ever reach the both-attempts-failed path
      -- the executor flagged this mid-run and was right. Splitting the *decision* out as a
      pure `probe_verdict(with_ok, without_ok)` puts the rule where a test reaches all of it,
      which is the same move as `approvedText` and `keyField` before it. Adding a mock
      transport to `llm-bridge` would be its own task, not a line in this one.
- [ ] Changing the endpoint clears the verdict.
- [ ] `npm run gate` clean; no `--ignored` test is required to pass in CI.
- [ ] No changes outside the listed files.

**Producer click-through:**
- [ ] Run the test call against QwenCloud, then confirm `config.json` shows
      `"accepts_reasoning_effort": true`.
- [ ] Generate a lyric and confirm it now returns in seconds rather than after a long think.
      **This is the whole point of the task** -- T-302's numbers say 33 s to 1 s.
- [ ] Point at Ollama, run the test call, confirm the verdict is recorded there too and
      lyric generation still behaves as before.

## Out of scope

- **Do not try to discover which models honour the field.** Acceptance is the only question
  worth asking (LLM-SURFACE 13.4), and per-model discovery is a much larger surface for no
  benefit.
- Do not send any value other than `"none"`. `"low"` is not honoured by Ollama (LLM-SURFACE
  12.2) and the observed valid set differs by provider.
- Do not remove the `thinks` capability or its "thinks first" chip -- it still explains a
  pause to the user, which is a separate job from this flag.
- Do not add a user-facing toggle for it. It is a verified fact, not a preference.
- Do not touch the `--ignored` measurements' findings, only their call signatures.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/LLM-SURFACE.md --read tasks/t-302b-brief.md --read crates/llm-bridge/src/error.rs --read app/src/state/config.ts --file crates/library/src/config.rs --file src-tauri/src/llm.rs --file src-tauri/src/lyrics.rs --file src-tauri/src/optimize.rs --file app/src/bridge/config.ts --file app/src/bridge/llm.ts --file app/src/state/llm.ts --file app/src/state/llm.test.ts
```

`llm-bridge/src/error.rs` is `--read` because the probe branches on `Result`, and
`state/config.ts` because the store calls `save` and constructs an `LlmConfig`; neither
changes. **docs/LLM-SURFACE.md is `--read`** — this brief's design is downstream of §13.3
and §13.4, and an executor that has not seen them will reach for error-string matching.
