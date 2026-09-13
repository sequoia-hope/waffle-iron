# Yang §4.3.3 + §4.4.1 — the tangent-point MESH UPDATE (C0058 / F0058)

Status: **CLOSED for the cyl×cyl POINT-tangency class** (2026-09-13) — both
drivers CONVERTED by increment 2 (§6). Corpus drivers:
**C0058** (equal-R cylinders, coplanar axes at 30°, UNION — `s6-curved-degenerate-loop`)
and **F0058** (equal-R perpendicular cylinders, CUT — `s4-shell-euler` χ=3 on a
4-triangle edge). Both are cylinder×cylinder POINT tangency. The LINE-tangency siblings
are NOT the same class and are routed separately (ledger 2026-09-13 addendum):
`R0038`'s remedy is already banked (`YANG_N2_RECDT_ENABLE`, task #168) and
`F0060`'s mesh DOES meet its tangent generator — its worklist is a ULP weld
(six coincident vertices ≤ 3.673940e-17 apart at the generator point), a
collinear-triangle drop, and a pinch-EDGE split. `C0065` and `R0050` are the
torus arms (`specs/yang_452_local_refinement.md` §6).

This spec supersedes `specs/kv9_f1_tangency_inout_labels.md` §2c.5a, whose named
next increment — "junction-aware boundary-walk continuation" — **is not the
defect** (§4).

---

## 1. The configuration

Two cylinders of the SAME radius whose axes intersect are TANGENT at two
isolated points. For C0058 (A: r = 0.4 on +ẑ through the origin, z ∈ [0, 2];
B: r = 0.4, axis (0.5, 0, √3/2) through (0, 0, 1)) the surfaces touch at
**(0, ±0.4, 1)**; both outward normals there are ±ŷ, so the solids lie on the
SAME side and the union boundary is locally the graph
`y = r − min(d_A², d_B²)/2r` — a MANIFOLD point with **four sectors**
A, B, A, B alternating around it, one closed fan.

The exact intersection curve is two plane sections through the tangency,
`z = 1 + k₊·x` and `z = 1 + k₋·x` with `k₊ = sinβ/(1 − cosβ)` and
`k₋ = −sinβ/(1 + cosβ)` (β = the axes' half-angle); they CROSS at the tangency.
On A's lateral chart the region inside B is the band between them, which
**pinches to a single point** at the tangency. Face A therefore splits into two
faces (above the band, below it) meeting at that one vertex.

## 2. What the meshes actually do (measured, `YANG_STAR_PROBE`)

The two tessellated prisms do **not** meet at the tangency. A's seam RIDGE runs
through it (v1 = (0, −0.4, 0) and v17 = (0, −0.4, 2) are ridge samples at
θ = −90°, N = 10), and the whole ridge column survives in A's kept region while
every nearby mesh crossing sits OFF the ridge — so near z = 1 the ridge is
entirely outside B, and the mesh-level intersection detours around the tangent
point. The standoff vertices stand at 3.80450e-1 from A's axis and 3.70751e-1
from B's (both inside the exact radius 0.4), i.e. the crossing happens at
FACET depth, not at the ridge. At Stage-4 entry:

| case | verts within 1e-9 of the tangency | the four nearest |
|---|---|---|
| C0058 | **none** | 1.334403e-1 ×2, 1.868510e-1 ×2 (then 2.128254e-1 ×2) |
| F0058 | **none** | 2.921888e-2 ×2, 6.325992e-2 ×2 |

Four mesh vertices in a standoff QUAD around a tangent point that no vertex
occupies — the KV9-F1 hyperbola `x² − z² = 2r(b−a)`, standoff ≤ √(2r·B).

Stage 4 then relocates the two `vert_ell_junction` vertices of the quad ONTO
the exact tangency (the KV9-F1 increment-0c arm, gate `√(2·r·B) + B`), merges
them (§4.4.1(b)), and `split_pinch_vertices` splits the merged 2-fan star apart
again. **None of that changes the mesh's CONNECTIVITY**, and the connectivity is
what encodes "the branches do not cross".

C0058's A-lateral star at the tangency vertex, `before-validate`:

```
A: [32,33,1] [1,15,32] [32,15,36]   [32,79,17] [17,31,32] [32,31,72]
B: [33,32,94] [36,94,32]            [72,86,32] [79,32,86]
```
with v1 = (0, −0.4, 0) and v17 = (0, −0.4, 2) the seam column's rim ends.
`[32,33,1]` reaches from v33 (on the `k₊` branch, ABOVE z = 1) down to v1
(BELOW), straddling the band; `[32,79,17]` is its mirror. Those two triangles
are the entire edge-connection between A's upper and lower sheets.

F0058 shows the same thing in its perpendicular form: all FOUR of v30's
A-triangles — `[29,1,30] [30,1,31] [1,104,30] [1,30,103]`, one per quadrant —
fan onto the single lower seam vertex v1 = (0, −0.2, −0.3), so the undirected
edge (1, 30) carries **four** triangles. That is the `s4-shell-euler` χ = 3
double cover the ledger row records, and it is why
`yang_tangency_pinch_split.md` §0 was right to declare the perpendicular case
out of scope for a VERTEX-fan split: the defect is on an EDGE.

## 3. The consequence, C0058 end to end

A's kept region stays ONE edge-connected patch, so its boundary is a single
64-edge (70 after §5) cycle carrying the bottom rim arc, both branch chains and
the top rim arc. Its Newell vector cancels — `|N|/extent² = 3.565e-17` — and
Stage 6's E2 guard stops with `s6-curved-degenerate-loop`, which surfaces as
`reassembled output would be non-2-manifold`.

## 4. The boundary walk is NOT the defect (correction to KV9-F1 §2c.5a)

`patch_boundary_cycle`'s wedge-consistent successor map (#169 P3b inc-4a)
resolves the crossing correctly and does not fall back: at the OTHER tangency
(0, 0.4, 1), where the star really does present two A-fans, the orbit pairs
incoming (57,50) with outgoing (50,51) and incoming (52,50) with outgoing
(50,58) — each within its own fan, which is exactly right. The 64-cycle is the
HONEST boundary of the patch it is given. Reading it as a walk defect
(KV9-F1 §2c.5a, and the `kv9_cyl_cyl_special::steinmetz_union_exact_volume`
quarantine tag) was an inference, never a measurement.

Nor is an angular re-fan of the star a local repair, though it does compute the
right answer: projecting the merged star's 10 link vertices into the common
tangent plane and sorting by angle yields
`86, 79, 1, 15, 36, 94, 33, 17, 31, 72` — the A,B,A,B four-sector order — whose
link edge set differs from the mesh's by exactly two, `(1,33)`/`(17,79)` versus
`(79,1)`/`(33,17)`. But those two edges are each shared with a triangle OUTSIDE
the star, and the outside triangles have the same defect: v33's own A-fan is
`[32,33,1]`, `[38,1,33]` — both apexed on v1. The mis-attachment is a STRIP up
the seam, not two triangles.

## 5. Increment 1 (LANDED) — the cyl×cyl relocation move guard

`stage4_correct`'s ellipse loop had the cyl×cyl arm SKIP the "move within band"
check outright (`er.second_cyl.is_some() || move_len(proj) <= gate`), justified
as "its per-point-amplified `gate` already carries the KV9 gradient machinery".
It does not: that gate bounds the RESIDUAL ρ, while
`cyl_cyl_point_amplification` returns `None` → `f64::INFINITY` at tangency
grade, so there was no finite bound on the MOVE at all.

Measured on C0058: v33 at (0.12191459396982109, −0.36038754716096766,
1.0370674791033905) — 1.3344e-1 from the tangency — was slid **4.427e-1** by the
azimuth projection (22 % of the model's own extent) onto a position where two
other vertices already sat, **stacking three vertices on one point**. With the
check applied, the arm takes the in-plane nearest point: move **1.1506e-1**,
landing 6.78e-2 from the tangency.

The check is not new and not a band: it is the R1/R2/R3 ladder the
cylinder×plane path already runs, with an unjustified exemption removed.
Pinned by `tests_unit::s433_tangent_relocation` (4 tests, all readings
recomputed from the geometry). **Corpus: 287C / 0W / 18E / 4EE / 0T (+3 U),
ZERO category and ZERO detail moves** (release, 8 jobs, 600 s, wall 748.4 s) —
it closes a latent, it converts nothing.

New instrument: **`YANG_STAR_PROBE="x,y,z"`** — the position-keyed triangle-star
dump at six Stage-4 checkpoints (`s4-entry`, `after-reloc`, `before-3c-merge`,
`after-3c-merge`, `before-3d`, `before-validate`), with per-triangle
`(input, face)` attribution and the six nearest vertices. Position-keyed on
purpose: the collapse/compact/merge passes RENUMBER vertices between
checkpoints, so an id-keyed probe cannot follow a site across Stage 4.

## 6. Increment 2 (LANDED, ALWAYS-ON) — the tangent-point Stage-1 mint

The band between the two branches pinches to zero width at the tangent point, so
**no mesh resolution resolves it**: a triangle of A's lateral adjacent to the
seam column always straddles it, and relocation moves vertices onto curves
without CUTTING the mesh along them. Yang cuts — §4.4.1: *"we trim and update
the meshes using the intersection curves … **Then we set r_A = r_B = r**, so that
the two polylines in the meshes coincide with the intersection curve … Next,
through CDT we obtain valid discretizations of the trimmed meshes"*
(`refs/text/yang2025_hybrid_boolean.txt:552-570`), with §4.3.3's collinear-normal
test making the tangent point first class (`:518-570`).

Our pipeline runs the arrangement before the relocation, so the full pre-boolean
re-trim is deviation **N2**'s architectural closure and out of scope here. But
the half that matters for this class can be done where the pipeline still has
freedom — **Stage 1**:

> Mint the exact tangent point into BOTH operands' Stage-1 meshes, so the two
> tessellations MEET there and the exact arrangement resolves the crossing
> itself.

This is literally "we set r_A = r_B = r": one point, identical bits, in both
meshes. Both inscribed surfaces then fall away from that shared apex in the SAME
normal direction with different second-order forms (`d_A²/2r` vs `d_B²/2r`), so
their intersection near the apex is the four rays `|d_A| = |d_B|` — the four
alternating A,B,A,B sectors the exact geometry has. Nothing downstream needs a
special case: A's kept region separates into two patches by ordinary flood fill,
both boundary cycles come out simple, and the Newell guard never sees a
figure-eight.

**Implementation.** `boolean::tangency` (new):

- `cyl_cyl_tangent_points` — the closed form. A cylinder's normal is radial,
  hence ⊥ to its axis, so a shared normal direction must be
  `m = ±(û × v̂)/|û × v̂|`. Expanding a candidate in the basis `{û, v̂, m}`
  anchored at the common perpendicular's foot `f_A` kills both axial
  components, leaving `p = f_A + s_A·R_A·m` admissible **iff**
  `s_A·R_A − s_B·R_B = δ`, where `δ = (b − a)·m` is the signed axis offset.
  Equal radii with intersecting axes (`δ = 0`) give the TWO Steinmetz points;
  `δ = R_A − R_B` gives one; unequal radii with intersecting axes give none.
  Parallel axes return `None` — tangency along a GENERATOR is the F0060 line
  pinch, a different vehicle.
- `tangent_point_face_overrides` — per cylinder-face pair, with fail-closed
  gates: the canonical-TUBE vocabulary `line_edge_cylinder_face_pierce` already
  uses (hole-free, outer loop = exactly two full-circle rims, so axial
  containment is exact via the rim planes); the tangency identity within the
  **`TAU_WORK·(1+scale)` ROUNDING band** — never `TAU_MODEL`, which would fuse a
  real sub-resolution gap into a tangency (the R0053 lesson); an on-surface
  postcondition at `TAU_EVAL·(1+scale)` against both cylinders (a producer-fault
  guard on the closed form); and strict axial containment inside BOTH tubes with
  the `TAU_MODEL·(1+scale)` rim margin — a tangency AT a rim is a corner of
  higher order.
- Wiring: merged into `junction_stage1_overrides`'s `face_a`/`face_b` **and**
  `rim_a`/`rim_b` with the arm's own band dedup, so the points ride the EXISTING
  P3a #146 / P3b inc-2 channels — `rebuilt_with_all_overrides` →
  `splice_lateral_interior_points`. They are registered in
  `minted_junction_keys` like every other Stage-1 mint. A tangency is not a
  pierce (no edge crosses a face), so it has no entry in `pierce` and the P3a
  arms above cannot see it; it is minted on the SURFACES alone.
- **Both channels are load-bearing.** The face interior carries the point; the
  rim samples carry its AZIMUTH onto the tube's two rim rings so the Stage-1 grid
  has a full RULING through it, and the interior splice then lands ON that ruling
  (a conforming 2+2 edge split) instead of fanning a mid-quad Steiner point into
  three slivers. The rim sample is exact and needs no re-derivation: a tangency's
  radial direction from either axis IS the shared normal, so the sample is
  `rim centre + R·m̂`. Where the tangency azimuth already IS the seam's, the
  sample is SKIPPED — the ruling exists, and pushing a re-derived copy that
  differs from the authoritative B-Rep vertex in the last bits is refused loudly
  by the rim build. **Measured, face-channel-only:** on an operand whose seam
  phase puts the tangency mid-quad (the 30°/`cylinder_brep` fixtures, azimuth
  3.5 steps off the seam) the 3-fan produced an arrangement edge **1.35e-1 from
  one exact branch and 1.88e-1 from the other** — on neither, attributable to
  neither — and Stage 3 refused it loudly (`AmbiguousCurve { candidates: 2,
  matched: 2 }`, both matching only because `cyl_cyl_point_amplification` is
  unbounded at tangency grade, the same structural weakness §5 fixed in Stage 4).
  With the ruling, the mint is conforming on every seam phase.

**Measurement.** Corpus (release, 8 jobs, 600 s; wall 762.0 s at host load ≈ 7):
**289C / 0W / 16E / 4EE / 0T, 3 UNSUPPORTED(coplanar-boolean)** — exactly TWO
category moves, C0058 and F0058 both ERROR → SUPPORTED_CORRECT, and **ZERO
detail moves**, so no CORRECT case sees a different mesh. Solo: C0058 28.0 s,
F0058 0.6 s release. ALWAYS-ON; `YANG_433_TANGENT_INSERT=off|0` is the dev A/B
disable (the `YANG_JUNCTION_SAMPLING_ENABLE` pattern).

Both KV9-F1 steinmetz E2E oracles are **un-quarantined**. The union's is the
binding check — its analytic `V = 2·πr²h − 16r³/3` is only right if the tangency
resolved — and the mutation check confirms it: with the gate off it fails with
the original `reassembled output would be non-2-manifold`. Its subtract twin
passes gate-OFF too, so that tag was a STALE quarantine, recorded as such rather
than credited here.

**One fixture goes the other way, and it is recorded as a cost.**
`tangency_pinch_split.rs`'s two union fixtures pinned the pre-mint
representation — a pinch-VERTEX split per sheet, which was honest only while the
tessellations did NOT meet at the tangent point. The C0058-authored one now
passes a strictly stronger oracle (watertight, edge-manifold, χ = 2, and **no
coincident-position vertex group at all** — the four sectors form around ONE
manifold vertex). The 30° symmetric one (r 0.4, h 4.0, hand-built
`cylinder_brep`) is **QUARANTINED**: the four sheets that now meet at its
tangency defeat the Stage-6 boundary walk — at vertex 44 the patch presents one
wedge whose BOTH terminal boundary edges are incoming ((77,44) and (64,44)) and
another with both outgoing, so the wedge rotation emerges on an incoming edge,
`s6-wedge-walk-not-outgoing` fires and the legacy consumption fallback also
returns `NonManifoldOutput`. That is the limitation `patch_boundary_cycle`
already names in its own comment — four mutually tangent sheets degenerate
first-order dihedral sorting, awaiting a **curvature-aware radial sort** — now
REACHABLE where the un-resolved tangency kept it out of reach. Net test coverage
still rises (two kernel-v2 E2E oracles un-quarantined, one fixture parked), and
the radial sort is the named next increment.

Not asserted, deliberately: that the tangent point survives as an OUTPUT vertex.
The mint's job is to make the two Stage-1 meshes MEET so the arrangement resolves
the four sectors; once Stage 2 has done that, a downstream §4.4.1(b) sub-feature
collapse may legitimately absorb the vertex. Measured on the C0058-authored pair:
one tangency survives bit-exactly in the output B-Rep, the other is absorbed into
a vertex 7.663e-3 away.

## 7. Increment 3 — BUILT, MEASURED, **REFUTED**: the amplified band is not a bound on the MOVE, but the nearest point is not the repair

Two halves. The first is a real, measured defect and stands. The second is the
obvious repair for it; it was implemented, run against the whole corpus, and
**rejected**. Both are recorded so the next reader does not re-derive the first
and re-try the second.

### 7a. The defect (stands): `gate` is vacuous exactly where it is needed

§5 put the R1/R2/R3 move check on the cyl×cyl arm and recorded the latent as
closed. It is not. The band the check compares against is `gate = amp · budget`
with `amp = 1/sin α` — and `sin α → 0` IS the tangency. `KV11_PROBE` (extended
here to print `rho`, `gate`, `az_move` and the section's `plane_n` for every
ellipse relocation) on the 30° SYMMETRIC Steinmetz pair
(`crates/yang-rs/tests/tangency_pinch_split.rs`: r = 0.4, h = 4, axes crossing at
the ORIGIN, tangencies at (0, ±0.4, 0)) reads

| vertex | ρ | gate | azimuth move | nearest move |
|---|---|---|---|---|
| v53 | 9.575e-2 | **6.986e-1** (1.7× r) | **3.782e-1** (95 % of r) | 9.765e-2 |
| v77 | 3.385e-2 | **1.302e0** (3.3× r) | 1.317e-1 | 3.418e-2 |

A band wider than the model is not a band. Under it the azimuth closed form is
accepted while it carries the vertex from one arm of its section to the
**OPPOSITE** arm, straight across the tangent point (`e1_arm` sign flip, pinned
in `tests_unit::s433_tangent_relocation`). The amplified band bounds how far the
vertex may be *from* the curve; it licenses no slide *along* it.

The mechanism is `project_onto_ellipse_via_cylinder`, which holds ONE owner's
azimuth fixed and solves for the axial coordinate. That is the natural chart for
cylinder ∩ **plane** — the plane has no azimuth of its own — but a cylinder ×
**cylinder** section lies on BOTH cylinders equally, so pinning cylinder-1's
azimuth is an asymmetry inherited from the reused closed form, and near tangency
the section plane tilts toward the axis until that asymmetry turns a
sagitta-scale correction into a macroscopic re-parameterization.

### 7b. The repair that does NOT work: `project_onto_ellipse_nearest` unconditionally

The apparent fix is to take the move-minimizing projection on the cyl×cyl arm:
symmetric in the two owners, what §4.4.1 relocation means, and monotone by
construction (`near_move ≤ az_move` always). Implemented and measured:

- **Effective at what it claims.** v53 3.782e-1 → 9.765e-2 and v77
  1.317e-1 → 3.418e-2, both staying on their own arm.
- **Corpus-neutral.** Release, 8 jobs, 600 s, wall 745.7 s at host load ≈ 4:
  289C / 0W / 16E / 4EE / 0T + 3 U, **zero category and zero detail moves**
  against the committed `results.json` (per-id diff; the file came back
  byte-identical). All 8 `kv9_cyl_cyl_special` oracles green; C0058 28.8 s and
  F0058 0.7 s still `SUPPORTED_CORRECT`.
- **Converts nothing.** The 30° fixture fails identically — same vertex ids,
  same wedge continuations — because its wall is §8's braid, not the slide.
- **And it turns a GREEN test RED.** The rewrite tier caught
  `c0058_authored_geometry_union_mints_its_tangent_points` failing with
  `NonManifoldOutput` (`s6-wedge-walk-not-outgoing` at v9 = (0, 0.4, 1), the +y
  tangency, plus an `s4-shell-euler` double-cover edge (9, 58)); A/B-confirmed
  against `YANG_CYLCYL_AZIMUTH=1`.

**The lesson, which is the point of recording this.** *Moving less is not the
same as colliding less.* The nearest point on a section near a tangency lies
TOWARD the node, so minimizing each vertex's move pulls more of them into the
node's neighbourhood — feeding precisely the braid of junction proxies §8
describes, which is what the pipeline cannot absorb. A relocation operator
cannot be judged by its move length alone while the junction layer is missing.
Reverted; only the `KV11_PROBE` line and the pins survive, so the pipeline is
byte-identical. The owner is §4.4.1 junction re-triangulation, not the choice of
projection.

### 7c. Still open on the same line, named and measured, NOT changed

The ρ gate maps `cyl_cyl_point_amplification`'s `None` — its documented
"tangency-grade: no finite band" signal — to `f64::INFINITY`, i.e. *everything
matches*. That is the exact opposite of the contract
`surface_pair_point_amplification`'s own doc states for the same `None` ("the
caller keeps the flat band and the tangent-direction discriminator decides — the
SAFE fallback, never a silent everything-matches"). No vertex took that path in
either fixture (every gate was finite), so it is recorded rather than changed:
it is a ρ-ACCEPTANCE change and needs its own corpus run.

## 8. What this does NOT cover

- **Line tangency.** Parallel-axis cylinders and plane×cylinder generators touch
  along a whole line, and the solid is genuinely LINE-pinched: F0060's `A − B` is
  two thin cusps (`−0.3 < z < −0.3 + x²/0.6`) joined along the generator. Its
  manifold B-Rep needs the tangent EDGE duplicated per sheet — see the ledger's
  2026-09-13 addendum; measured unchanged by this increment.
- **Torus tangency.** R0050 (exact torus×torus tangency) and C0065 need the same
  idea with a torus tangent-point solver; measured unchanged.
- **R0038**, the plane-tangent-cylinder generator, whose §4.4.1 remedy is
  already banked behind `YANG_N2_RECDT_ENABLE` (task #168); measured unchanged.
- **Deviation N2 proper.** The general pre-boolean trim + CDT is untouched. This
  increment closes the ONE case where a tangency is invisible to the
  tessellations, by giving them the point they were missing.
- **The BRAID of junction proxies at a minted node** — the 30° symmetric
  fixture's remaining wall, and a REPLACEMENT for the diagnosis its 2026-09-13
  quarantine recorded. That note said the four mutually tangent sheets "defeat
  the Stage-6 boundary walk … awaiting a curvature-aware radial sort". Measured
  on today's tree, that is wrong on the decisive point: at `s4-entry` the minted
  node is **a clean 12-triangle manifold vertex whose link is ONE closed cycle of
  four alternating A,B,A,B sectors** — precisely what §6 set out to build, and a
  configuration the wedge walk already handles. Stage 4 then destroys it.

  What happens instead (`YANG_STAR_PROBE="0,-0.4,0"`, `KV11_PROBE`,
  `NONMANIFOLD_SITE_PROBE`): the two polyhedral section polylines CROSS each
  other three more times near the tangency — the polyhedral braid the exact
  geometry resolves into a single node — so besides the mint there are three
  ellipse×ellipse JUNCTION vertices (v34, v36, v49), each with one curve
  neighbour on each branch. §4.5.3's junction relocation correctly sends all
  three to `(plane₁ ∩ plane₂) ∩ cylinder` = the exact node, so `after-reloc`
  reads **four** vertices at (0, −0.4, 0) (three of them at z = 4.441e-16). The
  P3b inc-4a moved×minted weld then fuses them into the mint. But none of the
  three is ADJACENT to the mint — there is no shared edge, so the fusion is a
  positional identification with no topological path, and the union of their
  stars gives edge (43, 44) — A's generator ruling from the bottom rim to the
  node — **four** incident triangles. A 4-valent edge is not a vertex pinch: no
  wedge rotation and no `split_pinch_vertices` can undo it, and
  `s6-wedge-walk-not-outgoing` fires at vertex 44 with a wedge whose BOTH
  terminal boundary edges are incoming.

  So the owner is the §4.4.1 mesh update again, in its junction form: after
  collapsing a braid of junction proxies onto a minted node, the merged
  neighbourhood must be RE-TRIANGULATED (a local CDT constrained by the node's
  four curve arms), not merely relabelled. That is epic #169's phase-3 junction
  layer / deviation N2, not a boundary-walk sort. §7b's nearest-point relocation
  removes the two macroscopic slides that compound it (v53, v77) and leaves the
  fixture failing byte-for-byte the same — same vertex ids, same wedge
  continuations — which is how we know the braid, not the slide, is the wall.

## 9. Oracles

- **Corpus**: C0058 and F0058 ERROR → `SUPPORTED_CORRECT`; zero CORRECT lost, 0 WRONG.
- **kernel-v2 E2E**: `kv9_cyl_cyl_special::steinmetz_union_exact_volume` (exact
  bicylinder volume; RED gate-OFF) and `..._subtract_...`.
- **yang-rs unit**: `tests_unit::s433_tangent_relocation` — 6 closed-form tests
  (two-point, one-point, none, near-tangency refused beyond the rounding band,
  parallel axes) plus the 4 relocation-guard tests of §5.
- **Smoke pin**: F0058 (0.6 s release). C0058 is NOT smoke-pinned — 28.0 s
  release is past this gate's debug-ratio budget (the R0044 / F0082 rule).

## 10. Research basis

- [#24 Yang 2025] §4.3.3 (`:518-570`) — method selection; a single surviving
  point with COLLINEAR normals is a tangent point. §4.4.1 (`:552-570`) — trim,
  `r_A = r_B = r`, CDT; Fig. 11 split/merge/insert.
- [#24] §4.5.2 termination covers TRANSVERSAL intersections only; yang2023 §5.4
  certifies refinement does not converge near tangency — so mesh refinement was
  never the route here (the R0050 adjudication,
  `specs/yang_452_local_refinement.md` §6).
- `specs/kv9_f1_tangency_inout_labels.md` — the tangency band and the exact
  junction closed form (still correct; only its §2c.5a "next increment" is
  superseded).
- `specs/yang_146_conformal_junction_sampling.md`, `specs/yang_169_p3b_curved_partner_pierce.md`
  — the face-interior mint channel this increment reuses.
- `specs/yang_tangency_pinch_split.md` — the vertex-fan split; §0's exclusion of
  the perpendicular EDGE pinch is confirmed correct by §2 above.
