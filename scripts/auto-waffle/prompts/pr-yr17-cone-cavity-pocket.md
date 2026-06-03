# PR-YR17 — yang-rs curved Subtract CONE cavity: box − cone conical pocket (genus 0)

Context: the cavity-sense mechanism (`BRepFace.reversed`, PR-YR13) is
**surface-agnostic** (set from `op == Subtract && info.input == B`, lib.rs ~3484,
in reconstruct's curved branch). PR-YR16 makes the cone tessellate + resolve.
`plane ∩ cone → Circle` (perpendicular cut) is in `ssi-rs` (`plane_cone`, SSI3)
and wired into Stage 3 (P3). So this PR composes them into `box − cone`.

## Target
A cone with its **apex inside** the box (the pocket bottom) and its **base above**
the box top, so `box − cone` carves a **conical pocket**. Single shell, **genus 0
(χ = 2)**. Cavity wall = the cone lateral (the part inside the box); rim =
`cone ∩ box-top plane` = exact `Circle`; apex = the pocket-bottom singular vertex.

## What to build (composition; confirm each composes)
1. **Cone solid as input B** via the PR-YR16 `cone_brep`; tessellated by the
   YR16 Stage-1 path.
2. **`box − cone` flows through reconstruct's curved branch**: the surviving
   cone-lateral cavity patch (attributed to B via the YR16 cone face-resolution)
   emits `BRepFace { surface: Surface::Cone { apex, axis_dir, half_angle },
   reversed: true, … }` — `reversed` set by the existing op==Subtract&&B rule, no
   new mechanism. Verify the cavity patch (incl. the apex singularity) reassembles
   without a STOP.
3. **Exact rim**: the `cone ∩ box-top plane` (perpendicular) section is a `Circle`
   (plane∩cone SSI); Stage 3 assigns it, Stage 4 relocates the rim crossings onto
   it (reuse YR9/YR10, op- and surface-agnostic).

## Scope
- **`box − cone` conical pocket only** (apex inside, base above one face,
  perpendicular axis). Deferred: through-cone, oblique cuts (ellipse/parabola/
  hyperbola rims), fully-internal cone void (multi-shell), the cone-base-subtracted
  case. Union + planar + YR13/14/15 cavity paths byte-for-byte.

## Oracle (RED contract)
1. **Cavity wall is `Surface::Cone`** with the input cone's exact `apex`/
   `axis_dir`/`half_angle`, `reversed == true`; box faces `Plane`, `reversed ==
   false`.
2. **Effective outward normal into the pocket**: on the cavity wall, the negated
   tilted cone normal (because `reversed`) points into the removed (pocket) region,
   NOT into the box material. Mesh-winding ↔ `reversed` consistency (mirror the
   YR13/YR15 adversary witness).
3. **Watertight 2-manifold, χ = 2** (genus-0 pocket; 0 unpaired half-edges; the
   apex closes cleanly).
4. **Exact `Circle` rim** on both the cone (radial residual) and the box-top plane
   to `TAU_MODEL`; Stage-4 relocates rim crossings onto it.
5. **Sidecar `Subtract` mesh-parity** (env-gated, LOUD skip); determinism; ALL
   prior tests (Union, planar, YR13/14/15) unregressed. Sidecar-independent direct
   path for the GREEN gate (hand-built attributed cone-cavity → reconstruct →
   assert `Surface::Cone` + `reversed` + sense).

**STOP-and-report (P9/P10)** if the cone-cavity patch won't reassemble (esp. the
apex), the rim won't relocate, or the pocket isn't watertight — do not fake it.

## CI gate (FULL crate)
`cargo test -p yang-rs` (whole crate), `cargo fmt -p yang-rs -- --check`,
`cargo clippy -p yang-rs --all-targets -- -D warnings`, all clean.

On completion: update `docs/yang_functional_roadmap.md` (PR-YR17 — cone cavity,
box − cone conical pocket; curved Subtract now covers cylinder + sphere + cone).
Remaining curved-Subtract: through-cone, oblique cone cuts, fully-internal voids
(multi-shell), the side-face/corner guard.
