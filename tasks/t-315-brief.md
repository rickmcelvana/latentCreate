# T-315 — the crash path says what to do about it

**Lane: architect-direct.** One pure function, one call-site change, and their tests. WORKFLOW
section 1: writing the reference *is* writing the task, so an executor round trip cannot change
the outcome. Brief written first anyway, and reviewed against afterwards as if someone else had
written it.

**Depends:** T-310b (landed — the row that renders this string). **Crate/dir:** `src-tauri`.

**Files to modify:**

- `src-tauri/src/jobs.rs` — `transport_reason`, the `Err` arm of `terminal_outcome`, the
  session-log write in `monitor_job`, and the tests
- nothing else

## Why

A producer closed ComfyUI mid-generation on 2026-08-29 — the check T-314 owes, done early. The
library came out clean and the pump retired within seconds, both correct. The queue row showed
this (MCP-SURFACE 28.2, verbatim):

```
job failed [server_not_running]: Error executing tool job: comfy jobs status d3715c6b-92a0-4f1b-ad3b-104e2d0cb7fe
failed [server_not_running]: ComfyUI not running on 127.0.0.1:8188 - job d3715c6b... was 'running' when the server
was last seen (submitted 2026-08-29T07:49:09+00:00, last update 2026-08-29T07:49:27+00:00). The server may have died
while executing it (e.g. killed by the OS on an out-of-memory allocation). hint: run: comfy launch - then check
`comfy jobs ls` for the job's last recorded state
```

Around 400 characters in a row sized for a short sentence, and CONVENTIONS line 29 requires every
user-facing error to say what to do next. The one actionable phrase here — `run: comfy launch` —
is buried in the middle of a prompt id, two ISO timestamps and a backticked shell command.

The cause is structural, not a slip. `terminal_outcome`'s `Err` arm is `e.to_string()` and nothing
else, while `failure_reason` directly beneath it does careful, evidence-backed work for the *node*
failure path. The transport path has no vocabulary of its own, so every crash, kill and unreachable
port renders as whatever string the tool happened to produce.

**None of this is T-310b's fault** — its brief called the failure line correct for a node failure
and explicitly provisional for a crash, which is exactly what it turned out to be.

## Two things this brief must not get wrong

**1. The app never sees `server_died`** (MCP-SURFACE 28.1). That code names precisely this event
and is the obvious thing to build the mapping around. It is unreachable: it exists only in
comfy-cli's state file *after* the server comes back, by which time the pump has retired having
emitted the transport failure instead. A live crash reaches the app as `server_not_running`. An
arm for `server_died` would be dead code for the only situation it describes.

**2. comfy-mcp's own message already opens with the code.** `ComfyError::Tool`'s Display is
`"{tool} failed [{code}]: {message}"` ([error.rs:19](../crates/mcp-bridge/src/error.rs:19)) and
the payload's `message` *starts* `"Error executing tool job: comfy jobs status <id> failed
[server_not_running]: ..."`. That is why the row said the code and the word "failed" twice. Any
fallback in the new function must therefore read `message`, **never** `to_string()`.

## Spec

### 1. The function

Add directly below `failure_reason`, so the two paths read as the pair they are.

```rust
/// The sentence a row shows when *polling* a job failed, as opposed to the job
/// itself failing.
///
/// [`failure_reason`] is the equivalent for a node failure. This path had no
/// vocabulary at all until 2026-08-29 and rendered `ComfyError::to_string()`
/// verbatim, which put ~400 characters of tool diagnostics into a row sized for
/// a short sentence, with the error code and the word "failed" each appearing
/// twice (MCP-SURFACE 28.2). The doubling is structural: `ComfyError::Tool`'s
/// Display is `"{tool} failed [{code}]: {message}"` and comfy-mcp's own message
/// already opens with `"... comfy jobs status <id> failed [server_not_running]:
/// ..."`. The fallback below therefore reads `message`, never `to_string()`.
///
/// **`server_died` is deliberately absent.** That code names exactly this event
/// and never reaches the app: it exists only in comfy-cli's state file once the
/// server is back, by which time this pump has retired, having emitted the
/// transport failure instead (MCP-SURFACE 28.1). An arm for it would be dead
/// code for the one situation it describes.
///
/// Nothing is lost: `monitor_job` writes the full diagnostic to `session.log`
/// before this string reaches the row.
fn transport_reason(error: &ComfyError) -> String {
    match error {
        ComfyError::Tool { code, message, .. } => match code.as_deref() {
            Some("server_not_running") => {
                "ComfyUI stopped while this was generating. Start ComfyUI, then queue it again."
                    .to_string()
            }
            // No cause is offered. A restart is the likely one, but this
            // project has not observed *why* an id goes missing, and a row is
            // the wrong place to publish a guess. What to do next is known.
            Some("prompt_not_found") => {
                "ComfyUI has no record of this job. Queue the generation again.".to_string()
            }
            _ => message.clone(),
        },
        ComfyError::Transport(_) => {
            "Lost the connection to ComfyUI. Start ComfyUI, then queue this generation again."
                .to_string()
        }
        other => other.to_string(),
    }
}
```

**Only two codes, and both are verified.** `server_not_running` is the observed crash (28.2).
`prompt_not_found` is what `job(action="status")` returns for an id the server does not know
(MCP-SURFACE lines 437 and 462) — the second and only other way this poll is known to fail. No
third arm is invented here; guessing a code guesses a remedy, and the `parse_error_code` doc
already records why a wrong slug is worse than none.

**The unknown-code arm returns `message` un-truncated.** Cutting it to a length would be the third
option and the wrong one: the actionable half of an unknown message could sit anywhere in it, and
a truncated diagnostic is a worse row than a long one *plus* it destroys the report a user would
paste into an issue. Dropping the wrapper already fixes the doubling, which is the defect we have
evidence for.

**`other => other.to_string()`** covers `NotInstalled`, `Spawn` and `Payload`. `NotInstalled`'s
Display already ends in a next step; the other two cannot be reached from a pump polling a job
that has already been submitted, and inventing copy for them would be writing against nothing.

### 2. The call site

```rust
        Err(e) => TerminalOutcome::Failed {
            error: transport_reason(e),
        },
```

`terminal_outcome` stays pure and stays a `&Result<..>` in, `TerminalOutcome` out — the logging
below does not belong in it.

### 3. Keep the diagnostic

In `monitor_job`, the `TerminalOutcome::Failed` arm. `result` is still in scope there, so the
original error is still available after `terminal_outcome` has replaced it:

```rust
        TerminalOutcome::Failed { error } => {
            if let Err(e) = &result {
                if let Ok(log) = SessionLog::open(root.join("session.log")) {
                    log.log_result("job_status", false, &e.to_string());
                }
            }
            let _ = app.emit(
                "job://failed",
                JobFailed {
                    id: id.clone(),
                    error,
                },
            );
        }
```

Same shape as `log_ingest_failure` ([jobs.rs:335](../src-tauri/src/jobs.rs:335)), which already
writes a failure to `session.log` this way — reuse the pattern rather than a new one. **This is
what makes the shortening safe**: the 400 characters are not deleted, they move to the file the
session log exists to be. Redaction is `SessionLog`'s job and is already covered (T-102b/T-102c).

Only the `Err` path logs. A node failure that arrives as `Ok(status)` has its detail in the row
already, via `failure_reason`.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] **`test_a_crash_mid_job_says_what_to_do`** — `ComfyError::Tool` with code
      `server_not_running` and the **verbatim 400-character message from MCP-SURFACE 28.2** as its
      payload. Assert the result contains `"Start ComfyUI"`, and assert it does **not** contain
      `"server_not_running"` or `"comfy jobs status"`. The invariant: the row a person reads after
      a crash is a sentence, not the tool's diagnostics. Use the real message, not a short stand-in
      — a stand-in is how `failure_reason` shipped a bug that every test passed (see its doc).
- [ ] **`test_an_unknown_tool_code_shows_the_message_without_the_wrapper`** — `Tool` with some code
      this function does not know. Assert the result **equals** `message` exactly. The invariant:
      even unmapped, the code and the word "failed" appear once rather than twice.
- [ ] **`test_prompt_not_found_says_to_queue_it_again`** — the second verified code maps, and the
      result contains no id.
- [ ] **`test_a_lost_connection_says_what_to_do`** — `ComfyError::Transport` maps to the sentence.
- [ ] **`test_terminal_outcome_maps_poll_error_to_failed` is updated, not deleted.** It currently
      asserts `error.contains("closed")` — the raw transport string — which is precisely the
      behaviour being removed. Re-point it at the new sentence. Deleting it would drop the only
      test that `Err` reaches `Failed` at all.
- [ ] Mutation: changing the `server_not_running` arm to `message.clone()` must fail a test; so
      must reverting the unknown-code arm to `error.to_string()`, deleting the `Transport` arm, and
      **unwiring `transport_reason` from `terminal_outcome`** — that last one is the reason the
      updated wiring test above must assert the mapped copy rather than merely `Failed`. Nothing
      else tests the wiring.
- [ ] No changes outside `src-tauri/src/jobs.rs`.

**The session-log write is not unit-testable here** and this brief will not pretend otherwise:
`monitor_job` needs an `AppHandle`. It is verified by construction and by the click-through step
below. Say so rather than adding a test that asserts nothing.

## Manual verify (producer click-through)

The one path that matters, and the same gesture that found the defect:

1. Queue a generation on either profile, and **close ComfyUI while it runs**.

2. The row settles to Failed within seconds and reads **one sentence** ending in a next step. No
   prompt id, no timestamps, no `comfy jobs ls`.


3. `%APPDATA%\com.latentbeats.create\session.log` has the full original diagnostic on a
   `job_status` line — the detail is moved, not lost.


4. Restart ComfyUI and queue again: it generates. The failed row is history, not a stuck state.


5. Library unchanged — no partial track, `next_track_seq` unmoved. (This passed on 2026-08-29,
   MCP-SURFACE 28.3; re-checking it costs one glance and it is the thing a regression here would
   break.)

## Out of scope

- **`server_died`.** See above — unreachable from the app.
- **Reading the recovered state-file record after a restart.** A "what happened to that job?"
  lookup is a real feature and this is not it; it needs a surface that polls a *retired* job, which
  nothing in the app has. Note it for T-314's fix-up list if the producer wants it.
- **The node-failure path.** `failure_reason` is correct and evidence-backed; do not touch it.
- **Retry-in-place on the failed row.** Queueing again is the existing gesture and it works.
- **Frontend changes.** `state/queue.ts` renders `job.error` and should keep knowing nothing about
  error codes; the mapping belongs where the typed error is.

## Changed during review

Two things, both in the direction of claiming less:

1. **The `prompt_not_found` copy dropped its cause.** The first draft read "it was most likely
   restarted". The *code* is verified; the reason an id goes missing is not — this project has
   never observed one. A row is the wrong place to publish an inference, and "what to do next" is
   known without it.
2. **The crash fixture's comment no longer says "byte for byte".** It is the message as recorded
   in MCP-SURFACE 28.2, reflowed to fit a Rust string literal. Close enough to matter, not close
   enough to claim exactness.

Four mutations run, four killed. The fourth — replacing `transport_reason(e)` with `e.to_string()`
at the call site — is the one that justified keeping the updated wiring test's assertion on copy
rather than weakening it to `matches!(.., Failed { .. })`: nothing else covers the wiring, and the
weaker version would have let the whole defect back in.
