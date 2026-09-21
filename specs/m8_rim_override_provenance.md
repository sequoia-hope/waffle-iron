# M8: Rim-override provenance — own emission vs mirrored scaffolding

**Status:** IMPLEMENTED 2026-09-21 (`crates/yang-rs/src/stage0/mesh_build.rs`
`RimSplitMap`; producers in `stage0/rim_chords.rs` and `stage0/mod.rs`).
**Customer:** a through-bore whose caps are coplanar with the bored part's
caps (the B3 sprocket bore, `test-harness/tests/sprocket_kv2.rs::
sprocket_bore_with_coplanar_caps`, quarantined `#[ignore = "M8 …"]` since
2026-09-19 — un-quarantined by this increment).
**Paper basis:** Yang 2025 §4.5.5 (`refs/text/yang2025_hybrid_boolean.txt:
718-740`): the coplanar overlap is replaced by ONE trimmed common surface
whose mesh is identical for both models, and "the common part and the other
two parts share identical sampling points on their boundaries" (Fig. 16 d).
The rim-override map is how Stage 0 pushes those shared boundary samples
into the incident laterals; this spec fixes WHOSE bits a shared sample
carries when two overlays produce it.

## 1. Measured mechanism (`YANG_SPLIT_PROBE=1`, `[mixed-cross]` / `[rim-count]`)

A 20T ISO 08B sprocket (extruded 5 mm) minus a coaxial cylinder bore of the
same height: both of the sprocket's caps pair with the tool's caps (pairs
`(0,0)` at z=0 and `(1,1)` at z=5 mm). Each pair's overlay refines the
sprocket's tooth arcs with on-circle samples (`dr` ≈ 1e-18) — the same
geometric points on both caps, because the bore is a vertical prism — and
`collect_mixed_crossings` propagates them:

1. pair `(0,0)`: rim edge 71 (bottom arc) gets 8 OWN points P; the opposite
   arc 89 (top) gets `mirror(P)` by the f64 axial projection (`scale − 1`
   = 0 … 2.2e-16 → some coordinates move by one ULP);
2. pair `(1,1)`: rim edge 89 emits its 8 OWN points Q in ITS overlay frame
   — ULP-twins of `mirror(P)` (3 of 8 bit-equal, 5 not) — and mirrors
   them back onto 71, where 4 of 8 bit-match P.

Bit-exact dedup therefore leaves 13 samples on edge 89 and 12 on edge 71
(natural chain 15 → `15 vs 16` at Stage 1: "arc chains cannot pair —
routed to the chart CDT", where the twins have IDENTICAL `(u, v)` — same
azimuth, same z — and the CDT STOPs `duplicate (coincident) loop vertex`).
The full-circle disc path has the same latent: the bore's own rims came
back 121 vs 124 (57 of 64 top-cap crossings failed to bit-match the
bottom cap's mirrors; the azimuth merge happened to be reached later than
the strip STOP).

Three f64 spellings of one geometric point exist: the bottom cap's own
emission, the top cap's own emission, and each one's f64 image of the
other. Only the OWN spellings are bit-shared with a cap's Stage-0 mesh
(§4.5.5's "identical sampling points"); a mirror is scaffolding that only
the lateral consumes, so that its two chains stay count-matched when the
opposite cap has no overlay of its own.

## 2. Rule

`RimSplitMap` stores `(point, kind ∈ {Own, Mirror})` per rim edge:

| push        | bit-equal sample exists | OWN sample within `TAU_WORK` | MIRROR sample within `TAU_WORK` | else     |
|-------------|-------------------------|------------------------------|---------------------------------|----------|
| `push_own`  | `DuplicateBits`         | insert (bit-exact policy)    | **`ReplacedMirror`** (in place) | insert   |
| `push_mirror` | `DuplicateBits`       | **`AbsorbedByNear`** (drop)  | insert (bit-exact policy)       | insert   |

- OWN vs OWN stays bit-exact: genuinely distinct band-close crossings of
  one overlay (the R0088/R0070 twin population, ≥ TAU_MODEL apart) both
  enter the ring — unchanged (`m8_stage0_band_scale_crossing_verts` E-C1).
- MIRROR vs MIRROR stays bit-exact: same-ray twin images (task #144
  refutation record, `stage0_rim_projection::…same_ray_twins…`) are
  unchanged — the azimuth-merge count wall keeps that class loud.
- `TAU_WORK` (1e-12) separates ULP-twins (1e-18 … 1e-15 for model-scale
  coordinates) from anything the kernel treats as distinct (≥ TAU_MODEL,
  1e-7). It is the working-precision constant, not a new band.

Producers and their kind:

| site | kind |
|---|---|
| `collect_ring_crossings` cap set | Own |
| `collect_ring_crossings` opposite images (`push_opp`) | Mirror |
| `refine_rim_membership` inserted samples / their images | Own / Mirror |
| `collect_mixed_crossings` arc chain / opposite arc | Own / Mirror |
| `DiscPair::Identical` `shared_rim` / `opp_a`,`opp_b` | Own / Mirror |
| rim-table fusion elected value (`apply_rim_table_fusion`) | Own |

Stage 1 still consumes plain `BTreeMap<u32, Vec<Point3>>`
(`RimSplitMap::to_points`, insertion order preserved) — its ring builders
are byte-identical for maps with no near-twins.

## 3. Oracles

- Unit (`tests_unit/m8_rim_override_provenance.rs`, 7): each table cell,
  the sprocket cap-pair push sequence leaving ONE own sample per rim,
  own-vs-own and mirror-vs-mirror bit-exactness, the `TAU_WORK` edge,
  `to_points` order.
- End-to-end: `sprocket_bore_with_coplanar_caps` (mesh volume removed =
  π r² h within the un-bored mesh's chord error, watertight, χ = 0).
- Existing pins unchanged: `tests_unit/stage0_rim_projection.rs` (3),
  `tests_unit/m8_rim_refine.rs`, the `m8_*_campaign` harness suites.
- Corpus: full categorized assay (release, 8 jobs, 600 s) — see the
  roadmap note of 2026-09-21 for the measured score.

## 4. What this does NOT do

- It does not make the two caps' overlays bit-consistent (they run in
  independent frames by design); it makes the RIM carry each cap's own
  bits, which is the §4.5.5 contract.
- It does not touch the refuted exact-translation arm (#144) nor the
  chord-deep mirror class (C0048/F0067): a chord-deep own point and the
  opposite cap's on-circle mirror are a sagitta apart, far beyond
  `TAU_WORK`, and stay two samples → the loud azimuth-merge wall.
