# T-112d: the LLM step's view
**Depends:** T-112c | **Crate/dir:** `app/src`
**Files to modify:**
- `app/src/views/Setup.tsx` (modify: one import, one `<LlmStep />`, one new component)
- `app/src/theme.css` (modify: append one block)

## Goal
Render the lyric-model step: the catalogue, what each model means for the user, the suggestion
help, and a test call.

## Spec
Exactly the reference implementation below.

**Rules that are correctness, not layout:**

- **The privacy disclosure appears on the row itself**, next to the model it applies to, not in
  a footnote. Choosing a remote model sends unreleased lyrics to a third party.
- **When the endpoint could not be enriched, the step says so once, plainly**, and every row
  carries `capabilities unknown`. It must not look like a clean bill of health.
- **The pull command is shown, never run.** This app does not pull an LLM (docs/MODELS.md).
- **No polling.** Probes once on mount, then only when the user asks -- the same rule the other
  two steps follow. This probe is also the step's only keychain read.

**Use the existing CSS tokens**: `--text-muted`, `--border`, `--border-bright`, `--warning`,
`--radius`, `--gap-*`. There is no `--font-mono`; the mono stack is written out, matching
`.setup-command`.

## Reference implementation

Apply exactly this diff:
```diff
diff --git a/app/src/theme.css b/app/src/theme.css
index 1e59e53..dce74b5 100644
--- a/app/src/theme.css
+++ b/app/src/theme.css
@@ -434,3 +434,77 @@ body {
   flex: none;
   color: var(--text-muted);
 }
+/* --- Setup wizard, lyrics model step --- */
+
+.llm-models {
+  margin: 0;
+  padding: 0;
+  list-style: none;
+  display: flex;
+  flex-direction: column;
+  gap: var(--gap-sm);
+  max-height: 320px;
+  overflow-y: auto;
+}
+
+.llm-model {
+  display: flex;
+  flex-direction: column;
+  gap: 4px;
+  padding: var(--gap-sm) 0;
+  border-top: 1px solid var(--border);
+}
+
+.llm-model-pick {
+  display: flex;
+  align-items: center;
+  gap: var(--gap-sm);
+  cursor: pointer;
+}
+
+.llm-model-pick input:disabled {
+  cursor: not-allowed;
+}
+
+.llm-model-pick input:disabled + code {
+  color: var(--text-muted);
+  text-decoration: line-through;
+}
+
+.llm-model-pick code {
+  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
+  font-size: 13px;
+  color: var(--text);
+  word-break: break-all;
+}
+
+.llm-chips {
+  display: flex;
+  flex-wrap: wrap;
+  gap: 6px;
+  padding-left: 26px;
+}
+
+.llm-chip {
+  font-size: 11px;
+  line-height: 1.6;
+  padding: 0 8px;
+  border-radius: var(--radius);
+  border: 1px solid var(--border-bright);
+  color: var(--text-muted);
+  white-space: nowrap;
+}
+
+.llm-disclosure {
+  margin: 0;
+  padding-left: 26px;
+  font-size: 12px;
+  line-height: 1.5;
+  color: var(--warning);
+}
+
+.llm-suggestion {
+  display: flex;
+  flex-direction: column;
+  gap: var(--gap-sm);
+}
diff --git a/app/src/views/Setup.tsx b/app/src/views/Setup.tsx
index 540582a..fd3c942 100644
--- a/app/src/views/Setup.tsx
+++ b/app/src/views/Setup.tsx
@@ -3,6 +3,7 @@ import type { ComfyStatus } from '../bridge/comfy'
 import { useComfyStore, formatVram, pillFor } from '../state/comfy'
 import type { ProfileStatus } from '../bridge/models'
 import { curatedFirst, formatBytes, installView, rowFor, useModelsStore } from '../state/models'
+import { canTest, modelView, testSummary, useLlmStore } from '../state/llm'
 
 /**
  * Setup wizard, ComfyUI step.
@@ -64,10 +65,138 @@ export function Setup() {
       </section>
 
       <ModelsStep />
+      <LlmStep />
     </>
   )
 }
 
+/** Where the lyric LLM lives. Ollama's default, and the commonest case. */
+const DEFAULT_BASE_URL = 'http://127.0.0.1:11434/v1'
+
+/**
+ * Setup wizard, lyric-LLM step.
+ *
+ * Probes once on mount and otherwise only when the user asks. The probe is
+ * also the step's only keychain read -- `has_key` rides on the status, so
+ * nothing here calls `has_secret`, whose answer requires reading the secret
+ * and on macOS can raise a prompt (T-004).
+ */
+function LlmStep() {
+  const status = useLlmStore((state) => state.status)
+  const busy = useLlmStore((state) => state.busy)
+  const testing = useLlmStore((state) => state.testing)
+  const result = useLlmStore((state) => state.result)
+  const model = useLlmStore((state) => state.model)
+  const probe = useLlmStore((state) => state.probe)
+  const choose = useLlmStore((state) => state.choose)
+  const test = useLlmStore((state) => state.test)
+
+  useEffect(() => {
+    void probe(DEFAULT_BASE_URL, null)
+  }, [probe])
+
+  return (
+    <section className="panel setup-step">
+      <header className="setup-step-head">
+        <h2 className="setup-step-title">Lyrics model</h2>
+        <button
+          type="button"
+          className="setup-button"
+          onClick={() => void probe(DEFAULT_BASE_URL, model)}
+          disabled={busy}
+        >
+          {busy ? 'Checking...' : 'Retry'}
+        </button>
+      </header>
+
+      {status !== null && status.state === 'not_configured' ? (
+        <p className="setup-next-step">Set an endpoint to write lyrics with a model.</p>
+      ) : null}
+
+      {status !== null && status.state === 'unreachable' ? (
+        <>
+          <p className="setup-next-step">{status.detail}</p>
+          {status.hint !== null ? <p className="setup-next-step">{status.hint}</p> : null}
+        </>
+      ) : null}
+
+      {status !== null && status.state === 'ready' ? (
+        <>
+          {/* Said once, plainly: without Ollama's native API neither the
+              capability nor the privacy question can be answered at all. */}
+          {!status.enriched ? (
+            <p className="setup-next-step">
+              This endpoint does not report model capabilities, so it cannot be checked whether a
+              model runs locally or can write lyrics at all.
+            </p>
+          ) : null}
+
+          <ul className="llm-models">
+            {status.models.map((row) => {
+              const view = modelView(row)
+              return (
+                <li key={view.id} className="llm-model">
+                  <label className="llm-model-pick">
+                    <input
+                      type="radio"
+                      name="lyric-model"
+                      value={view.id}
+                      checked={model === view.id}
+                      disabled={!view.selectable}
+                      onChange={() => choose(view.id)}
+                    />
+                    <code>{view.id}</code>
+                  </label>
+                  {view.chips.length > 0 ? (
+                    <span className="llm-chips">
+                      {view.chips.map((chip) => (
+                        <span key={chip} className="llm-chip">
+                          {chip}
+                        </span>
+                      ))}
+                    </span>
+                  ) : null}
+                  {view.disclosure !== null ? (
+                    <p className="llm-disclosure">{view.disclosure}</p>
+                  ) : null}
+                </li>
+              )
+            })}
+          </ul>
+
+          {status.missing_suggestions.map((suggestion) => (
+            <div key={suggestion.label} className="llm-suggestion">
+              <p className="setup-next-step">
+                {suggestion.label} is suggested for lyrics
+                {suggestion.why === null ? '' : ` -- ${suggestion.why}`}
+                {suggestion.vram_hint === null ? '' : ` Needs ${suggestion.vram_hint}.`}
+              </p>
+              {/* The command is shown, never run: this app does not pull an
+                  LLM onto the user's disk (docs/MODELS.md). */}
+              {suggestion.pull_command !== null ? (
+                <code className="setup-command">{suggestion.pull_command}</code>
+              ) : null}
+            </div>
+          ))}
+
+          <div className="setup-actions">
+            <button
+              type="button"
+              className="setup-button setup-button-primary"
+              onClick={() => void test(DEFAULT_BASE_URL)}
+              disabled={!canTest(status, model) || testing}
+            >
+              {testing ? 'Testing...' : 'Test call'}
+            </button>
+          </div>
+
+          {result !== null ? <p className="setup-next-step">{testSummary(result)}</p> : null}
+        </>
+      ) : null}
+    </section>
+  )
+}
+
 /**
  * Setup wizard, models step.
  *
```

## Acceptance criteria
- `npm run gate` green, zero oxlint warnings.
- Test counts unchanged from T-112c (**51** vitest).
- Verified by driving the store directly in a browser against the real 13-model catalogue: the
  two embedding models render **disabled** with a `cannot chat` chip; every `:cloud` model shows
  `remote` plus a sentence naming its host (including the `:443` variants); `gemma4:12b-32k` is
  **checked** and chipped `recommended for lyrics`, with `gemma4:12b-it-qat` chipped but not
  checked; `mistral-large-3:675b-cloud` shows `remote` **without** `thinks first`. With
  `enriched: false`, every row goes to `capabilities unknown`, nothing is disabled, and no
  disclosure is shown.
- **No non-ASCII characters anywhere in the diff.**

## Out of scope
Everything else. Do not change the ComfyUI or models steps.

## If unclear
Follow the reference implementation exactly.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read app/src/state/llm.ts --read app/src/bridge/llm.ts --file app/src/views/Setup.tsx --file app/src/theme.css
```
