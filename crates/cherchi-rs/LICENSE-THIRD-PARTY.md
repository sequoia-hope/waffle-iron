# Third-Party License Attributions — cherchi-rs

This crate contains Rust code ported from the following upstream C++ projects.
All upstream sources are MIT-licensed.

## Cherchi 2020 — FastAndRobustMeshArrangements

- **Repo**: https://github.com/gcherchi/FastAndRobustMeshArrangements
- **License**: MIT
- **Copyright**: © 2020 Gianmarco Cherchi, Marco Livesu, Riccardo Scateni, Marco Attene
- **Paper**: "Fast and Robust Mesh Arrangements using Floating-point Arithmetic" (SIGGRAPH Asia 2020)
- **What we port**: indirect predicates, mesh arrangement (§4 + §5), `FastTrimesh` data structure

## Cherchi 2022 — InteractiveAndRobustMeshBooleans

- **Repo**: https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
- **License**: MIT
- **Copyright**: © 2022 Gianmarco Cherchi et al.
- **Paper**: "Interactive and Robust Mesh Booleans" (ACM TOG 2022)
- **What we port**: boolean labeling via ray-cast (§5), result mesh construction, segment-insertion CDT

## cinolib (transitive dependency)

- **Repo**: https://github.com/mlivesu/cinolib
- **License**: MIT
- **Copyright**: © Marco Livesu
- **What we port**: pieces required by the Cherchi codebases — typically `Rational`-like exact arithmetic helpers (replaced by `dashu` in our port), `points_are_colinear_3d` and similar predicate helpers

## Per-file ported-source ledger

| Rust file | Upstream source | Project |
|---|---|---|
| `src/arrangements/intersection_points.rs` | `arrangements/code/intersection_classification.cpp` | Cherchi 2022 |
| `src/arrangements/aux_structure.rs` | `arrangements/code/triangulation.cpp` (aux-structure / point grouping) | Cherchi 2022 |
| `src/arrangements/retriangulate.rs` | `arrangements/code/triangulation.cpp` (`splitSingleTriangle` / `findContainingTriangle` / `fastPointOnLine`) | Cherchi 2022 |

## Per-file attribution

Every `.rs` source file that contains code ported from one of the above
projects MUST include a header comment naming the upstream file and license:

```rust
//! Ported from [project]/[upstream-file].cpp (MIT).
//! © [year] [copyright holder(s)].
//! See ../LICENSE-THIRD-PARTY.md for full attribution.
```

## Cargo dependencies (linked crates, not vendored source)

- **`spade`** (v2) — https://github.com/Stoeoef/spade — dual MIT / Apache-2.0.
  Constrained Delaunay triangulation backend for
  `triangulation::cdt_polygon_with_holes` (planar non-convex / holed face
  tessellation). Pure-Rust, WASM-clean; exact orientation / in-circle predicates
  via the `robust` crate (Shewchuk adaptive). We link it as a normal dependency
  — no source is vendored or ported.

## Adaptation notes

- Where the C++ uses cinolib's `Rational`, we use `dashu` (pure-Rust).
- Where the C++ uses TBB for parallelism, we use single-threaded execution (parallelism via `rayon` is a future feature flag).
- Where the C++ uses pointers and unsafe casts (e.g., `genericPoint*` polymorphism), we use Rust enums and indices.

These adaptations preserve correctness and observable behavior. Where the
adaptation introduces a deliberate deviation, the rationale is documented in
the file's header and in `docs/cherchi_port_deviations.md` (to be created
as deviations accrue).
