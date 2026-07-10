# yang-rs `stage0.rs` Decomposition — Directory Module Split

**Status:** PROPOSED
**Scope:** `crates/yang-rs/src/stage0.rs` (8,602 lines as of 2026-07-10)
**Type:** Pure mechanical refactor — zero behavior change, same rules and
oracles as `specs/yang_rs_lib_decomposition.md` (the lib.rs decomposition,
executed 2026-07-10). This spec inherits that spec's Invariants I1–I6,
Branch table (empty by construction), and Oracles O1–O4 verbatim, with the
consumer-facing constraint restated below.

## Goal

`stage0.rs` is the largest remaining god module (8,602 lines) — the Stage-0
§4.5.5 coplanar preprocessing engine plus five accreted helper regions from
the M8/N2 campaigns. Split it into a **directory module** `src/stage0/` so
every existing `stage0::X` consumer path (e.g. `stage0::PairCylinder` in
`boolean.rs`) stays byte-identical, with no file over ~1,700 lines.

## Current region census (line ranges in today's stage0.rs)

| Lines | Region | Contents |
|---|---|---|
| 1–163 | header | module doc, imports, `PairPlane`/`PairCylinder`/`Stage0` structs, `probe()` |
| 164–1373 | driver | `stage0_preprocess` (one ~1,200-line fn) |
| 1374–1942 | frame/gates | `Frame`, `canonical_frame`, face/loop helpers, overlay gates, `orient_sign_exact` |
| 1943–2822 | relocation | `RelocOutcome`, `EarclipErr`, `earclip_cavity_polygon`, `relocate_minted_*`, `first_ring_crossing` (M8 incr 8–11) |
| 2823–3680 | rim chords | `RimChordCtx`, `mixed_chord_ctxs`, `resolve_rim_chord_vertex`, `lateral_for_cap`, `collect_{rim,ring,mixed}_crossings`, `ArcLateral` |
| 3681–3928 | polygon extraction | `TessellatedFacePolygon`, `cluster_frame_coords(_rim_aware)`, `face_polygon_2d_tessellated`, `mixed_planar_face`, `mixed_face_polygon_2d` |
| 3929–4503 | disc pair | `DiscPair`, `V2`, disc containment, convex/fan/earclip/annulus tris |
| 4504–5377 | mesh build | `SplitMap`, `dump_pair_overlay`, `collect_edge_splits`, `BuildErr`, `fan_split_tri`, `edge_split_curved_face`, `build_stage0_mesh`, `intern_vert`, `triangulate_ring` |
| 5378–6411 | cylinder | coincident cylinder pair/group detection, `coincident_cylinder_stage0`, conformal ring machinery |
| 6412–8602 | tests | 7 `#[cfg(test)]` mods, 53 tests: annulus, cylinder_pair, fan_split, earclip_ring, frame_cluster, reloc, ring_exact_projection |

The module's crate-facing surface is 6 `pub(crate)` items: `Stage0`,
`PairPlane`, `PairCylinder`, `stage0_preprocess`,
`detect_coincident_cylinder_pairs`, `coincident_cylinder_stage0`. All
consumer paths go through `stage0::…` and MUST keep resolving unchanged.

## Target layout

```
crates/yang-rs/src/stage0/
  mod.rs        ~1,400  module doc, imports, the 3 structs, probe(),
                        stage0_preprocess, mod decls + pub(crate) use globs
  frame.rs      ~1,200  frame/gates region + polygon-extraction region
                        + frame_cluster_tests
  reloc.rs      ~1,600  relocation region + reloc_tests
  rim_chords.rs   ~860  rim-chord region
  disc_pair.rs    ~690  disc-pair region + annulus_tests
  mesh_build.rs ~1,650  mesh-build region + fan_split_tests +
                        earclip_ring_tests + ring_exact_projection_tests
  cylinder.rs   ~1,290  cylinder region + cylinder_pair_tests
```

Test mods move WITH their subject file (each of the 7 test mods exercises
exactly one region). Their module paths change from `stage0::<tests>` to
`stage0::<file>::<tests>` — nothing references test-module paths.

## Increments (one commit each, green per the lib.rs-spec gates)

1. `git mv src/stage0.rs src/stage0/mod.rs` — pure rename, zero content
   change (directory-module form; `mod stage0;` in lib.rs is untouched).
2. `cylinder.rs` (+ cylinder_pair_tests)
3. `mesh_build.rs` (+ fan_split / earclip_ring / ring_exact_projection tests)
4. `disc_pair.rs` (+ annulus_tests)
5. `rim_chords.rs`
6. `reloc.rs` (+ reloc_tests)
7. `frame.rs` (both regions + frame_cluster_tests)

Recipe per increment (proven in the lib.rs decomposition): cut the region
verbatim; child file starts `#[allow(clippy::wildcard_imports)]
use super::*;`; promote moved top-level items/fields `pub(crate)`; mod.rs
gains `mod <file>;` + `pub(crate) use <file>::*;` so intra-stage0 and
crate-facing names keep resolving. Zero logic edits; compiler-guided only.

## Gates

- Per increment: `cargo test -p yang-rs` (641/0 conservation) +
  `cargo clippy -p yang-rs` + `cargo fmt --check` + `cargo check -p kernel-v2`.
- Completion: fast tier green; full assay per-case **byte-identical** to the
  committed `app/tests/cases/assay/results.json` (232 pass / 46 error /
  16 unsupported); WASM bundle rebuilt and committed.
- Test conservation: 53 `#[test]`s in stage0/ after the split (and 641 total
  crate-wide passing).

## Failure modes

Same as the lib.rs spec (hidden coupling → pub(crate), stale line refs →
grep function names, assay TIMEOUT → container artifact, re-measure solo).
One addition: `stage0.rs` has NO `// ===` banners — region boundaries above
were derived from the top-level item census; each cut is asserted against
the expected first/last item before deletion (same python line-surgery
discipline).

## Research basis

None required — no algorithm change (P8 n/a); boundaries follow the module's
own campaign regions.
