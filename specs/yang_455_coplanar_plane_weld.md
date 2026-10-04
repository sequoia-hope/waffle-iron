# §4.5.5 — the shared plane at the B-Rep level (Stage-0 plane weld)

**Status: LANDED 2026-10-04.** `crates/yang-rs/src/stage0/plane_weld.rs`,
always-on, kill switch `YANG_PLANE_WELD=off`, probe `YANG_PLANE_WELD_PROBE=1`.
Anchor: `error_oct4.waffle` (a user document, 2026-10-04), two STOPs deep.
Ledger: `docs/yang_deviations.md` N79.

## 1. The paper

Yang §4.5.5 (`refs/text/yang2025_hybrid_boolean.txt:717-731`): coplanar faces
are detected BEFORE discretization; the overlap is replaced by ONE shared
trimmed surface and "identical meshes are generated for both models in this
part"; the common part and the two remainders "share identical sampling
points on their boundaries". The paper assumes coplanar faces are coplanar.
The implementation's near-band (`scan_near_coplanar`, spec
`yang_178_subres_coplanar_gap_stop.md`) admits pairs whose planes differ by a
sub-resolution gap (`gap ≤ band/100`, the coincidence-authoring noise class),
and dissolves the gap: `stage0_preprocess` snaps every loop vertex of the pair
onto the group's canonical plane (face A's) and lifts every overlay vertex
onto it (`Frame::snap` / `Frame::lift`, `stage0/frame.rs`).

## 2. The gap that was left

What the snap did NOT move was the analytic geometry those vertices bound:
face B's stored `Surface::Plane`, and the `Curve::Circle` centre of a rim on
it. `PairPlane::face_a`'s doc says so ("its STORED plane can be up to `band`
away"), and `specs/yang_137_torus_plane_grazing_corner.md` §(186-188) records
it as accepted. Two Stage-1 consumers read the stored circle:

- the uniform rim samples, `c + r(cosθ·e1 + sinθ·e2)` with `c` the stored
  centre (`stage1_tessellate.rs`, closed ring and arc chain);
- the opposite-rim image of a rim crossing, which strips the axial component
  and re-attaches it at the opposite circle's stored centre
  (`stage0/rim_chords.rs::opposite_rim_image`).

Both land on B's plane. The seam vertex (snapped), the corners (snapped) and
every overlay mint (`Frame::lift`) land on A's. On a welded pair each rim ring
therefore carries two copies of every crossing, `gap` apart along the normal.

## 3. Measured (2026-10-04, `error_oct4.waffle`)

A 100 × 85 × 10 mm rectangular frame (80 × 75 mm hole), then a sketch on its
top face and a Ø22.36 mm circle centred ON the hole's corner vertex, whose rim
passes exactly through the frame's outer corner, cut 10 mm (through). The
app's `computeFacePlane` takes the sketch origin from the render mesh's
`Float32Array`, so the sketch plane — and the cut cylinder's top cap — sits at
`f32(0.01) = 0.009999999776482582`, 2.235e-10 below the frame's top face at
`0.01`. The bottom cap sits 2.235e-10 below `y = 0`. Both pairs weld.

Because the circle is centred on the hole corner, the top cap's crossings of
the hole edges and the bottom cap's crossings of the same edges share EXACT
azimuths; the crossing of the hole's `x = −0.04` edge is at exactly 90° from
the seam. The Stage-1 self-contact refinement drove the ring to N = 56, which
samples 90°:

1. `circle edge 0: two distinct rim-crossing overrides claim uniform sample
   k=14 (n_seg=56): first (−0.04000000003725291, 0.009999999776482582,
   −0.02631966009687242) then (−0.04000000003725291, 0.01,
   −0.02631966009687242), 2.235e-10 apart` — the mirrored image (B's plane)
   and the own crossing (A's plane). Row 5 of
   `specs/m8_rim_override_uniform_merge.md` refused by bit identity.
2. With row 5 inserting the twin instead (§5): `Stage-1 mesh of one operand
   self-intersects: 12 improper triangle contact(s) … between faces 0 and 2`
   — operand B's own cap and lateral meeting in T-junctions between the
   twins.

## 4. The fix

`stage0::plane_weld::weld_coplanar_planes(a, b)` runs in `boolean()` right
after the §4.5.5 edge-in-plane identification and before `stage0_preprocess`:

- the same scan and the same plane groups (`scan_near_coplanar`,
  `build_plane_groups`, `canonical_frame` of the group's first A face);
- for every participating face whose UNIT plane is not the canonical plane
  bit for bit: rewrite `Surface::Plane` to `±n, ±d` (the face's own
  orientation, so a stacked B face keeps its opposing normal), `Frame::snap`
  the in-plane anchor of every curved edge on it (circle / ellipse /
  hyperbola centre, parabola vertex) and every loop vertex;
- rebuild through `BRep::rebuilt_with_geometry` (same counts, same indices;
  the standing Stage-1 overrides carried on — the `rebuilt_with_vertices`
  shape).

A group in which every face already carries the canonical plane is skipped,
so the step is `None` — no rebuild, byte-identical — for bit-exact coplanar
input (the whole generated corpus). A scan `stage0_preprocess` refuses (an
intra-solid pair, a sub-resolution gap) is left to it, untouched.

## 5. Row 5 of the uniform-merge rule

Independently of the weld, the ring build's row 5 was a bit-identity refusal
where the rest of the rule is distance-based: a SECOND sub-TAU twin of a
uniform sample, distinct in bits from the one the slot took, now enters the
ring as an inserted override at its own angle — exactly what the generic
path does for band-close twins anywhere else on the rim (both must enter, or
the ring desynchronizes from the cap overlay that carries both). The row-3
real-scale wall is checked first, so claimant order cannot smuggle a graze in.
Spec table updated in `specs/m8_rim_override_uniform_merge.md`; pinned by
`rim_override_same_slot_repeat_dedups_conflict_is_loud`.

## 6. Pins

`crates/yang-rs/src/tests_unit/plane_weld.rs`: bit-exact pair ⇒ `None`; no
pair ⇒ `None`; residual cap (2.235e-10) ⇒ B's cap plane, rim centre and seam
vertex at the canonical plane bit for bit, A untouched, the untouched bottom
cap untouched; the residual through-cut builds and matches the exact cut's
face count with every top vertex at `z = 1` exactly.

Document replay (`WAFFLE_PATH=error_oct4.waffle … user_case_probe
replay_waffle_env`): no engine error; 142 render triangles; all mesh oracles
PASS; volume 2.301939e-5 m³ against the analytic 2.301865e-5 m³ (the
difference is the render chord deficit on the cut's concave arc).

## 7. The upstream producer (app — FIXED 2026-10-04, same day)

`app/src/lib/engine/store.svelte.js::computeFacePlane` derived a face's plane
from the FIRST rendered triangle of the face range — `Float32Array`
positions — so every sketch started on a model face carried an f32-rounded
origin, and every extrude from it started a sub-TAU distance off the face.
Only ghost (in-context) face ranges carried the engine's exact plane.

Fix: `wasm-bridge/src/render_view.rs::build_face_entries` now emits
`plane: { origin, normal }` (the engine's f64 face centroid + normal, the
same `planar_face_plane` definition the rebuild re-derives) for EVERY planar
face range, ghost or not; `computeFacePlane` already preferred it, and the
f32 triangle path is left only as a fallback for a range without a plane.
Pinned by `app/tests/gui/face-sketch-exact-plane.spec.js`: a 61.3 mm
extrusion (not f32-representable), every face range carries a unit-normal
plane, the +Z cap's origin is 61.3 to f64 rounding and NOT an f32 value, the
sketch started on it takes that origin bit for bit, and the stored sketch
keeps it. The kernel weld stays: imported STEP and other producers carry the
same class.
