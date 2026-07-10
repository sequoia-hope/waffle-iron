# yang-rs — Stage-3 chord bound for ellipse-rim cylinder owners

**Status:** spec (FIP Phase 1) — root cause measured 2026-07-10.
**Change class:** bug fix (modeling-related). **Crate:** `yang-rs`
(`chord_tol_for_curved_owner`, Stage-3 selection-tolerance derivation).

## 1. Goal

A boolean whose cylinder-bearing operand carries ONLY ellipse rims (a
right circular cylinder trimmed by oblique planes in a previous boolean —
the KV14 ellipse-arc re-entry vocabulary) must derive an honest Stage-3
selection tolerance instead of the loud `AmbiguousCurve { candidates: 0,
matched: 0 }` producer fault. This is the measured root of the F-series
trio F0082 / F0083 / F0085 (5 fault sites total, all in the CYLINDER
arm; probe `[s3-ambig-probe] PRODUCER FAULT … cylinder-owning input A
has NO Circle rim; edges {"ellipse": N, "seg": M}`).

## 2. Measured mechanism (2026-07-10)

1. A prior boolean leaves a re-entering body whose single cylinder face
   is bounded by `Curve::Ellipse` edges only (oblique plane∩cylinder
   sections; kernel-v2 `EllipseArc` → yang `Ellipse`). No `Curve::Circle`
   edge exists anywhere in that operand.
2. Stage-1 tessellates those rims through the KV14 ellipse chain
   pre-pass, whose chord bound is self-contained: `d_ε = 1e-2 ·
   major_radius` per ellipse edge (spec `kv14_ellipse_arc_reentry` — an
   ellipse-bounded body "may carry no Circle edge to derive a shared
   bound from").
3. Stage-3's `chord_tol_for_curved_owner` only consults
   `curved_chord_bound` (the Circle-rim AABB × 1e-2). With zero Circle
   edges it returns `None` → the producer-fault stop — but the producer
   is NOT at fault; the tol lookup simply predates the KV14 vocabulary.

## 3. Design

Factor the KV14 literal into one shared source (A14.3 — no second copy):

- `fn ellipse_chord_bound(major_radius: f64) -> f64 { 1e-2 * major_radius }`
  — used by the KV14 ellipse chain pre-pass (replacing its inline
  `1e-2 * major_radius`, byte-identical) AND by the new Stage-3 fallback.
- `fn ellipse_rim_chord_bound(edges: &[BRepEdge]) -> Option<f64>` — the
  max of `ellipse_chord_bound(major_radius)` over the owner's
  `Curve::Ellipse` edges; `None` when the owner has no ellipse edge.
- In `chord_tol_for_curved_owner`: when `curved_chord_bound` is `None`,
  try `ellipse_rim_chord_bound`; only when BOTH are `None` does the loud
  producer fault stand.

The fallback is the bound Stage-1 ACTUALLY guaranteed for every sample
on those rims — the same derivation, not a widening. (Extending the
Circle AABB census to ellipses instead would yield `1e-2 ·
AABB-diag ≥ 2e-2 · major_radius`, LOOSER than the Stage-1 guarantee —
rejected per P9.) The cone arm (`cone_chord_tol_for_owner`) is NOT
touched: zero measured fault sites are cone-owning (demand-driven).

## 4. Parameters

None user-facing. No new tolerance literal: the `1e-2` chord factor
already exists in the KV14 pre-pass; this change de-duplicates it.

## 5. Branch table

| # | Owner rim inventory (cylinder-owning edge) | Behavior |
|---|---|---|
| T1 | ≥1 `Circle` rim | Byte-identical (`curved_chord_bound` AABB path) |
| T2 | No `Circle`, ≥1 `Ellipse` rim | NEW: tol = `1e-2 · max major_radius` |
| T3 | Neither | Loud producer fault (unchanged) |

## 6. Invariants

- I1: T1 inputs produce bit-identical tolerances to pre-change.
- I2: the T2 tolerance equals the largest Stage-1 ellipse-chain bound of
  the owner — never larger.
- I3: exactly one copy of the `1e-2` ellipse chord factor exists
  (`ellipse_chord_bound`), consumed by both Stage-1 and Stage-3.

## 7. Oracles

- E2E corpus trackers (RED today): replays of F0082 / F0083 / F0085
  assert no failure contains `AmbiguousCurve { candidates: 0, matched:
  0 }` (success or a DIFFERENT loud typed error both pass — the trio has
  known downstream walls: edge-not-2-directed sites in F0083, CDT
  re-entry in F0082/F0085 later ops).
- Unit (yang-rs): T2 — `ellipse_rim_chord_bound` returns
  `1e-2 · max(major_radius)` over mixed seg/ellipse edge lists; T3 —
  `None` on a seg-only list (mutation guard: dropping the max or picking
  minor_radius must fail).
- Regression gate: full release assay, 0 WRONG, zero SUPPORTED_CORRECT
  lost vs committed results.json; targets move ERROR→CORRECT or
  ERROR→downstream-typed (measured, not assumed).

## 8. Failure modes

- An owner with neither Circle nor Ellipse rims keeps today's loud
  producer fault (T3).
- Downstream selection with the new tol can still yield `matched != 1` —
  the existing loud AmbiguousCurve surfaces are unchanged.

## 9. Research basis

- [#24] Yang et al. 2025 §4.1.2/§4.2 (mesh chord bounds `d_ε` derived
  per surface; membership tests at the tessellation's own guarantee).
- Spec `kv14_ellipse_arc_reentry` (the Stage-1 ellipse chain bound this
  change reuses).
- Governance A14.3 (single source for the bound), P9 (bound reuse, not
  widening).

### 9a. Analytical vs approximate

Tolerance derivation only; no geometry, no SSI change. The selected
curves remain exact SSI conics.
