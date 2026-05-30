# PR-YR7 (P2a) — yang-rs curved Stage-1 tessellation: CYLINDER only, no boolean

Context: PR-YR6 added curved `Surface`/`Curve` variants and made the pipeline
reject curved faces *loudly* (`YangError::CurvedSurfaceNotYetSupported`). This PR
is **P2a**, the first curved-geometry *processing* step and the **highest-risk
single cycle in the boolean effort** (roadmap M5 / Phase 2). Scope is deliberately
narrow: make Stage 1 tessellate a **closed solid cylinder** B-Rep into a
watertight, error-bounded triangle mesh with a correct `TessellationMap`, and
resolve cylinder faces by point-to-surface distance.

**Hard scope limits (do NOT exceed):**
- **Cylinder only.** `Surface::Sphere` and `Surface::Cone` MUST still return
  `YangError::CurvedSurfaceNotYetSupported` (sphere is P2b; cone later). Do not
  tessellate them.
- **No boolean, no `ssi-rs` call, no exact intersection curves.** This PR
  produces a *mesh + TessellationMap* for a single cylinder solid and verifies it.
  Wiring into `boolean()` end-to-end is P2c; exact SSI curves are P3. Do not
  import or call `ssi-rs`.
- **No NURBS, no non-convex profiles, no Steiner-point machinery beyond what a
  cylinder needs.**

Read `crates/yang-rs/CLAUDE.md` (scope rules; Stage development order item 3),
`refs/text/yang2025_hybrid_boolean.txt` §4.1 (lines ~286–407 — error-bounded
triangulation `d_ε`, per-vertex `(u,v)` bijection, §4.1.2 "discretize each patch
independently, re-sample boundary curves, reconstruct around boundaries" — the
watertightness mechanism), and `crates/yang-rs/src/lib.rs`: `BRep::new` Stage-1
tessellation (~line 360; note it currently assumes **mesh verts = B-Rep verts
1:1, no Steiner points** at line 404 — that assumption is what you generalize),
the `TessellationSource` enum (`BRepVertex` / `BRepEdge { edge, t }` /
`BRepFace { face, u, v }`), the planar Newell winding (~line 434), and the
face-resolution point-to-plane distance (~line 868 and ~line 1016).

## What to build

1. **Cylinder B-Rep input encoding.** A closed solid cylinder = 2 planar disk
   caps (`Surface::Plane`) + 1 lateral (`Surface::Cylinder`), bounded by 2 circular
   edges (`Curve::Circle`). The lateral face is topologically a tube (two boundary
   circles), which the current single-`outer_loop` `BRepFace` does not express.
   **You decide the minimal encoding** — either a B-Rep seam edge (a
   `LineSegment` joining the rims, making the lateral a topological disk with one
   loop that traverses the seam twice — the standard CAD representation), or
   extend `BRepFace` to carry the lateral's two boundary loops. Justify the choice
   in the spec. Provide a constructor/test-fixture helper that builds a cylinder
   solid (axis, radius, height) as a `BRep`.

2. **Curved Stage-1 tessellation for the cylinder.** Generalize Stage 1 so a
   cylinder face emits **sampled** mesh vertices (the 1:1 assumption no longer
   holds):
   - Lateral: `u` = angle ∈ [0, 2π), `v` = axial. Choose `N` angular segments so
     the chord error `r·(1 − cos(π/N)) ≤ d_ε`. A cylinder is ruled along the axis,
     so 2 axial rings suffice. `d_ε = 1e-2 × (AABB diagonal)` (paper §4.1.1).
   - Caps: triangulate each disk (fan), **reusing the exact rim-ring vertices the
     lateral generated** — generate each rim ring ONCE and have both the lateral
     and the adjacent cap index those vertices. This shared sampling is the
     watertightness mechanism (§4.1.2); cracks here make Cherchi `inputcheck` hang
     (roadmap M1) — this is the failure mode the oracle must catch.
   - Winding: orient each curved triangle by the **analytic surface normal** at
     its `(u,v)` (governance A15.5), not the planar Newell path (which is for
     `Plane` faces).

3. **The bijection (`TessellationMap`).** Every emitted vertex records its source:
   rim vertices → `BRepEdge { edge, t = angle }` (shared lateral+cap), lateral
   interior → `BRepFace { face, u, v }`, cap interior/center →
   `BRepFace`/`BRepVertex`. This is the load-bearing output.

4. **Point-to-surface face resolution.** Where face resolution currently rejects
   `Surface::Cylinder` (~line 868 / ~line 1016), add the cylinder signed distance
   `dist(x, axis_line) − radius`. Leave sphere/cone rejecting loudly.

## Oracle (RED contract — this is where the rigor lives; all four are hard)

A wrong-but-plausible curved mesh MUST fail. Author RED tests asserting, for a
tessellated cylinder (try a few radii/heights/axes incl. an off-axis, non-unit
axis_dir):
1. **Surface-to-mesh distance ≤ `d_ε`**: sample points across every triangle;
   max distance from the analytic cylinder surface ≤ `d_ε`.
2. **Watertight + 2-manifold**: every mesh edge is shared by **exactly two**
   triangles (no boundary edge, no edge in >2 tris). If the `mesh_booleans`
   sidecar / `inputcheck` is available in the harness, assert it passes; otherwise
   assert the exact-2-manifold property directly and note the sidecar check is
   environment-gated.
3. **Bijection round-trip**: for every vertex, evaluating its `TessellationSource`
   on the source surface (`eval(face, u, v)` / `eval(edge, t)`) reproduces the
   vertex position within tolerance. (A wrong `(u,v)` is caught HERE.)
4. **Euler**: `V − E + F = 2` for the closed cylinder mesh (genus 0).

Also assert: sphere/cone faces still → `CurvedSurfaceNotYetSupported`; the
existing **planar** box-boolean tests are byte-for-byte unchanged.

## CI gate (the test step is the FULL crate suite)
`cargo test -p yang-rs` (whole crate — a Stage-1 change can regress the planar
path; do NOT scope to the new file), `cargo fmt -p yang-rs -- --check`,
`cargo clippy -p yang-rs --all-targets -- -D warnings`, all clean.

If the watertightness / shared-rim design turns out to need a `BRepFace`
two-loop change that ripples further than expected, or the oracle can't be made
to pass honestly, **STOP and report** (P9/P10) — do not weaken the oracle or
fake watertightness with snapping hacks. A partial, honest result is the correct
outcome for the riskiest cycle.

On completion: update `docs/yang_functional_roadmap.md` — record PR-YR7/P2a
(cylinder curved tessellation + bijection + oracle) done; note next is P2b
(sphere, pole handling) then P2c (first curved boolean end-to-end,
mesh-approximate) then P3 (Stage-3 ssi-rs wiring). No ssi-rs work yet.
