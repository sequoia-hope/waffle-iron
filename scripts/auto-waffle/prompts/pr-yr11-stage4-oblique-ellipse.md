# PR-YR11 (Stage 4, oblique) — yang-rs: relocate onto the exact ELLIPSE (oblique cylinder ∪ box)

Context: Stage 4 (PR-YR10) relocates mesh intersection crossings onto the exact
**circle** for the perpendicular cap of `cylinder ∪ box`, with reversed-point
correction (§4.5.3), watertightness inherited, and a no-skip audit. It currently
**loudly STOPs on oblique cuts**: an `Ellipse` intersection edge →
`Err(Stage4RegionInvalid{EllipseProjectionUnsupported})`. This PR lifts that STOP
for the oblique `cylinder ∪ box` case, where a box face cuts the cylinder at an
angle and `plane ∩ cylinder` is an **ellipse** (ssi-rs PR-SSI2 already solves it;
P3 already emits `Curve::Ellipse`).

## The crux — relocate via the cylinder parameterization (stays closed-form)

Do NOT compute the ambient "nearest point on the 3D ellipse" — that needs a
quartic / iteration and is not exact. Instead relocate using the **cylinder's
own parameterization**, exactly as Yang §4.3.2 uses the surface parameterization:
a crossing point must end up on BOTH the cylinder (radius `r` about its axis) AND
the cutting plane. Closed-form:
1. Snap the point to the cylinder: keep its **angle θ** about the axis, set its
   radial distance to `r` (project to the lateral surface).
2. Snap the **axial coordinate** so the point also satisfies the cutting plane's
   equation `n·x + d = 0` (solve the linear plane equation along the axis at the
   fixed angle θ).
The result lies on `cylinder ∩ plane` = the exact ellipse, to machine precision,
with no quartic. (For an oblique plane each θ gives a unique axial value; the
axis-parallel/degenerate-to-lines case is out of scope — stays whatever it is.)

Keep everything else from PR-YR10: **§4.5.3 reversed-point correction** (now the
ordering invariant is along the ellipse), **watertightness inherited** from the
mesh boolean + the combinatorial `check_watertight_2manifold` gate, the
**no-skip `processed`-set audit**, and the `TessellationMap` update
(`BRepEdge { edge, t }` on the exact ellipse).

First **confirm P3 emits `Curve::Ellipse`** for the oblique cylinder∪box; if a
minimal P3 gap exists for the oblique case, fix it minimally and note it.

## Hard scope
- Oblique `cylinder ∪ box` (plane∩cylinder → ellipse) only. Circle/perpendicular
  path (PR-YR10) must stay byte-for-byte. Sphere/Cone still reject loudly.
- `§4.5.2` local refinement stays a loud `Err(LocalRefinementRequired)` STOP (not
  in scope). Axis-parallel/degenerate-line sections out of scope.
- No global CDT, no Newton/iterative projection, never skip an edge.

## Oracle (RED contract)
1. **Relocated points on the exact ellipse to `TAU`**: independently assert each
   relocated crossing has cylinder radial residual `|dist(x,axis) − r| ≤ TAU_MODEL`
   AND plane residual `|n·x + d| ≤ TAU_MODEL` (on BOTH surfaces ⇒ on the exact
   ellipse — stronger and simpler than checking the ambient ellipse equation).
2. **Oblique cylinder∪box now SUCCEEDS** (the PR-YR10 `EllipseProjectionUnsupported`
   STOP is gone for this case) and produces `Curve::Ellipse` intersection edges.
3. **Chord deviation strictly decreases** vs the pre-Stage-4 polyline.
4. **Watertight 2-manifold** (0 unpaired half-edges, Euler χ=2); **no reversed/
   inverted/degenerate triangles** (winding vs analytic normal; loop order matches
   the ellipse tangent).
5. **Bijection round-trips**; **determinism**; **circle/perpendicular case
   (PR-YR10) unregressed**; **planar `fuzz_boxes` unregressed**; scope held.
   Sidecar-independent direct path for the GREEN gate; env-gate the sidecar E2E
   with a LOUD skip.

## CI gate (FULL crate)
`cargo test -p yang-rs`, `cargo fmt -p yang-rs -- --check`, `cargo clippy -p
yang-rs --all-targets -- -D warnings`, all clean.

On completion: update `docs/yang_functional_roadmap.md` — PR-YR11 (Stage 4
oblique: relocate onto the exact ellipse via the cylinder parameterization;
oblique cylinder∪box now conforms). Note remaining: §4.5.2 local refinement,
sphere (P2b), curved Subtract, broader pairs.
