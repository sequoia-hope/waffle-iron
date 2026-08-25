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
//! > denominators, so "845 minting" is not 845 defects — it is 845 vertices whose
//! > displacement exceeds their tightest cycle spacing, most of which never
//! > folded. Widening also makes `min_pre_spacing` a minimum over a much larger
//! > set, so a single sub-resolution near-duplicate neighbour (the pipeline
//! > collapses these later anyway) drives the ratio for everything around it.
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
//! instead of shipping wrong geometry. The arm is kept, gated off, as the record
//! of what a bare collapse does.
//!
//! # RESOLVED 2026-08-19d — [`fold_merge_sites`] + `rebuild_merge_fan` (§4-I6)
//!
//! Two things had to change, and the 08-05 note named only one of them.
//!
//! **The repair primitive** is now a LOCAL re-triangulation
//! (`stage4_construct::rebuild_merge_fan`): the triangles at the victim are
//! DISCARDED and its link polygon re-CDT'd in the patch chart. Not
//! `collapse_vertex` (this note's finding), and not the whole-patch
//! `rebuild_patch_planar` either — measured 2026-08-19, that declines every
//! merge in the ring-reject family, on `ThetaUnwrap` where the merge is on the
//! rim of an ENCIRCLING lateral and on `TriangulationFailed` where the patch
//! still carries other folds. A fan spans a small θ window and one local
//! polygon, so it has neither precondition.
//!
//! **The selector** also had to change, and that is the part this note did not
//! anticipate. [`merge_customers_chord`] ranges over MOVED vertices, but in the
//! dominant class the fold apex is the vertex that did NOT move — it is the
//! plain rim vertex a relocation stepped OVER. Measured on F0045:
//! `CHORD_CUSTOMERS=0` while the loop is `class=MINTED_BY_S4`. [`fold_merge_sites`]
//! ranges over apexes instead and picks the survivor by the SIGN of the chord
//! parameter. F0045 and R0090 convert; the corpus moves 263C → 265C with no
//! other delta.

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

/// One Yang §4.4.1 **Fig-11(b)→(c)** merge site on a patch boundary cycle: a
/// plain mesh vertex the Stage-4 relocation OVERRAN.
///
/// Fig 11's `q` is "an intersection point on the boundary curve" and `p` is the
/// endpoint of the constrained edge containing it; when `p` is too close, "we
/// merge `p` with `q`". This is that configuration reached from the other
/// direction: Stage 4 relocates `q` onto its exact analytic position and the
/// displacement carries it PAST `p` along the same input-edge chain, so the
/// kept patch's boundary walks out to `q` and back over `p`.
///
/// Measured signature (2026-08-19 ring-reject census, F0045 face 0): the turn
/// at `p` goes `27.69° → 167.34°` when its neighbour moves `2.382e-2` across a
/// `1.283e-2` pre-spacing, and `p`'s own residual on both its surfaces is
/// exactly 0 — a healthy discretization vertex on the wrong side of a refined
/// curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoldMergeSite {
    /// Fig-11's `p`: the overrun mesh vertex. Merged away — it carries no
    /// analytic certificate (it was never relocated).
    pub victim: u32,
    /// Fig-11's `q`: the relocated neighbour that overran it. SURVIVES, because
    /// it is the one holding Stage 4's exact curve position.
    pub survivor: u32,
    /// `chord_param(prev, victim, next)` at the POST positions. `< 0` ⇒ the
    /// victim lies past `prev`; `> 1` ⇒ past `next`. The survivor is whichever
    /// end was overrun, so the sign PICKS it — no distance tie-break.
    pub chord_t: f64,
}

/// Select every [`FoldMergeSite`] over a patch-cycle set — the threshold-free
/// selector for the §4.4.1 Fig-11 merge.
///
/// A corner `(a, b, c)` qualifies iff ALL of:
/// 1. `b` did NOT move across Stage 4 (`post == pre`) — a relocated vertex sits
///    on an exact analytic curve and is never merged away (that would discard
///    the certificate), and a Stage-4-MINTED vertex (no `pre` entry) has no
///    "was it overrun" question to ask;
/// 2. `b` sat INSIDE the chord of its own neighbours before and lies OUTSIDE it
///    after — [`chord_order_inversions`]' certificate, which is exactly the
///    `class=MINTED_BY_S4` verdict the loop-simplicity census reports. A
///    sign/interval test on a ratio: no band, no angle, scale-free;
/// 3. the END it overran (`a` when `t < 0`, `c` when `t > 1`) DID move — that
///    relocation is what carried the boundary across `b`. An inversion between
///    two unmoved vertices is inherited geometry and is left alone.
///
/// `pre` is the `S4_PRE_POS` map. It — not the `relocations` vector — is the
/// correct "was it relocated?" oracle: `relocations` carries conic `(vertex, t)`
/// retags only, so the implicit-pair and junction arms move vertices without
/// appearing in it. Measured 2026-08-19: on R0074/R0085/R0095/R0025 the vector
/// is EMPTY while 59–83 vertices per loop moved, so a `relocations`-keyed
/// condition 3 rejects every inversion in the family.
///
/// Ambiguity is dropped, never guessed: a victim claimed by two different
/// survivors is excluded. Deterministic — `BTreeMap` iteration only.
/// I13c — the on-curve TERMINAL-overrun arm of the Fig-11 merge selector
/// (and the driver's construct/fold-merge alternation). **FLIPPED ALWAYS-ON
/// 2026-08-25** with the I13 corpus proofs (see
/// `stage4_project::cone_chart_enabled`). `YANG_441_ONCURVE_MERGE=0|off` is
/// the dev A/B off-knob.
pub(crate) fn oncurve_merge_enabled() -> bool {
    !matches!(std::env::var("YANG_441_ONCURVE_MERGE"), Ok(v) if v == "0" || v == "off")
}

/// I13d — the run-level junction-absorption arm (`YANG_441_RUN_ABSORB`).
/// **FLIPPED ALWAYS-ON 2026-08-25** (same day as landing) with the corpus
/// proofs: gate-off default corpus BIT-IDENTICAL to the committed baseline;
/// gate-on corpus CATEGORY-IDENTICAL — 271C/0W/36E/1EE/0T with exactly ONE
/// explained detail row (R0003 advances face 467 → 517, the I13e
/// interlocked-pair wall). `YANG_441_RUN_ABSORB=0|off` is the dev A/B
/// off-knob; `census` selects and reports at the fold-merge fixed points
/// without applying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunAbsorbMode {
    Off,
    Census,
    On,
}

pub(crate) fn run_absorb_mode() -> RunAbsorbMode {
    match std::env::var("YANG_441_RUN_ABSORB") {
        Err(_) => RunAbsorbMode::On,
        Ok(v) if v == "0" || v == "off" => RunAbsorbMode::Off,
        Ok(v) if v == "census" => RunAbsorbMode::Census,
        Ok(_) => RunAbsorbMode::On,
    }
}

/// I13c certificate: is corner `i` of `cyc` (post-inversion `t` already
/// established by the caller) a TERMINAL overrun on its own intersection
/// curve? Yes iff:
///
/// 1. both corner edges carry the SAME curve (exact or up to the stored
///    normal's sign);
/// 2. the end the apex crossed (`t < 0` ⇒ prev, `t > 1` ⇒ next) is the seam
///    run's TERMINAL — its far-side cycle edge does not carry that curve
///    (the run ends there: a junction shared with other boundary chains);
/// 3. the curve parameters of (other end → survivor → apex) at the POST
///    positions are strictly monotone — the survivor lies strictly BETWEEN
///    its run neighbour and the apex on the curve, i.e. the apex's
///    relocation carried it past the terminal. Periodic params compare via
///    (−π, π]-wrapped deltas (a chain chord subtends < π — the standing
///    convention); open-conic params (I13b) compare raw.
///
/// Returns the survivor. No distance band anywhere (P10) — the certificate
/// is the parameter order itself.
fn oncurve_terminal_overrun(
    curves: &BTreeMap<(u32, u32), crate::Curve>,
    cyc: &[u32],
    i: usize,
    t: f64,
    post: &[[f64; 3]],
) -> Option<u32> {
    use crate::stage4_correct::{
        conic_param, conic_param_periodic, conics_equal_up_to_normal_sign,
    };
    let n = cyc.len();
    let (a, b, c) = (cyc[(i + n - 1) % n], cyc[i], cyc[(i + 1) % n]);
    let key = |x: u32, y: u32| (x.min(y), x.max(y));
    let cab = curves.get(&key(a, b))?;
    let cbc = curves.get(&key(b, c))?;
    let same = |x: &crate::Curve, y: &crate::Curve| x == y || conics_equal_up_to_normal_sign(x, y);
    if !same(cab, cbc) {
        return None;
    }
    let (survivor, other, far) = if t < 0.0 {
        (a, c, cyc[(i + n - 2) % n])
    } else {
        (c, a, cyc[(i + 2) % n])
    };
    if survivor == far || survivor == other || far == b {
        return None; // degenerate cycle neighbourhood — not a run shape
    }
    if let Some(cf) = curves.get(&key(survivor, far)) {
        if same(cf, cab) {
            return None; // run continues past the crossed end — not a terminal
        }
    }
    let param = |v: u32| conic_param(cab, cad_primitives::Point3::from(*post.get(v as usize)?));
    let (t_o, t_s, t_b) = (param(other)?, param(survivor)?, param(b)?);
    let (d1, d2) = if conic_param_periodic(cab) {
        let wrap = |mut d: f64| -> f64 {
            while d > std::f64::consts::PI {
                d -= 2.0 * std::f64::consts::PI;
            }
            while d <= -std::f64::consts::PI {
                d += 2.0 * std::f64::consts::PI;
            }
            d
        };
        (wrap(t_s - t_o), wrap(t_b - t_s))
    } else {
        (t_s - t_o, t_b - t_s)
    };
    if !d1.is_finite() || !d2.is_finite() || d1 == 0.0 || d2 == 0.0 {
        return None;
    }
    (d1 * d2 > 0.0).then_some(survivor)
}

pub fn fold_merge_sites<'a>(
    cycles: impl IntoIterator<Item = &'a [u32]>,
    pre: &HashMap<u32, [f64; 3]>,
    post: &[[f64; 3]],
) -> Vec<FoldMergeSite> {
    fold_merge_sites_censused(cycles, pre, post, &BTreeMap::new()).0
}

/// Per-condition rejection counts for [`fold_merge_sites`] — the selector's own
/// coverage ledger, so "found nothing" is always attributable to a CONDITION
/// rather than inferred. Reported by the driver under `YANG_441_VERBOSE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FoldMergeCensus {
    /// Corners examined (degenerate ones, and ones with no pre position for all
    /// three vertices, excluded).
    pub corners: usize,
    /// Corners whose chord order Stage 4 inverted (condition 2).
    pub inversions: usize,
    /// Inversions rejected because the apex itself MOVED (condition 1) — two
    /// on-curve vertices crossed each other, which is chain ORDER (§4.3.4
    /// `ReorderConic`), not Fig-11.
    pub apex_moved: usize,
    /// Inversions rejected because the apex was MINTED during Stage 4 (no `pre`
    /// entry at all — e.g. an appended §4.3.4 on-curve sample). Counted apart
    /// from [`Self::apex_moved`] because the two are different populations with
    /// different owners, and collapsing them into one counter would attribute
    /// the residue to chain order without having measured it. **Measured
    /// 2026-08-19d over the ring-reject family: 0 in every case** — every
    /// rejected inversion has an apex that genuinely moved.
    pub apex_minted: usize,
    /// Of [`Self::apex_moved`], how many sit on an INTERSECTION-CURVE chain
    /// (both incident cycle edges are curve edges). Those are two on-curve
    /// vertices that crossed each other — chain ORDER, owned by §4.3.4's
    /// `ReorderConic`. The remainder are relocated vertices that crossed a
    /// neighbour on a PLAIN boundary, which is neither that nor Fig-11, and is
    /// the class with no owner yet.
    pub apex_moved_on_curve: usize,
    /// Inversions rejected because the overrun end never moved (condition 3) —
    /// including the case where NOTHING at the corner moved, i.e. the crossing
    /// was minted by a relocation elsewhere on the loop.
    pub survivor_still: usize,
    /// Victims dropped as ambiguous (two survivors claim them).
    pub ambiguous: usize,
    /// I13c (gated `YANG_441_ONCURVE_MERGE`): of
    /// [`Self::apex_moved_on_curve`], corners certified as a TERMINAL
    /// overrun on their shared curve — the apex's relocation carried it past
    /// the seam run's terminal junction in curve parameter — and proposed as
    /// merge sites (victim = apex, survivor = the junction). Zero when the
    /// gate is off.
    pub oncurve_sites: usize,
}

/// As [`fold_merge_sites`], with the per-condition rejection census.
/// `curves` is the `intersection_curves` map, keys canonicalized `(min, max)`.
/// The KEYS drive the census's on/off-curve split as before; the VALUES feed
/// only the gated I13c terminal-overrun arm (`YANG_441_ONCURVE_MERGE`) —
/// with the gate off, selection is byte-identical to the key-set-only
/// selector. May be empty.
pub fn fold_merge_sites_censused<'a>(
    cycles: impl IntoIterator<Item = &'a [u32]>,
    pre: &HashMap<u32, [f64; 3]>,
    post: &[[f64; 3]],
    curves: &BTreeMap<(u32, u32), crate::Curve>,
) -> (Vec<FoldMergeSite>, FoldMergeCensus) {
    let mut claimed: BTreeMap<u32, (u32, f64)> = BTreeMap::new();
    let mut ambiguous: BTreeSet<u32> = BTreeSet::new();
    let mut census = FoldMergeCensus::default();
    let moved = |v: u32| -> Option<bool> { Some(*pre.get(&v)? != *post.get(v as usize)?) };
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
            census.corners += 1;
            let (Some(t_pre), Some(t)) = (chord_param(pa, pb, pc), chord_param(qa, qb, qc)) else {
                continue;
            };
            if !(0.0..=1.0).contains(&t_pre) || (0.0..=1.0).contains(&t) {
                continue;
            }
            census.inversions += 1;
            match moved(b) {
                Some(false) => {}
                Some(true) => {
                    census.apex_moved += 1;
                    let on_curve = |x: u32, y: u32| curves.contains_key(&(x.min(y), x.max(y)));
                    let oc = on_curve(a, b) && on_curve(b, c);
                    if oc {
                        census.apex_moved_on_curve += 1;
                        // I13c (gated): the TERMINAL-overrun arm. The apex
                        // moved — the still-apex Fig-11 conditions cannot
                        // hold — but when its relocation carried it past the
                        // seam run's TERMINAL junction in curve parameter,
                        // the same Fig-11 merge (victim = apex, survivor =
                        // the junction) is the paper's repair; the reorder
                        // authority refuses this shape by its endpoint
                        // guard. Shares `claimed` so cross-arm duplicates
                        // and ambiguity resolve in one place.
                        if oncurve_merge_enabled() {
                            if let Some(s) = oncurve_terminal_overrun(curves, cyc, i, t, post) {
                                census.oncurve_sites += 1;
                                match claimed.get(&b) {
                                    Some(&(s0, _)) if s0 != s => {
                                        ambiguous.insert(b);
                                    }
                                    Some(_) => {}
                                    None => {
                                        claimed.insert(b, (s, t));
                                    }
                                }
                            }
                        }
                        // Per-corner detail for the ON-CURVE arm (2026-08-25,
                        // R0003 face-437 rim×cut boundary-hook census): two
                        // on-curve vertices whose chain order Stage 4 inverted.
                        // What decides its treatment is WHERE on the shared
                        // curve the apex landed relative to the end it overran
                        // — an apex carried past a run-TERMINAL junction is
                        // Fig-11's p/q (the reorder authority refuses endpoint
                        // re-roots by design); an interior×interior crossing is
                        // §4.3.4 chain order. Ids + pre/post positions let the
                        // offline curve fit assign the parameter certificate.
                        if std::env::var_os("YANG_441_FOLD_CENSUS").is_some() {
                            let over = if t < 0.0 { a } else { c };
                            eprintln!(
                                "YANG_441_FOLD_ONCURVE apex={b} over={over} other={} t={t:.4} \
                                 pre_a={pa:?} pre_b={pb:?} pre_c={pc:?} \
                                 post_a={qa:?} post_b={qb:?} post_c={qc:?}",
                                if t < 0.0 { c } else { a },
                            );
                        }
                    }
                    // Per-corner detail for the OFF-CURVE arm (the class with no
                    // owner as of 2026-08-19d). What decides its treatment is
                    // how far the apex ended up from the neighbour it overran,
                    // measured against the corner's own edge lengths — a
                    // near-duplicate pair is a Fig-11 merge with a
                    // richer-certificate survivor; a far one is not.
                    if !oc && std::env::var_os("YANG_441_FOLD_CENSUS").is_some() {
                        let over = if t < 0.0 { a } else { c };
                        let d = |x: [f64; 3], y: [f64; 3]| {
                            ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2))
                                .sqrt()
                        };
                        let qo = if t < 0.0 { qa } else { qc };
                        let disp_b = d(pb, qb);
                        let disp_o = d(if t < 0.0 { pa } else { pc }, qo);
                        eprintln!(
                            "YANG_441_FOLD_OFFCURVE apex={b} over={over} t={t:.4}                              gap={:.4e} seg_ab={:.4e} seg_bc={:.4e} disp_apex={disp_b:.4e}                              disp_over={disp_o:.4e} edge_ab_curve={} edge_bc_curve={}",
                            d(qb, qo),
                            d(qa, qb),
                            d(qb, qc),
                            on_curve(a, b),
                            on_curve(b, c),
                        );
                    }
                    continue;
                }
                None => {
                    census.apex_minted += 1;
                    continue;
                }
            }
            let survivor = if t < 0.0 { a } else { c };
            if moved(survivor) != Some(true) {
                census.survivor_still += 1;
                continue;
            }
            match claimed.get(&b) {
                Some(&(s0, _)) if s0 != survivor => {
                    ambiguous.insert(b);
                }
                Some(_) => {}
                None => {
                    claimed.insert(b, (survivor, t));
                }
            }
        }
    }
    for b in &ambiguous {
        claimed.remove(b);
    }
    census.ambiguous = ambiguous.len();
    // No chained-substitution filter is needed: survivors moved and victims did
    // not, so the two sets are DISJOINT by conditions 1 and 3 — a victim can
    // never also be some other site's survivor.
    let sites = claimed
        .into_iter()
        .map(|(victim, (survivor, chord_t))| FoldMergeSite {
            victim,
            survivor,
            chord_t,
        })
        .collect();
    (sites, census)
}

/// I13d (spec `yang_441_trim_cdt_construction.md` §I13(c)) — one **run-level
/// junction absorption** site: a maximal same-curve boundary run whose
/// Stage-4 relocation carried a junction-adjacent prefix of vertices PAST the
/// run's junction terminal, in curve parameter. Every out-of-band prefix
/// vertex merges into the junction (Fig-11's p→q at run granularity).
///
/// The corner-level I13c arm is structurally blind to this family (measured,
/// R0003 face 467): the only chord-inverted corner's chord-sign survivor is
/// the NEXT run vertex — itself out-of-band — and its far-side edge continues
/// on the same curve, so the terminal condition refuses. The run vantage sees
/// what the corner cannot: the junction bounding the run, and each vertex's
/// pre→post side of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunAbsorptionSite {
    /// The junction terminal — carries strictly more surfaces than every
    /// victim (a model junction), so the merge discards no authority (I8).
    pub survivor: u32,
    /// The out-of-band run vertices, junction-nearest first. Each one's curve
    /// parameter crossed the junction's between its pre and post positions.
    pub victims: Vec<u32>,
}

/// Per-condition coverage ledger for [`run_absorption_sites`] — every
/// refused candidate is attributable to a CONDITION, mirroring
/// [`FoldMergeCensus`]'s discipline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunAbsorptionCensus {
    /// Maximal same-curve runs examined (≥1 typed edge, both ends bounded).
    pub runs: usize,
    /// Terminal ends walked (two per run unless the cycle is fully typed).
    pub terminals: usize,
    /// Terminals refused: the junction's own curve parameter is undefined.
    pub no_param: usize,
    /// Terminals refused: no vertex flipped sides of the junction (the
    /// healthy-chain verdict — pre and post both in-band).
    pub no_flip: usize,
    /// Terminals refused: a flipped prefix exists but no MINTED chord
    /// inversion sits on it (pre inside its neighbours' chord, post outside)
    /// — the doubling-back witness Stage 4's own relocation must supply.
    pub no_inversion: usize,
    /// Terminals refused: the junction is not strictly richer than every
    /// flipped vertex (missing carrier ⇒ not a model junction ⇒ absorbing
    /// would discard authority).
    pub not_richer: usize,
    /// Victims dropped because two sites with different survivors claim them.
    pub ambiguous: usize,
    /// Sites emitted (after cross-cycle dedupe).
    pub sites: usize,
}

/// I13d selection — the run-level junction-absorption sites over a
/// patch-cycle set.
///
/// For each maximal run of consecutive cycle edges typed on the SAME curve
/// (identity up to the stored normal's sign), each bounded end `J` is a
/// candidate junction terminal. Walking outward from `J`, a vertex `w` is
/// out-of-band iff Stage 4's relocation INVERTED the order of the pair
/// `(w, J)` along the curve — strict opposite signs of
/// `t_pre(w) − t_pre(J)` and `t_post(w) − t_post(J)` (wrapped to (−π, π]
/// for periodic conics, raw for open ones; the standing chord-subtends-<π
/// convention). The test is symmetric in WHICH endpoint moved — measured
/// both ways on R0003: a spur vertex carried past a solved junction (the
/// face-437 shape), and a junction solved 0.67 PAST its first two chain
/// samples (face 467: v2332 hops v2331/v2330 on their shared hyperbola,
/// which the samples' own one-sided view calls healthy). The maximal
/// inverted prefix is the victim set. A site is emitted iff:
///
/// 1. the prefix is nonempty (the pair order inverted for someone);
/// 2. at least one prefix vertex is a MINTED chord inversion in its cycle
///    corner (pre inside `[0,1]`, post outside) — the doubling-back
///    witness. Load-bearing, not decorative: the junction's own large
///    relocation can invert its pre/post pair order against the NEIGHBOUR
///    chain's in-domain samples too (projection artifacts of the drifted
///    pre position), but an in-domain chain stays post-monotone and can
///    have no minted fold at its samples, so this witness structurally
///    refuses that side;
/// 3. `strictly_richer(J, w)` holds for every victim `w` — the caller's
///    carrier oracle: `carried(w) ⊂ carried(J)` proper, the I8 containment
///    plus junction-hood.
///
/// No distance band anywhere (P10): every test is an order or containment
/// certificate. Sites found from two cycles (each boundary chain appears in
/// both adjacent patches' cycles) dedupe by value; a victim claimed by two
/// different survivors drops BOTH sites, loudly, never guessed.
pub fn run_absorption_sites<'a>(
    cycles: impl IntoIterator<Item = &'a [u32]>,
    pre: &HashMap<u32, [f64; 3]>,
    post: &[[f64; 3]],
    curves: &BTreeMap<(u32, u32), crate::Curve>,
    strictly_richer: impl Fn(u32, u32) -> bool,
) -> (Vec<RunAbsorptionSite>, RunAbsorptionCensus) {
    use crate::stage4_correct::{
        conic_param, conic_param_periodic, conics_equal_up_to_normal_sign,
    };
    let mut census = RunAbsorptionCensus::default();
    let mut sites: Vec<RunAbsorptionSite> = Vec::new();
    let same = |x: &crate::Curve, y: &crate::Curve| x == y || conics_equal_up_to_normal_sign(x, y);
    let wrap = |mut d: f64| -> f64 {
        while d > std::f64::consts::PI {
            d -= 2.0 * std::f64::consts::PI;
        }
        while d <= -std::f64::consts::PI {
            d += 2.0 * std::f64::consts::PI;
        }
        d
    };
    for cyc in cycles {
        let n = cyc.len();
        if n < 3 {
            continue;
        }
        let key = |x: u32, y: u32| (x.min(y), x.max(y));
        let edge_curve =
            |i: usize| -> Option<&crate::Curve> { curves.get(&key(cyc[i], cyc[(i + 1) % n])) };
        // Maximal same-curve runs of edges. A run STARTS at a typed edge whose
        // predecessor does not continue its curve; a cycle typed end-to-end on
        // ONE curve has no start and is skipped — that is a closed seam, the
        // reorder authority's shape, with no bounded end to absorb into.
        for i in 0..n {
            let Some(c0) = edge_curve(i) else {
                continue;
            };
            if edge_curve((i + n - 1) % n).is_some_and(|c| same(c, c0)) {
                continue; // mid-run: its start is found at its own index
            }
            let mut len = 1usize;
            while len < n {
                match edge_curve((i + len) % n) {
                    Some(c) if same(c, c0) => len += 1,
                    _ => break,
                }
            }
            census.runs += 1;
            // Run vertices w_0..w_len (len edges): cyc[i..=i+len] mod n.
            let w = |k: usize| cyc[(i + k) % n];
            for (jk, dir) in [(0usize, 1i64), (len, -1i64)] {
                census.terminals += 1;
                let j = w(jk);
                let param = |p: [f64; 3]| conic_param(c0, cad_primitives::Point3::from(p));
                let (Some(t_j), Some(t_j_pre)) = (
                    post.get(j as usize).copied().and_then(param),
                    pre.get(&j).copied().and_then(param),
                ) else {
                    census.no_param += 1;
                    continue;
                };
                let sided = |t: f64, tref: f64| -> f64 {
                    if conic_param_periodic(c0) {
                        wrap(t - tref)
                    } else {
                        t - tref
                    }
                };
                // Maximal ORDER-INVERTED prefix walking outward from the
                // junction: the pre→post relocation swapped which side of the
                // junction the vertex lies on, in curve parameter. Symmetric
                // in which endpoint moved — measured both ways on R0003:
                // face 437's spur vertex carried past a solved junction, and
                // face 467's junction solved 0.67 PAST its first two chain
                // samples (v2332 hops v2331/v2330 on their shared hyperbola).
                let mut victims: Vec<u32> = Vec::new();
                let mut k = jk as i64 + dir;
                while (0..=len as i64).contains(&k) {
                    let v = w(k as usize);
                    if v == j {
                        break;
                    }
                    let flipped = (|| -> Option<bool> {
                        let tq = param(*post.get(v as usize)?)?;
                        let tp = param(*pre.get(&v)?)?;
                        let (dq, dp) = (sided(tq, t_j), sided(tp, t_j_pre));
                        Some(dq != 0.0 && dp != 0.0 && (dq > 0.0) != (dp > 0.0))
                    })();
                    if flipped != Some(true) {
                        break;
                    }
                    victims.push(v);
                    k += dir;
                }
                if victims.is_empty() {
                    census.no_flip += 1;
                    continue;
                }
                // Doubling-back witness: a MINTED chord inversion whose apex
                // is a victim, in this cycle's corner context.
                let minted_inversion = |v: u32| -> bool {
                    let Some(iv) = (0..n).find(|&x| cyc[x] == v) else {
                        return false;
                    };
                    let (a, b, c) = (cyc[(iv + n - 1) % n], cyc[iv], cyc[(iv + 1) % n]);
                    if a == b || b == c || a == c {
                        return false;
                    }
                    let (Some(&pa), Some(&pb), Some(&pc)) = (pre.get(&a), pre.get(&b), pre.get(&c))
                    else {
                        return false;
                    };
                    let (Some(&qa), Some(&qb), Some(&qc)) = (
                        post.get(a as usize),
                        post.get(b as usize),
                        post.get(c as usize),
                    ) else {
                        return false;
                    };
                    let (Some(tp), Some(tq)) = (chord_param(pa, pb, pc), chord_param(qa, qb, qc))
                    else {
                        return false;
                    };
                    (0.0..=1.0).contains(&tp) && !(0.0..=1.0).contains(&tq)
                };
                if !victims.iter().any(|&v| minted_inversion(v)) {
                    census.no_inversion += 1;
                    continue;
                }
                if !victims.iter().all(|&v| strictly_richer(j, v)) {
                    census.not_richer += 1;
                    continue;
                }
                let site = RunAbsorptionSite {
                    survivor: j,
                    victims,
                };
                if !sites.contains(&site) {
                    sites.push(site);
                }
            }
        }
    }
    // A victim claimed by two different survivors: drop every site touching
    // it — ambiguity is refused, never guessed.
    let mut owner: BTreeMap<u32, u32> = BTreeMap::new();
    let mut poisoned: BTreeSet<u32> = BTreeSet::new();
    for s in &sites {
        for &v in &s.victims {
            match owner.get(&v) {
                Some(&j0) if j0 != s.survivor => {
                    poisoned.insert(v);
                }
                _ => {
                    owner.insert(v, s.survivor);
                }
            }
        }
    }
    census.ambiguous = poisoned.len();
    sites.retain(|s| s.victims.iter().all(|v| !poisoned.contains(v)));
    census.sites = sites.len();
    (sites, census)
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
    #[allow(non_snake_case)] // the CAPS verdict is the test's claim
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
    #[allow(non_snake_case)] // the CAPS verdict is the test's claim
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
    #[allow(non_snake_case)] // the CAPS verdict is the test's claim
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

    // ---- Fig-11 merge-site selector (§4.4.1) ---------------------------

    /// The F0045 witness in miniature: a rim chain `q, p, r` whose junction `q`
    /// was relocated PAST `p`. `p` is still, so `merge_customers_chord` (which
    /// ranges over MOVED vertices) cannot see it; this selector must.
    #[test]
    fn fold_merge_site_picks_the_overrun_still_vertex() {
        // Cycle: 0 (relocated junction), 1 (still rim vertex), 2, 3.
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, [1.4, 0.0, 0.0]), // 0 sat BEYOND 1 — chain order 0,1,2
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
        ]);
        let post = vec![
            [1.0, 0.0, 0.0], // 0 — relocated, landed on the FAR side of 1
            [1.2, 0.0, 0.0], // 1 — the overrun still vertex
            [0.0, 0.0, 0.0],
            [0.5, -1.0, 0.0],
        ];
        let sites = fold_merge_sites([cyc.as_slice()], &pre, &post);
        assert_eq!(sites.len(), 1, "exactly the overrun vertex: {sites:?}");
        assert_eq!(sites[0].victim, 1);
        assert_eq!(sites[0].survivor, 0, "the RELOCATED end survives");
        assert!(sites[0].chord_t < 0.0, "victim lies past `prev`");
    }

    /// The same overshoot from the other side: the relocated vertex is the NEXT
    /// neighbour, so the sign flips and it is still the survivor.
    #[test]
    fn fold_merge_site_picks_the_relocated_next_neighbour() {
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [1.4, 0.0, 0.0]), // 2 sat beyond 1
            (3, [0.5, -1.0, 0.0]),
        ]);
        let post = vec![
            [0.0, 0.0, 0.0],
            [1.2, 0.0, 0.0], // 1 — overrun
            [1.0, 0.0, 0.0], // 2 — relocated, landed BEFORE 1
            [0.5, -1.0, 0.0],
        ];
        let sites = fold_merge_sites([cyc.as_slice()], &pre, &post);
        assert_eq!(sites.len(), 1, "{sites:?}");
        assert_eq!((sites[0].victim, sites[0].survivor), (1, 2));
        assert!(sites[0].chord_t > 1.0, "victim lies past `next`");
    }

    // ---- I13c on-curve terminal-overrun certificate --------------------

    /// Circle standing in for the seam conic (its `conic_param` is ungated);
    /// positions minted via `conic_eval` at chosen parameters so the ORDER is
    /// exact by construction.
    fn unit_circle() -> crate::Curve {
        crate::Curve::Circle {
            center: cad_primitives::Point3::new(0.0, 0.0, 0.0),
            normal: crate::Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        }
    }

    fn on_circle(t: f64) -> [f64; 3] {
        crate::geom::conic_eval(&unit_circle(), t)
            .expect("circle eval")
            .as_array()
    }

    fn curve_map(edges: &[(u32, u32)]) -> BTreeMap<(u32, u32), crate::Curve> {
        edges
            .iter()
            .map(|&(x, y)| ((x.min(y), x.max(y)), unit_circle()))
            .collect()
    }

    /// The R0003 face-437 shape in miniature: cycle run `1 → 2 → 3` on one
    /// curve, the run TERMINAL at 3 (edge (3,4) is off-curve), and the apex
    /// 2's post position BEYOND 3 in curve parameter. The certificate fires
    /// and names 3 the survivor.
    #[test]
    fn oncurve_terminal_overrun_certifies_the_r0003_shape() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let post = vec![
            [5.0, 5.0, 0.0], // 0 — far, not involved
            on_circle(0.10), // 1 = other end of the corner
            on_circle(0.28), // 2 = apex, past the terminal
            on_circle(0.20), // 3 = survivor (terminal junction)
            [6.0, 6.0, 0.0], // 4 — off-curve continuation
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let got = oncurve_terminal_overrun(&curves, &cyc, 2, 5.0, &post);
        assert_eq!(got, Some(3));
    }

    /// If the crossed end's far-side edge carries the SAME curve, the run
    /// continues past it — it is not a terminal, and the corner belongs to
    /// chain-order (ReorderConic) territory, not the merge.
    #[test]
    fn oncurve_terminal_overrun_declines_when_the_run_continues() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let post = vec![
            [5.0, 5.0, 0.0],
            on_circle(0.10),
            on_circle(0.28),
            on_circle(0.20),
            on_circle(0.35), // 4 — the run continues on the same curve
        ];
        let curves = curve_map(&[(1, 2), (2, 3), (3, 4)]);
        assert_eq!(oncurve_terminal_overrun(&curves, &cyc, 2, 5.0, &post), None);
    }

    /// A curve-parameter-monotone corner is healthy chain order regardless of
    /// what the 3D chord test claimed — the certificate is the parameter
    /// order, and it must decline.
    #[test]
    fn oncurve_terminal_overrun_declines_a_parameter_monotone_corner() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let post = vec![
            [5.0, 5.0, 0.0],
            on_circle(0.10),
            on_circle(0.15), // apex BETWEEN its ends on the curve
            on_circle(0.20),
            [6.0, 6.0, 0.0],
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        assert_eq!(oncurve_terminal_overrun(&curves, &cyc, 2, 5.0, &post), None);
    }

    /// Two DIFFERENT conics meeting at the apex is a junction, not a run —
    /// no shared parameter, no certificate.
    #[test]
    fn oncurve_terminal_overrun_declines_mismatched_curves() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let post = vec![
            [5.0, 5.0, 0.0],
            on_circle(0.10),
            on_circle(0.28),
            on_circle(0.20),
            [6.0, 6.0, 0.0],
        ];
        let mut curves = curve_map(&[(1, 2)]);
        curves.insert(
            (2, 3),
            crate::Curve::Circle {
                center: cad_primitives::Point3::new(0.0, 0.0, 0.0),
                normal: crate::Vector3::new(0.0, 0.0, 1.0),
                radius: 2.0,
            },
        );
        assert_eq!(oncurve_terminal_overrun(&curves, &cyc, 2, 5.0, &post), None);
    }

    /// Periodic params certify across the ±π branch cut — the wrapped deltas
    /// keep the order reading, exactly like `conic_param_deltas`' convention.
    #[test]
    fn oncurve_terminal_overrun_wraps_periodic_params_across_the_cut() {
        let pi = std::f64::consts::PI;
        let cyc = vec![0u32, 1, 2, 3, 4];
        let post = vec![
            [5.0, 5.0, 0.0],
            on_circle(pi - 0.05),  // other
            on_circle(-pi + 0.03), // apex — 0.04 past the survivor, wrapped
            on_circle(pi - 0.01),  // survivor at the terminal
            [6.0, 6.0, 0.0],
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        assert_eq!(
            oncurve_terminal_overrun(&curves, &cyc, 2, 5.0, &post),
            Some(3)
        );
    }

    /// Gate pin (FLIPPED 2026-08-25): the arm is always-on — the certified
    /// corner yields exactly one site by default; the off-knob
    /// (`YANG_441_ONCURVE_MERGE=0|off`) restores the still-apex-only
    /// selector, byte-identically.
    #[test]
    fn oncurve_arm_follows_the_flipped_gate() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        // v4 sits so corner (2,3,4) is NOT chord-inverted — only the on-curve
        // corner (1,2,3) carries the inversion, and its apex MOVED, so the
        // still-apex selector has nothing and only the gated arm could act.
        let pre = m(&[
            (0, [5.0, 5.0, 0.0]),
            (1, on_circle(0.10)),
            (2, on_circle(0.15)), // apex sat between its ends…
            (3, on_circle(0.20)),
            (4, [0.99, -1.0, 0.0]),
        ]);
        let post = vec![
            [5.0, 5.0, 0.0],
            on_circle(0.10),
            on_circle(0.28), // …and was relocated past the terminal
            on_circle(0.20),
            [0.99, -1.0, 0.0],
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census) = fold_merge_sites_censused([cyc.as_slice()], &pre, &post, &curves);
        assert_eq!(census.apex_moved_on_curve, 1, "the corner IS censused");
        if oncurve_merge_enabled() {
            assert_eq!(census.oncurve_sites, 1, "the always-on arm proposes it");
            assert_eq!(sites.len(), 1, "{sites:?}");
            assert_eq!((sites[0].victim, sites[0].survivor), (2, 3));
        } else {
            assert_eq!(census.oncurve_sites, 0, "off-knob: arm never proposes");
            assert!(sites.is_empty(), "{sites:?}");
        }
    }

    /// A healthy convex cycle has every corner inside its own chord — the
    /// selector must not touch a patch that is fine (P10: never rewrite a valid
    /// boundary).
    #[test]
    fn fold_merge_site_leaves_a_healthy_cycle_alone() {
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [1.0, 1.0, 0.0]),
            (3, [0.0, 1.0, 0.0]),
        ]);
        let post = vec![
            [0.0, 0.0, 0.0],
            [1.1, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        assert!(fold_merge_sites([cyc.as_slice()], &pre, &post).is_empty());
    }

    /// An inversion that ALREADY existed before Stage 4 is inherited geometry,
    /// not a mint — excluded (the repair must not claim it).
    #[test]
    fn fold_merge_site_refuses_an_inherited_inversion() {
        let cyc = vec![0u32, 1, 2, 3];
        // 1 lies past 0 both BEFORE and after; only 0's z nudges.
        let pre = m(&[
            (0, [1.0, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
        ]);
        let post = vec![
            [1.0, 0.0, 0.001],
            [1.2, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.5, -1.0, 0.0],
        ];
        assert!(fold_merge_sites([cyc.as_slice()], &pre, &post).is_empty());
    }

    /// An inversion whose overrun end never MOVED is not this defect — the
    /// apex must have been overrun BY a relocation.
    #[test]
    fn fold_merge_site_refuses_an_inversion_with_no_moved_end() {
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, [1.4, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
        ]);
        // Nobody moved: the inversion cannot have been minted here, so the
        // pre-order test already excludes it. Move only a far-away vertex.
        let post = vec![
            [1.4, 0.0, 0.0],
            [1.2, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.5, -1.1, 0.0],
        ];
        assert!(fold_merge_sites([cyc.as_slice()], &pre, &post).is_empty());
    }

    /// A MOVED apex is never merged away: it is the one carrying Stage 4's
    /// exact analytic position.
    #[test]
    fn fold_merge_site_never_merges_a_moved_vertex() {
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, [1.4, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
        ]);
        // BOTH ends of the inversion moved — the apex is no longer a plain
        // discretization vertex, so the site is refused.
        let post = vec![
            [1.0, 0.0, 0.0],
            [1.25, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.5, -1.0, 0.0],
        ];
        let sites = fold_merge_sites([cyc.as_slice()], &pre, &post);
        assert!(sites.iter().all(|s| s.victim != 1), "{sites:?}");
    }

    /// Two different survivors claiming one victim is AMBIGUOUS — dropped,
    /// never resolved by a distance guess.
    #[test]
    fn fold_merge_site_drops_an_ambiguous_victim() {
        let a = vec![0u32, 1, 2, 3];
        let b = vec![4u32, 1, 5, 6];
        let pre = m(&[
            (0, [1.4, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
            (4, [1.4, 0.1, 0.0]),
            (5, [0.0, 0.1, 0.0]),
            (6, [0.5, -1.0, 0.1]),
        ]);
        let post = vec![
            [1.0, 0.0, 0.0],
            [1.2, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.5, -1.0, 0.0],
            [1.1, 0.1, 0.0],
            [0.0, 0.1, 0.0],
            [0.5, -1.0, 0.1],
        ];
        let sites = fold_merge_sites([a.as_slice(), b.as_slice()], &pre, &post);
        assert!(
            sites.iter().all(|s| s.victim != 1),
            "two survivors claim v1, so it is dropped: {sites:?}"
        );
    }

    /// Victims and survivors are disjoint by construction (a survivor moved, a
    /// victim did not), so no batch can ever chain two substitutions. Pinned so
    /// a later relaxation of either condition confronts chaining explicitly.
    #[test]
    fn fold_merge_victims_and_survivors_are_disjoint() {
        let a = vec![0u32, 1, 2, 3];
        let b = vec![7u32, 0, 8, 9];
        let pre = m(&[
            (0, [1.4, 0.0, 0.0]),
            (1, [1.2, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.0]),
            (3, [0.5, -1.0, 0.0]),
            (7, [1.1, 0.0, 0.0]),
            (8, [0.2, 0.0, 0.0]),
            (9, [0.7, -1.0, 0.0]),
        ]);
        let post = vec![
            [1.0, 0.0, 0.0],
            [1.2, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.5, -1.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.9, 0.0, 0.0],
            [0.2, 0.0, 0.0],
            [0.7, -1.0, 0.0],
        ];
        let sites = fold_merge_sites([a.as_slice(), b.as_slice()], &pre, &post);
        let victims: BTreeSet<u32> = sites.iter().map(|s| s.victim).collect();
        let survivors: BTreeSet<u32> = sites.iter().map(|s| s.survivor).collect();
        assert!(victims.is_disjoint(&survivors), "{sites:?}");
        assert!(!victims.contains(&0), "a moved vertex is never a victim");
    }

    #[test]
    fn degenerate_inputs_are_skipped_not_scored() {
        // Self-loop edge, and a zero-length pre spacing (coincident pair).
        let pre = m(&[(0, [0.0, 0.0, 0.0]), (1, [0.0, 0.0, 0.0])]);
        let post = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        assert!(rank_fold_risks(&pre, &post, &e(&[(1, 1), (0, 1)])).is_empty());
    }

    // ---- I13d run-level junction absorption ----------------------------

    /// Positions off the run curve (the rim chain in the measured shape):
    /// scaled circle points, so every conic_param is still defined but the
    /// vertices sit on no typed edge.
    fn off_circle(t: f64) -> [f64; 3] {
        let p = on_circle(t);
        [2.0 * p[0], 2.0 * p[1], p[2]]
    }

    fn absorb(
        cycles: &[Vec<u32>],
        pre: &HashMap<u32, [f64; 3]>,
        post: &[[f64; 3]],
        curves: &BTreeMap<(u32, u32), crate::Curve>,
        richer: impl Fn(u32, u32) -> bool,
    ) -> (Vec<RunAbsorptionSite>, RunAbsorptionCensus) {
        run_absorption_sites(cycles.iter().map(Vec::as_slice), pre, post, curves, richer)
    }

    /// The R0003 face-467 shape in miniature: junction 1 at t=0.50, victims
    /// 2 (t 0.55→0.40, deepest) and 3 (t 0.58→0.45) carried past it, then
    /// the healthy ascending chain 4 (0.60), 5 (0.70) up to the far junction
    /// 6 (0.80). The run selector absorbs BOTH victims into junction 1; the
    /// corner selector is blind here (2's chord-sign survivor is 3, whose
    /// far edge continues the curve).
    #[test]
    fn run_absorption_certifies_the_face_467_two_vertex_run() {
        let cyc = vec![0u32, 1, 2, 3, 4, 5, 6, 7];
        let pre = m(&[
            (0, off_circle(0.45)),
            (1, on_circle(0.50)),
            (2, on_circle(0.55)),
            (3, on_circle(0.58)),
            (4, on_circle(0.60)),
            (5, on_circle(0.70)),
            (6, on_circle(0.80)),
            (7, off_circle(0.85)),
        ]);
        let post = vec![
            off_circle(0.45),
            on_circle(0.50),
            on_circle(0.40),
            on_circle(0.45),
            on_circle(0.60),
            on_circle(0.70),
            on_circle(0.80),
            off_circle(0.85),
        ];
        let curves = curve_map(&[(1, 2), (2, 3), (3, 4), (4, 5), (5, 6)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |j, _| j == 1);
        assert_eq!(
            sites,
            vec![RunAbsorptionSite {
                survivor: 1,
                victims: vec![2, 3],
            }]
        );
        assert_eq!(census.runs, 1);
        // The far terminal's walk finds no flipped vertex (healthy chain).
        assert_eq!(census.no_flip, 1);
    }

    /// The REAL face-467 shape (measured 2026-08-25): the chain samples
    /// barely move, and the JUNCTION's own relocation solves it past the
    /// first two of them in curve parameter. The one-sided view from the
    /// samples calls this healthy; the pair-order inversion certifies it,
    /// and the same two-victim absorption falls out.
    #[test]
    fn run_absorption_certifies_a_junction_that_hopped_its_samples() {
        let cyc = vec![0u32, 1, 2, 3, 4, 5];
        let pre = m(&[
            (0, off_circle(0.35)),
            (1, on_circle(0.40)), // junction pre: below the whole chain
            (2, on_circle(0.44)),
            (3, on_circle(0.47)),
            (4, on_circle(0.60)),
            (5, off_circle(0.65)),
        ]);
        let post = vec![
            off_circle(0.35),
            on_circle(0.50), // junction solved PAST samples 2 and 3
            on_circle(0.44),
            on_circle(0.47),
            on_circle(0.60),
            off_circle(0.65),
        ];
        let curves = curve_map(&[(1, 2), (2, 3), (3, 4)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |j, _| j == 1);
        assert_eq!(
            sites,
            vec![RunAbsorptionSite {
                survivor: 1,
                victims: vec![2, 3],
            }]
        );
        assert_eq!(census.no_flip, 1); // the far terminal's healthy walk
    }

    /// The v3264 shape: ONE victim between two junctions, carried past the
    /// nearer one. Same site the corner arm certifies — the two arms agree.
    #[test]
    fn run_absorption_matches_the_corner_arm_on_a_single_overrun() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let pre = m(&[
            (0, off_circle(0.65)),
            (1, on_circle(0.60)),
            (2, on_circle(0.55)),
            (3, on_circle(0.50)),
            (4, off_circle(0.45)),
        ]);
        let post = vec![
            off_circle(0.65),
            on_circle(0.60),
            on_circle(0.45),
            on_circle(0.50),
            off_circle(0.45),
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |_, _| true);
        assert_eq!(
            sites,
            vec![RunAbsorptionSite {
                survivor: 3,
                victims: vec![2],
            }]
        );
        assert_eq!(census.no_flip, 1); // the far junction's end
    }

    /// A healthy chain — every vertex pre AND post in-band — produces no
    /// site: the flip test is what separates a legitimate same-curve run
    /// bounded by a junction from an overrun one.
    #[test]
    fn run_absorption_leaves_a_healthy_chain_alone() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let pts = [
            off_circle(0.65),
            on_circle(0.60),
            on_circle(0.55),
            on_circle(0.50),
            off_circle(0.45),
        ];
        let pre = m(&[
            (0, pts[0]),
            (1, pts[1]),
            (2, on_circle(0.53)), // moved, but stays in-band
            (3, pts[3]),
            (4, pts[4]),
        ]);
        let post = pts.to_vec();
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |_, _| true);
        assert!(sites.is_empty());
        assert_eq!(census.no_flip, 2);
    }

    /// A flipped vertex whose corner was ALREADY outside its neighbours'
    /// chord before Stage 4 is inherited geometry: no MINTED inversion, no
    /// absorption.
    #[test]
    fn run_absorption_refuses_an_inherited_fold() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let pre = m(&[
            (0, off_circle(0.65)),
            (1, on_circle(0.60)),
            (2, on_circle(0.65)), // outside [1,3]'s chord already
            (3, on_circle(0.50)),
            (4, off_circle(0.45)),
        ]);
        let post = vec![
            off_circle(0.65),
            on_circle(0.60),
            on_circle(0.45),
            on_circle(0.50),
            off_circle(0.45),
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |_, _| true);
        assert!(sites.is_empty());
        // Both terminals walk a flipped prefix (the pre position sits outside
        // the whole band), and both refuse it as inherited.
        assert_eq!(census.no_inversion, 2);
    }

    /// The junction must be strictly richer than every victim — a terminal
    /// that is a plain 2-carrier vertex is not a model junction, and
    /// absorbing into it would discard nothing-provable.
    #[test]
    fn run_absorption_requires_the_richer_junction() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let pre = m(&[
            (0, off_circle(0.65)),
            (1, on_circle(0.60)),
            (2, on_circle(0.55)),
            (3, on_circle(0.50)),
            (4, off_circle(0.45)),
        ]);
        let post = vec![
            off_circle(0.65),
            on_circle(0.60),
            on_circle(0.45),
            on_circle(0.50),
            off_circle(0.45),
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |_, _| false);
        assert!(sites.is_empty());
        assert_eq!(census.not_richer, 1);
    }

    /// Periodic parameters: the flip test wraps deltas to (−π, π], so an
    /// overrun across the circle's branch cut still certifies.
    #[test]
    fn run_absorption_wraps_the_flip_across_the_branch_cut() {
        let cyc = vec![0u32, 1, 2, 3, 4];
        let pre = m(&[
            (0, off_circle(2.7)),
            (1, on_circle(2.8)),
            (2, on_circle(2.9)),
            (3, on_circle(3.0)),
            (4, off_circle(3.05)),
        ]);
        let post = vec![
            off_circle(2.7),
            on_circle(2.8),
            on_circle(-3.1), // ≈ +3.18 wrapped: past the junction at 3.0
            on_circle(3.0),
            off_circle(3.05),
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, _) = absorb(&[cyc], &pre, &post, &curves, |_, _| true);
        assert_eq!(
            sites,
            vec![RunAbsorptionSite {
                survivor: 3,
                victims: vec![2],
            }]
        );
    }

    /// The same boundary chain appears in BOTH adjacent patches' cycles —
    /// the reversed walk derives the identical site, deduped to one.
    #[test]
    fn run_absorption_dedupes_across_the_two_holder_cycles() {
        let fwd = vec![0u32, 1, 2, 3, 4];
        let rev = vec![4u32, 3, 2, 1, 0];
        let pre = m(&[
            (0, off_circle(0.65)),
            (1, on_circle(0.60)),
            (2, on_circle(0.55)),
            (3, on_circle(0.50)),
            (4, off_circle(0.45)),
        ]);
        let post = vec![
            off_circle(0.65),
            on_circle(0.60),
            on_circle(0.45),
            on_circle(0.50),
            off_circle(0.45),
        ];
        let curves = curve_map(&[(1, 2), (2, 3)]);
        let (sites, census) = absorb(&[fwd, rev], &pre, &post, &curves, |_, _| true);
        assert_eq!(sites.len(), 1);
        assert_eq!(census.sites, 1);
    }

    /// A vertex shared by TWO curve chains whose pair order inverted against
    /// a different junction on each: two sites with different survivors
    /// claim it — both drop, loudly, and the census names the ambiguity.
    #[test]
    fn run_absorption_drops_an_ambiguous_victim() {
        // C1 = the unit circle; C2 = a unit circle centred at (0.5, 0, 0).
        let c2 = crate::Curve::Circle {
            center: cad_primitives::Point3::new(0.5, 0.0, 0.0),
            normal: crate::Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let on_c2 = |t: f64| [0.5 + t.cos(), t.sin(), 0.0];
        // Vertex 2 sits on both chains: its C1 order vs junction 1 inverts
        // (0.55 → 0.45 about 0.50), and its C2 order vs junction 3 inverts
        // (C2 params of those positions are ≈0.977 → ≈0.827 about 0.90).
        let cyc_a = vec![8u32, 1, 2, 9];
        let cyc_b = vec![7u32, 3, 2, 6];
        let pre = m(&[
            (8, off_circle(0.40)),
            (1, on_circle(0.50)),
            (2, on_circle(0.55)),
            (9, on_circle(0.60)),
            (7, off_circle(0.40)),
            (3, on_c2(0.90)),
            (6, on_c2(1.05)),
        ]);
        let mut post = vec![[0.0; 3]; 10];
        post[8] = off_circle(0.40);
        post[1] = on_circle(0.50);
        post[2] = on_circle(0.45);
        post[9] = on_circle(0.60);
        post[7] = off_circle(0.40);
        post[3] = on_c2(0.90);
        post[6] = on_c2(1.05);
        let mut curves = curve_map(&[(1, 2)]);
        curves.insert((2, 3), c2);
        let richer = |j: u32, _v: u32| j == 1 || j == 3;
        let (sites, census) = absorb(&[cyc_a, cyc_b], &pre, &post, &curves, richer);
        assert!(sites.is_empty(), "both claims dropped: {sites:?}");
        assert_eq!(census.ambiguous, 1);
    }

    /// A cycle typed end-to-end on one curve is a CLOSED seam — no bounded
    /// end, the reorder authority's shape, never absorbed.
    #[test]
    fn run_absorption_skips_a_fully_typed_cycle() {
        let cyc = vec![0u32, 1, 2, 3];
        let pre = m(&[
            (0, on_circle(0.0)),
            (1, on_circle(1.5)),
            (2, on_circle(3.0)),
            (3, on_circle(4.5)),
        ]);
        let post = vec![
            on_circle(0.0),
            on_circle(1.5),
            on_circle(3.0),
            on_circle(4.5),
        ];
        let curves = curve_map(&[(0, 1), (1, 2), (2, 3), (3, 0)]);
        let (sites, census) = absorb(&[cyc], &pre, &post, &curves, |_, _| true);
        assert!(sites.is_empty());
        assert_eq!(census.runs, 0);
    }
}
