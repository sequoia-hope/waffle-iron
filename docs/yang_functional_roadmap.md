# Yang Functional Roadmap — Single Source of Truth

> **Status:** authored 2026-05-28. This document supersedes the per-crate
> `PLAN.md` concept for the Yang effort (there are none in the new crates) and
> supersedes the stale "Current architecture (what's built)" block that used to
> live in the root `CLAUDE.md`. When this roadmap and a crate `CLAUDE.md`
> disagree on sequencing, this roadmap wins; when this roadmap and the Yang 2025
> paper disagree on *algorithm*, the paper wins (see `docs/yang_deviations.md`).

## 0. Honest status (refreshed 2026-06-26)

The kernel rewrite is **live in the app**. The legacy `crates/kernel/` is
DELETED; the app, feature-engine, and all tests run on `kernel-v2` through the
`Kernel`/`KernelIntrospect` traits, and the WASM bundle builds on **stable
Rust**. M0–M7 are COMPLETE, the Phase-6 migration shipped (2026-06-11), and the
curved-geometry stack (cylinder/cone/torus, partial+full revolve) is in
production. The native `cherchi-rs` Stage 2 (arrangement + boolean labeling +
clean-room indirect predicates, WASM-clean) is the production backend; the C++
sidecar and the LGPL IP FFI are dev-only parity oracles. `ssi-rs` drives the
exact analytical intersection curves.

So the foundations are deep and the boolean works across a broad range of real
geometry. **What is NOT yet faithful to the paper is the remaining set of Yang
deviations** (`docs/yang_deviations.md`) — and per the project's standing rule
("Paper-Spec Compliance is MANDATORY; deviations are errors"), closing those, not
chasing a score, is the work.

### 0.1 Posture (2026-06-26 directive): implement Yang faithfully; deviations are errors

The plan of record is **the paper**. We implement what Yang 2025 describes,
*generally*, not the narrowest special case that moves a metric. Two consequences
that override older phrasing in this document:

- **The assay is a regression detector, not a goal.** `SUPPORTED_CORRECT` counts
  prove we didn't break working geometry and help localize failures; they are NOT
  the target and we do NOT prioritize work by "which increment bumps the score."
  A faithful Yang stage that lights up zero new corpus cases is still correct work
  and ships; a score-chasing special case that papers over a structural deviation
  does not. (This reframes the per-slice "assay N→M" framing throughout §4/§4b:
  those numbers are history, not the objective.)
  *(2026-07-05: the corpus grew 194 → 294 with the curated C-series complexity
  cases — genus-N topology, long interleaved chains, near-degeneracy, and named
  milestone trackers per family: [M8] C0041–C0050, [M5/KV9] C0051–C0058,
  [KV6] C0059–C0070, [KV7] C0071–C0074. Trackers flip green when their
  milestone lands — un-pin them in the same PR. Group 1/3 cases carry EXACT
  volume oracles (`expected_volume` in the meta). Spec:
  `specs/assay_complexity_corpus.md`.)*
- **General over piecemeal.** Where the paper gives one algorithm for a family of
  cases (coplanarity, face provenance, mesh updating), we implement that one
  algorithm — not a lattice of shape-specific handlers (planar-only,
  opposite-normal-only, non-holed-only, single-pair-only) each with its own loud
  wall. The piecemeal Stage-0 slices (YR25–27, disc/gear specializations) were
  real machinery progress but are explicitly an interim scaffold to be subsumed by
  the general §4.5.5 implementation below.

### 0.2 Open paper-faithfulness deviations (the real backlog)

These are the substantive divergences from Yang 2025 in the live `yang-rs`
pipeline (full entries in `docs/yang_deviations.md`). They — not the assay — are
the roadmap's remaining work:

1. **Stage 0 §4.5.5 coplanarity is NOT general** (the keystone; see §4 M8 below).
   Today a lattice of shape/normal-specific handlers ships and everything else
   walls loud (`CoplanarFacesUnsupported`): same-normal overlaps, holed faces,
   curved coplanar surfaces, a face in >1 pair, intra-solid near pairs. The paper
   specifies ONE general 2D-Boolean-before-discretization with a single shared
   trimmed surface and identical meshes for both models — **no same/opposite
   distinction, any surface type** (§4.5.5; Fig. 16).

   > **M8 wall decomposition (probed 2026-07-02, `YANG_COPLANAR_PROBE` full
   > corpus): the 22 remaining `UNSUPPORTED(coplanar-boolean)` cases resolve
   > into exactly FIVE mechanisms**, two of which shipped the same day:
   > 1. ✅ **Intra-solid opposite-normal femto-negated step pairs** (10 cases)
   >    — sign-aware sibling-plane canonicalization (kernel-v2) + exactly-
   >    negated benign intra exclusion (yang-rs scan). Spec
   >    `specs/m8_intra_opposite_plane_canonicalization.md`; commits
   >    3dd32340 (RED) → 36d36789 (GREEN) → e0b422fa (adversary). R0064 →
   >    SUPPORTED_CORRECT; R0022/R0031/R0025/R0061 et al. progress to their
   >    next honest wall; several become >30s-cap TIMEOUTs (real exact-
   >    arithmetic compute, the known gear-perf class).
   > 2. ✅ **Non-star subdivided neighbor rings** (4 walled + 3 TIMEOUT-class)
   >    — exact closed-containment ear-clip fallback + B6 exact consecutive-
   >    duplicate collapse in `triangulate_ring`. Spec
   >    `specs/m8_nonstar_ring_earclip.md`; commits 69f3c8a8 (RED) →
   >    18dea32f (GREEN) → f5386e56 (B6 oracle). R0098 clears end to end.
   > 3. **Femto-twin shared-boundary identity — PARTIALLY SHIPPED
   >    (2026-07-02, spec `specs/m8_shared_boundary_identity.md`).** Root
   >    measured as TWO layers, not cross-solid constraints (§8 P10 record):
   >    (a) chained-output vertices femto-off their canonical planes, and
   >    (b) the f64 FRAME PROJECTION minting femto-split coordinates for
   >    OBLIQUE solids even on consistent world coords. **Shipped +
   >    assay-certified (0 WRONG, quiet box): §2b in-frame coordinate
   >    clustering for pure-polygon pairs** (04f14094 + adversary bbbcd407)
   >    — R0070, R0076, R0081 clear the wall (with the earlier ear-clip,
   >    R0098 too). **Banked, deliberately UNWIRED: the world-space vertex
   >    canonicalization** (49a373eb primitive + unit suite; unwired
   >    3cf6f40b) — wiring it flipped R0064/F0047 to SUPPORTED_WRONG (§8a):
   >    its femto motion drives `tessellate_cylinder_patch` into sub-f32
   >    slivers. **The F0047 hole is now GATED LOUD** (spec
   >    `specs/kv2_patch_render_degeneracy_gate.md`, 9eede895→80f3cc73:
   >    always-on f32 render-precision gate; I2 assay-verified
   >    category-neutral). **Root fix SHIPPED 2026-07-03 (spec
   >    `specs/kv2_cdt_triangulation_core.md`, 7c4641ba…f8d68ac1): both
   >    kernel-v2 render cores (cylinder patch + planar) moved from greedy
   >    exact ear-clip + f64 flip to the exact-predicate CDT (cherchi-rs
   >    flood-fill variant via a yang-rs re-export) with a grid-degeneracy
   >    flip pass (M1, cocircular ties), flood-fill interior classification
   >    (M2), pinch/keyhole ring splitting + shared-vertex welding (M3a/b/c),
   >    and the planar G1 gate. Full-assay diff vs baseline = F0042
   >    ERROR→CORRECT only (82 CORRECT / 0 WRONG). Re-wire experiment
   >    re-run (§8a-ii): NO silent-WRONG remains under the world-space
   >    pass and the R0046/R0088/F0063 coplanar walls lift into deeper loud
   >    errors, but F0016/F0024 flip CORRECT→loud Stage-6
   >    "reassembled output would be non-2-manifold" (net −1 CORRECT) ⇒
   >    unwired at that point.** **THEN (same day): the Stage-6 class was
   >    root-caused and fixed (spec `specs/yang_stage6_sliver_topology.md`,
   >    8064537f; deviation N22: fold-sliver exclusion from patch boundary
   >    walks + loop T-subdivision at on-segment foreign vertices), and the
   >    world-space pass is now PERMANENTLY WIRED (14bc23ce, m8 §8a-iii):
   >    re-wire gate 83↔83 CORRECT / 0 WRONG / zero lost; F0022
   >    ERROR→CORRECT even unwired. Assay now 83 CORRECT.** Also new:
   >    KV9-F3 output seam femto-twin quarantine (spec
   >    `kv2_cdt_triangulation_core` §6a — an output-identity defect, §8b
   >    target). **2026-07-03 (same day, two more cycles): §2c rim-aware
   >    clustering SHIPPED (781e2e2e — polygon-chain domain, rim samples
   >    excluded; F0061's Stage-0 coplanar wall closed) and the
   >    LabelMismatch class RETIRED via reference parity
   >    (`specs/cherchi_patch_label_tolerance.md`, 0f9e2982: the C++
   >    label-homogeneity assert is debug-only; L2a subset-compatible
   >    floods proceed, L2b disjoint stays loud, deviation N23; assay
   >    83/0, zero lost).** **Item (v) SHIPPED 2026-07-03 (spec
   >    `specs/m8_stage0_inputcheck_clean_emission.md`): Stage-0
   >    inputcheck-clean overlap emission.** Increment-0 measurement (new
   >    per-operand diagnosis: `YANG_STAGE0_DUMP_DIR` dump +
   >    `cherchi_rs::inputcheck` native five-axiom census + sidecar
   >    oracle) proved ALL violations Stage-0-INTRODUCED (every pre mesh
   >    clean; the F0063 "chained-inherited" hypothesis disproven) and
   >    localized TWO mechanisms: **M-A** — the §2b/§2c clustering rewrote
   >    the overlay's 2D domain while `collect_edge_splits` re-projected
   >    edge endpoints RAW, so its exact collinearity test silently
   >    dropped every boundary split on a clustering-moved edge (holes +
   >    T-junction improper contacts; F0063 op0: 567 boundary edges);
   >    **M-B** — the many-to-one 2D→3D vertex resolution emitted
   >    `[u,u,v]` degenerate triangles (+ orphaned vertices that SEGFAULT
   >    the reference inputcheck binary). Fixes: endpoint projections
   >    routed through the pair's cluster map; resolved-degenerate
   >    triangles dropped at emission; unreferenced-vertex compaction.
   >    **Outcomes: F0063 → SUPPORTED_CORRECT end-to-end; F0016/F0024 +
   >    F0061 land CORRECT; assay 96 CORRECT / 0 WRONG / zero lost;
   >    corpus-wide operand sweep
   >    (`docs/audits/stage0_operand_inputcheck_sweep_2026-07-03.tsv`):
   >    356 Stage-0 operands, only 10 with introduced defects.**
   >    Remaining after (v): (i-residual) R0046 progresses to a NEW loud
   >    kernel-v2 wall (`InvalidBooleanOutput("output loop with fewer
   >    than 3 edges...")`); ~~R0088 keeps ONE edge-pairing instance —
   >    measured **M-C**~~ **M-C SHIPPED 2026-07-04 (spec
   >    `specs/m8_stage0_band_scale_crossing_verts.md`): the root was the
   >    rim-override insertion's ANGULAR merge_tol dedup silently dropping
   >    the second of each band-close override twin pair (R0088-a: 4+4
   >    drops = the 12 boundary edges + pinch; R0070-b: 2 ULP-twin drops).
   >    Fix = exact bit-identity dedup (a tolerance REMOVAL). R0088's
   >    edge-pairing wall GONE (operands five-axiom clean; op now stops
   >    loudly at `EmptyBooleanResult`); R0070's defective op stops loudly
   >    pre-backend (`azimuth-merge rims have mismatched samples` — the
   >    opposite-rim f64 azimuth projection collapses ULP twins; named
   >    follow-up: exact opposite-rim projection).** **Fold-pair emission
   >    class SHIPPED 2026-07-04 (spec
   >    `specs/m8_stage0_fold_pair_emission.md`): the disc-pair builders'
   >    angle-only annulus merge fanned inner chords to outer corners on
   >    the CENTER side of the chord's line (silently orientation-flipped
   >    into same-side overlap pleats — F0027/28/29, R0007, R0023, R0038,
   >    R0089). Fix = exact half-plane visibility guard on each advance +
   >    exact shoelace coverage certificate, loud
   >    Wall("disc-annulus-tri") on deadlock. F0029 + R0089 lose their
   >    E2E ERRORs outright; R0038 drops to an M5-class Stage-3 SSI wall;
   >    all sweep introduced-dirty operands now emit clean.**
   >    **(iii) KV9-F3 output vertex identity SHIPPED 2026-07-04 (spec
   >    `specs/kv9_f3_output_vertex_identity.md`): unmoved junction
   >    duplicates (both twins already on-curve within TAU_WORK) escaped
   >    the §4.4.1(b) sub-feature merge's moved-only scan — eligibility
   >    extended to conic-endpoint-touching triangles (criterion stays the
   >    MIN_FEATURE_SIZE floor); plus from_yang_brep now accepts genuine
   >    2-arc LENS BIGONS on distinct curves (the twin artifact had been
   >    masking them; CurveKey pairing already supported them). Both KV9-F3
   >    kv9 quarantines un-quarantined; F0041 + F0057 ERROR→CORRECT;
   >    assay 88/0 zero lost.** **KV9-F1 layer 1 SHIPPED 2026-07-04
   >    (spec `specs/kv9_f1_tangency_inout_labels.md`, deviation N24):
   >    the steinmetz "tangency crack" was measured down to a PREDICATE
   >    soundness hole — Shewchuk adaptive orient3d/orient2d certify a
   >    FALSE Zero under subnormal UNDERFLOW (the in/out ray's graze on a
   >    femto-skewed seam edge), silently discarding ALL of input B. Fix
   >    = exact-rational zero-certification in the wrappers (only exact
   >    arithmetic certifies Zero). F0056 ERROR→CORRECT; steinmetz
   >    progresses to a LOUD Stage-4 OffCurveBeyondChordBand at a
   >    tangency-adjacent vertex — the ellipse-junction relocation at
   >    tangency grade is the named next increment (kv9 quarantine tags
   >    updated).** **KV9-F1 Increment 0c SHIPPED 2026-07-04 (spec §2c):
   >    a `vert_ell_junction` whose two ellipses name the SAME unordered
   >    cylinder pair is always the pair's surface-tangency point; the
   >    pinch vertex's standoff is second-order (√(2rB)+B derived band,
   >    B = combined chord budget) — gate converted from the
   >    inapplicable first-order 2·d_ε/|d̂·r̂| line metric (which stays
   >    byte-identical for the KV11 box-edge class). Steinmetz SUBTRACT
   >    now clears yang-rs with the exact-volume oracle
   >    (yang-rs kv9f1_tangency_junction trio green). Named next walls
   >    (kv9 tags updated): (a) UNION stops at Stage-6
   >    s6-curved-degenerate-loop — extract_boundary_cycles interleaves
   >    the top/bottom lens cycles at the now-4-valent tangency junction
   >    into a Newell-cancelling figure-eight; junction-aware
   >    continuation pairing needed. (b) SUBTRACT walls at kernel-v2
   >    import NonManifoldVertex — FOUR elliptical arcs share both
   >    endpoints (two per ellipse), defeating vertex-pair edge keying
   >    (the M8 disc∩disc CurveKey lesson, now same-curve bigons). New
   >    kept probes: KV9_JUNCTION_PROBE, NONMANIFOLD_SITE_PROBE
   >    (self-localizing NonManifoldOutput gates).** Still open:
   >    (iv) R0078 azimuth/coplanar. The P10 records stand:
   >    no mesh-level kept-set gate (`yang_kept_mesh_manifold_gate.md`
   >    §2b); kernel-v2's post-subdivision edge pairing stays the honest
   >    downstream wall.
   > 3a. **MIXED Line+Arc planar faces SHIPPED 2026-07-09 (spec
   >    `specs/m8_mixed_loop_coplanar_overlay.md`, ad88e942)** — the
   >    2026-07-09 residue census's largest class (`face-unsupported`:
   >    R0021 R0026 R0051 R0059 F0075). Planar faces mixing LineSegment +
   >    Circle edges enter the general overlay: loops spliced from Stage 1's
   >    own chains (`loop_polyline_attributed`), one `RimChordCtx` per
   >    curved edge, `collect_mixed_crossings` propagating splits into the
   >    arc chain AND the strip lateral's opposite arc (exact axial
   >    projection), Stage-1 arc-chain `rim_overrides` insertion, and
   >    straight-edge-only split keying (the semicircle-arc/diameter-chord
   >    vertex-pair collision). `build_disc_pair` excludes mixed partners
   >    (chord-geometry hazard closed). Amendment 1 P10 record in the spec:
   >    the planned wall-on-curved-subdivision slice was dead on arrival
   >    (the trapezoidal overlay subdivides curved chords at every partner
   >    event column) — the full disc-stack generalization was required.
   >    Assay 216 CORRECT / 0 WRONG, zero lost: F0075 → CORRECT, R0059 →
   >    typed ring-reject ERROR. Remaining sub-walls (typed):
   >    `mixed-arc-lateral-not-cylinder`, `mixed-arc-lateral-unpaired`
   >    (hole-free Slice-D laterals — the increment-2 one-sided mechanism
   >    plausibly applies, no corpus case targets it), Ellipse edges
   >    (`face-unsupported`, no ellipse mint).
   > 3b. **Increment 2 — HOLED (chain-consuming) laterals SHIPPED
   >    2026-07-10 (spec `specs/m8_mixed_arc_lateral_holed.md`, 36ee1b88).**
   >    `mixed-arc-lateral-holed` lifted: a holed cylinder lateral takes the
   >    KV14 unroll+CDT path, which splices its boundary loops from the
   >    shared per-edge chains (`loop_polyline`) — no strip index-pairing
   >    constraint, so the crossing split point inserts ONE-SIDED into the
   >    arc's own chain. `arc_lateral_opposite` classifies Strip vs
   >    ChainConsuming and verifies loop spliceability before admitting
   >    (a non-spliceable loop keeps the typed wall rather than degrading
   >    to Stage-1 MalformedTopology). R0021 → CORRECT; R0026/R0051 →
   >    typed downstream walls (Stage-3 AmbiguousCurve conic class,
   >    Stage-6 Newell-normal validation). TEST-PHASE FINDING (spec
   >    amendment 1, named follow-up `kv14-lateral-cdt-chord-bound`): the
   >    KV14 holed-lateral CDT is boundary-only EARCUT with no
   >    triangle-quality bound — unsampled seam rulings make it fan wall
   >    triangles to window corners (radial sag ~5× the one-chord sagitta
   >    in the windowed-half-cylinder fixture, mesh under-fills ~15%
   >    watertight). Union oracles use delta-volume (sag cancels).
   > 3c. **Exact frame projection in ring triangulation SHIPPED 2026-07-10
   >    (spec `specs/m8_ring_exact_projection.md`).** The 2026-07-10
   >    census of the 14 remaining coplanar walls (probe re-run): 8×
   >    overlay-admitted-but-emission-fails + 3× `multi-pair` (R0046,
   >    R0081, F0073) + 3× `rim-lateral-none` (row 4). The emission-8
   >    decompose into THREE mechanisms, all downstream of ONE geometric
   >    fact — mirrored disc-rim samples carry 1–2-ULP-split frame
   >    projections (rim coords are §2c-excluded from clustering), and
   >    the exact trapezoidal sweep faithfully builds femto event-column
   >    slabs from them:
   >    (i) `build-mesh-triangulate` (F0068/F0069/C0075) — the femto
   >    Steiner twins propagate as splits into neighbor face rings, and
   >    `triangulate_ring`'s **f64** frame projection ALIASED the twins
   >    onto one bit-identical 2D point (zero-length exact edge → every
   >    fan/ear rejected). FIXED: the ring projection is now evaluated
   >    over exact rationals (fixed f64 basis; faithfulness I-EP1, no
   >    tolerance). F0068+F0069 → SUPPORTED_CORRECT end-to-end; C0075 →
   >    typed downstream edge-not-2-directed ERROR (KV15b/§4B family).
   >    Assay 226 CORRECT / 0 WRONG / 53 ERROR / 15 UNSUPPORTED / 0
   >    TIMEOUT, zero-lost (diff = exactly the three targets).
   >    (ii) `overlay-failed RoundingCollapse` (F0067, C0048, R0053) —
   >    femto-slab CELLS whose exact-positive triangles round to
   >    f64-collinear slivers (measured: all three verts share one f64
   >    x). **2026-07-10: the emission-gate T-subdivision candidate was
   >    prototyped and REFUTED (P10 abort record in spec
   >    `m8_overlay_femto_slab_emission` §8).** Clean slab needles ARE
   >    locally repairable, but the measured walls are not: (ii-a)
   >    C0048's twin corners mint crossing points EXACTLY collinear on
   >    an input chord while every off-chord vertex rounds onto the same
   >    f64 event column — every local apex is exactly degenerate or
   >    f64-collinear; needs per-region re-emission fanning femto
   >    boundary sub-segments to FAR apexes (constrained snap-rounding
   >    grade). (ii-b) R0053 has twin mints on one input edge (union
   >    boundary) whose ROUNDED order inverts their exact order — NO
   >    triangulation over the fixed rounded vertex set exists; fix
   >    belongs at the MINT SITE (sub-representable twin collapse at
   >    emission = the KV15b/A14.2 class, increment-4 precedent), which
   >    joins this sub-case to mechanism (iii) below. Free T-subdivision
   >    chaining CYCLES (measured fixpoint churn); strict-progress
   >    restores termination but cannot pass the two walls. Banked:
   >    `[sliver-probe]`/`[pocket-probe]` structure census on the gate's
   >    error path + corpus wall pins
   >    (`yang-rs/tests/m8_overlay_femto_slab_emission.rs`). Input
   >    welding remains NOT the path (two P10-reverted variants, spec
   >    `m8_shared_boundary_identity` §2b/2c scope limits).
   >    (iii) `overlay-failed DegenerateLoop` (R0007, R0071) — MICRO-scale
   >    models (~1e-4): chained inputs carry SUB-FLOOR (~7e-8 <
   >    MIN_FEATURE_SIZE) corner twin pairs; the §2b/§2c band clustering
   >    correctly welds them and the polygon then has bit-identical
   >    consecutive vertices → loud `exact_loops` reject. This is the
   >    KV15b mint-site class (collapse sub-floor twins at the EMITTING
   >    boolean, A14.2) — fixing it here (consecutive-dup collapse) would
   >    desynchronize the pair face from neighbors that still carry both
   >    corners.
   > 4. **Disc rim on a non-cylinder lateral** (`rim-lateral-none`: R0050,
   >    R0025 2nd wall) — `lateral_for_cap` is cylinder-only; R0025's rim
   >    lateral is a TORUS (circle-revolve), so crossing propagation +
   >    the downstream boolean are KV6d-torus-gated (R0015 ceiling class).
   > 5. **Swiss-cheese holed discs** (F0086–F0090) — holed-disc overlay
   >    routing (the reverted 2026-06-24 implementation is re-derivable; its
   >    old blocker, the same-normal gate, is gone).
   >    **2026-07-05/06 status (spec `m8_holed_disc_coplanar_overlay`):
   >    increments 1–3 SHIPPED.** The isolated holed-disc pair is GREEN for
   >    BOTH partners: polygon (increment 2, exact axial projection) and
   >    disc-with-rim-interior-to-the-annulus (increment 3: exact ULP-twin
   >    ring ordering `exact_rim_ccw_tiebreak` + flood-fill Stage-1 planar
   >    CDT with exact hole parity + §4.4.1(b) merge at Stage-4 entry).
   >    Remaining for this row: the CHAINED F0086–F0090 corpus cases.
   >    **Increment 5 (2026-07-06, task #62): chained cut 2 GREEN** — wrap-
   >    aware cyclic azimuth-merge pairing (yang) + sweep-aware closed-rim
   >    arc fallback (kernel-v2 recover). F0086/F0089 ERROR→UNSUPPORTED
   >    (typed re-entry boundary at cut 3). The WHOLE family now bottlenecks
   >    on one root: Stage-0 mints rim crossings ON CHORDS (off-circle by
   >    the sagitta) → mixed chains defeat recover's canonical anchor +
   >    VertexOffSurface residues (F0087/88/90). Boundary pinned:
   >    `kernel-v2/tests/m8_swiss_cheese_chain.rs`.
   >    **Increment 4 (2026-07-06/07, task #61): SHIPPED** — Stage-0
   >    sub-floor shared-mint collapse (minted on-circle vertices closer
   >    than MIN_FEATURE_SIZE collapse per rim circle to ONE shared target,
   >    crossing-branch preferred) + fold-gate skip of never-emitted
   >    3D-degenerate triangles. R0072-class adversary
   >    `crossing_one_ulp_inside_rim_sample` un-quarantined GREEN; assay
   >    175 CORRECT / 0 WRONG / zero lost. Task #62 (chained family
   >    re-measure on top of this) is now unblocked. Spec:
   >    `n2_stage4_junction_cluster_merge` §3 amendment 3 +
   >    `m8_holed_disc_coplanar_overlay` §8.
   >    **Increments 6+7 (2026-07-07, task #62): SHIPPED** — annular
   >    rim-mint contexts (one `RimChordCtx` per rim circle; cut-3 typed
   >    re-entry wall LIFTED, direct 5-hole chain GREEN) + constrained
   >    Lawson flip repair in the fold gate (amendment 4: the production
   >    sketch frame's rotated sweep order builds femto-strip slivers any
   >    on-circle rim mint inverts; repairable folds now re-triangulate
   >    locally instead of reverting to chord positions). **F0086 corpus
   >    replay SUPPORTED_CORRECT**; F0089 6→2 / F0090 27→22 errors; family
   >    residual = a second, constraint-bounded fold class (rim-mint
   >    clusters displaced ~4.6e-2, no legal flip) + F0088's typed
   >    partial-patch re-entry. Oracles: `m8_swiss_cheese_chain.rs`
   >    `engine_frame_*` (the corpus geometry, direct constructors).
   >    **Increment 8 (2026-07-07, task #62): SHIPPED** — Fig-11
   >    delete-and-reinsert cavity relocation in the fold gate (amendment
   >    5: star carve + constrained visibility growth deferring at
   >    intersection-curve/domain-boundary edges + constrained exact
   >    ear-clip when the cavity is not star-shaped from the mint). The
   >    rim-mint COLUMN-HOP class is repaired; F0087 cut 7 GREEN. Corpus:
   >    0 WRONG / 183 CORRECT (+5) / zero lost; F0087/F0089/F0090
   >    ERROR→typed UNSUPPORTED. **The whole residual family
   >    F0087–F0090 now sits on ONE wall: curved partial-patch operand
   >    re-entry (to_yang) — the next lever for this row.**
   >    **Increment 9 (2026-07-07, task #64): SHIPPED** — amendment 6
   >    JOINT region relocation: when per-vertex Fig-11 cavities are
   >    exactly NON-SIMPLE because two (or more) rim mints interact
   >    across one multi-column strip (F0087 cut 9: the plate-rim mint
   >    and a hole-rim mint each appear on the OTHER's cavity polygon,
   >    whose collapsed spokes cross), the seeds' star-UNION region is
   >    re-triangulated jointly: single closed boundary cycle, no
   >    interior vertex, single class, then the SAME shared constrained
   >    exact ear-clip (`earclip_cavity_polygon`, factored from
   >    amendment 5 byte-identically). `f0087_cut9` pin retired →
   >    positive regression; the full TEN-hole F0087 chain green with
   >    its volume oracle; **corpus F0087 flips ERROR →
   >    SUPPORTED_CORRECT** (the corpus-path partial-patch degradation
   >    disappeared with the strip repaired at Stage 0). Remaining tail
   >    of this row: F0088/F0089/F0090 (VertexOffSurface + one Stage-3
   >    AmbiguousCurve on F0088 — deeper strips/next mechanism, measure
   >    before designing) and C0048.
   >    **Increment 10 (2026-07-07, task #67): SHIPPED** — amendment 7
   >    CLASS-PARTITIONED joint region relocation. Probe census of the
   >    F0088/89/90 tail found the dominant wall: rim mints are minted
   >    exactly ON the intersection curve, so the amendment-6 star-union
   >    straddles the class boundary and its single-class guard rejected
   >    wholesale (`multi-class region` — F0089's only error, most of
   >    F0090's 18). The star-union is now partitioned by `RegionClass`;
   >    each FOLDED sub-region is relocated independently (valid-only
   >    sub-regions skipped — termination), and class-boundary edges become
   >    sub-region boundary by construction, so the intersection curve is
   >    never re-triangulated across. `f0089_cut11` pin retired → positive
   >    regression; 11-hole F0089 chain green with volume oracle; **corpus
   >    F0089 flips ERROR → SUPPORTED_CORRECT** (F0086/F0087 unchanged ✓).
   >    Residual tail, measured: F0090 (18× VertexOffSurface — each cut
   >    also folds a SECOND triangle whose per-vertex relocations all
   >    reject `interior vertex with constraint-blocked fan`, so the joint
   >    path never triggers; plus one `region polygon not simple`), F0088
   >    (2× VertexOffSurface via non-simple rings whose joint seeds stay
   >    SINGLETON + one `cavity polygon not CCW`, and one Stage-3
   >    `AmbiguousCurve{candidates:0}` SSI wall — a different subsystem),
   >    and C0048.
   >    **Increment 11 (2026-07-07, task #68): SHIPPED** — amendment 8
   >    REGION GROWTH TO SIMPLICITY. The post-amendment-7 census showed
   >    the dominant residual was `class … region polygon not simple`: a
   >    femto-strip sub-region's boundary is a BOW-TIE under the minted
   >    positions (the strip's two long sides cross exactly). The region
   >    form now grows across a crossing edge's single external same-class
   >    neighbor (constraint edges never crossed, apex-pinch guard) and
   >    rebuilds the boundary until the ring is exactly simple — the
   >    region analog of amendment 5's constrained visibility growth.
   >    `f0090_cut7` pin retired → positive regression; 7-hole F0090
   >    chain green with volume oracle; F0086/F0087/F0089 corpus ✓
   >    unchanged. F0090's 30-cut corpus chain now repairs all but ONE
   >    fold (24 reverts → 1; 93 growth events, 28+ cuts succeed vs 12
   >    before) — the corpus case flips ERROR(18) → TIMEOUT in the
   >    container because each newly-succeeding cut adds rim circles
   >    (overlay reaches 58 circles; the known heavy-chain container
   >    limit, not a hang — per-op timing curve verified monotone and
   >    finite). Remaining measured tail: ONE F0090 revert (a 33-seed
   >    star-union whose class sub-region boundary is NOT a single closed
   >    cycle), F0088's three walls (singleton non-simple seeds,
   >    `cavity polygon not CCW`, Stage-3 `AmbiguousCurve{candidates:0}`),
   >    and C0048.
   >    **Increment 12 (2026-07-07, task #69): SHIPPED** — amendment 9
   >    CONNECTED-COMPONENT SPLIT of class sub-regions (deterministic BFS
   >    through shared edges; each folded component is its own Fig-11
   >    instance). Unit-proven (two disjoint folded stars under one seed
   >    set both commit); required coverage for multi-strip joint
   >    triggers. **Post-ship measurement: the F0090 33-seed site is
   >    CONNECTED and ANNULAR** (vert 151's per-vertex ring alone has 40+
   >    edges; the ~30 ring mints inflate the joint region into a band
   >    encircling a hole — multiple boundary cycles on one component,
   >    still the loud `not a single closed cycle` reject). Next lever
   >    for the F0090 tail: measure the annular region's cycle structure,
   >    then either narrow the amendment-6 seed set to crossing-edge
   >    endpoints (keep the region a strip) or bridge-edge annular
   >    ear-clip. F0086/F0087/F0089 unchanged ✓; chain suite 13/0.
   >    **Increment 13 (2026-07-07, task #70): SHIPPED** — amendment 10
   >    CROSSING-ENDPOINT SEED NARROWING. The cycle probe confirmed the
   >    annular structure (2 cycles [32, 20]); root = the amendment-6
   >    trigger seeded EVERY minted vertex on the non-simple ring (~30 on
   >    vert 151's hole-encircling ring). `EarclipErr::NotSimple` now
   >    carries the crossing pair's endpoint positions; ring_mints narrows
   >    to mints ON the crossing (the interacting set — Fig-11 locality).
   >    The joint region stays a strip. **F0090 probe: 0 reverts, 0
   >    annular rejects in the container window (chain reaches 60-circle
   >    overlays)** — the family's fold-gate is clean as far as the
   >    container can run; F0090 corpus stays TIMEOUT on legit chain
   >    weight only. All 13 chain pins green (F0087 cut-9 interacting-pair
   >    semantics preserved). **The F0086–F0090 family's remaining
   >    pipeline walls are now F0088-only:** singleton non-simple seeds,
   >    `cavity polygon not CCW` (vert 674), and the Stage-3
   >    `AmbiguousCurve{candidates:0}` SSI wall (different subsystem) —
   >    re-measure F0088 on the current binary before designing. Plus
   >    C0048 (coplanar auto-union, separate row).
   >    **Increment 14 (2026-07-07, task #71): SHIPPED** — amendment 11
   >    SIMPLICITY BEFORE ORIENTATION. F0088's two VertexOffSurface
   >    errors (ops 14/15) both reverted at vert 674: a hair-thin
   >    full-height NET-CW BOW-TIE cavity (return edge crosses the
   >    up-chain, net 2A = −4.2e-3) died at the ear-clip's CCW guard
   >    BEFORE the crossing scan ran — a self-intersecting cycle's
   >    shoelace sign is not an orientation. Guard order swapped; the
   >    bow-tie now surfaces `NotSimple` → joint path (commits with seeds
   >    [672,674,677]). Singleton-trigger relaxation prototyped and
   >    REVERTED (3 seeds — untested branch, no measured case). **Corpus
   >    F0088: 3 errors → 1.** New chain regression
   >    `f0088_engine_frame_chain_no_offsurface_residue` (15 cuts,
   >    skip-on-error, volume oracle). **The ENTIRE F0086–F0090 fold-gate
   >    campaign is now closed** — the family's sole remaining pipeline
   >    wall is F0088's Stage-3 `AmbiguousCurve{candidates:0, matched:0}`
   >    for intersection edge (2,344): the SSI-refinement subsystem
   >    (Stage 3/4), not Stage-0/mesh-updating. Next: measure that edge's
   >    geometry (what curve should refine it, why zero candidates), plus
   >    C0048 (separate row).
   >    **AmbiguousCurve{0,0} DIAGNOSED (2026-07-07, task #72 — increment
   >    15 pending):** `ssi_rs::intersect` returns EMPTY — the plate outer
   >    cylinder and F0088 hole-4's tool cylinder are ANALYTICALLY
   >    DISJOINT (internal gap = R − d_axes − r = 0.0115) but the plate's
   >    N=14 chord facets dip inward by the sagitta ≈ 0.032 > gap, so the
   >    MESHES intersect. This is Yang Fig. 8 **Case IV** ("the meshes
   >    detect intersections that do not exist in surfaces",
   >    `refs/text/yang2025_hybrid_boolean.txt:436-447`) — the paper
   >    FILTERS it; the output topology follows analytic truth (the thin
   >    wall SURVIVES; it is 100× MIN_FEATURE_SIZE, a real feature).
   >    Planned fix: Stage-1 phantom guard — for each analytically
   >    disjoint cyl×cyl face pair whose chord bands overlap (gap <
   >    sagitta_A + sagitta_B), raise the affected circles' `min_n_seg`
   >    (the Stage-1 hook already exists) until the combined sagitta
   >    clears the gap (F0088 needs N ≥ 24; terminating criterion is
   >    ANALYTIC, unlike the rejected global NSEG floor for the
   >    column-hop class). RED = F0088 cut-4 direct-chain pin.
   >    **Increment 15 (2026-07-07, task #72): SHIPPED** — the Case-IV
   >    phantom guard. `phantom_min_rim_segments(a, b)` derives the
   >    minimal N with `sag(r_a,N)+sag(r_b,N) ≤ gap/2` over all disjoint
   >    cyl×cyl pairs (external + nested + skew; self-limiting natural-N
   >    gate; 4096 cap → the loud stop stays for true near-tangency);
   >    `boolean()` rebuilds both operands at that N, and the forced N is
   >    a BRep property (`forced_rim_n`) honored by ALL of Stage-0's
   >    internal re-tessellations (`disc_rim_ring`, `build_stage0_mesh`,
   >    the coincident-cylinder shared-N path) — plumbing WITHOUT which
   >    the boost was silently discarded (measured: identical edge ids at
   >    N=34). Pin retired → `f0088_cut4_phantom_intersection_filtered`
   >    (thin wall survives, volume oracle); the 15-cut F0088 direct
   >    chain green with NO AmbiguousCurve and NO VertexOffSurface.
   >    yang-rs 558/0 (4 new guard unit tests + mutation caught), chain
   >    suite 15/0, F0086/F0087/F0089 corpus ✓ unchanged. **Corpus F0088
   >    residual (measured): ops 7/15 `face 0: CDT triangulation failed`**
   >    — the corpus-path (sketch-extrude + auto-union) chained re-entry
   >    feeds a recovered cap whose CDT fails at the boosted rim density;
   >    a distinct pre-existing wall the coarser N never reached (the
   >    direct chain does NOT reproduce it). Next lever: measure that CDT
   >    input (dump the failing polygon; recover.rs vs
   >    cdt_polygon_with_holes at dense rims).
   >    **Increment 16 (2026-07-07, task #73): SHIPPED — the F0086–F0090
   >    CAMPAIGN IS COMPLETE.** The corpus CDT failures were the
   >    INTRA-solid form of the Case-IV criterion: the chained body's own
   >    hole-4-lateral sits 0.0115 from its plate wall, so ANY
   >    tessellation at natural N=14 puts the cap's outer-rim chords
   >    (dip 0.032) across the hole rim → crossing CDT constraints. A
   >    `boolean()`-level intra scan was tried and REVERTED (it made the
   >    corpus worse — 10 conversion-time failures: boosted outputs
   >    re-entered `BRep::new` at natural N, and the guard cannot reach
   >    conversion). The fold lives in **Stage 1's own N selection**
   >    (`stage1_tessellate_inner`; shared `cyl_pair_phantom_n` with the
   >    cross guard), where conversion, Stage-0 rebuilds, and guard
   >    rebuilds all pick it up natively. **Corpus: F0086 ✓ F0087 ✓
   >    F0088 ✓ (SUPPORTED_CORRECT, 289s solo — heavy-chain band)
   >    F0089 ✓; F0090 fold-clean (container-TIMEOUT only).** Family
   >    campaign closed end-to-end. Residual bookkeeping: F0088 runs
   >    ~290s solo in the container (the parallel 240s cap will class it
   >    TIMEOUT there — same band as F0090; the user's box decides the
   >    committed baseline).
2. **N4 — face provenance by centroid-proximity, not §4.2.3 barycentric
   provenance.** Stage-6 attributes each kept triangle by centroid-in-plane
   distance + a tolerance tier (`tol_for`). The paper maps each point to BOTH
   surfaces via the arrangement's per-triangle barycentric coordinates. N4 is the
   *structural root* of the `FaceResolutionFailed` fragility (and the
   tolerance-band/unmasking churn). Its blocker (N1, sidecar-only solid-level
   labels) is GONE now that the native arrangement is complete — so N4 is
   actionable and should be done *with* the general Stage-0 work.
3. **N2 — Stage-4 mesh updating is relocation-only (§4.4.1 CDT absent).** We
   relocate existing intersection vertices; the paper does full CDT mesh updating
   (split/merge/insert + per-triangle d(T) recompute). This is why Stage 4 hits
   loud `DegenerateTriangle`/`LocalRefinementRequired` stops on harder coplanar
   and curved configs. **N2-1 done (2026-07-01):** the faithful §4.4.1
   mesh-updating *primitive* now exists, unit-tested in isolation —
   `cherchi_rs::cdt_with_interior_constraints` (Fig 11 `split`, interior
   constraint CDT) + `yang_rs::stage4_update::stage4_mesh_update` (Fig 11
   split/merge/insert over the parametric domain). NOT yet wired into
   `stage4_relocate_and_correct`. **N2-2 done (2026-07-02):** per-triangle
   `d(T)` recompute — `yang_rs::stage4_dt::{eval_uv, d_of_t}` computes the
   certified Fig-6 bound from exact rational-Bézier surface-of-revolution
   control nets (one general constructor for cylinder/cone/sphere/torus; plane
   trivially 0; convex-hull certificate; pinned `eval_uv` parameterization the
   N2-3 patch extraction must share). Unit + adversary suites incl. a
   mutation-kill matrix (`tests/n2_dt_adversary.rs`). **N2-3 grounding
   (2026-07-02, instrumented):** the premise above is STALE — probes at all
   four Stage-4 repair STOPs hit ZERO times across the yang-rs suites, the
   same-normal campaign, and the 194-case assay, so wiring the primitives has
   NO live consumer today and stays deferred until one appears (demand-driven;
   the primitives are ready). The trail instead led to **N2-3a (done
   2026-07-02):** Stage-0 was minting coplanar-overlay vertices that subdivide
   a disc-rim chord at CHORD positions (raw `frame.lift` fallback) — off the
   exact rim circle by the sagitta, the dominant `VertexOffSurface` class
   (R0072), silent in release builds (the kernel tripwire is debug-only). Fix:
   mint on the exact rim `Curve::Circle` (radial projection for x-event
   subdivisions, exact circle∩line for rim×other-input-edge crossings),
   behind a fold-validity gate (a mint that would invert an incident overlay
   triangle reverts to the chord lift, deterministic fixpoint) — unconditional
   exactness folds coarse-rim overlays (R0013's 9-gon, sagitta 0.53). **The
   gate's revert population is the first recorded LIVE consumer for
   overlay-level mesh updating** (repositioned boundary vertices need local
   re-triangulation, Yang Fig 11) — that, not Stage-4, is where the N2-1/N2-2
   machinery should first wire in. Assay 0 WRONG, no CORRECT lost (60s run:
   82 CORRECT; R0013 30s-cap TIMEOUT flip is proven timing noise). R0072's
   blocker moved downstream to kernel-v2 `TessellationFailed { FaceId(19),
   "inverted final triangle" }`; R0021's is the Stage-1 partial-patch
   re-entry wall. R0096 = torus×torus v1 wall (different mode).
   **R0072 face-19 diagnosed (2026-07-02, `KV2_EARCLIP_PROBE`):** the output
   face's outer loop carries a collinear direction-REVERSAL micro-spur
   (p0→p1 4.4e-6 forward, p1→p2 1.4e-6 BACK along the same line, at face
   extent ~2e-4) — a §4.5.3-class reversed point on a straight boundary run
   that the exact ear-clip correctly refuses; sibling zigzags on the
   face-9/10/12–15 plane family. The fix is in yang OUTPUT loop emission
   (reconstruct_topology boundary-cycle ordering / spur elimination), not in
   kernel-v2 tessellation.
   Specs: `specs/n2_stage4_mesh_updating.md`, `specs/n2_stage4_dt_recompute.md`,
   `specs/n2_stage4_junction_cluster_merge.md` (amended ×2 — the grounding
   trail is recorded in its §0).
   **§4.5.3 junction-protected collapse SHIPPED 2026-07-08 (spec
   `specs/yang_453_junction_protected_collapse.md`):** the ERROR-census
   ellipse-endpoint class (R0009/R0011/R0091,
   `InvalidBooleanOutput("output ellipse-arc endpoint does not lie on its
   ellipse")`) was TWO Stage-4 victim-selection bugs, both violating Yang's
   "points along ONE curve C" scope. **(a) SHIPPED:** the §4.5.3 reversal
   sweep collapsed the NEXT point even when it was a curve-JUNCTION vertex
   (the exact shared endpoint of two conic sections — R0011's gear-flank ×
   revolve-cylinder loop lost all seven junctions, merging arcs of different
   ellipses into single output edges). Fix `reversal_collapse_direction`:
   junction p_n survives; the overshooting p_r is the victim. R0011
   ERROR(ellipse wall) → deeper loud render-CDT wall (R0072 class).
   **(b) DIAGNOSED, BANKED-UNWIRED:** the §4.4.1(b) sub-feature merge picks
   its survivor by LOWER INDEX, destroying an exactly-relocated conic
   endpoint in favor of an unrelocated chord vertex (R0091 AND R0009's
   ellipse walls — both micro scale). The ranked-survivor primitive
   `sub_feature_merge_direction` (junction > conic endpoint > plain; equal
   rank keeps the index rule) is banked with unit tests + mutation kills
   but NOT wired: wiring it clears both ellipse walls (R0009 → 1
   pre-existing non-2-manifold error, no WRONG) but flips R0091 ERROR →
   SUPPORTED_WRONG (χ=−4 vs meta 2, unverifiable in-session — spec §3b
   status has the unblock path: sidecar reference parity on the R0091
   output, or refute the meta χ). Trackers
   `test-harness/tests/s453_junction_collapse_campaign.rs`: R0011 GREEN;
   R0009/R0091 documented `#[ignore]` RED (un-ignore when §3b wires).
   **(c) §3c straight-run reversal sweep SHIPPED 2026-07-08 (same spec):**
   the sweep's `all_conic` loop gate never corrected reversed sequences on
   STRAIGHT intersection runs — R0072's seam mints (chord-crossing
   positions) become out-of-order when their neighbor triple point is
   exactly relocated (Yang Fig. 15 verbatim, on a line), and the output
   loop doubles back → kernel-v2 `"ring rejected by CDT"`. Shipped scope:
   mixed cycles sweep straight-run (LineSegment-pair) sites ONLY, with run
   identity via unordered incidence surface pairs, the n_A×n_B tangent
   (paper Fig. 15; §4.5.5 coincident pairs undiagnosable — checked before
   the U-turn arm), junction/run-end-protected victims, and a 2·d_ε
   resolution gate. PLUS a Stage-0 admission wall: partner disc rim
   STRICTLY CROSSING an annular face's HOLE rim → typed
   `CoplanarFacesUnsupported` (probe `annular-hole-rim-crossing`) — the
   `annular_cap_hole_crossing_stays_loud` pin's documented boundary, which
   previously held only via an accidental downstream NonManifoldOutput.
   R0072's FaceId(9) spur is repaired (wall→FaceId(11), a conic-site
   mixed-cycle reversal — DISPROVEN sweep class, see spec §3c P10 records:
   coarse 7-gon chords false-positive the 45° band per the corner_in_band
   adversary; no mesh-level manifold gate can backstop it per the standing
   §2b record, re-confirmed on the Steinmetz tangency seam). Trackers
   `s453_line_run_reversal.rs`: R0072/F0045 documented `#[ignore]` RED
   (F0045 = MACRO self-intersection at FaceId(9), a different mechanism).
   New probes kept: `YANG_S6_CYCLE_DUMP`, `annular-hole-rim-crossing`.
   **N2/F0059 epic increment 1 SHIPPED 2026-07-10 (spec
   `specs/yang_collapse_membrane_cancellation.md`, task #121):** the F0059
   "Stage-6 double-cover" origin is the Stage-4 **PR-KV9 junction-twin
   collapse itself** — identifying the two arrangement vertices minted for
   ONE Steinmetz seam apex turns the pleat spanning the twin gap into an
   exact opposite-winding duplicate pair (zero-volume flap, count-4 fan
   edges, `s6-wedge-walk-not-outgoing`). Fixed at the mint site:
   `collapse_vertex` now cancels opposite-winding exact-duplicate pairs
   (both copies; unit red→green ×3 branches). The 2026-07-08 diagnosis's
   deeper layers are corrected: the χ=4 "two-shell stitching gap" was an
   artifact of that experiment's exclusion-style workaround and does NOT
   exist; with the (still banked-unwired, `YANG_TRIPLE_JUNCTION_EXPERIMENT`)
   conic triple-junction handler + this fix, F0059's boolean COMPLETES and
   the wall moves to kernel-v2 render CDT `FaceId(7)` ring-reject — the
   cap-disc's four segment lobes meet the trim chords exactly ON the rim
   circle and the chord-sampled rim crosses the trim chords near those
   junctions (§4.3.3 Case-IV **rim-junction-insertion class**, suspected
   same wall as the F0045/R0011 ring-reject family; M8 incr-15
   `forced_rim_n` is the precedent machinery). Epic increment 2 = rim
   junction insertion; increment 3 = wire the handler (green = F0059
   end-to-end). Probes banked: `YANG_DOUBLECOVER_PROBE` (dup-triple scans +
   collapse-site tags), wedge-dump under `NONMANIFOLD_SITE_PROBE`.
   **N2/F0059 epic increments 2+3 SHIPPED 2026-07-10 (spec
   `specs/yang_rim_junction_insertion.md`, task #122) — F0059
   ERROR→SUPPORTED_CORRECT end-to-end; assay 232 CORRECT / 0 WRONG /
   45 ERROR / 0 TIMEOUT, zero-lost (R0015 also advances: its junction
   resolves and it lands on the TYPED M8 coplanar boundary,
   ERROR→UNSUPPORTED).** Increment 2 = Stage-1 rim junction insertion:
   `rim_junctions_against` derives the exact §4.3.3 Case-IV points where
   a full-circle rim transversally crosses the other operand's
   parallel-axis cylinder laterals (circle∩line closed forms; lateral
   extent from loop circles ±TAU_WORK keeps boundary triple corners;
   tangency gate = root pair < TAU_MODEL), wired via
   `from_topology_with_rim_overrides` + the M8-incr-6
   `stage1_tessellate_with_rim_overrides` vocabulary, scope-gated to
   pairs with NO Stage-0 interaction (the incr-15 pass-through trap is
   avoided, not threaded). Increment 3's final form is a **pre-scan
   exactness certificate** (measured: the corners trip INSERT-TIME
   detectors — the line∩line "out of scope" STOP — so the planned
   post-scan escape can never run): `exact_junctions` = vertices with ≥3
   distinct inc0 surfaces all within TAU_WORK, skipped by every Stage-4
   map insertion; everything inexact keeps today's loud walls. The
   twice-reverted Newton triple-junction handler is UNNECESSARY for this
   class and was removed (its spec stays as the design record).
   Green pins un-ignored: truncated-Steinmetz exact-volume union +
   never-stops-at-Stage-4 (`yang-rs/tests/rim_junction_insertion.rs`).
   Remaining named residue: quartic-class rim junctions (transversal
   axis), partial-arc rims, the ring-reject family (F0045/R0011 — no rim
   junctions, different mechanism), the 13 other Stage-4 LRR configs.
   **ERROR-census campaign 3 SHIPPED 2026-07-08 (spec
   `specs/cut_consumes_body.md`):** the EmptyBooleanResult cluster
   (R0023/R0027/R0058/R0088) is ONE mechanism — a cut whose tool ENGULFS
   the whole target; yang's all-inside classification and kernel-v2's
   typed `EmptyBooleanResult` are both CORRECT, but the engine recorded an
   operation ERROR instead of applying body-lifetime policy. Fix: new
   typed `KernelError::BooleanEmptyResult` (waffle-types, A6.2) mapped by
   the adapter; `modeling_ops::execute_boolean` turns Subtract/Intersect
   empties into zero-output OpResults with a consumed-body warning
   (Union-empty stays loud — a kernel defect must not masquerade as
   consumption; mutation-killed); the engine's Cut/Intersect arm forwards
   per-target diagnostics into engine_warnings. Campaign suite
   `test-harness/tests/cut_consumes_body_campaign.rs` (engine fixture +
   disjoint-volume intersect adversary + 4 corpus trackers) 6/6 GREEN.
   **ERROR-census campaign 4 SHIPPED 2026-07-08 (spec
   `kv9_f3_output_vertex_identity` §4 row E-V6):** the short-loop cluster
   (R0046/F0064/R0088, `"output loop with fewer than 3 edges"`) was a
   vocabulary gap — a genuine D-FACE (circular-segment face: chord + conic
   arc between the same two vertices; R0046's plane∩cylinder cap fragment,
   chord 0.197 on the r=0.130 circle). `from_yang_brep` now accepts 2-edge
   loops with exactly one `Seg` + one conic arc (two `Seg`s and same-curve
   arc pairs stay rejected; mutation-killed). R0046's long-standing
   output-loop wall is GONE (→ typed UNSUPPORTED curved re-entry); F0064 →
   face-normal/Newell disagreement wall; R0088 → render tessellation wall
   (FaceId 492). Trackers `dface_bigon_campaign.rs` 3/3 GREEN.
4. **N5 — Stage-1 discretization bypasses the unified §4.1 d_ε-iterate + §4.1.2
   CDT framework** (per-surface ad-hoc Newell fans / rim rings instead).
5. **N6 — §4.5.4 illegal-self-intersection detection/removal is absent.**
6. **Scope (deferred, signed off):** **N7** — Stage-3 SSI is exact closed-form
   (`ssi-rs`) rather than Yang's Newton/geometric mesh optimization; this is a
   *sound, superior* substitution for analytic surfaces (invariant A15) and the
   correct choice — it only becomes a gap under NURBS. **NURBS/Bézier** (§4.1.1
   subdivision, §4.1.2 NURBS-boundary CDT) is a separate architectural milestone.

The shortest honest path to a first functional boolean was the M0–M8 milestones
(§4); the path to a kernel that **replaced legacy** was the Phase 1–6 completion
roadmap (§4b) — both now largely DONE. The path to a kernel that is **faithful to
Yang** is closing the deviations above, led by the general Stage-0 program.

## 1. Thesis: decouple "functional Yang" from "native arrangement complete"

The prior roadmap gated real Yang Stage 5/6 on a *complete native `cherchi-rs`
arrangement* — a large, entirely unwritten graph algorithm. That coupling is why
the project shipped throwaway substitutes instead of a validated vertical slice.

We break the coupling with a producer-agnostic **`LabeledArrangement`**
interface (§2). An *interim* producer (patched C++ sidecar, §3a) satisfies it
now, so `yang-rs` Stage 5/6 becomes **real** in weeks. The *native* `cherchi-rs`
arrangement (§3b) is then built behind the **same** interface, with the sidecar
as its differential-parity oracle.

## 2. The `LabeledArrangement` interface (the contract)

Defined **once, here**. Crate `CLAUDE.md` files reference this section; they must
not redefine the shape. Freeze it only after round-tripping the two validation
cases in §3a.

> **Revised 2026-05-29 (M2), solid-level provenance.** The original contract
> below asked for `source: SmallVec<(InputId, parent_tri_index)>`. Inspecting the
> Cherchi 2022 C++ source showed the input-*triangle* index is **lost** during
> arrangement subdivision — daughter triangles inherit only the parent's
> mesh-level label (`labels.surface`, a bitset of which input *solid*). Recovering
> a triangle index would need an invasive patch to arrangement internals we don't
> own. Yang reassembles *faces*, not triangles, and the face is recoverable from
> solid-id + plane-membership (the exact arrangement keeps each non-coplanar
> sub-triangle in its source face's plane) — which yang-rs does in M3. So the
> contract is **solid-level**; the triangle index is dropped. See
> `specs/yang_m2_labeled_arrangement.md`.

The output of Yang Stage 2 — the **full** arrangement mesh (all sub-triangles,
before any op filter; yang-rs does its own op selection). Per **output triangle**:

- `surface: Vec<InputId>` — which input solid(s) the triangle lies on. **len ≥1
  normally; len ≥2 only at coplanar overlap** (an output triangle can belong to
  both A and B — Cherchi 2022 §3). A scalar would silently mis-attribute coplanar
  faces (the case the legacy port died on).
- `inside: Vec<bool>` — in/out per input solid (`inside[k]` = the triangle is
  inside solid `k`). Captured **before** the op filter collapses it.
- `patch_id: u32` — Cherchi's connected same-surface patch (its own Stage-5
  grouping), one per triangle.

**Division of labour.** `yang-rs` owns the mesh→B-Rep mapping via its Stage-1
`TessellationMap` + geometric plane-membership. The producer reports only
**solid-level** provenance + in/out + patch; `yang-rs` composes: output tri →
(producer: solid A/B) + (geometry: which of that solid's face-planes contains it)
→ B-Rep face.

## 3. Producers

### 3a. Interim — patched `mesh_booleans` sidecar  *(chosen path)*

Cherchi 2022 already tracks per-output-triangle origin internally (§3 of the
paper: *"for each output triangle we propagate information on its origin"*) and
classifies patches in/out per input. The work is to **emit** it, not compute it.

- **Patch location:** reach into `customBooleanPipeline` (pre-filter), not just
  `main.cpp` — the op-specific selection collapses the per-input in/out vector,
  so dump the labels *before* that filter.
- **Format:** a sidecar file written alongside the result OBJ, encoding the
  `LabeledArrangement` shape from §2 (per-tri source list + patch id; patch
  in/out table).
- **Validate before freezing the interface:** round-trip on (1) two tetrahedra
  (clean 1:1 provenance) AND (2) one coplanar-overlap case (multi-attribution).
  Only then is §2's shape frozen.

`cherchi-sidecar-rs` owns this producer and the C++ patch.

### 3b. Native — `cherchi-rs` Stage 2, same interface

**Status: ✅ COMPLETE (M6, 2026-06-10).** Landed as PR-CR-AR1→AR3b (arrangement)
+ PR-CR-BL1→BL3 (labeling/boolean): `NativeBoolean` is parity-green vs the C++
sidecar on the BL3b corpus and is yang-rs's production backend (BL3c); the
sidecar remains as the test-only parity oracle. M7 (clean-room predicates →
WASM) is ✅ COMPLETE (PR-CR-M7a/b/c, 2026-06-10); remaining native-track work
is M8 (coplanar Stage 0).

Built incrementally, **diffed against the sidecar** on the corpus. The IP-FFI
predicates (`indirect-predicates-sidecar-rs`) are consumed **demand-driven** by
this code — we stop porting predicates ahead of a caller. Per user directive,
the native path used the FFI predicates *first*; the clean-room
reimplementation from Attene's paper then restored WASM (M7 ✅, PR-CR-M7c
swapped every consumer to `predicates::indirect`).

## 4. Critical path & milestones (ORDERED)

> The real gate to a first boolean is **not** the label interface — it is
> Stage-1 mesh *validity*. Cherchi loops forever on malformed input: fed real
> F0002 tessellation, `mesh_booleans` failed all three `inputcheck` predicates
> and ran ~6 h before being killed. The native arrangement would hit the same
> wall. So M1 precedes M2.

- **M0 — Operationalize the parity oracle.** ✅ **DONE** (`scripts/build_sidecars.sh`;
  the C++ sidecars build, `indirect-predicates-sidecar-rs` runs in available mode
  (42 tests), and the `cherchi-sidecar-rs` / `cherchi-rs` parity tests exercise
  the real binary instead of self-skipping).
- **M1 — Stage 1 emits Cherchi-`inputcheck`-clean meshes.** ✅ **DONE** (convex
  planar scope). `yang-rs` Stage 1 (`BRep::new`) canonicalizes each face's
  triangle winding to its analytic `Surface::Plane.normal` (Newell normal +
  dot-sign reverse); degenerate/sub-feature-area faces → `YangError::DegenerateFace`.
  Cube + tetrahedron pass all five `inputcheck` axioms against the real binary.
  Spec: `specs/yang_m1_stage1_orientation.md`; commits `f423581d` (spec) →
  `a66460f6` (RED) → `7da238d4` (GREEN) → `24e73307`/`d356297b` (adversarial
  area-threshold fix). **Scope:** convex planar faces only; non-convex/holes are
  banked (PR-YR2b–d) and not yet made inputcheck-clean.
- **M2 — Patched sidecar emits `LabeledArrangement`.** ✅ **DONE.** A
  version-controlled C++ patch (`patches/cherchi2022_labeled_arrangement.patch`,
  applied by `scripts/build_sidecars.sh`) dumps, per arrangement triangle, the
  surface solid(s) + per-solid in/out + Cherchi patch id; `cherchi_sidecar_rs::
  labeled_arrangement()` parses it into a `cherchi_rs::LabeledArrangement` (the
  **frozen, solid-level** §2 shape). Acceptance oracle green: `keep_set(op)`
  reproduces the stock `boolean(op)` triangle set for all four ops; coplanar
  cubes yield real multi-attribution; deterministic (TBB pinned to 1 thread).
  Spec: `specs/yang_m2_labeled_arrangement.md`; commits `0d321e6a` (spec+§2) →
  `3add0ebd` (RED) → `cd78d15b` (C++ patch+build) → `68bceb66` (GREEN Rust) →
  `b091553d` (adversarial + env hardening).
- **M3 — `yang-rs` Stage 5/6 consume true labels → FIRST functional boolean.**
  ✅ **DONE.** `boolean()` consumes the `LabeledArrangement` (via the new
  `MeshBoolean::labeled_arrangement` seam), welds the arrangement mesh,
  `keep_set(op)`-selects + orients (`flip_for_op`) the kept tris, resolves each
  tri's source face geometrically (centroid-in-plane, `TAU_WORK`; degenerate
  edge-slivers attributed to the lowest tied face), and reassembles via
  `reconstruct_topology` into a **watertight 2-manifold B-Rep**. Verified on
  independent interpenetration geometry (not just the canonical cubes): signed
  volumes exact (union/intersect/subtract), 0 unpaired half-edges, Euler V−E+F=2.
  **Scope:** Union/Intersect/Subtract on interpenetrating convex planar solids.
  **Deferred:** Xor (multi-shell — gated loudly via `YangError::UnsupportedOp`);
  coplanar overlap (M8, `FaceResolutionFailed`); curved surfaces/SSI (M5).
  Spec: `specs/yang_m3_functional_boolean.md`;
  commits `4f206b27`→`4bac08cb`→`a945e037`→`d81eeda4`→`f43294c2`.
- **PR-YR5c — B-Rep faces with inner loops (holes).** ✅ **DONE.** When one solid
  pierces a hole through another's face, `reconstruct_topology` now builds the
  annular face (multi-cycle boundary extraction; outer = largest-|area| cycle,
  the rest are holes; cavity-wall normals flipped to point result-outward) instead
  of erroring `NonManifoldOutput`. **Impact:** the randomized box-boolean fuzz
  (`tests/fuzz_boxes.rs`, 900 cases) went from **75.2% → 100%** correct
  (aligned 86.2→100%, rotated 64.2→100%), eliminating the entire
  `NonManifoldOutput` bucket, with `SILENT_WRONG` still 0. Genuine non-manifold
  (T-junction/dead-end) and nested holes still error loudly.
  Spec: `specs/yang_pr_yr5c_inner_loops.md`; commits
  `ed550ae5`→`bbb14283`→`d90aa5f1`→`59771f86`→`287ea5ee`.
- **M4 — Retain YR3/4/5 substitutes as a `#[cfg(test)]` differential oracle.**
  ✅ **DONE** (bundled with M3). `match_with_input`/`face_candidates`/
  `majority_vote` moved to `#[cfg(test)]`; differential test cross-checks the
  real-label attribution against the substitute. Not deleted.
- **M5 — Stage 3/4 SSI + CDT refinement** (faceted → surface-exact). `ssi-rs`
  solvers + mesh-updating CDT along refined curves. Stage 5/6's
  *patch-segmentation* logic is durable; only its *curve-source* changes — build
  the seam there.
  - **Decision (curve representation):** the kernel uses **true analytical
    curves** (a plane∩sphere edge is a `Circle`, not a polyline) with **f64
    parameters** — zero shape error; topology robustness stays in the exact mesh
    predicates (cherchi/dashu). This is the Yang/Parasolid/ACIS model and is NOT a
    deviation (SSI *is* Yang Stage 3). Faceted-but-displayed-smooth was rejected
    (inexact geometry that compounds through chained ops).
  - **Step 1 — `ssi-rs` exact-SSI foundation (PR-SSI1) ✅ DONE.** Stood up the
    crate's public surface (`QuadricSurface{Plane,Sphere}`, `SsiCurve{Line,Circle}`,
    `SsiError`, `eval`, deterministic `in_plane_basis`) + the first 3 closed-form
    solvers (`plane_plane`, `plane_sphere`, `sphere_sphere`, each citing
    Patrikalakis §5.8) + `intersect()` dispatch. On-surface oracle (sample curve →
    satisfy both surfaces) + analytical-geometry + symmetry + determinism oracles.
    Adversary: no bugs; near-tangent guards short-circuit before `sqrt` (no NaN);
    solvers relative-correct to ≥1e9 scale; the absolute on-surface oracle is
    bounded to coordinate magnitude ~1e8 (recorded in spec — relative residual for
    larger-scale Stage-3 consumers). Spec `specs/ssi_pr_ssi1_foundation.md`;
    commits `8b1c7282`→`7255b380`→`c001101e` (RED)→`a508e865` (GREEN)→`c4e1efe0`
    (adversary). 28/28 ssi-rs tests; CI gate clean; no sibling/legacy changes.
  - **Step 2 — `ssi-rs` plane∩cylinder (PR-SSI2) ✅ DONE.** A15.4 pair #2: adds
    the `QuadricSurface::Cylinder` surface and the first non-circular curve
    (`SsiCurve::Ellipse` + its `eval`), with the `plane_cylinder` solver (C1
    perpendicular→Circle, C2 oblique→Ellipse `a=r/|c|`, C3a parallel-secant→two
    Lines, C3b tangent→one Line, C3c disjoint→[], E1 degenerate→Err) and the first
    triggerable `AnalyticalSolutionNotAvailable` path (sphere∩cylinder). Stays in
    closed-form conic territory — no Degree-4 quartics. **Adversary found a real
    bug:** the C1 band, gated on `1−|c|`, let the snap-to-perpendicular circle sit
    up to `√(2·TAU)·r ≈ 4.5e-4·r` off the cutting plane (~4000× tolerance) because
    the off-plane error scales with the *sine* `√(1−c²)`. Fixed (RED→GREEN
    sub-cycle) by gating C1 on `|proj|=√(1−c²)<TAU_MODEL` (the axis's in-plane
    projection norm, which also unifies with C2's `normalize(proj)` guard) →
    off-plane error bounded by `r·TAU_MODEL`. Adversary also confirmed a C2
    ellipse's on-surface residual tracks `r`, not the (possibly huge) major axis.
    Spec `specs/ssi_pr_ssi2_plane_cylinder.md`; commits `b53e566c`→`22729f1f` (RED)
    →`394f772a` (GREEN)→`5a3cded6` (spec fix)→`9a8c6c37` (adversary RED)→`37e17ff7`
    (fix GREEN). 55/55 ssi-rs tests; CI gate clean; no sibling/legacy changes.
  - **Step 3 — `ssi-rs` plane∩cone, bounded sections (PR-SSI3) ✅ DONE.** A15.4
    pair #3: adds the `QuadricSurface::Cone` surface (infinite double cone, pure
    quadric) and `plane_cone` for the **bounded** sections — C1 circle (plane ⟂
    axis) + C2 ellipse (closed section) — reusing `Circle`/`Ellipse`. Classifies
    via the two symmetry-plane generators `g_±=cosα·â±sinα·û`; ellipse params from
    the vertex method + closed-form `b²=(d·â)²/cos²α−|d|²`. **Scope (user decision):
    bounded first** — parabola/hyperbola (PH) and through-apex (AP) return loud
    `Err` (`AnalyticalSolutionNotAvailable`/`DegenerateInput`), a deliberate staged
    gap removed in PR-SSI4, never a fallback (A15.2). On-surface oracle uses a cone
    **radial** residual (length). Adversary (17 attacks) confirmed the dangerous
    ellipse↔parabola boundary is robust (huge `a` stays finite + on-surface; clean
    flip to `Err` at the `gd_±` gate; no NaN/Inf/misclassification) and flagged a
    minor C1-gate conditioning inconsistency vs `plane_cylinder` — fixed for
    consistency (gate on the stable `|n̂−k·â|`, not `√(1−k²)`; reuse `proj` for `û`).
    Spec `specs/ssi_pr_ssi3_plane_cone.md`; commits `d0f3bfe1`→`f16f9fbd` (RED)→
    `014e7445` (GREEN)→`f3cacaae` (adversary)→`64047b06` (spec fix)→`ddc5e2be`
    (consistency fix). 86/86 ssi-rs tests; CI gate clean; no sibling/legacy changes.
  - **Step 4 — `ssi-rs` plane∩cone unbounded conics (PR-SSI4) ✅ DONE.** Completes
    the **four proper** plane∩cone sections: adds the first two **unbounded**
    `SsiCurve` types — `Parabola { vertex, normal, axis_dir, focal_length }` and
    `Hyperbola { center, normal, major_axis, semi_transverse, semi_conjugate }` —
    each with its own `eval`, and the `plane_cone` PARA/HYPE branches replacing the
    SSI3 staged `Err`. On the infinite double cone a hyperbola returns **two**
    branch curves (`±major_axis`, `+m̂` first). Constructions hand-verified before
    coding (hyperbola α=π/4 plane x=1 → center(1,0,0),a=b=1,vertices(1,0,±1);
    parabola → vertex(½,0,½),f=1/(2√2),eval(1)=(0,−1,1) on both surfaces). The new
    PH contract obsoleted PR-SSI3's staged-gap assertions (2 `ssi3.rs` placeholders
    + 6 `ssi3_adversary` attacks) — migrated to the new contract, **adversary-
    verified faithful** (no attack weakened; the sweep guard tightened). Adversary
    (13 attacks): no bugs; both classification boundaries clean (no NaN/blow-up/
    misclassification); parabola on-surface to coord ~1e8, hyperbola exact to T≈7.
    Through-apex degenerate conics (point/line/two-lines) **deferred to PR-SSI5**
    (still `Err(DegenerateInput)`). Spec `specs/ssi_pr_ssi4_plane_cone_unbounded.md`;
    commits `d7cbbd8a`→`39841dc5` (RED)→`b1f5da9f` (GREEN + faithful fixture
    migration)→`38f7d553` (adversary). 108/108 ssi-rs tests; CI gate clean.
    **plane∩{plane,sphere,cylinder,cone} now complete for all proper conics.**
  - **Step 5 — `ssi-rs` plane∩cone through-apex degenerate conics (PR-SSI5) ✅
    DONE — plane∩cone now COMPLETE.** Replaced the AP `Err(DegenerateInput)` with
    the degenerate result: point → `Ok([])` (`|k|>sinα`, incl. ⟂); one Line
    (`|k|=sinα`, tangent generator); two Lines (`|k|<sinα`, crossed generators
    through the apex). No new curve types (reuses `Line`); sub-case classified by
    the proven `gd_±` sign test (`gd₊·gd₋=k²−sinα²`); two-line dirs `cφ·m̂±sφ·ŵ`.
    Hand-verified (α=π/4, n̂=(1,0,0) → (0,∓1,1)/√2 = `z²=y²`; tangent → (−1,0,1)/√2;
    ⟂ → []). The new AP contract obsoleted PR-SSI3's AP assertions (2 ssi3 tests +
    1 adversary attack) — migrated to the new contract, **adversary-verified
    faithful**. Adversary (13 attacks): no bugs; clean monotone point↔line↔two-line
    boundary; clean AP-detection band; lines exact on both surfaces. Spec note: the
    tangent sub-case is a ~1.4e-7-wide k-window (intrinsic to the exact `k=sinα`
    degenerate). Spec `specs/ssi_pr_ssi5_plane_cone_through_apex.md`; commits
    `e974295b`→`476fc663` (RED)→`c2d9ed47` (GREEN)→`9d109bef` (adversary). 129/129
    ssi-rs tests; CI gate clean. **plane∩{plane,sphere,cylinder,cone} fully done.**
  - **Step 6 — `ssi-rs` sphere∩cylinder coaxial (first degree-4 pair, PR-SSI6) ✅
    DONE.** The degree-4 pairs' **coaxial/special** configs reduce to analytic
    conics (research-confirmed; the legacy code does the same). PR-SSI6 ships the
    first: coaxial sphere∩cylinder (axis through sphere center) → circles
    (`z²=r_s²−r_c²`): X2 two circles at `C±h·â`, X1 one tangent great circle, X0
    empty. Reuses `Circle` — no new curve type, no enum-match migration.
    **Non-coaxial (general degree-4) → `Err(AnalyticalSolutionNotAvailable)`** — a
    staged gap (the general degree-4 curve needs a new `SsiCurve` variant, deferred),
    never a fallback. Establishes the coaxial-detect→reduce-to-circles→general-ASNA
    pattern for the other circle-reducible pairs. **UPDATE (2026-07-08, M5 Option
    B ✅ SHIPPED for cyl×cyl):** the staged ASNA is now resolved for
    cylinder×cylinder — the general degree-4 curve is the **procedural
    surface-pair curve** (`SsiCurve::SurfacePair{a,b}` → yang `Curve::SurfacePair`
    → kernel-v2 `Curve::SurfacePair`), defined implicitly by its two exact
    surfaces and certified by Newton projection onto both (P8 degree-4
    clarification; `specs/m5_surface_pair_curve.md`). Stage-4 relocation reuses
    `relocate_onto_implicit_pair` as a sibling of the torus block. Corpus C0052
    ERROR→CORRECT; `unequal_perpendicular_now_supported` green. Cone-pair operand
    (R0008/R0003 AmbiguousCurve class) is the next producer. Adversary (15 attacks): no bugs;
    clean tangent + coaxial-detection boundaries; two characterized absolute-`TAU`
    ceilings (on-surface ~1e9, coaxial-detection ~1e8 → conservatively NC). Spec
    `specs/ssi_pr_ssi6_sphere_cylinder_coaxial.md`; commits `16dca4a0`→`818f2882`
    (RED)→`de7926c4` (GREEN)→`614292b1` (adversary). 141/141 ssi-rs tests; CI clean.
  - **Step 7 — `ssi-rs` sphere∩cone coaxial (second degree-4 pair, PR-SSI7) ✅
    DONE.** Reuses the SSI6 coaxial-detect→reduce-to-circles→general-ASNA pattern.
    Coaxial sphere∩cone (sphere center on the cone axis) reduces to one/two circles
    via `sec²α·h² − 2h0·h + (h0²−r_s²)=0`, roots `h=(h0±√D)·cos²α`,
    `D=sec²α·r_s²−h0²tan²α`. Gate on the **linear** gap `g=r_s−|h0|·sinα`
    (`sign(D)=sign(g)`, since `D=sec²α(r_s−|h0|sinα)(r_s+|h0|sinα)`) per the
    SSI2/3/6 lesson, so X2 (`g>TAU`) guarantees `D>0` and `√D` is safe: X2 two
    circles (`+√D` first), X1 one tangent circle (`|g|≤TAU` at `h_t=h0·cos²α`), X0
    empty (`g<−TAU`). Reuses `Circle` — no new curve type, no enum-match migration.
    **Non-coaxial (general degree-4) → `Err(AnalyticalSolutionNotAvailable)`** —
    staged, never a fallback. Adversary (18 attacks): no bugs; clean tangent
    (α≠π/4 exercised) + coaxial-detection boundaries; characterized absolute-`TAU`
    ceilings (on-surface ~1e8→1e9, coaxial-detection ~1e8→1e9 → conservatively NC)
    and the apex-grazing `r_s=|h0|` radius-0 point-circle degeneracy (downstream
    filters it). Spec `specs/ssi_pr_ssi7_sphere_cone_coaxial.md`; commits
    `6d58f415` (spec)→`9144dfd4` (RED)→`8b12402d` (GREEN)→`d575280b` (adversary).
    189/189 ssi-rs tests; CI clean.
  - **Step 8 — `ssi-rs` cylinder∩cone coaxial (third degree-4 pair, PR-SSI8) ✅
    DONE.** Reuses the SSI6/SSI7 coaxial-detect→reduce-to-circles→general-ASNA
    pattern. Coaxial cyl∩cone (axes parallel AND cyl axis_point on the cone axis
    line) reduces to **exactly two circles** at `h = ± r_c·cotα` via the classical
    `|h|·tanα = r_c` reduction. **Unlike SSI6/SSI7 there is NO discriminant / √ /
    tangent / empty branch** — the cone's `[0,∞)` per-nappe radial range meets the
    constant `r_c` at exactly one height per nappe, so valid coaxial input is
    *always* two circles; manufacturing a discriminant to mirror SSI7 would be a
    hack-to-pattern (P9/P10) and is prohibited. Branches: X2 (two circles, h>0
    nappe first) / **NC** (non-coaxial → `Err(AnalyticalSolutionNotAvailable)`,
    staged, never a fallback) / E1 (`r_c≤0`/non-finite, bad α, zero cone/cyl axis →
    `DegenerateInput`). Reuses `Circle` — no new curve type, no enum-match
    migration. RED enforces the anti-hack invariant (a 5×5 α/r_c sweep asserting
    `len()==2` always). Adversary (13 attacks): no bugs; parallelism + on-axis gate
    boundaries (each flips at `TAU_MODEL`), α near both E1 limits, reversed/
    antiparallel axes, a 525-config determinism sweep, and characterized
    absolute-`TAU` ceilings (on-surface oracle holds to r_c≈1e8 → breaks at 1e9;
    `d_ax` coaxial band holds to scale ~7e8 → conservatively flips to ASNA by ~1e9)
    plus the in-band snap-to-cone-axis slack. Spec
    `specs/ssi_pr_ssi8_cylinder_cone_coaxial.md`; commits `7d820153` (spec)→
    `45c4eed1` (RED)→`d25fa0cb` (GREEN)→`e3285699` (adversary). 217/217 ssi-rs
    tests; CI clean.
  - **Step 9 — `ssi-rs` cone∩cone coaxial (fourth & LAST circle-reducible
    degree-4 pair, PR-SSI9) ✅ DONE.** Reuses the SSI6/7/8
    coaxial-detect→reduce-to-circles→general-ASNA pattern. Coaxial cone∩cone (axes
    parallel AND apex₂ on the cone₁ axis line) reduces along the shared axis via
    `|t|·tanα₁ = |t−δ|·tanα₂` (`δ` = signed apex offset, `t` = axial height from
    apex₁) to the quadratic `(m₁²−m₂²)t² + 2m₂²δt − m₂²δ² = 0`. **No manufactured
    discriminant/√ sign gate (P9/P10):** the discriminant `(2m₁m₂δ)²` is a
    **perfect square** ⇒ always ≥0, so unequal-α offset input is *always* exactly
    two circles; the equal/unequal split and the apex-collapse are gated on the
    **linear** quantities `|α₁−α₂|` and `|δ|`, never on a square. Branches: X2 (two
    circles at `t=(−m₂²δ±m₁m₂|δ|)/(m₁²−m₂²)`, larger-t first) / X1 (equal α, offset
    → one circle at the bisector `t=δ/2`) / X0 (unequal α, apexes coincide →
    `Ok(vec![])` radius-0 point-circle) / **CO** (equal α + coincident → identical
    double cone → `Err(DegenerateInput)`) / **NC** (non-coaxial →
    `Err(AnalyticalSolutionNotAvailable)`, staged, never a fallback) / E1 (bad α
    either cone, zero axis either cone → `DegenerateInput`). Reuses `Circle` — no
    new curve type, no enum-match migration. RED enforces the anti-hack invariant
    (unequal-α × δ≠0 sweep asserting `len()==2` always). Adversary (10 attacks): no
    bugs; parallelism + on-axis gate boundaries (each flips at `TAU_MODEL`), the
    `|α₁−α₂|` equal/unequal and `|δ|` collapse boundaries, reversed/antiparallel
    axis-sign set-invariance, α near both E1 limits, a 40-config determinism sweep,
    cross-branch argument-swap symmetry, and characterized absolute-`TAU` ceilings
    (on-surface oracle holds to ~1e8 → breaks at 1e9; coaxial band conservatively
    flips to ASNA at large scale) plus the apex-grazing radius-0 (X0) collapse.
    Spec `specs/ssi_pr_ssi9_cone_cone_coaxial.md`; commits `da98380e` (spec)→
    `cc61f1bb` (RED)→`f960895d` (GREEN)→`6027d7c1` (adversary). 245/245 ssi-rs
    tests; CI clean.
  - **Step 10 — `ssi-rs` cylinder∩cylinder parallel axes → lines (PR-SSI10) ✅
    DONE.** First of the two cyl∩cyl special cases. Parallel-axis cyl∩cyl reduces
    to **circle∩circle** in the plane ⟂ the shared axis `û`, lifted along `û` →
    **lines** (reuse `SsiCurve::Line`). Inter-axis distance `d = |rel − (rel·û)·û|`;
    chord offset `a = (d²+r₁²−r₂²)/(2d)` along `n̂`, half-chord `h = √(r₁²−a²)`,
    `p̂ = û×n̂`, points `Q₁+a·n̂ ± h·p̂`. Gate on the **linear** `d` vs `r₁±r₂`:
    E1 (`DegenerateInput`) → NP (`|û₁×û₂|≥TAU` → ASNA) → coincident (d≤TAU, equal r
    → `DegenerateInput`, 2D overlap) → concentric (d≤TAU, unequal r → empty) →
    disjoint/contained (empty) → tangent (one line) → secant (two lines, +h·p̂
    first). Non-parallel stays ASNA (the equal-R intersecting → ellipses case is
    PR-SSI11). Reuses `Line` — no new curve type, no enum-match migration.
    **Adversary found a real bug:** a non-finite `axis_point` (NaN/Inf) leaked a
    NaN-bearing `Line` instead of erroring — `d=NaN` compares false against every
    branch threshold, so control fell through to the secant branch; the radius and
    axis_dir guards did not cover the point. Fixed with an early `axis_point`
    finiteness guard → `DegenerateInput` (the coaxial-detect siblings degrade to
    NC→ASNA on a NaN point, so the leak was unique to cyl∩cyl's curve-producing
    fall-through). Adversary 13 attacks (parallelism/tangent/coincident boundaries,
    argument-swap line-SET symmetry, antiparallel/non-unit axes, 36-config
    determinism sweep, characterized absolute-`TAU` oracle ceiling ~1e8 via an
    oblique config). **Process note:** the SSI10 worker hit the account usage limit
    mid-cycle after committing spec+RED; the interactive driver completed GREEN
    against the worker's frozen RED suite (test-author ≠ implementer preserved),
    spawned a distinct Adversary sub-agent, and fixed its finding. Spec
    `specs/ssi_pr_ssi10_cylinder_cylinder_parallel.md`; commits `fed67c3c` (spec)→
    `b53e55a2` (RED)→`7100c143` (GREEN)→`f0927aec` (adversary)→`721d7b23` (fix)→
    `7a18bb66` (adversary verify). 277/277 ssi-rs tests; fmt + clippy clean.
  - **Step 11 — `ssi-rs` cylinder∩cylinder equal-R intersecting axes → two
    ellipses (PR-SSI11) ✅ DONE.** Second of the two cyl∩cyl special cases, and
    the LAST circle/conic-reducible quadric pair. Two cylinders of **equal radius**
    whose axes are **coplanar and intersect** (non-parallel) meet in **two
    ellipses** lying in the angle-bisecting planes (Patrikalakis & Maekawa §5.8) —
    reuses the existing `SsiCurve::Ellipse` variant, no new curve type, no
    enum-match migration. **Unequal-radius or skew (non-coplanar) axes stay staged
    `Err(AnalyticalSolutionNotAvailable)`** — the general degree-4 curve, deferred,
    never a fallback (A15.2). Built via the role-separated RED/GREEN/ADVERSARY
    cycle (test-author ≠ implementer). Spec
    `specs/ssi_pr_ssi11_cyl_cyl_equal_r_ellipses.md`; commits `7f6e2d44` (RED)→
    `6bdcb05a` (GREEN)→`2e5e6e6f` (adversary). With Step 11, **ALL
    circle/conic-reducible coaxial & special-case quadric pairs are now complete.**
  - **PR-YR6 — curved `Surface`/`Curve` types + loud rejection (first Phase-2
    step). ✅ DONE.** Extends `yang-rs`'s `Surface` enum (`Sphere`, `Cylinder`,
    `Cone`) and `Curve` enum (`Circle`, `Ellipse`) with field shapes mirroring
    `ssi-rs` `QuadricSurface`/`SsiCurve` field-for-field (so the future Stage-3
    mapping is a trivial copy; radially-outward convention, no `sense` field).
    The pipeline **accepts curved faces at the type level** but **rejects them
    LOUDLY** — new `YangError::CurvedSurfaceNotYetSupported { face }` returned at
    the three `Surface::Plane` destructure sites (`BRep::new` winding
    canonicalization is the observable one; `boolean()` `plane_dist` closure and
    `reconstruct_topology` surface inheritance are defensive). P9/P10: never a
    panic, silent skip, or planar approximation. **No `ssi-rs` call and no curved
    tessellation exist yet** — this is a pure type extension. Spec
    `specs/yang_rs_curved_surface_curve_types.md`; role-separated cycle, commits
    `441e8748`/`076bf661` (RED + integration-test contract migration)→`0afdc6a3`
    (GREEN)→`07f6d12e` (adversary).
  - **PR-YR7 — P2a curved Stage-1 tessellation: CYLINDER only. ✅ DONE.**
    First curved-geometry *processing* step. `BRep::new` now dispatches by face
    surface type: a closed-solid cylinder (encoded with a seam edge — lateral
    `Surface::Cylinder` + 2 planar disk caps, no `BRepFace` two-loop change)
    tessellates into a watertight, chord-error-bounded mesh (`d_ε = 1e-2 ×
    AABB_diag`, `N` from `r·(1−cos(π/N)) ≤ d_ε`) with a correct
    `TessellationMap`. A shared per-`Circle`-edge rim-ring pre-pass gives
    cap+lateral identical rim vertices (watertight via shared indices, not
    snap-weld); `ortho_basis` is shared by sampling AND the new infallible
    `BRep::eval_source` bijection inverse (the round-trip oracle); the
    opposite-rim-normal twist is resolved by axis-frame azimuth alignment.
    Adds `signed_distance_to_surface` (Plane+Cylinder; Sphere/Cone loud) wired
    into `boolean()`'s distance closure. **No boolean wiring, no `ssi-rs` call,
    no exact intersection curves.** Sphere/Cone still reject loudly; the planar
    box path is unchanged; `reconstruct_topology` still defers cylinder (P2c).
    Cylinder-on-a-triangle is now `MalformedTopology` (lacks its 2 `Circle`
    rims), not `CurvedSurfaceNotYetSupported`. Spec
    `specs/yang_pr_yr7_cylinder_tessellation.md`; role-separated cycle, commits
    `16570a20` (spec)→`aca9d7e4` (RED + contract migration)→`b3dc3f65`
    (GREEN)→`81a3abcf` (adversary).
  - **PR-YR8 — P2c first curved boolean: cylinder ∪ box (mesh-approximate). ✅ DONE.**
    A curved solid runs through the WHOLE pipeline for the first time:
    `boolean(cylinder, box, Union)` flows Stage 2 (sidecar `LabeledArrangement`)
    → Stage 5/6 reassembly, and a kept lateral patch emits a `BRepFace` carrying
    `Surface::Cylinder` with the **input's exact parameters** (governance A15 —
    the mesh is a tool, the analytic surface is the truth). Two honest fixes:
    **(Blocker 1)** Stage-6 face resolution gains a **per-face membership
    tolerance** — `TAU_WORK` for `Plane`, the surface's own Stage-1 chord bound
    `d_ε` for `Cylinder` (NOT tolerance widening; the same bound Stage 1
    guarantees). The `1e-2 × analytic-AABB-diag` math is extracted into ONE
    shared `curved_chord_bound` helper consumed by both `BRep::new` (Stage-1
    `n_seg`) and face resolution (A14.3 single source). Applied to BOTH the
    non-degenerate count rule AND the degenerate-sliver branch (the sidecar
    emits a near-zero-area sliver ON the cylinder lateral whose centroid is
    ~`d_ε` inside the analytic surface — the §4-literal "keep TAU_WORK" was a
    planar-world assumption; the governing principle applies to any triangle on
    a curved face). Byte-for-byte identical for all-planar inputs (every face
    uses `TAU_WORK`; an all-planar solid has `band == None`) — **fuzz_boxes
    900/900 correct, 0 silent-wrong**. **(Blocker 2)** `reconstruct_topology`
    gains a `Surface::Cylinder` branch BEFORE the planar Newell/flip machinery:
    inherit the surface unchanged (Union = no cavity → no sense flip; curved
    Subtract cavity-sense deferred), reuse `patch_boundary_cycle`, keep the E2
    degenerate-loop guard, DROP the E3/`positive_count` + inherited-normal flip,
    deterministic loop assignment (most-edges = outer, tie-break lowest min
    start-vertex), edges = `Curve::LineSegment`. Sphere/Cone still loudly reject
    everywhere. **Verified against the live Cherchi sidecar:** cylinder ∪ box is
    watertight (0 unpaired half-edges), Euler V−E+F=2, analytic `Surface::Cylinder`
    survives — no F3 tie, no `NonManifoldOutput` (spec §5 STOP conditions all
    clear). **No `ssi-rs` call yet; intersection edges stayed mesh-approximate
    polylines — now made exact in PR-YR9 (P3).** Spec
    `specs/yang_pr_yr8_curved_boolean.md`; role-separated cycle, commits
    `c2a81e05` (RED)→`da85f4bd` (GREEN)→`56f395ba` (adversary).
  - **PR-YR9 — P3 Stage 3: exact intersection edges via `ssi-rs`. ✅ DONE.**
    The **first real use of `ssi-rs` inside the boolean** (Yang 2025 §4.3).
    `cylinder ∪ box` output intersection edges no longer carry P2c
    mesh-approximate `Curve::LineSegment` polylines — they carry the **EXACT
    analytical conic** from `ssi_rs::intersect`. An output intersection edge is
    an undirected mesh boundary edge incident to two patches of **different
    `InputId`** (one on a `Surface::Cylinder`, one on a box-cap `Surface::Plane`);
    `ssi_rs::intersect(Plane, Cylinder)` of those inherited surfaces is the
    plane∩cylinder solver → a `Circle` (cap ⟂ axis, canonical), `Ellipse`
    (oblique), or `Line`s (parallel). New helpers in `crates/yang-rs/src/lib.rs`:
    `surface_to_quadric` (yang `Surface` → ssi `QuadricSurface`; `Plane` point =
    `-d·n`), `ssi_curve_to_curve` (field-for-field; `Line`→`LineSegment`),
    `curve_contains_point` (implicit on-curve residual, no parameter solving),
    and `build_intersection_curves` (per A↔B edge: intersect, select the
    **unique** conic passing both endpoints within the cylinder owner's Stage-1
    chord bound `d_ε` — `TAU_WORK` for plane∩plane — keyed by canonical edge).
    `reconstruct_topology` refactored into two passes (a `PatchInfo` first pass
    owning the face-range check + inherited lookup in one place; an emission pass
    that sets each edge's `curve` via canonical-key lookup, falling back to
    `LineSegment` ONLY for non-intersection edges). The Newell/flip/E2/E3
    machinery is byte-unchanged. **P9 STOP**: a genuine `ssi_rs::intersect`
    failure or a non-unique selection (`matched != 1`) returns
    `Err(YangError::SsiRefinementFailed { edge, reason: SsiRefinementError })` —
    **never** a silent fallback to the polyline. **Scope held**: planar
    `fuzz_boxes` corpus stays all-`LineSegment` (plane∩plane → `Line` →
    `LineSegment`); same-input rim/seam edges keep `LineSegment` (no SSI entry);
    sphere/cone still loudly reject. Adversary proved the conic is analytic, not
    a mesh re-fit (**byte-identical cap `Circle` across N=8 vs N=16 facet mocks**).
    Role-separated cycle, commits `6e73a74d` (RED)→`f1c401f4` (GREEN)→`ec2b71d0`
    (adversary); spec `specs/yang_pr_yr9_stage3_ssi.md`.
  - **PR-YR10 — Stage 4: relocate mesh intersection points onto the exact
    curves + §4.5.3 reversed-point correction. ✅ DONE.** Yang 2025 §4.4.1 +
    §4.5.3 mesh updating. PR-YR9 gave the output edges exact analytical conics,
    but the **mesh** still had its intersection-edge vertices on the faceted
    polygon chords (inside the true circle by up to the Stage-1 chord bound
    `d_ε`). Stage 4 now **relocates** those crossing points radially onto the
    exact `Curve::Circle` (closed-form `project_onto_circle`, reusing
    `ortho_basis` so the angle `t` round-trips through `eval_source`), retags
    each moved/on-curve vertex's `TessellationSource` to `BRepEdge{edge,t}`, and
    runs the §4.5.3 reversed-intersection sweep on the ordered, oriented conic
    loops (discrete tangent `t̃` vs curve tangent; reversal ⟺ unsigned angle ∈
    (45°,135°), 1e-6 rad slack; edge-collapse the next point, reconnect, repeat;
    collinear `t̃` = healthy). **Watertightness is INHERITED** from the
    mesh-boolean output and gated by a combinatorial `check_watertight_2manifold`
    (half-edge pairing + Euler χ=2) — **not** a global CDT (per §4.4.3). Seam:
    `reconstruct_topology` takes `&mut Mesh`, enters Stage 4 on ANY analytic
    conic edge (Circle **or** Ellipse), and returns the per-vertex source vector
    `boolean()` uses for the output `TessellationMap`; Phase-B emission is
    otherwise unchanged. **Verified on the live sidecar**: `cylinder ∪ box`
    relocates every cap-ring crossing onto the exact circle to `TAU_MODEL`,
    chord deviation drops, output stays watertight χ=2; the adversary's
    independent threshold-free geometric audit (1000×1000 per-cap winding sweep +
    net-signed-area = exact analytic region to ~1e-16) found **NO cap fold**.
    **Scope / loud STOPs (P9/P10)**: circle projection only — an `Ellipse`
    intersection edge → `Err(Stage4RegionInvalid{EllipseProjectionUnsupported})`;
    §4.5.2 real local refinement is a loud `LocalRefinementRequired` STOP (the
    canonical fixture never triggers it); `OnAxis`/`OffCurveBeyondChordBand` are
    defensive guards (public-path-unreachable — the upstream YR9
    `curve_contains_point(·, d_ε)` selection is the same, strictly-tighter band,
    so a pathological crossing is rejected by `SsiRefinementFailed`/
    `FaceResolutionFailed` first; a `processed`-set no-skip audit forbids silently
    passing any conic endpoint — the failure mode of the **disproven**
    insert-and-fan attempt, branch `wip/yr10-insert-fan-disproven` commit
    `46980456`, which must NOT be repeated). **Planar path byte-identical**
    (Stage 4 early-returns when no conic edge exists; `fuzz_boxes` 900/900
    unchanged). One faithful spec→reality correction (adversary-verified, NOT a
    hack-to-green): the §4.5 step-4 literal per-facet "winding vs analytic
    normal, dot>0" gate was **removed** — on a faceted curved surface a facet
    normal legitimately deviates from the pointwise centroid normal, and a cap
    facet's kept winding is reconciled downstream by `reconstruct_topology`'s
    Newell orientation pass; orientation correctness is delegated to the §4.5.3
    sweep (loop monotonicity) + the watertight gate, exactly where Yang §4.4.3
    places it. Sphere/Cone still loudly reject. Role-separated cycle, commits
    `5a2da9f0`/`d7540578` (spec)→`d4bbe446`/`03464b29` (RED + fixture
    recalibration)→`e49a5a93` (GREEN)→`d402aa80` (adversary); spec
    `specs/yang_pr_yr10_stage4_relocate.md`.
  - **PR-YR11 — Stage 4 OBLIQUE: relocate onto the exact ellipse. ✅ DONE.**
    Lifts PR-YR10's `EllipseProjectionUnsupported` STOP for oblique
    `cylinder ∪ box`: an `Ellipse` intersection edge now relocates via the
    **cylinder parameterization** (snap radius at angle θ, then snap axial to the
    cutting plane → lands on `cylinder ∩ plane` exactly, closed-form, no quartic;
    Yang §4.3.2), with the §4.5.3 reversal sweep extended to `Ellipse` loops and
    the N3 degenerate-tangent fix preserved. **Verified on the live sidecar:** a
    *contained* oblique `cylinder ∪ box` (tilt `unit([1,0,3])`, axis through the
    box centre so both cap ellipses + the body stay inside the unit box — no
    side-face exit) relocates every crossing onto the exact ellipse (on BOTH the
    cylinder and the cutting plane to `TAU_MODEL`), chord deviation drops, output
    watertight χ=2. yr10 `t4` migrated Err→Ok (the Ellipse edge now relocates, not
    rejects — faithful contract migration). **Out of scope (deferred):**
    side-face-exit / ellipse∩line corner (triple-point) configs — the contained
    fixture avoids them; a loud-STOP guard for them is a follow-up. Commit
    `e72f2313`; `tests/yr11_stage4_ellipse.rs`. 170/170 yang-rs; fmt + clippy clean.
  - **PR-YR12 — P2b sphere Stage-1 tessellation. ✅ DONE.**
    The remaining curved Stage-1 primitive (after the PR-YR7 cylinder). A closed
    solid sphere — one `Surface::Sphere` face bounded by a single `Curve::Circle`
    meridian seam + 2 pole `BRepVertex` (no `BRepFace` two-loop change) —
    tessellates via `BRep::new` into a watertight (χ=2) lat/long grid mesh with a
    bijective `TessellationMap`. Fixed **z-up** parameterization
    `face_eval(u,v)=center+r·(cos v cos u, cos v sin u, sin v)` (a sphere is
    isotropic — an oriented sphere is a documented out-of-scope limitation, like
    the cylinder needing `axis_dir`); chord bound **`d_ε = 1e-2·2r√3`** (the
    sphere's exact AABB space diagonal — diameter `2r` ≠ diagonal); grid `n_lon`/
    `n_lat` refined honestly (segments sized to `d_ε/2` so triangle *centroids*
    stay ≤ full `d_ε` — more triangles, **never** tolerance widening; worst
    centroid dev 0.82·d_ε at n_lon=17/n_lat=9 for the unit sphere). Poles are the
    shared seam-vertex indices and the seam column is reused via modular wrap →
    watertight with no weld/snap/synthetic fill (verified on the live Cherchi
    `inputcheck` sidecar). `eval_source` Sphere FACE arm is byte-identical to
    `face_eval` (round-trip exact to 1e-9 over pole/seam/interior verts);
    `signed_distance_to_surface` Sphere → `|x−center|−r`. The rim-ring pre-pass
    excludes sphere-seam Circle edges so the **cylinder path is byte-for-byte
    unchanged** (`tests/yr7_cylinder.rs` diff empty). **Cone still rejects**;
    sphere-on-a-triangle is now `MalformedTopology` (lacks its seam Circle), a
    faithful guard migration swept across yr6/yr7/yr8/yr9 (cone arms keep their
    exact `CurvedSurfaceNotYetSupported { face: N }` assertions). **No boolean
    wiring, no `ssi-rs`, no exact intersection curves, no NURBS.** Spec
    `specs/yang_pr_yr12_sphere_tessellation.md`; role-separated cycle, commits
    `07c8cbe3` (spec)→`ee66cca3` (RED)→`b5b17e47` (GREEN)→`7e96e070` (adversary).
    184/184 yang-rs; fmt + clippy clean.
  - **PR-YR13 — curved `Subtract` box − cylinder, cavity-sense via
    `BRepFace.reversed`. ✅ DONE.** The first M5 increment after the curved
    `Union` chain (PR-YR8–YR11) and the curved Stage-1 primitives (PR-YR7/YR12).
    Closes the curved cavity-sense gap banked in PR-YR8 for the **`box − cylinder`
    BLIND POCKET** (genus 0, χ=2). A new `BRepFace.reversed: bool` records that a
    face's effective outward normal (outward from the result solid) is the
    **negation** of the surface's canonical analytic outward normal: the surviving
    cylinder-lateral cavity wall is emitted as `Surface::Cylinder` with the input's
    **exact** params and `reversed == true`, so its effective normal points
    **toward the axis** (into the pocket). `reversed` is derived from the SAME
    `flip_for_op` signal that flips the mesh winding —
    `op == Subtract && info.input == InputId::B` (threaded `boolean()` →
    `reconstruct_topology_stage4` → `emit_topology`) — so face sense and mesh
    winding are **provably consistent** (witnessed absolutely: the emitted
    cavity-wall mesh-triangle winding normals point toward-axis and the result has
    positive signed volume). Planar faces keep encoding sense in the
    possibly-flipped `Plane.normal` and stay `reversed == false` (no double-flip);
    surface params are never perturbed to signal sense. Union + planar Subtract
    are byte-identical (`reversed == false` everywhere). Faithful `reversed: false`
    migration swept across all `tests/*.rs` + the `#[cfg(test)]` lib fixtures
    (additive only). Adversary independently witnessed mesh↔`reversed` consistency
    on a second outward-oriented mock + mutation-verified the derivation is
    load-bearing. **Remaining curved-Subtract gaps:** through-hole (genus 1, χ=0),
    sphere/cone cavities (`Cone` still rejects loudly), box-as-subtrahend,
    side-face-exit / corner (triple-point) guard; cut-surface faces (PR-YR5
    deferral) still open. **No new `ssi-rs`.** Spec
    `specs/yr13_subtract_cylinder_cavity_sense.md`; role-separated cycle, commits
    `c4abc69d` (spec)→`78f73f65` (RED)→`42972890` (GREEN)→`3839f558`/`41819459`
    (RED fixups)→`85abbc10`/`86791834` (adversary). 195/195 yang-rs; fmt + clippy
    clean.
  - **PR-YR14 — through-hole genus-1 Subtract; per-shell Euler gate generalized
    to χ=2−2g. ✅ DONE.** Extends curved `Subtract` from PR-YR13's BLIND POCKET
    (genus 0, χ=2) to a **THROUGH-HOLE**: the cylinder passes fully through the
    box (both caps OUTSIDE the box) → a cylindrical tunnel, which is a single
    connected closed orientable 2-manifold of **genus 1 → χ = 0**. The ONE
    production change is in `check_watertight_2manifold`: the per-shell Euler gate
    was `V−E+F == 2` ("each shell is a sphere"), which wrongly rejected the χ=0
    result. Generalized to accept **χ = 2−2g for g ≥ 0** (χ even, ≤ 2) and reject
    odd χ or χ > 2 — impossible for a closed orientable manifold, so still a LOUD
    `NonManifoldOutput` (NOT a tolerance/fallback relaxation; P9/P10). The directed
    half-edge pairing loop stays strict and untouched. Everything else the
    through-hole needs already worked and is REUSED unchanged: the curved cavity-
    sense (`BRepFace.reversed` from `op==Subtract && input==B`, PR-YR13) on the
    tube wall, the annular box top+bottom faces (PR-YR5c multi-cycle /
    `positive_count==1`), the two-rim tube wall (one connected same-attribution
    patch → `patch_boundary_cycle` returns its two boundary cycles → curved branch
    emits outer+inner loops), and the **two** exact `Circle` rim edges (cylinder ∩
    box-top at z=2 AND cylinder ∩ box-bottom at z=0). Adversary independently
    witnessed mesh-winding ↔ `reversed` consistency on a SECOND outward-oriented
    through-hole mock (r=1.5, N=24, signed_volume>0, χ=0, wall winding toward-axis)
    and mutation-verified the χ relaxation is LOAD-BEARING for the accept path
    (reverting to `!= 2` turns the through-hole oracles red). Honest coverage note:
    the χ-clause's REJECT branch (odd/`>2`) is mutually shadowed on the reachable
    corpus by the half-edge-pairing and coincident-triangle guards — defects are
    still loudly rejected, never `Ok` (oracle `a6` pins this). All genus-0 cases
    (`fuzz_boxes`, YR8–YR13, YR13 blind-pocket χ=2) byte-unchanged. **Remaining
    curved-Subtract gaps:** sphere/cone cavities (`Cone` still rejects loudly),
    box-as-subtrahend, side-face-exit / corner (triple-point) guard; cut-surface
    faces (PR-YR5 deferral) still open. **No new `ssi-rs`.** Spec
    `specs/yr14_subtract_through_hole.md`; role-separated cycle, commits
    `36d2a7c4` (spec)→`aeefb4cc` (RED)→`b52a78a3` (GREEN)→`6995b6ec` (adversary).
    208/208 yang-rs; fmt + clippy clean.
  - **PR-YR15 — box − sphere HEMISPHERICAL DIMPLE (genus 0); `Surface::Sphere`
    wiring + `sphere_chord_bound` single-source helper. ✅ DONE.** Extends the
    curved `Subtract` cavity path to a **spherical** cavity: a sphere centred ON
    one box face (poking through exactly that face) so `box − sphere` carves a
    hemispherical dimple — a single shell, **genus 0 (χ=2)**, ONE exact great-
    `Circle` rim (`sphere ∩ box-face plane`, great because the centre is ON the
    plane), and a cavity wall that is the inside hemisphere (`Surface::Sphere`,
    `reversed=true`, effective outward normal pointing INTO the dimple toward the
    centre). The cavity-sense mechanism (`BRepFace.reversed` from
    `op==Subtract && input==B`, PR-YR13) and the per-shell Euler gate (χ=2−2g,
    PR-YR14) are surface-agnostic and REUSED unchanged. The work was honest wiring
    of an already-type-supported surface, NOT new mechanism: `Surface::Sphere` was
    loudly rejected at production sites that each mirror the existing `Cylinder`
    arm. **The plan named three sites; the live-sidecar oracle surfaced two more**
    — both governed by the spec's I-sphere-band invariant (a sphere face uses its
    OWN Stage-1 chord bound `sphere_chord_bound(radius)=1e-2·2r√3`, NOT the rim-
    AABB `curved_chord_bound`=2r√2, which underestimates). Final FIVE faithful
    edits: (1) `surface_to_quadric` Sphere→`QuadricSurface::Sphere` (enables the
    exact `plane ∩ sphere` rim); (2) `sphere_chord_bound` free helper extracted
    from `tessellate_sphere_face` (A14.3 single source) + the `tol_for` Sphere arm;
    (3) `emit_topology` curved-branch guard broadened to
    `Cylinder | Sphere` (body unchanged — already surface-agnostic);
    (4) `build_intersection_curves` selection tol (factored to
    `chord_tol_for_curved_owner`; sphere arm uses `sphere_chord_bound`);
    (5) `stage4_chord_band` relocation budget via new `input_curved_chord_bound`
    (max of rim-AABB and per-sphere-face bound). **No tolerance widening, no
    fallback** — each uses the surface's GUARANTEED Stage-1 bound; `Cone` stays a
    loud reject at every site; cylinder/all-planar inputs byte-for-byte. Adversary
    mutation-verified BOTH extra sites are load-bearing (each mutation reds a
    distinct oracle: `AmbiguousCurve{matched:0}` and `Stage4RegionInvalid{
    OffCurveBeyondChordBand}`), witnessed mesh-winding ↔ `reversed` consistency on
    a SECOND independent OUTWARD-authored off-axis mock (center (1,−0.5,5), r=1.5,
    different facet counts; χ=2, signed_volume>0, cap winding toward-centre), and
    confirmed no migration weakened. **Honest coverage note:** the two extra
    band sites are exercised only via the sidecar-backed oracle (the C++
    `mesh_booleans` binary is present in this env, so they ARE verified here); the
    mock-driven oracles 1–4 bypass real SSI/Stage-4, so on a machine WITHOUT the
    sidecar those two edits are untested — a future mock-path Stage-3/4 sphere-rim
    oracle would close that gap. All prior cases (`fuzz_boxes`, YR8–YR14)
    byte-unchanged. **Remaining curved-Subtract gaps:** cone cavities (`Cone`
    still rejects loudly), fully-internal spherical voids (multi-shell),
    through-sphere, box-as-subtrahend, side-face-exit / corner (triple-point)
    guard. **No new `ssi-rs`** (the `plane_sphere` solver already existed). Spec
    `specs/yr15_subtract_sphere_dimple.md`; role-separated cycle, commits
    `4b0b5af0` (spec)→`945c1c12` (RED)→`6e6239f0` (GREEN)→`bcdeecc4` (adversary).
    219/219 yang-rs; fmt + clippy clean.
  - **PR-YR16 — CONE Stage-1 tessellation (CONE only, no boolean); all three
    curved primitives now tessellate. ✅ DONE.** The cone was the last curved
    primitive still rejecting everywhere (`Surface::Cone` →
    `CurvedSurfaceNotYetSupported`). This PR teaches `BRep::new` to tessellate a
    closed solid cone, verified by the same 4-oracle contract as PR-YR7/YR12
    (surface-to-mesh ≤ d_ε, watertight + 2-manifold + env-gated `inputcheck`,
    bijection round-trip, Euler χ=2) over a corpus of 4 (z-up unit, wide-short,
    tall-thin, off-axis non-unit axis). **Encoding (minimal, justified):** the
    cone lateral is topologically a DISK — its only boundary is the base circle,
    the apex is a single interior singular point — so NO seam edge (unlike the
    cylinder). `verts=[apex, base_seam]`; one shared base-rim `Curve::Circle`
    (shared by lateral + base cap = the watertightness mechanism); faces
    `[Cone lateral, Plane base cap]`. The apex is a pre-seeded `BRepVertex`
    located in `tessellate_cone_face` by exact `TAU_MODEL` position match (no
    duplicate → watertight + Euler hold; round-trip via `BRepVertex`).
    **Tessellation = apex fan + base cap fan** over the shared rim ring (cap
    reuses the existing `tessellate_cap_face` unchanged). Because the cone is
    **ruled** (straight generators apex→rim, exactly on the surface), the worst
    residual anywhere on a lateral triangle — including its centroid — is the
    base-rim sagitta `R·(1−cos π/N)`; there is NO centroid amplification (unlike
    the sphere) and NO `/2` factor in N-sizing. **`cone_chord_bound(h,α)=
    1e-2·√((2R)²+h²)`** is the new single source (A14.3), folded into the rim
    pre-pass via `min(curved_chord_bound, cone_chord_bound)` ONLY when a cone
    face is present (cylinder/sphere/planar paths byte-for-byte) — the min is
    load-bearing for wide-short cones (`h<2R`, where the rim-AABB bound
    overestimates the honest bound). **Tilted outward normal** (A15.5)
    `n̂=unit(r̂−tanα·â)` ⟂ the generator (new `cone_outward_normal`). Three Stage-1
    production sites changed (`BRep::new` dispatch, `eval_source` cone-FACE arm,
    `signed_distance_to_surface` signed radial residual); the boolean-path cone
    rejections (`surface_to_quadric`, `emit_topology`, Stage-6 reassembly) stay
    LOUD — the cone never enters the boolean this PR. After this PR
    `CurvedSurfaceNotYetSupported` is no longer reachable from `BRep::new` for any
    curved surface on a triangle (all → `MalformedTopology`); it survives only on
    the boolean Stage-6 paths. **Guard migration:** the plan named 9 sites; the
    spec sweep surfaced 3 more (+1 inline `src/lib.rs` test) — exactly the
    under-enumeration the `yang_curved_primitive_guard_migration` lesson and the
    YR15 precedent anticipate — all migrated faithfully (only the expected
    outcome changed; every structural assertion preserved). **Two honest
    adversary findings (documented, no defect):** (1) oracle 1's 3-verts+centroid
    sampling could not distinguish the rim-only N from the correct N (the
    distinguishing sample is the base-edge MIDPOINT at f=1) — the adversary added
    `adv_wide_short_base_edge_midpoint_within_cone_bound`, which reds the
    dropped-`min` mutation with the exact residual-exceeds-bound assertion, so the
    min IS mutation-verified load-bearing; (2) the tilted normal produces a
    BYTE-IDENTICAL mesh to the pure-radial normal for the current pure apex-fan
    (`orient_tri`'s binary flip is identical at every steepness since both r̂ and
    n̂ share the fan triangle's half-space) — it is the *correct* surface normal
    but **orientation-dead-code until interior-ring (non-fan) triangles appear**
    (i.e. PR-YR17 cone cavity → a YR17 winding canary). The adversary pinned the
    math witness (`n̂·ĝ≈0` vs `r̂·ĝ≈0.565`) rather than fabricating a false catch.
    Independent second off-axis mock witnessed outward winding + `signed_volume>0`
    (per `yang_mock_orientation_witness`). 232/232 yang-rs; fmt + clippy
    `--all-targets` clean. Spec `specs/yr16_cone_tessellation.md`; role-separated
    cycle, commits `8e569c14` (spec)→`8ceb8d65`/`8d2fe8c6` (RED + clippy
    fixup)→`7f0dfe4e` (GREEN)→`6013a1fc` (adversary). **Next: PR-YR17 cone cavity
    `box − cone`.**
  - **PR-YR17 — box − cone CONICAL POCKET (curved `Subtract`, genus 0). ✅ DONE.**
    Closes the loop PR-YR16 opened: a cone with its **apex inside the box** (pocket
    bottom, `(0,0,0.5)`) and its **base above the box top** carves a conical pocket
    via `box − cone`. Result = a single genus-0 shell (χ=2): cavity wall = the cone
    lateral apex→rim (`Surface::Cone`, `reversed == true`), rim = exact `Circle`
    (`cone ∩ box-top plane`, a **perpendicular** cut → `ssi-rs` `plane_cone` C1
    branch → `radius = |h|·tanα`), apex = a singular pocket-bottom vertex closing
    the fan, box-top = an annular planar face (rim hole). This is pure
    **composition** — the cavity-sense mechanism (`BRepFace.reversed = op==Subtract
    && input==B`) is surface-agnostic and unchanged; the job was flipping the cone's
    loud-rejects to real wiring mirroring the Cylinder/Sphere precedent.
    **Production sites (`src/lib.rs` only, NO `ssi-rs` change):** the spec named
    FOUR — `emit_topology` curved-branch `matches!` (admit `Surface::Cone`),
    `emit_topology` defensive arm (drop Cone), `surface_to_quadric` (field-for-field
    → `QuadricSurface::Cone`, enabling the exact rim `Circle`), and `tol_for` (the
    cone's OWN `cone_chord_bound`, **height derived from the rim Circle in the cone
    face's outer loop** via the Stage-1 pre-pass idiom `|(rim_center−apex)·â|` —
    single-source bound, NOT tolerance widening). The GREEN implementer surfaced a
    **FIFTH** site (exactly the `yang_curved_primitive_guard_migration`
    under-enumeration the YR15/YR16 cycles anticipate): `build_intersection_curves`
    Stage-4 rim-curve selection had Cylinder/Sphere chord-tol arms but no Cone arm,
    so a cone∩plane rim edge fell to `TAU_WORK` and failed `curve_contains_point`
    against the exact circle (`AmbiguousCurve{matched:0}`). Added
    `cone_chord_tol_for_owner`, a faithful mirror of `chord_tol_for_curved_owner`
    (same loud-on-missing-rim producer-fault path, the cone's own single-source
    `cone_chord_bound`). **Confirm-or-STOP (P9/P10):** the `tol_for` cone-height
    anchor was verified (temporary `eprintln!`, removed) before coding; no
    widening, no fallback. **Honest adversary findings (no defect):** a second,
    distinct conical-pocket mock (shallow box, apex `(0,0,0.25)`, `tanα=2`) witnessed
    winding ↔ `reversed` sampling **edge midpoints**; mutation-verified that flipping
    `reversed` (M1), perturbing the cone params (M2), killing the SSI Cone arm (M3a),
    and breaking the fifth-site bound→`TAU_WORK` (M3b, reds the sidecar oracle), and
    flipping the tilted-normal **sign** (M4) each red a DISTINCT oracle. **M4b**
    (pure-radial, *correct* sign) reds NO oracle — **confirming** the YR16 finding:
    the cone cavity wall is still a pure apex-fan, so `orient_tri`'s binary flip is
    byte-identical for `r̂` and `n̂=unit(r̂−tanα·â)`; the **sign** is load-bearing, the
    tilt **magnitude** stays orientation-dead-code until interior-ring (non-fan)
    triangles appear (per `yang_cone_tessellation_oracle_findings`). The fifth site
    is verdicted a **faithful extension, not a tolerance hack** (M3b proves the bound
    does real work). The env-gated `Subtract` sidecar-parity oracle ran for REAL
    (default sidecar present), exercising the full pipeline (real `plane_cone` →
    Circle) end-to-end. Curved `Subtract` now covers **cylinder + sphere + cone**.
    **Still deferred (LOUD):** through-cone / cone-base-subtracted (two rims),
    **oblique cuts** (ellipse / parabola / hyperbola rims — the `plane_cone` non-C1
    branches), fully-internal cone void (multi-shell), side-face / corner
    (triple-point) exit, box-as-subtrahend. Full `yang-rs` crate green; fmt + clippy
    `--all-targets` clean. Spec `specs/yr17_subtract_cone_cavity.md`; role-separated
    cycle, commits `f9d597d8` (spec)→`f6a06012` (RED)→`f21434a1` (GREEN)→`741b50f1`
    (adversary).
  - **Non-convex / holed planar Stage-1 tessellation** ✅ (PR-NC1): planar faces
    with a reflex vertex (non-convex outer loop) **or** inner loops (holes) now
    tessellate via a constrained Delaunay triangulation
    (`cherchi_rs::cdt_polygon_with_holes`, backed by `spade` v2) instead of the
    fan path. No interior Steiner points, no boundary subdivision (the
    `TessellationMap` 1:1-on-boundary bijection is preserved); convex/box faces
    stay byte-for-byte on the existing fan path (`fuzz_boxes` 900/900
    unregressed). Resolves the D1-class (no ear-clipping) concern for the new
    kernel's planar Stage 1. Spec `specs/yang_pr_nc1_nonconvex_cdt.md`; deviation
    ledger **N9**.
  - **Curved boolean fuzz (the robustness-envelope map)** ✅ (PR-CF1): the curved
    analog of `fuzz_boxes` — a deterministic N=300 **correct-or-loud** fuzz over
    `boolean({cylinder|sphere|cone}, box, {Union|Subtract}, &sidecar)`
    (`tests/fuzz_curved.rs`, SEED=`0xcf1cadef00d2026`). Every `Ok` is audited
    (watertight `unpaired==0` / χ **==sidecar-reference χ** & even / analytic-surface
    survival with exact params / on-surface residual ≤ `TAU_MODEL` sampled on the
    exact `Curve::Circle/Ellipse` against BOTH incident surfaces / `vol>0` /
    chord-band volume envelope scaled from the Stage-1 `d_ε`); every `Err` is
    bucketed by `YangError` variant **and sub-reason**. Empty-result agreement
    (both engines ∅ → `ok_correct`; disagreement → silent-wrong) is a deliberate
    contract interpretation, not a relaxation. **Histogram (300 cases, all
    accounted for): `ok_correct=42`, `SILENT_WRONG=0`, `classified_err=257`,
    `skipped_bad_input=0`.** This **is the M5-gap map**: the dominant loud refusals
    are `SsiRefinementFailed::AmbiguousCurve` (183 — the SSI rim-selection gap),
    `FaceResolutionFailed` (54), and cone's `Stage4RegionInvalid::LocalRefinementRequired`
    (17); cone Union/Subtract are 0/0 correct (all loud refusals — oblique/ssi gaps),
    while sphere & cylinder land some correct results. Most `Subtract` cases are
    correctly loud because `boolean(prim, box, Subtract)` = `prim − box` =
    **box-as-subtrahend**, the DEFERRED direction (opposite of the `box − prim`
    demos). **ONE genuine production defect surfaced (a P9 violation): `boolean()`
    PANICKED** on sphere − box at case#23 — `emit_topology`'s curved branch indexed
    `cycles[outer_idx]` on an **empty** `cycles`. **GREEN fix** (`src/lib.rs`,
    commit `a568d9e6`): a minimal `if cycles.is_empty() { return
    Err(NonManifoldOutput); }` guard on the curved branch (+ a defensive mirror on
    the structurally-identical planar branch, latent since the all-planar fuzz never
    produces empty cycles), mirroring the adjacent E2/E3 degenerate-reassembly
    guards — converting the panic into a loud classified `Err`, so the fuzz now holds
    correct-or-loud (panic→Err ⇒ `PANICKED=0`). **Mechanism correction (adversary):**
    the case#23 sidecar reference is **NON-empty** (272 tris, vol≈0.0485) — the empty
    curved cycles are a reassembly artifact of the **deferred box-as-subtrahend
    direction**, NOT an enclosed-sphere empty solid; the loud `Err` is a legitimate
    refusal of an out-of-scope direction, not a suppressed wrong `Ok`. **Adversary
    verdict (`tests/cf1_adversary.rs`, commit `0771dcc6`):** all invariants real and
    discriminating (proved inside-out caught by `vol>0`; determinism replays case#23
    from SEED), GREEN fix principled; one noted **non-blocking GAP** — the chord-band
    volume alone is a coarse dropped-chunk detector (~0.38 at r=0.6), so χ==ref +
    on-surface residual are the real dropped-chunk gates (by design). The asserting
    fuzz stays `#[ignore]`d (default `cargo test -p yang-rs` green); an `#[ignore]`d
    `demonstrator_case23_*` pins the seed. **Follow-up increments seeded by the
    histogram:** the SSI `AmbiguousCurve` rim-selection gap (the single biggest
    blocker to curved `ok_correct`), `FaceResolutionFailed` coverage, cone Stage-4
    `LocalRefinementRequired`, and eventually the deferred box-as-subtrahend
    direction. Spec `specs/yang_pr_cf1_curved_boolean_fuzz.md`; role-separated cycle,
    commits `f0ea2e24` (spec)→`884726f5` (RED)→`a568d9e6` (GREEN)→`0771dcc6`
    (adversary).
  - **PR-YR18 — Stage-5 intersection-edge attribution fix (the CF1 `AmbiguousCurve`
    dominant-refusal). ✅ DONE.** Re-diagnoses CF1's biggest loud bucket: the
    `SsiRefinementFailed::AmbiguousCurve` mass (183/300 in the CF1 histogram) is
    **NOT** the "SSI rim-selection gap" the CF1 note guessed — a driver
    investigation found **0 cases with `matched ≥ 2`; every `AmbiguousCurve` is
    `matched == 0`**, and the bulk is **cylinder + sphere** (both fully handled by
    `ssi-rs`), not missing conics. It is a **surface-attribution defect**:
    `compute_phase_a` pushes a patch's single `info.inherited` face surface onto
    *every* boundary edge of the patch cycle (`src/lib.rs:3279-3289`), so a seam
    edge shared by two patches gets tagged `(surfA, surfB)` and handed to
    `ssi_rs::intersect` even when **one endpoint is genuinely off one surface**
    (decisive case: a cylinder∩plane edge, `tol≈3.1e-2`, one endpoint on both
    surfaces, the other `~8.9e-2` — ~2.9× the chord band — off the plane). Such an
    edge is an internal facet edge of a *single* surface, not a true intersection
    arc; the returned curve cannot pass through both endpoints → `matched == 0`.
    The SSI math is correct (`candidates == 1`); the defect is the
    **classification**. **GREEN fix (`src/lib.rs` `build_intersection_curves`
    only):** reorder so the Stage-1 chord band `tol` is computed FIRST, then gate
    each candidate edge with an **on-both-surfaces predicate** — both mesh
    endpoints must satisfy `|signed_distance_to_surface(surf, p)| <= tol` for BOTH
    attributed surfaces — *before* `ssi_rs::intersect`. A failing edge `continue`s
    and falls through to the unchanged `Curve::LineSegment` fallback in
    `emit_topology` instead of raising `AmbiguousCurve`. **No tolerance widening
    (P9/P10):** the gate reuses the SAME per-edge `tol` the selection already uses
    (the producer-fault helpers' diagnostic-only `candidates` arg is passed `0` in
    the pre-intersect position — untested). **No-regression invariant (proof):**
    the intersection curve lies ON both surfaces, so any edge that currently
    selects `matched == 1` necessarily passes the gate — the gate is a *necessary
    condition* of existing success and cannot regress YR8–YR17 or the planar
    corpus; it only reclassifies edges that today raise `AmbiguousCurve` with an
    endpoint off a surface beyond `tol`. Coincident-plane / yr9 loud STOPs
    preserved (both endpoints on both surfaces → pass the gate → reach the loud
    path); cone conics stay loud (a true cone∩plane edge passes the gate then still
    hits `matched != 1` because `curve_contains_point` returns `false` for conics —
    correct, the deferred analytic-conic follow-up). **Before/after counts:** CF1
    baseline = `AmbiguousCurve == 183` (cylinder + sphere `matched == 0` bulk). The
    **empirical post-fix sidecar-fuzz histogram could NOT be obtained in this
    container** — the Cherchi sidecar subprocesses zombie out and the
    `fuzz_curved` harness hangs without printing a final histogram (pervasive
    un-reaped `<defunct>`/`Z` processes, some days old, independent of this
    change); repeated N=300/120/40 runs all stalled. Per
    `feedback_no_regression_chasing` / "don't loop", no numbers were fabricated.
    Correctness evidence is instead **deterministic, sidecar-free**: a RED fixture
    (`tests/yr18_attribution.rs`) that reproduces the EXACT cylinder∩plane
    `matched == 0` case (`AmbiguousCurve { candidates: 1, matched: 0 }`, edge
    `(0,1)`, off endpoint 2.90× the band) and goes GREEN under the fix; the
    no-regression invariant (proof, statically audited by the adversary); and the
    adversary over-skip guard (`tests/yr18_adversary.rs`) proving genuine cap-ring
    cylinder∩plane edges still pass the gate and emit `Curve::Circle` (the RED
    test's negative-only assertions cannot catch a degenerate skip-everything
    "fix"). Full `cargo test -p yang-rs` green; `cargo fmt -p yang-rs -- --check`
    and `cargo clippy -p yang-rs --all-targets -- -D warnings` clean. Spec
    `specs/yr18_intersection_edge_attribution.md`; role-separated cycle, commits
    `ea94cc1c` (spec)→`5536432b` (RED)→`2345b791` (GREEN)→`44dc1cde` (clippy
    chore)→docs+adversary. **Empirical delta (driver-verified post-merge, curved
    fuzz N=90 same seed, before→after):** `ok_correct` **11 → 37** (3.4×);
    `AmbiguousCurve` **56 → 30**; **cylinder `AmbiguousCurve` eliminated entirely
    (21 → 0)**; sphere materially improved (20 → 15); cone unchanged (15 → 15, the
    deferred-conic share); **`SILENT_WRONG` 0 → 0** (safety bar held). The worker
    itself could not obtain these numbers (Cherchi sidecar subprocesses zombie out
    in-container); the driver reproduced the run successfully at N=90. **Deferred
    follow-ups:** (a) analytic-conic support (`Parabola`/`Hyperbola` for oblique
    cone cuts) so true cone∩plane edges that pass the gate stop being loud — the
    remaining cone `AmbiguousCurve=15`; (b) the residual **sphere**
    `AmbiguousCurve=15` (a distinct, smaller cause — the gate only partially
    cleared sphere; needs its own diagnosis).
  - **PR-YR19 — sphere∩plane chord-band metric consistency (the residual sphere
    `AmbiguousCurve`). ✅ DONE.** Diagnoses PR-YR18's deferred follow-up (b): the
    15 residual sphere `AmbiguousCurve` cases are all `surf0=Sphere`,
    `surf1=Plane`, `candidates == 1` (sphere∩plane is never ambiguous). The mesh
    endpoints PASS the YR18 on-both-surfaces gate (within `tol` of both surfaces
    along the surface normal) but FAIL `curve_contains_point` because the
    **in-plane radial** deviation `|radial − r_circle|` exceeds the flat `d_ε`,
    even though the **sphere-normal** distance is within `d_ε`. A **metric
    inconsistency**, not a real off-curve point: `d_ε` bounds the surface-normal
    error; a vertex within `d_ε` of the sphere along its normal projects (on the
    cut plane) to an in-plane radial deviation up to `(R/r_circle)·d_ε`
    (derivation: `|p−C| = √(h²+radial²)`, `d/d(radial)√(h²+radial²) ≈ r_c/R` at
    `radial=r_c`, so `dr ≈ (R/r_c)·d_sphere`). When the cut plane is far from the
    sphere centre, `r_c` is small and `R/r_c` is large. **Approach (A)
    projection-scaled radial band** (`src/lib.rs` only): the in-plane radial band
    becomes `(R/r_circle)·d_ε` while the axial (out-of-plane) band stays `d_ε`
    (the cut plane is exact). Surface-type-gated on a `Surface::Sphere` owner via
    `source_radius: Option<f64>` — every non-sphere path (`None`) is byte-identical;
    near-tangent guard (`r_circle > MIN_FEATURE_SIZE`) fails closed. **Two sites,
    both load-bearing:** (1) selection — `curve_contains_point` + caller
    `build_intersection_curves`; (2) Stage-4 relocation — `vert_circle` extended
    to carry the source sphere radius, the combined `circle_residual > d_eps`
    guard split into per-component axial/radial bands via `circle_residual_split`.
    Fixing only site 1 would convert `AmbiguousCurve` → `OffCurveBeyondChordBand`
    with **zero net `ok_correct` gain**, so the success criterion is `ok_correct`
    **rising**, not the `AmbiguousCurve` count alone. **NOT tolerance widening
    (P9/P10):** the band is the exact geometric propagation of the same `d_ε`,
    derived not picked; a point off by more than the propagated band still STOPs
    loudly. RED `tests/yr19_sphere_chord_band.rs` (a small-cap dimple, `r_c≈0.31`,
    `R/r_c≈3.2`, rim verts authored at `dr ∈ (d_ε, (R/r_c)·d_ε)` so the band is
    magnitude-load-bearing without the sidecar) reproduces the `AmbiguousCurve`
    today and goes GREEN under both fixes. Spec `specs/yr19_sphere_chord_band.md`;
    deviation **N11** (cross-refs N10). **Driver-verified empirical delta**
    (curved fuzz N=90, same seed, before→after; the worker could not run the
    sidecar fuzz — `curved_fuzz_sidecar_zombie_blocker`): **sphere `AmbiguousCurve`
    15 → 0 (eliminated)**; sphere `ok_correct` 15 → 30 (Union 4→14, Subtract
    11→16); total `ok_correct` 37 → **52**; total `AmbiguousCurve` 30 → 15 (now
    ALL cone); **`SILENT_WRONG` 0 → 0**. Critically, **no conversion to
    `Stage4RegionInvalid::OffCurveBeyondChordBand`** (sphere has zero Stage-4
    errors post-fix) — confirming the dual-site fix yields real `Ok`, not a
    downstream swap. **Deferred (still LOUD):** the cone analytic-conic share
    (`Parabola`/`Hyperbola`, oblique cone∩plane, the remaining 15) is unaffected
    and stays out of scope.
  - **PR-YR20 — Stage-6 tiered face-resolution tie-break (the largest non-cone
    `FaceResolutionFailed` bucket). ✅ DONE.** A driver investigation (env-gated
    prints, since reverted) found 12/12 sampled curved-fuzz `FaceResolutionFailed`
    cases share ONE uniform root cause — NOT a no-match. Stage-6 geometric face
    resolution (non-degenerate branch) attributes a kept triangle to the input
    face whose surface contains the triangle **centroid** within that face's
    per-face tolerance `tol_for` (`TAU_WORK` for a `Plane`, the Stage-1 chord band
    `d_ε` for `Cylinder`/`Sphere`/`Cone`): exactly 1 hit → attribute, 0 or ≥2 →
    `FaceResolutionFailed`. Every refusal is an `n_hits == 2` tie of one shape —
    a triangle lying **exactly on a planar cap near the rim** (`dist ≈ 5.5e-17`,
    `tol = TAU_WORK = 1e-12` → HIT) ALSO falls inside the curved lateral's
    necessarily-loose chord band (`dist ≈ 7.6e-3`, `tol ≈ 2.4e-2` → HIT) →
    spurious second hit → tie → F3. The rule wrongly treated an **exact**
    `TAU_WORK` planar hit and an **approximate** `d_ε` chord-band hit as equal
    weight; the triangle's true face is the cap. **The fix (tiered tie-break,
    `src/lib.rs` non-degenerate branch only):** rank hits by **tier** — EXACT
    (`dist < TAU_WORK`, the centroid lies ON the surface) dominates BAND
    (`TAU_WORK ≤ dist < tol_for`). Attribute to the unique hit at the minimum
    populated tier; ≥2 at that tier, or no hit, still `FaceResolutionFailed`.
    `tol_for` is untouched — each face keeps its own A14.3 single-source band; we
    only break ties by the exact-vs-band tier. **All-planar byte-identity (the
    critical non-regression):** for a `Plane` `tol_for == TAU_WORK`, so a hit
    (`dist < tol_for`) means `dist < TAU_WORK` ⇒ ALWAYS EXACT tier; the BAND tier
    is unreachable for planar faces. So for an all-planar input the BAND tier is
    empty, `n_exact` == the old hit count, and the new `match` reduces
    **byte-for-byte** to the old "exactly one face within `TAU_WORK`" rule — the
    box fuzz, the m3 coplanar-tie tests, and the yr5c planar-sliver tests are
    unaffected, and genuine coplanar / multi-solid ties (≥2 EXACT) still STOP
    loudly. **Tier-by-distance, NOT a `dist/tol` ratio:** a ratio would
    distinguish two sub-`TAU_WORK` planar hits and silently flip a current planar
    F3 to an attribution, breaking that safety property. **NOT tolerance widening
    (P9/P10):** `TAU_WORK` is the existing planar tolerance reused as the tier
    boundary, not a new looser constant. The degenerate-sliver branch is left
    unchanged (it never raises F3 for a tie; minimal regression surface). RED
    `tests/yr20_tiered_tiebreak.rs` (a closed-cylinder boolean with a near-rim
    cap triangle authored at the `n_hits == 2` cap-vs-lateral tie, tie magnitude
    asserted load-bearing without the sidecar) + an all-planar coplanar-tie safety
    canary that MUST still F3; adversary adds a 0-EXACT + 2-BAND two-cylinder
    curved tie that MUST still F3. Spec
    `specs/yr20_face_resolution_tiered_tiebreak.md`; deviation **N12** (refines
    N4). **Calibrated metric:** total `FaceResolutionFailed → ~0`, cylinder
    `ok_correct` rises (the cap-tie unblocks it), **ZERO new silent-wrong / no new
    `NonManifoldOutput`**. **Driver-verified empirical delta** (curved fuzz N=90,
    same seed, before→after; worker hit `curved_fuzz_sidecar_zombie_blocker`, did
    NOT fabricate): **total `FaceResolutionFailed` 16 → 0 (eliminated)**; cylinder
    `ok_correct` 22 → 31 (Subtract now 12/12, Union 19/20 — the 1 remaining is an
    unrelated `NonManifoldOutput`); total `ok_correct` 52 → **61**; **`SILENT_WRONG`
    0 → 0**. As calibrated, cone `FaceResolutionFailed` 7 → 0 but cone `ok_correct`
    stayed 0 — the refusal shifted to the deferred `AmbiguousCurve` conics (15 → 21)
    + `LocalRefinementRequired`, exactly the intended sibling-variant shift, not a
    real failure. **Deferred (still LOUD):** cone `ok_correct` stays 0 — a cone
    triangle that stops being an F3 tie simply refuses later for the deferred
    analytic-conic reason (`Parabola`/`Hyperbola`, oblique cone∩plane; see N7 /
    N10 / N11). That is correct, not a regression.
  - **Cone analytic-conic sequence (PR-YR21→YR24, PLANNED 2026-06-03).** Cone is
    `0/26` in the curved fuzz, blocked across ALL non-perpendicular sections. The
    analytic math is DONE in `ssi-rs` (`plane_cone` returns Circle/Ellipse/
    Parabola/Hyperbola); this is purely `yang-rs` integration. Missing pieces:
    `Curve::Parabola`/`Hyperbola` variants, their `ssi_curve_to_curve` +
    `curve_contains_point` arms, `eval_source` parametric eval, and — the
    keystone — **Stage-4 relocation for cone sections** (the existing ellipse
    relocation is hard-wired to the *cylinder* parameterization, YR11 §4.3.2, so a
    cone+plane edge hits `LocalRefinementRequired` at lib.rs ~3616; this breaks
    cone ELLIPSE too, not just parabola/hyperbola). **Design:** a
    **cone-section parameterization** relocation (`project_onto_cone_section`):
    for a mesh vertex take its angle θ around the cone axis, intersect that
    generator with the cutting plane → the exact conic point. Type-AGNOSTIC
    (ellipse/parabola/hyperbola identical), the cone analog of YR11's
    cylinder-ellipse projector, avoids generic foot-of-perpendicular quartics.
    **Sequence:**
    - **PR-YR21 — cone-section relocation foundation + cone ellipse. ✅ DONE
      (2026-06-04).** Shipped `project_onto_cone_section` (closed-form: relocate a
      vertex along its azimuth's generator `g = nappe·cosα·â + sinα·r̂`, solving
      `s` so `apex+s·g` lies on the cutting plane → on BOTH cone and plane = on the
      conic; type-agnostic, reused by YR22/YR23) + `ConeEllipseReloc` +
      `cone_chord_budget_from_owner` (per-cone-face budget `cone_chord_bound`,
      height from the cone owner's rim Circle — the single source). The Stage-4
      `Curve::Ellipse` arm now branches on incidence: cylinder+plane → the YR11
      path **byte-identical**; cone+plane → the new cone relocation loop; neither →
      the existing loud STOP. cone ELLIPSE now lands end-to-end (RED oracle1/2/3/4 +
      **real-sidecar E2E oracle8** green; held loud-STOPs — asymptotic/through-apex/
      parabola/hyperbola — stay LOUD per oracle6 + the adversary suite). Zero crate
      regressions; cyl/sphere `stage4_chord_band` untouched. **Loud STOPs (P9/P10):**
      `OnAxis` (ρ<MIN_FEATURE_SIZE), `LocalRefinementRequired` for `|n·g|≈0`
      (generator ∥ plane / asymptotic) and `s≤0` (wrong-nappe / through-apex).
      **Findings:** (1) the *spec's "secondary site"* Stage-4 cone budget gate
      `OffCurveBeyondChordBand` is **defensively redundant** — it is shadowed by the
      identical upstream `on_both` gate in `build_intersection_curves` (same
      `cone_chord_bound` tol), so a beyond-band vertex is demoted to `LineSegment`
      before Stage-4 (adversary-verified; kept as a fail-closed backstop, not
      load-bearing through the public surface). (2) oracle3's chord-deviation tight
      check inherited yr11's coarse 200k-sample `dist_to_ellipse_sampled` whose
      ~1.8e-5 half-spacing floor (perimeter-7.26 ellipse) cannot resolve TAU_MODEL
      — fixed with a resolution-independent two-level refined sampler; the rigorous
      on-ellipse guarantee remains enforced by oracle2 + the real sidecar. **Step-0
      cone-refusal split / curved-fuzz `ok_correct` delta deferred to the driver**
      per the `curved_fuzz_sidecar_zombie_blocker` (the bounded E2E oracle8 on the
      real `mesh_booleans` binary stands in as the live-boolean proof; no fabricated
      fuzz numbers). Gate: cone ELLIPSE `LocalRefinementRequired` → 0 (mock + real
      sidecar). **Driver-verified delta + Step-0 split** (curved fuzz N=90, same
      seed, before→after): cone `ok_correct` **0 → 5** (Union 2, Subtract 3 — cone's
      FIRST successes); cone `LocalRefinementRequired` **5 → 0** (eliminated); total
      `ok_correct` 61 → **66**; `SILENT_WRONG` 0 → 0. **Cone-refusal split** (of the
      26 cone cases): **5 ellipse** (now ✅), **21 parabola+hyperbola** (the
      `AmbiguousCurve`, YR22/YR23 targets), **0 axis-parallel/through-apex** in this
      sample. **Next: PR-YR22 (parabola), then YR23 (hyperbola) — target the 21.**
    - **PR-YR22 — Parabola end-to-end. ✅ DONE (2026-06-04).** `Curve::Parabola`
      (mirrors `SsiCurve`) + `ssi_curve_to_curve` + `curve_contains_point` +
      `parabola_point(t)` eval (`vertex + (t²/4f)·axis_dir + t·(normal×axis_dir)`)
      + point→t (conjugate-axis) inversion; Stage-4 relocation reuses YR21's
      `project_onto_cone_section`. **Recovered finish-from-RED across a session
      limit:** the worker's RED phase (6 oracles + a real-sidecar E2E oracle8 +
      the migrated yr21 oracle6) was preserved and committed; a GREEN subagent
      implemented production; it STOPPED at a verified RED-author fixture/oracle
      bug (oracle4's per-triangle winding check false-positived on the mock's
      ring-closure scaffold). Resolution (driver + second-opinion review +
      adversary): **reframe oracle4** to the invariant production actually enforces
      — boolean output is a consistently-oriented watertight 2-manifold (0 unpaired
      half-edges, χ=2, signed volume > 0) + the per-facet degenerate-area floor —
      since production deliberately does NOT do a per-facet winding test
      (`validate_relocated_triangles`, Yang §4.4.1/§4.4.3). Adversary added 9
      canaries (no silent-wrong, eval round-trip vs independent re-impl, no
      ellipse/cylinder/circle regression, hyperbola+axis-parallel stay loud, local
      fold breaks watertight). Full `cargo test -p yang-rs` green; fmt+clippy clean.
      Commits `3cf1f482` (RED)→`18909a5d` (GREEN)→`4fc114f2` (oracle4 reframe)→
      `955ef698` (adversary). **Fuzz delta = 0 BY CONSTRUCTION** (driver-verified
      N=90, unchanged from YR21): an exact parabola section needs the cut plane
      EXACTLY parallel to a generator (θ=α), which is **measure-zero** in the random
      fuzz — random box cuts give ellipses (YR21) or hyperbolas, never exact
      parabolas. So the parabola capability is real (proven by the θ=α oracles +
      E2E) but invisible to the random fuzz. **⇒ the 21 remaining cone
      `AmbiguousCurve` are (near-)all HYPERBOLA — PR-YR23 is the high-leverage one
      for the cone fuzz number.**
    - **PR-YR23 — Hyperbola end-to-end. ✅ DONE (2026-06-04).** `Curve::Hyperbola`
      (mirrors `SsiCurve`) + `ssi_curve_to_curve` + `curve_contains_point` +
      `hyperbola_point(t)` eval (`center + a·cosh(t)·major + b·sinh(t)·(normal×
      major)`) + point→t `asinh(v/b)` inversion (the bijective `sinh` coordinate);
      Stage-4 relocation reuses YR21's `project_onto_cone_section` /
      `cone_plane_residual` / `cone_chord_budget_from_owner` UNCHANGED (no new
      relocation method). **The new mechanism = two-branch selection:**
      `ssi_rs::intersect(Plane,Cone)` returns **2** `Hyperbola` for the HYPE case
      (one per nappe, opposite `major_axis`); the existing `matched==1` loop in
      `build_intersection_curves` needed NO structural change — `curve_contains_point`'s
      `(u/a)²−(v/b)²=1` membership **with the `u>0` discriminator** rejects the
      wrong-nappe branch (`u<0`), so exactly one matches and `matched==2/0` stays a
      LOUD `AmbiguousCurve`. Membership band = the geometric residual `|F|/|∇F|`
      (first-order perpendicular distance, the hyperbola analog of the
      ellipse/parabola arms), **NOT** a flat widening (P9/P10 held; adversary
      re-verified the band scales ~linearly with off-distance). Role-separated FIP
      cycle: spec → RED (8 oracles incl. an independent 2-candidate ssi oracle, a
      two-branch-selection oracle, the YR22 reframe oracle4 invariant, and a
      real-sidecar E2E) → GREEN (production-only; **STOPPED at oracle7 P9/P10**
      rather than widen a tolerance) → **driver oracle7 reframe** (the 2·cone_d_eps
      beyond-band fixture rejects honestly with `FaceResolutionFailed` — the
      reloc-band guard is geometrically UNREACHABLE for an on-plane ring since the
      YR18 on-both gate skips the edge first, and a narrower δ lands in a
      LineSegment-fallback dead zone that would SILENTLY succeed; "spec principle
      over literal", mirroring the YR22 oracle4 reframe) → Adversary (7 attacks,
      all PASS: wrong-nappe rejection witnessed by u-sign, band-not-flat,
      no YR21/YR22 regression, no silent-wrong across δ∈{0.5,1,1.2,2}·d_ε,
      independent oracle7-honesty recompute (centroid 1.33–1.60·d_ε off-cone),
      from-scratch eval round-trip). Commits `c2e088e6` (spec)→`0e22956d` (RED)→
      `c3dc4f13` (GREEN)→`713c9901` (oracle7 reframe)→`cca14e25` (adversary). Full
      `cargo test -p yang-rs` green (yr23 8/8 + yr23_adversary 7/7); fmt+clippy
      clean; kernel-v2 (consumer) still builds. **Fuzz delta: this DOES move the
      number** (unlike the measure-zero parabola) — a random box cut of a cone is
      (near-)always a hyperbola (`plane_cone` HYPE), so the ~21 remaining cone
      `AmbiguousCurve` are (near-)all hyperbola and cone `ok_correct` should rise
      from 5 toward ~26. **NOT fabricated here** (curved fuzz can't complete
      in-container per `curved_fuzz_sidecar_zombie_blocker`); capability proven by
      the unit oracles + the real-sidecar E2E (oracle8 ran green against the C++
      binary). The worker PREDICTED cone `ok_correct` 5→~26.
      **DRIVER-VERIFIED DELTA (CORRECTION — the prediction was wrong; cone is NOT
      closed):** curved fuzz N=90 (same seed) post-YR23: cone `AmbiguousCurve`
      **21 → 4** (hyperbola selection WORKS), but cone `ok_correct` only **5 → 6**
      and the bulk **shifted to `LocalRefinementRequired` 0 → 16** (Union 5,
      Subtract 11); overall `ok_correct` 66 → 67; `SILENT_WRONG` 0. The hyperbola
      SELECTION is correct, but the YR21 cone-section relocation **breaks down for
      hyperbola points reaching toward the ASYMPTOTIC generator** (where
      `|n·g|→0` ⇒ the `project_onto_cone_section` guard fires
      `LocalRefinementRequired`). The RED oracle + E2E sample near the vertex (where
      relocation works), so they passed; the random fuzz generates arcs extending
      toward the asymptote and exposes the gap. **This is the "moved-the-failure-
      to-a-sibling-variant" pattern** (cf. memory [[fix_all_gates_sharing_a_metric]]):
      the gate is cone `ok_correct` rising, not `AmbiguousCurve` dropping. **⇒ Cone
      is NOT closed. Closing it needs a Stage-4 hyperbola near-asymptote relocation
      cycle (a real geometric gap, larger than the PR-YR24 axis-parallel triage) —
      OR an explicit decision to leave near-asymptote hyperbola arcs as a sanctioned
      LOUD `LocalRefinementRequired` (out-of-scope), which is honest but caps cone
      coverage.** Shipped YR23 is sound (selection + near-vertex relocation, zero
      silent-wrong) and not a regression — it is progress that revealed a deeper
      gap.
    - **PR-YR24 — residual triage (likely small).** Remaining
      `LocalRefinementRequired` (axis-parallel / through-apex sections): confirm
      genuinely-degenerate ones correctly stay LOUD (out of scope, not a
      regression); close out cone. May fold into YR23.
    Each is a full RED→GREEN→Adversary cycle with the calibrated fuzz gate (cone
    `ok_correct` rises for the targeted section type; ZERO new silent-wrong;
    driver-verified delta). Cavity-sense / watertightness / on-surface oracle are
    surface-agnostic, so Subtract comes along per type.
  - **Next M5 increments (sequenced):** ~~curved `Subtract` cavity-sense~~
    (PR-YR13 ✅, box − cylinder blind pocket; ~~through-hole genus-1~~ PR-YR14 ✅;
    ~~box − sphere hemispherical dimple~~ PR-YR15 ✅; ~~box − cone conical pocket~~
    PR-YR17 ✅)
    + cut-surface handling (deferred in PR-YR8/YR5; through-cone / oblique cone cuts
    + internal spherical/conical voids still open)
    → side-face-exit / corner (triple-point) loud-STOP guard (oblique
    out-of-scope case) → broader SSI surface/pair coverage (cyl∩cyl) → the **general
    degree-4 curve** (a new parametric `SsiCurve` variant + the 5 general-position
    solvers) + torus pairs (rest of A15.4) → ~~curved `Surface` variants~~ (PR-YR6
    ✅) → ~~P2a curved cylinder tessellation~~ (PR-YR7 ✅) → ~~P2b sphere Stage-1
    tessellation~~ (PR-YR12 ✅) → ~~P3: Stage 3 wire
    `ssi-rs`~~ (PR-YR9 ✅). The general degree-4 cyl∩cyl curve requires a NEW
    parametric `SsiCurve` variant + general-position solvers, and **MUST be planned
    with a human before implementation.**
- **M6 — Native `cherchi-rs` Stage 2** behind the same interface, parity-green
  vs the sidecar on the corpus. ✅ **COMPLETE (2026-06-10, PR-CR-BL3c)** — all
  three BL3 slices landed; the sidecar is now a test-only parity oracle and
  yang-rs's default build runs the native backend (unconditional since M7c —
  the former `native-boolean` feature is removed; WASM restored at M7). **The biggest milestone — a faithful port of the
  MIT Cherchi C++ (`/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/`)
  to native Rust, removing the `cherchi-sidecar-rs` subprocess.** Reference parity
  vs the C++ sidecar is the LOAD-BEARING oracle (every PR diffs native vs sidecar
  on a corpus subset; CLAUDE.md hard-rule #2). MIT attribution header on every
  ported file. Used the `indirect-predicates-sidecar-rs` FFI for LPI/TPI
  until M7 (M6 landed native-but-not-yet-WASM; M7 clean-roomed the predicates →
  restored WASM at M7c). **Foundations already in `cherchi-rs`:** predicates (CR1–10),
  FastTrimesh+Tree (CR11–12c), Stage-1 pair detection (CR13), the CDT (NC1), the
  `MeshBoolean` trait + `LabeledArrangement` contract, and the IP FFI
  (`lambda3d_lpi/tpi`, `orient3d`). **Decomposition (PR-CR-AR* arrangement /
  PR-CR-BL* boolean-labeling; reference-parity-gated; demand-drive any missing IP
  predicate wrapper per CLAUDE.md #8):**
  - **PR-CR-AR1 — tri-tri intersection → implicit points. [DONE]** Ported
    `arrangements/code/intersection_classification.cpp`: for each CR13 candidate
    pair, classify (sign-pattern decoders cpp:834-925) and construct the typed
    intersection-vertex set per pair (`classify_pair` / `classify_all` in
    `crates/cherchi-rs/src/arrangements/intersection_points.rs`). **First FFI
    consumer inside `cherchi-rs`**, gated behind the off-by-default
    `indirect-predicates` feature (WASM still builds with the feature off; CI runs
    the crate both ways). **Scope (source-faithful, deviation N13):** builds
    **explicit + LPI only** — the source constructs *no* TPI here; TPI lives in
    `triangulation.cpp::createTPI`, deferred to **AR2**. AR1 ports the generic
    non-coplanar **transversal** crossing (`checkSingleNoCoplanarEdgeIntersection`
    → LPI via `ImplicitPoint3DLpi` + `lambda3d_lpi_*`; `checkVtxInTriangleIntersection`
    → explicit). Fully-coplanar and single-coplanar-edge pairs are emitted with a
    loud `Deferred(..)` marker (not dropped) for a later slice. Correctness oracle:
    each LPI vertex lies on BOTH supporting triangles' planes, asserted via exact
    indirect `orient3d == Zero` (not a float tolerance); plus CR9-agreement and
    hand-verified transversal cases. Full sidecar-corpus parity engages at AR3/BL3.
  - **PR-CR-AR2 — per-triangle constrained re-triangulation** (port
    `arrangements/code/triangulation.cpp`, ~1366 lines — **split into two
    slices**; NOT the spade NC1 CDT, which is f64-Delaunay and cannot handle
    exact/implicit points — port Cherchi's incremental insertion on implicit
    points via exact predicates + the CR12c `splitTri`/`splitEdge` API):
    - **PR-CR-AR2a — point/edge insertion. ✅ DONE.** Ported
      `triangulateSingleTriangle`'s point-collection (`aux_structure.rs`
      `group_intersection_points` → per-base-tri interior/edge buckets) +
      `splitSingleTriangle` (`retriangulate.rs` `split_single_triangle`): inserts
      AR1's intersection POINTS (interior → `split_tri`, on-edge → `split_edge`)
      into the per-triangle submesh with **exact** point-location via the FFI
      generic dispatch on `vert_coords`. Produces a valid covering
      sub-triangulation whose vertices include every intersection point (segments
      not yet enforced as edges). **Precursor CR-IP6b** added the implicit 2D
      predicates `orient2d_xy/yz/zx` + `point_in_triangle` to
      `indirect-predicates-sidecar-rs`; **Cycle 2** generalized `FastTrimesh`
      vertex storage to typed `VertexCoords { Explicit, Lpi }`. Oracle: exact
      covering triangulation (pure-dashu `RBig` signed-area-sum + same-sign
      winding, LPI coords from exact line-plane intersection — independent of the
      FFI split path), all intersection points are vertices, completeness/incidence
      via the exact FFI. Deviation **N14** (readable `splitSingleTriangle` with a
      uniform on-edge check; structural LPI dedup). **AR2b is next.**
    - **PR-CR-AR2b — constraint segments + TPI.** Decomposed A/B/C.
      - **Cycle A (done)** — FFI segment predicates (`inner_segments_cross` /
        `point_in_inner_segment` / `point_in_segment`).
      - **Cycle B (done)** — exact `point_in_segment_3d` (swaps the N13 raw-`f64`
        guard for the CR1 collinearity predicate) + `ConstraintSegment` grouping.
      - **Cycle C1 (done, PR-CR-AR2b Cycle C1)** — real `ImplicitPoint3DTpi`
        handle routing: `VertexCoords::Tpi` now flows through the
        per-base-triangle re-triangulation as an exact TPI handle (replacing the
        Cycle-B `sum/9` centroid placeholder), with macro-generated E/L/T
        predicate dispatch (`with_gp!`). Exact on-3-planes `orient3d==Zero`
        oracle. **The N13 TPI-handle deferral is RESOLVED at the routing layer.**
      - **Cycle C2 (remaining → AR3-coupled)** — `addConstraintSegment`
        enforcement + the segment-crossing `createTPI`. **STOP banked (P9/P10):**
        `createTPI`'s 2nd/3rd supporting-plane sourcing
        (`computeTriangleOfSegment` → global `seg2tris` + `jollyPoint` coplanar
        fallback) is AR3-level global state — the Cycle-B `source_tri` covers only
        an original transversal's witness, not mid-recursion sub-segments or the
        coplanar fallback. Deferred to Cycle C2 / AR3 rather than improvised.
  - **PR-CR-AR3 — constraint enforcement + global conforming soup** (absorbs the
    AR2b-deferred Cycle-C2 enforcement, which needs global cross-triangle state).
    **Parity-oracle correction (2026-06-08):** there is NO standalone C++
    arrangement binary (the 2020 arrangement code is embedded library-only — no
    main, no CMake target; only `mesh_booleans` (full boolean) is built). So AR3
    does NOT diff against a C++ arrangement. **AR3 oracle = structural + EXACT
    predicate invariants** (no self-intersections via exact `orient3d`; every
    detected intersecting pair realized as shared/constrained edges; consistent
    topology; Euler sanity) — strong for an exact arrangement. **Full C++
    reference parity engages at BL3** (the existing `mesh_booleans` binary
    transitively validates the arrangement: a wrong arrangement → wrong boolean →
    parity fail), honoring the parity-rule intent without speculative C++
    arrangement-dump infra. (Build such a sidecar later only if the structural
    oracle proves insufficient.) **Split:**
    - **PR-CR-AR3a — constraint-edge enforcement (DONE, 2026-06-08).** Ported
      `triangulation.cpp::addConstraintSegment` (cpp:597) + `createTPI` (cpp:1007)
      + helpers (`findIntersectingElements`, `boundaryWalker`, `earcutLinear`,
      `segmentsIntersectInside`, `pointInsideSegment`, `splitSegmentInSubSegments`)
      into `arrangements/enforce.rs`: realizes each AR2b `ConstraintSegment` as
      constraint-flagged mesh edge(s), constructing `createTPI` (real
      `ImplicitPoint3DTpi` from C1) at segment crossings. Public surface
      `SegmentSpec` / `EnforceError` / `enforce_constraint_segments` /
      `enforce_constraints`. The C++ global `seg2tris` is replaced by a
      per-work-item carried `source_tri` plus a `constraint_planes` side map keyed
      by sorted vertex-id pair (the minimal `TriangleSoup`). Oracle met
      (structural + EXACT, no C++ arrangement binary): constraints realized
      end-to-end; TPI exact on all 3 planes (`orient3d == Zero`); exact conforming
      sub-triangulation (pure-`dashu` covering); no spurious TPI (one crossing →
      one TPI, robust to endpoint/spec ordering — adversary-verified). The TPI
      handle/predicate dispatch (C1) was factored into a shared
      `arrangements/gp_dispatch.rs` (pure move) and reused. **Deferred to AR3b
      (the STOP walls, P9/P10):** `computeTriangleOfSegment`'s global `seg2tris`
      sourcing and the coplanar `jollyPoint` fallback — surfaced as the
      `EnforceError::SourcePlaneUnavailable` / `DegenerateTpi` errors (not hit by
      the in-scope original-transversal crossings; the multi-crossing case
      resolves its planes from the recorded sub-edge planes).
    - **PR-CR-AR3b — global conforming soup + topology (DONE, 2026-06-09).**
      `mesh_arrangement` (`arrangements/soup.rs`) wires the full
      detect→classify→group/canonicalize→split→enforce→assemble pipeline into a
      global non-self-intersecting soup: input scaling (`compute_multiplier`),
      global vertex dedup/weld (`merge_duplicated_vertices` +
      degenerate/duplicate-triangle removal), per-pair AR1 classification, global
      intersection-point grouping with N18 EXACT-coordinate canonicalization
      (coincident LPI/TPI points reached via different generator tuples weld to
      one identity across triangles), per-base-triangle fast-path-or-split+enforce,
      and a global weld of the emitted submeshes. Oracle met (structural + EXACT,
      RED 5-invariant + hand corpus; no C++ arrangement binary): conforming,
      jolly-tailed, label-aligned, no-degenerate, implicit-points-welded. An
      independently-authored ADVERSARY module pins input-ordering invariance
      (winding/order/label-swap), multi-crossing faces (conform or loud
      `DeepRecursionRequired`), the `SingleCoplanarEdge` loud-defer branch, N18
      anti-over-weld, and planar fast-path fidelity (resolved-position parity).
      **Still deferred (loud, P9/P10):** the AR3a `SourcePlaneUnavailable` /
      `DegenerateTpi` walls remain typed errors where unreached; coplanar
      overlap + single-coplanar-edge-through-interior are loud
      `CoplanarPairDeferred` (the §4.5.5 2D-Boolean pre-pass is **M8**). Feeds
      BL*. **(UPDATE 2026-06-20:** the single-coplanar-edge class is now
      CONSTRUCTED end-to-end — contained, edge-CROSSING, and the
      tvX-corner / through-interior sub-configs all classify with sidecar
      parity; only fully-coplanar `0 0 0` whole-triangle overlap still defers as
      `DeferReason::Coplanar`. See `single_coplanar_edge_parity.rs` and
      `soup.rs::adversary_coplanar_edge_through_interior_is_constructed`.**)
  - **PR-CR-BL1 — patch flood-fill (DONE, 2026-06-09).** Ported
    `computeAllPatches` / `computeSinglePatch` (booleans.cpp:396/426, serial
    variant) into the new feature-gated `labeling/patches.rs`:
    `compute_all_patches(&ArrangementSoup) -> Patches { patches, tri_to_patch,
    border_verts }` — ascending seed scan + stack flood across manifold edges
    (≤2 incident tris), stop at non-manifold intersection edges, border-vert
    marking for BL2 `findRayEndpoints`. Oracle (10 invariants): partition /
    label-constant / manifold-maximality / intersection-cuts / border-verts ==
    non-manifold endpoints / disjoint + enclosed + point-touch degeneracies /
    determinism / loud errors. Independently-authored ADVERSARY (12 tests):
    ordering/winding/concat invariance, 3-solid two-loop chain, through-cut
    (3 patches per solid), hand-built 3-incident edge, LabelMismatch path.
    Deviations: adjacency built from the soup (Rust FastTrimesh is
    per-base-triangle), serial-only (rule #5), sorted-Vec patches, returned
    border set, loud error for the C++ assert. **Scope note:** the `foctree`
    octree is NOT built here — it has no consumer until the BL2 ray-cast
    (demand-driven, CLAUDE.md #8 spirit); port it in BL2 alongside
    `findRayEndpoints`/`computeInsideOut`.
  - **PR-CR-BL2 — ray-cast in/out (2022 §5).** The robust per-patch in/out.
    - **Cycle A (DONE, 2026-06-10)** — `labeling/inside_out.rs`:
      `compute_inside_out(&soup, &patches) -> Vec<Label>` (per-patch inner
      labels). Full port of `findRayEndpoints` (explicit-origin branch) /
      `fast2DCheckIntersectionOnRay` / `checkIntersectionInsideTriangle3D` /
      `perturbX|Y|ZRay` + `perturbRayAndFindIntersTri` /
      `sortIntersectedTrisAlong*` (exact LPI sort keys via FFI
      `lessThanOnX/Y/Z`, btree-set equal-key-drop semantics) /
      `analyzeSortedIntersections`. Structural prerequisite: the soup now
      carries the prepped original `in_tris`/`in_labels` (C++ `arr_in_tris`).
      Oracle (5 invariants) + independent ADVERSARY (11 tests) — the
      adversary found 3 real bugs, all fixed: winner-less perturbation
      events now SKIP (N19, C++ `winner != -1` semantics; the fatal error
      was wrong on grazing input) and ray-parameter-ZERO hits are discarded
      (N20 — the C++ keeps them and silently mislabels point-touching
      solids; justified deviation, see docs/yang_deviations.md). Port
      finding: the C++ octree rayAABB query is semantically LOAD-BEARING
      (excludes behind-origin events); the brute-force port reproduces it
      with an explicit ray-AABB pre-filter.
    - **Cycle B (DONE, 2026-06-10)** — the C++ "generated ray" branch:
      synthetic origin at a patch triangle's approx centroid −0.1 along its
      dominant-normal axis (pure-f64 LPI/TPI approx eval; CR1/CR4 gates),
      EXACT validation (orient3d straddle + strict interior passage via
      gp_dispatch E/L/T), `seed_tri` recorded, and the sort's
      seed-plane-side discard (C++ `ray.tv` branch). Through-cut bands +
      hole discs now classify correctly (RED draft's expectation was
      itself corrected: a pierced solid's through-hole DISCS are inside
      the peg). ADVERSARY (9 more tests): X/Y-axis cuts, two pegs, peg
      through two stacked cubes, behind/forward seed-plane third solids,
      45° diamond peg, 0.01 sliver peg — no Cycle-B bugs.
    - **Cycle C (DONE, 2026-06-10)** — octree candidate producer:
      `labeling/octree.rs` `TriOctree` (build over `in_tris` AABBs;
      `query_aabb` = the booleans.cpp:580 `intersects_box` stack walk;
      deterministic params, sorted output). NOTE: the C++ pipeline uses
      `cinolib::Octree` — upstream `code/foctree.h` is NOT used by
      booleans.cpp and was not ported. Design invariant: the octree is a
      pure SUPERSET producer; the exact per-tri `in_ray_aabb` filter (the
      load-bearing behind-origin exclusion from Cycle A) applies to every
      candidate unconditionally, so octree parameters cannot affect
      labels. Oracles: superset vs brute (incl. degenerate zero-thickness
      ray AABBs), end-to-end label equivalence vs the permanent
      `#[cfg(test)]` brute path, determinism. Ring/edge searches stay
      full-scan (documented deviation; both complete). **BL2 COMPLETE.**
  - **PR-CR-AR3c — input-order-invariant constraint realization (DONE,
    2026-06-10).** Fixed at the diagnosed anchor: AR1/aux point identity
    was STRUCTURAL (generator-tuple) where the C++ interns by EXACT
    geometry (aux_structure.cpp:230) — one geometric point reached via two
    generator tuples counted twice, and `group_constraint_segments`
    silently dropped any pair resolving to ≠2 ids. New `PointInterner`
    keyed by exact rational coords (pure-dashu Lpi/Tpi evaluation) interns
    at SOURCE; >2 geometric endpoints is the loud
    `TransversalEndpointOvercount`; N18's post-hoc `canonicalize_points`
    folded into the interner (see amended N18 in yang_deviations.md).
    Oracles: anchor pair both orders, `segments_per_tri` keyed by triangle
    geometry invariant under reversal/concat-swap, end-to-end 16-fence +
    patch-count invariance; the BL2 RED witness un-ignored and green.
    Originally: **(OPENED 2026-06-10, blocked BL3 corpus parity).** The BL2-Cycle-B adversary
    found AR3b's constraint realization is input-order-DEPENDENT on
    CLOSED intersection loops: reversing global triangle order or
    swapping the two solids' concat order on a through-cut fixture
    leaves 4 intersection-loop fence segments unrealized as shared
    multiplicity-4 edges (two realized on only one side, two on
    neither), so the BL1 flood leaks and 6 patches collapse to 2. The
    AR3b conforming oracle (no interior AREA overlap) is structurally
    blind to a constraint segment missing from a perpendicular face's
    re-triangulation. RED witness: `#[ignore]`d
    `adversary_b_generated_ray_permutation_invariance`
    (labeling/inside_out_adversary_tests.rs) — un-ignore when fixed.
    Fix belongs in the `mesh_arrangement` orchestration (soup.rs):
    every intersection-curve segment must end as a shared edge of BOTH
    incident surfaces regardless of input presentation. New invariant
    for the AR3b oracle suite: per-pair constraint segments appear as
    multiplicity-4 edges, asserted under order/winding permutations.
  - **PR-CR-BL3 — emit `LabeledArrangement` + native `MeshBoolean` impl.**
    Assemble the per-tri source + patch_id + per-input in/out, implement
    `MeshBoolean` natively, **parity-green vs the sidecar on the corpus**, then
    switch `yang-rs` to the native backend behind the trait (the sidecar stays as
    the `#[cfg(test)]` parity oracle).
    - **PR-CR-BL3a ✅ DONE (2026-06-10).** `labeling/native.rs`:
      `native_labeled_arrangement` (concat → AR3b soup → BL1 patches → BL2
      in/out → frozen Stage-2 contract, exact-rational → descaled-f64 vertex
      emission) + `NativeBoolean` (`keep_set(op)` + per-op orientation flips
      at emission). Hand-fixture volumes exact.
    - **PR-CR-BL3b ✅ DONE (2026-06-10) — the M6 reference-parity gate.**
      `tests/parity_native_vs_sidecar.rs`: 12 generic-position fixtures × 4
      ops + concat-swap invariance (60 sidecar-compared cells), all GREEN
      under a triangulation-independent metric (exact vertex weld →
      watertight 2-manifold / Xor even-multiplicity, signed volume + area at
      1e-9 relative, Euler characteristic, vertex-set Hausdorff-0 at 1e-6).
      Native↔C++ agreement is ~1 ulp on volumes/areas across the corpus.
      Coplanar overlap stays a loud exclusion (N17 → M8); RED surfaced that
      an edge lying EXACTLY in the other solid's face plane triggers the
      same loud `SingleCoplanarEdge` deferral (N13 family) — promote a
      deliberate edge-in-plane fixture when that C++ path is ported.
      ~~Remaining BL3 work: switch `yang-rs` to the native backend behind the
      trait.~~ → BL3c below.
    - **PR-CR-BL3c ✅ DONE (2026-06-10) — yang-rs on the native backend; M6
      CLOSED.** yang-rs gained a default-ON `native-boolean` feature (enabled
      `cherchi-rs/indirect-predicates`; both features REMOVED at M7c — the
      backend is unconditional and WASM-clean now) and
      `yang_rs::native_backend() -> Option<NativeBoolean>` (None on the FFI
      stub build, P9 skip-loud). The whole yang-rs suite (334 tests) runs on
      the native backend unchanged — zero invariant re-anchors; the sidecar
      survives only as (a) the yang-level dual-backend parity suite
      (`tests/backend_parity.rs`), (b) the inputcheck Stage-1 oracle, and
      (c) the reference-mesh oracle inside the two #[ignore]d deep-fuzz
      harnesses. Coplanar M8 inventory: EMPTY — no yang-rs test ever passed
      coplanar input through the sidecar (yang's Stage-6 F2 gate already
      rejected it); on native the loud error simply fires earlier
      (`CoplanarPairDeferred` at the arrangement). Banked finding: with
      bit-exact mesh-level parity and identical yang-level vertex sets / χ /
      surfaces, curved-case output VOLUME is triangulation-dependent after
      Stage-4 relocation (~2e-4 on cylinder∪box) — yang-level parity uses
      the chord band d_ε × A_lateral for curved volume, 1e-9 relative for
      planar.
- **M7 — Clean-room indirect predicates from Attene's paper → restore WASM.**
  ✅ **COMPLETE (2026-06-10, PR-CR-M7a/M7b/M7c).** The LGPL FFI is demoted to a
  dev-only differential oracle; cherchi-rs / yang-rs / kernel-v2 all compile
  for wasm32-unknown-unknown with no feature flags.
  - **PR-CR-M7a DONE (2026-06-10): pattern-setter slice.** New
    `crates/predicate-gen` (dev tool, zero deps): SSA polynomial IR + FPG
    (Meyer-Pion 2008) forward error analysis → Attene App. A semi-static
    constants `δ(1)`/degree, emitting the checked-in
    `cherchi-rs/src/predicates/indirect/generated.rs` (byte-frozen by a
    freshness test; `cargo run -p predicate-gen` regenerates). cherchi-rs
    gains the UNGATED pure-Rust `predicates::indirect` module:
    `GenericPoint3D` (owned Explicit/Lpi/Tpi, §5.4 lambda caching) +
    `orient3d_indirect` over all 14 canonical instances (§6 reduction),
    cascading semi-static filter (§5.1) → interval arithmetic (§5.2,
    next_up/next_down outward rounding — no FPU modes, WASM-clean) →
    exact RBig (§5.3, `d == 0` → Undefined).
    `cargo check -p cherchi-rs --target wasm32-unknown-unknown` PASSES —
    first WASM proof-point. Oracles: filter soundness (95.6% generic
    implicit hit rate, gate ≥90%), ~357-case differential parity vs the
    black-box FFI sidecar (C++ orient3d is the MIRROR of Shewchuk's
    convention), δ cross-checks vs Cherchi's published εdL/εdT constants
    (ours 5.11e-15 / 8.89e-14 vs 4.885e-15 / 8.704e-14 — same degrees,
    few % more conservative). **Banked lesson:** the semi-static
    worst-case bound alone certifies only ~52% of generic TPI-heavy
    cases (degree up to 39) — the paper's interval tier is load-bearing,
    not an optimization; plan it into every remaining M7 predicate
    slice.
  - **PR-CR-M7b DONE (2026-06-10): full catalog slice.** predicate-gen
    grows a shared instance emitter (per-instance D′ parity slots: only
    ODD-multiplicity denominators flip; even ones feed undefinedness
    checks) + two families: `orient2d_{xy,yz,zx}` (27 instances, the
    exact Cherchi 2020 Appendix A set, canonical rank L < T < E pivoting
    on the FIRST argument; one-implicit LEE/TEE use the appendix's
    factored degree-5/8 form) and `less_than_on_{x,y,z}` (15 instances,
    Appendix B POINTCOMPARE; EE = direct f64 compare). All 13 published
    Appendix A/B filter constants matched: degrees EXACTLY, our δ 6-12%
    above (conservative band). cherchi-rs `predicates::indirect` adds
    the public catalog (`*_indirect` + `_filtered`/`_exact` tiers), the
    composites built purely on the primitives (`point_in_triangle`
    closed containment via first-non-degenerate projection;
    `inner_segments_cross` 4-orientation proper-crossing;
    `point_in_{inner_,}segment` collinearity gate + separating-axis
    betweenness — symmetric, deliberately fixing the FFI's documented EE
    order-sensitivity) and `approx_lpi` (interval-midpoint readback, the
    `lambda3d_lpi_interval` consumer's swap target). Oracles: hit rates
    orient2d 0.928/0.967/0.996, lessThan 1.000×3 (gate 0.90); composite
    parity vs independent pure-RBig formulations (~1000 coplanar
    configs); FFI differential parity incl. EE-quirk mappings; suites
    386 default / 549 gated; wasm32 check still green.
  - **PR-CR-M7c DONE (2026-06-10): consumer swap → M7 COMPLETE.** Every
    `arrangements/` + `labeling/` production call site swapped from the
    FFI to `predicates::indirect`: gp_dispatch's `Backing`/`Gp<'a>`/
    `with_gp!` 3^N handle machinery collapsed to a one-match
    `VertexCoords → GenericPoint3D` conversion (owned, lifetime-free,
    internal lambda caching preserves the per-vertex reuse pattern —
    inside_out's ray-sort arena is a `Vec<GenericPoint3D>`); enforce's
    fwd||rev EE-quirk workaround collapsed to ONE symmetric native
    `point_in_inner_segment_indirect` call; the two orient3d production
    sites (inside_out straddle / behind-seed-plane tests) are
    sign-RELATIVE, so the native↔FFI sign mirror needs no flip
    (annotated per-site); `lpi_approx` keeps its midpoint fallback on
    the native `approx_lpi`'s degenerate `None`. `init_fpu` removed from
    production (native predicates need no FPU mode). The
    `indirect-predicates` feature is REMOVED (modules always compiled);
    the FFI crate is a dev-dependency oracle only
    (`tests/indirect_*_parity.rs` keep `require_ffi_shim`, now in
    `tests/indirect_common`); yang-rs's `native-boolean` feature removed
    (`native_backend()` unconditional); test.sh's duplicate feature run
    dropped. Gates: cherchi-rs 557 (incl. the 60-cell
    native-vs-sidecar parity corpus — end-to-end proof the swap changed
    nothing), yang-rs 334 (incl. backend_parity), kernel-v2, rewrite
    tier, and `cargo check --target wasm32-unknown-unknown` green for
    cherchi-rs + yang-rs + kernel-v2.
- **M8 — Stage 0 coplanar preprocessing** hardened last (special case that
  complicates everything earlier). **Verified a genuine native need** (deviation
  N8, 2026-06-02): the patched sidecar emits multi-solid-labeled
  (`surface.len()==2`) triangles on coplanar overlap (test
  `c3_coplanar_face_yields_multi_attribution`), which surface in `yang-rs` as a
  loud `FaceResolutionFailed` (F2) — coplanarity is NOT delegated away, so M8 must
  implement the §4.5.5 2D-Boolean pre-pass (currently a correct loud-STOP
  deferral). **Also folded into M8: §4.5.4 illegal-self-intersection
  detection/removal** (deviation N6) — absent in the new crates; currently benign
  for analytic inputs (sidecar mesh validly trimmed + `check_watertight_2manifold`
  gate), to be added as a post-trim detector here.

  > **Reframe (2026-06-26): M8 is now the GENERAL §4.5.5 program, not a lattice
  > of special cases.** The slices below (a–e) built and shipped real machinery —
  > the exact 2D overlay engine (YR25), its wiring into `boolean()` (YR26),
  > Stage-6 hardening (YR27), the intra-solid plane-bit chained case (KV10), and
  > the flat-disc∩polygon containment (M8-disc). They are kept as the historical
  > record and the substrate to build on. But they are an **interim scaffold**:
  > each handles a specific face shape / normal config and walls the rest. Per the
  > §0.1 posture, the objective is now to implement §4.5.5 **once, generally**, and
  > retire the shape-specific gates. The general algorithm (paper §4.5.5, Fig. 16;
  > `refs/text/yang2025_hybrid_boolean.txt:716-760`):
  >
  > 1. **Detect every coplanar surface pair** between A and B (planar first; the
  >    paper's method extends to coplanar curved surfaces — Fig. 24b's 24 coplanar
  >    cylinders — via the same overlap-region machinery).
  > 2. **2D Boolean of the overlapping trims, BEFORE discretization.** Segment the
  >    coplanar region into three parts: A-only, B-only, and the overlap.
  > 3. **Replace the overlap with ONE shared trimmed surface**, meshed
  >    **identically for both models**; the overlap boundary becomes the
  >    intersection curve; all three parts share identical boundary samples. No
  >    same/opposite-normal branching — the result/keep rule is applied downstream
  >    from in/out classification, not by special-casing the normal at Stage 0.
  >
  > **Done generally, this subsumes** same-normal overlaps, holed/multi-loop faces,
  > a face in >1 pair (n-ary overlay), and curved coplanar pairs — all currently
  > walled. **Co-requisite: deviation N4** (§4.2.3 barycentric per-triangle
  > provenance). The current `tol_for` centroid-proximity attribution is the
  > structural reason coplanar overlaps are fragile (multi-attributed overlap
  > triangles can't be resolved by a single nearest plane); replacing it with the
  > arrangement's intrinsic provenance removes the tolerance-band machinery and the
  > "lift a gate → unmask a downstream tolerance bug" churn (the R0082 class). The
  > general Stage-0 and N4 should land together. **The 6-mode same-normal campaign
  > (`m8_samenormal_campaign.rs`) is the concrete first proving ground** for the
  > general path — its modes are the downstream fixes the general overlap exposes.
  - **M8 slice a ✅ (PR-YR25, 2026-06-10): the EXACT 2D overlay engine**
    (`yang_rs::coplanar_overlay`, standalone — NOT yet wired into
    `boolean()`). Two polygons-with-holes in one shared-plane frame → ONE
    conforming classified triangulation (AOnly/BOnly/Overlap) via exact
    rational (dashu `RBig`) edge arrangement (proper crossings, T-junctions,
    collinear partial overlaps; shared A/B edges dedup to one constraint) +
    exact vertical (trapezoidal) decomposition + exact ear-clip per cell;
    parity classification at cell centroids. Coverage post-conditions are
    exact and loud: `area(XOnly)+area(Overlap) == area(X)` in rationals,
    every input edge tiled gap-free by triangle edges, no zero-exact-area
    triangle; f64 rounding happens LAST and a rounding-collapsed sliver is a
    typed `RoundingCollapse` error, never silence. Derived queries: exact
    per-class areas + interface/region-boundary polylines (the future
    intersection curves). `cherchi_rs::cdt_polygon_with_holes` was evaluated
    and rejected (loops-only contract, no interior constraints, f64-only).
    10 oracle tests in `tests/yr25_coplanar_overlay.rs` (yang-rs 336→346).
  - **M8 slice b ✅ (PR-YR26, 2026-06-10): wired into `boolean()`** —
    coplanar-overlap booleans now produce correct MESH-level results through
    the native backend. The YR24 gate became the Stage-0 DETECTOR
    (`scan_near_coplanar`, ALL cross pairs); `stage0_preprocess`
    (`yang-rs/src/stage0.rs`) snaps both faces of each planar A×B pair onto
    ONE canonical plane (face A's — the §4.5.5 "trimmed common planar
    surface" where femto residuals are reconciled; cross-solid corner weld
    by exact in-plane coordinates), runs the PR-YR25 exact overlay, and
    re-tessellates Stage 1: face A gets AOnly+Overlap, face B BOnly+Overlap,
    Overlap triangles BIT-IDENTICAL in both meshes (wound per solid);
    boundary subdivision propagates into adjacent faces (Fig. 16 "identical
    sampling points"), re-triangulated by an exact strictly-positive
    apex-fan with an exact area-coverage certificate (a generic ear-clip can
    chord across the femto-crooked subdivided chain and re-create YR24-style
    sliver patches — found on the R0029 oblique corpus geometry). Downstream
    the duplicates weld into multi-label `{A,B}` triangles; cherchi-rs now
    restores removed duplicates into `in_tris`/`in_labels`
    (`DuplTriInfo` + the `addDuplicateTrisInfoInStructures` port,
    booleans.cpp:179-313/358-393/1530-1539) so each input stays a closed
    single-label shell for BL2. The C++ keep-rules keep the zero-volume
    overlap sheet for EVERY op (verified vs the sidecar); `boolean()`
    resolves it by the result-boundary rule (keep iff exactly one side of
    the plane is inside the result): opposite normals → keep only for
    Subtract; equal normals → keep only for Union/Intersect. Plane∩Plane
    intersection edges short-circuit to `Curve::LineSegment` (exact; SSI
    correctly refuses coincident planes — the §4.5.5 seam curve comes from
    the overlay). Oracles: stacked/near-stacked/partial/pocket fixtures with
    exact volumes + watertight + χ=2 + analytic sidecar-deviation pins
    (`tests/yr26_coplanar_boolean.rs`, 12), in/out restoration
    (`cherchi-rs/tests/coplanar_dupl_restore.rs`, 4), and **R0029 — the
    KV4-F1 corpus case — now unions successfully end to end** (yr24 test
    flipped to success). Unsupported residue keeps the typed wall:
    intra-solid near pairs (chained-output class), curved faces in a pair,
    faces in >1 pair, holed/non-continuous neighbor rings, overlay engine
    failures. yang-rs 346→358; cherchi-rs 559→563; kernel-v2 + rewrite tier
    + wasm32 checks green. §4.5.4 illegal-self-intersection detection (N6)
    remains the open M8 remainder, and B-Rep-level shared-face output
    topology (the slice-c question: today the kept sheet attributes to input
    A's face) is deferred to a future slice.
  - **M8 slice c ✅ (PR-YR27, 2026-06-10): Stage-6 face resolution hardened**
    — four probe-verified findings from the independent YR26 review.
    (1) *Finite-extent membership*: a multi-hit Stage-6 membership tier is
    narrowed by EXACT strict point-in-face containment
    (`point_strictly_in_planar_face`, rational 2D in the face's plane
    frame) before being declared a tie — kills the infinite-plane false
    positives (an L-profile cap CDT triangle whose centroid lies bit-exactly
    on a side plane; a chained input carrying same-plane sibling faces).
    Curved/undecidable faces are never excluded; unresolved ties stay the
    loud `FaceResolutionFailed` (now with precise Display text — the old
    "coplanar multi-solid label" wording was misleading). (2) *Keyed pair
    membership*: faces that went through a Stage-0 pair are measured against
    the CANONICAL pair plane (`PairPlane.face_a/face_b`), since the snap put
    their mesh there — fixes near-partial overlaps with residual in
    (TAU_WORK, band]; keyed per pair, no global tolerance change.
    (3) *Same-plane output-face merge* (`merge_same_plane_patches`):
    edge-adjacent patches on one plane (unit-normalized agreement within
    TAU_WORK, same orientation) emit as ONE output face (stacked union:
    10→6 faces), so chained booleans never see bit-identical-plane sibling
    faces; non-adjacent same-plane patches stay separate. (4) *Unmasked
    Stage-1 latent*: the fan path emitted a ZERO-AREA glue triangle for a
    convex outer loop with a COLLINEAR boundary run (re-fed outputs carry
    such subdivided edges); the next arrangement drops it → T-junction →
    non-watertight kept set. Collinear-run loops now route to the CDT
    (`planar_outer_loop_fan_unsafe`); the yr5c chained-subtract adversary
    passes its REAL branch (two 2-holed faces, vol 0.92). Assay: **ERROR
    1→0** (F0066 ERROR → honest typed UNSUPPORTED(coplanar-boolean) — its
    residue is the intra-solid chained wall), F0008 WRONG→CORRECT
    (SUPPORTED_CORRECT 5→6). Oracles: `tests/yr27_face_resolution.rs` (7).
    yang-rs 358→365; cherchi-rs 563; kernel-v2 55 (+ restored kv3
    coplanar-touching Intersect cell); rewrite tier + wasm32 + clippy/fmt
    green. Still open in M8: §4.5.4 illegal-self-intersection detection
    (N6) and the intra-solid near-coplanar chained-output class
    (`CoplanarFacesUnsupported`, F0066's residue).
  - **M8 slice d ✅ (PR-KV10, 2026-06-12): the intra-solid chained-output
    class RESOLVED for planar chains** — two stacked rounding-identity
    fixes, found by a probe-instrumented survey of ALL 54 coplanar-walled
    corpus cases (per-case residue distribution: 20 intra-solid /
    19 curved-face-in-pair / 8 arrangement-deferred / 4 build-mesh /
    3 overlay-RoundingCollapse).
    (1) *Sibling plane-bit canonicalization* (`to_yang_brep`): disjoint
    fragments of ONE plane (a box side split in two by a crossing union)
    emitted femto-distinct `(normal, d)` bits — per-fragment Newell
    normals + per-face first-vertex `d` derivation — and
    `scan_near_coplanar`'s benign intra exclusion is BIT-identity, so a
    fragment-carrying output could not enter ANY further boolean even
    when the incoming solid shared no plane (F0016-class, the dominant
    sub-class). Planar faces whose unit normals agree within `TAU_WORK`
    and offsets within `TAU_WORK·(1+|d|)` now adopt the first face's
    exact bits (greedy, deterministic; legitimate parallel planes are
    ≥ MIN_FEATURE_SIZE apart — six orders beyond the band).
    (2) *Near-aware I6 weld for ALL-PLANAR input pairs* (`boolean()`):
    behind the wall sat a second latent — chained oblique inputs make
    adjacent same-face tessellation triangles span femto-different EXACT
    planes, so the arrangement legitimately mints distinct copies of one
    junction point (~1e-16·scale apart, one per generating tri pair);
    left distinct they chain into sliver fans in the output B-Rep and
    break the NEXT boolean's patch boundaries (`NonManifoldOutput`
    dead-end / duplicate-CDT-vertex). The weld now clusters vertices
    within the scale-relative band (grid-bucketed, exact per-pair check —
    quantization only nominates, the KV8c lesson) and welds to the lowest
    member index. CURVED inputs keep the bit-exact weld: Stage-4 owns
    junction duplicate collapse there, and the near-weld collapsed
    cyl×cyl lens-tip seam edges (caught RED by kv9_cyl_cyl_special on the
    first attempt — the planar-only scope is load-bearing).
    Oracles: `kernel-v2/tests/kv10_plane_canonicalization.rs` (2: the
    bit-identity invariant + an oblique F0016-class chain with exact
    volume); m3 a4 adversary updated (the near-coincident gap it
    documented is now CLOSED for planar inputs — the perturbed duplicate
    trips the same NonManifoldInput guard as the bit-exact one).
    Corpus: SUPPORTED_CORRECT 42→46 (F0017/F0020/F0023/F0024),
    UNSUPPORTED(coplanar) 54→43, SUPPORTED_WRONG 0→0; ERROR 40→47 — six
    of the seven new ERRORs are ONE named pre-existing bug whose corpus
    footprint the wall had been masking: **KV4-F1 `NoExplicitRayOrigin`**
    (cherchi ray-cast: no explicit non-border patch vertex and no
    generated-ray triangle passes the exact straddle test — the point
    where the C++ reference exits "requires rationals"); the seventh is a
    gear-replay timeout (performance class). KV4-F1 is now the top
    M8-adjacent lever (~6 cases) and needs its own cycle (likely a
    rational-ray or implicit-origin extension of `find_ray_endpoints`).
    Still open in M8: §4.5.4 (N6), curved faces in a pair (19 cases,
    needs an arc-capable overlay), faces in >1 pair (n-ary overlay),
    arrangement-deferred exact-coplanar tri pairs (8 cases),
    build-mesh/overlay robustness (7 cases). Stage-0's survey probes are
    kept env-gated (`YANG_COPLANAR_PROBE=1`) for future residue sizing.
  - **M8 slice e ✅ (PR-M8-disc, 2026-06-15): the flat-disc∩convex-polygon
    containment class** — the dominant `face-unsupported` sub-class. A
    probe-instrumented survey (`YANG_COPLANAR_PROBE=1`, both-face curve
    histograms) showed EVERY `face-unsupported` coplanar-wall hit is the same
    shape: a flat circular DISC (a planar face bounded by a single closed
    `Curve::Circle` — a cylinder end-cap) coplanar with a planar POLYGON face
    (a 4-sided box face or a high-segment tessellated profile); no disc∩disc,
    no ellipse, no partial arc in that class. The disc is now built DIRECTLY
    (NOT through the sweep overlay, which re-subdivides the disc rim at every
    sweep-line crossing and would break conformality with the cylinder lateral
    that shares the rim): (1) `scan_near_coplanar`'s per-face AABB now expands
    by a `Circle` edge's analytic box `center ± r·√(1−n_k²)` — previously a
    disc face's AABB was just its seam vertex (a single point), so a coplanar
    disc∩polygon pair was detected only when the seam happened to overlap the
    other face (the `polygon ⊆ disc` orientation was missed entirely);
    (2) `build_disc_pair` (`stage0.rs`) extracts the disc's exact Stage-1 rim
    ring from Stage 1's OWN output (bit-identical → conformal with the
    cap/lateral mesh), tests strict containment (disc ⊆ poly or poly ⊆ disc,
    exact), and emits a shared rim/boundary triangulation as the overlap
    (rim fan for a disc inner, ear-clip for a polygon inner) plus an
    angular-merge annulus (the region between two nested convex rings,
    star-shaped about the inner centroid — no keyhole, no Steiner points, so
    every boundary vertex stays bit-shared) on the larger face, wound
    frame-CCW and distributed per the existing (op, normal-agreement) rule.
    Scope: pure containment with a CONVEX polygon. The loud residue keeps:
    disc∩disc (the newly-unmasked dominant remainder — coaxial cap pairs,
    86 probe hits), disc×polygon CROSSING (circle×segment is irrational on
    the sampled ring + needs boundary-split propagation, 13 hits),
    non-convex / holed polygons (gears). Corpus: **SUPPORTED_CORRECT 58→62**
    (F0026, F0030, F0062 + R0067 — the prior KV4-F1c render-tess ERROR, now
    chains through), SUPPORTED_WRONG 0→0, coplanar-walled 41→39; several
    ERROR↔coplanar lateral moves (R0079/R0092 ERROR→loud-typed coplanar wall,
    R0023/R0089 coplanar→a distinct downstream ERROR), zero CORRECT→worse
    regressions. Oracle: `crates/yang-rs/tests/m8_disc_coplanar.rs` (both
    containment orientations end-to-end watertight + volume; the crossing
    case pinned to stay `CoplanarFacesUnsupported`). Next M8 disc lever:
    **disc∩disc containment** (coaxial caps — the same direct construction
    with two rims instead of a rim + polygon), then disc×polygon crossing.
  - **KV4-F1 ✅ RESOLVED (PR-KV4-F1, 2026-06-12): the rational-ray
    fallback** — `rational_ray_inner_label` in cherchi-rs
    `labeling/inside_out.rs` implements the branch the C++ reference
    acknowledges and exits on ("a fully implicit patch that requires
    exact rationals for evaluation… does not support rationals",
    booleans.cpp:578 → deviation N21). Trigger class diagnosed on F0016:
    a sub-f64-resolution NEEDLE patch — an input edge piercing a triangle
    femto-close to its corner mints an LPI ~1e-17 from an existing vertex
    (the same f64-crooked-input root as PR-KV10, one layer down), so
    every explicit patch vertex is a border vertex and the approximated
    triangle defeats the f64 generated ray. The fallback classifies in
    pure RBig: exact centroid of a patch triangle as origin (strictly
    interior by positive exact area — the border restriction dissolves),
    exact axis-ray crossing/strictly-inside/sort over ALL `in_tris`, the
    nearest-hit rule reduced to `inner ⇔ n_k > 0` (pinned against the
    f64 `orient3d` convention by a dedicated oracle), exact grazes retry
    X→Y→Z then the loud typed `RationalRayDegenerate`. Set semantics
    mirror the f64 sort (equal-key collapse + the N20 `t ≤ 0` discard).
    Oracles: needle-fixture trio in `inside_out.rs` (sub-ulp LPI whose
    f64 approximation collapses onto the explicit vertex; inside → {B},
    outside → {}), convention pin, full cherchi suite + sidecar parity
    untouched (the fallback fires only where the reference terminates).
    Corpus: **SUPPORTED_CORRECT 46→52** (F0016/F0018/F0019/F0021/F0025 +
    R0086), ERROR 47→41, WRONG 0→0. Residue findings: **KV4-F1b** F0022
    classifies fine through the fallback but its third union fails yang
    reassembly (`NonManifoldOutput` — patch-boundary layer, distinct
    cycle); **KV4-F1c** R0067's boolean now SUCCEEDS (was the curved-
    patch NoExplicitRayOrigin) but the result fails kernel-v2 render
    tessellation — moved one layer, still ERROR.
  - **PR-KV11 ✅ (2026-06-13): the ellipse-arc junction class** — the
    largest coherent ERROR class ("output ellipse-arc endpoint does not
    lie on its ellipse", 8 cases: F0046–F0050, R0041, R0095, F0076 —
    cylinder × oblique box). FIVE stacked fixes, each one layer deeper:
    (1) *Stage-4 ellipse junction detection generalized to the
    cylinder+plane arm* (`insert_ellipse_or_junction`, shared with the
    PR-KV9 cyl×cyl arm): a vertex ending TWO different cylinder∩plane
    ellipses (the box-EDGE crossing) was silently overwritten in
    `vert_ellipse` and relocated onto only the last-scanned ellipse,
    staying off the first by the Stage-1 chord error (~1e-4 ≫ the 1e-9
    import band).
    (2) *§4.5.3 reversal test scoped to ONE curve* (paper
    `refs/text/yang2025_hybrid_boolean.txt:709-745`: p_b/p_r/p_n progress
    along the intersection curve C): at a junction where the loop
    TRANSITIONS between conics the discrete tangent legitimately kinks;
    the angle test false-positived and the sweep collapsed the junction
    loop vertex-by-vertex (the kv11 fixture's vanishing back-bulge —
    watertight but missing material).
    (3) *Ellipse × (plane∩plane line) TRIPLE points* (`vert_pp_planes`):
    a pp-segment (capB∩faceA trace) is exact, but its endpoint on the
    chordized lateral is the rim triple point — relocation onto the
    ellipse alone slid it off the cap plane (the "plane normal disagrees
    with Newell" rejects). Resolved into the ellipse-junction closed form
    `(plane ∩ plane) ∩ cylinder` with a synthetic second member; the
    junction gate carries the derived `1/|d̂·r̂|` along-line amplification
    (the KV9 gradient-band pattern — flat 2·d_ε under-gates).
    (4) *EllipseArc midpoint augmentation in from_yang pass-1d Newell*
    (the KV9 crescent fix existed in `validate::winding_points` but NOT
    in boolean.rs — the fix-all-gates-sharing-a-metric trap).
    (5) *Hybrid exact/quantized mesh oracles* (test-harness): the pure
    position-weld ALIASED junction-pinch thin-wedge tessellation (sliver
    fans hugging boundary arcs within the grid cell) into false
    non-manifold/Euler verdicts — the KV8c gear lesson on PARTIALLY
    exact-paired meshes. Exactly-paired (f32-bitwise) edges are provably
    closed and excluded; only the cross-face residue is quantized +
    T-subdivided; vertices weld by cell only where they bound residue
    edges. Self-intersection skip widened to vertex-adjacent pairs
    (chord sagitta at a shared junction vertex is tolerance-band
    geometry, ~1e-3 ≫ the weld threshold; real penetrations still fail
    via non-adjacent pairs). Plus a mixed-2D-orientation fold tripwire at
    patch emit.
    Oracles: `yang-rs/tests/kv11_ellipse_edge_junction.rs` (box ∪
    edge-piercing oblique cylinder: endpoints on BOTH ellipses, junctions
    ON the edge, back-bulge present; RED→GREEN through fixes 1–2).
    Corpus: **SUPPORTED_CORRECT 52→58** (F0046–F0050 + R0041),
    **ERROR 41→31**, WRONG 0→0; R0006/R0095/F0076 now stop at the KV7
    curved partial-patch re-entry wall (typed, honest). Remaining ERROR
    classes by size: 14 gear timeouts (perf), 5 geometric face
    resolution (R0011/R0073/R0082/R0090), 5 NonManifoldOutput reassembly
    (R0079/R0092/F0022/F0056 + F0076 residue), 2 Stage-4 DegenerateTriangle
    (R0009), 2 patch folds (F0042/F0045, KV9-F2), singles F0041/F0057/
    F0058/F0059/R0067.

- **KV12 — arc-segment profile extrude (gears).** A gear/arc profile already
  carries a complete sampled `vertex_ids` polygon (the app samples each arc into
  16 chord points); `arc_segments` are annotations indexing into it
  (`ArcSegment.start/end_vertex_index`), exactly like the `spline_segments` form
  PR-KV8 already extrudes.
  - **Tier 1 (chord-approx, the unblock) — ✅ DONE (2026-06-13).** The
    `arc_segments` wall in `make_faces_from_profiles` (`adapter.rs`) now mirrors
    the spline wall: an arc-segment profile is rejected ONLY when it has no
    `vertex_ids` polygon; with a polygon it routes through the existing polygon
    path (the chords are the samples the solver/viewport already use — no new
    approximation, documented/loud not silent). Bores = an inner loop of the
    profile (single extrude, no boolean). Sufficient for 3D printing. Tests:
    `kernel-v2/tests/kv8_gear_profile.rs` (with-polygon extrudes / no-polygon
    walled / E2E arc prism volume), GUI `app/tests/gui/arc-profile-extrude.spec.js`
    (closed D-shape line+arc → extrude → body). WASM rebuilt.
  - **Tier 2 (exact): SPEC WRITTEN — `specs/kv12_tier2_arc_extrude.md`** (Phase
    E, 2026-06-14). New `ProfileRegion::ArcPolygon` variant + an extrude
    assembler that emits arc-bearing planar caps + per-edge walls (planar for
    segments, **cylinder patch** for arcs — an arc swept along the normal is a
    cylinder lateral). Surface vocab (cylinder faces, planar-arc caps,
    `signed_volume` for arc faces) + the assembler templates (`extrude_circle`,
    `build_partial_revolve`) already exist. Bulk of the cost is **exact arc-loop
    simplicity validation** (arc–segment / arc–arc intersection predicates) — the
    only piece without scaffolding (P9: exact, not sampled). Increments E1–E4 +
    the B-Rep target, predicates, and acceptance are in the spec. Downstream:
    extruded-arc solids as boolean operands hit the KV7 curved partial-patch
    re-entry wall — a separate gap (extrude-only).
  - **Sequencing:** Tier 1 is the gearbox unblock (prototype-release Phase B);
    Tier 2 is a quality follow-up (Phase E). Scoped to extrude-only — booleans on
    the result are out of scope. **Comes before KV6 revolve** (independent).
    Driven by `docs/prototype_release_roadmap.md`.

- **KV13 — provenance / topological naming (face → creating feature).** 🔜 **PLANNED.**
  "Click any face → the feature that *introduced* it, through chained
  booleans/extrudes," + the inverse (feature → its faces), surviving rebuilds.
  The persistent-naming problem (`docs/PERSISTENT-NAMING.md`). Capstone of the
  prototype-release arc (Phase F); week-scale, multi-subagent.
  - **Substrate (exists):** yang-rs `TriangleAttribution` `(InputId, input_face_idx)`
    per output triangle; modeling-ops `Provenance` (`created`/`deleted`/`modified`
    with `Rewrite{before,after,reason}` + `role_assignments`); GeomRef `Role`/
    `Signature` selectors.
  - **Work:** (F1) `FaceOrigin { created_by, derived_from }` keyed on persistent
    identity, not churning `KernelId`. (F2) boolean origin attribution via
    `TriangleAttribution` → operand `feature_id`; new cut-faces → the boolean
    feature; inherited faces chain back (loud on `None`-attribution, P9).
    (F3) lineage propagation across rebuild (feature-engine, re-derived per
    rebuild like body-name inheritance). (F4) **persistent identity hardening**
    (the long pole — survive upstream-sketch edits; pragmatic role+signature
    scope, short of full Parasolid-grade naming, boundary documented).
    (F5) `get_face_data` emits resolved `created_by_feature`. (F6) UI: face →
    feature highlight + inverse. (F7) verification matrix incl. edit-and-reresolve
    + adversarial no-mislabel.
  - **Note:** a coarse **Tier 1** (click face → the body's *producing* feature
    via the GeomRef anchor already on the wire) is a separate cheap app-only win
    (prototype-release Phase D), exact for single-feature bodies; KV13 is the
    full through-boolean lineage. Strictly after the gearbox.
    Driven by `docs/prototype_release_roadmap.md`.

- **KV14 — adapter hole assembly (single-sketch holed extrude / ring gears).**
  ✅ **DONE (2026-06-13).** `make_faces_from_profiles` previously built every
  `ClosedProfile` as a hole-less `Profile` (passed empty holes, ignored
  `is_outer`), so a plate-with-bore / ring gear drawn as one sketch did not
  extrude as an annulus (kernel-v2 `extrude` always supported holes — the gap
  was adapter-side). Now a three-pass assembler: (1) classify each profile
  (circle / polygon, `is_outer`); (2) group each inner (`is_outer=false`) loop
  into the **strictly-larger** containing outer (centroid witness + area filter
  — rejects the app region-detector's redundant same-loop pairing, where a loop
  is emitted as both outer and hole); (3) stage one `Profile::new(outer, holes)`
  per outer, output aligned 1:1 with input profiles (the `profile_index`
  contract). A circle rim that carries holes is polygonized (64-gon) since a
  holed circle needs a polygon outer; plain circles stay exact. `Profile::new`
  remains the exact gate (containment/disjointness/nesting + CCW-normalize), so
  the f64 grouping can't produce a wrong solid. Tests: `kv8_gear_profile.rs`
  holed-square→annulus (volume == (outer−hole)×depth, output len == input len);
  GUI `holed-extrude.spec.js` (two nested rects → annulus, inner-wall face
  count). kernel-v2 (17 bins) + test-harness (29) + GUI (46) green; WASM
  rebuilt. Resolves prototype-release C2.

## 4b. Completion roadmap — Phases 1–6 (the full path to replacing legacy)

The M0–M8 list above is the *milestone* sequence for the boolean. This section is
the **completion** view: what it takes for `kernel-v2` to **replace** the legacy
kernel — handle planar + curved + coplanar + non-convex, implement the `Kernel`
trait, pass assay at parity-or-better, and run in WASM. It reconciles M5–M8 with
the **under-tracked** `kernel-v2` driver (Phase 4) + migration (Phase 6).

**"Complete Yang" ::=** kernel-v2 implements `Kernel`/`KernelIntrospect`; planar +
curved + coplanar + non-convex all handled; assay ≥ legacy on the supported
corpus; runs in WASM; `crates/kernel/` deleted — with **reference parity vs the
Cherchi C++ sidecar maintained throughout** (the non-negotiable correctness oracle).

```
Phase 1 ─► Phase 2 ─► Phase 3 ─► Phase 4 ─┐
                                          ├─► Phase 6 (migrate, assay, delete legacy)
Phase 5 (native arrangement + WASM) ──────┘   [parallel track, joins before 6]
```

- **Phase 1 — Finish the analytical SSI engine (`ssi-rs`).** *[in progress; ⊂ M5]*
  PR-SSI4 (parabola/hyperbola + through-apex), then the remaining A15.4 pairs
  (Degree-4: sphere∩cyl, cyl∩cyl, cone∩cone, sphere∩cone, cyl∩cone; torus pairs).
  **Exit:** all 15 quadric pairs analytically solved, adversary-hardened,
  on-surface-exact. **Risk:** low–moderate. **Size:** medium (~10–15 PRs).
  *Frontier (out of scope):* revolving an arbitrary profile → non-quadric surface
  of revolution → numerical/marching SSI (Patrikalakis Case F), a later capability.
- **Phase 2 — Curves enter the pipeline (Stages 1/3/4/6 curved).** *[⊂ M5; the
  heart, highest risk]* **First step done (PR-YR6 ✅):** curved `Surface`/`Curve`
  enum variants exist (mirroring `ssi-rs` field shapes) and the pipeline rejects
  them LOUDLY (`YangError::CurvedSurfaceNotYetSupported`) — no curved
  tessellation or `ssi-rs` call yet. **Stage-1 cylinder tessellation done
  (PR-YR7 ✅)**, **first end-to-end curved boolean done (PR-YR8 ✅):
  cylinder ∪ box flows through Stages 2/5/6, watertight + Euler=2, analytic
  `Surface::Cylinder` survives with exact params**, and **Stage 3 exact
  intersection edges done (PR-YR9 ✅): `ssi_rs::intersect` now refines the
  cylinder∪cap arrangement edges to the exact `Curve::Circle`/`Ellipse` (no
  longer mesh-approximate), with a P9 STOP on intersect/selection failure**, and
  **Stage 4 mesh updating done (PR-YR10 ✅): the mesh crossing points are
  RELOCATED onto the exact circle (Yang §4.4.1, not a global CDT) + §4.5.3
  reversed-point correction; watertightness inherited; cylinder ∪ box is now
  exact-edge AND on-curve, adversary-verified fold-free.**
  **Stage-1 sphere tessellation done (PR-YR12 ✅):** closed solid sphere → a
  watertight z-up lat/long mesh with a bijective `TessellationMap` (`d_ε =
  1e-2·2r√3`; cone still rejects loudly).
  Remaining: Stage 1 curved tessellation for the rest (cone; non-convex
  profile triangulation via Livesu earcut-CDT, for gears; Steiner points);
  ~~Stage 3 refine arrangement edges to the exact SSI curve (wire `ssi-rs` in —
  P3)~~ (PR-YR9 ✅); ~~Stage 4 conform mesh to refined curves~~ (PR-YR10 ✅,
  circle only — ellipse relocation + §4.5.2 real local refinement still loud
  STOPs);
  ~~Stage 6 curved cavity-sense for Subtract (deferred in PR-YR8)~~ (PR-YR13 ✅,
  `box − cylinder` blind pocket via `BRepFace.reversed`; ~~through-hole genus-1~~
  PR-YR14 ✅ via per-shell Euler gate χ=2−2g; ~~box − sphere hemispherical
  dimple~~ PR-YR15 ✅ via `Surface::Sphere` wiring + `sphere_chord_bound`;
  CONE cavities + internal spherical voids + box-as-subtrahend still open) +
  cut-surface
  faces (deferred in PR-YR5). **Exit:** cylinder ∪ box ✅ (exact edges + on-curve mesh),
  sphere − cylinder → correct curved B-Rep, sidecar mesh-parity + analytically
  exact edges. **Risk:** HIGH (paper-critical). **Size:** large.
- **Phase 3 — Coplanar preprocessing (Stage 0).** *[= M8]* detect coplanar face
  pairs pre-tessellation; 2D boolean → A-only/B-only/overlap; shared trimmed
  surface + identical meshes; overlap boundaries → intersection curves. **Exit:**
  flush/stacked faces + multi-plane cross-booleans work without conformal-edge
  explosions. **Risk:** moderate–high. **Size:** medium–large.
- **Phase 4 — The `kernel-v2` driver (Kernel trait).** *[NEW — the integration
  unlock; not in M0–M8]* **Phase 4a COMPLETE (2026-06-10):** PR-KV1 (arena +
  Euler operators) → PR-KV2 (Profile + lamina/extrude constructors) →
  PR-KV3 (boolean via yang-rs, exact tessellation, introspection) →
  **PR-KV4 (EXIT)**: `KernelV2Adapter` in test-harness implements the legacy
  `Kernel`/`KernelIntrospect` traits over kernel-v2 (polygon profiles,
  extrude, booleans, tessellate→RenderMesh, extract_edges, signatures;
  revolve / curved profiles / fillet-chamfer-shell are loud `NotSupported`),
  and `tests/assay_kv2.rs` replays the 190-case corpus categorized.
  **First honest corpus score (2026-06-10): 0 SUPPORTED_CORRECT / 5
  SUPPORTED_WRONG / 173 UNSUPPORTED (137 curved-profile, 24 revolve, 12
  coplanar-boolean) / 12 ERROR.** Every corpus case has ≥2 ops; the planar
  multi-op cases all hit either the M8 coplanar wall or real yang-rs boolean
  defects, so the always-on SUPPORTED_CORRECT gate is synthetic dispatch-path
  scenarios (single/oblique/L-profile extrudes pass ALL mesh oracles;
  subtract/intersect/cut/union pass with exact volumes but fail render-mesh
  conformity — finding KV4-F3). **Findings logged in PR-KV4:**
  KV4-F1 yang-rs `NoExplicitRayOrigin` in/out classification failure on
  oblique-box unions (R0029, F0016, F0018, F0019, F0021, F0025);
  KV4-F2 yang-rs "geometric face resolution failed (coplanar multi-solid
  label)" on coincident/oblique unions (F0001, F0017, F0022–F0024) and
  "input B-Rep is not 2-manifold" on a union-of-union round trip (F0020);
  KV4-F3 kernel-v2 tessellation drops exactly-collinear chain vertices
  per face → boolean render meshes are not position-paired watertight
  (B-Rep validates, volumes exact); KV4-F4 disjoint-operand unions return
  valid 2-shell solids that the legacy meta scores as wrong (F0011–F0015,
  χ=4 vs euler_target 2 — needs triage: correct output vs legacy
  expectation). The curved/coplanar walls confirm Phases 2–3 as the score
  unlock (137 + 12 cases). **Exit (met):** feature-engine builds +
  tessellates through kernel-v2; assay runs categorized
  (`cargo test -p test-harness --test assay_kv2 -- --ignored --nocapture`,
  report at `target/assay_kv2_report.json`).
  **PR-TH1 (2026-06-10) — oracle fixes + KV4-F4 triage:** the assay's mesh
  oracles were mis-scoring correct solids. Fixed in `test-harness/src/oracle.rs`
  (the NEW kernel's measurement instrument): (1) T-junction-aware
  watertight/χ pairing — edges are split at position-quantized vertices lying
  exactly on them before pairing, since kernel-v2's tessellation legitimately
  subdivides a shared boundary on one side only (KV4-F3's false-positive
  half); edges that do not close under subdivision still fail; (2) the
  penetration-depth guard now normalizes plane equations (it compared
  geometric thresholds against |n|-scaled distances, |n|≈2·area ~1e4, so
  f32-noise grazing contacts were flagged); a pair counts as penetrating only
  if each triangle crosses the other's plane by > the weld-tolerance depth;
  (3) KV4-F4 RESOLVED: disjoint-union outputs are correct 2-shell solids —
  χ now expects `euler_target + 2·(#shells−1)` with shells derived from the
  welded mesh. **Corpus score: 14 SUPPORTED_CORRECT / 1 SUPPORTED_WRONG /
  0 ERROR** (was 6/9/0). Movers: F0003, F0009, F0010 (T-junction FPs),
  F0011–F0015 (KV4-F4), F0012/13/15 grazing FPs. The one remaining WRONG is
  **R0029 — a REAL defect**: T-junction-aware pairing exposed a latent seam
  the raw pairing could not see — 4 coincident sheets along a split box edge,
  χ=3 (two spheres glued along an arc); the PR-YR24 near-coplanar gate does
  not fire for it. KV4-F3 NARROWED: union/pocket/through-hole smoke scenarios
  now pass the FULL oracle set; subtract/intersect still emit one degenerate
  sliver triangle (real tessellation defect, allowance kept for exactly
  `watertight_mesh` + `no_degenerate_triangles` on those two).
  **PR-KV5a (2026-06-11) — circle profiles → cylinder solids:** kernel-v2
  curved core (vertex-anchored closed `Curve::Circle` edges, `Surface::Cylinder`,
  V2/E3/F3 topology deliberately matching the yang-rs M5 fixture), curved-aware
  `validate_solid`, cap/lateral tessellation under the sagitta band, analytic
  volume/area.
  **PR-KV5b (2026-06-11) — cylinder booleans wired through yang-rs:** the
  curved boolean boundary is REAL. Survey-driven (yang's native cylinder×box
  outputs carry `Plane`+`Cylinder` surfaces and `LineSegment` + Circle-ARC
  edges — never full circles; original rims come back faceted at Stage-1
  resolution): `to_yang_brep` emits canonical cylinder solids as the shared-edge
  M5 fixture shape; `from_yang_brep` reassembles partial cylinder patches
  (`Curve::Arc` vocabulary, cavity sense on `Surface::Cylinder::reversed`,
  unrolled-winding orientation validation — the developable Newell analog);
  cylinder-patch render tessellation (unrolled cut + exact ear-clip +
  Delaunay-flip quality pass + Euclidean-LEPP chord-bound refinement); adapter
  maps legacy `CircleProfile`. End-to-end GREEN: cylinder∪box, blind pocket,
  through-hole (genus 1), intersect, canonical round-trip (bitwise analytic
  volume). Typed walls: Ellipse/Parabola/Hyperbola outputs named
  (`UnsupportedBooleanOutputCurve`), cyl×cyl surfaces yang's Stage-3 SSI text,
  partial-patch results cannot re-enter yang Stage 1 (`UnsupportedCurvedBoolean`).
  **Corpus: 17 SUPPORTED_CORRECT / 11 WRONG / 18 ERROR / 144 UNSUPPORTED
  (70 curved-profile, 38 revolve, 36 coplanar)** — first fully-correct curved
  cases R0006, R0083, F0044. **Findings:** KV5b-F1 yang emits one ULP-LENGTH
  edge per intersection circle (two verts 1 ulp apart; I6 weld is bit-exact
  only — Stage-4 relocation dedup needed upstream; collapses to zero-area
  slivers in f32 render meshes); KV5b-F2 euler oracle double-counts shells when
  the case meta's `euler_target` already encodes a multi-body expectation
  (F0031–F0040: correct 2-shell χ=4 scored against 4+2); KV5b-F3 yang walls hit
  by the corpus: Stage-3 `AmbiguousCurve` (cyl×cyl: F0041/43/45/58),
  `NoExplicitRayOrigin` (R0067/R0086), Stage-4 `LocalRefinementRequired`
  (F0056/57), non-2-manifold reassembly (R0092, F0052), an output edge used ≠2
  times (F0059), and one debug-tier `VertexOffSurface` import-band trip (F0042).
- **Phase 5 — Native arrangement + WASM.** *[= M6 + M7; parallel track]* M6 native
  `cherchi-rs` Stage-2 behind the `LabeledArrangement` seam, parity-green vs the
  sidecar (retires the C++ subprocess) — **✅ COMPLETE (M6: PR-CR-BL3c; M7:
  PR-CR-M7a/b/c, 2026-06-10)**: clean-room indirect predicates from Attene's
  paper replaced the LGPL FFI in every production path and the WASM build is
  restored (`cargo check --target wasm32-unknown-unknown` green for
  cherchi-rs / yang-rs / kernel-v2, no feature flags). Browser-side
  validation happens with the Phase-6 wasm-bridge migration.
  **Exit (met):** pure-Rust boolean compiling to WASM.
- **Phase 6 — Migration + assay.** **✅ COMPLETE (2026-06-11, user-directed
  early cutover).** The user overrode the parity-or-better exit gate ("the
  legacy kernel is useless and its scores are irrelevant" — its boolean output
  was geometrically wrong, e.g. F0038 rendered without cylinder walls). Landed
  as six commits: known-red legacy test purge → kernel contract
  (traits/types/units/MockKernel) moved to `waffle_types::kernel` →
  `KernelV2Adapter` moved into kernel-v2 as the production trait impl → all
  consumers swapped (wasm-bridge, file-format `export_step` over
  `dyn KernelBundle`, test-harness `ModelBuilder::kernel()` deleted) →
  **`crates/kernel/` deleted (64k lines)** → WASM bundle rebuilt on STABLE
  wasm-pack (3.0MB, was 4.9MB; catch_unwind/panic=unwind machinery deleted).
  `test.sh fast`/`full` fully green for the first time since the rewrite
  began; boolean GUI specs 60/60 on kernel-v2; F0038 renders correctly in
  the app. Capability-pending tests carry `#[ignore]`/`test.skip` milestone
  tags (M8 coplanar, M5 degree-4, CDT profile tail) — un-quarantine when
  each milestone lands.

- **KV6b — booleans over revolve solids. ✅ COMPLETE (2026-06-12, PR-KV6b-1/2).**
  yang Stage-1 ingests the revolve vocabulary: the NEW directional input-arc
  convention (Circle edge with start≠end = CCW sweep around curve.normal,
  unique in (0,2π) — π and major arcs unambiguous; outputs unchanged at
  SSI sub-arc granularity), per-edge sample CHAINS (shared, watertight),
  generalized planar CDT over spliced chains (annular sectors, holed circle
  caps), partial-cylinder quad strips, reversed input walls (+ Stage-6
  reversed XOR propagation). kernel-v2 to_yang_brep converts all revolve
  face classes; the operand wall narrowed to boolean-OUTPUT re-entry.
  End-to-end: revolve×box union/subtract incl. exactly-π and 350°
  operands, washer cuts with cavity-sense preservation, R0084 = first
  corpus revolve boolean SUPPORTED_CORRECT (31 total). FINDINGS for later
  milestones: **KV6b-F1** R0060 union output carries 2 non-manifold edges
  (yang triage); **KV6b-F2** most corpus 'rectangle' revolves are OBLIQUE
  to their axis (the assay generator's axis basis ≠ the engine's staging
  basis) → they sweep CONES and wall at KV6c — corpus revolve movement is
  gated by KV6c (12 cases) + KV6d torus (14 cases); **KV6b-F3** crossing
  booleans where a box face is PARALLEL to the revolve axis hit the
  plane×cylinder SSI LINE case (ssi-rs pair #2 'partial') — Stage-4
  relocates intersection points off-surface, caught loudly by the output
  Newell check / AmbiguousCurve.

- **KV6 on-axis slice 1 — lathe shaft (rectangle touching the axis). ✅
  SHIPPED (2026-07-07, task #65, spec
  `specs/kv6_on_axis_revolve_rectangle.md`).** `revolve` no longer
  conflates axis-CROSSING (invalid input) with axis-TOUCHING: a full-turn
  4-gon with exactly one on-axis edge — the most common lathe op — now
  builds the canonical KV5a solid cylinder by DELEGATING to the
  extrude-of-circle construction (no new topology code; analytic π·r²·h
  volume bitwise-adjacent). C0061/C0062/C0069 ERROR→SUPPORTED_CORRECT
  including their chained groove-cut booleans; corpus 183→186 CORRECT /
  0 WRONG. Remaining on-axis shapes (C0063/C0064 solid cones/frusta —
  apex or on-axis edge + oblique) stay on the typed boundary = KV6c
  slice 2's vocabulary.

- **KV6 on-axis slice 2 increment A — solid frustum (oblique off-axis
  edge). ✅ SHIPPED (2026-07-07, task #66, spec
  `specs/kv6_on_axis_revolve_oblique.md`).** The on-axis recovery
  classifier dispatches on the off-axis edge class: axis-parallel keeps
  the slice-1 extrude-of-circle delegation; OBLIQUE builds the SOLID
  FRUSTUM via a direct assembler mirroring `extrude_circle` (same Stroud
  §3.1.4 single-fake-edge census — 2 seam vertices, 2 rims + 1 seam
  ruling, 3 faces) with the analytic `Surface::Cone` from the slant
  (EdgeClass::Oblique formulas). Everything downstream is the existing
  KV6c vocabulary: `validate_cone_face`, exact `(πH/3)(r₀²+r₀r₁+r₁²)`
  flux volume, `tessellate_cone_lateral`, and the 5c boolean path —
  frustum − ⊥ slab chains end-to-end in the oracle suite. **C0064 (three
  coaxial stacked frusta, interpenetrating unions through cone×cone
  coaxial-circle SSI) flips ERROR → SUPPORTED_CORRECT on its exact-volume
  oracle**, pinned green in `named_case_categories`. Adversary findings:
  a mixed-sign oblique quad passing the perpendicular-cap gates always
  self-intersects the on-axis edge (rejected upstream as
  `ProfileNotSimple`, pinned); pencil quads (oblique CAP edge) and
  partial-angle on-axis profiles stay on the typed boundary. Remaining
  slice-2 shape: the on-axis APEX TRIANGLE (C0063 primary — solid cone,
  apex an interior singular point of the lateral) = increment B: 1-rim
  apex branches in `validate_cone_face` / `signed_volume` / the cone
  tessellator; apex-cone boolean operands stay typed
  (`UnsupportedCurvedBoolean` — the 1-half-edge lateral loop fails
  `to_yang`'s 4-edge pattern loudly).

- **KV6 on-axis slice 2 increment B — solid apex cone (C0063 primary). ✅
  SHIPPED (2026-07-07, task #66, same spec).** The on-axis APEX TRIANGLE
  (one perpendicular cap edge + one oblique edge reaching the axis) builds
  the SOLID CONE: 1 seam vertex, 1 edge (base rim), 2 faces — the apex is
  an INTERIOR SINGULAR POINT of the lateral (yang's own cone model), not a
  topological vertex; V−E+F = 2 holds and the vertex-fan orbit closes at
  arity 2. New 1-rim apex branches in `validate_cone_face` (outward sense
  = rim traversal axis toward the apex; `reversed` apex cavities rejected
  typed — no producer), `signed_volume` (same frustum flux with ρ_lo = 0 —
  exact π·r²·h/3), and `tessellate_cone_lateral` (base ring in the cap's
  bitwise frame + apex fan reusing the 2-rim strip's orientation
  transform; watertight against the disc cap). Apex-cone boolean OPERANDS
  stay typed (`UnsupportedCurvedBoolean` → adapter NotSupported — the
  1-half-edge lateral loop fails `to_yang`'s 4-edge pattern). **C0063
  moves ERROR → UNSUPPORTED(curved-profile)**: the primary cone builds;
  the case's real boundary is its OBLIQUE slab cut (conic-bounded cone
  patch — the genuine cone-patch vocabulary, still future work with
  KV6c 5c's oblique-cut note). Adversary: fan-winding and lateral-rim-
  sense mutations both caught; bicone triangles pinned typed;
  reversed-apex fixture pinned typed. Remaining KV6 revolve gaps after
  this slice: KV6d torus boolean-output recovery, C0070 non-alternating
  profiles, oblique cone cuts (conic-bounded patches), partial-angle
  on-axis profiles.

- **KV6c increment 5 — partial revolve of oblique edges: the arc-bounded
  cone patch. ✅ SHIPPED (2026-07-08, tasks #81/#82, spec
  `specs/kv6c_partial_revolve_cone_patch.md`, commits 710a783b + 92f21d15).**
  The single largest capability wall in the corpus: 34 of the 38
  UNSUPPORTED(revolve) cases were this one mechanism (2026-07-08 census).
  Increment 1 (kernel-v2): `build_partial_revolve` passes the
  `EdgeClass::Oblique` params through as the `Surface::Cone` wall (the
  [seg, arc, seg, arc] loop was already class-generic); new
  `validate_cone_patch` (the cylinder unrolled-winding analysis in the
  cone's (θ, τ) development, per-arc on-cone agreement `r = τ·tan α`);
  new `cone_arc_patch_flux` (per-arc Green's-theorem closed form — on a
  cone `x·n̂` is τ-independent, so each arc contributes
  `−(τ²/2)·(apex·(t̂s−t̂e) − tanα·(apex·â)·Δθ)`; reproduces the shipped
  full-band form at ±2π); render tessellation via ONE shared
  developable-patch engine (the cylinder patch unroll+CDT machinery
  parameterized by a `DevSurface` chart — radius-at-v and normal tilt are
  the only differences; `r_unroll` = max boundary radial distance).
  Increment 2 (yang): the partial cone STRIP arm in `tessellate_cone_face`
  (arc chains sample by sweep fraction of the shared n_seg —
  radius-independent, so a wall's two chains always pair), rims/arcs split
  explicitly (an open chain in the frustum-band arm would phantom-wrap
  silent-wrong geometry — now impossible); the kernel-v2 conversion gate
  removed. The FULL chain (cherchi, Stage-3/4 cone sections, output
  recovery into the new patch vocabulary) worked with no further changes —
  exact truncation volumes through the wall-crossing subtract.
  **Corpus: +9 SUPPORTED_CORRECT (R0002/10/18/33/37/55/68/69/80), 0 WRONG,
  zero lost.** The remaining 27 of the group's 36 cases moved to their
  next honest boundaries: 15 loud downstream ERRORs (several Stage-3
  `AmbiguousCurve{2,2}` cone-pair matching stops — M5-family — plus
  boolean/tessellation failures to census), 10 deeper typed UNSUPPORTED
  walls (curved re-entry, multi-shell, coplanar), 2 container TIMEOUTs
  (R0052/R0094). R0008's pin reconciled to ERROR (Stage-3 AmbiguousCurve).
  Adversary: flux-sign / reversed-sense / tolerance-widening mutations all
  caught (the third needed a new negative test, added); finding —
  near-cylindrical cones (apex ~1e9 away) lose ~8 digits to f64
  cancellation in the flux (~7 sig figs retained, documented, not a
  defect). Remaining KV6 revolve stock after this slice: KV6d closed
  torus (C0065/C0067), oblique cone sections (ellipse-arc vocabulary —
  the `#[ignore]`d boundary probe in `kv6c_partial_cone_boolean.rs`),
  boolean-output partial-patch re-entry (R0051 class), C0070
  non-alternating profiles.
  **Post-slice ERROR census (the 15 new loud stops), by mechanism:**
  4× Stage-4 `LocalRefinementRequired` (R0017/32/47/49); 2× Stage-3
  `IntersectFailed(AnalyticalSolutionNotAvailable)` (R0019/44 — missing
  SSI solver, M5); 2× Stage-3 `AmbiguousCurve` (R0008 {2,2}, R0003
  {1,0}); 3× render KV9-F2 fold tripwire on boolean-output cone patches
  (R0034/54/65 — DIAGNOSED via the new `KV2_PATCH_FOLD_PROBE`: the
  trimmed cone face's boundary is a conic-section CHORD polyline at
  yang's coarser d_ε (~1.1 off-surface at r≈519) interleaved with
  render-tolerance on-surface interior points at ~0.2 triangle height →
  inherent fold; the exact-arc retag in recover.rs only covers ⊥-plane
  circle sections, and these cuts are oblique box faces → conics. The
  honest fix is the cone conic-arc vocabulary (ellipse/hyperbola
  sections — see R0100's explicit `UnsupportedBooleanOutputCurve
  ("Hyperbola")` wall), not tolerance games); 1× CDT ring rejection
  (R0016, likely same family); 1× `VertexOffSurface` (R0099); 1×
  R0004 partial on-axis revolve (a KV6 gap: the on-axis recovery is
  full-turn-only) behind a second boolean failure.

- **Tangency pinch-vertex split + figure-eight wedge walk (the KV9-F1
  union follow-up). ✅ SHIPPED at yang unit level (2026-07-08, task #86,
  spec `specs/yang_tangency_pinch_split.md`); C0058 corpus residual
  DIAGNOSED and banked.** Two mechanisms landed: (1)
  `split_pinch_vertices` at the Stage-4 exit — a mesh vertex whose star is
  ≥ 2 edge-connected CLOSED fans (the weld of a tangency pinch) splits
  into one vertex per sheet (identical position bits, relocation tags
  duplicated), riding the §4.5.3 Phase-A recompute path. Red-phase
  discovery (Test Author): the SYMMETRIC weld read χ=0 and SILENTLY
  passed the even-χ shell gate as genus-1 — a pinched sphere masquerading
  as a torus — so this fixes a silent-wrong-topology class, not just the
  corpus path's loud χ=1 asymmetric weld. (2) The `patch_boundary_cycle`
  figure-eight WEDGE WALK: at a boundary vertex with out-degree > 1 (a
  patch pinched at a mesh-manifold vertex), the continuation is the
  wedge-consistent edge found by rotating the patch's triangle fan from
  the incoming edge's owning triangle — naive lowest-first could chain
  two lobes into one self-crossing cycle. Byte-identical at out-degree-1
  vertices. Direct 30°/C0058-geometry unions now complete end-to-end with
  χ=2, per-sheet coincident vertex pairs at the exact tangencies, and
  band-exact Steinmetz-family volumes (`tests/tangency_pinch_split.rs`).
  Adversary: split-disable and both guard mutations caught (two needed
  new in-src unit tests — open-fan no-split pin, bowtie two-cycle walk
  pin); noted guard-then-assume coupling on the closed-fan precondition
  (pinned). **C0058 corpus-path residual (still ERROR, honest; diagnosis
  REFINED by probes):** on the kernel-v2 tessellation route there is NO
  mesh vertex at the tangency at all (`YANG_S4_TWIN_PROBE` finds zero
  sub-1e-9·scale twins at Stage-4 exit) — the two removed lobes connect
  through a RESOLUTION-SCALE NECK the mesh cannot separate, so the kept
  face's boundary is genuinely ONE 64-edge figure-eight cycle whose
  Newell cancels; the s6-curved-degenerate-loop E2 guard rejects it (and
  kernel-v2's unrolled-winding rules would wall the same loop next: net
  wrap 0 with zero area, or ±2). The honest fix is Yang §4.3.3
  TANGENT-POINT INSERTION — detect the collinear-normal tangent point
  during intersection optimization and insert it as an explicit
  arrangement point, splitting the neck exactly (the shipped pinch split
  + wedge walk then finish the topology). That is a paper-faithfulness
  milestone needing its own planned cycle (P10 stop recorded here).
  Sibling class also named: the PERPENDICULAR equal-R union welds
  tangency along a shared mesh EDGE (2 four-valent edges, Ok + χ=0
  today) — the EDGE-pinch analog, separate follow-up.

- **KV6b-F3 — plane∥axis × cylinder line case. ✅ RESOLVED (2026-06-12,
  PR-F3 + PR-F3b).** ssi-rs pair #2's C3a/C3b line branches were already
  correct; the defects were ALL in yang Stage 4: (1) `LineSegment`
  intersection edges got NO relocation (chord points stayed off the exact
  line and off the cylinder → `VertexOffSurface`), and Stage 4 wasn't even
  entered when lines were the only intersection curves; (2) a TRIPLE point
  shared by a line edge and a circle edge relocated onto the circle alone,
  off the cutting plane (the Newell failure); (3) the Line membership band
  lacked the propagated factor `r/√(r²−d²)` — the radial Stage-1 chord
  contract measured in the cutting plane's in-plane metric (the line analog
  of PR-YR19's sphere `(R/r_c)` scaling) → `AmbiguousCurve{2,0}` on the
  app's through-box geometry. Fixes: line relocation arm (exact line
  recomputed from cylinder+plane incidence via ssi-rs, same selection rule
  as Stage 3), line∩circle-plane junction relocation, and
  `line_band_amplification` carried in every gate sharing the metric
  (Stage-3 matching, Stage-4 re-matching, both relocation gates) with
  surface-normal backstops unscaled. NOT revolve-specific — plain sideways
  cylinder×box overlaps were broken too. Oracles:
  `kernel-v2/tests/kv6f3_line_ssi.rs` (exact segment-slab volumes, on-line
  vertex pins, the KV6b probe, the exact app geometry). App: revolve(270°)
  + through-box auto-union produces ONE merged body. Corpus 30/2/19
  unchanged (no corpus case exercises the geometry). Walls that remain:
  two DIFFERENT lines through one vertex (box corner ruling piercing a
  cylinder), line+ellipse/cone-conic junctions, near-tangent planes — all
  loud STOPs.

- **KV8/KV9 — gear profiles, cyl×cyl special cases, EllipseArc vocabulary.
  ✅ COMPLETE (2026-06-12, PR-KV8/KV8b/KV8c/KV9/KV9b).** Corpus
  34 → 42-class SUPPORTED_CORRECT (audit-driven autonomous session).
  - **KV8 gear profiles** (was 73 walled cases): `make_faces_from_profiles`
    consumes the authored `vertex_ids` polygon of spline-annotated profiles
    (the gear's canonical sampled boundary from `generate_gear_profile`;
    the SplineSegment entries are fitted-control-point annotations) — no
    new sampling introduced; exact shoelace-volume oracles. Non-convex
    booleans flow through yang NC1 CDT. Arc-segment profiles stay walled
    (their samples shadow representable exact cylinder walls).
  - **KV8b**: planarity tripwire made SCALE-RELATIVE
    (`1e-12·(1+max|coord|)` — the absolute form mis-flagged legitimate f64
    mapping rounding at world scale ~70).
  - **KV8c**: assay oracles gain the EXACT-PAIRING fast path (bitwise edge
    closure is provably watertight; the quantization grid ALIASED distinct
    exact edges at gear density into false non-manifolds) and the
    resolution-relative degenerate-triangle rule (flat at the f32
    channel's resolution, height < 4 ulps of scale).
  - **KV9 parallel cyl×cyl** (F0041/42/43/45 class): ssi-rs's parallel
    branch (2 ruling lines) plumbed through Stage 3/4 — law-of-cosines
    gradient band `1/sin α`, combined owner chord budgets, Stage-4 line
    arm accepts (Cylinder, Cylinder); from_yang pass-1d Newell is
    arc-midpoint-augmented (crescent caps); KV7 recovery gains the
    minimum-loop-arity repair (lens caps).
  - **KV9b EllipseArc vocabulary** (was the 8-case named Ellipse wall):
    the oblique plane × cylinder section end-to-end — arena curve,
    classification (parametric minor side), twin rules, validation,
    twin-canonical sampling, patch unrolling (parameter = azimuth for
    cylinder sections), exact closed forms (elliptical segment
    `ab(Δt−sinΔt)`; sinusoid Green's flux verified against the circle
    special case). Oblique sections re-enter nothing (typed wall stays).
    Stage-3/4 cyl×cyl ELLIPSE plumbing (per-point gradient bands,
    tangent-direction discrimination, ellipse×ellipse junction relocation
    to (plane∩plane)∩cylinder with duplicate collapse).
  - **FINDINGS**: **KV9-F1** Steinmetz tangency-junction patch cycles
    disagree (equal-radius perpendicular pair quarantined; Stage-3/4 hold).
    **KV9-F2** (with KV7-F1, one class): `tessellate_cylinder_patch` folds
    on ring-holed partial laterals (F0042) and mid-arc loop rotations —
    now caught LOUDLY by the fold tripwire at patch emit (winding vs
    outward radial); the unrolled ear-clip/refinement needs a dedicated
    robustness cycle. ~15 gear cases ERROR on the 90s replay timeout
    (heavy exact arrangements — performance, not correctness).
  - Remaining top blockers by size (post-PR-KV11, corpus 58 CORRECT):
    KV6c/d revolve (~51), M8 coplanar (~44, of which 19 are the
    curved-face-in-pair sub-class), gear-replay timeouts (14, perf),
    geometric face resolution (5), NonManifoldOutput reassembly (5,
    incl. KV4-F1b F0022), patch-fold robustness (KV7-F1/KV9-F2, 2),
    KV4-F1c R0067 render tessellation. KV4-F1 RESOLVED (rational-ray
    fallback, deviation N21); ellipse-junction class RESOLVED (PR-KV11).

- **KV7 — boolean output curve recovery ("output curve tagging").
  ✅ COMPLETE (2026-06-12, PR-KV7).** Curved booleans are CHAINABLE: the
  former `UnsupportedCurvedBoolean` re-entry wall is gone. kernel-v2
  `recover.rs` rewrites yang outputs to B-Rep granularity before
  `from_yang_brep` pass 1, on the Yang-paper principle that output
  surfaces are exact (boundary = surface∩surface): chord runs between a
  cylinder face and a ⊥ plane retag onto the computed exact circle;
  valence-2 co-curve vertices fuse (T-vertex seg runs → single segs,
  circle chords → sub-π arcs / closed rims); 2-closed-rim cylinder faces
  with an azimuth-aligned anchor pair re-emit as the canonical
  `[rim, seam, rim, seam]` lateral (constructor vocabulary — `to_yang`
  unchanged). Supporting fixes: Stage-1 `all_line` inspects inner loops
  (seg-bounded faces with circle rings route to curved CDT);
  `stage4_chord_band = max(A,B)`; Stage-6 cylinder-face AXIAL tie-break
  (two faces of one infinite cylinder — drill stubs); general planar
  tessellation accepts full-circle edges; `tessellate_cylinder_lateral`
  rows sampled bitwise-in-the-cap-frame (watertight by construction).
  NEW typed wall `UnsupportedMultiShellBoolean` (voids / disjoint shells
  cannot re-enter yang reassembly — previously shadowed by the curved
  wall). Corpus 30→33 SUPPORTED_CORRECT, 2→0 SUPPORTED_WRONG (R0029 +
  R0060 — the KV6b-F1 non-manifold T-junction class — FIXED by the
  collinear fusion; F0066 flips CORRECT). Chains proven by
  `kernel-v2/tests/kv7_output_curve_tagging.rs` + `boolean_chains.rs`
  (boss→pocket, hole→hole, boss→concentric-hole tube, 3-deep, exact
  volumes). FINDINGS: **KV7-F1** `tessellate_cylinder_patch` folds its
  unrolled triangulation when a partial-lateral loop starts mid-arc, at
  specific chord densities (reproduced on R0084's oblique revolve at rel
  tol 1e-3; recovery canonicalizes loop rotation to start at a seg,
  which avoids it — the patch path bug itself is unfixed). **KV7-F2**
  re-entry of bodies with internal voids is the next chain wall (yang
  BRep input has no shell structure; reassembly cannot rebuild voids —
  the XOR-class limitation).
  **KV7-F2 RESOLVED 2026-07-10 (spec `kv2_multishell_boolean_operands` +
  amendment 1): the multi-shell operand wall is REMOVED.** Multi-shell
  operands of every flavor — disjoint lumps (disjoint auto-unions /
  multi-region extrudes), interlocking lumps (the
  `split_solid_into_bodies` under-split shape, R0035), and INTERNAL VOIDS
  (fully-enclosed subtracts, C0071) — re-enter `boolean_op`.
  `to_yang_brep_indexed` always emitted every shell into one
  multi-component BRep; only the guard blocked it. The pipeline is
  component- and cavity-agnostic: Cherchi 2022 §2.4/§5 in/out labeling is
  ray-cast parity against each whole input mesh (a cavity-interior point
  crosses two boundaries → OUTSIDE), the Stage-4 Euler gate is per
  connected shell, and `from_yang_brep` + `face_components` already
  assemble multi-component outputs into multi-shell solids. The PR-KV7
  "reassembly cannot rebuild voids" claim was measured STALE
  (`KV2_MULTISHELL_PROBE` bypass: C0071's genuine void operand runs
  SUPPORTED_CORRECT with exact volume);
  `KernelV2Error::UnsupportedMultiShellBoolean` is deleted. Tests:
  `kernel-v2/tests/multishell_boolean_operands.rs` (exact-volume
  union/subtract/intersect on 2-lump operands, lump-consumed subtract,
  voided-box suite incl. a tunnel that OPENS the cavity and an
  intersect straddling the cavity wall); the kv6b re-entry pin flipped
  to the positive `revolve_boolean_voided_output_reenters_boolean`.
  Assay: the entire `UNSUPPORTED(multi-shell)` category (6 cases)
  clears — C0071/C0072/C0073/C0074 → SUPPORTED_CORRECT, R0035 → typed
  Stage-4 `LocalRefinementRequired` ERROR, R0076 → typed
  `InvalidBooleanOutput` edge-pairing ERROR (both join their honest
  shared error classes).

- **KV15 — per-vertex planar near-weld for MIXED operands. ✅ SHIPPED
  2026-07-10 (spec `kv15_mixed_operand_planar_near_weld`).** The
  edge-not-2-directed `InvalidBooleanOutput` class (6 chained-extrude
  cases) measured to one choke point: upstream ops mint planar femto
  twins (≤ ~3e-14; Stage-0 overlay `lift_or_snap` for the stacked-Z
  subfamily, non-boolean machinery for the off-axis subfamily), and the
  PR-KV10 near-weld that reconciles exactly this class was gated on the
  WHOLE model being planar — one circle/gear profile anywhere in the
  chain dropped it to bit-exact, leaving a femto membrane (measured:
  odd 3-triangle edge uses) that makes Stage-6 patch walks disagree
  (chord vs twin-stopover chain; the §4B T-subdivision can't repair it —
  the twin projects AT the chord endpoint, t≈1). Fix: eligibility is now
  PER VERTEX — near-weld (same band `TAU_WORK·(1+scale)`, same grid,
  min-index survivor) unions only vertices whose every incident
  arrangement triangle positively proves planar descent via `la.source`
  + `tri_face`; curved-adjacent / empty-provenance / sentinel vertices
  keep bit-exact (kv9 junction-duplicate protection, sidecar producer
  unchanged; all-planar branch byte-identical). Tests:
  `edge_pairing_twin_weld_campaign` corpus trackers (F0070/F0081
  RED→GREEN) + `kv15_*` unit branch coverage, mutation-checked.
  **Assay 224 CORRECT / 0 WRONG / 52 ERROR / 17 UNSUPPORTED / 0 TIMEOUT**
  (was 221/0/55/17/0), zero-lost — F0070/F0076/F0081 ERROR→CORRECT, no
  other movement. **Residue census (named follow-ups):** (1) KV15b —
  R0076's twins arrive in the chained input at ~3.9e-8: genuinely
  distinct near-parallel crossings, sub-floor but 8 orders above the
  representability band; welding at the feature floor is the
  reverted-R0091 hazard, so the fix belongs at the MINTING boolean
  (A14.2 collapse at emission); tracker quarantined `#[ignore =
  "KV15b …"]`. (2) F0079 (+ sites in F0083/F0084) — the residual
  edge-pairing sites are NOT twins: three single-use edges over
  COLLINEAR vertices at real scale (chord on one face vs 2-edge chain
  on the neighbors) — the `yang_stage6_sliver_topology` §4B
  T-subdivision domain (its `had_fold_sliver` gate / exact-betweenness
  doesn't reach these sites).

- **KV15b — sub-resolution intersection-segment collapse at boolean
  emission. ✅ SHIPPED 2026-07-10 (spec
  `kv15b_mint_site_subresolution_collapse`).** The KV15 residue (1)
  measured to its mint site: R0076's gear-cut subtract is ALL-PLANAR, so
  `has_conic` is false and the Stage-4 §4.4.1(b) merge — the only
  existing collapse pass — never runs; the exact arrangement's two
  near-parallel crossings (gear flank grazing a box edge; measured
  `KV2_SUBFLOOR_TWIN_PROBE` pairs at 3.999e-8/6.472e-8, edge-connected)
  are emitted verbatim and poison the next union (Stage-6 patch walks
  disagree, kernel-v2 edge pairing rejects). Fix: one pass in
  `reconstruct_topology_stage4` before Phase-B emission, on EVERY path —
  collapse intersection segments (keys of `intersection_curves`, full
  provenance) whose resolved length is in (0, `TAU_MODEL`), min-index
  survivor keeping its own bits, single sweep (no chain drift), then
  compact + Phase-A recompute (the §4.5.3 machinery). `TAU_MODEL` (1e-7,
  the central A8.1/A14 vertex-merge resolution — NOT the 10×-coarser
  `MIN_FEATURE_SIZE` floor of the reverted-R0091 hazard) is forced by
  consistency: the Stage-0 clustering band floor downstream is exactly
  `TAU_MODEL`, so an emitted sub-`TAU_MODEL` pair is guaranteed to weld
  into a degenerate loop at the next coplanar op. Tests: tracker
  `r0076_no_edge_pairing_wall` un-quarantined RED→GREEN + 5 `kv15b_*`
  unit branch tests, mutation-checked (band widening and gate drop each
  kill their test). **PLAN-CORRECTION RECORD (mini-P10): the roadmap's
  "KV15b class" was FOUR cases; measurement splits it.** New probe
  `KV2_SUBFLOOR_TWIN_PROBE` (adapter-level sub-floor twin census after
  every op) shows R0007/R0071's sub-TAU pairs are PROFILE-CONGENITAL —
  emitted by the extrude/revolve constructors from micro-scale gear
  profiles (96/36 pairs at bit-identical spacing 7.790e-8/9.460e-8
  BEFORE any boolean; the models also carry hundreds of LEGITIMATE
  sub-`MIN_FEATURE_SIZE` profile features, so any absolute-floor weld
  stays dead) — and R0053's twins are sub-representable overlay mints
  inside its failing op (zero sub-floor pairs in any chained B-Rep).
  Neither is a boolean-emission miss; both need their own cycles
  (profile-ingestion hygiene vs overlay mint-site collapse). **Assay 229
  CORRECT / 0 WRONG / 49 ERROR / 15 UNSUPPORTED / 0 TIMEOUT** (was
  226/0/53/15/0), zero-lost — R0076 + F0078 + R0088 ERROR→CORRECT (the
  same emitted-twin mechanism sat in all three chains) and F0084
  ERROR→typed UNSUPPORTED(coplanar-boolean) (its chain now clears the
  twin wall and reaches the M8 boundary).

- **S3 ellipse-rim chord bound — the `AmbiguousCurve {0,0}` producer
  fault. ✅ SHIPPED 2026-07-10 (spec `yang_s3_ellipse_rim_chord_bound`).**
  2026-07-10 census of the 8-case Stage-3 `AmbiguousCurve` ERROR class
  (probe `YANG_S3_AMBIG_PROBE`, both the selector and the producer-fault
  arms): the `{candidates: 0, matched: 0}` trio F0082/F0083/F0085 (5
  fault sites, ALL in `chord_tol_for_curved_owner`'s cylinder arm) is a
  re-entering body whose single cylinder face carries ONLY ellipse rims
  (oblique plane∩cylinder trims from a prior boolean, KV14 vocabulary) —
  zero `Curve::Circle` edges, so the Circle-rim-AABB bound lookup
  returned `None` and blamed the producer. Fix: the Stage-1 ellipse
  chain bound (`d_ε = 1e-2·major_radius`) factored into ONE source
  (`ellipse_chord_bound`, consumed by the KV14 pre-pass AND a new
  Stage-3 fallback `ellipse_rim_chord_bound` = max over the owner's
  ellipse edges); owners with neither rim keep the loud fault. Trackers
  `s3_ellipse_rim_chord_bound.rs` (3 cases) RED→GREEN + 2 unit tests,
  mutation-checked (minor-radius swap kills the max test).
  **Amendment 1 (same session): Stage-4's `input_curved_chord_bound`
  had the IDENTICAL Circle-only gap** — with Stage-3 fixed the trio hit
  the relocation ENTRY producer fault (`vertex u32::MAX` LRR; probe
  `YANG_S4_MERGE_PROBE` proved the §4.4.1(b) merge budget guard never
  fires). Fix = the same fallback composed fallback-only
  (`curved_chord_bound(..).or_else(ellipse_rim_chord_bound)`),
  byte-identical whenever a Circle rim or sphere exists; tracker
  extension `*_no_stage4_band_fault` RED→GREEN. Net: the ellipse-rim
  booleans now COMPLETE — the trio's remaining failures are their known
  downstream classes (F0083 Extrude-11 edge-not-2-directed §4B site,
  F0082 reassembled-non-2-manifold, F0085 late-chain CDT re-entry),
  failing-op counts 3→2 / 3→2 / 3→1. **Census of the remaining
  AmbiguousCurve sub-classes (named follow-ups):** (1) C0043/C0056 —
  INTERNALLY TANGENT cylinder pairs (0.6+0.4 = 1.0, 0.5+0.5 = 1.0):
  exact surfaces touch along one generator, the chordal meshes cross in
  a wide band → phantom intersection edges far off the tangent line;
  this is the paper's §4.3.3 Case-IV/tangent-point machinery (C0058's
  wall — the tangency campaign's standing follow-up). (2) R0026 —
  near-tangent plane∥axis × cylinder secant: both parallel-generator
  candidates exist but BOTH endpoints sit outside the amplified band
  (sub-band chord sag; the R0072 position tie-break needs matched ≥ 1
  and never engages). (3) R0003/R0008 — cone∩plane conic selection at
  extreme cones (R0008 half-angle 88.95° blows `cone_chord_bound` to
  3e1; R0003 one endpoint beyond the band) — the M5-corrected
  "cone∩plane conic + Stage-4 N2" family. **Assay 229 CORRECT / 0
  WRONG / 49 ERROR / 15 UNSUPPORTED / 0 TIMEOUT — unchanged totals,
  zero-lost (with amendment 1 the trio moved WITHIN ERROR from the
  Stage-3 fault through the Stage-4 entry fault to their late-chain
  downstream walls). The LRR class stands at 14 genuine
  relocation-region cases — the N2 mesh-updating epic remains the top
  target.**

- **S7 — the certainly-fatal chord split (§4B follow-up). ✅ SHIPPED
  2026-07-10 (spec `yang_stage6_sliver_topology` amendment 1 + 1a).**
  The F0079-class edge-not-2-directed residue (KV15 census follow-up 2)
  measured at its site (`[s6-split-probe]`): a loop walks a spur+chord
  over a vertex that is f64-collinear (perpendicular distance 0.0) but
  sub-ULP OFF the exact segment, on a patch with NO fold sliver — both
  the §4B eligibility gate and the exact collinearity test miss it, and
  the chord's undirected use-count of 1 makes kernel-v2's rejection
  CERTAIN. Fix: a second split arm in
  `subdivide_loops_at_shared_vertices` on EVERY patch, gated on
  certain-fatality (use(a,b)==1 AND both complementary sub-segments
  walked AND v within TAU_WORK of the open segment, 0<t<1) — it can
  never alter a passing output (valid outputs use every segment exactly
  twice), so benign-T-junction reference parity is preserved
  structurally. Amendment 1a: the split alone left χ odd (the spur
  became a zero-width slit); S7 finishes by cancelling adjacent inverse
  pairs with a split-inserted member (null-excursion removal, fixpoint).
  Tracker `f0079_no_edge_pairing_wall` RED→GREEN; 4 `s7_*` unit tests,
  mutation-checked (gate drop caught after the benign fixture was
  strengthened to isolate the use==1 gate). **Assay 231 CORRECT / 0
  WRONG / 47 ERROR / 15 UNSUPPORTED / 0 TIMEOUT** (was 229/0/49/15/0),
  zero-lost: **F0079 AND F0083 ERROR→SUPPORTED_CORRECT** (F0083's
  residual §4B sites were this same class — combined with the same-day
  ellipse-rim chord-bound fixes its 12-op chain now completes
  end-to-end). C0075 measured NOT this class (six unpaired vertical
  seam segments, no on-segment vertex) — stays in the
  edge-not-2-directed residue with F0084's sites.

- **KV6a — revolve (kernel-v2). ✅ COMPLETE (2026-06-11, PR-KV6a).**
  Partial angles (0,2π) AND full 360° for polygon profiles with
  axis-parallel/perpendicular edges, axis in-plane, profile strictly one
  side. Partial output = the KV5b partial-patch vocabulary (sweep arcs,
  reversed inner-bore cylinder patches, arc-bounded annular sectors); 360°
  = the genus-1 washer (annular ring caps + canonical cylinders, incl. the
  first REVERSED canonical lateral — validate/tessellate/signed_volume
  vocabulary extended with mirrored rim rules, arc-loop closed forms
  (Green's theorem over the unrolled boundary; Pappus at 1e-12, washer
  bitwise 9π), and annular-cap strips). Adapter maps degrees→radians;
  axis-through-profile is INVALID INPUT (KernelError::Other — the
  F0073/F0074 expected-rebuild-error path), capability walls stay
  NotSupported. Corpus 28→30 SUPPORTED_CORRECT, UNSUPPORTED(revolve)
  38→26 (rect-revolves now stop at the KV6b boolean wall, label shifts to
  curved-profile). GUI: all 4 revolve spec files + the revolve
  input-validation block un-quarantined and GREEN (46 tests) after fixing
  their March-2026 staleness (the UX overhaul made axis-picking mandatory;
  specs now set the axis via the `__waffle.setRevolveAxis` test API with
  the engine's plane basis). Walls for later: KV6b (yang Stage-1 ingestion
  of partial patches + holed caps — unwalls boss/cut revolve corpus bulk),
  KV6c cones (oblique edges), KV6d torus (circle profiles), on-axis
  profiles (solid-of-revolution without bore — common CAD flow, currently
  rejected as touching).

**Where the risk lives:** almost all of it is Phase 2 (curved Stage 3/4). Phase 1
is a steady low-risk grind; Phases 4/6 are large but mechanical once geometry
works; Phase 5 is a contained predicates problem with a ready oracle. **Scale:**
multi-month, not a few sessions.

## 5. Risks & decisions

- **Coplanar multi-attribution** (S1): the `LabeledArrangement` source must be a
  list, and in/out a per-input vector. Locked in §2.
- **Stage-1 cleanliness is the true gate** (S2): M1 before M3. A label producer
  cannot mask inputs that violate Cherchi's axioms.
- **Substitutes retained, not deleted** (S3): M4.
- **`cherchi-rs` layering amended** (S5): its `CLAUDE.md` Hard-rule #7 (dashu
  only) was amended to permit a *temporary, feature-gated, non-WASM* dependency
  on `indirect-predicates-sidecar-rs`. **Amendment retired at M7c** — the
  clean-room predicates restored the pure-Rust/WASM end state; the FFI is a
  dev-dependency oracle only.
- **Dockerfile stays thin** (build caution): do **not** add a `RUN make` layer —
  it costs ~22 min and ~8 GB per image rebuild. Install only build prerequisites
  (cmake/clang are already present) and run `scripts/build_sidecars.sh` at
  container-create / first-test, with `CHERCHI2022_BIN` / `INDIRECT_PREDICATES_SRC`
  env defaults.

## 6. PR granularity for the arrangement

The micro RED→GREEN PR style (15–50 LOC) suited isolated predicates but cannot
meaningfully slice a graph algorithm. For Stage 2, use **vertical,
behavior-tested slices** where **GREEN ::= "matches the sidecar oracle on corpus
subset N"**, not "compiles + unit test". The oracle is what makes large slices
safe; this is why M0 (operationalize parity) comes first.

## 7. Doc-edit ledger

This re-charting touched:

| File | Change |
|---|---|
| `docs/yang_functional_roadmap.md` | this file (new SSOT) |
| `CLAUDE.md` | rewrote stale "Current architecture" block; re-sequenced phase tracker & priorities; PLAN.md note |
| `crates/cherchi-rs/CLAUDE.md` | Hard-rule #7 amended (interim IP-FFI dep); Stage-2 = `LabeledArrangement` producer |
| `crates/cherchi-sidecar-rs/CLAUDE.md` | elevated to interim `LabeledArrangement` producer + label-emission mission |
| `crates/yang-rs/CLAUDE.md` | `LabeledArrangement` consumption; retain substitutes as test oracle |
| `crates/indirect-predicates-sidecar-rs/CLAUDE.md` | predicates demand-driven; clean-room/WASM end-state |
| `docs/yang_deviations.md` | appended interim labels-from-sidecar deviation |
| `Dockerfile` + `scripts/build_sidecars.sh` | operationalize parity (thin image + build script) |
| `docs/prototype_release_roadmap.md` | **NEW** — epic doc for the planetary-gearbox / prototype-release arc; sequences kernel (KV12/KV13) + app/UX + tooling phases toward the 3D-print acceptance test |
| §4 KV12 / KV13 (this file) | added planned stubs — arc-segment profile extrude (gears) and provenance/topological naming — driven by the prototype-release roadmap |
| `projects/{04-3d-viewport,08-ui-chrome,13-dev-infrastructure}/PLAN.md` | pointer line → active forward work tracked in `prototype_release_roadmap.md` (those PLANs are accurate as-built snapshots, all milestones closed) |

## 8. Deviations from Yang 2025

The interim path takes Stage-2 labels from the C++ sidecar rather than from a
native arrangement, as the paper assumes. This is a tracked deviation — see
`docs/yang_deviations.md`. It is resolved: M6 (native Stage 2) and M7 (WASM)
are both complete.
