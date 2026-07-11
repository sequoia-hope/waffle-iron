# SPEC — Optional booleans & explicit multi-body targeting (Extrude, Increment 1)

**Status:** DESIGN (pre-code). Author: 2026-07-01. FIP cycle required
(spec → failing tests → impl → adversary; Test Author ≠ Implementer).
Roadmap: user directive (2026-07-01) — make booleans optional, build full
multi-part capability into the file format, UI, and all dialogs that previously
relied on auto-boolean. This increment is the **engine-core vertical slice for
Extrude**; bridge/UI/viewport/other-features are Increments 2–5 (see task list).

---

## 1. Goal (user-visible behavior)

When a user creates (or edits) an **Extrude**, they choose how it combines with
existing bodies:

- **New Body** — the extrude becomes a separate, independent body; no boolean.
- **Add** — the extrude unions into a set of target bodies.
- **Cut** — the extrude subtracts from a set of target bodies.
- **Intersect** — the extrude intersects with a set of target bodies.

The target set may be **none** (⇒ effectively a new body), **one**, or
**multiple** bodies within the current part. If the user does not pick an
explicit target set (the default), the target set is computed automatically as
**every existing body that shares a face with the selected sketch geometry**
(§4.3). If no body shares a face, the extrude is a new standalone body.

This replaces the current hard-coded "auto-boolean against the most recent
solid" behavior for **newly created** features. Existing `.waffle` files
continue to rebuild byte-identically (§6).

## 2. Parameters

New fields on `ExtrudeParams` (`crates/feature-engine/src/types.rs:163`):

| Field | Type | Default (serde) | Meaning |
|---|---|---|---|
| `combine` | `Option<CombineMode>` | `None` (`#[serde(default)]`) | The boolean mode. `None` ⇒ legacy file: derive from `cut`/`merge` (§6). |
| `targets` | `Option<Vec<GeomRef>>` | `None` (`#[serde(default, skip_serializing_if="Option::is_none")]`) | Explicit target bodies. `None` ⇒ Auto (share-a-face, §4.3). `Some([])` ⇒ forced new body / no targets. `Some([b0,..])` ⇒ exactly those bodies. |

New enum (`types.rs`), serde-tagged for forward-compat like `BooleanOp`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CombineMode { NewBody, Add, Cut, Intersect }
```

Retained legacy fields (NOT removed — migration inputs): `cut: bool`,
`merge: bool` (`#[serde(default = "default_true")]`), `target_body: Option<GeomRef>`.

`CombineMode` has **no `Union`/`Subtract` naming** — it is the *user-facing verb*
set. It maps to `modeling_ops::BooleanKind` as: `Add→Union`, `Cut→Subtract`,
`Intersect→Intersect`, `NewBody→` (no boolean).

- **Valid ranges / error conditions:** `Cut`/`Intersect` with a resolved target
  set that is **empty after auto-computation** ⇒ `EngineError::ResolutionFailed`
  ("Cut/Intersect requires at least one target body") — a loud STOP, never a
  silent new-body fallback (that would be silently-wrong: a cut that cuts
  nothing must not masquerade as a boss). `Add` with an empty resolved set ⇒
  standalone body, **silently** (Add-into-nothing is a benign no-op union,
  matching today's `merge` fallback at `rebuild.rs:578`). *Amended 2026-07-11
  (user directive): this case originally warned, but the dialog defaults to
  `Add`, so every first extrude on an empty document hit the warning, which is
  baked into the feature's diagnostics and replayed as a toast on every
  rebuild — pure noise for the most common flow.*

## 3. Normalization (Constitution §7 — early, once)

At the top of the `Operation::Extrude` arm, normalize the persisted params into
one internal struct before any geometry:

```rust
struct EffectiveCombine {
    mode: CombineMode,           // NewBody | Add | Cut | Intersect
    targets: TargetStrategy,     // ShareAFace | MostRecentLegacy | Explicit(Vec<GeomRef>)
}
```

Rules:

1. `combine = Some(m)` (new files):
   - `mode = m`.
   - `targets = None` ⇒ `TargetStrategy::ShareAFace`.
   - `targets = Some(list)` ⇒ `TargetStrategy::Explicit(list)` (empty list allowed).
2. `combine = None` (legacy files) — derive from legacy fields, **preserving
   today's exact behavior**:
   - `cut == true` ⇒ `mode = Cut`, `targets = MostRecentLegacy`.
   - `cut == false && merge == true` ⇒ `mode = Add`, `targets = MostRecentLegacy`.
   - `cut == false && merge == false` ⇒ `mode = NewBody`.
   - `target_body = Some(gr)` (legacy, currently never written by UI) ⇒
     overrides `targets = Explicit(vec![gr])`.

`MostRecentLegacy` resolves via the **existing** `find_most_recent_solid`
(`rebuild.rs:994`) — one handle — so legacy files are byte-identical. New
features never produce `MostRecentLegacy`.

No repeated branch checks downstream: after normalization, all geometry code
consumes `EffectiveCombine` only.

## 4. Behavioral branches

### 4.1 Branch table

| `combine` | `targets` | Resolved target set | Behavior |
|---|---|---|---|
| `Some(NewBody)` | (ignored) | ∅ | standalone body, no boolean |
| `Some(Add)` | `None` | share-a-face bodies | union tool into each; ∅ ⇒ standalone, silent (amended 2026-07-11) |
| `Some(Add)` | `Some([])` | ∅ | standalone body, no boolean |
| `Some(Add)` | `Some([b..])` | those bodies | union tool into each |
| `Some(Cut)` | `None` | share-a-face bodies | subtract tool from each; ∅ ⇒ **error** |
| `Some(Cut)` | `Some([b..])` | those bodies | subtract tool from each |
| `Some(Intersect)` | `None`/`Some([b..])` | share-a-face / those | intersect tool with each; ∅ ⇒ **error** |
| `None` (legacy, cut) | — | most-recent solid | subtract (byte-identical to today) |
| `None` (legacy, merge) | — | most-recent solid | union (byte-identical to today) |
| `None` (legacy, neither) | — | ∅ | standalone (byte-identical to today) |

### 4.2 Multi-target feature scope

For a resolved target set `{b0, b1, …}` with `n ≥ 1`:

- **Add:** fold the tool into the targets by successive union. Result: the union
  `((b0 ∪ b1 ∪ … ) ∪ tool)`. The targets are **consumed** (they merge into one
  body). Order-independent for a valid union.
  **Disjoint operands (amended 2026-07-06, C0079-F1):** this is a SET union.
  A pairwise union of disjoint operands legitimately yields multiple lumps
  (kernel `boolean_union_multi`); the fold must carry every lump, never
  `.first()`. Implementation: targets fold into pairwise-disjoint lumps, then
  the tool sweeps the lump list, merging every lump it touches (a tool that
  bridges disjoint targets merges them all into one body). A target lump the
  tool never reaches survives as its own output body (`Body{index}`) with a
  warning naming it — silent material loss is prohibited.
- **Cut:** `b_i' = b_i − tool` for each `i`, independently. Result: `n` bodies
  (each target minus the tool). Targets are **replaced** by their cut versions;
  the tool is consumed (not emitted).
- **Intersect:** `b_i' = b_i ∩ tool` for each `i`. Result: `n` bodies. Same
  consumption as Cut.

Emit each resulting body as an `OutputKey`: the first as `Main`, the rest as
`Body{index}` (mirror `modeling_ops::boolean` at `boolean.rs:54-76`). Body-name
inheritance (`Engine::recompute_body_name_inheritance`, `lib.rs:264`) must follow
the same target set — see §5 consumption.

### 4.3 Share-a-face default (the automatic target set)

Given the extrude's sketch (plane origin `o`, plane normal `n̂`, and the selected
profile footprint `P` in that plane), and the set `B` of all **live bodies**
present before this feature (every non-consumed prior feature's `Main` + `Body{}`
outputs), a body `b ∈ B` is a default target iff EITHER:

- **(a) Anchor ownership:** `sketch.plane.anchor` is
  `Anchor::FeatureOutput { feature_id, output_key }` and `b` is the body produced
  by `(feature_id, output_key)` (following consumption/inheritance to the body
  that now carries that face). Cheap primary signal — a sketch drawn *on* a body
  face is owned by that body.
- **(b) Geometric coincidence + overlap:** `b` has a planar face `f` whose
  supporting plane is coincident with the sketch plane — `|n̂ · n̂_f| > 1 − TAU_MODEL`
  (parallel) AND `|(c_f − o) · n̂| < TAU_MODEL` (same offset, `c_f` = face
  centroid) — AND `f`'s 2D projection into the sketch plane **overlaps** the
  profile footprint `P` (2D area of intersection `> 0`). The overlap guard
  prevents auto-merging with a coplanar body sitting *beside* the profile, not
  under it.

Default target set = `{ b ∈ B : (a) ∨ (b) }`. Empty ⇒ new standalone body.

Coincidence uses `KernelIntrospect`: `list_faces(handle)` +
`compute_signature(face_id, TopoKind::Face)` for normal + centroid; the profile
footprint is `sketch.solved_profiles[profile_index]` (or `region`/`regions`)
projected via the existing `project_loop_2d` frame. Overlap reuses
`waffle_types::union_regions` / a 2D polygon-overlap predicate (whichever the
region module already exposes; do not hand-roll a new clipper).

**Open point flagged for the user:** (b) requires *profile overlap*, not merely
plane coincidence. If the user wants plain plane-coincidence (auto-merge any
coplanar-faced body regardless of footprint), drop the overlap term. Recommended:
keep overlap (predictable, avoids surprise merges). — Resolve before impl.

## 5. Consumption, rendering, inheritance

`find_consumed_feature_ids` (`rebuild.rs:964`) currently returns the single
most-recent target for `Extrude` when `merge || cut`. It must be generalized to
return the feature ids of **all resolved targets** for the new path:

- `Add`/`Cut`/`Intersect` with resolved set `{b_i}` ⇒ consumed = the feature ids
  backing each `b_i` (from each target `GeomRef`/body's `Anchor::FeatureOutput`).
- `NewBody` ⇒ consumed = ∅.
- Legacy (`combine=None`) ⇒ unchanged (`find_most_recent_consumed`).

This keeps the "consumed features don't render as separate bodies" invariant
(`collect_renderable_bodies`, `wasm_api.rs:537`) correct for multi-target ops,
and lets body-name inheritance propagate custom names from each consumed target.

## 6. Backward compatibility & migration

- **Serde:** `combine` and `targets` are `#[serde(default)]`; old files lack them
  ⇒ `combine = None` ⇒ legacy normalization ⇒ identical behavior. No
  `FORMAT_VERSION` bump is required for *loading* (additive optional fields).
- **DECISION (N-mb-4): the `FORMAT_VERSION` 3→4 bump is DEFERRED, not done.**
  The bump's only value is letting a strictly-older binary *reject* a file that
  contains `combine` (rather than silently ignoring the unknown field and
  treating the extrude as legacy). Weighed against that: (a) the fields are
  serde-default so every old file loads and every new file loads on this
  codebase; (b) this is a personal experiment with no released older binaries to
  protect; (c) bumping breaks existing tests that pin v3 semantics
  (`migrate_unsupported_version_returns_error` *expects* `migrate(3,4)` to have no
  path and error; a hardcoded `version == 3` round-trip assertion) for marginal
  benefit. When a released version needs protecting, bump then (make `migrate(3,4)`
  a no-op content migration and update those two tests). The back-compat guarantee
  is instead pinned by the `load_old_file_without_combine` regression test.
- **Regression test (required):** `load_old_file_without_combine` — a v3 file
  with `cut`/`merge` but no `combine`/`targets` loads and rebuilds to the same
  geometry as before (mirror `load_old_file_without_body_names`,
  `format_tests.rs:454`).

## 7. Invariants (measurable)

1. **Legacy invariance:** for any `ExtrudeParams` with `combine = None`, the
   rebuilt geometry (V, E, F, signed volume, bbox, body count) equals the
   pre-change result exactly. (`fuzz_boxes` 900/900 unchanged; existing extrude
   tests unchanged.)
2. **New-body isolation:** `combine = Some(NewBody)` (or `Add` with empty
   resolved set) ⇒ body count = (prior live body count) + 1, and the new body's
   signed volume = the standalone prism volume `area(P)·|depth|`.
3. **Add merge:** `Add` into a single overlapping target ⇒ result body count =
   prior count (tool + target merge into one); signed volume =
   `vol(target) + vol(tool) − vol(overlap)`; watertight, Euler χ=2.
4. **Cut scope:** `Cut` from `n` targets ⇒ each result body volume =
   `vol(b_i) − vol(b_i ∩ tool)`; body count = `n` (targets replaced); tool not
   emitted.
5. **Share-a-face determinism:** the default target set is a pure function of
   (sketch plane, profile footprint, live bodies) — no dependence on feature
   ordering beyond body liveness. Recomputing it twice yields the same set.
6. **No silent-wrong cut:** `Cut`/`Intersect` resolving to ∅ targets ⇒ a loud
   `ResolutionFailed`, never a standalone body.

## 8. Oracles (per branch)

- Volume within `1e-9` (planar prisms are exact) for branches 2–4.
- Body count exact (usize equality) for NewBody / Add / Cut / Intersect.
- `check_watertight_2manifold` + Euler χ=2 on every emitted body.
- Bounding-box equality within `TAU_MODEL` for the standalone-prism and
  merged-body cases.
- Share-a-face: a fixture with (i) a body the sketch is drawn on, (ii) a coplanar
  body off to the side (no overlap), (iii) a non-coplanar body ⇒ default set =
  {i} only. Assert exact membership.

## 9. Failure modes

| Input | Handling |
|---|---|
| `Cut`/`Intersect`, resolved targets ∅ | `EngineError::ResolutionFailed` (loud) |
| `Add`, resolved targets ∅ | standalone body + `diagnostics.warnings` note |
| target `GeomRef` fails to resolve (deleted/rolled-back body) | per `ResolvePolicy`: `Strict` ⇒ error; `BestEffort` ⇒ drop that target + warn |
| union of two overlapping targets is itself a NotSupported boolean (coplanar/curved) | propagate the typed kernel error (do NOT swallow — this is real geometry, not the benign Add-into-nothing case) |
| profile empty / index OOR | existing `ProfileOutOfRange` (unchanged) |

## 10. Research basis (REFERENCES.md)

Multi-body / feature-scope modeling follows the parametric-solid-modeling
convention (Mäntylä [#16], Stroud [#33]): a feature produces bodies; boolean
combination is an explicit per-feature choice with a body scope, not an implicit
global merge. "Merge result" + "feature scope / bodies to affect" mirrors
mainstream MCAD (SolidWorks/Onshape). No new geometric algorithm is introduced —
this increment is *targeting and dispatch* over the existing boolean.

## 10a. Analytical vs approximate (FIP §7a)

**No new surface-surface intersection is introduced.** This increment only
selects *which* existing bodies the already-existing `execute_boolean`
(`modeling-ops/src/boolean.rs`) is invoked against, and how many result bodies
are emitted. All boolean math remains the Yang/cherchi analytical-primacy
pipeline (A15). Surface-pair coverage is unchanged from the current boolean.
The only new geometry predicate is the **plane-coincidence + 2D profile-overlap**
test in §4.3, which is exact (dot products vs `TAU_MODEL`, 2D polygon area) and
uses existing region machinery — no mesh approximation.

## 11. Decomposition (this increment only — Extrude engine core)

1. **N-mb-1** — types + normalization: add `CombineMode`, `combine`, `targets`;
   write `normalize_extrude_combine` producing `EffectiveCombine`; legacy
   mapping. No behavior change yet (all new-path branches route through legacy
   until wired). Unit tests for the normalization table.
2. **N-mb-2** — dispatch: replace the `if params.cut / else if params.merge`
   block (`rebuild.rs:548-596`) with an `EffectiveCombine` match; single-target
   path first (Add/Cut/Intersect against a resolved single body or MostRecent).
   Green the legacy-invariance tests.
3. **N-mb-3** — share-a-face default target computation (§4.3) + multi-target
   scope (§4.2) + consumption generalization (§5).
4. **N-mb-4** — file-format `FORMAT_VERSION` 4 + migrate v3→v4 + the
   `load_old_file_without_combine` regression test.

Each sub-increment is independently committable and assay-gated (the kernel-v2
assay must stay 0 SUPPORTED_WRONG; `./scripts/test.sh fast` green).

## 12. Open questions for the reviewer

1. §4.3(b): keep the **profile-overlap** guard (recommended) or plain
   plane-coincidence? (Affects whether a coplanar-but-beside body auto-merges.)
2. `Cut`/`Intersect` into ∅ auto-targets = loud error (recommended) vs silent
   new-body. Confirm loud.
3. Multi-target **Add** consumes all targets into one merged body — confirm that
   is the desired scope semantics (vs. keeping them separate, which is
   geometrically impossible for a union that connects them).
