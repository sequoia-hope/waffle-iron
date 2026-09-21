# Spec: M8 disc ∩ HOLED line-polygon coplanar pair — route the `disc-poly-holed` wall through the general §4.5.5 overlay (R0070)

**Status: LANDED always-on 2026-09-17** (`stage0_preprocess`, the
`DiscPair::Wall` dispatch arm in `stage0/mod.rs`). Vehicle: R0070
(`revolve(rectangle, 152°)` boss + `extrude(gear, cut)` + `extrude(circle,
cut)`, scale 1.7e-2), `UNSUPPORTED(coplanar-boolean)` at op 3 since
2026-09-11 (`disc-poly-holed | pair=(133,0)`), one of the three coplanar
residue cases with F0064/F0072.

## 1. The defect, measured (2026-09-17, `YANG_COPLANAR_PROBE=1`)

- Op 3 cuts a cylinder whose flat end lies ON the planar cap face the gear
  cut left behind. That cap is `surf=plane`, all-`LineSegment` loops, with
  ONE inner loop (the gear outline the cut punched through it).
- Stage 0 detects the pair (`cross-pair | pair=(133,0) band=1e-7
  gap=4.3e-18`), classifies it `disc_pair` (one face is a hole-free
  single-circle disc, neither face is annular or mixed) and calls
  `build_disc_pair`, which returns `DiscPair::Wall("disc-poly-holed")` at
  `disc_pair.rs:119` — the direct builder is a CONVEX CONTAINMENT
  construction (disc strictly inside a convex polygon or vice versa, one
  shared fan plus an annulus) and a hole is outside what it can express.
- The dispatch arm forwarded only two wall tags to the general overlay
  (`disc-poly-nonconvex`, `disc-crossing`); every other tag, this one
  included, raised `CoplanarFacesUnsupported`, which kernel-v2 maps to the
  typed `NotSupported` toast.
- The general overlay ALREADY consumes the shape: `face_polygon_2d`
  projects the outer loop AND every inner loop into a `PolygonWithHoles`,
  the exact 2D overlay segments it, and `overlay_face_supported` admits an
  all-line face regardless of holes. Annular caps (spec
  `m8_holed_disc_coplanar_overlay`) and non-convex partners already take
  exactly this path. The wall was a routing gap, not a missing algorithm.

## 2. The paper's rule — §4.5.5

`refs/text/yang2025_hybrid_boolean.txt:717-731`: "Two coplanar planes will
be segmented into three parts after a Boolean operation in 2D … The
overlapping part is replaced by a trimmed common planar surface, and
identical meshes are generated for both models in this part." Fig. 16
caption: "The common part and the other two parts share identical sampling
points on their boundaries." Nothing in the rule distinguishes a face with
a hole from one without: the 2D Boolean of two trims is defined for any
planar region. A holed partner is the ordinary case for a face a cut has
already passed through.

## 3. The fix

One dispatch change in `stage0_preprocess` (`stage0/mod.rs`):

```rust
DiscPair::Wall("disc-poly-nonconvex")
| DiscPair::Wall("disc-crossing")
| DiscPair::Wall("disc-poly-holed") => {}   // → general overlay path
```

The general path then does what it does for a non-convex partner: projects
both faces into the pair frame (`face_polygon_2d_tessellated`: the disc as
its exact Stage-1 rim ring with `rim_map`, the holed polygon as a
`PolygonWithHoles` with `corners`), runs the exact overlay, emits identical
overlap triangles to both solids, propagates any rim split into the
cylinder lateral + opposite cap (`collect_rim_crossings`), and propagates
any subdivided polygon edge — outer OR hole edge — into the neighbour faces
that share it (step 4, shared boundary sampling; the hole walls are plain
line faces and re-fan). No new geometry code.

## 4. Branch table

| Disc vs holed all-line polygon | Before | After |
|---|---|---|
| disc inside the face, clear of the hole (R0070's shape) | `disc-poly-holed` wall | general overlay, containment |
| disc rim crosses a hole edge | `disc-poly-holed` wall | general overlay, crossing (rim split → lateral, hole edge split → hole wall) |
| disc contains the hole (counterbore / cover) | `disc-poly-holed` wall | general overlay, overlap = disc − hole |
| disc vs hole-free convex polygon | direct builder | unchanged (byte-identical) |
| disc vs non-convex / crossing hole-free polygon | general overlay | unchanged |
| annular disc partner | general overlay (`m8_holed_disc…`) | unchanged (`annular_disc_face` is checked BEFORE the disc-pair gate) |
| holed polygon whose hole loop carries an arc/circle | `mixed_planar_face` → general overlay | unchanged |

## 5. Invariants

- **I1 (identical overlap mesh):** the overlap triangles are bit-identical
  in both Stage-0 meshes — inherited from the overlay engine, unchanged.
- **I2 (T-junction-free):** a hole edge subdivided by the disc rim is
  re-sampled in the hole wall that shares it (step 4), so the Stage-1 mesh
  stays watertight — the same mechanism as an outer edge.
- **I3 (loud residue):** every other `DiscPair::Wall` tag keeps raising
  `CoplanarFacesUnsupported`; the dispatch arm adds one tag and nothing else.
- **I4 (output correctness):** watertight, outward, and analytic volume
  within the chord band on every isolated fixture (§6).

## 6. Tests — `crates/yang-rs/tests/m8_disc_holed_polygon.rs` (8)

Fixture: a 4×4×1 plate with a 1×1 square through-hole (`holed_box`, ten
all-line planar faces, holed top and bottom caps) and the sibling suites'
`z_cylinder`. RED without the dispatch change (5 of 8 fail with
`CoplanarFacesUnsupported`), GREEN with it (measured 2026-09-17).

| test | configuration | analytic |
|---|---|---|
| `holed_box_fixture_is_a_valid_solid` | disjoint union | 15 + π/4 |
| `disc_in_holed_polygon_union_succeeds` | boss on the cap, clear of the hole | 15 + 0.36π |
| `disc_in_holed_polygon_cut_succeeds` | tool base on the cap (touching), then tool top on the cap through the plate | 15, then 15 − 0.36π |
| `disc_crossing_hole_edge_union_succeeds` | boss rim crosses the hole's +x edge | 15 + 0.2025π |
| `disc_crossing_hole_edge_cut_succeeds` | through-cut overhanging the hole across one edge | 15 − (0.2025π − segment) |
| `disc_over_hole_union_succeeds` | r = 1 boss covering the hole | 15 + π |
| `disc_over_hole_cut_succeeds` | counterbore, tool top on the cap | 15 − 0.5(π − 1) |
| ~~`doubly_flush_crossing_stays_loud`~~ `doubly_flush_crossing_cut_succeeds` / `_union_succeeds` | both tool caps flush with the plate's, rim crossing the hole edge (§7, CONVERTED 2026-09-21) | 15 − (0.2025π − segment) / 15 + segment |

## 7. Boundary found on the way (pre-existing, out of scope)

A tool whose BOTH caps are flush with the plate's two caps (two coplanar
pairs sharing ONE cylinder lateral) while its rim crosses a straight edge
fails loud — `azimuth-merge rims disagree` on the holed plate,
`FaceResolutionFailed` on an unholed plate with an outer-edge crossing —
so it is not a hole effect: each pair's rim split is merged into the
lateral independently and the two rims end up with different sample sets.
Containment with both caps flush passes. Ledgered here and pinned loud
(`doubly_flush_crossing_stays_loud`); the fix belongs to the rim-override
merge (`m8_rim_override_uniform_merge`), not to this slice.

> **CONVERTED 2026-09-21** (spec `m8_rim_override_provenance.md`): the two
> rims did not merge "independently" so much as each carry the other's f64
> MIRROR of the same crossing next to its own emission (ULP-twins the
> bit-exact dedup kept). With rim-override provenance each rim keeps its
> cap's own bits and the mirrors are absorbed; the pin is now the pair of
> positive oracles `doubly_flush_crossing_cut_succeeds` /
> `doubly_flush_crossing_union_succeeds` (analytic volumes within the chord
> band, watertight, outward).

## 8. Corpus outcome

R0070 advances `UNSUPPORTED(coplanar-boolean)` → `ERROR`: with Stage 0
handling the pair (probe: `disc-crossing-same-normal | pair=(133,0)` — the
cut disc crosses the gear outline), op 3 reaches Stage 4 and STOPs at
`Stage-4 relocation region around vertex 4294967295 is invalid:
LocalRefinementRequired` (`stage4_correct.rs`, the region-boundary walk
finding a boundary vertex of degree ≠ 2 — the §4.4.1 mesh-updating
region class of `docs/yang_tail_triage.md`). That is the honest next wall;
the coplanar capability gap is closed for this shape and the remaining
`UNSUPPORTED(coplanar-boolean)` set is F0064/F0072 (the N17 disc-crossing
identical-mesh class). Full-corpus numbers: `docs/yang_tail_triage.md`,
2026-09-17 entry.
