# M8 — Intra-solid OPPOSITE-normal coplanar step pairs: sign-aware plane canonicalization

**Status:** spec (FIP Phase 1). **Change class:** bug fix (modeling-related), M8 workstream.
**Crates:** `kernel-v2` (`canonicalize_sibling_planes`), `yang-rs` (`scan_near_coplanar` intra exclusion).

## 1. Goal

A chained boolean output that carries two coplanar faces with **opposite outward
normals** (a stepped solid: the top of a lower step and the bottom of an upper
overhang lie on one geometric plane) must be able to re-enter a further boolean.

Today every such solid walls with `CoplanarFacesUnsupported` (the intra-solid
gate) whenever the other operand's AABB touches either fragment — the dominant
intra-solid M8 residue class. Probed corpus examples (2026-07-02, `YANG_COPLANAR_PROBE`):
R0022 faces (59,311), R0025 faces (3,143), R0031 faces (3,171) — in each, the
pair's plane bits are the exact negation of one another **up to ~1e-16 rounding
drift** (per-fragment Newell normals + per-face first-vertex `d` derivation, the
same rounding-identity root as PR-KV10).

This is the opposite-orientation completion of PR-KV10, which canonicalized
same-orientation sibling fragments only ("Opposite-orientation coplanar faces
never match — sense is preserved" was correct for the *adoption* rule but left
the class unhandled).

## 2. Parameters

None user-facing. Internal constants reused unchanged: `TAU_WORK` component
band for normals, `TAU_WORK·(1+|d|)` for offsets (the exact KV10 bands — no new
tolerance, A14.3).

## 3. Branch table

| # | Input configuration | Behavior (after) |
|---|---|---|
| B1 | Two planar faces, unit normals agree component-wise within `TAU_WORK`, offsets within band (same orientation) | KV10 unchanged: adopt first face's exact `(n, d)` bits |
| B2 | Two planar faces, unit normals agree with the **negation** component-wise within `TAU_WORK`, offsets `|d + d_rep|` within band (opposite orientation) | NEW: adopt the **negated** representative bits `(-n_rep, -d_rep)` — face sense preserved, plane bits exactly negated |
| B3 | Faces on genuinely distinct parallel planes (≥ MIN_FEATURE_SIZE apart) | Unchanged: never cluster (band is ~6 orders below MIN_FEATURE_SIZE) |
| B4 | Non-planar faces, non-finite plane bits | Unchanged: skipped |
| B5 | `scan_near_coplanar` intra pair with **bit-identical** planes | Unchanged: benign, excluded |
| B6 | `scan_near_coplanar` intra pair with **exactly-negated** plane values (`n_i == -n_j && d_i == -d_j` as f64 values, so `0.0 == -0.0` matches) | NEW: benign, excluded — two orientations of ONE plane; a valid 2-manifold solid's faces on one plane are disjoint in-plane, so the arrangement needs no Stage-0 resolution |
| B7 | Intra pair near-negated but NOT exact (femto drift, e.g. hand-built B-Rep that never passed `to_yang_brep`) | Unchanged: still walls loud (`CoplanarFacesUnsupported`) — canonicalization is the producer's job; yang-rs's gate stays conservative |

## 4. Invariants

- I1 (sense preservation): canonicalization never flips a face's outward
  direction — a face's adopted normal satisfies `dot(n_before, n_after) > 0`.
- I2 (bit-exact negation): after `to_yang_brep`, any two planar faces of one
  solid whose planes matched a cluster with opposite signs carry plane bits
  `(n, d)` and exactly `(-n, -d)`.
- I3 (same-orientation path byte-identical): inputs with no opposite-normal
  near-coplanar pair canonicalize byte-for-byte as before (KV10 tests stand).
- I4 (no geometry motion): vertex coordinates untouched (KV10 property).
- I5 (loud residue): yang-rs intra gate still walls any near-but-not-exactly-
  negated (and near-but-not-bit-identical same-orientation) pair.
- I6 (regression gate): full assay `SUPPORTED_WRONG == 0`; no
  `SUPPORTED_CORRECT` case lost (30s-cap timeout flips are noise, verify by
  re-run).

## 5. Oracles

- kernel-v2 unit: construct yang face lists with (a) femto-near-negated pair →
  assert bits exactly negated after canonicalization + I1; (b) the KV10
  same-orientation fixture unchanged (I3); (c) a genuinely-distinct parallel
  pair (1e-3 apart) unclustered (B3).
- yang-rs unit: fabricated two-solid scan where solid A carries exactly-negated
  coplanar step faces overlapping B's AABB → `scan.intra == None` (B6); the
  same with 1-ulp drift on one component → `scan.intra == Some(..)` (B7).
- E2E: corpus replay of R0022 (and siblings) — the boolean must NOT fail with
  the intra-solid `CoplanarFacesUnsupported` wall; success OR a *different*
  loud typed error both count (layered blockers are expected and honest).
  Numeric gate: full assay I6.

## 6. Failure modes

- A cluster containing BOTH orientations adopts one representative; a third
  face matching either sign joins the same cluster (deterministic greedy,
  first-seen representative).
- Exactly-negated exclusion uses f64 VALUE equality of the raw plane data
  (`from_bits` compare), not bit equality, so `0.0 == -0.0` (a plane through
  the origin with a zero normal component) is correctly treated as negated.
- If a downstream stage cannot digest the newly-admitted geometry, it must
  fail loudly at its own gate (P9) — verified case-by-case in validation.

## 7. Research basis

Yang 2025 §4.5.5 [#24] concerns coplanarity BETWEEN operands; intra-solid
near-coplanarity is a floating-point artifact of chained outputs, resolved at
the producer boundary by rounding-identity canonicalization (PR-KV10 precedent,
`docs/yang_functional_roadmap.md` M8 slice d). No published algorithm applies —
this is representation hygiene, not geometry; the exact-arithmetic arrangement
(Cherchi 2020/2022 [#9]/[#38]) consumes disjoint coplanar triangles of one
input without any special handling.

### 7a. Analytical vs approximate

No surface-surface intersection involved; plane-bit canonicalization only.
Exactness: negation of an f64 is exact; adopted bits are exact copies.
