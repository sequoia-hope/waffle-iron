# Exact-membership sweep of the assay corpus — 2026-09-03

**Instrument:** `test_harness::assay::exact_membership` — every corpus
document as a closed-form point predicate (extrude / revolve of polygon,
circle and involute-gear profiles; bosses union, cuts subtract, in feature
order, with the feature engine's own semantics for direction, symmetric and
second-direction depths, through-all depth and cut auto-reversal), read on
a cubical lattice laid in the document's first sketch frame. No
tessellation and no kernel anywhere in the reading. Landed with spec
`yang_451_corner_transit.md` §3ah after the R0053 adjudication; tests in
`crates/test-harness/tests/assay_exact_membership.rs` (pinned adjudications
+ the `corpus_sweep` / `one_case_ladder` instruments); companion probe
`s453_r0053_output_obj.rs` dumps the kernel's result, a chain prefix, or a
single operand (a cut's tool as a boss) to OBJ.

**Why it matters.** Every other reference the harness has goes through a
mesh: the composition oracle compares the kernel's output against
kernel-v2 tessellations of the operands (and is SKIPPED on cut chains),
the Euler oracle reads the output mesh, the sidecar unions tessellations.
Two of this session's findings are invisible to all of them: a wrong
revolve wedge that is watertight and genus-0, and cuts that remove nothing
(the categorized runner never calls `check_volume_monotonicity`). The
exact predicate sees both.

## Coverage and scope

- Covered: 309 / 312 cases. Not covered (typed): C0084 (`BooleanCombine`),
  C0097–C0100 region extrudes (`params.regions`) — 4 features; C0100 is
  the "two adjacent sub-regions as one body" case.
- **Lattice scope.** A reading is the solid's topology when every rung
  agrees at cell sizes below the thinnest feature and the narrowest gap.
  The lattice must be laid in the model's frame: an oblique lattice
  perforates frame-aligned thin features at every cell size (R0053 read
  χ ∈ {+3, −3, −6, −8} at h = 1.04…0.37 in the world frame and 0 at every
  rung from h = 2 to 0.3 in the sketch frame). Features that TAPER to zero
  thickness (crescent pillars, knife edges) cannot be pinned at any cell
  size (R0091's pillars: genus 1 / 5 / 1 at 256 / 512 / 1024 cells);
  sub-cell gaps speckle a slab into cavities (R0063's 5.64e-6 gap reads
  χ_solid 295…−3508 until h < 2.8e-6, then 0). The VOLUME converges far
  earlier than the topology (R0091: 4.2624e-12 ± 0.01 % across all rungs).
- The corpus's near-degenerate C-series (1e-3 walls, 1e-4 floors) is below
  the sweep's 256-cell rung; their χ rows below are lattice-limited, not
  findings.

## Kernel volume against the exact volume (`EXACT_KERNEL=1`, 256 cells)

Filled from `scratchpad/exact_sweep_kernel.log` when the sweep completes
(the kernel runs every case in-process; the 20-op stacks take minutes).
Rows with |rel| below ≈ 2 % are within the lattice's own volume error on
thin-toothed profiles at 256 cells and are not findings without a finer
rung.

| case | kernel / exact − 1 | chain | status |
|---|---|---|---|
| R0045 | −0.869 | revolve(circle) boss ∖ revolve(rect) cut | **KERNEL WRONG** (class B below): boss and tool each match the kernel's own meshes in volume AND centroid; they are ≈ 1e-2 apart at 5e-3 scale (exact intersection ≈ 0), yet the kernel keeps a 7 % fragment of the boss at a third location. SUPPORTED_CORRECT in the corpus (watertight, χ = 2, volume > 0). |
| R0091 | −0.668 | revolve(circle) boss + extrude box + extrude(circle) cut | **KERNEL WRONG** (class A below): the untouched revolve body comes back as the COMPLEMENTARY 140.6° wedge of its 219.4° sweep after the cut (2.541e-12 vs 3.966e-12 by Pappus; intact after the union, flipped by the cut). SUPPORTED_CORRECT in the corpus. |
| R0096 | −0.649 | revolve(circle) boss ∖ revolve(circle) cut | **KERNEL WRONG** (class B): tool matches the kernel's (8.93e-7 vs 8.95e-7); exact removes 18 %, the kernel 71 %. |
| R0034 | +0.361 | box boss ∖ circle cut (tool contains the box) + gear revolve | **KERNEL WRONG** (class C): the cut removes NOTHING in the kernel (result after 2 ops = the intact box, 1.5233e7 exact) where the tool — a 288.6-radius, 695-deep cylinder on the base plane — contains the 209 × 235 × 309 box entirely; the authored monotonicity says decrease. |
| R0007, R0027, R0088 | (exact result EMPTY) | cut tools containing the whole body | class C: the exact chain is empty after the cut, the kernel completes with material. Each needs the same anchor as R0034. |
| R0058 | +0.063 | gear boss ∖ gear cut + gear boss | open — lattice error on gear teeth at 256 cells is a few %, so this needs a finer rung before it is a finding. |
| R0075 | +0.069 | gear boss + circle boss ∖ gear cut | open — as R0058. |
| R0009, F0042, F0002, F0010, R0020, R0031, F0012, R0039, R0094, F0005, R0078 | 0.5–1.8 % | mixed | within the lattice band at 256 cells; not findings. |

## Classes

- **A — boolean-output torus band takes the principal-branch longitude
  interval.** Anchor: yang-rs `stage1_tessellate/patch_tessellators.rs`,
  `tessellate_torus_patch`, the two-meridian-wrapping-loop ("band") case:
  each rim's longitude is inverted with `atan2` into (−π, π] and the ribbon
  is laid between the two values as they come, so a band spanning more
  than 180° is tessellated as its complement. Only the boolean-OUTPUT
  patch path is affected (the modeling lateral is structured and correct
  — the isolated revolve reads 3.960e-12 vs Pappus 3.966e-12). The side
  is determined by orientation, not span: kernel-v2's loops wind
  material-CCW about the outward normal (`validate/faces.rs`, the
  unrolled-winding rule for cylinder bands: the +1 wrap at the lower
  height), and the torus chart `(u = meridian, v = longitude)` with
  `(e1, e2, axis)` right-handed has `∂u × ∂v = −(R + r cos u)·r·n_out`, so
  a material-CCW loop appears CW in the chart and the band lies at
  DECREASING v from the +1-wrapping rim (mirrored for `reversed`). The
  fix is to unwrap `mc`'s longitude onto that side of `pc` before the seam
  bridge. Not yet fixed.
- **B — revolve ∖ revolve subtract mis-keeps / mis-drops patches when the
  operands barely or never meet.** R0045 (disjoint → 7 % kept), R0096
  (18 % overlap → 71 % removed). Not anchored: the operand meshes are
  right, so the defect is in Stage 2–5 classification/reassembly for
  these torus-segment pairs. Next: `YANG_RUN_PROBE` + a per-patch
  classification census on R0045.
- **C — a cut whose tool contains the whole body removes nothing.** R0034
  anchored at the engine+kernel level (result = the intact box); R0007,
  R0027, R0088 by the exact chain reading EMPTY. Localised past the feature
  engine: `YANG_RUN_PROBE=1` on R0034's two-op prefix shows yang-rs
  running `op=Subtract a: 8v/6f b: 2v/3f` — the box against the
  cylinder, un-reversed — and returning the box intact, where A ⊂ B has
  the empty set as its answer. So the defect is yang-rs's / the labeling
  stage's treatment of an arrangement with NO intersection curves (the
  in/out classification of A's single patch against B, Cherchi 2022 §5).
  Next: a synthetic box-inside-cylinder subtract through kernel-v2 with a
  per-patch classification census.

## Topology rows (exact-only sweep, 64 / 128 / 256 cells, sketch frame)

57 unstable ladders and 50 boundary-χ disagreements at these rungs. The
bulk are (i) disjoint-body chains where the authored `euler_target = 2`
names one shell (the exact reading is 2 × components: R0001, R0004,
R0014, R0030, R0043, R0055, R0061, R0062, R0066, R0068, R0069, R0074,
R0075, R0083, R0084, F0011–F0014, F0058, F0060, C0051, C0080, C0082,
C0094), (ii) the near-degenerate C-series below the rung (C0029–C0040,
C0111–C0113 read genus 0 where a 1e-3 wall makes the authored genus 1 —
C0029's kernel and exact VOLUMES agree at 0.8005 vs 0.8008), and (iii)
the class-C empties (R0007, R0027, R0088, C0035). Left for adjudication
with finer rungs: R0006 (exact 2 vs authored 0), C0065 (−2 vs 2), C0074
(2 vs 4), C0092 (2 vs −4), C0095 (0 vs −2), C0107 / C0108 (2 vs 4),
R0026 (an ERROR case: exact genus 1 vs authored 0).
