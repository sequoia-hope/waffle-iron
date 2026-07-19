# SPEC — #169 P3b (F0082): curved-partner pierce mint — the ellipse×wall corner

Status: **DESIGN + increment 0 (probe) MEASURED**. Grounded in the #181/inc-3c
characterization (`specs/yang_146_conformal_junction_sampling.md` §"Blocker (2)
CHARACTERIZED", memory `session_2026_07_19_146_inc3c_f0082_corner_class`) and
this spec's own increment-0 probe run (§2). Companion to — NOT a replacement
for — the #137 grazing-corner spec (`yang_137_torus_plane_grazing_corner.md`):
this spec covers the TRANSVERSAL corner class reachable by the Stage-1 mint
mechanism; #137 keeps the tangential/grazing class that needs refinement plus
the output-level stitch.

## 0. Scope

- **In scope:** a `LineSegment` boundary edge of one operand (incident to two
  PLANAR faces — the existing P3a owner-edge channel scope) transversally
  piercing a bounded CYLINDER lateral face of the other operand. Corpus
  driver: **F0082 Extrude-11** (`TessellationFailed "ring rejected by CDT"`,
  kernel-v2 FaceId 3716 / yang output face 362).
- **Out of scope (fail-closed skips, later increments):** cone/sphere/torus
  partner faces; curved-incident OWNER edges; holed/partial cylinder laterals
  as pierce targets (canonical full tubes only first); the tangential/grazing
  corner class (#137 part (b) — the transversality floor routes it there).

## 1. Contract grounding

The junction research findings (`docs/yang_junction_research_findings.md`)
bind all P3 specs:

- **Q4 corollary:** prevention lives at the SAMPLING/MINT layer — the
  arrangement is exonerated (it faithfully preserves whatever the Stage-1
  meshes carry, N48 sidecar-certified). A never-minted junction is a Stage-1
  sampling defect.
- **Q1 (corner taxonomy):** a boundary point where an intersection curve
  exits a face boundary invokes the junction path: mint the corner ONCE,
  insert into both operands as one shared arrangement vertex.
- **The junction contract:** mint once exactly; share by identity (same
  bits, both meshes); multiplicity below resolution is a loud STOP, never a
  fuzzy merge; refinement is not a lever here (the pierce is transversal).

Yang 2025 anchor: §4.4.1's `r_A = r_B = r` shared-junction precondition
(`refs/text/yang2025_hybrid_boolean.txt:551-554`) — the same clause P3a
implements for mid-edge pierces; this spec extends it to the corner class
where the pierced partner surface is curved.

## 2. The case, precisely (F0082 Extrude-11 — MEASURED, increment 0)

From inc-3c (#181): the union output face 362's cyl∩plane section
`Curve::Ellipse` arc (r≈0.2124) terminates at output vert 913 — a relocated
cylinder chord-ring crossing vertex, ON the ellipse to 4.4e-16 but at the
canonical parameter t≈π/2 — instead of the true terminus, the ellipse ×
wall-plane corner at t=1.5578, exact point
`(-0.06399183, -0.10911126, 2.10955341)`, 2.76e-3 away along-curve. The arc
overshoots the wall segment (x≈-0.063992) by 1.29e-3 in-face; the ring
self-intersects; the #173 render gate STOPs loudly. `YANG_INPUT_VERT_PROBE`
zero-hits: the defect is minted by this union, not inherited.

**Increment-0 probe (`YANG_P3B_PIERCE_PROBE`, this spec, read-only —
measured 2026-07-19 on the live F0082 chain):** the corner IS an enumerable
edge×face pierce:

```
[p3b-pierce] A edge 2424 (owner_planar=true) × cyl face 2 (r=0.212325):
    t=0.232061 J=(-0.063991829, 0.092341791, 2.113152675) transv=0.474
    t=0.767345 J=(-0.063991829,-0.109111255, 2.109553406) transv=0.474
```

- The t=0.767 root **matches the inc-3c true corner to 9 decimals**.
- The owner edge (operand A's wall edge 2424) is `owner_planar=true` —
  already inside the P3a owner-edge channel scope. Only the PARTNER side
  (operand B's cylinder face 2) is out of scope today, at the two documented
  gates: `junction.rs` "planar partners only" (`line_edge_plane_face_pierce`
  early return) and the ALL-LINE partner-loop restriction.
- Transversality 0.474 — well-conditioned, nowhere near the 1e-9 tangency
  floor. This is NOT a grazing corner; no refinement is needed. The #137
  Urick-stitch machinery is the wrong (heavier) tool for this class.
- The t=0.232 root is the arc's other-side wall crossing (the v915/near-dup
  8.5e-4 region of the same rejected ring) — the same mint fixes both ends.

**Why P3a's proven mechanism transfers:** once J carries identical exact bits
as a vertex in BOTH Stage-1 meshes, the arrangement dedups them into one
shared vertex and the intersection polyline threads it — the identical
mechanism proven at F0082's v588/v601 site (inc-2 measurement). J lies on
face-362's plane AND on the cylinder ⇒ J is ON the section ellipse exactly
(Stage-4 relocation residual ~0), and on the wall plane ⇒ the output ring's
arc/wall chains meet AT J.

## 3. Design

### 3.1 Pierce primitive (line × cylinder)

`line_edge_cylinder_face_pierce(p0, p1, s1, s2, f_idx, f, y) -> Vec<PiercePoint>`
mirroring `line_edge_plane_face_pierce` gate-for-gate:

- Roots of the quadratic `|w(t)|² = r²` (w = radial component of p(t) − axis)
  in `(0,1)` — up to TWO genuine pierces per edge×face (unlike the plane
  case; both are minted, subject to the gates below).
- Transversality `|t̂ · n̂(J)|` with the radial outward normal at J; same
  `TRANSVERSALITY_MIN = 1e-9` floor → tangential contacts route to #137,
  never minted (fail closed).
- Endpoint margin `TAU_MODEL·(1+scale)` — a pierce at an owner-edge endpoint
  is a higher-order corner (vertex-on-surface), P3b-later territory.
- On-surface postcondition `TAU_EVAL·(1+scale)` for the owner's two incident
  planes at J (producer-fault guard, identical to the planar arm).
- The `junction_stage1_overrides` sub-weld cluster filter applies unchanged
  (the two F0082 roots are ~0.2 apart — far above any band).

### 3.2 Containment on the bounded cylinder face (canonical tubes first)

The planar arm's 2D chord-polygon containment does not transfer. For a
**canonical full-tube lateral** (the `tessellate_lateral_face` hole-free
"2 FULL-circle rims" arm — F0082's face-2 shape): azimuth is always
contained; containment is the axial interval `v_J ∈ (v_rim0, v_rim1)` with
the same `TAU_MODEL·(1+scale)` boundary margin (a pierce within the margin
of a rim plane is a rim-corner — P3b-later, fail closed). Exact: the rim
planes are analytic. Partial-arc strips and holed laterals: fail-closed
skip this increment (unroll-space containment is a later widening).

### 3.3 Partner-side insertion into the cylinder Stage-1 mesh

The planar face channel (`cdt_polygon_with_holes_keep_interior` Steiner
mint) does not apply to the structured tube grid. Insertion for canonical
tubes: locate the containing grid triangle in the (θ, v) unroll of the tube
tessellation and split it into a 3-fan around J — J's EXACT bits become the
new mesh vertex (source `BRepFace{face, u, v}`), grid untouched elsewhere.
Fail-closed non-degeneracy gates, mirroring the planar margins:

- J within the weld band `TAU_MODEL·(1+scale)` of an existing mesh vertex →
  skip the mint on BOTH sides (multiplicity guard; status quo, never worse);
- J within the band of a grid EDGE → split the edge's two incident triangles
  (2+2 fan) instead of a degenerate 3-fan — or, first increment, skip
  fail-closed and measure whether F0082 needs it.

The owner side needs NO new machinery: the existing
`rebuilt_with_junction_overrides` edge-polyline splice carries J into both
copies of the owner edge (per-loop fan-out, proven by the P3a fixtures).

### 3.4 Non-goals

- No tolerance merges, no band widening — every gate above is the existing
  derived margin vocabulary (R0091 discipline).
- No output-level ring surgery: if the mint does not resolve the ring, the
  #173 gate keeps STOPping loudly (expected: chained models carry layered
  defects; the inc-2/3a/3b history says expose-the-next-layer is normal).
- The relocated chord-crossing vertex (v913-class, t≈π/2 beyond J) is NOT
  deleted by this spec: with J present it lands on the discarded side of
  the wall; if a residual sliver survives reassembly, that is a loud STOP
  naming the next increment — never a silent trim.

## 4. Oracles & verification

- Unit: pierce primitive pins F0082's two J's (9-decimal fixture from §2);
  containment red/green at the rim margins; insertion fixture proves both
  rebuilt operands carry J bit-exactly as closed 2-manifolds (the
  `p3a_junction_wiring.rs` end-to-end contract, cylinder edition).
- Gate-OFF full assay byte-identical (increments 1–2).
- Gate-ON: 0-WRONG ratchet; F0082 Extrude-11 ring-reject cleared or the
  next defect layer exposed LOUDLY; zero regressions; Stage-0 seam suite +
  Cherchi sidecar parity (arrangement input changes — same ledger as P3a
  inc-3).
- Always-on flip only on the standard ledger (the P3a inc-3 precedent).

## 5. Increments

- **inc-0 — DONE (this session): probe.** `YANG_P3B_PIERCE_PROBE` banked in
  `junction_pierce_points` (read-only): enumerates line×cylinder pierce
  candidates. Measured on F0082: the corner enumerated exactly (§2); ~250
  candidate lines across the whole 11-op chain (scope is modest).
- **inc-1 — pierce primitive + tube containment.** Production-shaped
  `line_edge_cylinder_face_pierce` behind the probe (unwired), unit tests
  pinning §2's values. Byte-identical production.
- **inc-2 — tube-grid insertion channel.** The 3-fan split with exact-bits
  J + fail-closed gates, reachable only via a new env gate
  (`YANG_P3B_PIERCE_ENABLE`); gate-OFF byte-identical (assay-verified).
- **inc-3 — wire + measure.** Cylinder partners join `junction_pierce_points`
  scope under the gate; gate-ON F0082 measurement + full ledger.
- **inc-4 — always-on** per the standard ledger; then scope widenings
  (strip/holed laterals, cone partners, curved-incident owners, rim-corner
  and edge-split arms) as separate measured increments.

## 6. Risks & guardrails (P9/P10)

- **Layered defects:** F0082 is a chained multi-defect model (inc-2
  history). The mint may expose an over-use/next-layer failure rather than
  green the case — that outcome is a CORRECT result of this spec (loud,
  named, next increment), not a refutation of it.
- **The t-ordering nuance (§3.4):** J (t=1.5578) sits INSIDE the arc's
  kept range; the chord-crossing relocation target (t≈1.5708) is beyond
  the wall. If the labeling does not discard the beyond-wall sliver
  cleanly, the §4.3 dedup / I6 wedge machinery may absorb it — measure,
  never special-case.
- **Grid degeneration near rims/seams:** the fail-closed margins skip the
  mint rather than fan a sliver (missed mint = status quo). Any STOP the
  skip leaves standing is the case's pre-existing state.
- **No global re-tessellation:** the 3-fan is local; rings, rims, and seam
  rulings stay byte-identical outside the containing triangle (the N54
  lesson: never move existing coordinates).
