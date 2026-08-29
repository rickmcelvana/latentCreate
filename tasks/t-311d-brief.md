# T-311d — the sidecar records which run produced it

**Lane: architect-direct.** One field, its population, and its tests. WORKFLOW section 1: a task
whose reference implementation *is* the task does not go through an executor, because the round
trip cannot change the outcome. Brief written first anyway, and reviewed against afterwards as if
someone else had written it.

**Numbered `d` but lands before `c`.** T-311c (the Library view) was named in the phase file and
PROJECT.md before this task existed; renaming a published number costs more than a letter out of
order. Numbers are labels here, not an ordering contract. **This lands first**, because it is
cheapest before anything reads sidecars.

**Depends:** T-311b (landed). **Crate/dir:** `crates/create-core`, `src-tauri`.

**Files to modify:**

- `crates/create-core/src/provenance.rs` — the field, and its round-trip test
- `src-tauri/src/ingest.rs` — populate it, and test that it is populated
- `crates/library/src/tracks.rs` — the one test that constructs a `Provenance`

## Why

Everything needed to *reproduce* a track is already in its sidecar, verified against the executed
graph on 2026-08-29 (MCP-SURFACE 27.1). What is missing is the ability to ask **"what actually
ran?"** -- and MCP-SURFACE 17.2 records that `GET /history/<prompt_id>` is the only surface that
answers it. Without the prompt id, that lookup needs a human to match timestamps against
`job(action="queue")`, which is exactly what verifying T-311b took.

This is a provenance record whose one gap is the key to the provenance surface. One field closes it.

## Spec

### 1. The field

```rust
    /// ComfyUI prompt id of the run that produced this track.
    ///
    /// `None` for a sidecar written before this field existed, and for any
    /// track whose origin is not a ComfyUI run.
    ///
    /// Not needed to reproduce the track -- everything for that is in `spec`
    /// and `resolved_slots` -- but it is the key to `GET /history/<prompt_id>`,
    /// the only surface that reports what the engine actually executed
    /// (MCP-SURFACE 17.2). Without it, matching a sidecar to its run means
    /// comparing timestamps by hand.
    #[serde(default)]
    pub prompt_id: Option<String>,
```

**`Option<String>`, not `String`, and this is settled by evidence rather than taste:** a real
sidecar already exists on disk without the field -- `projects/my-first-song/tracks/tr-0001.json`,
written 2026-08-29 -- and a required field would fail to load it. `String` with `#[serde(default)]`
would load it as `""`, and an empty string that means "absent" is the confusion that has produced
four separate bugs in this project. `None` says absent.

**Field order:** put it directly after `created_at`, at the end of the struct. Serde does not care,
but the sidecar is a file people read, and "which run" belongs beside "when".

### 2. Populate it

`src-tauri/src/ingest.rs`. `ingest_outputs` gains a `prompt_id: &str` parameter, threaded into
`build_track`.

**Source it from the pump's own job id**, which `ingest_if_pending` already has -- that is by
definition the job whose outputs were just fetched. **Do not** add it to `PendingTrack`: the pending
map is *keyed* by the prompt id, so a copy inside the value is a second source of truth for
something the call site already holds, and the two could drift under a later refactor.

`OutputBatch.prompt_id` is also present and is the server's own echo. It is not the source here --
prefer the id the app polled and fetched with -- but a debug-level mismatch is worth noticing if
that is cheap; skip it if it is not.

### 3. The three other construction sites

`provenance.rs` has two (its round-trip test), `library/src/tracks.rs` one. They only need the field
added. Give the `library` one `None`, so at least one test proves a `Provenance` still builds
without a prompt id.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] `test_track_sidecar_roundtrips` (provenance.rs) carries a `Some(..)` prompt id through the
      round trip.
- [ ] **`test_a_sidecar_written_before_prompt_id_still_loads`** — deserialize a `Track` JSON with
      no `prompt_id` key and assert it loads with `None`. The invariant: the sidecar already on the
      producer's disk must keep loading. This is the one that would actually break a user.
- [ ] **`test_ingest_records_the_prompt_id_that_produced_the_track`** — drive `ingest_outputs` and
      assert the written sidecar carries the id it was called with. Load it **back from disk**
      rather than asserting the returned value, for the same reason
      `test_ingest_reproduces_from_the_sidecar_alone` does: the file is the artifact.
- [ ] Mutation: replacing the populated id with `None` must fail a test. If it does not, the test
      is decorative.
- [ ] No changes outside the listed files. No other `Provenance` field added.

## Out of scope

- **Anything that reads the field.** A Library view showing "what ran" is T-311c's if it wants it.
- **Backfilling `tr-0001.json`.** The existing track keeps `None` honestly. Inventing an id for it
  by matching timestamps would put a guess into a provenance record, which is worse than an absence.
- **`Track` fields.** Only `Provenance` gains one.
