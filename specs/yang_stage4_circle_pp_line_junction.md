# Stage-4 circle × (plane∩plane line) junction relocation (task #146)

Amendment mirroring PR-KV11 fix 3 (`specs/…` record in
`yang_453_junction_protected_collapse.md` §1 / roadmap PR-KV11 entry) for the
CIRCLE arm. Bug-fix cycle per FIP §8. Targets the Newell-normal class
(F0064 ×2 ops, R0051, F0067, R0063): the producing boolean emits a planar
face whose loop vertex sits 1.7–4.5e-3 off the face plane, and the NEXT op's
kernel-v2 import rejects loudly
(`InvalidBooleanOutput("output face plane normal disagrees with its
outer-loop Newell normal")`).

## 1. Goal

Measured on F0064 (`YANG_T146_PROBE` + `YANG_V_PROBE`, 2026-07-12): the
off-plane loop vertices (mesh 57/62/82/83 op-3, 1198/1219 op-4) sit
BIT-EXACTLY on their side plane pre-relocation and are registered in BOTH
`vert_circle` (the section circle, e.g. A-cylinder ∩ B-cap) AND
`vert_pp_planes` (the exact plane∩plane trace, e.g. B-cap ∩ B-side). They
are TRIPLE points — the junction where the pp-line crosses the circle.
PR-KV11 fix 3 reroutes only `vert_ellipse ∩ vert_pp_planes` into a junction
closed form; the circle combination has NO analog, so the plain
`vert_circle` relocation wins and slides the vertex ALONG the circle, off
the pp-line's planes at real scale. The output face's Newell normal then
disagrees with its plane — one op downstream.

Geometry note: in the exhibited class one pp-plane IS the circle's own
cutting plane (the cap), so the pp-line lies IN the circle's plane and the
junction is the IN-PLANE line ∩ circle quadratic — NOT the PR-F3
`vert_junction` transversal plane-piercing form (which divides by `dir·n`).
PR-F3's map and arithmetic stay byte-identical; this increment adds its own
map + relocation arm.

## 2. Parameters

None (internal Stage-4 control flow). No new tolerances: gates reuse the
derived junction pattern (`2·d_ε / sin θ` with θ the crossing angle at the
junction — the `vert_circle_junction` precedent), and plane/radius residual
verification uses the pp-line's exactness (plane∩plane traces are exact) plus
the circle owner's chord band.

## 3. Branch table

Rerouting pass (runs after the KV11 ellipse×pp pass, before the PR-F3
line×circle pass; `v ∈ vert_circle ∩ vert_pp_planes`):

| # | pp entries for `v` dedup to | Action |
|---|---|---|
| 1 | exactly ONE distinct line (n1,d1,n2,d2 up to entry duplication) | remove from `vert_circle`, insert `(line, circle)` into `vert_pp_circle_junction` |
| 2 | zero lines (empty after dedup — cannot happen: membership implies ≥1) | defensive: loud `LocalRefinementRequired` |
| 3 | ≥2 distinct lines | loud `LocalRefinementRequired` (relocating onto any single junction leaves the vertex off the others — the KV11 rule) |

Relocation arm (per `vert_pp_circle_junction` entry):

| # | Configuration | Action |
|---|---|---|
| 4 | pp-line ∩ sphere(C, r) quadratic has real roots; the root nearer the CURRENT position also satisfies the circle-plane residual ≤ band | relocate onto that root (exactly on the line; on the circle within the derived band), `t`-retag via `project_onto_circle` |
| 5 | discriminant < 0 (line misses the circle) or best root's plane residual > band | loud `Stage4RegionInvalid { LocalRefinementRequired }` |
| 6 | relocation displacement ρ > `2·d_ε / sin θ` (θ = angle between the line direction and the circle tangent at the junction — the crossing amplification; sin θ = 0 → INFINITY per the circle-junction precedent) | loud `OffCurveBeyondChordBand` |

The line∩sphere form is chosen over the transversal plane-piercing form
because it is exact for BOTH configurations (in-plane and transversal): a
junction point on the circle is on the sphere `|x − C| = r` regardless of
the line's inclination; the plane residual check then certifies the circle
membership. No inclination tolerance branch exists (P9).

## 4. Invariants

- I1: inputs with `vert_circle ∩ vert_pp_planes = ∅` are byte-identical
  (the pass only moves vertices between maps; PR-F3 / KV11 / all other
  relocation arms untouched).
- I2: a relocated junction vertex lies EXACTLY on the pp-line (both plane
  residuals = 0 up to f64 evaluation) and on the circle within the circle
  owner's chord band.
- I3: the audit sites that exempt junction-handled vertices from the
  over-determined STOPs treat `vert_pp_circle_junction` exactly like the
  existing junction maps (no vertex is double-relocated or skipped).

## 5. Oracles

- Unit (helper): `pp_line_circle_junction` closed form — in-plane crossing
  (line in the circle plane, two roots, nearest-to-current picked),
  transversal crossing (root satisfies both planes), line missing the
  circle → None, tangent grazing (discriminant ≈ 0) still returns the
  touch point.
- Unit (rerouting): synthetic maps — one distinct pp-line reroutes; two
  distinct pp-lines STOP loudly (branch 3).
- Corpus (RED → GREEN): F0064 replay — no
  `plane normal disagrees with its outer-loop Newell normal` and the two
  failing auto-unions complete; class siblings R0051 / F0067 / R0063
  re-measured individually.
- Regression: full yang-rs suite, kv11 pin, `./scripts/test.sh rewrite`,
  full assay vs baseline 241C/0W/50E/4U/0T — zero-lost gate.

## 6. Failure modes

- Line misses / grazes past the circle beyond band → branch 5 loud STOP
  (never a silent pick).
- Displacement beyond the crossing-amplified band → branch 6 loud STOP.
- Multiple distinct pp-lines at one vertex → branch 3 loud STOP.

## 7. Research basis

- [#24] Yang et al. 2025 §4.4.1: intersection points are relocated onto the
  EXACT intersection curves; a point terminating two curves is their common
  junction and must satisfy both. Same basis as PR-KV11 fix 3 (the ellipse
  arm) and PR-F3 (the line×circle transversal arm).
- [#1] Patrikalakis-Maekawa-Cho: line–quadric intersection closed forms
  (degree-2 in the line parameter).

## 7a. Analytical vs. approximate method

Exact: plane∩plane line and line∩sphere quadratic are closed forms; the
result satisfies the line exactly and the circle within the derived Stage-1
band. No mesh approximation introduced.
