# nurbs-rs — Scope Rules

Shared Piegl & Tiller (*The NURBS Book*, 2nd ed., 1997) B-spline and NURBS curve and surface math, sitting directly above `cad-primitives` so both `kernel-v2` (Loft) and `yang-rs` (Milestone F5 generic parametric surfaces) share one implementation.

## What belongs here

- Validated polynomial and rational 3D curve and tensor-product surface data types (`BSplineCurve`, `BSplineSurface`, `BSplineError`)
- Intrinsic Piegl & Tiller evaluation and derivative algorithms (`A2.1–A2.3`, `A3.1–A3.2`, `A3.5–A3.6`, `A4.1–A4.4`)
- Knot insertion / refinement, Bézier decomposition, and degree elevation (`A5.4`, `A5.6`, `A5.7`, `A5.9`)
- Global curve interpolation (`A9.1` open and periodic closed)

## What does NOT belong here

- B-Rep topology, arenas, or half-edge structures (`kernel-v2`)
- Mesh data structures or exact mesh arrangements (`cherchi-rs` / `yang-rs`)
- Tessellation policy, adaptive mesh stitching, or chord-sag sampling (`kernel-v2` / `yang-rs`)
- Boolean pipeline stages or SSI dispatch (`ssi-rs` / `yang-rs`)

## Hard rules

1. **Zero workspace dependencies except `cad-primitives`.** Never depend on `waffle-types`, `cherchi-rs`, `ssi-rs`, `yang-rs`, or `kernel-v2`.
2. **Literature-cited implementations.** Every algorithm cites Piegl & Tiller algorithm numbers and `refs/text/piegl_tiller1997_nurbs_book.txt` line numbers in its doc comment.
3. **No `unsafe`, no `panic!`, no silent clamping or fallback paths.** Invalid degrees, knot vectors, control grids, non-positive weights, out-of-range parameters, and failed interpolations return a typed `Result<_, BSplineError>`.
4. **Determinism.** Same inputs → byte-identical outputs across runs and platforms.
