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

- **Line tangency.** ~~Parallel-axis cylinders and plane×cylinder generators touch
  along a whole line, and the solid is genuinely LINE-pinched: F0060's `A − B` is
  two thin cusps (`−0.3 < z < −0.3 + x²/0.6`) joined along the generator. Its
  manifold B-Rep needs the tangent EDGE duplicated per sheet — see the ledger's
  2026-09-13 addendum; measured unchanged by this increment.~~ **Parallel-axis
  cylinder×cylinder: LANDED 2026-09-17 as the generator arm (§11, C0056).**
  Plane×cylinder generators (R0038) remain out.
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

## 11. Increment 4 (LANDED 2026-09-17, ALWAYS-ON) — the GENERATOR arm: parallel axes tangent along a line (C0056)

**Configuration.** C0056: A = cylinder `r 1`, `z ∈ [0, 1]`; B = cylinder
`r 0.5` on the axis through `(0.5, 0, 1.4)` pointing down, `z ∈ [0.2, 1.4]`,
CUT. Axes parallel, offset `0.5 = R_A − R_B` exactly: the hole wall is
internally tangent to the outer wall along the generator `x = 1, y = 0,
z ∈ [0.2, 1]`. The output is a blind hole whose wall thins to ZERO along that
line — a line-pinched solid (the F0060 class), with the top face a crescent
whose cusp is that line's top.

**The wall, measured.** Stage 3 `AmbiguousCurve { candidates: 1, matched: 0 }`
on edge (37,70): the single candidate IS the exact generator
`Line{(1,0,0), ẑ}`, and the arrangement's intersection chords sit at
`(0.95990, 0.14964, z)` — a vertical chord 8.9° off the tangent azimuth, `4.9e-2`
from the line. `YANG_STAGE0_DUMP_DIR` on the two Stage-1 prisms: A is a 13-gon
whose seam sits at −90° and whose rulings fall at `−6.92° + k·27.69°` — NO
ruling at the tangent azimuth 0; B is a 12-gon with a ruling at 0 whose two rim
samples DIFFER IN THE LAST BITS (`(1.0, 0.0, 1.4)` vs
`(1.0, 1.2246e-16, 0.2)` — a `sin π` residue of the Stage-1 uniform-slot
evaluation in the far rim's own frame). B's ruling therefore stands at the full
radius where A's facet stands one sagitta inside, B pokes OUT of A along the
tangent line, and the mesh-level intersection is a chord pair 4.9e-2 off the
curve both operands are actually tangent along. The point form's finding (§2)
in line form.

**The mint (`boolean::tangency::cyl_cyl_tangent_generator` +
`mint_generator`).** With `û ∥ v̂` the shared normal is the unit perpendicular
from A's axis to B's, `m = w⊥/|w⊥|`, `δ = |w⊥|`, and the point form's identity
`s_A·R_A − s_B·R_B = δ` selects the contact: `(+,−)` external at
`δ = R_A + R_B`, `(+,+)` internal (B inside A) at `δ = R_A − R_B`, `(−,−)` (A
inside B) at `δ = R_B − R_A`. The line is `p₀ + t·û`, `p₀ = a + s_A·R_A·m`.
Rim samples ONLY — a line needs no face-interior point: each of the four rims
gets `p₀ + h·û` at its own axial height `h = (centre − p₀)·û`, so all four
samples share `p₀`'s bits in the two non-axial coordinates. Gates, fail-closed:
canonical tubes; exact tangency within the ROUNDING band; **the axis is exactly
a coordinate axis** (the only frame in which four rounded samples are exactly
collinear — an oblique axis would hand the exact arrangement two skew
femto-segments, so it declines, status quo); the two tubes' axial spans overlap
by more than the rim margin (spans that merely touch are a rim×rim circle
tangency, the rim-junction vehicle); on-surface postcondition of `p₀` against
both cylinders.

**Why B needs no producer fix.** B already carries the azimuth as uniform
Steiner slot k = 3 on both rims; the mint's sample lands angularly within
`merge_tol` of that slot and the rim build's task-#143 merge policy makes the
slot TAKE THE OVERRIDE'S BITS — so B's ruling becomes exactly the line without
touching the seam vertex (which stays authoritative and is skipped as before).
A's rims take the sample as a new slot and its lateral routes to azimuth-merge,
exactly as the point form's rim channel does. After the mint the arrangement
sees ONE shared collinear segment `z ∈ [0.2, 1]` (A's ruling split at B's rim
vertex, B's at A's), B's facets fall strictly inside A's (B's step 30° > A's
27.7°, the chord-depth ordering of §11c), Stage 3 matches the generator, and the
pipeline COMPLETES: yang emits a 5-face B-Rep with the honest Mäntylä
duplication — the cusp as TWO vertices (v34, v70 at `(1,0,1)`), the line as two
twin pairs (e29/e71, e30/e58), the outer wall carrying the line as a SPUR of
its top-rim loop (`70 → 57 → 34`, v57 = `(1,0,0.2)` the line's bottom) and the
hole wall carrying it as a SEAM (top circle, down, bottom circle, up). Every
vertex has one fan; kernel-v2's `validate_solid` accepts it.

**Three consumers had never seen a spur, and each was one layer of the same
fact:**

1. **kernel-v2 tessellation — M3d slit** (`tessellate/mod.rs::pinch_split_rec`,
   spec `kv2_cdt_triangulation_core` §6d). The pinch split read
   `70 → 57 → 34` as a two-vertex sub-ring ("pinch sub-ring has fewer than 3
   vertices"). A two-vertex sub-ring IS a slit: peel it (keep the twin copy in
   the ring, remember `[anchor, tip]`), recurse, and hand the slits to the CDT
   as INTERIOR CONSTRAINT edges — new
   `cherchi_rs::cdt_polygon_with_holes_floodfill_constrained` (the welding
   flood-fill variant plus constraints; empty constraints ⇒ byte-identical).
   Then `pass 1.5` of the developable tessellator treated the hole wall's seam
   copies (same position a whole window apart, in ONE loop) as a cross-loop
   pinch to canonicalize; a same-chain match at `k ≠ 0` is a seam duplicate and
   is now skipped.
2. **kernel-v2 self-intersection gate — the spur facet fraction**
   (`developable.rs::SPUR_FACET_FRACTION = 0.5`). Both faces tessellated, the
   render gate found four ~1e-4 penetrations between the outer wall and the hole
   wall next to the line — REAL crossings of the render, none of the B-Rep. A
   chord leaving the shared tangent line at angular step φ lies `s·φ/2` below
   the common tangent plane at tangent distance `s`, on any radius, while the
   surfaces separate only quadratically; the per-face RELATIVE sagitta gives
   both cylinders the same step (`sqrt(8·1e-3)` = 5.1°), so the two first
   chords coincide to second order and cross on the higher-order terms
   (`r = R/2` is the exact midpoint-circle configuration: A's chord midpoints
   from the tangent point lie ON B's circle). Halving the facet width for
   triangles with a corner on a spur node puts the outer wall's first chord
   strictly above the hole wall's; the hole wall carries the line as a seam,
   keeps its step, and the rule is asymmetric by construction. Not a band: a
   crossing that survives still trips the loud gate. Measured limit, named: the
   second chord's sag `R φ²/32` against the gap `R θ² (R−r)/(2r)` bounds the
   rule to roughly `r < 0.9 R`; thinner walls stay a loud STOP.
3. **assay χ oracle — per-FAN vertex counting** (`test-harness::oracle::
   shell_decomposition`). The render then graded `V 549 − E 1643 + F 1095 = 1`
   against 2: the position weld reads the cusp once, and the 2026-09-13 rule
   credited fused copies only across DISTINCT shells ("a self-pinch inside one
   shell still reads one χ short, as it must"). It must not: the same solid
   represented as one sphere folded to touch itself at a point has one B-Rep
   vertex per FAN there, exactly as F0060's separate lobes do, and kernel-v2's
   own validator accepts precisely that form. `pinch_extra` is now Σ over
   welded vertices of (link components − 1), computed from the per-edge-slot
   keys on both the exact and the hybrid path; identical to the shell count
   wherever every shell meets a vertex in one fan. Unit test
   `euler_characteristic_in_shell_vertex_pinch_counts_the_vertex_per_fan`
   (two tetrahedra sharing the origin, joined by a tube: one sphere through
   the origin twice; weld χ 1, per-fan χ 2). The watertight oracle needed
   nothing: the spur refinement subdivides the outer wall's copy of the line
   while the hole wall's copy stays whole, so the exact keys never coincide
   4-valent (the 2026-09-13 "named, not fixed" case did not arise here; it
   remains named).

**Authored-invalid expectation, corrected.** C0056's `expected_volume` was
`π − π·0.25·1.0` (a full-height hole); the cut spans `z ∈ [0.2, 1.4]` and only
0.8 of it lies inside the boss: `π − π·0.25·0.8 = 0.8π = 2.5133`. The kernel
measured 2.5104 (inscribed render, −0.1 %). The first correct output graded
`SUPPORTED_WRONG` on the old number by 6.7 % against a 5 % tolerance; the
generator knob and the meta are corrected in the same increment (the R0004
precedent).

**Oracles.** yang-rs `tests_unit::s433_tangent_relocation` +6 generator tests
(C0056's foot, external, A-inside-B far side, axial-offset invariance, near-
tangency refused, coaxial/crossing declined); kernel-v2
`m3d_slit_ring_tessellates_with_the_spur_as_a_constrained_edge` and the former
guard rewritten as `m3d_two_vertex_subring_is_a_slit_and_emits_no_degenerate_triangle`;
cherchi-rs `floodfill_constrained_keeps_the_interior_and_the_constraint_edge`;
oracle per-fan test above; smoke pin C0056 (0.4 s release). Corpus: Canonical corpus after the flip run (release, 8 jobs, 600 s; wall 738.1 s; F0085 321.6 s, R0044 293.3 s, F0065 110.9 s): **291C / 0W / 14E / 4EE / 0T + 3 UNSUPPORTED(coplanar-boolean)** — per-id diff of the committed `results.json`: exactly ONE category move (C0056 ERROR → SUPPORTED_CORRECT), ZERO detail moves.

**Not covered (still).** ~~C0043 — the same tangency with COPLANAR caps — takes
the Stage-0 path (`stage0: true`), where the P3a/tangency mint is not wired;
it stays at its Stage-3 wall, M8 territory.~~ **§12 (2026-09-21).** A slit whose BOTH ends are
interior to the face (a slot fully inside the outer wall's span) would arrive
as a zero-area INNER loop, which the M3d peel does not see. R0038
(plane-tangent-cylinder generator), torus tangency (R0050, C0065), and oblique
axes (the exact-collinearity gate) are unchanged.

## 12. Increment 5 (LANDED 2026-09-21, ALWAYS-ON) — the generator arm on the STAGE-0 path: coplanar caps (C0043)

**Configuration.** C0043: A = cylinder `r 1`, `z ∈ [0, 1]`; B = cylinder
`r 0.4` on the axis through `(0.6, 0, 0)`, `z ∈ [0, 1]`, UNION (the union
IS A by design — B lies inside A, touching its wall along the generator
`x = 1, y = 0`). The same internal tangency as §11 with the two cap pairs
COPLANAR, so `stage0_preprocess` is active and every Stage-1 mint in
`boolean()` (rim junction, P3a, the §11 tangency arm) was gated off by
`stage0.is_none()`. Reproduced in 0.1 s: Stage 3 `AmbiguousCurve {1, 0}` on
edge (23, 93), the §11 signature (chords 4.5e-2 off the generator).

**Why the naive wiring is one wall short (measured).** Pushing the §11 rim
samples into Stage 0's `RimSplitMap` after the pair loop moves the wall to the
I6 `NonManifoldInput` backstop: `NONMANIFOLD_SITE_PROBE` names one coincident
cap triangle `[(0.9464, 0.2, 0), (1, 0, 0), (0.6, 0, 0)]` kept from BOTH
inputs with single labels (`source [(A, 9)]` / `[(B, 8)]`). Both cap pairs
had been classified `DiscPair::Empty` (a lens: neither ring strictly contains
the other, they overlap) and left to the arrangement with their own fans;
with the ruling minted, B's fan edge from its centre to the tangent point
lies ON A's radial fan edge along the x-axis, the arrangement splits A's
triangle at B's centre, and the two caps agree on exactly that one
sub-triangle. cherchi's pocket dedup merges IDENTICAL INPUT triangles into a
multi-label sheet; a coincidence minted by the split is two single-label
copies, and the §4.5.5 membrane rule (which resolves multi-label sheets
only) never sees it. The paper's rule is the fix, not another dedup: the
overlap must be meshed identically BEFORE the arrangement.

**Two pieces.**

1. **STANDING rim samples (`BRep::standing_rim`, the `forced_rim_n`
   precedent for points).** The generator mint now runs FIRST in
   `boolean()` — before Stage 0 — through
   `tangent_generator_rim_overrides` (the §11 arm alone; the point arm's
   face-interior half has no Stage-0 carrier) and rebuilds both operands
   with `rebuilt_with_rim_overrides`. The map it inserted is stored on the
   rebuilt B-Rep and honored by every later re-tessellation from topology:
   Stage 0's ring readers (`disc_rim_ring`, the annular and mixed readers,
   the coincident-cylinder build, `build_stage0_mesh` — all through
   `stage1_tessellate_with_rim_overrides`), the §4.5.2 re-derivation, the
   phantom-guard boost, the spike normalization, and the two `rebuilt_*`
   entries, which COMPOSE new overrides over the standing ones bit-deduped
   (`merge_rim_points`). So the P3a sampler's re-mint of the same generator
   on the idle-Stage-0 route (C0056) is a no-op instead of a refused
   duplicate slot, and the "overrides do not compose across rebuilds" trap
   (`boolean.rs` P3a scope gate) is closed for rim samples. Mint keys are
   registered in `minted_junction_keys` like the rim-junction path's.
2. **Touching containment in the disc∩disc builder
   (`stage0::disc_pair::touching_containment` + `crescent_tris`).** With the
   standing sample on both rims, each cap ring carries the tangent point
   with identical bits (A: a 14th slot; B: its uniform slot k = 3 takes the
   override's bits, #143). The classification now recognizes "every inner
   vertex strictly inside the outer ring except exactly ONE bit-identical
   shared vertex" and emits the inner disc's fan to BOTH caps (the shared
   overlap) plus the CRESCENT to the outer cap. The crescent is weakly
   simple (the cusp T twice on its boundary); it is triangulated as the
   simple polygon `[T, o₁ … o_{n−1}, i_{m−1} … i₁]` (outer CCW from T, inner
   CW back, bridged between the two vertices adjacent to T) ear-clipped,
   plus the TIP triangle `[T, o_{n−1}, i_{m−1}]` the bridge cut off —
   under the annulus builder's exact coverage certificate (Σ area =
   area(outer) − area(inner), rational shoelace; any other outcome is the
   loud `disc-crescent-tri` residue). Two shared vertices, or a shared
   vertex with a non-interior neighbour, is not this class.

**What the pipeline then does.** Stage 0 emits 24 bit-identical cap
triangles on B (12 per cap) that all exist in A's mesh; cherchi dedups them
into `{A, B}` sheets; the union keeps A's copy (`!opposite`); B's lateral
falls inside A's along the shared ruling; the output is A's three surfaces,
closed 2-manifold, volume = A's own 14-gon prism (`s433_internally_tangent_
cylinders_with_coplanar_caps_union_is_a`), and on the real kernel the exact
B-Rep volume is π to 1e-12 with χ = 2 (`cyl_cyl_tangent_union_kv2`, both the
mid-quad C0043 placement and the on-seam placement where the mint's A-side
samples are skipped because the ruling already exists).

**Scope: INTERNAL contact only (measured on the first corpus run).** With
the boost admitting external contact too, C0042 — two equal cylinders
touching from OUTSIDE along a line, coplanar caps, union — regressed
CORRECT → ERROR: the two-lobe union pinched along the contact line is an
output only the pinch-edge family can emit (Stage 5 handed kernel-v2 one
shell whose contact line is a 4-valent edge, `InvalidBooleanOutput`),
whereas without the ruling the two tessellations never meet and the
regularized two-lobe union is CORRECT. Stage 0 has an emission for internal
contact only (the touching containment), so `mint_generator` declines
external contact under `internal_only` on the Stage-0 entry; the
idle-Stage-0 route keeps §11's full arm (`cyl_cyl_tangent_generator_contact`
now reports the kind; pin `s433_stage0_entry_declines_external_contact`).

**Not this increment (measured, quarantined).** The same pair as a
FULL-HEIGHT CUT (`internally_tangent_full_height_cut_leaves_a_crescent_prism`,
`#[ignore]`): yang completes with the cusp duplicated per cap, but Stage 5
hands kernel-v2 both walls as CLOSED tubes (outer loop = one rim, inner loop
= the other) and the outer wall's bottom rim chain is 4 coarse arcs
(`1→10→7→4`) against the cap's 14 — `InvalidBooleanOutput("an undirected
output edge is not used by exactly two directed edges")`, loud, one stage
later than before (Stage 3). A pinch that runs cap to cap is the pinch-edge
family's (F0060 / §11's `split_pinch_vertices`), not §12's.

**Oracles.** yang-rs `tests_unit::s433_generator_stage0` (touching
containment finds the one shared vertex and rejects strict containment and
crossing; crescent covered exactly — 24 triangles, CCW, no degenerate;
standing samples survive `retessellated_at_current_d_eps` /
`rebuilt_with_all_overrides` / a re-mint; Stage 0 emits the touching-disc
overlap identically; end-to-end union == A); test-harness
`cyl_cyl_tangent_union_kv2` (two union placements pass, the cut
quarantined). Full yang-rs suite 983 + integration binaries green; clippy
`--all-targets` clean on yang-rs and test-harness. Corpus (release, 8 jobs,
600 s; wall 802.6 s): **293C / 0W / 13E / 4EE / 0T + 2
UNSUPPORTED(coplanar-boolean)** — per-id diff of the committed
`results.json`: exactly ONE category move (C0043 ERROR →
SUPPORTED_CORRECT), ZERO detail moves.

## 13. Increment 6 (2026-09-21, ALWAYS-ON) — the CROSSING arm: parallel axes whose circles cross at a grazing angle (R0038's class, checkpoint 1)

**R0038 re-diagnosed.** The ledger and `yang_n2_stage4_cdt_mesh_updating.md`
§5c.10 recorded R0038 as "a plane tangent to a cylinder along a single
generator". Read off the document and the Stage-4 STOP probe's own
positions, it is not: the collapsed six-vertex chain
(`pa = (−2.5584, −5.8076, 13.2564)` … along `(0.4034, 0.9150, 0)`) stands
exactly 13.418501 from A's revolve axis and exactly 15.217519 from B's —
A's OUTER cylinder (`A#2`, the attribution tuple is `(is_a, face)`) and
B's outer cylinder (`B#2`), parallel oblique axes 1.9303 apart, radii
differing by 1.7990. The two cross-section circles CROSS, along one ruling
inside both 30.4° / 71.4° sectors, and the radial directions there differ by
**2.81°** (pin `s433_crossing_rulings_match_the_r0038_probe_vertex`). Stage 0
is active on the pair (both revolves start on the same sketch plane: A#0 ×
B#0 coplanar) and `cyl_pairs` is empty.

**The mechanism, in the point form's terms.** With parallel axes every
facet-pair intersection of the two prisms is an axis-parallel LINE, so the
exact arrangement's answer for one ruling is however many times the two
cross-section POLYGONS cross near it. The surfaces separate as `sin α · s`
(`α` the crossing angle, `s` the arc distance from the ruling) while each
polygon's chord sags as `s(L − s)/2R` inside its circle; at a grazing `α`
the sags dominate and the polygons cross several times (R0038: three
chords, `YANG_LRR_PATCH n_degen=3`, all six vertices at one θ over the full
7.5 axial span). Stage 3 matches every chord to the one exact ruling, Stage
4 relocates them all onto it, and the strips between them collapse into
zero-area collinear chains — `degenerate_no_longedge` →
`LocalRefinementRequired` (R0038), or on the reduced fixture an A-cap
boundary the Stage-6 walk cannot close (`s6-boundary-walk-deadend`). The
§5c.10 re-CDT refutation stands for what it measured (a ONE-SIDED keep-
interior re-CDT cannot reproduce the other side's collinear seam); it does
not name the owner, which is §4.4.1's rule applied one stage earlier: give
both meshes the curve BEFORE the arrangement, exactly as §11 does for a
tangent ruling.

**The mint (`boolean::tangency::cyl_cyl_crossing_generators` +
`mint_crossing_rulings`).** In the cross-section plane the circles (radii
`R_A`, `R_B`, centres `δ = |w⊥|` apart along `m`, `n = û × m`) meet at
`x = (R_A² − R_B² + δ²)/(2δ)`, `y = ±√(R_A² − x²)`: two feet `a + x·m ± y·n`,
each minted as a ruling of BOTH tubes through the §11 rim-sample channel
(four exact samples `p₀ + h·û`, `push_rim_unless_seam`). Admissibility is
STRICTLY transversal — `|R_A − R_B| + band < δ < R_A + R_B − band` with the
KV10 rounding band — so the crossing and tangent forms are disjoint in δ
and a δ inside the band is never minted as two rulings a rounding apart.
Same fail-closed gates as `mint_generator`: canonical tubes, an EXACT
coordinate axis (the collinearity frame), axial overlap beyond the rim
margin, on-surface postcondition of each foot; both rulings lie on both
full circles so there is no angular containment in this vocabulary. Runs
on both the pre-Stage-0 boost and the idle-route re-mint (rim samples only,
no Stage-0 emission concern: a crossing ruling is an ordinary transversal
edge, not a pinch — the C0042 external-contact scope of §12 does not
apply). With the ruling minted the two polygons SHARE the crossing vertex
and cross exactly once there — the chord-depth ordering `sin(θ_B/2) −
sin(θ_A/2) < sin α` holds on the fixture and R0038 (0.017 / 0.025 against
0.052 / 0.049); a member that violates it keeps its loud STOP (named, not
built: a rim-density demand from the same inequality, the §4.5.4 channel).

**The second wall, in kernel-v2 (`geom::planar_loop_signed_area`).** With
the ruling minted the fixture's cut and union both COMPLETE in yang and
were then refused by `from_yang` step 1d: "output face plane normal
disagrees with its outer-loop Newell normal". The refused face was the
CORRECT top cap — the 0.10-thick crescent between A's 118° arc (through
(−1, 0)) and B's arc back. The orientation oracle (PR-KV9 / KV11 / KV16)
sampled each arc by ONE parametric midpoint and took the polygon's Newell
normal; a 118° arc's one-midpoint polygon sags `R(1 − cos 29.5°) = 0.13`,
more than the crescent's thickness, so the sampled polygon was CLOCKWISE
(shoelace −0.014) and a correct output was refused. Finer sampling only
moves the threshold. Both consumers (`from_yang` 1d and
`validate_planar_face`) now take the EXACT signed area `½∮ n̂·(p × dp)`:
the vertex shoelace plus, per curved edge, the closed-form chord-to-curve
segment — circle `(R²/2)(θ − sin θ)`, ellipse `(ab/2)(θ − sin θ)` in the
parametric angle, hyperbola `(ab/2)(θ − sinh θ)` — each signed by the
traversal sense about the face normal. Positive = outer, negative = ring,
zero = degenerate; no sample, no tolerance. The δ = 0.25 sweep member
(10.7°, crescent 0.10) was failing on THIS wall alone (the arm off) and
converts on the oracle fix alone: the two pieces are independent and both
needed.

**Measured (`cyl_cyl_grazing_ruling_sweep`, A r 1 × B r 1.15, cut).**

| axis offset | crossing | crescent | before | arm on, oracle old | both |
|---|---|---|---|---|---|
| 0.16 | 2.98° | 0.010 | `NonManifoldOutput` (s6 walk dead-end) | Newell refused | OK, exact |
| 0.18 | 5.32° | 0.030 | same | Newell refused | OK, exact |
| 0.20 | 7.07° | 0.050 | same | Newell refused | OK, exact |
| 0.22 | 8.61° | 0.070 | same | Newell refused | OK, exact |
| 0.25 | 10.70° | 0.100 | Newell refused | Newell refused | OK, exact |
| 0.30 … 0.50 | 13.9° … 25.7° | ≥ 0.15 | OK | OK | OK, byte-identical |

"Exact" = kernel-v2's exact volume against the closed-form disc-minus-lens
to 1e-9 (`cyl_cyl_grazing_ruling_kv2`: cut, union, and the 25.7° control).

**Not this increment (R0038 itself stays ERROR, byte-identical).** R0038
needs three more things the fixture does not: (1) its laterals are partial
REVOLVE sectors (arc rims + two ruling edges), outside `tube_axial_span`'s
full-circle vocabulary — the rim samples would go on ARC edges; (2) its axis
is oblique, `(0.4034, 0.9150, 0)`, so the four rounded rim samples are not
exactly collinear and the coordinate-axis gate declines (the honest
remedy: splice the overlap segment's two endpoints into the OTHER operand's
lateral as on-ruling interior points, so the shared segment is bit-identical
in both meshes regardless of frame); (3) it is on the Stage-0 path, whose
from-topology rebuilds carry `standing_rim` but no face-interior overrides.
Each is a named checkpoint of this increment's sequel.

**Oracles.** yang-rs `tests_unit::s433_tangent_relocation` +3 (feet on both
circles at the fixture's 2.98°; R0038's real oblique geometry puts the
probe vertex on a returned ruling to 1e-12; declines: tangent band, nested,
disjoint, coaxial, crossing axes; a δ just past the band gives two honest
rulings); kernel-v2 `geom::loop_area::tests` (square, two half-arcs = π,
270° major arc, the thin crescent EXACT and its sampled polygon pinned
CW, two half-ellipse arcs = πab, hyperbola segment sign both ways);
test-harness `cyl_cyl_grazing_ruling_kv2` (3 tests) and the sweep driver
(`#[ignore]`). Corpus: recorded in the ledger row (`docs/yang_tail_triage.md`,
2026-09-21 later).

### 13.1 Checkpoint 2 (2026-09-21, later, ALWAYS-ON) — the SECTOR vocabulary and the crossing-angle rim-density demand; R0038 replicated in a coordinate frame CONVERTS

**Sector laterals.** `tube_axial_span` now also accepts a partial-revolve
lateral — the Stage-1 partial patch strip's own `[Arc, Line, Arc, Line]`
pattern (two `Circle` edges with `start != end`, two `LineSegment`
rulings, four edges) — with the two ARC edges as its rims and, per arc, an
`ArcGate` (centre, normal, radius, start, end). A ruling is minted only when
its rim sample lies STRICTLY inside BOTH arcs' CCW sweeps in their own
frames and farther than the chord margin from both endpoints
(`ArcGate::contains`); a ruling at an arc endpoint is the sector's own
boundary ruling, a corner of higher order, never a mid-face mint. Arc rims
already take rim overrides with the full rim's merge policy (the M8-mixed
amendment), and the partial strip pairs its two chains index-for-index, so a
sample inserted at one azimuth on both arcs keeps the pairing conformal. In
the crossing arm a declined ruling skips that ruling, not the pair (a sector
contains at most one of the two); in the generator arm it skips the mint.
Unit pin `s433_sector_arc_gate_contains_only_the_open_sweep`.

**The second crossing, measured on the replica (`r0038_replica_kv2`:
R0038's two revolves with their parallel axes moved onto z, Stage 0 ACTIVE
on the shared sketch plane).** With the ruling minted on both operands'
arc rims (all four samples bit-collinear, B's longer ruling split by the
arrangement at A's rim heights exactly as designed), the cut STOPped at
R0038's own site: ONE zero-area triangle `[53, 50, 19]` on B's outer face,
its long edge the ruling between A's rim heights and its off-vertex an
arrangement vertex 0.020 above A's bottom rim, RELOCATED onto the ruling.
The Stage-1 dump named the cause: A's arc has a uniform slot 0.118° past
the mint (at 22.82°, ON A's circle), while B's next vertex after the mint
is 6.87° away, so B's chord there is already `s(L − s)/2R_B = 1.65e-3`
inside its circle against a surface separation of only `sin α · s =
1.35e-3` — B's polygon dips back inside A's at A's slot and crosses out
again 0.02° later, a sliver strip between the ruling and that second
crossing, and relocation collapses it. This is exactly the chord-depth
ordering §13 named and did not build: with parallel axes the two polygons
must cross ONCE, at the minted vertex, and the sufficient condition on
both sides is that every chord step adjacent to the mint has
`sin(θ/2) < sin α` — the shared rim count **`N ≥ π/α`** (R0038: α =
2.806°, N ≥ 66).

**The demand (Yang §4.5.2's resolution rule, decided at mint time from the
geometry).** `mint_crossing_rulings` computes α from the two radial
directions at the foot, demands `N = ⌈π/α⌉ + 1`, and returns it through
`TangentOverrides::min_rim_n` / `tangent_generator_rim_overrides`'s third
element; the pre-Stage-0 boost rebuilds both operands with
`BRep::rebuilt_with_rim_overrides_at_least`, which raises `forced_rim_n`
(the phantom-guard channel — a STORED minimum, so Stage 0's and §4.5.2's
from-topology rebuilds keep it, and the larger of an existing boost and the
demand wins). Past `CROSSING_RIM_N_CEILING = 512` (α below 0.35°) the mint
DECLINES — status quo, the loud STOP — rather than tessellate every rim of
both solids at thousands of segments. Not a band: the demand is the
mesh-resolution certificate the crossing needs, the same class as the
§4.5.4 chart scan's rim demand (F0082).

**Result.** The replica completes as TWO bodies — the inner band between
A's inner arc and B's inner cylinder (which threads A's annulus without
crossing A's inner circle) and the outer crescent from the sketch plane to
the ruling — with exact volumes 7.560 and 3.474 against the closed-form
polar-grid components to 2e-3, each shell watertight with χ = 2 (the
harness gained `solid_handles` for multi-body results; `solid_handle`
returns only the first). The grazing sweep and fixtures, C0043/C0056's
pins, all still pass at the demanded density. Corpus (release, 8 jobs,
600 s; wall 803.8 s): **293C / 0W / 13E / 4EE / 0T + 2 UNSUPPORTED**,
results.json byte-identical — no corpus revolve pair with parallel
coordinate axes crosses at a grazing angle, and R0038's own axis is
oblique, so it declines at the collinearity gate exactly as before.

**Remaining for R0038 (checkpoint 3): the OBLIQUE frame.** In a coordinate
frame the four rim samples `p₀ + h·û` are exactly collinear (two
coordinates bit-identical); on `(0.4034, 0.9150, 0)` they are collinear
only to rounding and the exact arrangement would see two skew
femto-segments. The remedy is to make the SHARED segment bit-identical in
both meshes without relying on the frame: mint the overlap span's two
endpoints (whichever operand's rim sample bounds it) into the OTHER
operand's lateral as face-interior points ON its ruling — the P3b inc-4e
splice already performs a conforming 2+2 edge split for a point within the
weld band of a grid edge — so B's ruling becomes the chain `q_B0 → P → Q →
q_B1` and `[P, Q]` is one identical edge in both meshes. Two prerequisites
are named: the face-interior channel must become STANDING (a `standing_rim`
analog carried by every from-topology rebuild, Stage 0's
`build_stage0_mesh` included — today that path threads rim overrides only),
and the coordinate-axis route stays rim-only so C0043 / C0056 / the
replica remain byte-identical.

### 13.2 Checkpoint 3 (2026-09-21, night, ALWAYS-ON) — the OBLIQUE frame: the shared segment made bit-identical by on-ruling interior splices, standing through every rebuild; R0038 CONVERTS

**The planner (`ruling_rim_samples` → `RulingPlan`).** On an exact
coordinate axis nothing changes: rim samples only, byte-identical to
§11/§12/§13.1. On an oblique axis the overlap of the two axial spans
`[lo, hi]` (measured from `p₀` along `û`) has endpoints `S_lo = p₀ + lo·û`
and `S_hi = p₀ + hi·û`, computed ONCE; whichever operand's rim height
equals the bound bit-for-bit already has `S` as its rim sample (the same
expression, the same bits), and the OTHER operand — whose ruling runs past
the bound — receives `S` as a face-interior point on its lateral. The P3b
inc-4e splice finds `S` within the weld band of that lateral's ruling edge
(it is ulps away) and performs the conforming 2+2 split, so B's ruling
becomes the chain `q_B0 → S_lo → S_hi → q_B1` and `[S_lo, S_hi]` is ONE
identical edge in both meshes — the exact arrangement has nothing skew to
see. A rim within `FLUSH_RIM_GUARD = 64` chord margins of a bound, yet not
at it, declines the whole ruling (an interior point that close to a rim
ring would also lie in the weld band of the ring's edges — an ambiguous
split; fail closed). Both arms use the planner; the coordinate-axis SKIP is
gone.

**Standing face-interior points (`BRep::standing_face`).** The
`standing_rim` analog: populated by `rebuilt_with_all_overrides` /
`rebuilt_with_overrides_at_least` (composed bit-deduped with what stands),
carried by every `from_topology*` rebuild (`retessellated_at_current_d_eps`,
`rebuilt_with_min_rim_segments`, the spike normalization) and by Stage 0's
EMITTED-mesh builds — `build_stage0_mesh` and the coincident-cylinder build
now tessellate through `stage1_tessellate_with_standing_overrides` (rim +
face; an empty face map is the byte-identical rim-only path). The three
Stage-0 ring readers (`frame.rs`) read planar rings only and are untouched.
The pre-Stage-0 boost carries the payload as `GeneratorBoost { rim_a,
rim_b, face_a, face_b, min_rim_n }`, registers every point in
`minted_junction_keys`, and rebuilds both operands with
`rebuilt_with_overrides_at_least`.

**R0038, measured.** `[tangent-insert] A#2 B#2 MINT crossing ruling … over
h ∈ [−3.7528, 3.7528] (crossing 2.8061°, rim N ≥ 66, interior splices A 0
B 2)` — A's rims bound the overlap, B's lateral takes the two endpoints —
and all three operations COMPLETE. The final answer is TWO bodies: the
inner band between A's inner arc and B's inner cylinder, through which op
3's torus tube (minor radius 1.55, crossing the 0.13–0.35-thick band at
A-azimuth ≈ 15°, mid-height, its end cap in A's bore) bores a clean
THROUGH-HOLE — yang emits inner loops on both cylinder walls and the torus
face as the tunnel wall (genus 1, χ = 0) — and the outer crescent (r ≥
13.29, beyond the torus's farthest reach of 12.81; χ = 2). Total χ = 2 with
2 shells, which the corpus meta's single `euler_target = 2` decodes as ONE
shell (the oracle then expects 4): the first run graded SUPPORTED_WRONG on
that expectation, not on the geometry. Adjudication: the cubical
exact-membership ladder cannot read this document (the crescent tapers to a
knife edge at the ruling — 7/8 components and χ 10–12 at 128–512 cells,
the R0091 scope note), so the coordinate-frame replica carries the full
chain (`r0038_replica_full_chain_is_a_holed_band_and_a_crescent`): a
deterministic 3D polar-grid integral of A − B − T gives the two bodies'
volumes (the crescent's exact volume to 5e-3, the holed band's mesh volume
to 2e-2 — its cylinder patches carry boolean chord facets, outside the
exact closed form's declared scope), each shell watertight, the band χ = 0
and the crescent χ = 2. R0038's meta now authors
`expected_shell_count: 2` (the R0003 precedent; strict: the count must
match exactly and χ must equal 2), and the case grades SUPPORTED_CORRECT.

**Oracles.** test-harness `cyl_cyl_grazing_ruling_kv2` +2 (the grazing
pair on the oblique axis `(0.36, 0.48, 0.8)`, cut and union, exact
volumes to 1e-9); `r0038_replica_kv2` +1 (the full chain); the §12/§13
fixtures and all 29 §4.3.3 unit pins byte-identical; yang-rs lib 988
green; smoke pin R0038 (0.9 s release). Corpus: the ledger row.
