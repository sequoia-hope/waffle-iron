# Untouched-shell passthrough (P0030)

Status: inc-1 IMPLEMENTING (2026-10-10)
Driver: P0030 (`docs/yang_tail_triage.md` seed 3) — `s6-curved-empty-cycles:
face 0`, `reassembled output would be non-2-manifold`.

## 1. The finding

P0030's auto-union is a tilted square prism and a CLOSED ring torus whose
AABBs overlap but whose surfaces never meet. The exact arrangement agrees:
every one of the 7 Stage-6 patches is a WHOLE input face (the prism's six
4-edge quads and the torus), and no triangle was split. The torus face is
boundaryless — a closed T² has no mesh boundary edge at all — so
`patch_boundary_cycle` returns no cycle and Stage 6 refuses the face.

It is a family, not a case. A throwaway census of AABB-overlapping,
surface-disjoint pairs (sphere/torus/box/cylinder, ×{∪, −, ∩}) measured
**12 of 21 STOPs at this one text**, among them the most ordinary solids in
CAD: a hollow ball (sphere − inner box), a box with a spherical or toroidal
cavity (box − inner sphere/torus), a ball unioned with a box beside it. The
common factor: a sphere or torus face survives WHOLE. A whole cylinder or
planar face survives fine (its rims are mesh boundary edges).

## 2. Why the face is boundaryless, and what the paper does

Yang 2025 §4.4.2 segments the mesh Boolean result into patches "along the
boundary curves, which correspond to either the original boundary curves or
the intersection curves", then "restor[es] the corresponding parametric
surfaces and boundary curves". A face no intersection curve reaches is
bounded by its ORIGINAL curves alone, and is restored as it was. For a
closed sphere/torus the original curves are the seams (kernel-v2's closed
torus is V=1 E=2 F=1, the aba⁻¹b⁻¹ square), which yang's mesh makes
interior edges — so on our side the original topology has to come from the
operand, not from the mesh.

A sphere/torus face with no boundary is a whole SHELL by itself (no edge
joins it to another face). So the unit of "untouched" is the shell: a shell
no intersection reaches is kept or dropped WHOLE by the op's in/out rule and
its kept form is the operand shell verbatim (§4).

## 3. The certificate (paper §4.3.1, Fig. 10a)

"Untouched" must be EXACT, not mesh-level: a sub-sagitta graze leaves the
meshes apart while the exact surfaces meet, and passing such a shell
through would be silently wrong. The paper's conservative intersection
check is the gate: "triangles closer than 2dε are filtered" as potential
contacts even when the meshes do not intersect.

Per triangle `t` of a face on surface `S`: `dev(t)` bounds the distance of
the exact surface patch over `t` from `t`. With `ρ` = t's circumradius and
`κ` a bound on |normal curvature| of `S`:

| surface | κ |
|---|---|
| Plane | 0 ⇒ dev = 0 |
| Cylinder r | 1/r |
| Sphere r | 1/r |
| Torus R, r | max(1/r, 1/(R − r)) |
| Cone | cos α / ρ_min, ρ_min = least radial distance among t's vertices (∞ curvature at the apex ⇒ dev = ρ) |

`dev = (1 − √(1 − (ρκ)²))/κ` when `ρκ < 1` (the inscribed-cap sagitta of
the osculating sphere), else `ρ`.

A pair `(t ∈ X, s ∈ Y)` is a CONTACT when `dist(t, s) ≤ dev(t) + dev(s) +
band`, `band = 2·max(TAU_MODEL, scale·TAU_WORK)` (the same YR24 weld margin
`union_operands_strictly_disjoint` uses, so a weld-band touch stays in the
pipeline where Stage 0 owns it). `dist` is 0 when an edge of either
triangle crosses the other, else the least of the 6 vertex–triangle and 9
edge–edge distances. A shell with no contact against ANY triangle of the
other operand is CLEAR; its exact surface is disjoint from the other
operand's exact surface.

A clear shell is classified by the generalized winding number of the
OTHER operand's whole mesh at one of the shell's mesh vertices. The
vertex is > band from the other mesh and that mesh is within dev of its
exact surface, so the mesh and exact verdicts agree; a winding number that
is not within 0.25 of an integer is a loud error (an open or
self-overlapping mesh), never a guess.

## 4. Branch table — inc-1: NO contact anywhere

When EVERY shell of both operands is clear (and the AABBs overlap — the
AABB-disjoint union keeps #134's arena merge), the result is set algebra on
whole shells:

| op | A shell kept when | B shell kept when | B's kept sense |
|---|---|---|---|
| Union | outside B | outside A | as is |
| Subtract | outside B | INSIDE A | REVERSED (a cavity) |
| Intersect | inside B | inside A | as is |

No kept shell ⇒ `EmptyBooleanResult` (the existing contract). Kept shells
are COPIED into one new solid (fresh entities; a reversed copy walks every
loop backwards and complements every surface's sense — plane normal
negated, curved `reversed` toggled), validated, and journalled `Same` from
each operand face.

Any contact ⇒ the existing pipeline, byte-identical.

## 5. inc-2 (follow-up, NOT this increment): mixed operands

A hollow ball cut by a box that reaches only its outer skin: the inner
void sphere is clear, the outer shell is touched, and Stage 6 still STOPs
on the void face. Remedy: run yang on the TOUCHED shells only and add the
clear shells' kept forms by the §4 table — but only when every clear shell
`C` has winding number 0 at the OTHER operand's touched shells (else
removing `C` changes their in/out labels; then the full pipeline runs as
today, loud). Needs a shell-subset `to_yang_brep`.

## 6. Oracles

- yang-rs unit tests: the census on Stage-1 meshes — a torus with a box
  threaded through its hole (both clear, outside), sphere ⊃ box (box
  inside), a graze inside the sagitta band (CONTACT, though the meshes are
  apart), a crossing pair (CONTACT).
- kernel-v2 `untouched_shell_passthrough.rs`: the 21-pair census as pins —
  exact shell counts, volumes against analytic values within the render
  mesh's chord error, closed-face cavities validate.
- P0030 → SUPPORTED_CORRECT; full corpus zero-lost.

## 7. Ledger

- 2026-10-10: spec written; family census 12/21 STOP.
- 2026-10-10: inc-1 SHIPPED. `yang_rs::shell_contact_census`
  (`crates/yang-rs/src/shell_contact.rs`) + kernel-v2
  `clear_operands_set_algebra` / `transform::copy_shells`. The one-shot
  chord test was NOT enough for P0030: the coarse torus's per-triangle
  bound (7.46e-5) exceeds the 3.2e-5 exact clearance, so a pair that fails
  it is REFINED (§4.1 four-way subdivision, midpoints projected onto the
  exact surface; ≤ 16 levels) until it clears or its bounds fall under the
  band. A reversed half-edge takes its TWIN's curve (a `Circle` / `Arc` /
  `EllipseArc` normal is directional). Probes: `YANG_SHELL_CONTACT_PROBE`
  (contact pair, distance, reach, triangles), `KV2_SHELL_CONTACT_PROBE`
  (per-shell verdicts). P0030 ⇒ SUPPORTED_CORRECT; the 21-pair sweep all
  correct.
