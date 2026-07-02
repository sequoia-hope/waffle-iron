# N2-3a — Stage-0 exact rim placement for overlay vertices (amended) — Spec

Third N2-track increment (parents: `specs/n2_stage4_mesh_updating.md`,
`specs/n2_stage4_dt_recompute.md`; design history:
`specs/yang_n2_stage4_cdt_mesh_updating.md`). **AMENDED 2026-07-02 after the
test-phase grounding (§0 items 4–5): the root cause is one layer upstream of
the original diagnosis** — the fix is Stage-0 mint-time exactness (Yang §4.5.5
"overlap boundaries become intersection curves", i.e. exact curve geometry on
the shared-region boundary), with the originally-specified Stage-4 Fig-11(b)
junction-cluster merge retained only as a CONTINGENT part 2 (§3b).

## 0. Grounding trail (all measured)

1. **The Stage-4 phase-3 STOPs have no live consumer** (probes: 0 hits across
   yang-rs suites, campaign, 194-case assay). The §5c.5 re-mesh wiring stays
   deferred until a consumer exists; `stage4_mesh_update`/`d_of_t` remain its
   ready machinery.
2. R0072 fails kernel-v2's debug-only `VertexOffSurface` tripwire (release
   builds would ship the geometry silently) — the acceptance target of this
   increment.
3. Original diagnosis: a tangency junction cluster (v7/v8/v11, one geometric
   junction) merged onto an off-curve neighbor instead of the relocated point
   q — a real Fig-11(b) violation, but…
4. **Test-phase finding (spec-scope gap):** enumerating ALL off-band loop
   vertices of R0072 shows **12**, of which 11 are NOT the cluster: they sit
   at chord-sagitta positions (5.4e-6..7.3e-6 off the r=2.13e-4 cylinder;
   worst = r·(1−cos(π/13)), the N=13 chord sagitta exactly).
5. **Root cause (traced):** the coplanar overlay's trapezoidal decomposition
   (`coplanar_overlay.rs:394`, cell corners :447-450) splits every rim chord
   at every event x-coordinate; Stage-0's overlay-vertex resolution closure
   (`stage0.rs:418-442`) resolves those points through face-corner / rim-ring
   exact-key / ULP-snap branches, then **falls through to a raw in-plane
   `frame.lift` (stage0.rs:438)** — minting 3D vertices ON the chord,
   sagitta-deep off the exact rim circle. The SAME sweep already projects the
   *opposite* rim's points at exact radius (`opp_radius`, stage0.rs:1048-1055)
   — the asymmetry is the bug surface. Stage-4 can never repair this: the rim
   edges are same-input (A-lateral × A-cap) and the
   `build_intersection_curves` same-input skip (lib.rs:5428) is *semantically
   correct* per §4.5.5 (a solid's own face boundary is not an A×B intersection
   curve; emitting it also collapses membrane triangles, lib.rs:5529-5547).
   The exact circle is one call away at the mint: `disc_circle_edge(a,
   p.face_a)` (stage0.rs:660) yields the rim `Curve::Circle`.

## 1. Goal

Every overlay-derived vertex that lies on a disc-rim chord is minted ON the
exact rim circle at Stage-0 resolution time, for BOTH kinds:

- **pure subdivision points** (x-event splits of a rim chord; the 11): radial
  projection onto the exact circle in the cap plane;
- **rim × other-input-edge crossings** (the overlap-boundary junctions; the
  tangency corner): the **exact 2D circle∩line intersection** (radial
  projection would slide them off the other input's edge/plane — fixture
  invariant I2 pins the exact junction).

Both solids' meshes and the lateral rim overrides consume the SAME `coords`
resolution, so one projection site keeps cap, lateral, opposite rim, and both
meshes identical on the shared region (§4.5.5's identical-mesh requirement).

## 2. Parameters

No new public API, no new tunables. Inputs in hand at the seam (all existing):
the resolution closure's `a`/`p.face_a`/`frame`, `disc_circle_edge` (exact
`Curve::Circle`), the overlay's `exact_verts` (rational 2D positions), and the
existing exact on-rim-chord collinearity + interior-parameter test
(`rim_subdivided` / `collect_rim_crossings`, stage0.rs:1008-1027) that already
identifies exactly these vertices. Classification of "also on another input's
edge" uses the overlay's exact rational data (a crossing point lies on a B
input sub-segment) — exact predicates, no tolerance.

## 3. Branch table (the coords-resolution closure, stage0.rs:418-442)

| Case (checked in order) | Today | After |
|---|---|---|
| Face corner (`corners_a/b` hit) | exact corner | unchanged |
| Rim-ring vertex (exact-key hit) | exact rim sample | unchanged |
| ULP-snap (`rim_pts`, ~1e-13) | snapped | unchanged |
| **NEW: on a rim chord AND on another input's edge (exact rational tests)** | raw `frame.lift` → chord position | exact 2D circle∩line intersection point, lifted to 3D (on circle AND on the other edge) |
| **NEW: on a rim chord only (x-event subdivision)** | raw `frame.lift` → chord position | radial projection onto the exact circle: `center + radius·normalize(lift(q) − center)` in the cap plane |
| Not on any rim chord (straight-edge / interior points) | raw `frame.lift` (exact for straight edges) | unchanged |

### 3b. CONTINGENT part 2 — Stage-4 Fig-11(b) junction-cluster merge

Implement ONLY if, after part 1 is green at Stage-0, the acceptance oracle I1
still fails on a residual cluster (measure; do not build speculatively — P10).
Definition retained from the original spec: for each relocated
LineSegment-curve endpoint q, mesh-adjacent non-endpoint vertices within the
derived band (`LineReloc.band_budget + d_eps`) merge onto q via
`collapse_vertex` (survivor = q), ascending-(q,p) deterministic order;
ambiguous double-ownership → loud `LocalRefinementRequired`.

## 4. Invariants (measurable — unchanged; the committed red tests assert these)

- **I1 (on-surface output):** every output loop vertex attributed to a face
  lies on that face's `Surface` within the kernel import band
  `1e-9·(1+max(r,‖p‖∞))`.
- **I2 (exact junction survives):** the rim∩plane junction vertex at
  `(R−δ, +y*, 0)` remains exact (distance 0.0) — the circle∩line branch, not
  radial projection, must handle crossings.
- **I3 (watertight preserved):** watertight + Euler χ=2 + plausible volume.
- **I4 (locality / no-op):** non-Stage-0 pipelines byte-identical (fixture
  I4 does not traverse Stage-0); straight-edge and interior overlay points
  unchanged.
- **I6 (determinism):** repeat runs byte-identical.

## 5. Oracles (committed at ff3763e7, `crates/yang-rs/tests/n2_junction_cluster.rs` + `crates/test-harness/tests/n2_junction_cluster_campaign.rs`)

- `i1_cylinder_face_loop_vertices_on_surface` — RED today (11 off-band, worst
  6.200744e-6 vs band 1.000213e-9).
- `i2_exact_junction_vertex_survives`, `pins_watertight_euler_volume`,
  `i4_locality_noncoplanar_tangent_all_on_surface`, `i6_determinism` — GREEN
  pins.
- `red_r0072_vertex_off_surface` (`#[ignore]`, campaign replay) — RED today.
- R0096 verdict: DIFFERENT mode (torus×torus v1-scope STOP) — documented, no
  test.
- **Blast-radius regression (from the trace):** R0013/R0024 (now-green
  same-normal), disc∩polygon crossing suites (PR-M8 classes), non-convex
  containment, gear-flange chain, full assay: **0 SUPPORTED_WRONG, no
  SUPPORTED_CORRECT lost.**

## 6. Failure modes

- Circle∩line with no real intersection for a claimed crossing point (exact
  discriminant < 0) → loud `CoplanarOverlayError`/Stage-0 error naming the
  vertex (never fall back to the chord position silently).
- Anything downstream still off-band → the kernel tripwire stays (untouched);
  the campaign test keeps it observable.

## 7. Research basis

- **#24 Yang et al. 2025 §4.5.5** — overlap boundaries become intersection
  curves; the shared trimmed surface carries exact curve geometry and BOTH
  models receive identical meshes (`refs/text/yang2025_hybrid_boolean.txt`,
  §4.5.5). Minting shared-boundary points on the exact circle is that
  requirement; the chord-position mint was the deviation.
- **Precedent in-file:** the opposite-rim exact-radius projection
  (stage0.rs:1048-1055) — the same operation this spec extends to the own-cap
  rim.
- **Fig 11(b)** (part 2, contingent) — merge p with q, the on-curve point
  (`refs/text/yang2025_hybrid_boolean.txt:556-560`).

### 7a. Analytical vs approximate

Exact: radial projection and circle∩line are closed-form on the analytic
`Curve::Circle`; classification uses exact rational predicates. No SSI; A15
N/A.

## 8. Scope / non-goals

- No re-mesh wiring (deferred with cause, §0 item 1).
- No change to `build_intersection_curves` same-input semantics (correct per
  paper), to kernel-v2's tripwire, or to non-Stage-0 paths.
- R0021's re-entry wall and render-triangle blemish: different milestones.
- Part 2 (§3b) only on measured need.
