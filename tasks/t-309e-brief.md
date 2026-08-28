# T-309e — teach the audit to read a subgraph, and drop the three dead addresses

**Lane: architect-direct** (WORKFLOW §1). Pure Rust over a committed fixture, seven invariants,
no JSX, no CSS. Nothing here is broad mechanical work, so there is nothing for an executor to save.

---

## The premise in phase-3.md was wrong, and the correction makes this a bigger task

The phase file says the three inert addresses "are three of the seven 'could not be checked'
warnings a MiniMax user sees". That reading does not survive contact with `audit.rs`:

```rust
if instance.contains('/') {
    // Subgraph interior. Resolving it means walking from the instance
    // node to its definition, which is a different id space.
    audit.unchecked.push(address.clone());
    continue;
}
```

**Every address the MiniMax profile declares is a subgraph interior.** All eight. The audit is not
reporting three suspicious addresses among five sound ones — it is reporting that it cannot read
this workflow *at all*. Dropping the three would take the warning from eight to five and leave it
firing on every single generation, naming `37/6.unet_name` (without which no model loads) and
`37/38.seed` (which §18.5 proved reaches the sampler).

The owner has now seen this warning on every MiniMax run he has ever done. A warning with a 100%
false-positive rate is not a warning.

So the fix is not the profile edit. The fix is the audit, **and the profile edit becomes mandatory
rather than cosmetic** — see the blocker below.

## The blocker: fixing the audit alone bricks MiniMax

`generate.rs` does not warn about an inert slot. It refuses:

```rust
let inert = inert_slots(&audit);
if !inert.is_empty() {
    return Err(format!(
        "{} writes {} to inputs a node drives, so the engine would ignore them", ...
    ));
}
```

Today MiniMax passes that check **only because the audit is blind**. Teach it to see, and
`37/13.seed`, `37/9.seed` and `37/15.seconds` resolve to real backend drivers, `inert` comes back
non-empty, and **MiniMax cannot generate at all**.

The two halves therefore land in one commit. Neither is shippable alone:

- audit fix alone → MiniMax refuses every generation
- profile edit alone → five spurious warnings survive, and the audit stays blind

This is the sixth instance this phase of *a guard in one layer does not bind the layer above it*,
inverted: here the guard is correct and its **blindness** is what has been holding the layer above
it up.

## The evidence is already recorded; this task adds no new measurement

MCP-SURFACE §18.5 read `GET /history/<prompt_id>` on the first live run and tabulated all seven
addresses. My structural read of `testdata/workflows/minimax_music3_int8.json` reproduces that
table exactly:

| address | interior link origin | §18.5 verdict |
|---|---|---|
| `37/6.unet_name`     | `-10` (inputNode boundary)      | applied |
| `37/13.caption`      | `-10` (inputNode boundary)      | applied |
| `37/13.lyrics`       | `-10` (inputNode boundary)      | applied |
| `37/13.max_duration` | `-10` (inputNode boundary)      | applied |
| `37/38.seed`         | `-10` (inputNode boundary)      | applied |
| `37/13.seed`         | node 38 `SeedNode`              | **inert** |
| `37/9.seed`          | node 38 `SeedNode`              | **inert** |
| `37/15.seconds`      | node 13 `MiniMaxMusic3TextEncode` | **inert** |

Five boundary-fed, all five confirmed applied by a live run. Three backend-fed, all three confirmed
inert by the same run. **The existing `is_inert` rule is already correct** — a real backend node
drives, a virtual one does not. It only needs the boundary added to the virtual side and a way to
walk into the subgraph.

§18.5 says the current `unchecked` behaviour "is correct and should stay: the seven warnings were
honest". That was a statement about not *guessing*, and it was right at the time. Resolving is not
guessing — the fixture holds the subgraph, and the table above is ground truth to assert against.

## What §18.5 already warns the implementer about, and it matters

**Subgraph links are objects, not arrays.** Top level: `[42, 37, 0, 35, 0, "AUDIO"]`. Interior:
`{"id": 18, "origin_id": 3, "origin_slot": 0, "target_id": 13, "target_slot": 0, "type": "CLIP"}`.
`source_type_of` reads `l.get(0)` and `l.get(1)` positionally and will silently find nothing on the
interior array — returning `None`, which `is_inert` treats as inert. So a naive extension does not
fail loudly; it reports **all eight** as inert and refuses every MiniMax generation. The two shapes
need two readers.

## Scope

`crates/create-core/src/audit.rs` and `profiles/minimax-music-3.json`. No other file changes except
the three `profile.rs` tests that assert the seed and duration fan-out, which are updated
deliberately, not to make a failure go away.

### 1. Name the link source instead of stringly-typing it

`LinkFed.source_type: Option<String>` cannot express "the subgraph boundary" without smuggling a
sentinel into `VIRTUAL_NODE_TYPES` that a real node class could one day collide with. Replace it:

```rust
pub enum LinkSource {
    /// A real backend node. Its link survives conversion, so the write is inert.
    Backend(String),
    /// A frontend-only node (`PrimitiveNode`, `Reroute`), dropped at conversion.
    Virtual(String),
    /// The subgraph's own input boundary (`inputNode`): a promoted widget,
    /// not a driving edge. Five addresses fed this way were confirmed applied
    /// by a live run (MCP-SURFACE 18.5).
    Boundary,
    /// The link exists but its origin could not be identified.
    Unknown,
}
```

`is_inert()` is `matches!(self, Backend(_) | Unknown)`. Unknown stays inert: the safe reading of an
unidentifiable link is that it survives.

### 2. Resolve one level of subgraph

For `"37/13.seed"`: split the instance on `/` → outer `37`, inner `13`. Find top-level node `37`;
its `type` is a definition uuid (`ac99f841-…`). Find that entry in `definitions.subgraphs`. Find
interior node `13`, read `inputs[].link`, resolve it through the subgraph's **object-shaped**
`links`, and classify `origin_id` against the subgraph's own `inputNode.id`.

Read `inputNode.id` from the file rather than hardcoding `-10`. It is `-10` in this fixture
(`outputNode` is `-20`), and every interior node id is positive — but the file states it, so use
what it states.

**Anything that does not resolve stays `unchecked`.** A nested `A/B/C`, a definitions block that is
absent or does not carry the uuid, an interior node that is not there: report, do not guess. That
half of §18.5's instruction stands unchanged.

### 3. Drop the three dead addresses from the profile

```
"duration_s".slots: ["37/13.max_duration"]          (was + "37/15.seconds")
"seed".slots:       ["37/38.seed"]                  (was + "37/13.seed", "37/9.seed")
```

Update the two `profile.rs` fan-out tests to match. **Do not weaken them to `.contains()` over a
shorter list** — assert the exact slots, because the whole value of a fan-out test is that it
notices a slot leaving.

## Invariants, each with a test

1. **The seven-address live table is reproduced exactly.** One test walks all eight addresses of
   §18.5's table against the fixture and asserts each verdict. This is the contract test; if the
   flattener's behaviour ever diverges from this model, this is what says so.
2. **MiniMax's shipped profile has no inert slots** — the mirror of
   `test_no_inert_slots_in_shipped_ace_step_profile`, which has never existed for MiniMax and could
   not have meant anything if it had. With the same vacuity guards: addresses non-empty, `unchecked`
   empty, `link_fed` non-empty.
3. **A boundary-fed address is not inert** (`37/38.seed`).
4. **A backend-fed interior address is inert** (`37/13.seed` ← `SeedNode`).
5. **Object-shaped interior links are read as such** — the positional reader must not be reachable
   from the interior path, and vice versa.
6. **ACE-Step is unaffected.** It has `definitions: null`; every existing top-level test stands.
7. **What still cannot be resolved is still reported**: a nested `37/13/2.x`, and an interior node
   the subgraph does not have.

**The vacuity trap here is severe and worth naming.** `test_subgraph_address_is_unchecked` currently
asserts `37/6.unet_name` comes back unchecked. After this change it must come back *checked and
sound* — so that test is not "updated", it is **inverted**, and inverting a test to match new code is
exactly how a rule gets deleted by accident. It is replaced by invariant 1, which asserts a value
read off a live run rather than a value read off the implementation.

## Mutations

| # | mutation | must be killed by |
|---|---|---|
| M53 | `Boundary` classified as inert | invariant 3; MiniMax refuses to generate |
| M54 | `Backend` classified as not inert | invariant 4; the three dead addresses go unnoticed |
| M55 | `Unknown` classified as not inert | the existing missing-source test |
| M56 | interior links read positionally (`l.get(0)`) | invariant 5 — must fail, not silently return `None` |
| M57 | `inputNode.id` hardcoded to `-10` | a fixture whose boundary id differs |
| M58 | nested `A/B/C` resolved to `A/B` instead of unchecked | invariant 7 |
| M59 | the three dropped addresses restored to the profile | invariant 2 |
| M60 (control) | rename a private helper | nothing — validates the harness |

M59 is the one to watch. It is the same shape as M49 on the cancel task: the rule is an **absence**
(three addresses that are no longer written), and an absence needs a test that reads the profile,
not one that reads the audit.

## Gate

`cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `npm run gate`. create-core
139 → expect ~148. No frontend test count change: `unchecked_slots` keeps its type and its copy, and
`submissionNotes` is untouched — the list simply arrives empty for MiniMax.

## Click-through

One MiniMax generation. **The warning line should be gone entirely**, and the track should still
carry the seed and duration the panel asked for. One ACE-Step generation to confirm nothing
regressed on the top-level path.

If a MiniMax run instead fails with "writes … to inputs a node drives", the profile edit did not
land with the audit edit and this brief's central point was ignored.
