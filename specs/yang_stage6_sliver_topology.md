# yang-rs — Stage-6 degenerate-sliver topology (F0016/F0024 canon blocker)

**Status:** spec (FIP Phase 1) — root cause fully measured 2026-07-03;
design validated in parts by uncommitted experiments (see §6). **Change
class:** bug fix (modeling-related). **Crate:** `yang-rs` (Stage-6
attribution + `patch_boundary_cycle` + loop emission).

## 1. Goal

A chained boolean whose arrangement emits ZERO-AREA shim slivers along a
shared collinear solid-edge chain must reassemble a valid 2-manifold
output B-Rep. Today the outcome is knife-edge: with
`canonicalize_vertices_to_planes` wired (m8_shared_boundary_identity §2),
F0016/F0024 fail loudly ("reassembled output would be non-2-manifold");
unwired, the same structure survives only by femto-luck. This spec is the
named blocker of the canon re-wire decision (§8a-ii there).

## 2. Measured mechanism (2026-07-03, F0016 Extrude-3 union, canon wired)

1. Canon aligns chained-output vertices onto exact plane intersections →
   arrangement constraints inside a parent input triangle become exactly/
   near collinear → cherchi's per-triangle CDT emits ZERO-AREA children
   (measured: kept tris 76=(59,63,9), 77=(63,37,9), 2·area 3.1e-17 and
   5.6e-17, threshold 1e-12) lying along the solid-edge line through
   output vertices 59–63–37–9.
2. These shim slivers exist because the two sides of the line subdivide
   DIFFERENTLY (face 13's real triangle carries the chord edge (9,59);
   the other side's real triangles attach at 63/37). They are kept
   deliberately (mesh watertightness).
3. Attribution assigns them to face 13 — via provenance (N4 primary
   path; parent input tri 36 → tri_face → 13). NOTE: the N4 provenance
   short-circuit runs BEFORE the degenerate special-case, silently
   bypassing it — measured, though NOT causal here: the geometric
   degenerate rule ("lowest face within tolerance") picks 13 as well
   (experiment, §6).
4. In face 13's patch the sliver's directed edge 9→59 DUPLICATES real
   tri 74's 9→59 (a fold — for a zero-area triangle the inherited
   winding is sign-of-zero, combinatorially arbitrary) → the patch
   boundary in/out degrees go imbalanced (measured: v9 in=2/out=0,
   v37 out=2/in=0) → `patch_boundary_cycle` dead-ends at lib.rs:10566 →
   `NonManifoldOutput`.
5. The GLOBAL invariant that must hold at the output-B-Rep level: every
   SEGMENT of the shared line (59–63, 63–37, 37–9) is used by exactly two
   directed loop edges. Measured demand: three different loops each carry
   one piece (loop18 59→63, loop17 63→37, loop19 37→9) and face 13's loop
   carries the unsubdivided chord 9→59 — an index-level pairing failure
   regardless of where the slivers go.

## 3. Disproven fixes (P10 record — do not retry)

- **Reordering degenerate-check before provenance** (restoring the pre-N4
  special case): the geometric rule picks the same face → same fold.
- **Winding-aware greedy sliver re-homing** (place each sliver on the
  candidate face whose real triangles pair its directed edges,
  zero-conflict/max-pairing): fixes the WALK (NonManifoldOutput gone) but
  flips that face's boundary chord↔chain, orphaning a third loop's chain
  piece → `InvalidBooleanOutput("an undirected output edge is not used by
  exactly two directed edges")` in kernel-v2.
- **Adding flip freedom + provenance-first + fixpoint iteration** to the
  greedy: locally consistent, still globally wrong — local placement
  cannot see the per-segment 2-cover demand of OTHER faces' loops. The
  scattered-pieces failure persists (measured: (9,37) single-use).

## 4. Design (the mechanism to implement)

Two Stage-6-local parts; no geometry is moved, no tolerance invented:

- **A (walk robustness):** `patch_boundary_cycle` (and the patch
  edge-count preamble) EXCLUDES degenerate triangles (2·area <
  MIN_FEATURE_SIZE², the existing shared threshold) from boundary
  derivation. Slivers stay in the mesh (watertightness) and keep a face
  attribution (arbitrary, provenance is fine) — they just carry no
  boundary. Face 13's patch then walks the plain chord (9,59): no fold.
- **B (loop T-subdivision):** after boundary cycles are built, split any
  loop edge (a,b) at output vertices v that lie ON segment a–b (exact
  rational collinearity + strict betweenness, with the TAU_WORK band for
  the last-ulp case) AND are used by some other loop of the output.
  Face 13's chord 9→59 splits into 9→37→63→59; every segment then pairs
  exactly twice (loop17's 63→37 middle piece pairs against the
  subdivided chain — self-pairs within one weakly-simple loop are
  legitimate, matching the existing kernel-v2 self-pair handling).
  Deterministic: candidate vertices sorted by segment parameter.

## 5. Branch table

| # | Configuration | Behavior |
|---|---|---|
| S1 | Patch with no degenerate triangles | Byte-identical walk (A is a no-op) |
| S2 | Patch containing degenerate slivers | Slivers excluded from boundary derivation; walk on real triangles only |
| S3 | Loop edge with no on-segment foreign vertices | Unchanged (B no-op) |
| S4 | Loop edge with on-segment vertices used by other loops | Split at those vertices, parameter-ordered |
| S5 | A patch that is ALL degenerate (no real triangles) | Loud `NonManifoldOutput` (cannot bound a face) |
| S6 | Post-split loops that still fail kernel-v2 edge pairing | Loud (unchanged error) — the residue class, recorded |
| S7 | ANY patch: loop edge with undirected use-count 1 whose complementary sub-segments (a,v)+(v,b) are each walked by some loop, v within TAU_WORK of the open segment, 0<t<1 | Split at v (amendment 1 — the certainly-fatal chord repair; see §8) |

## 6. Oracles

- E2E RED: F0016 + F0024 with canon wired (temp-wire in the tracker like
  `m8_vertex_canon_campaign` does — or gate the trackers on an env knob)
  → both must build and pass the mesh oracles.
- Unit: a hand-built kept-mesh fixture reproducing §2's structure (chord
  side + chain side + two zero-area shims + a third face using a middle
  piece) → reassembly succeeds; loop edges pair 2× per segment.
- Regression gates: yang-rs + kernel-v2 suites; full assay unchanged
  (0 WRONG, no CORRECT lost) with canon UNWIRED; then the re-wire
  experiment (m8 §8a-ii) re-run — its gate ("no SUPPORTED_CORRECT lost")
  is the cycle's exit criterion.

## 7. Research basis

Yang 2025 §4.5 topology extraction assumes clean same-face regions; the
zero-area shim class is an exact-arrangement artifact (documented in-code
at the degenerate-attribution branch) whose orientation is meaningless —
excluding it from BOUNDARY derivation is not a tolerance decision but a
recognition that sign-of-zero carries no information. The loop
T-subdivision mirrors the render-side hybrid oracle's T-junction
subdivision (test-harness `subdivide_t_junctions`) at the B-Rep level.
Record as an implementation note in `docs/yang_deviations.md` (the paper
does not treat degenerate arrangement children).

### 7a. Analytical vs approximate

Index-level topology only; exact rational collinearity/betweenness for
the split test. No SSI, no surface approximation, no vertex motion.

## 8. Amendment 1 (2026-07-10): the S7 certainly-fatal chord split

**Measured residue (F0079 Extrude-8 union, probe `[s6-split-probe]`):**
face 642's loop walks a spur-and-chord `1294→1293, 1293→1295` while
face 645 walks `1294→1295`; vertex 1294 sits on chord (1293,1295) at
t = 0.374 with f64 perpendicular distance EXACTLY 0.0 — but the exact
rational `on_open_segment_param` test says NOT collinear (sub-ULP off
the segment), and the patch has no fold sliver, so BOTH the §4B
eligibility gate and the exactness test miss it. The output chord is
then used by exactly ONE loop → kernel-v2 edge pairing is CERTAIN to
reject (`an undirected output edge is not used by exactly two directed
edges`). Same class: sites in F0083/F0084 (KV15 residue census).

**Design (S7):** a second, independent split arm in
`subdivide_loops_at_shared_vertices`, applied to EVERY patch (no
fold-sliver requirement), with a criterion that can ONLY fire where the
un-split output is provably invalid:

- undirected use census over all loops: `use(a,b) == 1` (the chord is
  walked by exactly one loop — unpaired, certain rejection), AND
- `use(a,v) ≥ 1` and `use(v,b) ≥ 1` (the complementary chain exists on
  the other side), AND
- `v` used by a loop other than the chord's, strictly interior
  (0 < t < 1), within `TAU_WORK` of the open segment (the spec §4 "band
  for the last-ulp case", now implemented; F0079's site is dist 0.0).

Because any currently-VALID output has every undirected segment at
use-count exactly 2, S7 cannot alter a passing case — reference parity
for benign T-junctions is preserved structurally, not by the fold-sliver
scope. The fold-sliver arm (S4, exact test) is byte-identical unchanged.
Deeper chains (a→x→v→b with `use(a,v) = 0`) remain the S6 residue.

**Amendment 1a (same day, measured):** the S7 split alone moved F0079 to
`InvalidBooleanOutput("output component's Euler characteristic is not
genus-representable")` — the chord's owner loop carried a pre-existing
SPUR (`1294→1293` followed by the split-created `1293→1294`): pairing is
now valid (a legitimate self-pair) but the slit contributes E+1 with no
face, making χ odd. A null excursion `a→b, b→a` at adjacent loop
positions is not a boundary feature; S7 therefore finishes by CANCELLING
adjacent inverse pairs in which at least one member is a split-inserted
segment (wrap-around included, iterated to a fixed point). Restricting
cancellation to split-inserted members keeps every non-S7 loop
byte-identical and cannot touch legitimate bigons/self-pairs elsewhere.
