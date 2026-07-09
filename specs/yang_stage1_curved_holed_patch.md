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
- **Slice E — cone unroll.** Slant-radius-varying `u` scale. Targets R0020,
  R0093, C0063 (census correction: these are cones, not cylinders) + the
  non-cylinder holed cone cases (R0026, R0051 …).
- **Slice F — torus unroll.** Two wrapping params; targets R0028, R0059.

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
