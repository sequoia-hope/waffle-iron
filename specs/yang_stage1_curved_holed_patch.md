# Spec: yang Stage-1 curved holed-patch tessellation (partial-patch re-entry wall)

Status: DRAFT (census complete; implementation not started)
Owner area: `crates/yang-rs` (Stage-1 tessellation) + `crates/kernel-v2` (to_yang_brep conversion)
Milestone tag: **KV14** (curved partial-patch re-entry)
References: [#24] Yang 2025 §4.1 (bijective Stage-1 tessellation); [#39] Livesu 2021 (simplified earcut CDT)

## Goal

Let a boolean **result** solid whose curved lateral faces carry holes and/or
non-canonical boundaries re-enter the yang-rs pipeline as an **operand** of a
subsequent boolean. Today this is the single largest capability gap in the
kernel: the `KernelV2Error::UnsupportedCurvedBoolean` wall, surfaced as the
assay `UNSUPPORTED(curved-profile)` class.

This closes the "a previous curved boolean's result cannot re-enter yang-rs
Stage 1" boundary declared in `crates/kernel-v2/src/boolean.rs` (`to_yang_brep`)
and `adapter.rs`.

## Census (2026-07-09, HEAD 42785308 baseline: 213 CORRECT / 0 WRONG / 39 ERROR / 42 UNSUPPORTED)

The `UnsupportedCurvedBoolean` error now carries a diagnostic `reason: &'static
str` (this PR). Replaying the 21 partial-patch UNSUPPORTED cases through
`single_case` classifies them by sub-branch:

| Sub-branch (reason) | Count | Cases |
|---|---|---|
| `curved lateral has inner loops (holed patch)` | 9+ | R0021, R0026, R0028(Torus), R0046, R0051, R0059(Torus), R0063, R0074, R0095 |
| `curved lateral outer loop not 4 edges` | 4+ | R0020, R0053, R0093, C0063 |

> **Census correction (2026-07-09, probe `KV14_SLICED_PROBE`).** Of the four
> "not 4 edges" cases, only **R0053 is a CYLINDER** ([L,A,A,A,L,A,A,A],
> winding ≈ 0 — a bounded partial patch, the true Slice-D target). **R0020,
> R0093, C0063 are CONES** (R0020 [L,A,A,A,L,A,A,A], R0093 [L,A,A,L,A,A],
> C0063 a single full Circle) → they belong to **Slice E (cone unroll)**, not
> Slice D. The earlier row assumed all four were cylinders; that was wrong.
| `planar-loop degree-4 boundary (ellipse/surface-pair edge)` | 2 | R0006, F0076 |
| heavy/uncensused (big gear models, 455–2420 faces) | ~6 | R0061, F0081, F0082, F0083, F0084, F0085 |

**Key de-risking finding.** The holed-lateral inner loops are composed ENTIRELY
of `Arc` and `LineSegment` edges — NO `EllipseArc` / `SurfacePair` (degree-4)
curves. Probe output (`KV_HOLED_PROBE`, since removed):

```
R0021  Cyl   outer=[L,A,L,A,L,A,L,A]                       inners=[[A,A,A]]
R0026  Cyl   outer=[L×33, A,A,A]                           inners=[[A,A,A]]
R0028  Torus outer=[L×14]                                  inners=[[L×13],[L×14]]
R0046  Cyl   outer=[L,A,L,A,A,L,A,L,A]                     inners=[[A,A,A]]
R0051  Cyl   outer=[L×36]                                  inners=[[A,A,A],[A,A,A]]
R0059  Torus outer=[L×14]                                  inners=[[L×14]]
R0063  Cyl   outer=[L×8,A×25,L×17,A×24,L×9]                inners=[[L×31]]
```

So the class is a general **curved-patch triangulation** problem, NOT a hard SSI
problem. The boundary vocabulary is exactly what `loop_polyline` already samples
(Line/Arc/Circle → point chains).

## Design: unroll to parameter space + reuse existing CDT

yang-rs ALREADY has the machinery this needs:

- `loop_polyline(f_idx, loop, edges, chains)` — samples a loop's Line/Arc/Circle
  edges into a chain of global vertex indices (`lib.rs`).
- `cherchi_rs::triangulation::cdt_polygon_with_holes_floodfill(local_verts,
  outer_local, holes_local)` — exact 2D constrained Delaunay triangulation of a
  polygon-with-holes, topological flood-fill classification (used TODAY by
  `tessellate_planar_curved_cdt_face` for planar holed faces, `lib.rs:2157`).

The curved-patch path is the planar CDT path with the ortho-projection replaced
by a **surface unroll** to parameter space:

| Surface | Parameter map (u, v) | Notes |
|---|---|---|
| Cylinder (axis `a`, point `p`, radius `r`) | `u = r·θ`, `v = axial` where `θ = atan2(·)` about the axis frame, `axial = (P−p)·â` | equal-area unroll; `u` scaled by `r` so CDT sees isotropic geometry |
| Cone (half-angle `α`) | `u = s·θ`, `v = axial`, where `s` = slant radius at `v` | `u` scale varies with `v`; use local slant radius at each vertex |
| Torus (major `R`, minor `rm`) | `u = R·φ` (toroidal), `v = rm·ψ` (poloidal) | two angular params, both wrap |

### The θ branch-cut (the crux)

`atan2` has a ±π discontinuity. A patch that straddles the branch cut unrolls
into two disjoint halves and the CDT is garbage. Robust handling:

1. Collect all boundary vertices' raw angles.
2. Find the **largest angular gap** in the covered set (sort angles, max
   circular gap between consecutive samples). Place the branch cut in the middle
   of that gap, so the patch is contiguous in `u` after unrolling.
3. If the patch covers the full 2π with no gap (a canonical full tube with a
   hole not touching the seam), keep the existing seam edge as the cut — the
   outer loop's ruling segments define it.

This is the ONE genuinely new piece of logic. Everything else is reuse.

### Mapping back

CDT triangles reference `local` param-space vertices; each maps 1:1 to a global
3D vertex index already placed by `loop_polyline` (boundary) plus any Steiner
points the CDT introduces (interior). Steiner points must be lifted from param
space back onto the exact surface (evaluate the surface at (u,v)) — this is
where bijectivity is preserved (each new mesh vertex maps to the source face).
Orient each triangle by the radial-outward (or `reversed`-inward) surface
normal, matching `tessellate_lateral_face`'s `orient_target`.

## Branch table

1. `inner_loops.is_empty()` AND canonical/partial/torus 4-edge pattern → EXISTING
   structured path (`tessellate_lateral_face`). Unchanged.
2. `inner_loops` non-empty, all boundary edges ∈ {Line, Arc, Circle} → NEW
   unroll+CDT path. (slices A–C below)
3. outer loop not 4-edge, no holes, all edges ∈ {Line, Arc, Circle} → NEW
   unroll+CDT path (same code as 2 with empty holes). (slice D)
4. any boundary edge ∈ {EllipseArc, SurfacePair} → REMAINS
   `UnsupportedCurvedBoolean` (degree-4 input tessellation is a separate
   milestone; needs param-space sampling of degree-4 curves). Loud, typed.

## Implementation slices (TDD, each RED→GREEN, ordered by tractability)

- **Slice A — cylinder holed patch, bounded (non-wrapping) outer. ✅ DONE.**
  `tessellate_lateral_holed_cdt` in `yang-rs/src/lib.rs`: dispatched from
  `tessellate_lateral_face` when `!inner_loops.is_empty()`. Unrolls to
  (u=r·θ, v=axial) with largest-angular-gap branch cut, samples every boundary
  loop via `loop_polyline` (Line + Arc), CDTs via
  `cdt_polygon_with_holes_floodfill`, maps back and orients radial (inward if
  `reversed`). Full-circle / degree-4 boundary edges are rejected by
  `loop_polyline` (loud) → later slices. Tests: `lateral_holed_patch_excludes_hole`
  (partial-arc sector + triangular hole; oracles: hole excluded, hole boundary
  edges are mesh boundaries, radial-outward) and
  `lateral_holed_patch_reversed_and_multi_hole` (reversed cavity wall + two
  holes; covers the `reversed` branch, P4). yang-rs 204→206 lib tests green, no
  regression (structured hole-free arms untouched). NOT yet wired end-to-end —
  kernel-v2 `to_yang_brep` still walls these faces (Slice C).
- **Slice B — PERIODIC STRIP (model correction). ✅ DONE.** The census-real
  cases are NOT Slice A's "non-wrapping partial patch with a small hole". They
  are periodic wall STRIPS: a boolean OUTPUT cylinder wall is bounded by two
  ENCIRCLING loops (a rim circle and/or an intersection ring, each winding ≈
  2π), optionally with interior window holes. A full-2π encircling loop unrolls
  to a zero-area horizontal line, so it CANNOT be a `cdt_polygon_with_holes`
  hole — Slice A's model fails outright (`CDT backend failed to triangulate`,
  proven on R0021/R0046 with the `KV14_PROBE` winding dump). Implementation in
  `tessellate_lateral_holed_cdt`:
  1. Classify every boundary loop by axial winding (`Σ Δθ`): `|Σ| > 1.5π` ⇒
     encircling (a v-extent strip boundary); `≈ 0` ⇒ interior window.
  2. `0` encircling ⇒ the Slice A partial-patch path (unchanged).
  3. `2` encircling ⇒ periodic strip: open each encircling loop into a
     u-ascending chain (anchor at min-u, walk toward the ascending neighbor —
     orientation-agnostic since a −2π rim descends), lay the lower-v chain
     forward and the upper-v chain reversed into ONE simple ribbon, and
     DUPLICATE each chain's first vertex at `u += 2πr` so the ribbon spans the
     full 2π (the seam wedge). Interior windows become CDT holes.
  4. Seam placement avoids windows: the branch cut is chosen from the WINDOW
     vertices' angular coverage (widest window-free wedge) so no window
     straddles the seam and splits.
  5. Any other encircling count ⇒ typed `MalformedTopology` (loud).
  Unit tests: `periodic_strip_two_encircling_rims` (pure tube strip, exact
  inscribed-area oracle) + the Slice A tests stay green.
- **Slice C — wire kernel-v2 conversion. ✅ DONE.** `to_yang_brep` routes
  CYLINDER holed laterals (Line/Arc/Circle boundary) through with their
  `inner_loops` via the shared `convert_lateral_edge` converter (extracted from
  the structured 4-edge path — no behavior change there). Non-cylinder holed
  patches (cone/torus = Slice E/F) and degree-4 boundaries stay typed
  `UnsupportedCurvedBoolean` with precise reasons. End-to-end kernel-v2 test
  `curved_holed_lateral_reentry` (boolean_chains.rs): cyl − window box → holed
  lateral, then a second boolean → CORRECT (analytic op1 volume + EXACT 0.18
  planar-notch decrement). Assay delta (release, ASSAY_JOBS=6/240s): CORRECT
  213 (unchanged), WRONG 0; UNSUPPORTED(curved-profile) 19→16 — R0021/R0046
  advance to their next real wall `UNSUPPORTED(coplanar-boolean)` (M8), and
  R0063 (a strip+window) passes the holed lateral but exposes a PRE-EXISTING
  planar annular-cap CDT failure (`tessellate_planar_curved_cdt_face`, the
  TessellationFailed ring-reject class — separate from KV14) → its only assay
  transition is curved-profile → ERROR. No case regressed from CORRECT; the
  strip capability is proven by the unit + end-to-end tests.
- **Slice D — non-canonical CYLINDER outer loop (no holes). ✅ DONE
  (2026-07-09).** kernel-v2 `to_yang_brep` routes a cylinder lateral with no
  inner loops but a non-4-edge outer loop through the CDT converter (the same
  `convert_lateral_edge` path, empty inner set). yang-rs
  `tessellate_lateral_face` now falls through its structured 2-rim / 2-arc arms
  to `tessellate_lateral_holed_cdt` when the outer loop has no full-circle rims
  and only Line/Arc edges — the winding-0 partial-patch (0-encircling) branch
  triangulates it. Cone/torus non-4-edge laterals stay the typed wall
  (`... not 4 edges (non-cylinder)`) → Slice E/F. Unit test
  `lateral_partial_patch_multi_arc_no_holes` (yang-rs: R0053's [A,A,A,L,A,A,A,L]
  sector, oracles: inscribed sector-wall area, watertight bounded patch,
  radial-outward) + end-to-end `curved_partial_patch_no_hole_reentry`
  (kernel-v2 boolean_chains: cyl − slab → 6-edge segment-prism wall, then a
  planar pocket re-enters via Slice D; analytic-band volume oracle on the final
  solid). Assay: **R0053 advances curved-profile → its next real wall M8
  coplanar** (Stage 0); no CORRECT case regressed. Only R0053 was a cylinder in
  this class (census correction above), so corpus movement is the single case.
- **Slice E — cone unroll. ✅ DONE (2026-07-09).** A cone lateral develops via
  its ISOMETRIC development — slant `ℓ = |v|/cosα` (v = axial-from-apex),
  flattened angle `ψ = θ·sinα`, laid out Cartesian `(ℓ cosψ, ℓ sinψ)` — NOT the
  naive `u = (v·tanα)·θ` rectangular map the spec table first proposed. The
  rectangular map is anisotropic (u-scale grows with v), which makes the CDT
  emit a skewed fan whose flat facets INFLATE the mapped 3D area (a Schwarz-
  lantern artefact — proven: a frustum-sector unit test measured 7.70 vs the
  true 7.02); the isometric development preserves the cone's intrinsic metric so
  Delaunay yields well-shaped grid triangles. `tessellate_lateral_holed_cdt` is
  now surface-generic (`LateralKind::{Cylinder,Cone}`): cylinder → rectangular
  strip, cone → annular sector, with per-kind map-back normal
  (`cone_outward_normal`). yang `tessellate_cone_face` routes inner-loop and
  non-canonical (Line/Arc-only, no full rim) cone laterals to it; the
  2-encircling periodic frustum band stays a typed wall (polar seam handling is
  a later sub-slice). kernel-v2 `to_yang_brep` routes CONE non-4-edge / holed
  laterals through, guarded to EXCLUDE full-circle-rim cones (apex fan / frustum
  band = structured vocabulary, no CDT re-entry → stays the typed wall). Unit
  tests: `cone_partial_patch_multi_arc_no_holes` (frustum-sector, exact
  developable-area oracle) + `cone_holed_patch_excludes_hole` (isometric-param
  hole exclusion). **Assay (release, JOBS=6/240s): 213→214 CORRECT, 0 WRONG,
  UNSUPPORTED(curved-profile) 16→14.** R0093 UNSUPPORTED→CORRECT (a cone partial
  patch, the clean win); R0020 advances curved-profile → its next real wall
  (Stage-4 LocalRefinementRequired, the #1 ERROR class — a separate milestone);
  C0063 stays walled (apex/frustum cone, full rim — correctly excluded).
  **Census correction (probe single_case):** R0026/R0051 are TORUS holed
  patches (Slice F), not cyl/cone as an earlier census assumed. No CORRECT case
  regressed.
- **Slice F — torus band. ✅ DONE (2026-07-09).** Probe `KV14_TORUS_PROBE`
  (kernel-v2, since removed) showed the corpus torus booleans are POLOIDAL
  PERIODIC BANDS: the boundary wraps fully in the poloidal angle φ (around the
  tube, `φwind ≈ ±2π`) while the toroidal angle θ is bounded — TWO full profile
  boundaries (outer + one inner, oppositely wound) bounding the tube. A torus is
  NOT ruled in the toroidal direction, so a flat unroll+CDT chords the sweep (a
  systematic ~18% area loss — the toroidal seam edge, a single long constraint,
  is not subdividable under `keep_constraint_edges`). The fix is NOT new code:
  yang already had `tessellate_torus_patch` (the render-path UV-CDT consumer)
  which projects the boundary into the (meridian, longitude) plane, SEAM-BRIDGES
  the two profiles with ON-SURFACE subdivision (`band_seam_bridge` / `bridge_pts`
  — exactly the missing piece), and refines interior Steiner points onto the
  torus. New `tessellate_torus_band` (yang) gathers the face's boundary + hole
  loops as 3D polylines, delegates to `tessellate_torus_patch`, and maps the
  fresh pool back to global verts by QUANTIZED position (1e-9 m): a profile vert
  recovers its shared global (watertight with the caps), a seam duplicate / Steiner
  vert welds by position (the two seam copies are ULP-apart on the periodic
  meridian — an exact key would crack the band, and Cherchi needs it watertight).
  kernel-v2 `to_yang_brep` routes a torus lateral through iff `!curved_full_rim`
  AND `inner_loops.len() == 1` (the clean 2-boundary band — the patch tessellator's
  band branch). A HOLED band (a window in the tube → ≥2 inner loops, R0028) is out
  of the patch tessellator's scope (`ploops.len() != 2` → None) and stays the typed
  wall (a later sub-slice). Unit test `torus_poloidal_band_two_encircling_profiles`
  (exact developable-area 2π·R·rm·Δθ + watertight-seam oracle; the RED that caught
  the flat-unroll chording). **Assay: R0059/R0026/R0051 advance curved-profile →
  their next real wall (M8 coplanar Stage 0 / revolve), R0028 stays curved-profile
  (holed band). No CORRECT regressed; curved-profile UNSUPPORTED −3.**
- **Slice F-2 (later) — HOLED torus band** (a window bitten into the tube:
  R0028) needs the patch tessellator's band branch generalized to accept holes,
  and the **cone 2-encircling periodic frustum band** (Slice E sub-slice).
- **Slice F-3 — torus DISK patch. ✅ DONE (2026-09-04).** R0032's
  `FaceId(593)`: a torus lateral bounded by ONE 57-chord `Line` polyline (the
  previous boolean's torus∩cone curve, degree 8 — no analytic curve type), no
  inner loop. Three pieces, no new tessellation machinery:
  1. **Dispatch.** yang `torus_face_takes_patch_path` (single source): inner
     loops (a band) OR an outer loop with no closed profile circle and no
     closed equator (a disk) → `tessellate_torus_band` →
     `tessellate_torus_patch`'s EXISTING 0-wrapping DISK branch (the unwrap
     keeps both branch cuts away from the loop; the same quantized map-back).
     kernel-v2 `to_yang_brep` routes a torus lateral through iff
     `!curved_full_rim` (the "needs a wrapping inner profile" requirement and
     its reason string are gone).
  2. **Region check (P10).** The DISK branch fills the (u, v) polygon's
     INTERIOR, which is the face only when the loop is material-left about
     the face's outward normal — CW in this chart
     (`∂P/∂u × ∂P/∂v = −(R + r·cos u)·r·n̂_out`, the band's 2026-09-03 side
     rule), CCW for a `reversed` face. The other sense bounds the torus's
     COMPLEMENT: a typed decline (`MalformedTopology` "torus patch UV-CDT
     declined"), not a silently wrong region. Three synthetic fixtures (the
     yang roundtrip and chord-band tests, the kernel-v2 arena test) walked
     their rectangles CCW and were corrected; every KV6d render of a real
     arena loop passes the check unchanged.
  3. **Chord band.** A disk operand carries NO `Curve::Circle` rim, so
     `input_curved_chord_bound` was `None` and Stage 4 STOPped at
     `chord_band_none` ("a conic edge implies a circle-bearing input" — F-3
     broke that premise). New single source
     `torus_chord_bound(R, r) = chord_rel()·(R + r)` — the budget
     `tessellate_torus_band` already handed the UV-CDT (now derived from the
     same function, byte-identical) — folded into the input bound for
     PATCH-path torus faces only (a structured lateral samples at its rims'
     density and keeps the rim band). The sphere / cone precedent
     (I-sphere-band, PR-YR16), not tolerance widening.

  Tests — yang: `torus_disk_patch_lone_chord_loop` (a 48-chord (u, v)
  rectangle whose loop sense comes from a 3D WITNESS — `n̂ × t̂` of the first
  chord toward an interior point — not from the chart; oracles: exact
  developable area `r·Δv·[R·Δu + r·(sin u1 − sin u0)]` inscribed within
  1.5 %, the 48 chords are the only single-count edges, every vertex on the
  tube, every triangle outward), `torus_disk_patch_reversed_face_points_inward`,
  `torus_disk_patch_complement_sense_declines_typed`,
  `torus_patch_faces_carry_their_own_chord_band`. kernel-v2 end-to-end
  `torus_disk_patch_reentry` (`boolean_chains.rs`): a 270° torus (axis +x,
  R = 3, r = 1) minus a box leaves the sliver y ≤ −3.5 — ONE torus face, a
  lone chord loop, no inner loop; volume 1.422026 vs the 2-D quadrature
  1.430552 (−0.60 %, inscribed); then a 0.8 × 0.8 pocket into the disk's
  apex removes 0.170196 by quadrature, measured decrement error 2.5e-4 — the
  pocket's planes cross the disk mesh and Stage 4 relocates onto the
  torus∩plane curves through the disk's own chord band.

  **Assay: R0032 moves `UNSUPPORTED(curved-profile)` → its next real wall,
  Stage-4 torus JUNCTION relocation** (`YANG_LRR_PROBE`: v67
  `triple_newton_none` recorded first, then v7 `gt2_partners` aborts — a
  vertex of the union's arrangement with MORE than two partner surfaces on
  the torus, the `stage4_correct.rs` torus block) — the corner-junction
  family, outside this spec. Class delta: `UNSUPPORTED(curved-profile)`
  3 → 2 (R0044 M5 K11, C0063 apex-cone operand remain).

Land each slice as its own commit. Do NOT bank unwired: Slice A/B may be
internal, but Slice C must WIRE and prove end-to-end before Slice A/B are
considered done (repo lesson: unwired geometry code is adversary-swept and
low-confidence — see memory `n2_stage4_mesh_updating`).

## Invariants / oracles

- **Watertight patch**: every boundary polyline edge is used by exactly one
  output triangle; the patch shares its rim/seam vertices with adjacent faces
  (bijection preserved — the whole point of Stage 1).
- **Orientation**: every triangle's normal agrees with the surface's outward
  (or inward if `reversed`) radial normal within TAU.
- **On-surface**: every Steiner vertex lies on the analytic surface to `d_p`.
- **Manifold**: the resulting solid passes `validate_solid` (2-manifold).
- **End-to-end (Slice C+)**: exact-volume oracle on the chained boolean result
  for at least one synthetic and the un-quarantined corpus cases.

## Failure modes (all loud, typed — P9)

- Boundary edge is `EllipseArc`/`SurfacePair` → `UnsupportedCurvedBoolean`
  (branch 4). Unchanged.
- Branch-cut gap not found (patch covers full 2π with no seam and a hole crosses
  where the seam would be) → `MalformedTopology` naming the face. No silent
  guess.
- CDT fails (self-intersecting unrolled boundary) → propagate the cherchi CDT
  error, named by face. No tolerance widening.

## Non-goals

- Degree-4 (`EllipseArc`/`SurfacePair`) curved boundaries — separate milestone.
- Multi-shell operands — separate `UnsupportedMultiShellBoolean` wall.
- Coplanar Stage-0 — separate M8 milestone.


## Re-census 2026-09-04 — the three remaining `UNSUPPORTED(curved-profile)` walls

Instrument: `KV14_REENTRY_CENSUS=1` (kernel-v2 `adapter.rs`, the
`UnsupportedCurvedBoolean` wrapper) prints the refusing face's surface, every
loop's half-edge curve pattern with endpoints, and each edge's twin face.
After the corner-transit inc-3c flip (R0044's design boolean completes) the
class holds exactly three cases, and they are three DIFFERENT walls:

| case | refusing face | what the B-Rep carries | the wall |
|---|---|---|---|
| R0044 | `FaceId(458)`, op `boolean_subtract` (the circle cut on the design result) | a CYLINDER lateral (r = 2327.8), no inner loop, a 5-edge outer loop `[Arc, SurfacePair, SurfacePair, SurfacePair, SurfacePair]` — every `SurfacePair` is this cylinder × a CONE (three distinct cones, half-angles 1.011 / 1.048 / 0.440: the revolve's conical flanks) | **M5 K11**: no yang INPUT vocabulary for a degree-4 surface-pair edge. The re-entry needs the procedural (Option B) cylinder∩cone curve sampled onto the cylinder's (θ, z) chart as a shared boundary chain (twin-identical, conformal to the neighbouring cone's own chain), then the Slice-D CDT; downstream, the chain is a CARRIED input curve for §4.4.2 restoration / §4.5.1 relocation onto the procedural curve. A multi-session capability (roadmap M5), not a slice of this spec. |
| R0032 | `FaceId(593)`, op `boolean_union` | a TORUS lateral (R = 45.6, r = 30.4), no inner loop, a 57-edge outer loop of `Line` segments — the previous boolean's torus∩(other) intersection left as its chord polyline (no analytic curve type for a degree-8 curve) — every edge twinned to the neighbouring result faces | **Slice F-3, a torus DISK patch**: one non-wrapping loop. The Slice-F band path needs a second wrapping profile; a single loop needs the double-periodic chart's branch cuts placed away from the loop (the loop's (u, v) image must not straddle either cut), then the existing torus UV-CDT with an empty hole set and the band's lift. Tractable — the next slice of THIS spec. **DONE 2026-09-04** (Slice F-3 above): the Stage-1 wall falls; R0032 now STOPs in Stage-4 torus junction relocation (`gt2_partners` v7). |
| C0063 | `FaceId(1)`, op `boolean_subtract` — the FIRST boolean's operand, not a re-entry | an apex CONE lateral (apex (0, 0, 1.2), half-angle 0.588), no inner loop, a ONE-edge outer loop: the full base rim `Circle(r = 0.8)` twinned to the base cap `FaceId(0)`; the apex is a surface point, not a vertex | **an apex cone as a boolean OPERAND**: the 1-edge loop is routed to the CDT block (`outer_hes.len() != 4`) where a full rim on a cone is the typed wall; the "apex-fan (1 rim)" vocabulary the comment names is not built. Also an AUTHORING defect, see below. **DONE 2026-09-04** ("Apex-cone operand" below): operand arm + apex-cap output vocabulary; document re-authored; C0063 → SUPPORTED_CORRECT. |

**C0063 is also mis-authored.** By exact membership the chain reads EMPTY at
128 and 256 cells on two phases (the cone alone reads 0.8045 = π·0.8²·1.2/3):
the engine measures the cone's single B-Rep vertex (`FE_CUT_TRACE`: `target
verts=1 proj=[0.24, 0.24] sketch_proj=1.199 reverse=true`), so the 2 × 2 × 1.5
oblique slab is laid from its plane back through the whole cone — the cone
spans 0.24…1.145 along the slab normal, the slab −0.30…1.20, and its 2 × 2
footprint covers the cone's (−0.62…0.91) × (−0.8…0.8). The authored
"conic-bounded patch, χ = 2, volume decrease" is not what this document
computes under the engine's own reversal rule: the answer is the empty solid.
Once the apex-cone operand wall falls the runner will see an all-consumed
model; the meta must be re-authored then (a slab that bites the cone, or an
`expect_rebuild_error` per the `cut_consumes_body` precedent) — recorded, not
changed here.

## Apex-cone OPERAND — ✅ DONE (2026-09-04; C0063)

Two sides, both needed:

- **Input (kernel-v2 `to_yang_brep`).** A cone face with ONE closed-`Circle`
  loop and no inner loop (the on-axis apex-triangle full-turn revolve:
  `kv6a_revolve::on_axis_triangle_full_turn_builds_solid_cone` — 1 vertex,
  1 edge, 2 faces) converts to yang's PR-YR16 fixture shape: the rim shared
  with the disc cap through `convert_lateral_edge` (cap-outward normal), plus
  a MINTED edge-less apex `BRepVertex` (position-deduplicated), because
  yang's `[rim_e]` apex-FAN arm locates the apex by position among the
  pre-seeded vertices. `map_vertex` now takes its id from the yang vertex
  pool's length (the map's length no longer equals it).
- **Output (kernel-v2 validate + render).** The tip an oblique slab leaves is
  an **APEX CAP**: a cone face bounded by ONE loop that wraps the axis, the
  apex a singular interior point (its cavity twin is a conical pocket,
  `reversed`). `validate/faces/cone.rs` gains the `1`-wrapping-loop arm: the
  band whose lower (+1) loop collapsed onto the apex, so the survivor winds
  −1 (material toward the apex), lies strictly ahead of it (mean τ > 0), and
  windows wind CW. `tessellate_cone_patch` first asks `apex_cap_polyline`
  for the certificate — net winding ±1 with a MONOTONE azimuth walk (every
  generator through the apex crosses a planar section exactly once, so the
  face is star-shaped from the apex in its development) — and fans the
  sampled boundary from the apex (`tessellate_cone_apex_cap`): exact along
  both rulings, its only deviation the boundary chord's own sagitta, the
  bound the structured apex form's fan carries. Surface-pair boundary
  pieces sample through `surface_pair_edge_samples` like every other
  patch. Anything that is not a clean certificate — no wrap, more than one,
  a once-wrapping loop that doubles back in azimuth (a non-planar bite), a
  vertex on the axis — falls through to the developable path, which keeps
  its own verdict (byte-identical to before; the corpus proved a first
  version that REFUSED surface-pair edges typed had moved three cases:
  R0044, C0052-class CORRECT cases — the certificate must never fail a face
  the developable path handled).
- **Tests** (`boolean_chains.rs`): `apex_cone_operand_oblique_slab_keeps_tip`
  (0.050412 vs quadrature 0.050448), `…_removes_tip` (0.753198 vs 0.753799;
  the truncated body's cone BAND between the rim and the ellipse),
  `apex_cone_cavity_tilted_pocket` (box − a 20°-tilted cone with its apex
  inside: 3.923247 vs 3.923206 — the reversed apex cap).
- **C0063 re-authored.** Its slab plane (n·p = 1.199) lay beyond the apex
  (1.145), so the engine's first-vertex reversal laid the slab back through
  the whole cone (exact chain EMPTY). The plane now passes through the cone
  (origin (0, 0, 0.7338), n·p = 0.7, between the rim at ±0.24 and the apex);
  the rim seam vertex is the body's only B-Rep vertex and lies below the
  plane, so the reversal removes the BASE side and keeps the tip — an
  oblique cone over the elliptical section, the conic-bounded patch the
  case was designed for. `single_case`: SUPPORTED_CORRECT (0.4 s).
- **Corpus (release, 8 jobs, 360 s): 274C / 0W / 31E / 4EE / 0T** — one row
  moved, C0063; `UNSUPPORTED(curved-profile)` = R0044 alone (M5 K11).
