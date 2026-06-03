# PR-CF1 — curved boolean fuzz: randomized {cylinder,sphere,cone} ∪/− box, correct-or-loud

Context: planar booleans have a 900-case randomized fuzz (`fuzz_boxes`, 100%
correct / 0 silent-wrong). Curved booleans (Union YR8–YR11, Subtract YR13–YR17)
are validated only on **hand-picked demo cases** — no fuzz. This PR builds the
curved analog to map the *real* robustness and surface any silent-wrong edge case.

## Contract — the planar fuzz's "0 silent-wrong" generalized to **correct-or-loud**
For every randomized case, `boolean(...)` must be **either**:
- **Correct** — sidecar mesh-parity (canonicalized) AND all invariants hold:
  watertight 2-manifold, Euler **χ = 2−2g** (genus computed from topology, not
  hardcoded), analytic surfaces survive to the output B-Rep, every exact
  intersection edge lies on BOTH incident analytic surfaces to `TAU_MODEL`; **or**
- **A loud, classified `Err`** — an out-of-scope case the pipeline correctly
  refuses (e.g. side-face-exit / corner triple-point, oblique-cone parabola/
  hyperbola rim, fully-internal multi-shell void, near-tangency, degenerate).

**ZERO silent-wrong** is the bar: never `Ok` with a wrong/non-watertight/off-surface
result. Categorize and `log` the `Err` taxonomy (which out-of-scope class dominates)
— that distribution is itself the deliverable (it maps what M5 has left).

## What to build
- A new fuzz test `crates/yang-rs/tests/fuzz_curved.rs`, mirroring `fuzz_boxes.rs`:
  deterministic **splitmix64** PRNG (fixed recorded seed; **no `rand`, no system
  time, no FS side effects** — governance determinism), N cases (pick a sensible
  N given curved booleans are slower than box-box — ~200–400; document the count
  and that it's not silently truncated).
- Per case: pick a random primitive ∈ {cylinder, sphere, cone} (reuse the
  `cylinder_brep`/`sphere_brep`/`cone_brep` fixtures), random radius/height/half-
  angle within sane ranges, a random rigid transform (splitmix64-quaternion as in
  `fuzz_boxes`), and a random box; random `op` ∈ {Union, Subtract}. Run
  `boolean(prim, box, op, &sidecar)`.
- **Audit every `Ok`** structurally + numerically (the invariants above), using
  the existing helpers (`signed_volume`, `unpaired_half_edges`,
  `euler_characteristic` from `fuzz_boxes`/`m3_adversary`, the on-surface residual
  helpers). **Classify every `Err`** into the taxonomy and tally it.
- **Env-gate the sidecar-parity arm** on `SidecarBoolean::from_env()` with a LOUD
  skip (mirror `fuzz_boxes`); the invariant checks on `Ok` results run regardless.
- The test FAILS only on a **silent-wrong** case (an `Ok` that violates an
  invariant or disagrees with the sidecar) or an **unclassified** `Err` — print
  the offending seed/case so it's reproducible.

## Scope
- This PR is the **harness** + the correct-or-loud assertion + the taxonomy report.
  It is NOT a fix pass — if it surfaces silent-wrong cases, those are real bugs to
  be fixed in a FOLLOW-UP (do NOT fix production here beyond what's needed to make
  the harness itself correct; report findings in the module doc + commit message).
- Do not change the boolean/tessellation production paths (other than, if strictly
  necessary, *adding* a loud classified `Err` for an out-of-scope case that is
  currently silent-wrong — and if you do, say so loudly and keep it minimal).

## CI gate (FULL crate)
`cargo test -p yang-rs` (whole crate; the fuzz must pass — i.e. correct-or-loud
holds across the case stream, OR it documents the exact silent-wrong seeds it
found as the deliverable and `#[ignore]`s only the demonstrators, keeping the
suite green), `cargo fmt -p yang-rs -- --check`, `cargo clippy -p yang-rs
--all-targets -- -D warnings`. All prior tests unregressed.

**STOP-and-report (P9/P10)** is NOT failure here — surfacing silent-wrong cases is
the POINT. If the fuzz finds them, document the seeds + the failure mode clearly
(that drives the Step-2 fix pass); do not paper over them.

On completion: update `docs/yang_functional_roadmap.md` — record PR-CF1 (curved
boolean fuzz; report the correct/loud-Err/silent-wrong tallies + the Err taxonomy).
The silent-wrong findings (if any) become the next increment.
