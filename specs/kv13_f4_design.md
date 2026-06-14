# KV13 F4 — stored PersistentId selector (lineage-anchored) — implementation spec

Status: DESIGN (2026-06-14). Implements the **F4 long pole** of
`specs/kv13_provenance_naming.md` (§F4, lines 170–212) IF the redemption gate
(INC-0) confirms a genuine non-redundant consumer. Scope:
`crates/waffle-types/` (one `Selector` variant + a defaulted `KernelIntrospect`
method + the captured-provenance record), `crates/feature-engine/`
(a `resolve_by_persistent_id` resolver + capture plumbing),
`crates/kernel-v2/` (one read-only adapter method over the EXISTING journal).
**No arena/seeding change.** Design context in `docs/PERSISTENT-NAMING.md`;
substrate in `specs/kv13_provenance_naming.md` §F1–F3.

> **Goal.** A **stored** `GeomRef` whose selector pins a specific face by its
> lineage root **survives a geometry-changing upstream edit** and re-resolves to
> the same semantic face — including the case where Role/Signature alone would
> **tie** among multiple similar faces. This is the one capability the
> F1–F3/F5/F6 recompute + Role/Signature path (proven 2026-06-14) does not
> already deliver.

## 0. Where we are (the empirical finding)

The F6a edit-survival probe (`specs/kv13_provenance_naming.md:174–183`,
`test-harness/face_provenance.rs::created_by_survives_an_upstream_edit`) proved
the **user-facing Phase-F goal is already MET** without F4 machinery:

- `created_by` **already survives** upstream parameter edits — `pid_to_feature`
  is recomputed every rebuild (F6a), so churned pids re-bind to their features
  each time.
- A *downstream* feature add does NOT re-execute upstream, so those pids are
  trivially stable (F4a holds for free in that case).
- A face → introducing-feature click survives an extrude-depth edit via
  recompute + the existing Role/Signature `GeomRef` resolution.

**The one genuine gap.** A **stored** `Selector::Pid`-style reference that pins
*one* face among **multiple Role/Signature-identical siblings**, and survives a
*geometry-changing* edit, has **no working path today** — Role/Signature
**ties** (resolves to the wrong sibling, or errors ambiguously). This is the
`PERSISTENT-NAMING.md §"What May Break"` tie case. It is also **consumer-less in
the shipped codebase**: the only natural consumer (fillet/chamfer edge/face
refs) is DEFERRED INDEFINITELY. F4 therefore ships **only if** INC-0 below
demonstrates the tie failure is real; otherwise F4 stays DEFERRED and no
machinery lands (per `specs/kv13_provenance_naming.md:185–192`).

## 1. Architecture

Lineage-anchored persistence: a stored selector names the **lineage root** of a
face (a stable, edit-invariant anchor proven by the F6a probe), plus a
deterministic ordinal among that root's current live descendants. Resolution is
a **pure function of `OpResult`** — the captured provenance snapshot carries the
root pid, so the resolver needs no live-kernel handle (matching the existing
`resolve_*` shape, `resolve.rs:43–60`).

### 1a. Seeding — UNCHANGED (explicit non-decision)

Keep monotonic `alloc_pid` + `assign_face_pids` (arena.rs:497–528) **exactly**.
No content/structural reseeding, no `SeedCtx`, no new arena field. The durable
guarantee we exploit is the one the F6a probe already proved: an unchanged
constructor reproduces identical `FaceId`s ⇒ identical seeded Pids run-to-run,
so the **root_pid of any constructor-introduced face is stable across an
upstream parameter edit**. Volatile boolean-OUTPUT pids stay volatile by design
— a stored reference is NEVER anchored on them, only on the lineage root.

**Content-keyed seeding is explicitly REJECTED** (see §2 Open Questions, Q5): it
crosses the kernel↔feature-engine layering boundary (feeds feature identity into
`finalize_solid`, violating kernel-v2 hard rule 1 / rule 6), changes arena
`Debug` output (forcing every determinism golden to regenerate), and makes the
ordinal depend on yang attribution-order stability (a fuzz-invisible measure-zero
hazard). That is maximal blast radius on the F1–F3 substrate for an OPTIONAL
feature.

### 1b. Selector shape — waffle-types

Add ONE variant to `waffle_types::Selector` (geom_ref.rs:63–75), before
`Position`:

```
PersistentId {
    root_pid: u64,             // lineage root, captured at authoring from FaceProvenance.root_pid
    ord: u32,                  // index into the FaceId-ascending live descendants of root_pid
    witness: Option<Box<Selector>>,  // OPTIONAL Role/Signature/Position tiebreak; BestEffort-only
}
```

- `root_pid` — the stable anchor; `FaceProvenance.root_pid` already exists
  (kernel/types.rs:44–51), captured at authoring time.
- `ord` — index into the deterministically ordered (FaceId-ascending) live
  descendants of the root, so single-face refs are `ord: 0` and a `Split` is
  addressable.
- `witness` — an embedded Role/Signature/Position selector captured alongside,
  used **ONLY** as the BestEffort tiebreak for `Split` ambiguity (a bare
  PersistentId has nothing to fall back on). **Strict never consults it.**

`GeomRef.kind` still carries `Face`/`Edge`/`Vertex`; `kind ∈ {Edge, Vertex}` →
typed `NotSupported` (loud), keeping the F4d deferral honest (§1e).

### 1c. Resolution — pure function of OpResult

Extend the captured created-entity provenance record (waffle-types, populated by
feature-engine `capture_face_pids` from the existing `face_provenance` query)
with two fields:

```
persistent_id:      Option<u64>,   // the face's own pid    (None under MockKernel)
root_persistent_id: Option<u64>,   // its lineage root pid  (None under MockKernel)
```

New resolver `resolve_by_persistent_id(op_result, root_pid, ord, witness, kind,
policy)` (new arm in `resolve_geom_ref`, resolve.rs:43–60, shaped like
`resolve_by_role`):

1. Collect the created `Face` entities whose captured `root_persistent_id ==
   root_pid`.
2. Sort by a stable key (the captured `KernelId`, i.e. FaceId order — the SOLE
   ordering authority for `ord`).
3. Pick index `ord`.

This needs **no live kernel handle at resolve time**. A defaulted
`KernelIntrospect::faces_from_root` (returning `Vec::new()`) is added ONLY as
the optional clean seam for a future direct-kernel query / F4d; the primary path
uses the captured snapshot, so MockKernel is unaffected.

### 1d. Remap = re-resolve, not rewrite

There is **no stored-pid rewrite and no migration pass**. The stored root is
re-resolved through the CURRENT journal every rebuild — exactly as F6's
`pid_to_feature` is recomputed every rebuild. Expressed entirely through the
existing `journal::descendants` (journal.rs:110–127) ∩ live faces, applied at
capture time when building provenance records. `EvoKind` cases:

- **Same / Trimmed** — root has exactly one live descendant ⇒ `ord: 0` binds it.
  (A constructor-only face has no boolean `modified` edge, so its own pid IS its
  root and it is live — handled correctly: an untouched face is its own root.)
- **Split** — N > 1 live descendants in FaceId-sorted order; `ord` selects the
  piece. `ord` out of range → Strict typed `ResolutionFailed` listing
  candidates. BestEffort uses `witness` (if present) to disambiguate + warn;
  with **no** witness, `Split` is a Strict-style typed failure even under
  BestEffort (loud over wrong).
- **Merge** — a reference keyed on any merged-away root still reaches the
  surviving merged face via `descendants` ⇒ resolves with an absorption warning.
  Multi-merge ord-COUNTING is deferred to F4c because F2 records only the first
  input→output merge edge (journal.rs:74–76); resolution still SUCCEEDS, only
  ord-among-merge-contributors is unavailable (documented, not papered over).
- **Vanished** — `descendants ∩ live == ∅` → Strict typed `ResolutionFailed {
  reason: "PersistentId root {n} has no live descendants" }`. BestEffort falls
  to `witness`/Signature among kind-matching created entities + warns (mirrors
  the existing Role→Signature fallback, resolve.rs:96–111).

### 1e. Edges / vertices (F4d) — out of scope

Explicitly deferred. The journal records FACE pids only (journal.rs:41–53); no
edge/vertex journal exists; the only consumer (fillet/chamfer) is DEFERRED
INDEFINITELY — building it now is consumer-less. The selector shape
(`root_pid` + `ord` + `kind`) already accommodates edges/vertices with **no
schema change**; gate the implementation behind a real fillet/chamfer revival.

## 2. Soundness

- **Determinism (P-determinism, spec §3).** ZERO change to `alloc_pid` /
  `assign_face_pids` / `next_pid` / `face_pids` / the journal. Nothing touched is
  in the arena's canonical `PartialEq`/`Debug` state; the new provenance fields
  are rebuild-derived, not arena state. The `descendants` BFS over the
  append-only journal + the FaceId-sort is order-deterministic. The selector
  stores `u64` + `u32` only (no `f64`, no exact-equality hazard). The determinism
  oracle stays green by construction.
- **P9 (no silent mislabel).** A vanished root → empty descendant set → typed
  failure or witnessed/warned fallback, **never** an arbitrary face. An `ord`
  overflow → typed failure or witnessed pick. A yang-`generated` face is its own
  root and resolves to itself or fails loudly.
- **P10 (loud failure).** Every non-unique / vanished / overflow path returns
  `EngineError::ResolutionFailed` with a reason, surfacing through the existing
  error-toast path. BestEffort never silently picks among a tie — it warns.
- **Layering (kernel-v2 hard rule 1 / rule 6).** All new logic lives in
  waffle-types (a type) + feature-engine (a resolver). The kernel gains only one
  **read-only defaulted** trait method over the existing journal. **No feature
  identity enters the kernel** (the line Design 1 would have crossed).

## 3. Increments (each gates the next; RED→GREEN)

- **INC-0 — REDEMPTION GATE (build first, NO production code).** Add
  `test-harness/tests/kv13_f4_stored_pid.rs::role_signature_cannot_disambiguate_similar_faces_today`.
  Construct a model where a stored selection targets ONE face among MULTIPLE
  Role/Signature-identical faces (the `PERSISTENT-NAMING.md §"What May Break"`
  tie case) and a geometry-changing upstream edit occurs.
  - **RED assertion:** today's Role/Signature resolution TIES or mis-binds
    (resolves to the wrong sibling, or errors ambiguously).
  - **GO/NO-GO:** if this test does **not** fail today, **STOP** and report that
    F4 stays DEFERRED (`specs/kv13_provenance_naming.md:185`) — the capability is
    redundant and machinery must not ship without a failing consumer. This is the
    honest go/no-go.

- **INC-1 — consumer RED (fails today).** Add
  `stored_persistent_id_disambiguates_via_lineage` in the same file. Build the
  INC-0 tie scenario; capture `root_pid` (via `face_provenance`) of the specific
  target face; author a `GeomRef` with `Selector::PersistentId { root_pid,
  ord: 0, witness: Some(<the role selector>) }`; perform the geometry-changing
  edit; re-resolve.
  - **RED:** fails to compile first (no variant), then fails at runtime (no
    resolution).
  - **GREEN later (INC-5):** binds to the SAME semantic face the bare Role could
    not pin. This is the consumer **and** the proof of non-redundant value.

- **INC-2 — Selector variant + serde.** Add `Selector::PersistentId { root_pid:
  u64, ord: u32, witness: Option<Box<Selector>> }` to waffle-types
  geom_ref.rs (before `Position`; serde tag). `resolve_geom_ref` match gains a
  stub arm returning typed `Unimplemented` so the crate compiles. Wire through
  wasm-bridge serialization (anti-bit-rot, even with no UI author).
  - **RED → GREEN:** new serde round-trip unit test passes; determinism oracle +
    all kernel-v2 tests green (no arena change); INC-0/INC-1 now COMPILE and fail
    at the stub.

- **INC-3 — capture plumbing.** Extend the captured created-entity provenance
  record (waffle-types) with `persistent_id: Option<u64>` +
  `root_persistent_id: Option<u64>`; populate in feature-engine
  `capture_face_pids` from the existing `face_provenance` query (`None` under
  MockKernel).
  - **RED → GREEN:** after a single extrude under kernel-v2, created `Face`
    entities carry `Some(pid)` + `Some(root_pid)`; under MockKernel both `None`.

- **INC-4 — kernel seam `faces_from_root`.** Add
  `KernelIntrospect::faces_from_root(solid, root_pid) -> Vec<KernelId>` with
  default `Vec::new()` (traits.rs:177) and implement in adapter.rs using
  `journal::descendants ∪ {root-if-live} ∩ live faces`, FaceId-sorted.
  - **RED → GREEN:** kernel-v2 unit tests — extrude→union, `faces_from_root(side
    root)` returns exactly that face's current `KernelId`; a vanished root
    returns empty; assert the FaceId-sort is the SOLE ordering authority (`ord`
    stability). MockKernel still returns empty.

- **INC-5 — resolver (INC-1 RED → GREEN).** Implement
  `resolve_by_persistent_id` in feature-engine (replace the INC-2 stub),
  dispatching from `resolve_geom_ref`, operating on the captured provenance
  snapshot, applying Same/Trimmed single-bind + vanished →
  Strict-typed-fail / BestEffort-witness-fallback.
  - **RED → GREEN:** INC-1 goes GREEN. Add a Strict deleted-upstream → typed
    `ResolutionFailed` test.

- **INC-6 — Split branch.** A boolean that cuts the referenced face in two.
  - **RED → GREEN:** assert `ord: 0` / `ord: 1` resolve deterministically to the
    two pieces; `ord: 2` → Strict typed failure listing candidates; BestEffort
    WITH witness → witnessed pick + warning; BestEffort WITHOUT witness → still a
    typed failure (loud over wrong). Add an equal-area/equal-witness tie test
    asserting a warning is emitted, **never** a silent pick.

- **INC-7 — incremental-rebuild safety.** A `from_index > 0` partial-rebuild
  variant of INC-1 that does NOT re-execute the boolean.
  - **RED → GREEN:** assert the stored root still resolves (unchanged features
    keep their pids; `faces_from_root` reads the current arena consistently).
    Verify before claiming incremental-safe.

## 4. Open questions (human confirmation required)

1. **GO/NO-GO: does INC-0 actually fail today?** Per
   `specs/kv13_provenance_naming.md:174–188`, F4 is OPTIONAL because recompute +
   Role/Signature already meets the user-facing goal. If no tie/restructure case
   genuinely defeats Role/Signature, the correct outcome is to **ship nothing**
   and keep F4 deferred. A human must confirm there is a real consumer (a stored
   selection that survives a topology restructure) before building — otherwise
   this is consumer-less machinery.
2. **Is the embedded `witness: Option<Box<Selector>>` worth the serde/recursion
   complexity**, or should `Split`-without-disambiguation simply be a hard
   Strict-style failure under **both** policies? The witness is the only
   non-trivial schema surface; a human should decide whether BestEffort `Split`
   disambiguation is a required capability or can be deferred.
3. **Merge ord-counting.** F2 records only the first input→output merge edge
   (journal.rs:74–76). Resolution SUCCEEDS for merge, but ord-among-merge-
   contributors is unavailable. Confirm no near-term consumer needs to address a
   specific contributor of a coplanar-merged face before deferring the full
   merge edge-set to F4c.
4. **`ord` ordering authority.** The design fixes `ord` as **FaceId-ascending
   over descendants**, NOT journal-BFS order. Confirm this is the intended stable
   contract and document it — a future change making `descendants()` order
   load-bearing would silently shift `ord` across rebuilds.
5. **Reject content-keyed seeding for the record.** It crosses the
   kernel↔feature-engine layering boundary (feeds feature identity into
   `finalize_solid`) and changes arena `Debug` (forcing golden regeneration).
   Confirm with a human that reseeding is off the table, so a future contributor
   does not reintroduce the "just make output pids stable" trap (a
   determinism-oracle hazard if it ever drifts to f64/geometry keys).
