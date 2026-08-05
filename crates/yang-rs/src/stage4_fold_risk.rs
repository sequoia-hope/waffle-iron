//! Stage-4 fold-risk planner (Yang 2025 §4.4.1) — increment N2-3a.
//!
//! # The paper's own words
//!
//! §4.4.1 opens: *"As the intersections on the surfaces are relocated and
//! refined during the optimization, the bijectivity is essentially broken. Each
//! intersection curve is no longer mapped to the corresponding intersection
//! curve between the two meshes, **thus causing gaps or self-intersections**."*
//! (`refs/text/yang2025_hybrid_boolean.txt:605`.)
//!
//! That is the anchored F0067 / R0074 class verbatim: Stage 4 relocates the
//! intersection vertices onto the exact curve, the mesh around them is NOT
//! updated, and the loop Stage 5/6 later extracts from that stale mesh crosses
//! itself. The repair the paper prescribes is the Fig-11 mesh update
//! (`stage4_update::stage4_mesh_update`, built and unit-tested under N2-1,
//! still unwired) — NOT §4.5.2 local refinement, whose trigger is optimization
//! NON-convergence. F0067's relocations converge exactly; §4.5.2 does not apply
//! to it, and the roadmap's own finding Q3 already measured §4.5.2 as
//! recovering ~zero current cases.
//!
//! # What this module decides
//!
//! WHICH relocations need the Fig-11 treatment. The criterion is not invented
//! here — it is the one the 2026-07-29 R0074 fold census measured and recorded
//! in `docs/yang_deviations.md`: a fold is minted when the relocation
//! **displacement exceeds the pre-relocation spacing of the adjacent chain
//! vertices**. On R0074's 78 folds, `ratio < 1` was violated by 14 of the 16
//! Stage-4-MINTED folds and respected by 56 of the 62 INHERITED ones. The
//! displacement there was ~97% NORMAL to the chain, so what inverts local order
//! is its MAGNITUDE relative to the spacing, not its direction — which is why
//! the statistic is a bare ratio and carries no directional term.
//!
//! The 2026-08-03 loop-simplicity census reached the same statistic from the
//! other end: every self-crossing emitted planar loop reports
//! `disp_over_min_seg` well above 1 (F0067 41x…52,187x; the anchored notch is
//! 5.8x), and no SUPPORTED_CORRECT case has a self-crossing loop at all.
//!
//! # Scope of this increment
//!
//! Pure planner: pre/post positions + chain adjacency in, ranked risk list out.
//! No mesh mutation. Wired read-only at the end of Stage 4 behind
//! `YANG_S4_FOLD_RISK` (N2-3b step 1); applying the Fig-11 merge arm to the
//! plan is step 2. Landing the decision function first, unit-tested in
//! isolation, is the same shape N2-1 used for `stage4_mesh_update`.
//!
//! # MEASURED 2026-08-05 — adjacency widened, and the RATIO ALONE OVER-SELECTS
//!
//! Curve-key adjacency scored **0 on R0074**, the very case whose 16 minted
//! folds the 07-29 census measured: that census walked the patch BOUNDARY
//! CYCLE, so "adjacent chain vertices" meant cycle neighbours. Widening to
//! [`cycle_adjacency`] fixes it — but the counts it produces are NOT
//! comparable to the census's:
//!
//! | case  | adj edges | (curve only) | scored | minting | % |
//! |-------|----------:|-------------:|-------:|--------:|--:|
//! | R0074 |      2116 |        **0** |    329 |      95 | 29% |
//! | R0011 |      1391 |           39 |    115 |      20 | 17% |
//! | F0067 |      4858 |          738 |     76 |      74 | 97% |
//! | R0085 |      7708 |         1843 |    912 |     845 | 93% |
//!
//! R0085 went 2 → 845 and F0067 71 → 74. **The 07-29 census computed this
//! ratio only over the 78 vertices ALREADY IDENTIFIED AS FOLDS (turn angle
//! > 120°); this planner computes it over every vertex that MOVED.** Different
//! denominators, so "845 minting" is not 845 defects — it is 845 vertices whose
//! displacement exceeds their tightest cycle spacing, most of which never
//! folded. Widening also makes `min_pre_spacing` a minimum over a much larger
//! set, so a single sub-resolution near-duplicate neighbour (the pipeline
//! collapses these later anyway) drives the ratio for everything around it.
//!
//! **Consequence: `ratio >= 1` is a NECESSARY but not SUFFICIENT condition.**
//! The missing half is the fold restriction — local order actually inverting —
//! which is what made the census's 14/16-vs-56/62 separation meaningful.
//!
//! # The fold test closes it (2026-08-05)
//!
//! [`classify_folds`] measures each cycle corner's turn angle at pre- AND
//! post-Stage-4 positions and splits `Minted` (straight before, folded after —
//! Stage 4 caused it) from `Inherited` (already folded before Stage 4, so a
//! Stage-2/3 defect the Fig-11 merge would not repair).
//! [`merge_customers`] is the intersection, and it is what the merge arm
//! consumes:
//!
//! | case  | scored | ratio-only | minted folds | inherited | **customers** |
//! |-------|-------:|-----------:|-------------:|----------:|--------------:|
//! | R0074 |    329 |         95 |           38 |    **62** |        **25** |
//! | F0067 |     76 |         74 |           85 |        16 |        **32** |
//! | R0011 |     41 |         13 |            9 |         2 |         **4** |
//! | R0085 |    912 |        845 |          120 |       144 |        **72** |
//!
//! R0085's 845 becomes 72 — the over-selection is cut by 92%, leaving a
//! repair-sized set rather than a mesh-rewrite-sized one.
//!
//! # The 38-vs-16 discrepancy, RESOLVED (2026-08-05)
//!
//! The 07-29 census recorded R0074 as 16 minted + 62 inherited; the first
//! measurement here said 38 minted + 62 inherited. Two distinct causes, found
//! by dumping each minted corner's `turn_pre`:
//!
//! **1. A BUG here — 11 of the 22.** Every high-turn corner was printed TWICE
//! with `prev`/`next` swapped: a corner on a boundary shared by two patches is
//! visited once per patch, in opposite winding, and `turn_deg` is invariant
//! under that reversal. [`classify_folds`] keyed by cycle POSITION and so
//! reported one geometric corner twice. Now keyed by corner IDENTITY —
//! `(vertex, {neighbours})` unordered — which takes minted 38 → **27**.
//! `inherited` stayed 62 (those sit on single-patch cycles) and
//! `MERGE_CUSTOMERS` stayed 25, because [`merge_customers`] already dedups
//! through a vertex set. Pinned by
//! `a_corner_shared_by_two_cycles_is_reported_once`.
//!
//! **2. Pipeline drift — the residual 11.** 23 commits have landed in
//! `stage4_correct.rs` / `stage4_relocate.rs` / `stage0/` since 07-29,
//! including several that changed minting directly: the §4.4.1 mutual-pair arm
//! going always-on (84e9759a), amendment-19 (70d9df45, which explicitly took
//! F0067's Stage-4 crack field from 16 unbalanced edges to 0), the
//! two-sidedness precondition (d5b41a94), inc-6 (3adece7e), provenance-first
//! classification (1a9cee36). **The census's 16 measured a pipeline that no
//! longer exists.** Requiring today's count to reproduce it would be requiring
//! reproduction of a superseded state.
//!
//! What remains meaningful is that `inherited = 62` matches EXACTLY: that
//! population is inherited from Stage 2/3 and is evidently stable across all
//! 23 commits, which is precisely the calibration one wants — while the MINTED
//! count is exactly what those amendments would be expected to move.
//!
//! # The chord test — short-cycle exclusion, without a threshold (2026-08-05)
//!
//! [`chord_order_inversions`] is the F0067 anchor's own certificate (its minted
//! vertex lay on a neighbouring edge's supporting line at **t = −0.606**,
//! outside that edge). It asks only whether a vertex sat BETWEEN its two
//! neighbours on the chord before and lies PAST one of them after. No angle, no
//! constant, and — the point — **no short-cycle degeneracy**: an ordinary
//! triangle corner projects inside `(0, 1)` exactly as a hexagon corner does,
//! where the fixed 120° turn threshold flags two of that triangle's three
//! corners (`short_cycles_turn_sharply_by_construction`).
//!
//! Measured against the turn test, post-dedup:
//!
//! | case          | minted turn | chord inv | agree | turn only | chord only |
//! |---------------|------------:|----------:|------:|----------:|-----------:|
//! | R0074         |          27 |        24 |    23 |     **3** |          1 |
//! | F0067         |          72 |        75 |    72 |         0 |          3 |
//! | R0011 op1/op2 |         9/2 |       6/2 |   5/2 |       2/0 |        1/0 |
//! | R0085 op1/op2 |       80/30 |     78/32 | 73/29 |       1/1 |        5/3 |
//!
//! Two independent signals agreeing on the bulk is the calibration; the
//! disagreements are the information:
//!
//! - **`turn only` = the flagged 90–120° bucket.** R0074 has exactly 3 corners
//!   with `turn_pre` in 90–120° and exactly 3 `turn only` disagreements. Those
//!   are the "already nearly folded, nudged across the line" corners — not the
//!   census's `turn_pre ≈ 0 → 179.9x°` phenomenon. **The chord test excludes
//!   them by construction, which is the mechanism the previous note asked for
//!   in place of a second threshold.**
//! - **`chord only`** are vertices that slid PAST a neighbour along a nearly
//!   straight chain: order inverted, turn angle never large. Genuine folds the
//!   turn test cannot see.
//!
//! Neither is subsumed by the other, so [`classify_folds`] is kept unchanged —
//! its `inherited = 62` remains the tie to the 07-29 census.
//!
//! # GATED TRIAL on F0067 — NEGATIVE RESULT (2026-08-05)
//!
//! Gate chosen: [`merge_customers_chord`] (`ratio >= 1` ∩ chord inversion).
//! `YANG_S4_FIG11_MERGE` applied it to F0067's failing op: **33 customers, 33
//! merges applied, 0 skipped**. The case did NOT convert — but the wall MOVED:
//!
//! ```text
//! gate OFF: TessellationFailed { face: FaceId(3994), "ring rejected by CDT" }
//! gate ON : BooleanFailed("yang-rs: reassembled output would be non-2-manifold")
//! ```
//!
//! **The selection is not what failed; the REPAIR PRIMITIVE is.** All 33
//! customers were applicable and none cascaded, so the plan behaved as
//! designed. What broke is that this trial fused each pair with
//! `collapse_vertex`, which only rewrites triangle indices and drops
//! degenerates. **A bare edge collapse is NOT Yang's Fig-11 merge.** The
//! paper's merge happens INSIDE the §4.4.1 parametric-domain re-triangulation
//! (`stage4_update::stage4_mesh_update`): fuse the vertex AND re-triangulate
//! the affected patch so the result is still a valid triangulation. Collapsing
//! a real-length edge — these are 3.7e-3-scale, not the sub-resolution edges
//! `collapse_vertex` exists for — leaves the surrounding fan inconsistent, and
//! Stage 6's 2-manifold gate says so.
//!
//! So the substitution was mine, not the paper's, and the loud STOP caught it
//! instead of shipping wrong geometry. **Next: route this same plan into
//! `stage4_mesh_update` rather than `collapse_vertex`** — which is exactly the
//! built-but-unwired primitive N2-1 landed for this purpose. The arm is kept,
//! gated off, as the scaffolding for that wiring and as the record of what a
//! bare collapse does.

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// One relocation whose displacement is large enough, relative to its own
/// chain neighbourhood, to invert local order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoldRisk {
    /// The relocated mesh vertex.
    pub vertex: u32,
    /// How far Stage 4 moved it.
    pub displacement: f64,
    /// Distance to its CLOSEST chain neighbour, measured at PRE-relocation
    /// positions — the spacing the displacement has to fit inside.
    pub min_pre_spacing: f64,
    /// The neighbour realizing `min_pre_spacing`; the Fig-11 `merge`
    /// candidate.
    pub nearest_neighbour: u32,
    /// `displacement / min_pre_spacing`. `>= 1.0` is the minted-fold class.
    pub ratio: f64,
}

/// Rank every relocation by fold risk.
///
/// `pre` maps vertex → PRE-Stage-4 position (the `S4_PRE_POS` oracle's
/// contract: a POSITION, not a displacement, so it survives the four
/// `compact_unreferenced_verts` renumberings that run after Stage 4 — the gap
/// that left R0011/F0045 unmeasured in the 2026-07-29 census and that
/// `probe_remap_pre_pos` has since closed). `post` is the current mesh.
/// `chain_edges` are the `intersection_curves` keys: `(a, b)` means `a` and `b`
/// are consecutive on an analytic intersection chain.
///
/// A vertex is scored only when it MOVED, has a pre position, and has at least
/// one chain neighbour that also has a pre position — the spacing is otherwise
/// not defined, and reporting a risk without one would be a guess. Vertices
/// minted during Stage 4 (`pre` absent) are excluded for the same reason.
/// Returns every scored vertex, worst ratio first, so the caller applies its
/// own threshold rather than inheriting one from here.
pub fn rank_fold_risks(
    pre: &HashMap<u32, [f64; 3]>,
    post: &[[f64; 3]],
    chain_edges: &BTreeSet<(u32, u32)>,
) -> Vec<FoldRisk> {
    // Chain adjacency, both directions.
    let mut adj: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for &(a, b) in chain_edges {
        if a == b {
            continue;
        }
        adj.entry(a).or_default().insert(b);
        adj.entry(b).or_default().insert(a);
    }

    let dist = |p: [f64; 3], q: [f64; 3]| {
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    };

    let mut out = Vec::new();
    for (&v, &p0) in pre {
        let Some(&p1) = post.get(v as usize) else {
            continue;
        };
        let displacement = dist(p0, p1);
        if displacement == 0.0 {
            continue;
        }
        let Some(nbrs) = adj.get(&v) else {
            continue;
        };
        // Spacing is measured PRE-relocation on BOTH endpoints: the neighbour's
        // post position may itself have moved, and comparing a post-move
        // displacement against a post-move spacing would measure the outcome
        // rather than the risk.
        let mut best: Option<(u32, f64)> = None;
        for &w in nbrs {
            let Some(&q0) = pre.get(&w) else {
                continue;
            };
            let d = dist(p0, q0);
            if d == 0.0 {
                continue;
            }
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((w, d));
            }
        }
        let Some((nearest_neighbour, min_pre_spacing)) = best else {
            continue;
        };
        out.push(FoldRisk {
            vertex: v,
            displacement,
            min_pre_spacing,
            nearest_neighbour,
            ratio: displacement / min_pre_spacing,
        });
    }
    // Worst first; vertex id breaks ties so the order is deterministic
    // (`pre` is a HashMap, so iteration order is not).
    out.sort_by(|a, b| {
        b.ratio
            .partial_cmp(&a.ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.vertex.cmp(&b.vertex))
    });
    out
}

/// Build the adjacency [`rank_fold_risks`] scores against: every consecutive
/// pair of each patch BOUNDARY CYCLE, unioned with the analytic curve edges.
///
/// The boundary cycle — not the `intersection_curves` key set — is the
/// structure the 2026-07-29 R0074 fold census actually walked when it measured
/// turn angles and called their endpoints "adjacent chain vertices", and it is
/// the same neighbourhood the 2026-08-03 loop-simplicity census used. Scoring
/// against curve keys alone measured a STRICT SUBSET and reported `scored=0` on
/// R0074, whose failing op has no intersection curves at all.
///
/// The union can only ADD neighbours, and `min_pre_spacing` is a minimum over
/// them, so widening can only LOWER the spacing and RAISE the ratio — i.e. it
/// can only reveal fold risk, never hide it. That is the safe direction for a
/// planner whose output gates a repair: under-reporting leaves a defect
/// unrepaired, over-reporting is caught by the acceptance check on the repair.
///
/// Cycles are CLOSED: the last vertex is adjacent to the first. Pairs are
/// canonicalized `(min, max)` so the union dedups; `rank_fold_risks` expands
/// both directions itself, so orientation carries no meaning here.
pub fn cycle_adjacency<'a>(
    cycles: impl IntoIterator<Item = &'a [u32]>,
    curve_edges: &BTreeSet<(u32, u32)>,
) -> BTreeSet<(u32, u32)> {
    let mut out: BTreeSet<(u32, u32)> = curve_edges
        .iter()
        .filter(|(a, b)| a != b)
        .map(|&(a, b)| (a.min(b), a.max(b)))
        .collect();
    for cyc in cycles {
        let n = cyc.len();
        if n < 2 {
            continue;
        }
        for i in 0..n {
            let (a, b) = (cyc[i], cyc[(i + 1) % n]);
            if a != b {
                out.insert((a.min(b), a.max(b)));
            }
        }
    }
    out
}

/// Turn-angle threshold, in degrees, above which a cycle corner counts as a
/// FOLD.
///
/// Not a tunable band. It is the classification threshold the 2026-07-29 R0074
/// census used when it separated 16 MINTED folds (`turn_pre` 0.00° → 179.9x°)
/// from 62 INHERITED ones (already >120° before Stage 4, perturbed by a median
/// 1.25°). Changing it would not "recover" a case — it would redefine the
/// population the census measured, and silently invalidate the comparison this
/// whole plan rests on.
pub const FOLD_TURN_DEG: f64 = 120.0;

/// How a cycle corner's turn angle behaved across Stage 4.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FoldClass {
    /// Turn stayed below [`FOLD_TURN_DEG`]. Not a fold.
    None,
    /// Already folded BEFORE Stage 4 — inherited from the Stage-2/3 boundary
    /// cycle. Stage 4 did not cause it, so the Fig-11 merge is not its repair.
    Inherited,
    /// Straight (or merely bent) before Stage 4 and folded after: **Stage 4
    /// minted this one**. This is the Fig-11 merge's actual customer set.
    Minted,
}

/// One cycle corner's turn angle, measured at pre- and post-Stage-4 positions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoldTurn {
    /// The corner vertex.
    pub vertex: u32,
    /// Its two cycle neighbours, in walk order.
    pub prev: u32,
    /// See [`FoldTurn::prev`].
    pub next: u32,
    /// Deviation from straight at PRE positions, degrees. 0 = collinear
    /// forward, 180 = complete doubling back.
    pub turn_pre_deg: f64,
    /// Deviation from straight at POST positions, degrees.
    pub turn_post_deg: f64,
    /// The verdict.
    pub class: FoldClass,
}

/// Deviation from straight at `b`, walking `a -> b -> c`, in degrees.
/// `None` when either leg has zero length (the angle is undefined there).
fn turn_deg(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<f64> {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
    let nu = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    let nv = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if nu == 0.0 || nv == 0.0 || !nu.is_finite() || !nv.is_finite() {
        return None;
    }
    let dot = (u[0] * v[0] + u[1] * v[1] + u[2] * v[2]) / (nu * nv);
    Some(dot.clamp(-1.0, 1.0).acos().to_degrees())
}

/// Classify every cycle corner as [`FoldClass`], comparing the turn angle at
/// pre- and post-Stage-4 positions.
///
/// This is the half of the plan `minting_risks` cannot supply. The ratio says a
/// displacement was large enough to invert local order; this says local order
/// ACTUALLY inverted, and — decisively — whether Stage 4 is what inverted it.
///
/// ONE cycle structure is walked with TWO position maps. The cycles come from
/// the post-Stage-4 `compute_phase_a`, so they name current indices; the pre
/// positions are the same vertices' earlier locations. That is the same
/// construction the 07-29 census used ("each fold's turn angle at the
/// pre-Stage-4 positions"), and it is why the pre map must store POSITIONS
/// rather than displacements.
///
/// Corners whose turn is undefined at EITHER time (a zero-length leg) are
/// omitted entirely — never silently reported as `None`, which would read as
/// "measured, no fold" when nothing was measured.
pub fn classify_folds<'a>(
    cycles: impl IntoIterator<Item = &'a [u32]>,
    pre: &HashMap<u32, [f64; 3]>,
    post: &[[f64; 3]],
) -> Vec<FoldTurn> {
    // Keyed by CORNER IDENTITY — `(vertex, {neighbours})` with the neighbour
    // pair unordered — not by cycle position. A corner on a boundary shared by
    // two patches is visited once per patch, in OPPOSITE winding, and
    // `turn_deg` is invariant under that reversal (it compares `b-a` with
    // `c-b`; swapping negates both). Keying by position would therefore report
    // one geometric corner twice and inflate every count derived from it.
    let mut seen: BTreeMap<(u32, u32, u32), FoldTurn> = BTreeMap::new();
    for cyc in cycles {
        let n = cyc.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let (a, b, c) = (cyc[(i + n - 1) % n], cyc[i], cyc[(i + 1) % n]);
            if a == b || b == c || a == c {
                continue;
            }
            let (Some(&pa), Some(&pb), Some(&pc)) = (pre.get(&a), pre.get(&b), pre.get(&c)) else {
                continue;
            };
            let (Some(&qa), Some(&qb), Some(&qc)) = (
                post.get(a as usize),
                post.get(b as usize),
                post.get(c as usize),
            ) else {
                continue;
            };
            let (Some(t0), Some(t1)) = (turn_deg(pa, pb, pc), turn_deg(qa, qb, qc)) else {
                continue;
            };
            let class = if t1 <= FOLD_TURN_DEG {
                FoldClass::None
            } else if t0 > FOLD_TURN_DEG {
                FoldClass::Inherited
            } else {
                FoldClass::Minted
            };
            seen.entry((b, a.min(c), a.max(c))).or_insert(FoldTurn {
                vertex: b,
                prev: a,
                next: c,
                turn_pre_deg: t0,
                turn_post_deg: t1,
                class,
            });
        }
    }
    let mut out: Vec<FoldTurn> = seen.into_values().collect();
    out.sort_by(|x, y| {
        y.turn_post_deg
            .partial_cmp(&x.turn_post_deg)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(x.vertex.cmp(&y.vertex))
    });
    out
}

/// Where `b` projects onto the line through `a` and `c`, as the parameter `t`
/// with `a` at 0 and `c` at 1. `None` when `a == c` (no line).
///
/// `t` inside `[0, 1]` means `b` sits BETWEEN its two neighbours along the
/// chord; outside means it has moved PAST one of them — which is what "local
/// order inverted" means, stated without any angle or threshold.
fn chord_param(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<f64> {
    let d = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let len2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if len2 == 0.0 || !len2.is_finite() {
        return None;
    }
    let w = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    Some((w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / len2)
}

/// Corners whose CHORD ORDER Stage 4 inverted: `b` sat between its neighbours
/// before and lies past one of them after.
///
/// This is the same certificate the F0067 anchor used — the minted vertex lay
/// on a neighbouring edge's supporting line at **t = −0.606**, outside that
/// edge, "which IS the doubling-back".
///
/// **It needs no threshold and no cycle-length correction**, which is why it
/// exists alongside [`classify_folds`] rather than being folded into it. The
/// turn-angle test carries a fixed [`FOLD_TURN_DEG`]; because a convex n-gon
/// turns 360/n at every corner, that constant equals a TRIANGLE's own per-corner
/// turn, so on 3- and 4-vertex cycles it cannot separate "folded" from "small
/// loop" (`short_cycles_turn_sharply_by_construction`). A chord parameter has
/// no such degeneracy: an ordinary triangle corner projects at `t` inside
/// `(0, 1)` exactly as a hexagon corner does.
///
/// [`classify_folds`] is kept unchanged because its `inherited = 62` on R0074
/// is the calibration against the 2026-07-29 census; this is a SECOND,
/// independent signal, not a replacement. Where the two disagree, that
/// disagreement is a measurement to explain, not a knob to turn.
pub fn chord_order_inversions<'a>(
    cycles: impl IntoIterator<Item = &'a [u32]>,
    pre: &HashMap<u32, [f64; 3]>,
    post: &[[f64; 3]],
) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for cyc in cycles {
        let n = cyc.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let (a, b, c) = (cyc[(i + n - 1) % n], cyc[i], cyc[(i + 1) % n]);
            if a == b || b == c || a == c {
                continue;
            }
            let (Some(&pa), Some(&pb), Some(&pc)) = (pre.get(&a), pre.get(&b), pre.get(&c)) else {
                continue;
            };
            let (Some(&qa), Some(&qb), Some(&qc)) = (
                post.get(a as usize),
                post.get(b as usize),
                post.get(c as usize),
            ) else {
                continue;
            };
            let (Some(t0), Some(t1)) = (chord_param(pa, pb, pc), chord_param(qa, qb, qc)) else {
                continue;
            };
            if (0.0..=1.0).contains(&t0) && !(0.0..=1.0).contains(&t1) {
                out.insert(b);
            }
        }
    }
    out
}

/// The merge arm's gate as chosen 2026-08-05: `ratio >= 1` INTERSECTED with a
/// [`chord_order_inversions`] certificate.
///
/// Chosen over the turn-angle variant ([`merge_customers`]) because it is
/// threshold-free, it is the certificate the F0067 anchor itself used, and it
/// excludes the 90–120° `turn_pre` corners that were never the census's
/// `turn_pre ≈ 0 → 179.9x°` phenomenon. Ordering is inherited from `risks`
/// (worst ratio first), so the caller applies merges most-severe-first
/// deterministically.
pub fn merge_customers_chord(risks: &[FoldRisk], inversions: &BTreeSet<u32>) -> Vec<FoldRisk> {
    risks
        .iter()
        .copied()
        .filter(|r| r.ratio >= 1.0 && inversions.contains(&r.vertex))
        .collect()
}

/// The Fig-11 merge arm's ACTUAL customer set: vertices that both were moved
/// beyond their own spacing (`ratio >= 1`) **and** whose cycle corner Stage 4
/// actually folded ([`FoldClass::Minted`]).
///
/// Neither half is sufficient alone. The ratio over-selects — on R0085 it flags
/// 845 of 912 moved vertices, most of which never folded. The fold class
/// under-specifies the repair — an inherited fold is a Stage-2/3 defect the
/// Fig-11 merge would not fix, and a minted fold whose displacement fits inside
/// its spacing has some other cause. The intersection is what the 07-29 census
/// actually validated.
pub fn merge_customers(risks: &[FoldRisk], folds: &[FoldTurn]) -> Vec<FoldRisk> {
    let minted: BTreeSet<u32> = folds
        .iter()
        .filter(|f| f.class == FoldClass::Minted)
        .map(|f| f.vertex)
        .collect();
    risks
        .iter()
        .copied()
        .filter(|r| r.ratio >= 1.0 && minted.contains(&r.vertex))
        .collect()
}

/// The subset that CAN mint a fold: `ratio >= 1`, i.e. the vertex is moved
/// further than the gap it has to stay inside.
///
/// `>=` rather than `>`: at ratio exactly 1 the vertex lands ON its neighbour,
/// which is the Fig-11 `merge` case, not a safe relocation.
///
/// **NECESSARY, NOT SUFFICIENT — do not drive a repair from this alone.** The
/// 07-29 census's 14/16-vs-56/62 separation was computed over vertices ALREADY
/// IDENTIFIED AS FOLDS (turn angle > 120°); this function ranges over every
/// vertex that moved, so on R0085 it selects 845 of 912. A merge applied to
/// that set would rewrite a mesh that is mostly fine. The caller must intersect
/// this with a fold test before acting.
pub fn minting_risks(risks: &[FoldRisk]) -> Vec<FoldRisk> {
    risks.iter().copied().filter(|r| r.ratio >= 1.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(u32, [f64; 3])]) -> HashMap<u32, [f64; 3]> {
        pairs.iter().copied().collect()
    }
    fn e(pairs: &[(u32, u32)]) -> BTreeSet<(u32, u32)> {
        pairs.iter().copied().collect()
    }

    /// A chain of three, middle vertex nudged well inside its spacing.
    #[test]
    fn small_displacement_is_not_a_fold_risk() {
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [2.0, 0.0, 0.0]),
        ]);
        let post = vec![[0.0, 0.0, 0.0], [1.0, 0.1, 0.0], [2.0, 0.0, 0.0]];
        let r = rank_fold_risks(&pre, &post, &e(&[(0, 1), (1, 2)]));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].vertex, 1);
        assert!((r[0].ratio - 0.1).abs() < 1e-12, "{r:?}");
        assert!(minting_risks(&r).is_empty());
    }

    /// The F0067 shape: displacement several times the local spacing.
    #[test]
    fn displacement_beyond_the_spacing_is_a_minting_risk() {
        let seg = 6.4e-4;
        let push = 3.7e-3;
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [seg, 0.0, 0.0]),
            (2, [2.0 * seg, 0.0, 0.0]),
        ]);
        let post = vec![[0.0, 0.0, 0.0], [seg, push, 0.0], [2.0 * seg, 0.0, 0.0]];
        let r = rank_fold_risks(&pre, &post, &e(&[(0, 1), (1, 2)]));
        let mint = minting_risks(&r);
        assert_eq!(mint.len(), 1);
        assert_eq!(mint[0].vertex, 1);
        // The anchored 5.8x.
        assert!(
            (mint[0].ratio - push / seg).abs() < 1e-12,
            "ratio should be displacement/spacing: {mint:?}"
        );
        assert!(mint[0].ratio > 5.7 && mint[0].ratio < 5.9);
        assert_eq!(mint[0].nearest_neighbour, 0);
    }

    /// Spacing must come from the CLOSEST neighbour — a chain with one tight
    /// and one loose side is bounded by the tight one.
    #[test]
    fn spacing_is_the_closest_neighbour_not_the_average() {
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [1.01, 0.0, 0.0]),
        ]);
        let post = vec![[0.0, 0.0, 0.0], [1.0, 0.05, 0.0], [1.01, 0.0, 0.0]];
        let r = rank_fold_risks(&pre, &post, &e(&[(0, 1), (1, 2)]));
        let v1 = r.iter().find(|x| x.vertex == 1).unwrap();
        assert_eq!(v1.nearest_neighbour, 2);
        assert!((v1.min_pre_spacing - 0.01).abs() < 1e-12);
        assert!(v1.ratio > 1.0, "0.05 into a 0.01 gap must mint: {v1:?}");
    }

    /// Both endpoints are read at PRE positions: a neighbour that also moved
    /// must not change this vertex's measured spacing.
    #[test]
    fn spacing_ignores_where_the_neighbour_moved_to() {
        let pre = m(&[(0, [0.0, 0.0, 0.0]), (1, [1.0, 0.0, 0.0])]);
        let near = vec![[0.0, 0.0, 0.0], [1.0, 0.5, 0.0]];
        let far = vec![[0.0, -9.0, 0.0], [1.0, 0.5, 0.0]];
        let a = rank_fold_risks(&pre, &near, &e(&[(0, 1)]));
        let b = rank_fold_risks(&pre, &far, &e(&[(0, 1)]));
        let ra = a.iter().find(|x| x.vertex == 1).unwrap();
        let rb = b.iter().find(|x| x.vertex == 1).unwrap();
        assert_eq!(ra.min_pre_spacing, rb.min_pre_spacing);
        assert_eq!(ra.ratio, rb.ratio);
    }

    #[test]
    fn unmoved_vertices_and_stage4_mints_are_not_scored() {
        // v0 did not move; v9 has no pre position (minted during Stage 4);
        // v1 moved but has no chain neighbour with a pre position.
        let pre = m(&[(0, [0.0, 0.0, 0.0]), (1, [1.0, 0.0, 0.0])]);
        let post = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        assert!(rank_fold_risks(&pre, &post, &e(&[(1, 9)])).is_empty());
        // With a scorable neighbour it appears.
        assert_eq!(rank_fold_risks(&pre, &post, &e(&[(0, 1)])).len(), 1);
    }

    #[test]
    fn ranking_is_worst_first_and_deterministic() {
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [2.0, 0.0, 0.0]),
            (3, [3.0, 0.0, 0.0]),
        ]);
        let post = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.2, 0.0],
            [2.0, 3.0, 0.0],
            [3.0, 0.5, 0.0],
        ];
        let edges = e(&[(0, 1), (1, 2), (2, 3)]);
        let first = rank_fold_risks(&pre, &post, &edges);
        assert_eq!(
            first.iter().map(|r| r.vertex).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
        for _ in 0..8 {
            assert_eq!(rank_fold_risks(&pre, &post, &edges), first);
        }
    }

    /// Ratio exactly 1 lands the vertex on its neighbour — the Fig-11 `merge`
    /// case, so it belongs in the minting set.
    #[test]
    fn ratio_exactly_one_counts_as_minting() {
        let pre = m(&[(0, [0.0, 0.0, 0.0]), (1, [1.0, 0.0, 0.0])]);
        let post = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
        let r = rank_fold_risks(&pre, &post, &e(&[(0, 1)]));
        assert_eq!(r[0].ratio, 1.0);
        assert_eq!(minting_risks(&r).len(), 1);
    }

    #[test]
    fn cycle_adjacency_closes_the_loop_and_unions_the_curve_edges() {
        let cyc: Vec<u32> = vec![0, 1, 2, 3];
        let got = cycle_adjacency([cyc.as_slice()], &e(&[(7, 9)]));
        // Consecutive pairs INCLUDING the 3->0 wrap, plus the curve edge.
        assert_eq!(got, e(&[(0, 1), (1, 2), (2, 3), (0, 3), (7, 9)]), "{got:?}");
    }

    #[test]
    fn cycle_adjacency_canonicalizes_and_drops_self_pairs() {
        let cyc: Vec<u32> = vec![5, 5, 2];
        // (5,5) is dropped; (5,2)/(2,5) canonicalize to one entry.
        let got = cycle_adjacency([cyc.as_slice()], &e(&[(2, 5), (4, 4)]));
        assert_eq!(got, e(&[(2, 5)]), "{got:?}");
    }

    /// The whole point of widening: a vertex whose only CURVE neighbour is far
    /// away but whose CYCLE neighbour is close must become scorable, and its
    /// ratio can only go up.
    #[test]
    fn widening_reveals_risk_that_curve_edges_alone_miss() {
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [1.001, 0.0, 0.0]),
        ]);
        let post = vec![[0.0, 0.0, 0.0], [1.0, 0.05, 0.0], [1.001, 0.0, 0.0]];
        // Curve edges alone: v1's only neighbour is the distant v0 → ratio 0.05.
        let narrow = rank_fold_risks(&pre, &post, &e(&[(0, 1)]));
        let n1 = narrow.iter().find(|r| r.vertex == 1).unwrap();
        assert!(n1.ratio < 1.0 && minting_risks(&narrow).is_empty());
        // With the cycle, the tight neighbour v2 appears and it mints.
        let cyc: Vec<u32> = vec![0, 1, 2];
        let wide = rank_fold_risks(
            &pre,
            &post,
            &cycle_adjacency([cyc.as_slice()], &e(&[(0, 1)])),
        );
        let w1 = wide.iter().find(|r| r.vertex == 1).unwrap();
        assert_eq!(w1.nearest_neighbour, 2);
        assert!(w1.ratio > n1.ratio, "widening must not lower the ratio");
        assert_eq!(minting_risks(&wide).len(), 1);
    }

    /// The census's own signature: straight before (0.00°), doubled back
    /// after (179.9x°).
    #[test]
    fn straight_before_and_doubled_back_after_is_MINTED() {
        let cyc: Vec<u32> = vec![0, 1, 2];
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [2.0, 0.0, 0.0]),
        ]);
        // v1 pulled back past v0's side: the walk 0->1->2 now doubles back.
        let post = vec![[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        let f = classify_folds([cyc.as_slice()], &pre, &post);
        let v1 = f.iter().find(|x| x.vertex == 1).unwrap();
        assert!(v1.turn_pre_deg < 1e-9, "pre must be straight: {v1:?}");
        assert!(v1.turn_post_deg > 179.0, "{v1:?}");
        assert_eq!(v1.class, FoldClass::Minted);
    }

    /// Already folded before Stage 4 and merely perturbed: INHERITED, and the
    /// Fig-11 merge is not its repair.
    #[test]
    fn already_folded_before_stage4_is_INHERITED() {
        let cyc: Vec<u32> = vec![0, 1, 2];
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [3.0, 0.0, 0.0]),
            (2, [2.0, 0.0, 0.0]),
        ]);
        let post = vec![[0.0, 0.0, 0.0], [3.01, 0.0, 0.0], [2.0, 0.0, 0.0]];
        let f = classify_folds([cyc.as_slice()], &pre, &post);
        let v1 = f.iter().find(|x| x.vertex == 1).unwrap();
        assert!(v1.turn_pre_deg > FOLD_TURN_DEG);
        assert_eq!(v1.class, FoldClass::Inherited);
    }

    #[test]
    fn a_gentle_corner_is_not_a_fold() {
        // A regular hexagon: every corner turns 60 deg, well under the
        // threshold. Deliberately NOT a triangle — see
        // `short_cycles_turn_sharply_by_construction`.
        let pts: Vec<[f64; 3]> = (0..6)
            .map(|k| {
                let t = std::f64::consts::PI / 3.0 * f64::from(k);
                [t.cos(), t.sin(), 0.0]
            })
            .collect();
        let cyc: Vec<u32> = (0..6).collect();
        let pre: HashMap<u32, [f64; 3]> = pts
            .iter()
            .enumerate()
            .map(|(i, &p)| (i as u32, p))
            .collect();
        let f = classify_folds([cyc.as_slice()], &pre, &pts);
        assert_eq!(f.len(), 6);
        for c in &f {
            assert_eq!(c.class, FoldClass::None, "{c:?}");
            assert!((c.turn_post_deg - 60.0).abs() < 1e-9, "{c:?}");
        }
    }

    /// A LIMITATION, pinned so it stays visible: the turn at each corner of a
    /// convex n-gon is 360/n deg, so a TRIANGLE turns exactly
    /// [`FOLD_TURN_DEG`] at every corner and anything less regular exceeds it.
    /// The absolute threshold therefore cannot separate "folded" from "small
    /// cycle" on 3- and 4-vertex loops — of which the corpus has many. Any
    /// consumer must either exclude short cycles or compare against the
    /// cycle's own expected turn; `merge_customers` currently does NEITHER, so
    /// this is an open constraint on wiring the merge arm, not a solved one.
    #[test]
    fn short_cycles_turn_sharply_by_construction() {
        // A perfectly ordinary triangle, unmoved by Stage 4.
        let pts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 1.0, 0.0]];
        let cyc: Vec<u32> = vec![0, 1, 2];
        let pre: HashMap<u32, [f64; 3]> = pts
            .iter()
            .enumerate()
            .map(|(i, &p)| (i as u32, p))
            .collect();
        let f = classify_folds([cyc.as_slice()], &pre, &pts);
        // Two of its three corners already read as folds, with NOTHING moved.
        let folded = f.iter().filter(|c| c.class != FoldClass::None).count();
        assert_eq!(folded, 2, "{f:?}");
        // They are INHERITED, never MINTED, because pre == post — which is the
        // property that keeps this from reaching `merge_customers`.
        assert!(f.iter().all(|c| c.class != FoldClass::Minted), "{f:?}");
    }

    /// An unmeasurable corner is OMITTED, never reported as `None` — "no fold"
    /// and "nothing measured" must not look alike.
    #[test]
    fn zero_length_leg_is_omitted_not_classified_none() {
        let cyc: Vec<u32> = vec![0, 1, 2];
        // v0 and v1 coincide at PRE: the incoming leg has zero length.
        let pre = m(&[
            (0, [1.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [2.0, 0.0, 0.0]),
        ]);
        let post = vec![[1.0, 0.0, 0.0], [1.5, 0.0, 0.0], [2.0, 0.0, 0.0]];
        let f = classify_folds([cyc.as_slice()], &pre, &post);
        assert!(f.iter().all(|x| x.vertex != 1), "{f:?}");
    }

    /// A corner shared by two patches is walked once per patch, in OPPOSITE
    /// winding. It is ONE geometric corner and must be reported once.
    #[test]
    fn a_corner_shared_by_two_cycles_is_reported_once() {
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [2.0, 0.0, 0.0]),
        ]);
        let post = vec![[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        let fwd: Vec<u32> = vec![0, 1, 2];
        let rev: Vec<u32> = vec![2, 1, 0];
        let one = classify_folds([fwd.as_slice()], &pre, &post);
        let both = classify_folds([fwd.as_slice(), rev.as_slice()], &pre, &post);
        assert_eq!(
            one.len(),
            both.len(),
            "the reversed traversal must not double-count: {both:?}"
        );
        // And the turn is genuinely invariant under the reversal.
        let a = one.iter().find(|f| f.vertex == 1).unwrap();
        let b = both.iter().find(|f| f.vertex == 1).unwrap();
        assert_eq!(a.turn_pre_deg, b.turn_pre_deg);
        assert_eq!(a.turn_post_deg, b.turn_post_deg);
    }

    /// The chord test has NO short-cycle degeneracy: an ordinary triangle
    /// corner is not an inversion, where the 120-degree turn test flags two of
    /// its three corners.
    #[test]
    fn chord_test_is_clean_on_the_triangle_that_defeats_the_turn_test() {
        let pts = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 1.0, 0.0]];
        let cyc: Vec<u32> = vec![0, 1, 2];
        let pre: HashMap<u32, [f64; 3]> = pts
            .iter()
            .enumerate()
            .map(|(i, &p)| (i as u32, p))
            .collect();
        // The turn test reads two of three corners as folds on this triangle.
        let turns = classify_folds([cyc.as_slice()], &pre, &pts);
        assert_eq!(
            turns.iter().filter(|c| c.class != FoldClass::None).count(),
            2
        );
        // The chord test reads none — nothing moved, nothing inverted.
        assert!(chord_order_inversions([cyc.as_slice()], &pre, &pts).is_empty());
    }

    /// And it DOES catch the F0067 signature: a vertex driven past its
    /// neighbour lands outside `[0, 1]` on the chord.
    #[test]
    fn chord_test_catches_a_vertex_driven_past_its_neighbour() {
        let cyc: Vec<u32> = vec![0, 1, 2];
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [2.0, 0.0, 0.0]),
        ]);
        // v1 driven out past v2's far side.
        let post = vec![[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        let inv = chord_order_inversions([cyc.as_slice()], &pre, &post);
        assert!(inv.contains(&1), "{inv:?}");
        // The anchored certificate direction too: past the OTHER neighbour.
        let post2 = vec![[0.0, 0.0, 0.0], [-1.2, 0.0, 0.0], [2.0, 0.0, 0.0]];
        assert!(chord_order_inversions([cyc.as_slice()], &pre, &post2).contains(&1));
    }

    #[test]
    fn chord_param_is_none_on_a_degenerate_chord() {
        assert!(chord_param([1.0, 0.0, 0.0], [0.5, 1.0, 0.0], [1.0, 0.0, 0.0]).is_none());
        let t = chord_param([0.0, 0.0, 0.0], [0.25, 9.0, 0.0], [1.0, 0.0, 0.0]).unwrap();
        assert!(
            (t - 0.25).abs() < 1e-15,
            "projection is along the chord: {t}"
        );
    }

    /// The whole point: the merge arm's customer set is the INTERSECTION.
    #[test]
    fn merge_customers_is_ratio_AND_minted_fold() {
        let cyc: Vec<u32> = vec![0, 1, 2];
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [2.0, 0.0, 0.0]),
        ]);
        let post = vec![[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        let risks = rank_fold_risks(&pre, &post, &cycle_adjacency([cyc.as_slice()], &e(&[])));
        let folds = classify_folds([cyc.as_slice()], &pre, &post);
        let cust = merge_customers(&risks, &folds);
        assert_eq!(cust.len(), 1);
        assert_eq!(cust[0].vertex, 1);

        // A big ratio with NO fold is excluded — this is the R0085 over-select.
        let pre2 = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [1.001, 0.0, 0.0]),
        ]);
        let post2 = vec![[0.0, 0.0, 0.0], [1.0, 0.05, 0.0], [1.001, 0.0, 0.0]];
        let r2 = rank_fold_risks(&pre2, &post2, &cycle_adjacency([cyc.as_slice()], &e(&[])));
        let f2 = classify_folds([cyc.as_slice()], &pre2, &post2);
        assert!(!minting_risks(&r2).is_empty(), "ratio alone selects it");
        assert!(
            merge_customers(&r2, &f2).is_empty(),
            "but it never folded, so it is NOT a merge customer"
        );
    }

    #[test]
    fn degenerate_inputs_are_skipped_not_scored() {
        // Self-loop edge, and a zero-length pre spacing (coincident pair).
        let pre = m(&[(0, [0.0, 0.0, 0.0]), (1, [0.0, 0.0, 0.0])]);
        let post = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        assert!(rank_fold_risks(&pre, &post, &e(&[(1, 1), (0, 1)])).is_empty());
    }
}
