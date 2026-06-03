# PR-YR21 — yang-rs Stage 4: cone-section relocation foundation + cone∩plane ELLIPSE

Context: after PR-YR18/19/20 the curved fuzz is `ok_correct = 61/90`, with
**cone `0/26`** — blocked across ALL non-perpendicular sections. The analytic
math is DONE in `ssi-rs` (`plane_cone` returns Circle / Ellipse / Parabola /
Hyperbola); this is purely `yang-rs` integration. The roadmap cone-conic
sequence (PR-YR21→YR24, see `docs/yang_functional_roadmap.md`) starts here.

**The keystone gap this PR fixes — Stage-4 relocation for cone sections.** The
existing Stage-4 ellipse relocation (`stage4_relocate_and_correct`, the
`Curve::Ellipse` arm, lib.rs ~3583-3620) identifies the section's owning surface
from the edge incidence and requires it to be a **`Surface::Cylinder` + a
`Surface::Plane`** (the YR11 cylinder parameterization). A **cone**+plane ellipse
edge therefore hits the `(Some, Some)` else branch → `LocalRefinementRequired`
(lib.rs ~3616). So cone ELLIPSE cuts fail today even though `Curve::Ellipse`
already exists and `ssi_rs` returns the correct ellipse — the only thing missing
is a relocation method for a cone section.

## Step 0 — instrumented cone-refusal split (confirm payoff, then revert)

Before building, env-gate a temporary diagnostic (mirror the driver's prior
`YANG_DIAG` approach; **reverted before commit**) that, for the curved fuzz at a
reduced `N_CASES` with the sidecar, splits the cone refusals by section type:
ellipse (C2) / parabola / hyperbola / axis-parallel-or-through-apex. Report the
per-type counts in the cycle log + the roadmap entry. This confirms YR21's payoff
(the cone-ellipse share) and sizes YR22/YR23. Do NOT leave the diagnostic in.

## The design — cone-section (generator-angle) relocation

Build `project_onto_cone_section(p, cone, plane) -> Result<(Point3, …),
Stage4InvalidReason>` — the cone analog of YR11's `project_onto_ellipse_via_cylinder`
(Yang §4.3.2 / Patrikalakis Ch.5 cone parameterization). It is **type-agnostic**
(the SAME relocation serves ellipse, and later parabola/hyperbola), avoiding the
generic foot-of-perpendicular quartic:

1. Cone: `apex`, unit axis `â`, half-angle `α`. Plane: unit `n`, offset `d`
   (`n·x + d = 0`).
2. For mesh vertex `p`: pick the **nappe** from `sign((p − apex)·â)`; compute the
   in-plane radial direction of `p` about the axis and its angle `θ` in an
   orthonormal in-plane basis `(û, ŵ)` ⟂ `â` (the same basis convention the cone
   tessellation / `plane_cone` use — reuse, do not reinvent, to stay
   frame-consistent for the eval round-trip).
3. The generator at `θ` on that nappe: `g(θ) = ±cosα·â + sinα·(cosθ·û + sinθ·ŵ)`.
   The relocated conic point is the intersection of the ray `apex + s·g(θ)` with
   the cutting plane: `s = −(n·apex + d) / (n·g(θ))`, point `= apex + s·g(θ)`.
4. **Guards (loud `Stage4InvalidReason`, P9/P10 — never a silent snap):**
   `|n·g(θ)| < MIN_FEATURE_SIZE` ⇒ generator parallel to the plane (the
   asymptotic / parabola-tail direction; the section runs to infinity here) →
   `LocalRefinementRequired` (out of scope for the relocation). Apex-coincident /
   `s ≤ 0` wrong-nappe ⇒ likewise loud. Reuse `MIN_FEATURE_SIZE`, introduce no
   new tolerance.
5. Residual: distance from `p` to the relocated point, gated by the cone's
   Stage-1 chord band `d_ε` (`cone_chord_bound`, the SAME single-source bound) —
   identical discipline to the circle/ellipse relocation guards (and consistent
   with PR-YR19: if the in-plane vs surface-normal metric mismatch bites here,
   apply the SAME propagated-band reasoning, justified, not widened).

## Wire it into Stage 4 + eval round-trip

- In the `Curve::Ellipse` relocation arm, when the incidence owner is a
  **`Surface::Cone` + `Surface::Plane`** (scan `inc0` exactly as the existing
  cylinder scan does), relocate via `project_onto_cone_section` instead of
  rejecting. The **cylinder**+plane path (YR11) stays **byte-for-byte unchanged**.
- The relocated vertex is tagged `BRepEdge { edge, t }` for tessellation; `t` must
  be the stored `Curve::Ellipse`'s parameter such that `ellipse_point(...t)`
  round-trips to the relocated point (invert the ellipse frame for the relocated
  3D point, exactly as today). `eval_source`'s `Curve::Ellipse` arm is unchanged.

## Scope

- **Cone∩plane ELLIPSE only** (the `Curve::Ellipse` type already exists). The
  `Curve::Parabola`/`Hyperbola` variants are PR-YR22/YR23 and are OUT of scope —
  those cone cuts stay LOUD (`AmbiguousCurve`), which is correct for this PR.
- **Do NOT regress:** cylinder ellipse (YR11) must stay byte-identical (its
  cylinder-parameterization path is untouched); cone perpendicular circle (YR17),
  all planar, and YR8–YR20 demos unchanged. Genuinely-degenerate cone sections
  (axis-parallel, through-apex) stay LOUD.

## RED contract

A deterministic fixture (NO `rand`, NO system time, NO FS side effects): a cone +
a cutting plane oriented so the section is a proper **ellipse** (both
symmetry-plane generators pierce the same nappe — `plane_cone`'s C2 case),
currently failing with `Stage4RegionInvalid { LocalRefinementRequired }`. The RED
test asserts the correct post-fix behaviour: the boolean returns `Ok` with a
`Curve::Ellipse` edge; every relocated intersection vertex lies on the exact
ellipse (and on both the cone and the plane) to `TAU_MODEL`; watertight
2-manifold and `χ = 2−2g` hold. Include a fixture for an **axis-parallel /
asymptotic** cone section that MUST still STOP loudly (the relocation guard).
RED author ≠ GREEN author ≠ Adversary.

## CI gate (FULL crate)

`cargo test -p yang-rs` (whole crate, all prior tests unregressed — especially
the YR11 cylinder-ellipse tests byte-for-byte; the curved fuzz, run with the
sidecar, must show **cone ellipse cuts → `ok_correct`** and cone-ellipse
`LocalRefinementRequired` → 0, with **ZERO new silent-wrong**), `cargo fmt
-p yang-rs -- --check`, `cargo clippy -p yang-rs --all-targets -- -D warnings`.

Sidecar binaries (the worker may be unable to run the sidecar fuzz to
completion — `curved_fuzz_sidecar_zombie_blocker`; if so say so honestly and do
NOT fabricate numbers, the driver reproduces the delta):
`CHERCHI2022_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`
`CHERCHI2022_INPUTCHECK_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans_inputcheck`

On completion: update `docs/yang_functional_roadmap.md` (record PR-YR21 — the
cone-refusal split counts from Step 0, the cone-section relocation, the cone
ellipse result; mark YR22/YR23 as next) and `docs/yang_deviations.md` if the
relocation introduces a deviation. Parabola/Hyperbola remain the deferred YR22/23
follow-ups.
