# Spec: migrate output-rim curve typing into yang Stage 5/6

**Status:** planned (design review 2026-07-12 F6). **Owner milestone:** its own
milestone — do NOT fold into an unrelated increment. **Governance:** A15.5
(surface-tier preservation), A15.6 (Yang pipeline stages), P9 (no silent
degradation).

## Problem

The yang mesh boolean can degrade a solid's OWN surviving analytic rim to an
untyped `LineSegment` chord run on output:

- `yang-rs/src/stage3_ssi.rs::build_intersection_curves` types only the NEW
  A∩B intersection curves.
- `yang-rs/src/stage5_topology.rs` (~`:415`) emits an edge as
  `intersection_curves.get(key).unwrap_or(Curve::LineSegment)` — so a circular
  rim of one operand that was Steiner-subdivided during meshing, but is not a
  fresh A∩B curve, exits as a polyline of `LineSegment`s.

kernel-v2 then compensates in `recover.rs` (`recover_output_curves` →
`try_recover`): it re-fuses co-circular `LineSegment` runs back into a single
`Circle`/`Arc` by re-deriving the circle from the exact surfaces. This is the
after-the-fact repair layer A15 warns against, and it is:

- **Partial** — it handles cylinder∩⊥-plane circular rims and bails
  (`try_recover → None`, clone originals) on ellipse / hyperbola / torus /
  general rims, which then ship as chords (a silent A15.5 surface-tier erosion).
- **Duplicative** — the plane∩cylinder circle math it re-derives is the exact
  computation `ssi-rs` already owns (`lib.rs` `plane_cylinder` C1), unreachable
  from kernel-v2 across the dependency layering.
- **On the wrong side of the seam** — topology reconstruction runs in the
  consumer crate, which lacks the surfaces + tessellation map that yang has in
  scope.

## Target

Type every output boundary edge at EMISSION, inside yang, where both the
defining surfaces and the Stage-1 tessellation/bijective map are available. An
output rim `e` lying on surface `S` of one operand is `S ∩ (the plane/surface
that trimmed it)` — an SSI curve, obtainable via `ssi_rs::intersect` (yang
already depends on ssi-rs). After migration:

1. `stage5_topology` never emits a bare `LineSegment` for an edge that lies on
   a curved surface; it carries the typed `Curve` (`Circle`/`Arc`/`Ellipse`/
   `HyperbolaArc`/`SurfacePair`).
2. `recover.rs` shrinks from a reconstruction pass to a validation ASSERTION:
   a curved face whose loop still carries `LineSegment` chord runs is a yang
   emission bug → typed error, not a silent repair.
3. The duplicated plane∩cylinder closed form in `recover.rs` is deleted (the
   one true copy lives in `ssi-rs`).

## Branch table (edge classification at emission)

| Edge lies on | Trimmed by | Output curve |
|---|---|---|
| Plane | anything | `LineSegment` (correct — planar edge) |
| Cylinder | ⊥ plane | `Circle` |
| Cylinder | oblique plane | `Ellipse` |
| Cone | plane | `Circle`/`Ellipse`/`Parabola`/`HyperbolaArc` per §Plane–Cone |
| Sphere | plane | `Circle` |
| any curved | curved (A∩B) | already typed by `build_intersection_curves` |
| general quadric pair | — | `SurfacePair` (already M5) |

## Invariants / oracles

- **I1 (no degradation):** for every corpus case, no output edge on a non-Plane
  face is a `LineSegment`. Assert in a yang-level test over the assay operands.
- **I2 (recover.rs is a no-op):** after migration, `try_recover` returns the
  originals unchanged for every corpus case (its recovery branch never fires) —
  a regression test that its output equals its input pins the assertion role.
- **I3 (curve vocabulary parity):** the disjoint-union passthrough
  (`boolean.rs:1771`, already preserves rims bit-for-bit) and the normal
  boolean path produce the SAME curve types for the same surviving rim.
- **I4 (assay):** byte-identical or improved assay; any case that flips must be
  explained (a previously chord-shipped rim now correctly typed may change
  downstream tessellation — verify it is a genuine improvement, P9).

## Failure modes

- A curved rim whose trimming surface is itself unavailable at emission →
  typed error (`LocalRefinementRequired` or a new `RimTypingFailed`), never a
  chord fallback.
- Migration must land WITH the recover.rs → assertion change in the same PR, so
  the repair layer and its replacement never coexist as two authorities.

## Research basis

[#24] Yang 2025 §4.5 (output boundaries are surface∩surface); the paper types
edges at assembly. A15.5 (tier preservation). Interim state and the recover.rs
bail are annotated in `crates/kernel-v2/src/recover.rs`.
