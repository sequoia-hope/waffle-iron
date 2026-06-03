# PR-YR16 — yang-rs curved Stage-1 tessellation: CONE only, no boolean

Context: cylinder (PR-YR7) and sphere (PR-YR12) tessellate watertight + bijective.
The **cone** is the last unsupported curved primitive (it rejects everywhere
today). This PR adds cone Stage-1 tessellation + cone point-to-surface face
resolution, verified by the same 4-part oracle. Mirror PR-YR7/PR-YR12.

**Hard scope limits:** **Cone only.** No boolean, no `ssi-rs`, no NURBS. Cylinder
+ sphere + plane paths byte-for-byte. Do not import `ssi-rs`.

Read `crates/yang-rs/CLAUDE.md`, `refs/text/yang2025_hybrid_boolean.txt` §4.1, and
in `crates/yang-rs/src/lib.rs`: the cylinder + sphere tessellation paths, their
`eval_source` arms, `signed_distance_to_surface`, `sphere_chord_bound` /
`curved_chord_bound` (the `d_ε` helpers), and the `Surface::Cone { apex, axis_dir,
half_angle }` variant.

## What to build (cone is the SIMPLEST curved primitive — it is RULED)
1. **Cone B-Rep encoding.** A finite solid cone = lateral (`Surface::Cone`) + base
   cap (`Plane`) + an apex vertex + a base `Circle` rim. The lateral is
   topologically a disk (one boundary = the base circle; the apex is a single
   singular point). **You decide the minimal encoding** (apex vertex + one base-rim
   seam vertex, base `Curve::Circle` edge shared by lateral + cap) and justify it.
   Provide a `cone_brep(apex, axis_dir, half_angle, height)` fixture helper.
2. **Tessellation — apex fan + base cap fan.** A cone is **ruled** (straight
   generators apex→base), so it is EXACT along the axial direction — only the
   **angular** chord error matters. Sample `N` angular segments so the base-circle
   chord ≤ `d_ε = 1e-2 × cone-AABB-diagonal` (base radius `R = height·tanα`; AABB
   diag `√((2R)² + height²)`) — add a `cone_chord_bound` helper mirroring
   `sphere_chord_bound`. The lateral = a **fan of N triangles** apex→base-rim (the
   apex is a singularity, collapsed to one vertex — the sphere-pole analog). The
   base cap = a fan sharing the SAME base-rim vertices (watertightness, §4.1.2).
3. **Bijection.** Lateral interior/rim → `BRepFace { face, u=angle, v=height h }`;
   base-rim → `BRepEdge { edge, t=angle }`; apex → `BRepVertex`. `eval_source`
   gains a `Surface::Cone` arm: `apex + h·â + h·tanα·(cos u·ê₁ + sin u·ê₂)`.
4. **Cone point-to-surface face resolution:** the radial residual
   `| |(x−apex)_⊥| − |h|·tanα |` (h = (x−apex)·â). Replace the cone rejection.
5. **Winding — the ONE cone subtlety:** the cone lateral's outward **normal is
   TILTED** (perpendicular to the generator line, NOT purely radial like the
   cylinder; the `Surface` doc's "radially away from axis" is the *side*, not the
   exact normal). Orient each triangle by the **true tilted cone normal** at its
   centroid (A15.5). The surface-to-mesh oracle is the safety net.

## Oracle (RED contract — all four hard, mirroring P2a/P2b)
1. **Surface-to-mesh ≤ `d_ε`**: max distance from the analytic cone over every
   triangle ≤ `d_ε`.
2. **Watertight + 2-manifold**: every edge in **exactly two** triangles (apex fan
   + base close cleanly). Env-gated `inputcheck` if available, else direct.
3. **Bijection round-trip**: every vertex's source evals back to its position
   within `TAU_MODEL` — include the apex, base rim, and base center.
4. **Euler**: `V − E + F = 2` (closed solid cone, genus 0).
Also: existing **planar + cylinder + sphere** tests byte-for-byte unchanged.

## CI gate (FULL crate)
`cargo test -p yang-rs` (whole crate — a Stage-1 change can regress sibling
curved/planar paths), `cargo fmt -p yang-rs -- --check`, `cargo clippy -p yang-rs
--all-targets -- -D warnings`, all clean.

If apex/base watertightness or the tilted-normal winding can't pass the oracle
honestly, **STOP and report** (P9/P10) — no faked watertightness, no `d_ε`
widening.

On completion: update `docs/yang_functional_roadmap.md` (PR-YR16 — cone Stage-1
tessellation; all three curved primitives now tessellate). Next: PR-YR17 cone
cavity (`box − cone` conical pocket).
