# Yang §4.3.3 + §4.4.1 — the tangent-point MESH UPDATE (C0058 / F0058)

Status: **MEASUREMENT + increment 1 landed** (2026-09-13). Corpus drivers:
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

The two tessellated prisms do **not** meet at the tangency: A's seam RIDGE at
θ = −90° stands at the full radius while B's facet plane there stands one
sagitta inside its own cylinder, so A pokes OUT of B and the mesh-level
intersection AVOIDS the tangent point. At Stage-4 entry:

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

## 6. What remains — and why it is §4.4.1, not relocation

The band between the two branches pinches to zero width at the tangent point.
**No mesh resolution resolves it**: a triangle of A's lateral adjacent to the
seam column always straddles the band near the tangency, so A's two sheets are
always edge-connected there. Relocation cannot fix that — it moves vertices onto
curves, it does not CUT the mesh along them.

Yang's pipeline does cut: §4.4.1, *"we trim and update the meshes using the
intersection curves … Then we set r_A = r_B = r, so that the two polylines in
the meshes coincide with the intersection curve … Next, through CDT we obtain
valid discretizations of the trimmed meshes"* (`refs/text/
yang2025_hybrid_boolean.txt:552-570`), with Fig. 11's split / merge / insert,
and §4.3.3's collinear-normal test naming the tangent point as first-class
(`:518-570`). That is open deviation **N2** (`docs/yang_deviations.md`), and
this is a NEW instance of it: the 2026-08-06d census concluded "the repair
belongs on the RELOCATION side, and the §4.4.1 mesh update is confirmed to be
the wrong place" for the F0067 class, whose crossings are MINTED by Stage 4.
C0058/F0058 are the opposite class — the crossing the output needs was never in
the mesh to begin with — and for them §4.4.1 is the only place.

**The next increment, stated concretely.** Insert both exact branches into A's
(and B's) triangulation across the tangency: split every triangle the branch
chords cross, merge a split point into a vertex within the Fig-11(b) band, and
re-CDT the affected fan. For C0058's `[32,33,1]` the `k₋` chord leaves through
edge (1,33) at θ = −87.645 (z = 0.99560), 0.165° from v33 — i.e. the insert is
a Fig-11(b) MERGE into v33, not a new vertex. The split pieces' keep/drop labels
are determined without re-running the classifier (below the lower branch → kept
for A; between the branches → inside B). Expected structure afterwards: the
tangency vertex's star becomes the four-sector fan §4 computes, A's patch flood
fill yields TWO patches, and both boundary cycles are simple.

**P10 stop criterion.** If the insert cannot be made conformal on BOTH operands
at once (the two meshes must agree on the branch polyline, Yang's
`r_A = r_B = r`), STOP loudly rather than inserting on one side — a one-sided
insert is the membrane class that `remove_doubled_membranes` exists to clean up
after.

## 7. Oracles

- **C0058**: ERROR → `SUPPORTED_CORRECT` against its meta volume 2.0819348684923513.
- **F0058**: ERROR → `SUPPORTED_CORRECT`; the undirected edge (1, 30) must carry
  exactly 2 triangles.
- **yang-rs unit**: `tests/tangency_pinch_split.rs` (3 tests, green today — its
  `cylinder_brep` operands seam AWAY from the tangency, which is why they pass
  where the corpus fails; keep them) and
  `tests_unit::s433_tangent_relocation` (§5).
- **kernel-v2 E2E**: `kv9_cyl_cyl_special::steinmetz_union_exact_volume` /
  `..._subtract_...` — un-quarantine in the converting PR; their `#[ignore]`
  reasons must be re-pointed at this spec, not at §2c.5a.
- **Non-regression**: full corpus, zero CORRECT lost, 0 WRONG.

## 8. Research basis

- [#24 Yang 2025] §4.3.3 (`:518-570`) — method selection; a single surviving
  point with COLLINEAR normals is a tangent point. §4.3.4 (`:571-605`) —
  refinement/dedup. §4.4.1 (`:552-570`, `:605-…`) — trim + update + CDT, Fig. 11
  split/merge/insert.
- [#24] §4.5.2 termination covers TRANSVERSAL intersections only; yang2023 §5.4
  certifies refinement does not converge near tangency — so mesh refinement is
  not an alternative route here (the R0050 adjudication,
  `specs/yang_452_local_refinement.md` §6).
- `specs/kv9_f1_tangency_inout_labels.md` — the tangency band and the exact
  junction closed form (still correct; only its §2c.5a "next increment" is
  superseded).
- `specs/yang_tangency_pinch_split.md` — the vertex-fan split; §0's exclusion of
  the perpendicular EDGE pinch is confirmed correct by §2 above.
