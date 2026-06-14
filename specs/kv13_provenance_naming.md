# KV13 Spec — provenance / topological naming (Parasolid-grade)

Status: PLANNED (spec, 2026-06-14). Prototype-release **Phase F** (the capstone,
strictly after the gearbox print). Scope: `crates/kernel-v2/` (persistent tags +
operation journal — the bulk), `crates/waffle-types/` (`FaceOrigin`, a
PID-based `Selector`), `crates/feature-engine/` (rebuild-time lineage),
`crates/wasm-bridge/` + `app/` (face→feature UI). Driven by
`docs/prototype_release_roadmap.md`; design context in
`docs/PERSISTENT-NAMING.md`.

> **Goal.** "Click any face → the feature that *introduced* it, through chained
> booleans/extrudes," plus the inverse (feature → its faces), **surviving
> rebuilds** — including upstream-sketch edits. Per the Phase-F decision
> (2026-06-14) the target is **full Parasolid-grade naming**: persistent entity
> tags + an operation journal *integrated into the kernel*, not the
> role+signature heuristic alone. Role/signature become fallbacks; the journal
> is the source of truth.

## 0. Where we are (substrate inventory)

- **yang-rs** — `TriangleAttribution { input: InputId, input_face_idx }` +
  `TriangleAttributionMap` (per output triangle, 2-of-3 majority vote, `None`
  when no winner). Populated by `boolean()`. This is the raw signal for
  attributing boolean-output faces to operand input faces.
- **kernel-v2** — arena entities (`VertexId`/`HalfEdgeId`/`FaceId`/…) are
  **array-index handles that churn** on every rebuild; there is NO persistent
  tag and NO operation journal today. `boolean.rs` calls yang but **discards**
  the `TriangleAttributionMap` (the core F2 gap). `validate_solid` + the
  determinism oracle gate every operator.
- **waffle-types** — `GeomRef { kind, anchor: Anchor::FeatureOutput{feature_id,
  output_key}, selector, policy }`; `Selector::{Role, Signature, Query,
  Position}`; `Role` (EndCapPositive/Negative, SideFace, Boolean*Face, …);
  `OutputKey` with a stable `.tag()`. NO `FaceOrigin` yet.
- **modeling-ops** — `Provenance { created, deleted, modified: Vec<Rewrite>,
  role_assignments }`, `EntityRecord`, `Rewrite{before,after,reason}`. This is
  the *legacy/truck* provenance shape; KV13 re-homes the equivalent in
  kernel-v2's journal (the app runs kernel-v2, not modeling-ops).
- **Phase D (DONE)** — `getSelectedRefFeatureId()` reads the selected
  `GeomRef`'s `FeatureOutput.feature_id` → the body's **last** producing
  feature. Exact for single-feature bodies; the ceiling (a boolean body reports
  the boolean, not the original extrude) is exactly what KV13 lifts.

## 1. Architecture

Two layers, kernel-owned truth + types-level resolution:

### 1a. Persistent entity tags (`Pid`) — kernel-v2

A `Pid(u64)` stamped on each topological entity (faces first; edges/vertices in
a later sub-increment), **distinct from the churning `FaceId`**. A monotonic
per-arena allocator assigns them. The invariants:

- **Deterministic:** same operation sequence on same inputs ⇒ same Pids (the
  determinism oracle must stay green — Pids are part of the canonical arena).
- **Preserved through MODIFY:** an entity that an operation *carries through*
  (a face that survives a boolean unchanged, an edge that a rebuild reproduces)
  keeps its Pid. An entity that is *generated* (a boolean cut face) gets a fresh
  Pid tagged with the creating operation.
- **Seeded for re-derivation (F4):** fresh Pids are seeded from a stable key
  (operation identity + input-entity identity + a structural index), NOT arena
  order, so re-executing an unchanged operation reproduces the same Pids.

### 1b. Operation journal — kernel-v2

Per kernel operation, an `Evolution` record:

```
Evolution {
    op: OpTag,                       // Extrude / Revolve / BooleanUnion / … + feature id
    generated: Vec<Pid>,             // new entities (no input ancestor)
    modified: Vec<(Pid /*in*/, Pid /*out*/, EvoKind)>,  // GENERATED-from lineage
    deleted:  Vec<Pid>,              // consumed inputs
}
enum EvoKind { Same, Split, Merge, Trimmed }   // Parasolid-style evolution
```

The journal is the operation history. Lineage = the transitive closure of
`modified` edges back to a `generated` entity, which carries the feature that
introduced the geometry. This is the Parasolid "operation journal" / OCCT
"evolution labels (GENERATED/MODIFIED/DELETED)" model.

### 1c. `FaceOrigin` + resolution — waffle-types

```
FaceOrigin {
    created_by: FeatureId,           // the feature that INTRODUCED this face
    derived_from: Vec<FeatureId>,    // the lineage chain (operands, edits)
}
```

Resolved by walking the journal/Pid lineage. A new `Selector::PersistentId
{ pid }` becomes the PRIMARY GeomRef selector (most robust); `Role` then
`Signature` remain fallbacks for when a Pid can't be carried (a freshly
introduced reference, or a Pid lost to a massive restructure).

## 2. Increments (each gates the next; RED→GREEN; independently valuable)

Ordering note: **F1–F3 + F5 + F6 deliver "click face → creating feature through
booleans" for the *current* model** — the headline capability — and are
gearbox-grade on their own. **F4 is the long pole** that makes it survive
arbitrary upstream edits (the full-Parasolid promise); it is multi-week and
sub-divided. Ship F1–F3/F5/F6 first; F4 hardens.

- **F1 — `Pid` tags in the kernel.** Add `Pid(u64)` + a monotonic allocator to
  `BrepArena`; a `face_pid: HashMap<FaceId, Pid>` side-table (edges/vertices
  deferred to F1b). Every constructor (extrude, revolve, arc extrude, circle,
  lamina) stamps fresh Pids on its output faces. `validate_solid` asserts every
  shell face has a Pid; the determinism oracle includes Pids. Introspection
  (`FaceRange`) carries the face Pid out. **Tests:** Pids present/unique/
  deterministic (bit-identical arenas ⇒ identical Pid maps); a re-run of the
  same extrude yields the same Pids.

- **F2 — operation journal + boolean attribution.** Add the `Journal` and
  `Evolution` types; each operation appends a record. **Boolean is the crux:**
  consume the discarded `TriangleAttributionMap` — for each output BRep face,
  the majority `(InputId, input_face_idx)` over its triangles ⇒ MODIFY (inherit
  the operand face's Pid, `EvoKind::Same`/`Trimmed`) or, when the face is new
  (no consistent operand attribution / a cut face along the intersection),
  GENERATED with a fresh Pid tagged to the boolean. **Loud on `None`
  attribution** (P9 — never silently guess; a face that yang can't attribute is
  a typed error, not a wrong label). **Tests:** box∪box and box−cyl journals —
  inherited faces carry operand Pids, cut faces are `generated` by the boolean;
  attribution is total (no silent `None`).

- **F3 — `FaceOrigin` for the current model.** Walk the journal lineage from a
  face's Pid back to its `generated` origin → `created_by` feature +
  `derived_from` chain. **Tests:** a face on a unioned body resolves to the
  original extrude (NOT the boolean); a boolean cut face resolves to the
  boolean; a 3-deep chain (extrude→union→subtract) resolves correctly; the
  inverse (feature → its current faces) enumerates.

- **F4 — persistent identity across rebuild (THE long pole, Parasolid-grade).**
  Make Pids + the journal survive a feature-tree rebuild so downstream GeomRefs
  and `FaceOrigin` re-resolve to the SAME entity after upstream edits.
  - **F4a — stable Pid seeding.** Re-derive Pids from a content/structural key
    (operation + input identity + role/index), so re-executing an unchanged
    feature reproduces identical Pids. RED: change a *downstream* feature, prove
    upstream Pids unchanged.
  - **F4b — journal remap across re-execution.** When a feature re-runs with
    *changed* geometry, the new journal's `modified` edges map old Pids → new
    Pids (Same/Split/Merge/Trimmed). Downstream Pid references re-bind. RED:
    change a sketch dimension (box wider), prove a downstream face reference
    re-resolves.
  - **F4c — split/merge/vanish.** Handle an input face that splits into two
    (which keeps the Pid? — both get derived Pids linked to the parent), merges,
    or vanishes (reference fails gracefully → `Strict` errors, `BestEffort`
    falls to signature). RED: PERSISTENT-NAMING.md scenarios 2, 4, 5.
  - **F4d — edges + vertices.** Extend Pids/journal to edges and vertices (F1
    did faces only) — fillet/chamfer references need edges. (Fillet/chamfer
    themselves stay DEFERRED; this is just the naming substrate.)
  - **Boundary (documented):** even Parasolid-grade naming cannot resurrect
    truly vanished geometry or disambiguate exact pattern duplicates without a
    user choice — `BestEffort` + notify, per PERSISTENT-NAMING.md §"What May
    Break". State it; don't pretend.

- **F5 — `get_face_data` emits `created_by_feature`.** Thread `FaceOrigin`
  through the `Kernel`/`KernelIntrospect` trait + `KernelV2Adapter` + wasm-bridge
  so the app gets a resolved feature id (+ lineage) per face. **Tests:** trait
  + wasm-bridge round-trip.

- **F6 — UI: face → feature (through booleans) + inverse.** Supersede Phase D
  Tier 1: clicking a face highlights the feature that *introduced* it (lineage,
  not last-feature); selecting a feature highlights *its* faces. **GUI tests:**
  union-then-click-original-face → original extrude highlighted; feature→faces.

- **F7 — verification matrix.** The five PERSISTENT-NAMING.md scenarios
  (stability, role→signature fallback, feature-insertion, graceful break,
  over-constrained) as automated tests, **plus an adversarial no-mislabel pass**
  (a wrong feature highlight is worse than "unknown" — assert the system says
  "unknown/ambiguous" rather than confidently wrong).

## 3. Invariants / discipline

- **P9/P10:** a face yang cannot attribute is a LOUD typed error, never a
  guessed label. A reference that cannot be re-resolved fails per its
  `ResolvePolicy` (Strict → rebuild error; BestEffort → closest + warning),
  never a silent wrong bind.
- **Determinism:** Pids are part of the canonical arena; the determinism oracle
  must stay green. Same inputs ⇒ same Pids ⇒ reproducible journals.
- **No kernel-internal mutation leakage** (kernel-v2 hard rule 6): the journal
  is append-only per operation, derived from operator I/O, not a cache mutated
  across features.
- **Composition:** Pids ride alongside existing geometry; tessellation /
  boolean / validate are unaffected in their geometric behavior (Pids are
  metadata). Booleans on Tier-2 arc results still hit the KV7 wall — orthogonal.

## 4. Acceptance

- F1–F3: current-model face→feature lineage through booleans, exact, with the
  inverse; attribution total (no silent `None`).
- F4: a downstream face reference + face→feature survive a parameter change and
  a moderate topology change (PERSISTENT-NAMING scenarios 1–3, 5); scenario 4
  fails gracefully.
- F5–F6: the app shows the *introducing* feature on click (through booleans) and
  the inverse; GUI green.
- F7: all five scenarios + the adversarial no-mislabel pass automated and green.

## 5. Risks

- **F4 is genuinely multi-week and kernel-deep** — stable Pid seeding + journal
  remap is the hard core of persistent naming (the problem FreeCAD/OCCT/Parasolid
  each spent years on). Mitigate: ship F1–F3/F5/F6 (current-model lineage) first
  for immediate value; build F4 sub-increment by sub-increment behind the same
  GeomRef API, each with its own RED scenario.
- **Boolean attribution gaps** — yang `None`-attribution on degenerate triangles.
  Mitigate: loud typed error + a targeted fixture; treat a gap as a yang/cherchi
  bug to fix upstream, not to paper over here.
- **Determinism drift** — adding Pids must not perturb the determinism oracle.
  Mitigate: seed Pids deterministically and include them in the oracle from F1.
