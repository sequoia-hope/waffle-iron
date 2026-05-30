# Spec: PR-SSI5 — ssi-rs plane∩cone through-apex degenerate conics

**Status:** active (M5 / roadmap §4b Phase 1 — finish the analytical SSI engine)
**Feature cycle:** ssi-5
**Roles (P5):** Spec Writer = Manager; Test Author and Implementer are distinct agents.

## Goal

Handle the last plane∩cone case: the **through-apex degenerate conic** (cutting
plane passes through the cone apex). PR-SSI3/4 left this gated as
`Err(DegenerateInput)` (the AP branch); PR-SSI5 replaces that with the correct
degenerate result, **completing plane∩cone** (all sections — circle, ellipse,
parabola, hyperbola, and the through-apex degenerates).

A plane through the apex meets the infinite double cone in:
- **a point** (the apex) when the plane is steeper than the cone (`|k| > sinα`,
  incl. plane ⟂ axis) — not a curve ⇒ `Ok(vec![])`;
- **one line** (a tangent generator) when `|k| = sinα`;
- **two lines** (crossed generators) when `|k| < sinα`.

**No new `SsiCurve` types** (reuses `Line`). **No new classification gates** — the
sub-case is decided by the SAME `gd_±` sign test proven in SSI3/4 (since
`gd₊·gd₋ = k² − sinα²`). Line directions reuse SSI4's
`m̂ = normalize(â − k·n̂)`.

**Precision (unchanged):** true analytical curves, f64. Clean-room from legacy.

## `plane_cone` — replace the AP branch

The current AP branch (`|n̂·(apex − p)| < TAU_MODEL` ⇒ `Err(DegenerateInput)`) is
replaced. Reuse the existing setup: `n̂ = normalize(plane.normal)`,
`â = normalize(cone.axis_dir)`, `α`, `k = n̂·â`, `cosα`, `sinα`. Let
`apex` and `p = plane.point` (arrays). Define
`axis_in = â − k·n̂` (the in-plane projection of the cone axis — it lies in the
cutting plane since `n̂·axis_in = k − k = 0`), `s_n = |axis_in| = √(1 − k²)`.

### Branch table (apex on plane: `|n̂·(apex − p)| < TAU_MODEL`)

| # | sub-case | condition | result |
|---|---|---|---|
| AP-pt⊥ | point (plane ⟂ axis) | `s_n < TAU_MODEL` | `Ok(vec![])` (apex only) |
| AP-line | tangent (one generator) | `min(\|gd₊\|, \|gd₋\|) < TAU_MODEL` | one **Line** { point: apex, dir: m̂ } |
| AP-lines | crossed (two generators) | `gd₊.signum() ≠ gd₋.signum()` | **two Lines** through the apex (below) |
| AP-pt | point (steeper than cone) | else (`gd₊, gd₋` same sign ⇒ `k² > sinα²`) | `Ok(vec![])` (apex only) |

After the `s_n < TAU_MODEL` early-out, define `m̂ = axis_in / s_n` and the SSI3/4
symmetry-plane generators: `û = normalize(n̂ − k·â)` (norm `s_n`, safe here),
`g_± = cosα·â ± sinα·û`, `gd_± = n̂·g_±`. (`s_n < TAU_MODEL` is exactly the
plane-⟂-axis quantity gating C1 in the non-apex path; here it also guards `û`/`m̂`
against a zero in-plane projection.)

**AP-lines construction** (verified): `ŵ = normalize(n̂ × â)` (in-plane, ⟂ m̂);
`cφ = cosα / s_n`; `sφ = √(−gd₊·gd₋) / s_n` (note `−gd₊·gd₋ = sinα² − k² > 0` in
this branch). `d₁ = cφ·m̂ + sφ·ŵ`, `d₂ = cφ·m̂ − sφ·ŵ` (already unit:
`cφ² + sφ² = (cos²α + (sinα²−k²))/s_n² = (1−k²)/s_n² = 1`). Return
`vec![Line{point: apex, dir: d₁}, Line{point: apex, dir: d₂}]` — **`+ŵ` first**
(determinism). Each `d_i·â = cφ·(m̂·â) = (cosα/s_n)·s_n = cosα` (on the cone) and
`d_i·n̂ = 0` (in the plane).

**AP-line:** the two generators have merged (`sφ → 0`); `dir = m̂`. (`m̂·â = s_n`,
and at `|k| = sinα` ⇒ `s_n = cosα` ⇒ `m̂·â = cosα` on the cone; `m̂·n̂ = 0` in the
plane.)

*Tangent-window note (PR-SSI5 adversary):* the AP-line (tangent) sub-case is gated
by `min(|gd₊|,|gd₋|) < TAU_MODEL` on a dimensionless dot product, so it occupies a
k-window only ≈`1.4e-7` wide around `|k| = sinα` — correct and reachable, but a
coarsely-sampled caller sweep can step over it. This is intrinsic to an
exact-equality degenerate (the tangent conic is measure-zero), not a defect;
either side of the window the result (point `Ok([])` / two Lines) is correct.

**Evaluation order:** E1 (invalid cone / zero vectors → `Err(DegenerateInput)`) →
compute `k, axis_in, s_n` → **AP branch** (if apex on plane) → else the existing
non-apex path (C1 circle / C2 ellipse / PARA / HYPE, unchanged). The AP branch is
self-contained (computes its own `û/g_±/gd_±` after the `s_n` early-out).

## Invariants / oracles (P1, DoD §1)

- **I1 (on-surface — core):** every returned `Line` sampled via `eval` over a
  bounded `t ∈ [−T,T]` lies on **both** the plane (`|n̂·(x − p)| < TAU_MODEL`) and
  the cone (radial residual `| |(x−apex) − h·â| − |h|·tanα | < TAU_MODEL`,
  `h = (x−apex)·â`; reuse the SSI3 helper). Every line passes through the apex
  (`eval(0) = apex` since `Line::eval(t) = point + t·dir` and `point = apex`).
- **I2 (analytical geometry):** AP-lines → exactly two Lines through the apex, each
  `|dir·â| = cosα` (a generator on the cone), symmetric about `m̂`
  (`normalize(d₁ + d₂) ∥ m̂`), and distinct (`|d₁ − d₂| > TAU_MODEL`); AP-line →
  one Line through the apex, `dir ∥ m̂`, `|dir·â| = cosα`; AP-pt → `Ok(vec![])`.
- **I3 (branch coverage, P4):** AP-pt⊥, AP-pt (oblique, `sinα < |k| < 1`), AP-line,
  AP-lines each ≥1 test; non-apex C1/C2/PARA/HYPE still pass (regression).
- **I4 (symmetry):** `intersect(plane, cone) == intersect(cone, plane)` for an
  AP-line and an AP-lines case (same line set; order/sign tolerant).
- **I5 (determinism):** identical inputs → byte-identical output (two-line order
  `+ŵ` first).

## Failure modes
- **AP-pt returns `Ok(vec![])`** — the surfaces meet in a single point (the apex),
  which is not a curve. This matches the sphere-tangent / cylinder-disjoint `Ok([])`
  convention; it is **NOT** `DegenerateInput` (the input is a well-formed cone and
  plane). No `panic!`/`unwrap`.
- E1 (invalid cone half-angle, zero/non-finite `axis_dir`/`normal`) → still
  `Err(DegenerateInput)` (unchanged).
- `AnalyticalSolutionNotAvailable` is unaffected (still the verdict for
  unimplemented *pairs*: sphere∩cone, cyl∩cone, cone∩cone, …).

## Contract migration (PR-SSI3 AP tests)
PR-SSI3 asserted AP ⇒ `Err(DegenerateInput)`. Those assertions are obsoleted by
this spec and must migrate to the new contract (point → `Ok([])`, tangent → one
Line, crossed → two Lines): `ssi3.rs::{ap_through_apex_is_degenerate,
ap_through_apex_oblique_is_degenerate}` and any `ssi3_adversary` AP-boundary attack.
Migrate faithfully — determine each test's conic-type (compute `k` vs `sinα`) to
pick the correct new result, and preserve any structural intent. (Same
contract-migration discipline as PR-SSI4's PH migration; the PR-SSI5 Adversary
reviews it for faithfulness.)

## Research basis
- **Patrikalakis & Maekawa**, *Shape Interrogation for CAD/M*, **§5.8** (natural
  quadrics) — `docs/references/patrikalakis-shape-interrogation.txt`. The
  plane-through-apex degenerate conic (point / single line / two crossed lines) is
  the classical degenerate case of the conic-section family; cite §5.8 + the
  degenerate-conic fact.
- **Governance:** A15.1 (exact SSI), A15.2 (no fallback — point is `Ok([])`, not a
  grid/mesh fallback), A15.4 (pair #3), P8 (cite research), A14.3 (`TAU_MODEL`).

## Definition of Done (DoD §1)
Spec (this file); RED→GREEN separate commits; every AP sub-case (point⊥, point
oblique, one line, two lines) tested; numeric/structural oracles (on-surface w/
cone radial residual + lines-through-apex + generator-angle + symmetry, not "no
panic"); canonical (two-lines, one-line, point) + edge (near the point↔line↔two-line
boundaries; oblique non-axis apex) cases; symmetry + determinism; no
`unsafe`/`panic!`; CI gate (fmt + clippy -D warnings) clean for `ssi-rs`; the SSI3
AP-contract assertions migrated (adversary-reviewed faithful) and all other SSI1–4
tests untouched & green.
