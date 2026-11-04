# Prospector Seed-6 Triage (`ERROR` and `TIMEOUT` Rows)

> Antigravity (Gemini) triage, 2026-11-03, report only. The milestone attributions are its own and are
> **not yet verified** by the kernel owner; the two harness findings (empty disjoint Intersect, the
> prospector's multi-pass budget) are being fixed in `agy/seed6-triage`.

- **Base commit:** `dc422b05` (`main`, 2026-11-03), triaged on branch `agy/seed6-triage` in `/home/claude/wt/agy-seed6`.
- **Seed-6 corpus summary (`/home/claude/workspace/target/prospect/seed-6/report.jsonl`):** 200 candidates → `178 SUPPORTED_CORRECT / 6 SUPPORTED_WRONG / 12 ERROR / 4 TIMEOUT` under the default `900 s` CPU budget. The 6 `SUPPORTED_WRONG` rows are excluded per brief.
- **Op-numbering convention:** `scratch/prefix.py` indexes 1-based non-sketch `.waffle` features (`Extrude`, `Revolve`, `BooleanCombine`). In `gen3` (`crates/test-harness/src/assay/prospect/gen3.rs:647-708`), an explicit binary `Boolean` recipe step (`∪`, `−`, `∩`) emits two `.waffle` features (`Extrude(merge=false)` + `BooleanCombine`). Both the `.waffle` non-sketch op count and the `gen3` recipe step count are shown below where they differ.
- **Evidence discipline:** Every timing, coordinate, triangle count, edge/face ID, and loop count below was measured in this session (`scratch/logs/prefixes.log`, `scratch/logs/probes/*.log`, `scratch/logs/timeout_*.log`). The two unmeasured deductions (`00105` face-22 4-arc seam-free topology by parity with probed `00144` face 0, and `00159` single-pass step-6 `Standalone` completion time) are explicitly marked **inferred**.

---

## 1. Summary Table (16 Cases: 12 `ERROR` + 4 `TIMEOUT`)

| id | ops (`.waffle` / recipe) | first failing prefix | text | raising site | attached to | confidence |
|---|---|---|---|---|---|---|
| `X00000006-00000` | `3` (`2` steps) | `3` (`BooleanCombine` Intersect) | `empty_result: no live solid — the chain consumed every body (e.g. a disjoint Intersect); nothing left to judge` | `crates/test-harness/src/assay/categorize.rs:257-261` | Harness hole (`categorize.rs:245-262` + `prospect/mod.rs:242-255` `derive_meta`) | High |
| `X00000006-00004` | `6` (`6` steps) | `TIMEOUT` (`> 3600 s` in 3-pass `judge_generated`; `gen3::generate` steps 1–5 emit in `2875.82 s`, step 4 `Union` = `2408.39 s`) | `cpu 900.0s > budget 900s` (at `3600 s` wall cap: killed in pass-2 `gen3::build` step 4 `s2`) | `crates/test-harness/src/assay/prospect/mod.rs:344-348` (cost in `crates/yang-rs/src/stage4_construct.rs` `before-441-update` + `cherchi-rs` `s2`) | Roadmap F2, item 3 (`docs/yang_faithfulness_roadmap.md:139-142`, retiring pre-CDT Stage-4 repair/contact scans `N81`) | High |
| `X00000006-00047` | `9` (`8` steps) | `5` (`Extrude` boss auto-union) | `BooleanFailed("yang-rs rejected the converted input B-Rep: yang-rs: face 17 is degenerate (zero-area / collinear)")` | `crates/yang-rs/src/stage1_tessellate.rs:2452-2455` (wrapped at `crates/kernel-v2/src/boolean/to_yang.rs:1061-1063`) | Ledger `N5` (`docs/yang_deviations.md`, Stage-1 absolute `MIN_FEATURE_SIZE²` floor) | High |
| `X00000006-00051` | `9` (`8` steps) | `9` (`Extrude` boss auto-union) | `BooleanFailed("yang-rs rejected the converted input B-Rep: yang-rs: malformed B-Rep topology: face 3: bounded lateral patch spans 6.6844 rad of azimuth unwrapped (≥ 2π): no seam wedge for a single-sheet unroll")` | `crates/yang-rs/src/stage1_tessellate.rs:3476-3486` (wrapped at `crates/kernel-v2/src/boolean/to_yang.rs:1061-1063`) | Roadmap F5, item 4 (`docs/yang_faithfulness_roadmap.md:226-228`, single `(u, v)` chart CDT per face incl. cones, `N61`) | High |
| `X00000006-00053` | `4` (`3` steps) | `4` (`BooleanCombine` Intersect) | `BooleanFailed("yang-rs rejected the converted input B-Rep: yang-rs: face 7 is degenerate (zero-area / collinear)")` | `crates/yang-rs/src/stage1_tessellate.rs:2452-2455` (wrapped at `crates/kernel-v2/src/boolean/to_yang.rs:1061-1063`) | Ledger `N5` (`docs/yang_deviations.md`, Stage-1 absolute `MIN_FEATURE_SIZE²` floor) | High |
| `X00000006-00058` | `6` (`6` steps) | `5` (`Extrude` cut subtract) | `BooleanFailed("yang-rs: §4.5.2 local refinement ended with 1 unresolved failure site(s): IllegalIntersection×1; first IllegalIntersection on faces [(A, 113), (A, 113)]: output faces 11 × 12 on Cone …: edges 53 × 187 cross …")` | Outer: `crates/yang-rs/src/boolean.rs:668-688`; inner: `crates/yang-rs/src/boolean/failures.rs:1384-1394` (`output_curve_crossings`) | Roadmap F3, item 6 (`docs/yang_faithfulness_roadmap.md:180-181`, §4.5.4 illegal-intersection detection on updated mesh, `N83`) | High |
| `X00000006-00065` | `6` (`6` steps) | `3` (`Extrude` boss auto-union) | `TessellationFailed { face: FaceId(2278), reason: "ring rejected by CDT (degenerate/self-intersecting)" }` | `crates/kernel-v2/src/tessellate/mod.rs:981-1007` (called at `crates/kernel-v2/src/boolean/mod.rs:549`) | Roadmap F3, item 6 (`docs/yang_faithfulness_roadmap.md:180-181`, §4.5.4 illegal-intersection detection on updated mesh, `N83`) | High |
| `X00000006-00078` | `5` (`5` steps) | `5` (`Extrude` cut subtract) | `BooleanFailed("yang-rs: §4.5.2 local refinement ended with 7 unresolved failure site(s): BoundaryDomainFire×3 IllegalIntersection×4; first IllegalIntersection on faces [(A, 25), (A, 26), (B, 1), (B, 6)]: §4.4.1 update refused patch 28: its cycles cross in the Plane … chart")` | Outer: `crates/yang-rs/src/boolean.rs:668-688`; inner: `crates/yang-rs/src/stage4_correct/domain_postcondition.rs:148-212` + `crates/yang-rs/src/stage4_construct.rs:4169-4179` + `crates/yang-rs/src/boolean.rs:750-756` | Roadmap F3, item 5 (`docs/yang_faithfulness_roadmap.md:178-179`, §4.5.1 interior-point truncation/continuation/`q_1, q_2`, `N90`) | High |
| `X00000006-00095` | `3` (`2` steps) | `3` (`BooleanCombine` Intersect) | `empty_result: no live solid — the chain consumed every body (e.g. a disjoint Intersect); nothing left to judge` | `crates/test-harness/src/assay/categorize.rs:257-261` | Harness hole (`categorize.rs:245-262` + `prospect/mod.rs:242-255` `derive_meta`) | High |
| `X00000006-00097` | `5` (`5` steps) | None (completes: `SUPPORTED_CORRECT`; op 5 `Union` = `677.10 s` pass 1 / `639.14 s` pass 2) | `cpu 900.0s > budget 900s` (completes at `1350.38 s` wall / `1348.82 s` CPU across 2-pass `measure_stem`) | `crates/test-harness/src/assay/prospect/mod.rs:344-348, 456-489` (multi-pass harness evaluation over `900 s` cap) | Harness hole (`judge_generated` / `measure_stem` multi-pass evaluation under single-pass `900 s` budget) | High |
| `X00000006-00105` | `7` (`7` steps) | `7` (`Extrude` boss auto-union) | `BooleanFailed("yang-rs rejected the converted input B-Rep: yang-rs: malformed B-Rep topology: face 22: cone periodic strip (2 encircling rims) not yet supported (KV14 Slice E holed frustum band — later sub-slice)")` | `crates/yang-rs/src/stage1_tessellate.rs:3921-3928` (wrapped at `crates/kernel-v2/src/boolean/to_yang.rs:1061-1063`) | Gate-3 capability gap (`docs/kernel_1_0.md:52-55, 65-67`, periodic seam-free cone frustum representation / chain re-entry) | High |
| `X00000006-00107` | `4` (`4` steps) | `4` (`Extrude` cut subtract) | `BooleanFailed("yang-rs: §4.5.2 local refinement ended with 1 unresolved failure site(s): IllegalIntersection×1; first IllegalIntersection on faces [(A, 0), (B, 0), (B, 1), (B, 4), (B, 5)]: output face 0: loop 0 winds against the face's material side (oriented area -8.067e1)")` | Outer: `crates/yang-rs/src/boolean.rs:668-688`; inner: `crates/yang-rs/src/boolean/failures.rs:1183-1193` (caused by `crates/yang-rs/src/stage5_topology/patches.rs:943-969` `select_outer_cycle`) | Ledger `N100` (`docs/yang_deviations.md:7822-7825, 7888-7890`, item 4: `select_outer_cycle` 3D `\|Newell\|` ranking on curved bounded faces) | High |
| `X00000006-00128` | `3` (`3` steps) | `3` (`Extrude` boss auto-union) | `BooleanFailed("yang-rs: geometric face resolution failed for kept triangle 721 (centroid off all face surfaces, a membership tie unresolved by finite-extent containment, or a multi-solid label with no matching Stage-0 pair plane)")` | `crates/yang-rs/src/boolean.rs:2385` (`YangError::FaceResolutionFailed`) | Ledger `N80` (`docs/yang_deviations.md:6609-6696`, pre-Stage-4 `keep_set` classification and raw-arrangement overlap-sheet resolution) | High |
| `X00000006-00144` | `7` (`6` steps) | `7` (`Extrude` cut subtract) | `BooleanFailed("yang-rs rejected the converted input B-Rep: yang-rs: malformed B-Rep topology: face 0: cone periodic strip (2 encircling rims) not yet supported (KV14 Slice E holed frustum band — later sub-slice)")` | `crates/yang-rs/src/stage1_tessellate.rs:3921-3928` (wrapped at `crates/kernel-v2/src/boolean/to_yang.rs:1061-1063`) | Gate-3 capability gap (`docs/kernel_1_0.md:52-55, 65-67`, periodic seam-free cone frustum representation / chain re-entry) | High |
| `X00000006-00159` | `6` (`6` steps) | `TIMEOUT` (`> 3600 s` in 3-pass `judge_generated`; `gen3::generate` steps 1–5 emit in `1960.68 s`, step 3 `Subtract` = `1701.87 s`) | `cpu 900.0s > budget 900s` (at `3600 s` wall cap: killed in pass-2 `gen3::build` step 3 post-Stage-4 B-Rep emission) | `crates/test-harness/src/assay/prospect/mod.rs:344-348` (cost in `cherchi-rs` `s2` + `crates/yang-rs/src/stage4_construct.rs` `before-441-update`) | Roadmap F2, item 3 (`docs/yang_faithfulness_roadmap.md:139-142`, retiring pre-CDT Stage-4 repair/contact scans `N81`) | High |
| `X00000006-00190` | `8` (`8` steps) | None (completes: `SUPPORTED_CORRECT`; op 7 `Subtract` = `475.94 s` pass 1 / `545.44 s` pass 2 across `§4.5.2` `iter 0..3`) | `cpu 900.0s > budget 900s` (completes at `1044.08 s` wall / `997.18 s` CPU across 2-pass `measure_stem`) | `crates/test-harness/src/assay/prospect/mod.rs:344-348, 456-489` (multi-pass harness evaluation over `900 s` cap) | Harness hole (`judge_generated` / `measure_stem` multi-pass evaluation under single-pass `900 s` budget) | High |

---

## 2. Grouped `ERROR` Findings (12 Cases in 7 Groups)

### Group 1 — Disjoint `Intersect` Consuming All Live Solids (`00000`, `00095`)

- **Cases & first failing prefixes:**
  - `X00000006-00000` (`scale = 1.509e1 m`, recipe `circle:boss` → `nonconvex5:∩`): prefixes 1–2 `SUPPORTED_CORRECT` (`solids 1`, `solids 2`); fails at **prefix 3 (`BooleanCombine` Intersect)**.
  - `X00000006-00095` (`scale = 5.727e0 m`, recipe `gear23:boss` → `convex7:∩`): prefixes 1–2 `SUPPORTED_CORRECT` (`solids 1`, `solids 2`); fails at **prefix 3 (`BooleanCombine` Intersect)**.
- **Raising site:** `crates/test-harness/src/assay/categorize.rs:257-261`.
- **Failing operand (`yang-input` vs boolean):** Neither. Both kernel operands are valid, geometrically disjoint solids (`YANG_BREP_PROBE` emitted zero errors; `boolean_intersect` succeeded with 0 kept output solids). After replay finishes with zero engine errors, `categorize` checks `builder.live_solid_handles().is_empty()` (`categorize.rs:245-261`) and returns `AssayCategory::Error` (`empty_result`) unless `meta.oracles.expected_solid_count == Some(0)`. Prospector's `derive_meta` (`crates/test-harness/src/assay/prospect/mod.rs:242-255`) unconditionally writes `expected_solid_count: None`, so any generated recipe whose final binary `Intersect` has disjoint operands is categorized as `ERROR`.
- **Attachment:** **Harness hole** (`crates/test-harness/src/assay/categorize.rs:245-262` and `crates/test-harness/src/assay/prospect/mod.rs:242-255`).

---

### Group 2 — Sub-`MIN_FEATURE_SIZE²` Planar Triangle Rejected on Stage-1 Re-Entry (`00047`, `00053`)

- **Cases & first failing prefixes:**
  - `X00000006-00047` (`scale = 1.218e-4 m`, 9 `.waffle` ops / 8 steps): prefixes 1–4 `SUPPORTED_CORRECT`; fails at **prefix 5 (`Extrude`, `star6(0.40):boss` auto-union)** on `face 17` (prefixes 6–9 repeat the identical error on the retained prefix-4 solid).
  - `X00000006-00053` (`scale = 1.754e-4 m`, 4 `.waffle` ops / 3 steps): prefixes 1–3 `SUPPORTED_CORRECT`; fails at **prefix 4 (`BooleanCombine`, `convex6:∩`)** on `face 7`.
- **Raising site:** `crates/yang-rs/src/stage1_tessellate.rs:2452-2455` (`YangError::DegenerateFace { face }` inside `tessellate_planar_face` when twice-Newell magnitude `mag < MIN_FEATURE_SIZE * MIN_FEATURE_SIZE`, i.e. `1e-12 m²`), wrapped into `KernelError::BooleanFailed("yang-rs rejected the converted input B-Rep: …")` at `crates/kernel-v2/src/boolean/to_yang.rs:1061-1063`.
- **Failing operand (`yang-input` vs boolean):** **`yang-input`** (an input B-Rep face emitted by the previous boolean in the chain):
  - In `00047` (`scratch/logs/probes/00047_p5.log`), prefix 4 (`convex7:cut` subtract) emitted `OUT f17` (`Plane`, `outer = [77, 78, 79]`, `inner = []`) with `LineSegment` edge lengths `1.3306e-6 m`, `6.6153e-7 m`, and `1.4859e-6 m`; its twice-Newell magnitude is `|n| = 8.8020e-13 m²` (area `4.4010e-13 m² < 1e-12 m²`).
  - In `00053` (`scratch/logs/probes/00053_p4.log`), prefix 2 (`convex8:cut` subtract) emitted `OUT f7` (`Plane`, `outer = [37, 38, 39]`, `inner = []`) with `LineSegment` edge lengths `2.3852e-7 m`, `3.9641e-7 m`, and `5.8970e-7 m`; its twice-Newell magnitude is `|n| = 6.6858e-14 m²` (area `3.3429e-14 m² < 1e-12 m²`).
- **Paper reference & attachment:** Yang 2025 §4.1.1 (`refs/text/yang2025_hybrid_boolean.txt:298-322, 391-395`) scales discretization to the model AABB diagonal `d` (`d_ε = 10⁻² d`) with no absolute `1e-12 m²` model-space area rejection. At `scale ≈ 1.2e-4..1.8e-4 m`, valid grazing-cut faces fall below `cad_primitives::MIN_FEATURE_SIZE = 1e-6 m` (governance A14.2) and trip the Stage-1 absolute floor. Attached to **Ledger `N5`** (`docs/yang_deviations.md`, bespoke Stage-1 tessellation / absolute input floor; same class noted at `docs/yang_tail_triage.md:1398`).

---

### Group 3 — Bounded Cone Lateral Patch Spanning `≥ 2π` Unwrapped Azimuth (`00051`)

- **Case & first failing prefix:** `X00000006-00051` (`scale = 4.121e2 m`, 9 `.waffle` ops / 8 steps): prefixes 1–8 `SUPPORTED_CORRECT`; fails at **prefix 9 (`Extrude`, `convex3:boss` auto-union)** on `face 3`.
- **Raising site:** `crates/yang-rs/src/stage1_tessellate.rs:3476-3486` (`YangError::MalformedTopology` in `tessellate_cone_face` when `(hi - lo) * half_angle.sin() >= 2π`), wrapped at `crates/kernel-v2/src/boolean/to_yang.rs:1061-1063`.
- **Failing operand (`yang-input` vs boolean):** **`yang-input`** emitted by prefix 8 (`gear18:boss` auto-union into prefix 7's `convex4:rev` cone). `YANG_BREP_PROBE` (`scratch/logs/probes/00051_p9.log`) shows prefix 8 emitted `OUT f3` on `Cone { apex: (793.649, 181.481, 937.948), axis_dir: (0, 0, -1), half_angle: 1.331986 rad }` with `inner = []` and a single 123-edge outer loop (`Circle` arc `e3666` plus gear-tooth `Hyperbola` notches `e71..e185, e3659..e3665`). Walking the loop unwraps an azimuth excursion `hi - lo = 6.6844 rad`; isometric planar development into a cone sector multiplies by `sin(1.331986) = 0.97163`, giving `6.4948 rad ≥ 2π`, which self-overlaps in 2D unrolled space and is refused.
- **Paper reference & attachment:** Yang 2025 §4.1.2 (`refs/text/yang2025_hybrid_boolean.txt:294-296, 397-407`) triangulates every face in its rectangular `(u, v)` parameter domain (cutting periodic surfaces along a parameter seam rather than developing a cone isometrically into the plane). Attached to **Roadmap F5, item 4** (`docs/yang_faithfulness_roadmap.md:226-228`: *"One CDT per face, replacing the per-surface dispatch (`tessellate_planar_face` / `cylinder` / `cone` / `sphere` / `torus` / `holed_lateral` / `periodic_strip`), with cones included (`N61`)"*).

---

### Group 4 — Cone Periodic Strip (2 Encircling Rims) Rejected on Stage-1 Re-Entry (`00105`, `00144`)

- **Cases & first failing prefixes:**
  - `X00000006-00105` (`scale = 2.405e1 m`, 7 `.waffle` ops / 7 steps): prefixes 1–6 `SUPPORTED_CORRECT`; fails at **prefix 7 (`Extrude`, `convex4:boss` auto-union)** on `face 22`.
  - `X00000006-00144` (`scale = 7.500e-1 m`, 7 `.waffle` ops / 6 steps): prefixes 1–6 `SUPPORTED_CORRECT`; fails at **prefix 7 (`Extrude`, `circle:cut` subtract)** on `face 0`.
- **Raising site:** `crates/yang-rs/src/stage1_tessellate.rs:3921-3928` (`YangError::MalformedTopology` in `tessellate_lateral_holed_cdt` when `encircling_rim_loops == 2` on `Surface::Cone`), wrapped at `crates/kernel-v2/src/boolean/to_yang.rs:1061-1063`.
- **Failing operand (`yang-input` vs boolean):** **`yang-input`** emitted by prefix 6:
  - In `00144` (`scratch/logs/probes/00144_p7.log`), prefix 6 (`gear19:sym` auto-union into `nonconvex7:rev`) emitted `OUT f0` on `Cone { apex: (0.971228, 0.485526, -0.455143), axis_dir: (0, 0, -1), half_angle: 1.148038 rad }` bounded by `outer = [4138, 4139, 4136, 4137]` (4 coaxial `Circle` arcs at `z = -0.840854`) and `inner = [[4141, 4142, 4143, 4140]]` (4 coaxial `Circle` arcs at `z = -0.722737`) with zero seam edges (`recover.rs` PASS 1C left the frustum band on the 4-arc seam-free fallback; cf. `docs/yang_deviations.md:6348-6356` under `N77`).
  - In `00105` (`scratch/logs/prefixes.log:101-109`), prefix 6 (`convex3:boss` auto-union into `gear11:rev`) emitted `face 22` which hits the identical `encircling_rim_loops == 2` `Surface::Cone` arm at `stage1_tessellate.rs:3921-3928` (4-arc seam-free coaxial frustum band topology on `face 22` is **inferred** from the shared guard with `00144` `OUT f0`, as `YANG_BREP_PROBE` was not re-run on `00105`).
- **Paper reference & attachment:** Yang 2025 §4.1.2 (`refs/text/yang2025_hybrid_boolean.txt:294-296, 397-407`). Attached to **Gate-3 capability gap in `docs/kernel_1_0.md:48-70`** (`:52-55, 65-67`, `kernel-v2` periodic seam-free cone frustum representation / boolean chain re-entry, as noted in `AGY_BRIEF_SEED6.md:35-37`; the `yang-rs` Stage-1 side is also tracked in Roadmap F5 item 4 / `N61` and `N77`).

---

### Group 5 — Post-Boolean `kernel-v2` Render-Mesh CDT Ring Rejection (`00065`)

- **Case & first failing prefix:** `X00000006-00065` (`scale = 3.699e-1 m`, 6 `.waffle` ops / 6 steps): prefixes 1–2 `SUPPORTED_CORRECT`; fails at **prefix 3 (`Extrude`, `circle:boss` auto-union)** on output `FaceId(2278)` (prefixes 4–6 repeat the same failure).
- **Raising site:** `crates/kernel-v2/src/tessellate/mod.rs:981-1007` (`triangulate_ring` returning `KernelError::TessellationFailed { face: FaceId(2278), reason: "ring rejected by CDT (degenerate/self-intersecting)" }`), invoked by post-boolean self-intersection validation `validate_boolean_output_self_intersection` at `crates/kernel-v2/src/boolean/mod.rs:549`.
- **Failing operand (`yang-input` vs boolean):** **The boolean itself** (an invalid output planar face emitted by `yang_rs::boolean` on prefix 3 `Union` without raising a Stage-4/5 STOP).
- **Measured probe data (`scratch/logs/probes/00065_p3.log`):** `KV2_RING_PROVENANCE=1` and `KV2_RING_REJECT_PROBE=1` show `FaceId(2278)` is a planar gear-tooth flank (`holes = 0`, 8 B-Rep half-edges expanding to `npts = 21` polygon vertices). Its top straight crease runs along `z = 0.348873244` across three collinear `LineSegment` half-edges `he 13331`, `13332`, `13333` (`y = 0.521820 → 0.110778 → 0.074967 → 0.0`). Its lower boundary `he 13336` (`twin 13313` on cylinder `FaceId(2277)`) is an `EllipseArc` whose 13 interior samples arch from `z = 0.343273813` (`y = 0.050065`) up to `z = 0.350037037` (`y = 0.092873`) — **`1.1638e-3 m` past** the straight `z = 0.348873244` top edge — crossing the top edge near the collinear T-vertices `y = 0.074967` and `y = 0.110778`. In `yang-rs`, `T3c` (`output_loop_crossings`, `crates/yang-rs/src/boolean/failures.rs:1215-1255`) requires strict endpoint straddling outside `band` (`sp > band && sq < -band`), missing crossings that pass within `band` of a collinear vertex junction (`y = 0.074326 → 0.074967` and `y = 0.111420 → 0.110778`), so `§4.5.2` never fired (`[selfx-cost] improper=0 unresolved=0`).
- **Paper reference & attachment:** Yang 2025 §4.5.4 (`refs/text/yang2025_hybrid_boolean.txt:752-759`, detecting illegal intersections on the updated mesh and refining locally). Attached to **Roadmap F3, item 6** (`docs/yang_faithfulness_roadmap.md:180-181`: *"§4.5.4 detection on the updated mesh for every surface type (`N83`)"*).

---

### Group 6 — `§4.5.2` Local Refinement Exhausted (`00058`, `00078`, `00107`)

All three cases share the **outer raising site** `crates/yang-rs/src/boolean.rs:668-688` (`YangError::RefinementExhausted`), mapped to `KernelError::BooleanFailed` at `crates/kernel-v2/src/boolean/mod.rs:480`, and in all three the failing operand is **the boolean itself**. Probing with `YANG_452_PROBE=1` (`scratch/logs/probes/{00058_p5,00078_p5,00107_p4}.log`) shows each row hits a **distinct inner failure site and mechanism**, so each subgroup below carries its own single attachment.

#### Subgroup 6a — Cone `output_curve_crossings` Missing Across-Edge `B` Attribution (`00058`)

- **Case & first failing prefix:** `X00000006-00058` (`scale = 1.807e1 m`, 6 `.waffle` ops / 6 steps): prefixes 1–4 `SUPPORTED_CORRECT` (note: prefix 2 `convex6:rev-cut` hit 4 natural failures and resolved cleanly at `§4.5.2` `iter 0` `k=16`); fails at **prefix 5 (`Extrude`, `nonconvex6:cut` subtract)**.
- **Inner raising site:** `crates/yang-rs/src/boolean/failures.rs:1384-1394` (`output_curve_crossings`: `output faces 11 × 12 on Cone …: edges 53 × 187 cross at (1.017914e1, 8.050805e0, 4.231677e0)`).
- **Mechanism:** Unlike `output_loop_crossings` (`failures.rs:1250-1253`), which attributes crossings to both `[lineage(fi), across(ei), across(ej)]`, `output_curve_crossings` (`:1387`) records only `[lineage(fi), lineage(fj)]` = `[(A, 113), (A, 113)]`, omitting the `B` cutter faces across `edges 53` and `187`. In `00058_p5.log:84-163`, `§4.5.2` refines 46 1-ring faces on `A` (`factors ×2, ×4, ×8, ×16`) while **`b={}` stays empty (`b=20` tris unchanged)** across `iter 0..3`, until `iter 4` stops with `every named face is at the ceiling` (`FACE_REFINE_MAX = 16`).
- **Paper reference & attachment:** Yang 2025 §4.5.2 / §4.5.4 (`refs/text/yang2025_hybrid_boolean.txt:659-677, 752-759`). Attached to **Roadmap F3, item 6** (`docs/yang_faithfulness_roadmap.md:180-181`, §4.5.4 illegal-intersection detection on the updated mesh, `N83`, retiring the B-Rep curve-fit `output_curve_crossings` detector; also interacts with F3 item 2 `L3`/`N62` 2-ring triangle neighborhood attribution).

#### Subgroup 6b — Interior-Point Domain Escapes Unrepaired by §4.5.1, Aborting in Stage-1 Chart Budget (`00078`)

- **Case & first failing prefix:** `X00000006-00078` (`scale = 2.249e-4 m`, 5 `.waffle` ops / 5 steps): prefixes 1–4 `SUPPORTED_CORRECT`; fails at **prefix 5 (`Extrude`, `convex5:cut` subtract)**.
- **Inner raising sites:**
  1. `crates/yang-rs/src/stage4_correct/domain_postcondition.rs:148-212` (`BoundaryDomainFire`×3: `v186 left A:25 across edge 137 f_post=-1.581e-6`, `v187 left B:6 across edge 28 f_post=5.811e-7`, `v188 left B:1 across edge 6 f_post=1.123e-6`, while `[i13f-rehome]` declines `v182/v183` with `ShapeMismatch`);
  2. `crates/yang-rs/src/stage4_construct.rs:4169-4179` (`§4.4.1 update refused patch 28` and `patch 31`: cycles cross in `Plane` chart);
  3. `crates/yang-rs/src/boolean/failures.rs:1244-1255` (`output_loop_crossings` on `output face 25` and `output face 26`);
  4. `crates/yang-rs/src/boolean.rs:750-756` (`00078_p5.log:143`: `iter 2` aborts in `refined_by`: `Stage-1 chart triangulation of face 12 cannot meet the face's chord budget after 4 domain-grid round(s) (worst interior edge 2.833× the budget)`).
- **Paper reference & attachment:** Yang 2025 §4.5.1 (`refs/text/yang2025_hybrid_boolean.txt:635-657, 672-689`, Fig. 12: truncating the Newton step at boundary `C_b`, continuing onto adjacent patch `S_1`, and solving `q_1, q_2`). Attached to **Roadmap F3, item 5** (`docs/yang_faithfulness_roadmap.md:178-179`: *"Finish §4.5.1 for interior points (`N90`): truncation at `C_b`, continuation on `S_1` (pinned, unwired), and `q_1`/`q_2`"*).

#### Subgroup 6c — Bounded Cylinder Outer-Cycle Mis-Selection by 3D `|Newell|` (`00107`)

- **Case & first failing prefix:** `X00000006-00107` (`scale = 8.083e0 m`, 4 `.waffle` ops / 4 steps): prefixes 1–3 `SUPPORTED_CORRECT`; fails at **prefix 4 (`Extrude`, `convex4:cut` subtract)**.
- **Inner raising site:** `crates/yang-rs/src/boolean/failures.rs:1183-1193` (`output face 0: loop 0 winds against the face's material side (oriented area -8.067e1)`), caused upstream by `select_outer_cycle` at `crates/yang-rs/src/stage5_topology/patches.rs:943-969`, then aborted at `iter 2` in `crates/yang-rs/src/boolean.rs:750-756` (`00107_p4.log:101`: `Stage-1 chart triangulation of face 0 cannot meet the face's chord budget after 4 domain-grid round(s) (worst interior edge 5.020× the budget)`).
- **Mechanism:** `output face 0` (descended from cylinder `A:0`) is a bounded non-encircling cylindrical patch pierced by an interior 4-sided window hole from `convex4:cut`. Neither cycle encircles `2π`, so `select_outer_cycle` (`patches.rs:943-969`) ranks cycles by 3D `|Newell|` magnitude. Because the outer cylindrical boundary wraps past `π` rad and folds back in 3D, its 3D `|Newell|` vector cancels below the flatter 4-sided window hole's `|Newell|`, swapping `outer_loop` and `inner_loops[0]` so `loop 0` has negative oriented `(u, v)` chart area (`-8.067e1`), invariant under `iter 0 (×2)` and `iter 1 (×4)` refinement.
- **Paper reference & attachment:** Yang 2025 §4.4.2 (`refs/text/yang2025_hybrid_boolean.txt:581-598`, boundary restoration in the face's 2D `(u, v)` parameter domain). Attached to **Ledger `N100` (item 4)** (`docs/yang_deviations.md:7822-7825, 7888-7890`: `select_outer_cycle` 3D Newell ranking on curved bounded faces).

---

### Group 7 — Mesh-Only `Plane × Cylinder` Coplanarity in Pre-Stage-4 `keep_set` Overlap Resolution (`00128`)

- **Case & first failing prefix:** `X00000006-00128` (`scale = 2.995e0 m`, 3 `.waffle` ops / 3 steps: `gear18:boss` → `nonconvex7:cut` → `circle:boss`): prefixes 1–2 `SUPPORTED_CORRECT`; fails at **prefix 3 (`Extrude`, `circle:boss` auto-union)**.
- **Raising site:** `crates/yang-rs/src/boolean.rs:2385` (`YangError::FaceResolutionFailed { tri: 721 }` inside `if la.surface[orig_t].len() > 1`, `boolean.rs:2338-2387`). Because `FaceResolutionFailed` converts to a faceless `ReassemblyStop` (`faces = []`), `§4.5.2` exits immediately at `iter 0` (`boolean.rs:724-729`: `no site names a face — nothing to refine`).
- **Failing operand (`yang-input` vs boolean):** **The boolean itself** (prefix 3 `Union`).
- **Measured Stage-2 mesh & `keep-probe` data (`scratch/logs/probes/{00128_p3,00128_s2_dump}.log`):**
  - Operand `A` (`1201 verts, 2398 tris`, 510 `Surface::Plane` faces) has **442** triangles (`t510..`) lying on its base plane `A f1` (`z = 0.0`, `normal = (0, 0, -1)`).
  - Operand `B` (`circle:boss`, `219 verts, 434 tris`, right circular cylinder `B f2` with `axis_point = (-0.275380634, 1.275711308, 2.633464055)`, `axis_dir = (0, 1, 0)`, `radius = 2.642619130`, dipping to `z_min = -0.009155076 m`) slices `z = 0.0` along two generator rulings at `x = -0.495159934` and `x = -0.055601335` (chord angle `≈ 0.1665 rad`). Stage-0/Stage-1 rim-crossing overrides place vertices at both `z = 0.0` crossings on `B`'s end rims (`v6, v7` at `y = 1.275711308` and `v33, v32` at `y = 5.754122575`) with no intermediate arc vertex on the shallow cap between them. As a result, `B`'s tessellated cylinder lateral mesh contains **4 flat chordal triangles lying bit-for-bit on `z = 0.0`**: `B t44 [6, 42, 7]`, `B t45 [32, 43, 33]`, `B t84 [42, 33, 7]`, and `B t87 [43, 7, 33]`.
  - `cherchi-rs` subdivides the `A f1 ∩ B f2` `z = 0.0` strip into **66** coplanar arrangement triangles with multi-solid label `la.surface = [InputId(0), InputId(1)]` (`patch 4: 1 tri`, `patch 5: 65 tris`, `kept = true`). Pre-Stage-4 sheet resolution (`boolean.rs:2338-2387`) looks up only `coplanar_pairs` (`Plane × Plane`) and `cyl_pairs` (`Cylinder × Cylinder`), finds no analytical pair for `Plane × Cylinder`, and raises `FaceResolutionFailed { tri: 721 }`.
- **Paper reference & attachment:** Yang 2025 §4.4.2 (`refs/text/yang2025_hybrid_boolean.txt:574-580`, inside/outside classification *after* §4.3 optimization and §4.4.1 mesh updating) and §4.5.4 (`:752-759`, discretization-induced illegal intersection resolved by local refinement). Attached to **Ledger `N80`** (`docs/yang_deviations.md:6609-6696`, pre-Stage-4 `keep_set` classification and raw-arrangement overlap-sheet resolution in `boolean_once`; also interacts with Roadmap F2 item 5 and F3 item 2 faceless-STOP attribution).

---

## 3. `TIMEOUT` Measurements (`PROSPECT_BUDGET_SECS=3600`, `YANG_STAGE_TIME_PROBE=1`, `ASSAY_JOBS=1`)

All four rows were re-judged one at a time in `--release` under `~/.claude/bin/reserve -c 2` with `PROSPECT_BUDGET_SECS=3600`, `YANG_STAGE_TIME_PROBE=1`, `ASSAY_JOBS=1`, and a `3600.0 s` wall-clock cap (`scratch/logs/timeout_{00004,00097,00159,00190}.log`).

### Harness Multi-Pass Multiplier (Why `00097` and `00190` Timed Out at `900 s`)

Inspecting `crates/test-harness/src/assay/prospect/{gen3.rs:459-469, mod.rs:444-489}` and `crates/test-harness/tests/prospect.rs:205-257`:
- In `prospect_run` (or `PROSPECT_SEED=6 PROSPECT_INDEX=<i>`, used for `00004` and `00159` because `seed-6/candidates/` had no `.waffle` on disk), `judge_generated` evaluates the boolean chain **up to 3× in one subprocess** against a single `PROSPECT_BUDGET_SECS` CPU budget:
  1. **Pass 1 (`gen3::generate`, `gen3.rs:459-469`):** builds steps `0..n_steps - 1` live in the kernel to sample solid AABBs — **before** writing `<id>.lineage.json`;
  2. **Pass 2 (`gen3::build`, `mod.rs:447`):** rebuilds steps `0..n_steps` from scratch to save `<id>.{waffle,meta.json}`;
  3. **Pass 3 (`categorize`, `mod.rs:459`):** replays `<id>.waffle` from scratch to run the oracle suite.
- In `PROSPECT_CANDIDATE=<stem>` (`measure_stem`, `mod.rs:456-489`, used for `00097` and `00190` whose `.waffle` files were already written on disk before the `900 s` kill in Pass 3), the harness evaluates the `.waffle` chain **2× in one subprocess**: Pass 1 (`categorize`) + Pass 2 (`builder.load` for `volume`/`chi`/`bodies`).

### Summary of `TIMEOUT` Runs

| id | judge mode | completes ≤ `3600 s`? | category | total wall / CPU | single-pass wall / CPU | dominant op & stage | attached to |
|---|---|---|---|---|---|---|---|
| `X00000006-00004` | `PROSPECT_SEED=6 PROSPECT_INDEX=4` | **No** (stopped at `3600 s` wall cap during Pass-2 step 4 `s2`) | `TIMEOUT` | `3600.18 s` wall / `3590.22 s` CPU | Pass 1 (`gen3::generate` steps 1–5): **`2875.82 s` wall / `2868.89 s` CPU** | Step 4 (`convex6:boss` `Union` into `gear21:rev`): `2408.39 s` (`before-441-update = 1417.37 s` [58.9%], `s2 = 897.19 s` [37.3%]); Step 3 (`star5:boss` `Union`): `449.13 s` (`before-441-update = 395.80 s` [88.1%]) | Roadmap F2, item 3 (`N81` pre-CDT Stage-4 repair/contact scans) |
| `X00000006-00097` | `PROSPECT_CANDIDATE` (`measure_stem`, 2 passes) | **Yes** | **`SUPPORTED_CORRECT`** (`bodies=3, chi=4, vol=51916.595`) | `1350.38 s` wall / `1348.82 s` CPU | Pass 1 (`categorize`): **`709.0 s` wall / `708.4 s` CPU**; Pass 2: `641.2 s` wall / `640.4 s` CPU | Op 5 (`convex5:boss` `Union` into `360°` `gear13:rev`): `677.10 s` pass 1 (`s2 = 480.90 s` [71.0%], `before-441-update = 164.41 s` [24.3%]) | Harness hole (`judge_generated` multi-pass evaluation under `900 s` cap) |
| `X00000006-00159` | `PROSPECT_SEED=6 PROSPECT_INDEX=159` | **No** (stopped at `3600 s` wall cap at end of Pass-2 step 3) | `TIMEOUT` | `3600.26 s` wall / `3589.85 s` CPU | Pass 1 (`gen3::generate` steps 1–5, all booleans): **`1960.68 s` wall / `1955.41 s` CPU** (`< 2000 s` full 6-step pass **inferred**, as step 6 is `Standalone`) | Step 3 (`convex6:cut` `Subtract` on `gear23:rev`): `1701.87 s` (`s2 = 845.00 s` [49.7%], `before-441-update = 764.23 s` [44.9%]); Step 5 (`nonconvex10:thru` `Subtract`): `162.36 s` (`s2 = 113.47 s`, `before-441-update = 36.70 s`) | Roadmap F2, item 3 (`N81` pre-CDT Stage-4 repair/contact scans) |
| `X00000006-00190` | `PROSPECT_CANDIDATE` (`measure_stem`, 2 passes) | **Yes** | **`SUPPORTED_CORRECT`** (`bodies=3, chi=6, vol=0.120103`) | `1044.08 s` wall / `997.18 s` CPU | Pass 1 (`categorize`): **`490.0 s` wall / `466.8 s` CPU**; Pass 2: `551.4 s` wall / `530.4 s` CPU | Op 7 (`circle:cut` `Subtract`) `§4.5.2` `iter 0..3` (`tris a=1,665,132 b=576` at `×16`): `475.94 s` pass 1 / `545.44 s` pass 2 (`iter 3` = `384.54 s` / `474.06 s`, led by `s2 = 236.46 s` / `318.12 s` [61.5% / 67.1%]) | Harness hole (`judge_generated` multi-pass evaluation under `900 s` cap) |

---

### Per-Case `TIMEOUT` Breakdowns

#### 1. `X00000006-00004` (`scratch/logs/timeout_00004.log`)
- **Recovered recipe (`scratch/timeout_out/candidates/X00000006-00004.lineage.json`):** 6 steps, `scale = 105.704 m`: (1) `convex4:boss` → (2) `gear21:rev` (`295.48°` around `+X`, 21 teeth) → (3) `star5:boss` → (4) `convex6:boss` → (5) `convex5:boss` → (6) `convex6:boss`.
- **Completion & verdict:** Did **not** complete within `3600.0 s`; killed cleanly at `3600.18 s` wall / `3590.22 s` CPU (`TIMEOUT`).
- **Stage timings:**
  - **Pass 1 (`gen3::generate`, steps 1–5) completed at `+2875.82 s` wall / `2868.89 s` CPU** (0 `§4.5.2` rounds on every op):
    - Step 3 (`op=Union tris a=136412 b=36`): `total = 449.13 s` (`pre_s2 = 1.45 s`, `s2 = 29.79 s`, `keep_resolve = 1.22 s`, **`s4_emit = 416.60 s`**, `failures = 0.42 s`), where `[s4-lap] before-441-update = 395.80 s` (`tris = 416460`), `before-453-sweep = 3.95 s`, `441-update = 1.79 s`, `before-3c-merge = 1.38 s`, `before-transit = 1.26 s`, `phase-a = 1.14 s`, plus `~9.4 s` post-wrapper B-Rep emission.
    - Step 4 (`op=Union tris a=138962 b=20`): **`total = 2408.39 s`** (`pre_s2 = 8.55 s`, **`s2 = 897.19 s`**, `keep_resolve = 3.24 s`, **`s4_emit = 1499.15 s`**, `failures = 0.59 s`), where **`[s4-lap] before-441-update = 1417.37 s`** (`tris = 580420`), `before-453-sweep = 19.36 s`, `441-update = 5.24 s` (`tris = 586794`), `before-transit = 2.13 s`, `before-3c-merge = 1.67 s`, `phase-a = 1.10 s`, `[i13d/i13e] = 34.80 s`, and B-Rep extraction `~15.5 s`.
    - Step 5 (`op=Union tris a=64268 b=16`): `total = 7.82 s` (`pre_s2 = 0.49 s`, `s2 = 0.64 s`, `s4_emit = 6.64 s`).
  - **Pass 2 (`gen3::build` replay from step 1, `+2875.82 s → +3600.14 s`):** re-evaluated Step 3 (`total = 377.16 s`, `s2 = 22.82 s`, `s4_emit = 352.06 s` [`before-441-update = 336.34 s`]) at `+3260.61 s`, then spent the remaining `339.5 s` in Step 4's `s2` arrangement until the `3600.0 s` wall cap.
- **Dominant stage & attachment:** Stage-4 `before-441-update` (`detect_improper_contacts`: `1813.17 s` = **63.2%** of Pass 1) and Stage-2 `s2` (`926.98 s` = **32.3%** of Pass 1) on the `136k–139k`-triangle 21-tooth revolved gear (`docs/kernel_1_0.md:72-77` Gate 4 / K10). Attached to **Roadmap F2, item 3** (`docs/yang_faithfulness_roadmap.md:139-142`, retiring pre-CDT Stage-4 repair/contact scans `N81`; paired with Roadmap F5 item 1 `L1`/`N61` surface discretization and amplified 3× by `judge_generated`).

#### 2. `X00000006-00097` (`scratch/logs/timeout_00097.log`)
- **Recipe (`/home/claude/workspace/target/prospect/seed-6/candidates/X00000006-00097.lineage.json`):** 5 ops / 5 steps, `scale = 4.352 m`: (1) `nonconvex6:sym` → (2) `circle:boss` → (3) `convex7:solo` → (4) `gear13:rev` (`360°`, 13 teeth) → (5) `convex5:boss`.
- **Completion & verdict:** **Completes — `SUPPORTED_CORRECT`** (`bodies = 3`, `chi = 4`, `volume = 51916.595298564935`, `exact_vs_mesh rel = -3.537e-8`).
- **Wall & CPU:** **`1350.38 s` wall / `1348.82 s` CPU** across `measure_stem`'s 2 passes (**Pass 1 `categorize` = `709.0 s` wall / `708.4 s` CPU**; Pass 2 `builder.load` = `641.2 s` wall / `640.4 s` CPU). *(Note: `os.wait4` raised `ChildProcessError` on line 91 of `run_timeout.py` only after `PROSPECT_OUTCOME` and `test result: ok. … finished in 1350.19s` had already printed on stdout.)*
- **Stage timings:** Op 2 (`circle:boss` `Union`) took `0.02 s`; **Op 5 (`convex5:boss` `Union` into `gear13:rev`, `tris a=106288 b=16`, 0 `§4.5.2` rounds)** dominated:
  - Pass 1: `total = 677.10 s` (`pre_s2 = 1.57 s`, **`s2 = 480.90 s` [71.0%]**, `keep_resolve = 1.63 s`, **`s4_emit = 192.90 s` [28.5%]**, `failures = 0.33 s`), plus `~31.4 s` post-boolean render tessellation/oracles. Inside `s4_emit` (`tris = 369808 → 383696`): **`before-441-update = 164.41 s`**, `before-453-sweep = 10.27 s`, `[i13d/i13e] = 10.92 s`, `441-update = 2.69 s`, `before-transit = 1.34 s`, `before-3c-merge = 1.01 s`, `phase-a = 0.82 s`.
  - Pass 2: `total = 639.14 s` (`pre_s2 = 1.51 s`, `s2 = 454.31 s`, `keep_resolve = 1.44 s`, `s4_emit = 181.78 s` with `before-441-update = 158.75 s`).
- **Dominant stage & attachment:** Stage-2 `s2` (`71.0%`) + Stage-4 `before-441-update` (`24.3%`) on the `106k`-triangle revolved gear (`docs/kernel_1_0.md:72-77`), but a single evaluation (`708.4 s` CPU) finishes **within** the `900 s` budget. Attached to **Harness hole** (`judge_generated` running `gen3::build` + `categorize` in one `900 s`-capped subprocess).

#### 3. `X00000006-00159` (`scratch/logs/timeout_00159.log`)
- **Recovered recipe (`scratch/timeout_out/candidates/X00000006-00159.lineage.json`):** 6 steps, `scale = 32.478 m`: (1) `nonconvex6:boss` → (2) `gear23:rev` (`233.43°`, 23 teeth) → (3) `convex6:cut` → (4) `convex5:boss` → (5) `nonconvex10:thru` → (6) `convex5:solo` (`Standalone`).
- **Completion & verdict:** Did **not** complete within `3600.0 s`; killed cleanly at `3600.26 s` wall / `3589.85 s` CPU (`TIMEOUT`).
- **Stage timings:**
  - **Pass 1 (`gen3::generate`, steps 1–5 — every boolean op in the recipe, since step 6 is `Standalone`) completed at `+1960.68 s` wall / `1955.41 s` CPU** (0 `§4.5.2` rounds on every op):
    - Step 3 (`op=Subtract tris a=122356 b=20` on `gear23:rev`): **`total = 1701.87 s`** (`pre_s2 = 5.36 s`, **`s2 = 845.00 s` [49.7%]**, `keep_resolve = 3.97 s`, **`s4_emit = 847.34 s` [49.8%]**, `failures = 0.59 s`), where **`[s4-lap] before-441-update = 764.23 s`** (`tris = 621172`), `before-453-sweep = 17.08 s`, `441-update = 5.80 s` (`tris = 642430`), `before-transit = 2.24 s`, `before-3c-merge = 1.60 s`, `phase-a = 1.25 s`, `[i13d/i13e] = 32.72 s`, and B-Rep extraction `~20.3 s`.
    - Step 4 (`convex5:boss` `Union` across 2 bodies): `op=Union tris a=65990 b=16` `total = 74.49 s` (`s2 = 24.35 s`, `s4_emit = 48.23 s` [`before-441-update = 40.39 s`]) + `op=Union tris a=6456 b=54000` `total = 2.41 s`.
    - Step 5 (`nonconvex10:thru` `Subtract`, `tris a=60540 b=36`): `total = 162.36 s` (`pre_s2 = 2.14 s`, `s2 = 113.47 s`, `keep_resolve = 0.63 s`, `s4_emit = 46.06 s` [`before-441-update = 36.70 s`]).
  - **Pass 2 (`gen3::build` replay from step 1, `+1960.68 s → +3600.21 s`):** re-ran Step 3's `s2` (`~841 s`) and Stage 4 (`before-441-update = 729.79 s`, `[i13d/i13e]` at `+3597.50 s`) when the `3600.0 s` wall cap fired. Because step 6 is a `Standalone` `convex5` extrude with no boolean, a single 6-step build would finish in `< 2000 s` (**inferred**), still above `900 s` on its own (`1955.41 s > 900 s`) and pushed past `3600 s` by `judge_generated`'s 3-pass structure.
- **Dominant stage & attachment:** Co-dominated by Stage-2 `s2` (`983.46 s` = **50.3%** of Pass 1) and Stage-4 `before-441-update` (`841.32 s` = **43.0%** of Pass 1) on the `122k`-triangle 23-tooth revolved gear (`docs/kernel_1_0.md:72-77` Gate 4 / K10). Grouped with `00004` under **Roadmap F2, item 3** (`docs/yang_faithfulness_roadmap.md:139-142`, retiring pre-CDT Stage-4 repair/contact scans `N81`; paired with Roadmap F5 item 1 `L1`/`N61` Stage-1 surface discretization and `judge_generated` 3× replay).

#### 4. `X00000006-00190` (`scratch/logs/timeout_00190.log`)
- **Recipe (`/home/claude/workspace/target/prospect/seed-6/candidates/X00000006-00190.lineage.json`):** 8 ops / 8 steps, `scale = 0.1351 m`: (1) `gear20:boss` → (2) `circle:boss` → (3) `nonconvex9:thru` → (4) `convex5:boss` → (5) `circle:boss` → (6) `gear19:cut` → (7) `circle:cut` → (8) `nonconvex8:solo`.
- **Completion & verdict:** **Completes — `SUPPORTED_CORRECT`** (`bodies = 3`, `chi = 6`, `volume = 0.12010284007703817`, `exact_vs_mesh rel = -4.570e-8`).
- **Wall & CPU:** **`1044.08 s` wall / `997.18 s` CPU** (`utime = 991.04 s, stime = 6.14 s`) across `measure_stem`'s 2 passes (**Pass 1 `categorize` = `~490.0 s` wall / `~466.8 s` CPU**; Pass 2 `builder.load` = `~551.4 s` wall / `~530.4 s` CPU).
- **Stage timings:** Ops 1–6 total `6.62 s` in Pass 1 (`2.15 + 0.26 + 0.08 + 2.39 + 1.74 s`). **Op 7 (`circle:cut` `Subtract`)** STOPped at natural and `§4.5.2` `iter 0..2` before emitting cleanly at **`iter 3` (`×16` density, `tris a = 1,665,132, b = 576`)**:
  - Pass 1 op 7 (`475.94 s` total): `natural = 0.56 s` (`a=7396 b=36`), `iter 0 = 2.21 s` (`a=26758 b=72`), `iter 1 = 13.12 s` (`a=104858 b=144`), `iter 2 = 75.51 s` (`a=415776 b=288`, `s2 = 31.06 s`, `before-3c-merge = 26.52 s`), **`iter 3 = 384.54 s`** (`s1_refine = 28.62 s`, `pre_s2 = 23.06 s`, **`s2 = 236.46 s` [61.5% of iter 3]**, `keep_resolve = 1.29 s`, **`s4_emit = 93.52 s` [24.3%, led by `441-update = 23.97 s`, `[i13d/i13e] = 13.22 s`, B-Rep `~20.9 s`, `before-3c-merge = 14.89 s`, `before-transit = 6.39 s`, `phase-a = 3.94 s`, `before-441-update = 2.73 s`]**, `failures = 1.26 s`).
  - Pass 2 op 7 (`545.44 s` total): `natural = 0.61 s`, `iter 0 = 2.31 s`, `iter 1 = 12.05 s`, `iter 2 = 56.41 s`, **`iter 3 = 474.06 s`** (`s1_refine = 17.38 s`, `pre_s2 = 19.96 s`, `s2 = 318.12 s`, `keep_resolve = 2.11 s`, `s4_emit = 114.35 s`, `failures = 1.73 s`).
- **Dominant stage & attachment:** Stage-2 `s2` (`61.5%` / `67.1%` of `iter 3`) on the `1.67M`-triangle whole-face `×16` refinement mesh (`L3`/`N62`, Roadmap F3 item 2, `refs/text/yang2025_hybrid_boolean.txt:659-677`), but a single evaluation (`~467–530 s` CPU) finishes **well under** the `900 s` budget. Attached to **Harness hole** (`judge_generated` multi-pass evaluation under a single-pass `900 s` CPU budget).

---

## 4. Post-Fix Re-Judgement (`agy/seed6-triage`: H1 & H2)

Rebased onto `origin/main` (`07cb9a4f`) and fixed both harness holes in `crates/test-harness` only:
- **H1 (`f310312e`):** `categorize::judge_empty_result` + conservative ball-covering `ExactChain::certified_empty_at` in `exact_membership::exact_volume_verdict` judge zero-live-solid results (`SUPPORTED_CORRECT` when certified empty, `SUPPORTED_WRONG` when non-empty / lost body, `ERROR` with reason when uncovered). Exact-chain coverage (`assay_exact_coverage`, **computed**): corpus `355 → 356 / 374` (`P0023` covered; thin-wall `C0034`, `C0117` still declined), seed-6 `195 → 197 / 198` (`00000`, `00095` covered; only `00043` remains `NotCovered`).
- **H2 (`db725aa7`):** removed two redundant evaluations (`gen3::generate` + `gen3::build` fused into single-pass `gen3::generate_and_build_with`; `prospect::measure_document` reuses `categorize_with_builder`'s loaded `ModelBuilder` instead of calling `builder.load` twice) and gave the unavoidable `AddFeature` build pass (`PROSPECT_BUILD_GENERATED` / `PROSPECT_BUILD_RECIPE`) and `LoadProject` verdict pass (`PROSPECT_CANDIDATE`) separate per-evaluation CPU budgets with `build_cpu_secs` and `judge_cpu_secs` recorded in `report.jsonl`.

### Computed Re-Judgements (`PROSPECT_BUDGET_SECS=900`, `PROSPECT_JOBS=4`, `prospect_run`)

All 6 rows below were **computed** end-to-end through `prospect_run` (`scratch/rejudge_seed6/report.jsonl`, `scratch/logs/h2_rejudge_seed6.log`):

| id | baseline (`dc422b05`) | post-H1+H2 verdict | `build_cpu_secs` | `judge_cpu_secs` | reported `cpu_secs` | detail |
|---|---|---|---|---|---|---|
| `X00000006-00000` | `ERROR` (`empty_result`) | **`SUPPORTED_CORRECT`** | `0.00 s` | `0.00 s` | `0.00 s` | `empty result, confirmed by exact_volume (exact 0.000000e0)` |
| `X00000006-00095` | `ERROR` (`empty_result`) | **`SUPPORTED_CORRECT`** | `0.09 s` | `0.09 s` | `0.09 s` | `empty result, confirmed by exact_volume (exact 0.000000e0)` |
| `X00000006-00097` | `TIMEOUT` (`> 900 s` combined) | **`SUPPORTED_CORRECT`** | `654.01 s` | `662.59 s` | `662.59 s` | `all checks passed` (`solids 3`, `exact_vs_mesh rel -3.537e-8`) |
| `X00000006-00190` | `TIMEOUT` (`> 900 s` combined) | **`SUPPORTED_CORRECT`** | `430.72 s` | `441.67 s` | `441.67 s` | `all checks passed` (`solids 3`, `exact_vs_mesh rel -4.570e-8`) |
| `X00000006-00004` | `TIMEOUT` | **`TIMEOUT`** | `900.02 s` | — (not run) | `900.02 s` | `build pass: cpu 900.0s > budget 900s` (partial 4-step `.lineage.json` flushed) |
| `X00000006-00159` | `TIMEOUT` | **`TIMEOUT`** | `900.01 s` | — (not run) | `900.01 s` | `build pass: cpu 900.0s > budget 900s` (partial 3-step `.lineage.json` flushed) |

### Seed-6 Totals Without a Full 200-Case Re-Run

- **Baseline (`dc422b05`, `/home/claude/workspace/target/prospect/seed-6/report.jsonl`, 200 cases):**
  `178 CORRECT / 6 WRONG / 12 ERROR / 0 UNSUPPORTED / 4 TIMEOUT`
- **Post-H1+H2 harness delta (6 rows computed above + 194 rows inferred unchanged from `seed-6/report.jsonl`):**
  **`182 CORRECT / 6 WRONG / 10 ERROR / 0 UNSUPPORTED / 2 TIMEOUT`** (`ERROR` rate `12/200 → 10/200 = 5.0%`; `TIMEOUT` count `4 → 2`).
  - **Why the 194 un-run rows are inferred unchanged under H1 + H2 alone:** H1 (`judge_empty_result` / zero-cube `certified_empty_at`) only fires when `live_solid_handles().is_empty()` or `exact_membership` reads zero inside cubes (`00000` and `00095` were the only 2 `empty_result` rows in `seed-6/report.jsonl`, and `h1_census_seed6.log` confirmed zero coverage changes across the other 196 `.waffle` files); H2 is verdict-preserving on non-`TIMEOUT` rows (`generate_and_build_with` is bit-identical to `generate` + `build`, and `measure_document` reuses the same loaded `ModelBuilder`), while all 4 `TIMEOUT` rows were re-run above.
  - **Note on concurrent `main` changes outside `dc422b05`:** `origin/main` (`07cb9a4f`) also includes commits landed after `dc422b05` (e.g. `docs/kernel_1_0.md:93` records `X00000006-00021` moving `WRONG → CORRECT`, which would make a full fresh re-run on current `main` + H1 + H2 **inferred** `183 CORRECT / 5 WRONG / 10 ERROR / 0 UNSUPPORTED / 2 TIMEOUT` pending any other `main` oracle/engine changes not re-evaluated here).

> **Reviewer notes on H1 and H2 (Claude Code, 2026-11-03).** (1) H1 adds a second new WRONG path besides the
> one in the brief: a NON-empty kernel result whose chain certifies empty is now a Flag (WRONG), where it
> used to be NotCovered — the kernel made material where none should be. (2) H2 splits the build and judge
> passes into separate budgeted processes for `prospect_run`; the `PROSPECT_RECIPE` and default
> `judge_generated` entries still evaluate twice in one process, so a budgeted replay through them covers
> two passes. Coverage census on this branch: corpus 356 / 16 / 2 (main 355 / 17 / 2; P0023 newly covered).
