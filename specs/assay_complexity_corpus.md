# Assay Complexity Corpus (C-series, C0001–C0100)

**Status**: approved 2026-07-05 (session design review with Sequoia)
**Owner**: test-harness (`crates/test-harness/src/assay/gen_complexity.rs`)

## Goal

Extend the categorized kernel-v2 assay corpus (194 cases) with 100 curated,
deterministic, higher-complexity cases that target coverage gaps found by
comparing (a) what the kernel claims to support, (b) what the feature engine
can express, and (c) what the existing corpus actually exercises.

Every case is one of two kinds:

- **Bug hunter** — inside the declared capability boundary. Expected category
  `SUPPORTED_CORRECT`. A failure is a REAL finding (named, documented, never
  pinned aspirationally).
- **Milestone tracker** — outside the boundary, named after the milestone that
  will flip it (`M8`, `M5`, `KV6`, `KV7`, CDT tail). Expected category
  `UNSUPPORTED(...)`/`ERROR`/`TIMEOUT` today. When the milestone lands, the
  case flips green in the same PR (existing quarantine-tag convention).

## Research basis

- P1/P2/P6 (Engineering Constitution): numeric/structural oracles, spec first.
- A15.6 (Yang 2025 hybrid pipeline) — Stage-0 coplanar classes, tangency
  classes, degree-4 SSI walls define the tracker families.
- A14 (units/tolerance) — the mixed-scale family probes MIN_FEATURE_SIZE
  (1e-6 m) and TAU_MODEL (1e-7 m) against a large body, which uniform-scale
  cases cannot.
- Euler characteristic χ = 2 − 2g for closed orientable genus-g shells; χ
  totals add per shell (existing per-shell oracle convention).

## Oracle extensions (schema)

`OracleExpectations` gains two optional fields (serde-defaulted; the 194
legacy metas are byte-identical and untouched):

- `expected_volume: Option<f64>` — exact analytic volume of the final body
  (or bodies, summed). Checked as `|vol − expected| ≤ tol_rel · |expected|`.
- `expected_volume_tol_rel: Option<f64>` — default **1e-3** when absent.
  Curved-profile cases set 0.05 (tessellation chord error at the runner's
  scale-adaptive tolerance).
- `expected_solid_count: Option<usize>` — deliberate multi-body cases
  (Group 3a) declare their intended body count; when absent the legacy
  "multi-op ⇒ 1 merged solid" check applies unchanged.

Volumes are computed at generation time from kernel-independent arithmetic
(shoelace polygon area × depth; axis-aligned inclusion–exclusion for
overlaps; Pappus for revolves) so the oracle is not circular.

The `assay_kv2` replay checks `expected_volume`/`expected_solid_count` in its
validation step (step 5). Category semantics are unchanged.

## Generation mechanics

- New module `gen_complexity.rs`, entry `generate_complexity_cases(dir)`.
- Deterministic: hand-parameterized specs, no RNG.
- Its own writer (NOT `write_featured_case`): the no-op repair
  (`fix_noop_operations`) must never move C-series geometry, because the
  meta's `expected_volume` is derived from the authored coordinates. The
  independent `assay_noop_guard` still scans C-series files; cases are
  designed to satisfy it by construction (every op changes volume; no boss
  swallows the body; first cuts breach a face unless described as
  `internal-void`).
- The generator bin gains `--complexity-only` mode: writes C-series files and
  MERGES their entries into the existing manifest without regenerating the
  194 legacy files (regeneration mints fresh UUIDs → full-corpus churn).
- `assay_kv2::full_corpus_categorized` corpus-count assertion 194 → 294.
- ~15 representative C-cases pinned in `smoke_corpus_boundary_categories`.

## Families

### Group 1 — bug hunters inside the boundary (expected SUPPORTED_CORRECT)

**1a. Genus-N topology — C0001–C0012.** Plates and boxes with multiple
through-holes. All boolean contacts interpenetrate (no coplanar operand
pairs): cut sketches sit strictly above the top face, cut depth overshoots
the bottom. Oracles: exact χ = 2 − 2g pinned in `euler_target`, exact volume.
- C0001–C0004: 2/3/4/5 square through-holes in a 4×4×0.5 plate (g = 2..5).
- C0005–C0006: 2×2 and 3×3 hole grids (g = 4, 9).
- C0007: crossing orthogonal tunnels with OFFSET cross-sections (z-ranges
  differ so tunnel walls are NOT coplanar) → g = 3, χ = −4.
- C0008: blind pocket + lateral through-hole breaching the pocket wall (g=1).
- C0009: U-tunnel (two vertical bores + horizontal connector cut) g = 1.
- C0010: 3-level offset boss tower, one through-hole per level (g = 3).
- C0011: plate with 2 holes, then a boss bridging OVER one hole (arch) —
  genus preserved g = 2.
- C0012: staggered 5-hole pattern (g = 5).

**1b. Interleaved boss/cut chains — C0013–C0020.** Alternating boss and cut,
8–16 ops, each boss interpenetrating (never face-flush), each cut removing a
hand-computed axis-aligned intersection. Running exact volume. 8/10/12/16 ops
× 2 drift patterns (lateral staircase, spiral-ish quadrant walk).

**1c. Non-convex profile booleans — C0021–C0028.** Star (5/7-point), comb
(20 teeth, 40 reflex vertices), keyhole, rectangular spiral (3 turns),
zigzag ribbon, plus-with-notches. Boss extrudes with rect through-cuts whose
tools lie entirely inside solid material regions (removed volume = tool area
× thickness, exactly computable). Shoelace area × depth for the boss volume.

**1d. Near-degeneracy — C0029–C0040.** All above the 1e-6 m feature floor.
- C0029–C0031: through-cut wall passing ε from the body's side face,
  leaving a sliver wall: ε = 1e-3, 1e-5, 2e-6 (× body 1 m).
- C0032–C0033: sliver profiles: 1 × 1e-4 needle boss on a box; needle cut.
- C0034–C0035: thin-wall remainders: square tube wall 1e-4; U-channel floor
  1e-4.
- C0036–C0037: near-coplanar tilts: second boss on a plane tilted 0.001° /
  0.0001° from the first's top plane, interpenetrating 1e-3 deep (the
  Stage-0 coplanar gate must NOT fire; op must succeed exactly).
- C0038–C0040: mixed scale in one model: 1 m box with 10 µm through-hole;
  1 m box with 20 µm standing rib; 100 m slab with 1 mm through-hole.

### Group 2 — milestone trackers (expected UNSUPPORTED/ERROR/TIMEOUT today)

**2a. M8 coplanar residue — C0041–C0050.** [M8]
- C0041: crossing tunnels with IDENTICAL cross-sections (coplanar tunnel
  walls — the in-boundary twin is C0007).
- C0042/C0043: externally / internally rim-tangent coplanar disc caps.
- C0044: opposite-normal annular cap on disc (tube stacked cap-to-cap).
- C0045/C0046: edge-only and corner-only box contact (1D / 0D contact).
- C0047: holed-disc partner overlap (annular cap partially covering a hole —
  task #54 flavor).
- C0048: two swiss-cheese plates stacked cap-to-cap (chained holed coplanar).
- C0049: flush cut (cut tool sharing a side wall with the body).
- C0050: 3-box staircase, each sharing a partial coplanar top strip.

**2b. Degree-4 / tangency cyl×cyl — C0051–C0058.** [M5 / KV9-tangency]
Unequal radii perpendicular (union + subtract), unequal 45°, skew axes,
parallel external tangency, parallel internal tangency, near-tangent
(gap 1e-6), equal-radius 30° oblique crossing (dual-ellipse solver probe —
may already pass; boundary-mapping either way).

**2c. Revolve compositions — C0059–C0070.** [KV6]
Partial-revolve boss + through-cut; square-torus ring + axial bore; revolve
CUT groove on an extruded shaft; coaxial revolve-on-revolve (interpenetrating,
not flush); cone (triangle revolve) + oblique box cut [KV6c-oblique]; stacked
frusta; full torus + box cut [torus-recovery]; 90° partial torus + cut;
sphere (circle revolve, axis through center) + box cut; washer (holed
rect revolve) — where a holed revolve profile is expressible; lathe part
with 3 grooves; revolve boss on tilted axis + extrude cut.

**2d. Multi-shell re-entry — C0071–C0074.** [KV7]
Internal-void body (marked `internal-void` for the guard exemption) then:
cut breaching the void; boss on the outside; explicit BooleanCombine union
bridging two disjoint bodies; intersect with a half-space box.

**2e. Gear / CDT tail — C0075–C0078.** [CDT-tail]
12-tooth gear × rotated gear crossing union at scale 1; coaxial gear ring
(gear boss + gear cut); 40-tooth gear single extrude (pure CDT, no boolean);
gear + rect through-cut at scale 1 (fast variant of the pathological
R0007 class).

### Group 3 — dispatch-path parameter space (expected SUPPORTED_CORRECT)

**3a. Explicit combine modes / targets — C0079–C0084.**
NewBody × 2 + BooleanCombine union (bridging overlap); three bodies + cut
with explicit target on body 2 only; Intersect combine-mode extrude; Add
combine with explicit non-most-recent target; NewBody overlapping existing
(stays 2 bodies — `expected_solid_count: 2`); BooleanCombine subtract.

**3b. Depth modes / directions — C0085–C0090.**
Symmetric boss; symmetric cut; second_direction two-sided asymmetric boss;
ThroughAll cut; explicit reversed `direction`; symmetric boss + ThroughAll
cut combined.

**3c. Holed / multi-profile sketches — C0091–C0096.**
Annular one-op extrude (outer rect + inner rect hole, KV14 path); rect with
3 holes one-op; two disjoint profiles in one sketch, extrude
`profile_index = 1`; two extrudes off one sketch (index 0 then 1); holed
profile boss + through-cut; holed circle (washer) one-op extrude
(curved, tol_rel 0.05).

**3d. Region extrudes — C0097–C0100.**
Annulus region (two concentric circles); lens region (two overlapping
circles); crescent region; two adjacent rect sub-regions extruded as one
body (`regions` plural path). Curved regions use tol_rel 0.05.

## Failure modes / expected errors

- Group 2 cases are EXPECTED to land in typed non-green categories; the
  baseline run records the honest category per case and pins representatives.
- A Group 1/3 case that fails at baseline is a finding: documented in the
  baseline commit message and left honest (its meta still carries the true
  expected geometry; the category table shows the miss). It must NOT be
  re-authored to dodge the bug, and must NOT block landing the corpus.

## Baseline findings (2026-07-05, first full run)

The corpus found three real defects on its first run:

- **C0079-F1** — multi-target `Add` with DISJOINT explicit targets `[A, B]`
  silently drops body B: no error, no warning, one output solid, volume
  1.625 (= A∪tool) instead of 2.5 (= A∪B∪tool). Silent material loss in the
  optional-booleans multi-target path. Repro: `C0079`.
- **C0035-F1** — *reclassified 2026-07-06: AUTHORING ERROR, not a kernel
  defect.* The cut depth was written `3.0 − 1e-4` (copying C0034's
  through-depth) but the cut sketch sits at z=2 over a z∈[0,1] body, so
  that depth reaches z=−0.9999 — a geometric through-cut. The meta was
  self-contradictory: its exact-volume field (0.36) encoded the through-cut
  while its χ pin (2) encoded the floor; the kernel matched the authored
  coordinates exactly (χ=0, vol 0.36). Replaying the *intended* geometry
  (depth `2.0 − 1e-4`) shows the kernel preserves the 100 µm floor
  correctly (χ=2, vol 0.360064 exact) — A14.2 holds. The case was
  regenerated with the intended depth and its pin flipped to
  `SUPPORTED_CORRECT` (the meta's volume/χ oracles now agree). Lesson: a
  Group-1 finding must be validated against the *authored coordinates*
  (chain-volume vs χ consistency) before being attributed to the kernel.
- Boundary corrections — several designed trackers are SUPPORTED today
  (capability better than documented): same-section crossing coplanar
  tunnel walls (C0041), external rim tangency (C0042), edge-only box
  contact (C0045), holed-disc partner (C0047, the task-#54 class), flush
  cuts and partial-overlap chains (C0049/C0050), parallel lateral tangency
  and 1e-6 near-tangency (C0055/C0057), partial revolve/torus + bore
  (C0059/C0060/C0066), washer flange genus-5 (C0068), all four gear/CDT
  cases including the 40-tooth CDT stress (C0075 is coplanar-walled;
  C0076–C0078 pass). The M5/KV6 walls that remain: unequal-R and oblique
  degree-4 CUTS (C0052–C0054, C0056), equal-R oblique union (C0058),
  revolve-cut grooves and revolve-on-revolve (C0061/C0062/C0069), oblique
  cone cut (C0063), frusta chain (C0064), torus/sphere booleans
  (C0065/C0067 — typed revolve walls), tilted-axis revolve (C0070), and
  KV7 multi-shell re-entry (C0071–C0074).

Two authoring errors found by the same run were fixed in-generator (C0051
bbox bound; C0074 missing explicit Intersect target — cf. C0081) and the
corpus regenerated; generator UUIDs are deterministic (FNV-based) so
regeneration is byte-stable.

## Runtime budget

Designed for ≤ ~5 s/case typical (scale ≈ 1, modest profiles; no microscale
gears). 2b/2e may time out — acceptable; the slow-list mechanism already
handles them. Estimated full-run cost: +8–15 min.
