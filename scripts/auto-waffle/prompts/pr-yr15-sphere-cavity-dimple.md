# PR-YR15 — yang-rs curved Subtract SPHERE cavity: box − sphere dimple (genus 0)

Context: the cavity-sense mechanism (`BRepFace.reversed`, PR-YR13) is
**surface-agnostic** — it is set from `op == Subtract && info.input == B`
(lib.rs ~3484) in reconstruct's curved-face branch, for ANY curved cavity
surface. Sphere tessellation + sphere point-to-surface resolution shipped in
PR-YR12. `plane ∩ sphere → Circle` is in `ssi-rs` and already wired into Stage 3
(P3/PR-YR9). So this PR is largely **composition**: `box − sphere` carving a
spherical dimple, the cavity wall a `Surface::Sphere` with `reversed = true`, the
rim an exact `Circle`.

## Target
A sphere centred ON a box face (poking through exactly **one** face) so that
`box − sphere` removes the hemisphere inside the box → a **hemispherical dimple**.
Single shell, **genus 0 (χ = 2)**, ONE `Circle` rim (`sphere ∩ box-face plane`),
cavity wall = the inside hemisphere of the sphere.

## What to build (mostly wiring; confirm each composes)
1. **Sphere solid as input B** via the PR-YR12 `sphere_brep` helper; tessellated
   by the YR12 Stage-1 path.
2. **`box − sphere` flows through reconstruct's curved branch**: the surviving
   sphere-cap patch (attributed to B via the YR12 sphere face-resolution) emits a
   `BRepFace { surface: Surface::Sphere { center, radius }, reversed: true, … }`
   — `reversed` set by the existing op==Subtract&&B rule, no new mechanism.
   Verify the cap patch reassembles (boundary cycle = the rim) without a STOP.
3. **Exact rim**: the `sphere ∩ box-face plane` section is a `Circle` (plane∩sphere
   SSI); Stage 3 assigns it, Stage 4 relocates the rim crossings onto it (reuse
   the YR9/YR10 machinery, op- and surface-agnostic).

## Scope
- **`box − sphere` single-face dimple only.** Deferred: a fully-internal spherical
  void (multi-shell — the E3 single-shell path), through-sphere, cone cavities,
  box-subtracted. `Cone` still rejects loudly.
- Union + planar + YR13 cylinder-cavity + YR14 through-hole paths byte-for-byte.

## Oracle (RED contract)
1. **Cavity wall is `Surface::Sphere`** with the input sphere's exact `center`/
   `radius`, `reversed == true`; box faces `Plane`, `reversed == false`.
2. **Effective outward normal toward the centre** (into the dimple): sampled on
   the cap wall, the analytic away-from-centre normal negated (because `reversed`)
   points toward `center`, NOT away.
3. **Watertight 2-manifold, χ = 2** (genus-0 dimple; 0 unpaired half-edges).
4. **Exact `Circle` rim** on both the sphere (`|x−center| = radius`) and the box
   face plane to `TAU_MODEL`; Stage-4 relocates rim crossings onto it.
5. **Sidecar `Subtract` mesh-parity** (env-gated, LOUD skip); determinism; ALL
   prior tests (Union, planar, YR13, YR14) unregressed. Sidecar-independent direct
   path for the GREEN gate (hand-built attributed sphere-cap cavity → reconstruct
   → assert `Surface::Sphere` + `reversed` + sense).

**STOP-and-report (P9/P10)** if the sphere-cap cavity patch won't reassemble, the
rim won't relocate onto the exact circle, or the dimple isn't watertight — do not
fake it.

## CI gate (FULL crate)
`cargo test -p yang-rs` (whole crate), `cargo fmt -p yang-rs -- --check`,
`cargo clippy -p yang-rs --all-targets -- -D warnings`, all clean.

On completion: update `docs/yang_functional_roadmap.md` (PR-YR15 — sphere cavity,
box − sphere dimple). Remaining curved-Subtract: cone cavities, fully-internal
voids (multi-shell), the side-face/corner guard.
