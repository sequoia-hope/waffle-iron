# PR-YR12 (P2b) — yang-rs curved Stage-1 tessellation: SPHERE only, no boolean

Context: PR-YR7/P2a gave `yang-rs` a watertight, `d_ε`-bounded, bijective
**cylinder** tessellation + point-to-surface cylinder face resolution, verified by
a 4-part oracle. This PR is **P2b**: the same for a **sphere** — the remaining
curved Stage-1 primitive. Scope is deliberately narrow and mirrors P2a.

**Hard scope limits (do NOT exceed):**
- **Sphere only.** `Surface::Cone` MUST still return
  `YangError::CurvedSurfaceNotYetSupported` (cone is later). Cylinder + plane
  paths unchanged.
- **No boolean, no `ssi-rs` call, no exact intersection curves.** This PR produces
  a *mesh + TessellationMap* for a single sphere solid and verifies it. Do not
  import or call `ssi-rs`.
- **No NURBS, no Steiner machinery beyond what a lat-long sphere needs.**

Read `crates/yang-rs/CLAUDE.md`, `refs/text/yang2025_hybrid_boolean.txt` §4.1
(error-bounded triangulation `d_ε`, per-vertex `(u,v)` bijection, §4.1.2
watertightness), and in `crates/yang-rs/src/lib.rs`: the PR-YR7 **cylinder**
tessellation + `eval_source` Cylinder/Circle arms + `signed_distance_to_surface`
(the templates to mirror), the `Surface::Sphere { center, radius }` variant
(PR-YR6), and `curved_chord_bound` (the shared `d_ε = 1e-2 × analytic-AABB-diag`
helper).

## What to build (mirror P2a)

1. **Sphere B-Rep input encoding.** A full sphere is one closed `Surface::Sphere`
   face. Topologically it needs a seam to be a disk-with-identifications (the
   cylinder used a `LineSegment` seam joining the rims). **You decide the minimal
   encoding** — e.g. a meridian seam edge + two pole vertices (the standard CAD
   sphere), or a periodic single face — and justify it in the spec. Provide a
   `sphere_brep(center, radius)` test-fixture helper.
2. **Lat-long tessellation with pole handling.** Sample `u` = longitude ∈ [0,2π),
   `v` = latitude ∈ [−π/2, π/2]. Choose ring/segment counts so the chord error
   `≤ d_ε = 1e-2 × (2r)` (the sphere AABB diagonal is `2r`; reuse
   `curved_chord_bound`). **The two poles are singular** — collapse each polar
   ring to a single pole vertex with a triangle FAN (no degenerate quads). The
   seam meridian's two sides must share the SAME sample vertices (watertightness,
   §4.1.2) — generate seam vertices once, index from both sides.
3. **Bijection (`TessellationMap`).** Interior vertices → `BRepFace { face, u, v }`;
   seam vertices → `BRepEdge { edge, t }`; pole vertices → `BRepVertex` (or the
   apt source). `eval_source` gains a `Surface::Sphere` arm: `center + r·(cos v
   cos u, cos v sin u, sin v)` (or your chosen parameterization — be consistent).
4. **Point-to-surface face resolution.** Where face resolution currently rejects
   `Surface::Sphere`, add the sphere signed distance `|x − center| − r`. Leave
   cone rejecting loudly.
5. **Winding:** orient each triangle by the analytic **outward radial normal**
   `(x − center)` at its centroid (governance A15.5), not a planar Newell path.

## Oracle (RED contract — all four hard, mirroring P2a)
1. **Surface-to-mesh distance ≤ `d_ε`**: sample across every triangle; max
   distance from the analytic sphere ≤ `d_ε`.
2. **Watertight + 2-manifold**: every edge shared by **exactly two** triangles
   (poles included — fans must close cleanly). If the sidecar/`inputcheck` is
   available, assert it passes; else assert exact-2-manifold directly + note the
   env gate.
3. **Bijection round-trip**: every vertex's `TessellationSource` evals (closed
   form) back to its position within `TAU_MODEL` (a wrong `(u,v)` is caught here;
   include pole + seam vertices).
4. **Euler**: `V − E + F = 2` for the closed sphere mesh (genus 0).
Also: `Surface::Cone` still → `CurvedSurfaceNotYetSupported`; existing planar +
cylinder tests byte-for-byte unchanged.

## CI gate (FULL crate suite)
`cargo test -p yang-rs` (whole crate — a Stage-1 change can regress the planar/
cylinder paths; do NOT scope to the new file), `cargo fmt -p yang-rs -- --check`,
`cargo clippy -p yang-rs --all-targets -- -D warnings`, all clean.

If the seam / pole watertightness needs a `BRepFace` change that ripples further
than expected, or the oracle can't pass honestly, **STOP and report** (P9/P10) —
no faked watertightness, no `d_ε` widening.

On completion: update `docs/yang_functional_roadmap.md` — record PR-YR12/P2b
(sphere curved tessellation done). Note remaining M5: curved `Subtract`,
side-face/corner loud-STOP guard, broader SSI/general-degree-4. No ssi-rs work.
