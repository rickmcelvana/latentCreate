# T-111e: the models step's view
**Depends:** T-111d | **Crate/dir:** `app/src`
**Files to modify:**
- `app/src/views/Setup.tsx` (modify: two import lines, one `<ModelsStep />`, two new components)
- `app/src/theme.css` (modify: append one block)

## Goal
Render the models step: one row per profile, its licence, what is missing, and an Install
button where the app can actually install.

## Spec
Exactly the reference implementation below.

**The licence is shown on every row, installed or not.** CONVENTIONS requires per-model terms
wherever a model is chosen or installed, and MiniMax Music 3 is exactly why: open weights, not
OSI-open, with an attribution obligation and a revenue threshold the user takes on by
generating with it. Note the terms come **from the profile**, never from the download host --
the Comfy-Org repackage of MiniMax is tagged Apache-2.0 on Hugging Face while the upstream
weights carry a custom community licence (MCP-SURFACE 14.6).

**Rules that are correctness:**

- **One banner, not one message per row.** When the inventory could not be taken every row is
  `unknown`; the step says so once, using `inventory_detail` so it can distinguish "ComfyUI is
  stopped" from "comfy-mcp is not installed".
- **The Install button appears only for `missing` **and** `installable`.** A partly-installable
  set offered as one click leaves the user with a model that still will not run.
- **The file list is only rendered for `missing`.** An `unknown` row must not show 18.5 GiB of
  files it cannot confirm are absent.
- **While a download runs, the static next step is replaced by live progress**, and every
  Install button on the step is disabled -- one transfer at a time.

**No polling on load.** `ModelsStep` checks once on mount and otherwise only when the user
asks, the same rule the ComfyUI step follows: a wizard that re-probes on a timer spawns
`comfy-mcp` processes behind the user's back.

**Use the existing CSS tokens.** `--text-muted`, `--border`, `--gap-*`. There is no
`--text-dim` and no `--font-mono`; the mono stack is written out, matching `.setup-command`.

## Reference implementation

Apply exactly this diff:
```diff
diff --git a/app/src/theme.css b/app/src/theme.css
index 32edcd6..c2f6561 100644
--- a/app/src/theme.css
+++ b/app/src/theme.css
@@ -370,3 +370,66 @@ body {
   border-color: var(--accent);
   color: var(--accent);
 }
+/* --- Setup wizard, models step --- */
+
+.model-row {
+  display: flex;
+  flex-direction: column;
+  gap: var(--gap-sm);
+  padding: var(--gap-md) 0;
+  border-top: 1px solid var(--border);
+}
+
+.model-row-head {
+  display: flex;
+  align-items: center;
+  justify-content: space-between;
+  gap: var(--gap-md);
+}
+
+.model-row-title {
+  margin: 0;
+  font-size: 15px;
+  font-weight: 600;
+  color: var(--text);
+}
+
+.model-row-license {
+  margin: 0;
+  font-size: 12px;
+  line-height: 1.5;
+  color: var(--text-muted);
+}
+
+.model-row-license-name {
+  font-weight: 600;
+  color: var(--text);
+}
+
+.model-files {
+  margin: 0;
+  padding: 0;
+  list-style: none;
+  display: flex;
+  flex-direction: column;
+  gap: 4px;
+}
+
+.model-files li {
+  display: flex;
+  align-items: baseline;
+  justify-content: space-between;
+  gap: var(--gap-md);
+  font-size: 12px;
+}
+
+.model-files code {
+  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
+  color: var(--text);
+  word-break: break-all;
+}
+
+.model-file-folder {
+  flex: none;
+  color: var(--text-muted);
+}
diff --git a/app/src/views/Setup.tsx b/app/src/views/Setup.tsx
index d0c5b54..540582a 100644
--- a/app/src/views/Setup.tsx
+++ b/app/src/views/Setup.tsx
@@ -1,6 +1,8 @@
 import { useEffect } from 'react'
 import type { ComfyStatus } from '../bridge/comfy'
 import { useComfyStore, formatVram, pillFor } from '../state/comfy'
+import type { ProfileStatus } from '../bridge/models'
+import { curatedFirst, formatBytes, installView, rowFor, useModelsStore } from '../state/models'
 
 /**
  * Setup wizard, ComfyUI step.
@@ -60,10 +62,117 @@ export function Setup() {
           ) : null}
         </div>
       </section>
+
+      <ModelsStep />
     </>
   )
 }
 
+/**
+ * Setup wizard, models step.
+ *
+ * Readiness is decided by comparing each profile's declared files against what
+ * ComfyUI reports it has -- never by `local_check.runnable`, which answers a
+ * different question and calls a working MiniMax install unrunnable over a
+ * filename the profile already corrects.
+ */
+function ModelsStep() {
+  const view = useModelsStore((state) => state.view)
+  const busy = useModelsStore((state) => state.busy)
+  const refresh = useModelsStore((state) => state.refresh)
+
+  useEffect(() => {
+    void refresh()
+  }, [refresh])
+
+  const profiles = view === null ? [] : curatedFirst(view.profiles)
+
+  return (
+    <section className="panel setup-step">
+      <header className="setup-step-head">
+        <h2 className="setup-step-title">Models</h2>
+        <button type="button" className="setup-button" onClick={() => void refresh()} disabled={busy}>
+          {busy ? 'Checking...' : 'Retry'}
+        </button>
+      </header>
+
+      {view !== null && !view.inventory_available ? (
+        <p className="setup-next-step">
+          Cannot see which models are installed. {view.inventory_detail ?? 'Start ComfyUI above.'}
+        </p>
+      ) : null}
+
+      {profiles.map((profile) => (
+        <ModelRow key={profile.id} profile={profile} />
+      ))}
+    </section>
+  )
+}
+
+/** One model, its licence, and whether it can be used. */
+function ModelRow({ profile }: { profile: ProfileStatus }) {
+  const install = useModelsStore((state) => state.install)
+  const installing = useModelsStore((state) => state.installing)
+  const progress = useModelsStore((state) => state.progress)
+
+  const row = rowFor(profile.readiness)
+  const active = installing === profile.id
+  const live = active ? installView(progress) : null
+
+  return (
+    <article className="model-row">
+      <header className="model-row-head">
+        <h3 className="model-row-title">{profile.display_name}</h3>
+        <span className={`status-pill status-pill-${row.tone}`}>{row.label}</span>
+      </header>
+
+      {/* Shown for every model, installed or not: some weights are open with
+          conditions the user takes on by generating with them (CONVENTIONS). */}
+      <p className="model-row-license">
+        <span className="model-row-license-name">{profile.license}</span>
+        {profile.license_notes !== null ? ` -- ${profile.license_notes}` : null}
+      </p>
+
+      {row.nextStep !== null && !active ? <p className="setup-next-step">{row.nextStep}</p> : null}
+
+      {live !== null ? (
+        <p className="setup-next-step">
+          Downloading {live.done} of {live.total} files
+          {live.percent === null ? '' : ` -- ${live.percent}%`}
+          {live.failed.length > 0 ? ` -- ${live.failed.length} failed` : ''}
+        </p>
+      ) : null}
+
+      {profile.readiness.state === 'missing' ? (
+        <ul className="model-files">
+          {profile.readiness.files.map((file) => (
+            <li key={`${file.folder}/${file.file}`}>
+              <code>{file.file}</code>
+              <span className="model-file-folder">
+                {file.folder}
+                {formatBytes(file.size_bytes) === null ? '' : ` -- ${formatBytes(file.size_bytes)}`}
+              </span>
+            </li>
+          ))}
+        </ul>
+      ) : null}
+
+      {profile.readiness.state === 'missing' && profile.readiness.installable ? (
+        <div className="setup-actions">
+          <button
+            type="button"
+            className="setup-button setup-button-primary"
+            onClick={() => void install(profile.id)}
+            disabled={installing !== null}
+          >
+            {active ? 'Downloading...' : 'Install'}
+          </button>
+        </div>
+      ) : null}
+    </article>
+  )
+}
+
 /** The details worth showing once ComfyUI is up. */
 function ComfyFacts({ status }: { status: Extract<ComfyStatus, { state: 'ready' }> }) {
   const vram = formatVram(status.vram_bytes)
```

## Acceptance criteria
- `npm run gate` green, zero oxlint warnings.
- Test counts unchanged from T-111d (**41** vitest).
- Verified by driving the store directly in a browser: with the captured payload, MiniMax
  renders first with an `ok` pill reading `Installed` and no button, and ACE-Step renders
  second with a `warn` pill reading `Not installed`, the four files with sizes, and an Install
  button. With `inventory_available: false` both rows go **neutral** `Cannot check` with no
  button and no file list.
- **No non-ASCII characters anywhere in the diff.**

## Out of scope
Everything else. Do not change the ComfyUI step.

## If unclear
Follow the reference implementation exactly.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read app/src/state/models.ts --read app/src/bridge/models.ts --file app/src/views/Setup.tsx --file app/src/theme.css
```
