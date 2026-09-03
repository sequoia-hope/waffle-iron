//! Corpus `euler_target` self-consistency guard (design-review finding F15,
//! 2026-07-12).
//!
//! The assay corpus's `oracles.euler_target` meta is a HAND-MAINTAINED oracle:
//! `gen::compute_euler_target` emits it for the randomized R-series,
//! `gen_complexity` hand-authors it for the C/F complexity cases, and several
//! metas were curated by hand after mesh inspection (`app/tests/cases/assay`
//! is a FROZEN artifact — `assay_gen`'s header states the R/F files are never
//! regenerated). Two historical authoring errors (R0099, R0006 — an
//! `euler_target` that disagreed with the mesh-verified topology) were caught
//! only by manual investigation. This binary is the automated guard.
//!
//! **What it can and cannot do — read before extending.** The strongest guard
//! (derive χ = V−E+F independently from a reference mesh and diff it against
//! `euler_target`) is NOT implementable HERE (mesh-free, always-on): no
//! reference mesh or measured V/E/F is checked in. It IS available as a
//! manual instrument since 2026-09-03 — `assay_topology_oracle` with
//! `TOPO_SIDECAR=1` unions the isolated operand tessellations through the
//! Cherchi sidecar and reads χ and the shell count off the result (see
//! `docs/TESTING.md` § "Topology adjudication"). Original note: no reference
//! mesh or measured
//! V/E/F is checked into the repo (`.meta.json` carries only scalar oracle
//! targets), and the only artifact with a measured result
//! (`target/assay_kv2_report.json`) stores a pass/fail *category* string, not
//! χ, and exists only after building all 295 solids — which is the assay
//! itself. So the checks below are the cheapest SOUND, mesh-free guards:
//!
//!   1. `every_euler_target_is_even` — χ = 2·B − 2·g is even for any set of
//!      closed orientable shells (the disjoint-shell credit in
//!      `check_mesh_euler_characteristic` adds +2 per shell, preserving
//!      parity). An odd `euler_target` is therefore always an authoring error.
//!   2. `description_chi_matches_field` — 70 metas embed `chi=<n>` in their
//!      human description, authored separately from the numeric field. The two
//!      serialized representations of the same fact must agree; a hand-edit to
//!      one that forgets the other is exactly the F15 drift class.
//!   3. `historical_authoring_fixes_pinned` — R0099 and R0006 pinned so their
//!      corrected targets can never silently regress.
//!   4. `generator_output_is_even` — the generators themselves (not just the
//!      frozen corpus) must only ever emit even targets, so a future edit to
//!      `compute_euler_target` / `gen_complexity` fails loudly before anyone
//!      regenerates and commits.
//!
//! Deliberately NOT asserted: `compute_euler_target(ops) == stored`. The
//! corpus is frozen and the heuristic has since evolved (and intentionally
//! under-claims on multi-plane cuts), so ~62/295 metas legitimately disagree
//! with a fresh op-scan. Asserting equality would be ~62 false failures. See
//! the R0006 pin below for a worked example.

use std::fs;
use std::path::PathBuf;

use test_harness::assay::gen::{compute_euler_target, generate_case, AssayMeta, OpMeta};

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

/// Load every committed `*.meta.json` as `(case_id, AssayMeta)`.
fn load_all_metas() -> Vec<(String, AssayMeta)> {
    let dir = assay_dir();
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).expect("read assay corpus dir") {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".meta.json") {
            continue;
        }
        let id = name.trim_end_matches(".meta.json").to_string();
        let meta: AssayMeta = serde_json::from_str(&fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| panic!("{id}: parse meta: {e}"));
        out.push((id, meta));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(
        out.len() >= 200,
        "expected the full committed corpus, found only {} metas",
        out.len()
    );
    out
}

fn load_meta(id: &str) -> AssayMeta {
    let path = assay_dir().join(format!("{id}.meta.json"));
    serde_json::from_str(&fs::read_to_string(&path).unwrap())
        .unwrap_or_else(|e| panic!("{id}: parse meta: {e}"))
}

/// Extract an embedded `chi=<int>` from a description, if present. Independent
/// of any serde field — a plain scan of the human string.
fn description_chi(desc: &str) -> Option<i64> {
    let idx = desc.find("chi=")?;
    let rest = &desc[idx + 4..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    rest[..end].parse::<i64>().ok()
}

/// Check 1 — sound invariant over the whole committed corpus.
///
/// χ of any set of closed orientable manifolds is even (2·B − 2·g). This is a
/// necessary condition, so any odd `euler_target` is a definite authoring
/// error — regardless of which generation path produced the case.
#[test]
fn every_euler_target_is_even() {
    let metas = load_all_metas();
    let odd: Vec<(String, i64)> = metas
        .iter()
        .filter(|(_, m)| m.oracles.euler_target % 2 != 0)
        .map(|(id, m)| (id.clone(), m.oracles.euler_target))
        .collect();
    assert!(
        odd.is_empty(),
        "euler_target must be even (χ = 2B − 2g); odd targets found: {odd:?}"
    );
    eprintln!(
        "every_euler_target_is_even: {} metas checked, all even",
        metas.len()
    );
}

/// Check 2 — sound cross-check between two independently-authored
/// representations of χ.
///
/// A description like `... genus-N plate: 2 through-holes (g=2, chi=-2)` states
/// χ in prose; `oracles.euler_target` states it as a number. A hand-edit that
/// changes one and not the other is precisely the F15 drift the review flagged.
#[test]
fn description_chi_matches_field() {
    let metas = load_all_metas();
    let mut checked = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for (id, m) in &metas {
        if let Some(desc_chi) = description_chi(&m.description) {
            checked += 1;
            if desc_chi != m.oracles.euler_target {
                mismatches.push(format!(
                    "{id}: description chi={desc_chi} but field euler_target={}",
                    m.oracles.euler_target
                ));
            }
        }
    }
    assert!(
        checked >= 60,
        "expected the C/F chi-annotated cases; only {checked} descriptions carried chi="
    );
    assert!(
        mismatches.is_empty(),
        "description/field χ disagreement:\n  {}",
        mismatches.join("\n  ")
    );
    eprintln!("description_chi_matches_field: {checked} chi-annotated metas all agree");
}

/// Check 3 — regression pins for the two historically miswritten targets.
///
/// R0099 (`extrude(circle,boss)+extrude(circle,through-cut)+revolve(rect,cut)`)
/// was stored as χ=2 but is genus-1 (χ=0, mesh-verified); the fix (74564242)
/// corrected BOTH the meta and `compute_euler_target`, so today they agree at
/// 0 — pin both.
///
/// R0006 was curated to χ=0 by hand after mesh inspection (its later boss does
/// not refill the multi-plane through-cut). `compute_euler_target` still
/// returns 2 here BY DESIGN — it conservatively under-claims on multi-plane
/// cuts (`plane_normal.is_none()` gate). So we pin ONLY the frozen field, and
/// assert the divergence-by-design is intact; this is the worked example for
/// why an equality guard against the op-scan would be unsound.
#[test]
fn historical_authoring_fixes_pinned() {
    let r0099 = load_meta("R0099");
    assert_eq!(
        r0099.oracles.euler_target, 0,
        "R0099 corrected target regressed (must stay genus-1 χ=0)"
    );
    assert_eq!(
        compute_euler_target(&r0099.operations),
        0,
        "compute_euler_target regressed for the R0099 boss+through-cut+revolve-cut class"
    );

    let r0006 = load_meta("R0006");
    assert_eq!(
        r0006.oracles.euler_target, 0,
        "R0006 curated target regressed (must stay χ=0)"
    );
    // Divergence-by-design: the op-scan conservatively returns 2 for this
    // multi-plane case. If this ever equals 0, the heuristic changed and the
    // "frozen corpus ≠ op-scan" rationale in this file's header must be
    // re-examined — but it is NOT a corpus error.
    assert_eq!(
        compute_euler_target(&r0006.operations),
        2,
        "compute_euler_target(R0006) changed; revisit the frozen-corpus soundness note"
    );

    // R0091 was hand-corrected 2026-07-21 (task #186 / spec
    // `yang_453_junction_protected_collapse` §3b unblock): the naive 3-op
    // default 2 was refuted — the tilted wide-tube cut leaves 4 corner
    // pillars (genus 3, χ=−4), verified by the Cherchi-2022 sidecar
    // reference boolean on the exact operand meshes AND an independent
    // voxel-CSG derivation from the authored numbers (both χ=−4, 1 shell).
    // `compute_euler_target` still returns the conservative 2 for this
    // class BY DESIGN (same divergence-by-design as R0006).
    let r0091 = load_meta("R0091");
    assert_eq!(
        r0091.oracles.euler_target, -4,
        "R0091 corrected target regressed (must stay genus-3 χ=−4)"
    );
    assert_eq!(
        compute_euler_target(&r0091.operations),
        2,
        "compute_euler_target(R0091) changed; revisit the frozen-corpus soundness note"
    );

    // R0063 was hand-corrected 2026-07-22 (task #195 spec §5e flip-blocker
    // triage): the naive 3-op default 2 was refuted by exact derivation from
    // the authored sketch numbers. The concentric prism stack (circle boss
    // r=4.538e-4 depth 9.354e-4; rectangle cut half-extents 4.761e-4 ×
    // 4.222e-4 depth 6.393e-4; gear boss root≈8.49e-4 depth 6.3365e-4)
    // satisfies w/2 > r (the slot spans the full cylinder), h/2 < r (two
    // crescents survive in the slit band), rect ⊂ gear-root disc, and
    // gear-top < cut-floor by 5.64e-6 — so the z-slabs stack disc(top) ↔
    // {crescent A, crescent B} ↔ gear disc(bottom): one independent cycle
    // through the two crescents = genus 1, single shell, χ=0. The passing
    // volume-monotonicity oracle (increase/decrease/increase) confirms the
    // cut direction. `compute_euler_target` still returns the conservative
    // 2 for this class BY DESIGN (same divergence-by-design as R0006/R0091).
    let r0063 = load_meta("R0063");
    assert_eq!(
        r0063.oracles.euler_target, 0,
        "R0063 corrected target regressed (must stay genus-1 χ=0)"
    );
    assert_eq!(
        compute_euler_target(&r0063.operations),
        2,
        "compute_euler_target(R0063) changed; revisit the frozen-corpus soundness note"
    );

    // R0011 was hand-corrected 2026-08-31 (corner-transit inc-2c-3b-2, spec
    // `specs/yang_451_corner_transit.md` §3j): the naive default 2 was
    // refuted by an independent voxel-CSG derivation from the authored
    // numbers PLUS the §4.5.2 density ladder. The 14-tooth gear prism
    // (module 357.4, depth 4998.2 along (0.261, 0.023, −0.965)) grazes the
    // 295.56° rectangle-revolve band (r ∈ [4708, 6277] about the in-plane
    // axis) ONLY near its start cap (spine t < 150): the exact involute
    // polygon (`generate_gear_preview_polyline`) intersected with the band
    // yields exactly TWO disjoint adjacent-tooth contact patches (k = 2
    // across tooth phases; root circle r=2055 misses the band entirely, so
    // contact is strictly tooth-tip territory), and two genus-0 solids
    // glued along k patches have genus k−1 = 1 ⇒ χ = 0, single shell. The
    // first completed R0011 result (gated corner-transit repair,
    // `YANG_451_TRANSIT=1`) measures χ = 0 at 1×/2×/4× chord density —
    // ladder-stable through three DIFFERENT §4-I9 fire anatomies and
    // repair paths — with volume, watertightness, and single-shell all
    // passing. `compute_euler_target` still returns the conservative 2 for
    // this class BY DESIGN (same divergence-by-design as R0006/R0091).
    let r0011 = load_meta("R0011");
    assert_eq!(
        r0011.oracles.euler_target, 0,
        "R0011 corrected target regressed (must stay genus-1 χ=0)"
    );
    assert_eq!(
        compute_euler_target(&r0011.operations),
        2,
        "compute_euler_target(R0011) changed; revisit the frozen-corpus soundness note"
    );

    // R0053 was hand-corrected 2026-09-03 (corner-transit inc-3b, spec
    // `specs/yang_451_corner_transit.md` §3ah): the naive 3-op default 2
    // was refuted by EXACT analytic membership — no tessellation anywhere.
    // The 287.6° rectangle revolve (half-extents 20.81 axial × 44.56
    // radial, axis 62.44 from the sketch origin), the 100.27-deep box
    // (41.49 × 52.07) on the same plane, and the 301.9° revolve of the
    // 16-tooth involute gear (module 7.455; root 50.32, addendum 67.10)
    // about a parallel axis 27.02 further out are each a closed-form
    // point predicate (`tests/s453_r0053_exact_topology.rs`); the cubical
    // χ of their set union reads 0 with one component at cell sizes
    // 2, 1, 0.7, 0.5 and 0.4 on two lattice phases — ladder-stable, genus
    // 1: the box bridges the C-ring's gap (the two-op prefix already reads
    // 0) and the gear adds no handle (ring ∪ gear alone reads 1; the ring
    // reaches only three teeth, 247.5°–292.5°, and fills their grooves
    // from the root side). The kernel's completed result under the §4.5.3
    // surface-pair arm reads χ = 0, one shell, and its output mesh carries
    // no face inside the exact union beyond the root arc's chord band.
    // The Cherchi sidecar's contrary reading (χ = −28, "genus 15") was a
    // closed manifold with 606 faces strictly INSIDE the true solid —
    // coplanar membranes at the shared plane and sliver strips along the
    // ring-end/tooth-flank crossings — so it was never the union's
    // boundary. `compute_euler_target` still returns the conservative 2
    // for this class BY DESIGN (same divergence-by-design as R0011).
    let r0053 = load_meta("R0053");
    assert_eq!(
        r0053.oracles.euler_target, 0,
        "R0053 corrected target regressed (must stay genus-1 χ=0)"
    );
    assert_eq!(
        compute_euler_target(&r0053.operations),
        2,
        "compute_euler_target(R0053) changed; revisit the frozen-corpus soundness note"
    );

    // C0075 was hand-corrected 2026-08-19: the gen_complexity `tracker(2, …)`
    // default was refuted by an independent 2D derivation — two identical
    // 12-tooth gears (pitch r 0.48, tip 0.56, root 0.38) at centre distance
    // 0.6 interleave so the union of the involute profiles encloses exactly
    // two pockets (grid flood-fill over the preview polyline: 2 bounded
    // complement components, ≈0.0098 area each, mirror-symmetric), each a
    // through-hole of the equal-height extrusion ⇒ genus 2, χ=−2. The
    // kernel's first completed C0075 result (after the Stage-0 split-
    // collector identity fix) measured exactly χ=−2 with the in-line
    // composition oracle agreeing on volume, which is what exposed the
    // authoring default.
    let c0075 = load_meta("C0075");
    assert_eq!(
        c0075.oracles.euler_target, -2,
        "C0075 corrected target regressed (must stay genus-2 χ=−2)"
    );

    // R0003 was hand-adjudicated 2026-08-28 (spec
    // `yang_441_trim_cdt_construction.md` §I13(f) item 6, the f2c-3
    // rescope): the completed result is 3 shells with the MAIN shell
    // genus 2 — two micro-filament handles where the gear-flange corners
    // arch over the pocket-corner void. Adjudication: the `YANG_441_SLIT`
    // bridge census (closed-form analytic-lift classification against
    // the all-planar convex tool + rim-window solves locating both true
    // junctions) + the density ladder (`YANG_NSEG_FLOOR` 41/82/164,
    // per-component χ [−2,2,2] at every rung — converged, not chord-gap
    // artifact; at 164 the case completes end-to-end and the finished
    // B-Rep reads χ=2 with 3 shells) + the void-arch verification (a
    // point under each film's witness is inside all six tool
    // half-spaces; both film ends attach to material). Total
    // χ = −2+2+2 = 2 EQUALS the authored euler_target by cancellation,
    // but the χ-derived shell floor decodes "1 shell" and telescopes the
    // expectation to 2·shells = 6 — inexpressible without the authored
    // `expected_shell_count` (which the oracle enforces STRICTLY: exact
    // shell count, no extra-shell allowance).
    let r0003 = load_meta("R0003");
    assert_eq!(r0003.oracles.euler_target, 2, "R0003 χ target regressed");
    assert_eq!(
        r0003.oracles.expected_shell_count,
        Some(3),
        "R0003 adjudicated shell count regressed (3 shells, main genus 2)"
    );

    eprintln!("historical_authoring_fixes_pinned: R0099, R0006, R0091, R0063, R0011, R0053, C0075 & R0003 held");
}

/// Check 4 — the randomized generator must only ever emit even targets.
///
/// Guards `compute_euler_target` + `generate_case` at the source (pure, no
/// I/O): a future edit that emits an odd χ fails here immediately, before the
/// corpus is regenerated and committed. Deterministic — fixed seeds.
#[test]
fn generator_output_is_even() {
    let mut checked = 0usize;
    for seed in 0u64..4 {
        for index in 0usize..100 {
            let case = generate_case(seed, index);
            let chi = case.meta.oracles.euler_target;
            assert!(
                chi % 2 == 0,
                "generate_case(seed={seed}, index={index}) emitted odd euler_target {chi}"
            );
            checked += 1;
        }
    }
    // Also exercise the pure op-scan directly on a hand-built through-hole case
    // so a regression to the genus-1 rule is caught even if RNG never hits it.
    let boss = OpMeta {
        kind: "extrude".into(),
        profile_type: "rectangle".into(),
        profile_size: 1.0,
        depth_or_angle: 1.0,
        is_cut: false,
        plane_origin: None,
        plane_normal: None,
    };
    let through_cut = OpMeta {
        depth_or_angle: 2.0,
        is_cut: true,
        ..boss.clone()
    };
    assert_eq!(
        compute_euler_target(&[boss.clone(), through_cut]),
        0,
        "same-plane penetrating extrude-cut must open a genus-1 through-hole (χ=0)"
    );
    assert_eq!(
        compute_euler_target(&[boss.clone(), boss]),
        2,
        "boss+boss must stay genus-0 (χ=2)"
    );
    eprintln!(
        "generator_output_is_even: {checked} generated cases + op-scan spot checks, all even"
    );
}

/// Check 5 — the through-hole heuristic OVER-CLAIM class (task #155).
///
/// `compute_euler_target` calls a same-plane extrude-cut deeper than the boss a
/// genus-1 through-hole and emits χ=0 (`gen.rs`). That test is DEPTH-only: it
/// never checks whether the cut profile is XY-contained inside the boss
/// footprint. A cut that penetrates in depth but sits partly (or wholly)
/// OUTSIDE the boss cross-section removes a notch/chunk, not a closed tunnel —
/// the result stays genus-0 (χ=2). So the heuristic's 0 is an UPPER-BOUND on
/// genus; mesh measurement is the ground truth that corrects it back to 2. This
/// is one of the ~62 legitimate `compute != stored` disagreements the header
/// documents, in the OVER-claim (0-vs-2) direction (R0006 is the UNDER-claim
/// 2-vs-0 direction).
///
/// These five R-series metas were flagged (task #155) as "suspected miswritten
/// χ=2→0" precisely because the op-scan returns 0. This session's per-case
/// `single_case` run resolved the suspicion as WRONG — the metas are correct:
///
/// | case  | op-scan | stored | single_case (2026-07-14)          |
/// |-------|---------|--------|-----------------------------------|
/// | R0027 | 0       | 2      | SUPPORTED_CORRECT — mesh χ = 2     |
/// | R0055 | 0       | 2      | SUPPORTED_CORRECT — mesh χ = 2     |
/// | R0079 | 0       | 2      | SUPPORTED_CORRECT — mesh χ = 2     |
/// | R0088 | 0       | 2      | SUPPORTED_CORRECT — mesh χ = 2     |
/// | R0007 | 0       | 2      | UNSUPPORTED(coplanar/M8) — no mesh |
///
/// For the four SUPPORTED_CORRECT cases the euler oracle
/// (`check_mesh_euler_characteristic`) measured V−E+F on the real output and it
/// equalled the stored 2 (a single genus-0 shell) — the cuts are not contained
/// tunnels. R0007 walls at Stage-0 coplanar (roadmap M8) so no solid is built
/// and its target is not yet mesh-verified; it is pinned here only to prevent a
/// stale-suspicion flip to 0, and MUST be re-measured when M8 lands (its op-3
/// gear/rect cuts are LARGER than the circle boss — an engulfing cut, not a
/// contained loop — so genus-0 is the expected outcome).
///
/// The pin: stored stays 2, and the op-scan divergence (0) is asserted so a
/// future change to `compute_euler_target` that silences it forces a re-read of
/// this analysis rather than a silent corpus edit.
#[test]
fn throughhole_heuristic_overclaim_targets_pinned() {
    // The four mesh-verified genus-0 outputs plus the M8-pending R0007.
    for id in ["R0027", "R0055", "R0079", "R0088", "R0007"] {
        let meta = load_meta(id);
        assert_eq!(
            meta.oracles.euler_target, 2,
            "{id}: stored euler_target must stay 2 (mesh-verified genus-0; task #155 \
             resolved the χ=2→0 suspicion as a depth-only heuristic over-claim)"
        );
        assert_eq!(
            compute_euler_target(&meta.operations),
            0,
            "{id}: op-scan no longer over-claims a through-hole — if the heuristic \
             gained XY-containment awareness, delete this pin and revisit task #155"
        );
    }
    eprintln!(
        "throughhole_heuristic_overclaim_targets_pinned: R0007/R0027/R0055/R0079/R0088 held at 2"
    );
}
