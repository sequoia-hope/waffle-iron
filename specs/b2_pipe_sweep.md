# B2: pipe sweep — a circle along a planar tangent chain of lines and arcs

Roadmap: `specs/custom_features_and_modeling_roadmap.md` §B2 (Part C step 4).
Owner crates: `kernel-v2` (checkpoint 1), `waffle-types` + `modeling-ops` +
`feature-engine` + `wasm-bridge` (checkpoint 2), `app` (checkpoint 3).

## 1. The join-strategy decision (the gate Part C names)

§B2 left open how consecutive pipe segments join: a union through the M8
coplanar Stage-0 path, or the share-a-face cap. **Neither.** A pipe is built
as ONE solid by direct assembly, the way `extrude_circle` and
`build_torus_revolve` already build a straight and a bent tube: consecutive
laterals SHARE their rim circle as one closed `Curve::Circle` edge (twin
pair), so there is no cap to join and no boolean at all. The rim edge is
exactly where the two analytic surfaces (cylinder / torus) meet, and on a
G1 chain that is a smooth junction — there is no seam *surface* to reconcile.

Why this is the structural choice and not a shortcut:

- The union of two bodies that touch on a full disc is a Stage-0 coplanar
  case with a shared-boundary circle — the hardest M8 class, and a rounding
  hazard on every join. Direct assembly has zero tolerance content: the rim
  circle is one edge, bit-shared.
- The share-a-face cap only defers the same coplanar union to the next
  feature.
- The result is exactly the B-Rep any commercial kernel produces for a
  pipe: caps + one face per segment, G1 rims between them.

## 2. Seam placement (the one generalization the kernel needs)

Every lateral needs a seam edge (Stroud §3.1.4): a straight ruling on a
cylinder, a longitude `Curve::Arc` on a torus. Each rim circle is anchored at
ONE seam vertex, shared by the two laterals meeting there, so the seam of
every segment must pass through the same phase of the tube. The existing
torus builders put the seam on the OUTER equator (poloidal φ = 0, seam radius
`R + r`). On an S-bend the outer side flips between consecutive arcs, so an
equator seam cannot be shared across the joint.

Decision: the pipe seam runs along the **binormal** of the path plane —
anchor = rim centre `+ r·n̂` (n̂ the plane normal, the same side for every
segment). On a torus segment whose axis is `s·n̂` (`s = ±1` by the arc's
sense) this is the poloidal phase `φ₀ = s·π/2`: the seam is the
`Curve::Arc { center: C + r·n̂, normal: ±axis, radius: R }` longitude.

The two sites that recover a torus lateral's seam by *radius* (`R + r`) are
generalized to accept a seam at any phase, byte-identical for φ₀ = 0:

- `kernel_v2::tessellate::surfaces::torus::tessellate_torus_lateral` — the
  seam is any open `Curve::Arc` about the torus axis; φ₀ is derived from
  the seam arc's own (centre offset, radius) and the φ table starts at φ₀.
- `yang_rs::stage1_tessellate::patch_tessellators::tessellate_torus_face`
  — an OPEN circle edge is the seam whatever its radius; the uniform grid
  arm places the seam chain in the column of its actual φ slot (slot 0 for
  the equator, unchanged), the non-uniform arm was already phase-general.

`validate_solid` checks nothing about torus seams (audit 2026-09-21), so
no validation change is needed; the torus `reversed` flag is likewise
unvalidated — the pipe sets it explicitly per chain (outer `false`, inner
bore `true`).

### 2.1 What the boolean round trip needed (found by the re-entry oracles)

Four more generalizations, each a pre-existing gap the pipe is the first
customer of; every one is byte-identical for the shapes that existed
before:

1. **Stage 4 skips a tangent pair at an operand's own boundary vertex**
   (yang `stage4_correct.rs`, the torus implicit-pair arm). The arm
   projects EVERY torus-bearing patch-boundary vertex onto its surface
   pair — intersection vertices AND an operand's own rims (§4.4.2 says the
   original boundary curves are restored as they are, but the projection
   is doing real work there: Stage-1 samples of a recovered rim sit off
   the exact torus by more than the on-surface band — R0026 — and the
   snap is the same well-posed Newton). On a G1 cylinder↔torus rim the two
   surfaces are TANGENT and the pair Newton is rank-deficient by
   construction, so the arm now checks tangency at a vertex that is NOT on
   the intersection curve and skips it (the vertex lies on the shared rim
   circle exactly; nothing to solve). A tangency at a vertex ON the curve
   stays the loud STOP. (A blanket "intersection vertices only" filter was
   tried first and regressed R0026 CORRECT→ERROR — the unmasked-latent
   pattern; the tangency-only skip is byte-identical everywhere the old
   arm converged.)
2. **Full-circle rim sense from a curved use** (kernel `from_yang.rs`).
   The output conversion derived a rim circle's directional normal only
   from an adjacent planar cap. A rim between two curved laterals now
   reads it from the lateral's material sense — the direction INTO the
   face along the surface (`±axis` on a cylinder, `±` the tube-centre
   tangent on a torus), signed by the boundary edge leaving the rim's
   anchor, negated for a cavity wall.
3. **Torus-band seam recovery** (kernel `recover.rs`, PASS 0). yang drops a
   band's seam as patch-interior; the band comes back as two closed rims
   and no seam, which nothing downstream can orient. The recovery pass now
   re-mints it the way it re-mints cylinder rulings: both rims anchored at
   one poloidal phase (the rim's ORIGINAL input vertex when it survives —
   `boolean_op` hands the operands' vertex positions in — so a whole pipe
   chain stays coherent), joined by the longitude arc through the feet.
   Which of the two complementary bands the face is comes from the
   output mesh (the attributed triangle nearest a rim). A band wider than
   one sub-π seam piece (`MAX_ARC_PIECE_SWEEP`) is left alone and fails
   typed downstream — a bend over ≈149° that SURVIVES a boolean is a
   known wall; the constructor itself takes any sweep.
4. **Cavity winding in the structured torus grids** (yang Stage 1
   `patch_tessellators.rs`). The `(θ × φ)` band and closed-torus grids
   wound every triangle to the torus's own outward normal, ignoring
   `reversed`; a hollow bend's bore band therefore ran its shared rim in
   the same direction as the bore cylinder and the arrangement was
   non-manifold at the input. Same rule as every other Stage-1 path now.

## 3. Kernel constructor (checkpoint 1)

```
PipePath::new(origin, u, v, edges: Vec<ProfileEdge>) -> Result<PipePath>
pipe(arena, &PipePath, radius, inner_radius: Option<f64>) -> Result<PipeResult>
PipeResult { solid, shell, start_cap, end_cap, walls: Vec<FaceId>, inner_walls: Vec<FaceId> }
```

`PipePath` validation (all before the first arena mutation):

1. frame finite, orthonormal (the `Profile::circle` rules);
2. ≥ 1 edge; edges chain head-to-tail; the chain is OPEN (a closed loop is
   a typed `PipeClosedPathUnsupported` — genus-1 rings are a later slice);
3. every line has positive length; every arc has finite positive radius,
   both endpoints on its circle (import band), a sweep in `(0, 2π)` taken
   from `ccw` (arcs > π ARE allowed here, unlike `ArcPolygon`);
4. every interior joint is G1: incoming and outgoing unit tangents agree
   within `PIPE_TANGENT_TOLERANCE` (1e-9) — else
   `PipeJoinNotTangent { joint }` (a mitre is a later slice).

`pipe` validation: `radius` finite `> 0`; `inner_radius` finite and in
`(0, radius)`; every arc's radius `> radius + clearance`
(`PipeBendRadiusTooSmall { segment }` — a tube bent tighter than its own
radius pinches to a non-manifold seam, exactly the revolve axis-clearance
rule).

Topology for `n` segments, solid tube: `V = n+1`, `E = 2n+1` (`n+1` rims +
`n` seams), `F = n+2`, genus 0. Per segment one lateral face whose outer
loop is `[rim_start, seam_up, rim_end, seam_dn]` (the `extrude_circle`
template): `Surface::Cylinder { axis_point: start centre, axis_dir: t_j,
radius }` for a line, `Surface::Torus { center: C, axis_dir: s·n̂,
major_radius: ρ, minor_radius: r }` for an arc. Rim directional normals are
the joint tangents `t_i` (start rim `+t_i`, end rim `−t_{i+1}`, twins exact
negations — ONE `t_i` value per joint). Caps: planes through the end centres
with normals `−t_0` / `+t_n`, one closed circle CCW around the normal.

Hollow tube (`inner_radius = Some(ri)`): a second chain of laterals at `ri`
with `reversed: true` (cylinder rims traverse AWAY from each other, the
validated cavity-wall sense) and annular caps (outer circle + inner ring
CW). `V = 2(n+1)`, `E = 2(2n+1)`, `F = 2n+2`, `R = 2`, **genus 1** (an
annulus swept along an open path is a solid torus: `V − E + F − R = 0`).

Exact volume: `geom::signed_volume` gains the torus-band flux
`(1/3)Φ = α·R·π·r² + (π r²/3)(c₀·ν₀ + c_α·ν_α)` (the band's flux is the
Pappus tube volume minus the two disc-cap fluxes; `ν` the rims' directional
normals in the lateral loop), so `solid_volume` of a pipe is the closed form
`π(r² − rᵢ²)·L` to rounding.

## 4. Oracles (checkpoint 1, `crates/kernel-v2/tests/b2_pipe.rs`)

1. Topology census for line / arc / line→arc→line / S-bend (line→arc→arc→
   line with opposite senses) / U-bend (π arc) / 270° arc, solid and hollow.
2. `validate_solid` green; render mesh watertight (1e-9 keys), sane,
   positive volume within the chord band of `π(r²−rᵢ²)L`.
3. **Exact** `signed_volume = π(r² − rᵢ²)·L` to 1e-12 relative.
4. Every rim lies on both adjacent surfaces; every seam vertex is at
   `centre + r·n̂`.
5. Determinism: two builds are bit-identical arenas and meshes.
6. Refusals typed: closed path, non-tangent joint, bend too tight, bad
   radius, inner ≥ outer.
7. Boolean re-entry: a pipe minus a box through a bend completes
   (`to_yang` accepts the φ₀ seam; Stage 1 tessellates it) with the
   inclusion–exclusion volume from the exact oracle.
8. Existing torus revolve tests byte-identical (the φ₀ = 0 path).

## 5. Checkpoints

1. kernel-v2 constructor + seam generalization + exact volume + tests
   (this spec §3–4).
2. `Kernel::pipe` (defaulted `NotSupported`), `KernelV2Adapter`,
   `MockKernel`; `waffle_types::kernel::PathSegment`; a sketch-chain
   extractor (`waffle_types::path::extract_open_chain`: entity ids →
   ordered, oriented line/arc chain with tangency); `Operation::Pipe`
   (`PipeParams { sketch_id, entity_ids, radius, radius_expr,
   inner_radius, inner_radius_expr, combine, targets }`), execution in
   `rebuild.rs`, `modeling_ops::pipe`, roles, `ctx.pipe` script binding,
   `AUTHORABLE`, dispatch display name, `docs/FILE_FORMAT.md` §7.x (no
   reader-floor bump), `ModelBuilder::pipe`, `test-harness/tests/pipe_kv2.rs`
   with the `π(r² − rᵢ²)L` oracle against `solid_volume`.
3. `PipeDialog.svelte` (pick a sketch chain, radius, wall thickness),
   toolbar, feature list / property editor, GUI spec.

**Status 2026-09-21: checkpoints 1, 2 and 3 all LANDED** (kernel-v2
`tests/b2_pipe.rs`; feature-engine PLAN.md M15; `app/tests/gui/pipe-dialog.spec.js`).

Later slices (typed refusals until then): closed loops (genus 1), mitred
non-tangent joints, non-planar (3D) chains, a biarc approximation of
splines in the sketch layer.
