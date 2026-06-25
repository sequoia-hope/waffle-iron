# Torus Stage-4 SSI relocation — scope

**Status:** Tier B BUILT, 2026-06-25 (commit 2963c430). The implicit-pair Newton
relocation (§4) is implemented and the end-to-end blocker is cleared: a torus
boolean's output boundary now lands on the analytic torus (~0.096 → ~1e-8), so
`from_yang_brep` + `validate_torus_face` accept it
(`kv6d_torus_boolean_recovery::torus_boolean_relocates_boundary_onto_surface_and_reconstructs`,
green). Remaining: the render of a seam-WRAPPING (cylindrical-topology) boolean
patch is the UV-CDT consumer's v1 boundary (simple-patch render is unit-tested);
Tier A analytic-circle edges are optional polish. Original scope below.

The proven blocker for the KV6d torus-boolean end-to-end path
(`docs/kv6d_torus_boolean_scope.md` §5b2). The Stage-5/6 reassembly + kernel-v2
render wiring are DONE and tested (commits b51b67b6, ca0e1699); the only missing
piece was placing the trimmed torus output boundary ON the analytic torus.

---

## 1. The blocker, measured

A boolean that trims a torus produces an output torus face whose boundary is the
intersection curve. yang's Stage-4 (`§4.4.1` mesh updating) RELOCATES every mesh
intersection vertex onto the exact analytic curve so the boundary lands on both
incident surfaces. For a torus this is skipped:

```
crates/yang-rs/src/lib.rs:4334   Surface::Torus { .. } => Err(SsiRefinementError::UnsupportedSurfaceForSsi)
```

So the intersection vertices stay on the INPUT tessellation's chords —
**measured ~0.096 off the analytic torus** for the KV6d fixture
(`kernel-v2/tests/kv6d_torus_boolean_recovery.rs::torus_output_boundary_is_chord_off_surface`,
passing). `validate_torus_face` (`validate.rs:929`) checks every boundary vertex
has `torus_residual ≤ CURVED_SURFACE_DEBUG_TOLERANCE·minor` (1e-12) and
correctly rejects it inside `from_yang_brep`. Loosening that tolerance to ~0.1
would be exactly the masking P9 forbids — the boundary is genuinely off-surface.
The fix is to MOVE the vertices onto the surface, not to widen the gate.

---

## 2. How Stage-4 relocation works today (per-pair, conic, closed-form)

`stage4_relocate_and_correct` (`lib.rs:7000`) walks each intersection edge,
converts both incident surfaces to `ssi_rs::QuadricSurface` (`surface_to_quadric`,
`lib.rs:4304`), calls `ssi_rs::intersect` to get the analytical `SsiCurve`(s),
selects the curve the edge lies on, and projects each endpoint onto it with a
CLOSED-FORM per-curve-type method:

| `SsiCurve` | projector | pairs |
|---|---|---|
| `Circle` | `project_onto_circle` (`lib.rs:3496`) | ⊥-cuts, sphere/cyl sections |
| `Ellipse` | `project_onto_ellipse_via_cylinder` (`lib.rs:3864`) | oblique cyl∩plane |
| `Parabola`/`Hyperbola` | `project_onto_cone_section` (`lib.rs:4009`) | cone∩plane |
| `Line` | perpendicular foot (`lib.rs:7553`) | plane∩plane, cyl rulings |

Every curve is a CONIC with a closed-form projection and a scalar parameter `t`
stored on the output `BRepEdge` for the `eval_source` round-trip. The relocated
vertex lands on BOTH surfaces (the conic lies on both), which is what keeps the
shared edge watertight across the two incident faces.

---

## 3. Why the torus does not fit that machinery

The torus is a degree-4 surface, so its intersections are **not conics**:

| Torus ∩ … | intersection curve | tier |
|---|---|---|
| **Plane ⊥ axis** | 0/1/2 concentric **circles** (latitude) | A (closed-form) |
| **Plane through axis** | 2 **circles** radius `r` (meridian profiles) | A (closed-form) |
| **Plane, general oblique** | **spiric section** (quartic; Cassini-oval / lemniscate family) | B (degree-4) |
| **Cylinder / Cone / Sphere** | degree-4 in general; **circles** only in the coaxial special cases | A coaxial / B general |
| **Torus** | degree-4 in general; circles when coaxial | A coaxial / B general |

Consequences for the three crates as they stand:

- `ssi_rs::QuadricSurface` has **no `Torus` variant** (`ssi-rs/lib.rs:98`,
  "Torus arrives with its solver"); `ssi_rs::intersect` has **no torus arms**.
- `ssi_rs::SsiCurve` has **no degree-4 representation** — every variant is a
  closed-form conic with a rational `eval(t)` (`ssi-rs/lib.rs:135`).
- yang has **no projector** for a degree-4 curve and **no output `Curve`**
  variant for one (`Curve` is Line/Circle/Ellipse/Parabola/Hyperbola,
  `lib.rs:196`).

The KV6d fixture's box cut hits Tier B: the box's `x = 3.4` face is parallel to
the torus axis but OFFSET from it → a spiric section, not circles.

---

## 4. The minimal unblock — Newton projection onto the implicit pair (yang-only)

**Key realization:** Stage-4 only needs to MOVE each shared intersection vertex
onto BOTH surfaces. It does NOT need a parameterized curve. yang already has
every surface's implicit form `F(x)` via `signed_distance_to_surface`
(`lib.rs:3404`, torus arm at 3461: `F = √((ρ−R)²+τ²) − r`), and each `∇F` is
closed-form. So a vertex can be relocated by **Newton on the 2-surface system**:

```
given p, surfaces (S0, S1):
  repeat:
    F  = [F0(p), F1(p)]                      # both ≈0 on the intersection
    J  = [∇F0(p); ∇F1(p)]                    # 2×3
    p -= Jᵀ (J Jᵀ)⁻¹ F                       # least-norm Gauss–Newton step
  until |F0|,|F1| ≤ TAU_MODEL
```

This lands `p` exactly on the intersection curve (transversal case: quadratic
convergence) with NO closed-form curve. It is the degree-4 analog of the conic
projectors and slots into `stage4_relocate_and_correct` as a new branch taken
when either incident surface is a `Torus` (bypassing `surface_to_quadric` /
`ssi_rs::intersect` for those edges).

**What the minimal path does NOT need** (this is the scope-shrinking insight):

- **No `SsiCurve` degree-4 variant** — we project onto implicit surfaces, not a
  parameterized curve.
- **No new output `Curve` type** — the torus-intersection output edges stay
  `LineSegment` chords between on-surface vertices. `validate_torus_face` checks
  vertices (not edges), and `tessellate_torus_patch` already consumes a boundary
  POLYLINE. (The KV6d reconstruction already accepted the polyline structurally;
  only the off-surface VERTEX check failed.)
- **No `ssi-rs` torus solvers** — Tier B is entirely a yang Stage-4 concern.

**Loud-STOP cases (P9, no guessing):**
- **Tangential / near-degenerate** intersection: `∇F0 × ∇F1 ≈ 0` → `J` rank-
  deficient → STOP (`Stage4InvalidReason::LocalRefinementRequired`), do not
  relocate to a wrong root.
- **Junction vertices** on two torus intersection curves (a 3-surface point):
  same class as the existing line+circle junctions (KV6b-F3). Relocate onto the
  3-equation system or loud STOP if ambiguous — do NOT overwrite one with the
  other.
- **Non-convergence** within an iteration cap → STOP, not a partial move.

This Tier-B-only path unblocks torus booleans END TO END: vertices land on the
torus → `validate_torus_face` passes → `from_yang_brep` succeeds →
`tessellate_torus_patch` (already wired + unit-tested) renders the patch. Flip
`kv6d_torus_boolean_recovery::torus_minus_box_reconstructs_and_tessellates` from
`#[ignore]` to green in the same change.

---

## 5. Tier A (optional polish) — analytic circle edges for the special cases

When the intersection IS a circle (⊥-axis plane, through-axis plane, coaxial
cylinder/sphere/torus), emit an analytic `Curve::Circle` output edge instead of
a chord polyline, reusing the existing `project_onto_circle` relocation and its
exact `t` round-trip. This gives a nicer analytic B-Rep (exact edges, smaller
meshes) for the common "slice the donut flat" boolean, but is NOT required for
correctness — Tier B's Newton already lands those vertices on the circle
numerically. Tier A needs:

- `ssi_rs::QuadricSurface::Torus` + the torus solver row (`plane_torus`,
  `cylinder_torus`, `cone_torus`, `sphere_torus`, `torus_torus`) returning
  `SsiCurve::Circle` for the recognizable special cases and
  `Err(AnalyticalSolutionNotAvailable)` for the degree-4 general case (yang then
  falls back to Tier B Newton). Patrikalakis Ch.5 is the reference (A15.4 row).
- yang `surface_to_quadric` torus arm + a `Circle`-curve selection branch (the
  existing circle path already exists; only the torus→quadric mapping is new).

The full degree-4 ANALYTIC curve (a real `SsiCurve::Spiric` with a traced
parameterization and an output `Curve` variant) is a much larger, lower-value
effort — only needed if downstream consumers require analytic torus-intersection
EDGES (none do today; tessellation and validation are vertex-based). Defer it.

---

## 6. Recommended sequencing & effort

1. **Tier B Newton relocation (yang-only)** — the unblock. New
   `relocate_onto_implicit_pair` helper (`F`+`∇F` for every `Surface`, Gauss–
   Newton step, convergence + rank guards) + a torus branch in
   `stage4_relocate_and_correct` + the junction guard. Un-ignore the KV6d
   end-to-end test; add oblique-plane and ⊥-plane torus-cut fixtures with
   watertight + on-surface + volume oracles. **A focused multi-increment effort**
   — the math is standard, the care is in the degenerate/junction guards and the
   shared-vertex watertightness (relocate the mesh vertex ONCE so both faces see
   the moved point).
2. **Corpus verify** — the 23 revolve-circle corpus cases
   (`kv6d_torus_boolean_scope.md` §1); SUPPORTED_WRONG==0 gate.
3. **Tier A analytic circles (optional)** — `ssi-rs` torus row + circle output
   edges for the perpendicular/coaxial special cases. Polish, not a gate.
4. **Degree-4 analytic curve (deferred)** — only if analytic torus-intersection
   edges are ever required.

**Honest headline:** the torus-boolean unblock is a **yang Stage-4 Newton-
projection onto the implicit surface pair**, NOT the full A15.4 torus SSI row.
The conic special cases and analytic edges (`ssi-rs`) are downstream polish.

---

## 7. References

- yang Stage-4: `lib.rs:3483` (relocation module), `:7000`
  (`stage4_relocate_and_correct`), `:3404`/`:3461` (`signed_distance_to_surface`
  + torus implicit), `:4304` (`surface_to_quadric`), `:196` (`Curve` enum).
- ssi-rs: `lib.rs:98` (`QuadricSurface`, no Torus), `:135` (`SsiCurve`, conics
  only), `:355` (`intersect` dispatch, no torus arms).
- validate: `validate.rs:929` (`validate_torus_face`, the rejecting gate), `:94`
  (`CURVED_SURFACE_DEBUG_TOLERANCE`).
- Memory: `kernel_v2_kv6d_torus` (the wiring + this blocker),
  `kernel_v2_kv6b_f3_line_ssi` (the junction-relocation precedent).
- Math: Patrikalakis & Maekawa Ch.5 (SSI, marching for transcendental/higher-
  degree curves), Yang 2025 §4.4.1 (mesh updating / relocation). Spiric
  sections: the torus∩plane quartic family.
```
