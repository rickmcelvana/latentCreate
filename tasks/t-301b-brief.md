# T-301b: let the user set the endpoint and the API key

**Depends:** T-301 | **Crate/dir:** `app` (frontend only)
**Files to modify:**
- `app/src/state/llm.ts`
- `app/src/state/llm.test.ts`
- `app/src/views/Setup.tsx`
- `app/src/theme.css`

## Goal

A user can point the Lyrics-model step at any OpenAI-compatible endpoint and store an API
key for it, both persisted. Today `DEFAULT_BASE_URL` is a hardcoded constant in five places
and there is no field for it, so the step can only ever reach a local Ollama on the default
port — which means a user on OpenAI, Anthropic, OpenRouter, LM Studio, vLLM or a LAN box
cannot connect at all.

This is the task that makes the 2026-08-27 decision true. T-301 removed the app's opinion
about *which* model; this removes the assumption about *where* it runs.

## Spec

### 0. No backend change. None.

Everything needed already exists and is registered: `llm_probe` and `llm_test` both take
`base_url` as an argument, `set_secret` / `has_secret` / `delete_secret` are Tauri commands,
`SecretKey::LlmApiKey` is in the whitelist, and `save_config` persists `llm.base_url`.
**Do not add, rename or re-register any Tauri command, and do not touch any `.rs` file.**
If something seems to require one, stop and ask (footer rule).

### 1. The effective endpoint is a store selector, not a value derived in JSX

Phase 2's recurring defect was correct logic derived inline in a view. Add to
`app/src/state/llm.ts`:

```ts
/** The endpoint the step should use, before the user has typed anything. */
export const DEFAULT_BASE_URL = 'http://127.0.0.1:11434/v1'

/**
 * Which endpoint the step actually talks to.
 *
 * The configured value wins; the default is a **prefill**, not a fallback the
 * user is stuck with (owner, 2026-08-27). A blank or whitespace-only stored
 * value is treated as unset, because that is what clearing the field leaves
 * behind and an empty string would probe nothing forever.
 */
export function effectiveBaseUrl(config: Config | null): string {
  const stored = config?.llm?.base_url ?? null
  return stored !== null && stored.trim() !== '' ? stored.trim() : DEFAULT_BASE_URL
}
```

Move `DEFAULT_BASE_URL` out of `Setup.tsx` (it is defined there today at line 75) and import
it from the store, so one module owns the value.

### 2. ⚠ Saving the endpoint must not wipe the model

`useConfigStore.save` does a **shallow** merge (`{ ...current, ...patch }`), so `llm` is
replaced wholesale. `choose` already gets this right by passing the complete block. Anything
that saves the endpoint must do the same:

```ts
await useConfigStore.getState().save({
  llm: {
    provider: 'open_ai_compat',
    base_url: url,
    model: useConfigStore.getState().config?.llm?.model ?? null,
  },
})
```

Saving `{ llm: { base_url: url } }` would drop the user's model selection — the same class of
loss T-212 fixed. `tsc` catches the missing fields; it does not catch passing `null` for a
model the user had chosen.

### 3. The endpoint field

A text input in the LLM step, above the model list. Rules:

- **Prefilled with the effective endpoint** on mount (owner decision: the Ollama address, so
  nothing regresses for local users while everyone else can see what the app had been
  assuming).
- **Local draft state in the component.** The typed value is uncommitted text, which is
  genuinely view-local — this is not the "derived logic in a view" the note above forbids.
- **Applied on an explicit action, never per keystroke:** a "Connect" button and `Enter` in
  the field. Applying means: trim, save per §2, then `probe(url, configuredModel)`.
- The existing "Retry" button re-probes the **effective** endpoint. Keep it; it is the
  no-change path.
- Every remaining `DEFAULT_BASE_URL` use in `Setup.tsx` (`probe`, `choose`, `test` — lines
  107, 117, 158, 198) becomes the effective endpoint.

### 4. The API key field — write-only, and it stays that way

- `type="password"`, empty on mount, **never populated from the backend**. No Tauri command
  returns a secret value (T-004, WORKFLOW §4.6) and nothing here may change that.
- Save writes through the existing config store: `storeSecret('llm_api_key', value)`.
- When a key is stored, show a "Remove" action calling `removeSecret('llm_api_key')`.
- **After storing or removing a key, re-probe.** An endpoint that needs a key lists nothing
  without one, so the model list is stale the moment the key changes.

⚠ **Read `has_key` off `LlmStatus::Ready`, not from the config store's `secrets` map.**
Both can answer "is a key stored", and two sources for one fact is how they end up
disagreeing. `llm_probe`'s doc comment states the reason plainly: it is the step's *only*
keychain read, deliberately, because answering the question means reading the secret and on
macOS a read can raise a prompt. **Do not call `refreshSecrets(['llm_api_key'])` in this
step.**

### 5. The status copy, now that `not_configured` is reachable

Until this task, `Setup.tsx` always probed a non-empty constant, so `not_configured` could
never render — T-301 shipped its guidance in that branch before this was noticed, and it was
moved to `unreachable`. With a clearable field, `not_configured` becomes a real state again
and each branch says its own thing:

- **`not_configured`** (field cleared / nothing stored): "Enter the address of an
  OpenAI-compatible endpoint to write lyrics with a model." No address named — there isn't
  one.
- **`unreachable`**: keep `status.detail`, keep `status.hint` (the `/v1` hint from
  LLM-SURFACE §11.3, considerably more valuable now that users type URLs), keep the
  "works with any OpenAI-compatible endpoint" sentence, and keep naming the address —
  but it must name the **effective** endpoint, not the constant.
- **`ready`**: unchanged.

### 6. `theme.css`

Every new `className` needs a rule (WORKFLOW §4.5). Expect roughly `.llm-endpoint`,
`.llm-endpoint-row`, `.llm-key-row`. Match the existing `.setup-*` / `.llm-*` idiom and use
the existing custom properties — no new colours, no hardcoded hex.

## Acceptance criteria

- [ ] `effectiveBaseUrl` tested: configured value wins; `null`, `''` and `'   '` all fall
      back to the default. Name the invariant, not the mechanics.
- [ ] A test that saving an endpoint **preserves the configured model** — it would fail if
      the `llm` block were patched partially.
- [ ] A test that the key input is never populated from any backend call: the component
      renders with an empty key field when `has_key` is true, and shows the stored/remove
      affordance instead.
- [ ] Existing `llm.test.ts` tests still pass, `modelView`'s capability and disclosure tests
      untouched.
- [ ] `npm run gate` clean.
- [ ] No `.rs` file changed. No new Tauri command.
- [ ] No changes outside the four listed files.

**Producer click-through** (none of this is visible to the gate):
- [ ] Point it at a **non-Ollama** endpoint — a hosted API with a key, or LM Studio — and
      confirm models list and a test call returns. This is the first time in the repo's life
      that path has been exercised, and it is the whole point of the task.
- [ ] Endpoint survives a restart; model selection survives changing the endpoint and back.
- [ ] Clear the field: the `not_configured` copy appears.
- [ ] Store a key, restart, confirm the field is empty and the "stored" affordance shows —
      **the key must never reappear in the input.**

## Out of scope

- Do not change `reasoning_effort` or the `thinks` gating (**T-302**). Reaching a non-Ollama
  endpoint here is what finally makes that measurable, but measuring it is that task.
- Do not add provider presets, a dropdown of known hosts, or model-name suggestions of any
  kind. T-301 removed the app's opinion about models; this must not reintroduce one about
  vendors.
- Do not touch the ComfyUI step or `comfy_cloud_api_key`.
- Do not add per-endpoint profiles or multiple saved endpoints. One endpoint, as today.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read tasks/t-301b-brief.md --read app/src/bridge/config.ts --read app/src/bridge/llm.ts --read app/src/state/config.ts --file app/src/state/llm.ts --file app/src/state/llm.test.ts --file app/src/views/Setup.tsx --file app/src/theme.css
```

`bridge/config.ts`, `bridge/llm.ts` and `state/config.ts` are `--read`: the new code calls
`storeSecret` / `removeSecret` / `save` and constructs an `LlmConfig`, so their definitions
must be in view, but none of them changes.
