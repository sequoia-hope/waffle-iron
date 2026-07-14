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
//! `euler_target`) is NOT implementable here: no reference mesh or measured
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

    eprintln!("historical_authoring_fixes_pinned: R0099 & R0006 targets held");
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
