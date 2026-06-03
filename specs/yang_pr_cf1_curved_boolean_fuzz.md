# PR-CF1 — Curved Boolean Fuzz (`fuzz_curved.rs`)

**Status:** Spec (Manager phase). FIP cycle: Spec → RED → GREEN → Adversary.
**Crate:** `crates/yang-rs/` (tests only; production change only under the narrow
GREEN exception below).
**Date:** 2026-06-03.

## 1. Why

Planar booleans already have a 900-case randomized fuzz
(`crates/yang-rs/tests/fuzz_boxes.rs`) that proves **0 silent-wrong** across
aligned + rotated box pairs and reports the `YangError` taxonomy. Curved
booleans (Union YR8–YR11, Subtract YR13–YR17) are validated **only on
hand-picked demo configs**. There is no fuzz that maps the *real* robustness
envelope of curved input.

PR-CF1 builds the curved analog: a deterministic, randomized harness that runs
`boolean({cylinder|sphere|cone}, box, {Union|Subtract}, &sidecar)` over a
stream of cases and enforces a **correct-or-loud** contract on every result.
The deliverable is (a) the harness + the hard "0 silent-wrong / 0
unclassified-`Err`" assertion, and (b) the `Err`-taxonomy distribution — that
histogram *is* the map of what M5 has left.

**This is a harness PR, not a fix pass.** If it surfaces silent-wrong cases,
those are real bugs: documented (seeds in the module doc + commit msg) and
`#[ignore]`d as demonstrators for a FOLLOW-UP increment. We do **not** change
production boolean/tessellation paths here. The one allowed exception is the
GREEN exception in §6. STOP-and-report on silent-wrong is the POINT, not a
failure (P9/P10).

## 2. Established facts (from exploration)

- **Sidecar runs in this env.** `SidecarBoolean::from_env()` falls back to
  `DEFAULT_BIN_PATH` → present binary at
  `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`.
  `CHERCHI2022_BIN` is unset in the shell but the default resolves, so the fuzz
  actually executes and produces a real taxonomy. Sub-agents may export
  `CHERCHI2022_BIN` explicitly to be safe.
- **Hang risk mitigated.** `process::run_with_timeout` (30 s default) kills a
  hanging `mesh_booleans` → `SidecarError::TimedOut` → a recoverable classified
  `Err` (surfaces as `YangError::MeshBooleanFailed`), never an infinite loop.
- **`yang_rs::signed_distance_to_surface(surface: Surface, point: Point3) ->
  Result<f64, YangError>`** is public (`crates/yang-rs/src/lib.rs:1936`) — the
  exact tool for the on-surface-residual audit. Plane/Cylinder/Sphere/Cone all
  wired (YR15/YR17).
- **`YangError` has 11 variants** (`crates/yang-rs/src/lib.rs:2620`) + sub-reason
  enums `Stage4InvalidReason` (`OffCurveBeyondChordBand`, `OnAxis`,
  `EllipseProjectionUnsupported`, `InvertedTriangle`, `DegenerateTriangle`,
  `LoopTooSmall`, `LocalRefinementRequired`) and `SsiRefinementError`
  (`IntersectFailed`, `AmbiguousCurve`, `UnsupportedCurve`,
  `UnsupportedSurfaceForSsi`). `fuzz_boxes::err_variant_name` maps all 11;
  PR-CF1 reuses it and **extends** the tally to the sub-reasons, since those
  name the *specific* M5 gaps.
- **Direction catch:** `boolean(prim, box, Subtract)` = `prim − box` =
  **box-as-subtrahend**, the *opposite* of every demo (`box − prim`).
  Box-as-subtrahend is explicitly DEFERRED, so most Subtract cases are
  out-of-scope and should resolve to a loud `Err`. Whether they are loud vs
  silently-wrong is exactly what the fuzz maps.
- Curved B-Rep fixtures are **not shared** across integration test files — each
  re-declares `cylinder_brep` / `sphere_brep` / `cone_brep` + array-math helpers
  locally. `fuzz_curved.rs` copies the same pattern. Reference fixtures:
  `tests/yr8_curved_boolean.rs:102` (cylinder), `tests/yr12_sphere.rs:72`
  (sphere), `tests/yr16_cone.rs:125` / `tests/yr17_subtract_cone.rs:275` (cone),
  and `tests/fuzz_boxes.rs` (splitmix64 PRNG, quat→Mat3, `OrientedBox`, audit
  helpers).
- `Surface` (`lib.rs:118`): `Plane{normal,d}`, `Sphere{center,radius}`,
  `Cylinder{axis_point,axis_dir,radius}`, `Cone{apex,axis_dir,half_angle}`.
  `Curve` (`lib.rs:156`): `LineSegment`, `Circle{center,normal,radius}`,
  `Ellipse{center,normal,major_axis,major_radius,minor_radius}`.

## 3. The correct-or-loud contract (the bar: ZERO silent-wrong)

For every randomized case, `boolean(...)` must be **either**:

### Correct — ALL of:
1. **Watertight** closed 2-manifold: `unpaired_half_edges(mesh) == 0`.
2. **Euler χ even AND == sidecar-reference χ.** Topological genus parity from
   the reference mesh (`2−2g` computed from topology), NOT a hardcoded 2 —
   robust to tessellation-fidelity differences. (A corner clip can split into
   several shells; a through-hole punches genus. The CORRECT check is
   differential against the reference, plus χ even as a non-manifold-corruption
   guard.)
3. **Analytic surfaces survive** to the output B-Rep: the output carries the
   input's curved `Surface` variant with **exact** params; no
   Sphere/Cone/Cylinder appears in the output that was not an input surface.
4. **On-surface exactness (the strict gate):** every exact intersection-edge
   point lies on BOTH incident analytic surfaces to `TAU_MODEL`. Sample each
   `Curve::Circle/Ellipse/…` output edge and check `signed_distance_to_surface`
   against the two incident faces' surfaces. `|sd| ≤ TAU_MODEL` for all samples.
5. **Positive signed volume** (outward-oriented, not inside-out):
   `signed_volume(mesh) > 0`.
6. **Chord-band volume sanity:** `|vol_yang − vol_sidecar|` within a chord-error
   envelope (principled, scaled from the curved face's Stage-1 `d_ε`). Catches
   gross volume loss / a dropped chunk while tolerating the legitimate
   exact-curve-vs-faceted fidelity difference.

### OR a loud, classified `Err`
An out-of-scope case the pipeline correctly refuses (box-as-subtrahend,
side-face-exit / corner triple-point, oblique-cone parabola/hyperbola rim,
fully-internal void, near-tangency, degenerate, sidecar timeout, …). Every
`Err` must classify into the `YangError` taxonomy (+ sub-reason) and be tallied.

### The test FAILS only on
- a **silent-wrong** `Ok` (violates any of 1–6 above), or
- an **unclassified** `Err` (an "unknown" bucket is non-empty).

On failure it prints the offending **seed + case index + primitive + op +
params** so it is reproducible.

## 4. The curved-oracle decision (deliberate scoping, NOT tolerance widening)

PR-CF1 **replaces** `fuzz_boxes`' strict `VOL_TOL = 1e-6` differential with a
chord-band volume envelope (contract §3.6). Rationale: yang's exact-curve mesh
is **more accurate** than the sidecar's faceted reference, so a strict 1e-6
volume diff would manufacture false silent-wrong on every curved case. This is a
documented oracle scoping, not a hack: the on-surface residual ≤ `TAU_MODEL`
(§3.4) is a **stricter** exactness check than any volume number, and it is the
real correctness gate. The chord-band volume is only a gross-loss / dropped-chunk
backstop.

**Decision (Manager): adopt the chord-band envelope.** The strict-1e-6 and
drop-volume variants were considered and rejected (strict-1e-6 → false positives;
drop-volume → loses the dropped-chunk backstop). RED implements the chord-band
envelope and documents the derivation in the module doc.

## 5. What to build — `crates/yang-rs/tests/fuzz_curved.rs` (NEW, ~600–750 LOC)

Mirror `fuzz_boxes.rs` structure:

1. **Module doc** — the anti-requirement (0 silent-wrong), the robustness-envelope
   report (taxonomy split by primitive AND op), determinism note, explicit
   statement of N (not silently truncated), and the curved-oracle decision (§4).
2. **Determinism** — hand-rolled `SplitMix64` (copy from fuzz_boxes), fixed
   recorded `SEED`. No `rand`, no system time, no FS side effects. `range(lo,hi)`
   + `quat_to_mat3` / `Mat3` / `mat_vec` rigid transform (copy from fuzz_boxes).
3. **N** — `N ≈ 300` total cases (each picks a random primitive ∈ {cylinder,
   sphere, cone} and random op ∈ {Union, Subtract}). Document the count
   explicitly and that it is not silently truncated. Heavy (`#[ignore]`d) —
   ~1200 sidecar spawns; some 30 s timeouts possible. (If runtime proves
   excessive, drop to ~240 and document the change.)
4. **Generators** — `cylinder_brep` / `sphere_brep` / `cone_brep` copied from the
   reference test files; random radius/height/half-angle within sane in-scope
   ranges (r∈[0.2,0.6], h∈[0.8,2.0], half-angle∈[0.2,0.6] rad), a random rigid
   transform (splitmix64 quaternion), and a random box (reuse `OrientedBox` /
   `gen_box`). Sizes/offsets chosen so the primitive interpenetrates the box
   (overlap near-certain).
5. **Audit helpers** — copy `signed_volume`, `unpaired_half_edges`,
   `euler_characteristic` from fuzz_boxes; copy the array-math
   (sub/add/scale/dot/cross/norm/unit) from yr8/yr17. Add on-surface-residual
   sampling over output `Curve` edges using `yang_rs::signed_distance_to_surface`.
6. **Classifier + buckets** — extend `err_variant_name` to the 11 variants **and**
   the sub-reasons; `Buckets` keyed by `(primitive, op)` with `ok_correct`,
   `ok_multi_shell` (χ≠2 valid), `silent_wrong`, `skipped_bad_input` (sidecar
   errors on the reference call itself / timeouts on the ref), and per-`Err`-variant
   tallies. Record `SilentWrong { seed, case, primitive, op, params, vol_y,
   vol_ref, unpaired, euler, euler_ref, residual_max, … }` for the panic message.
7. **Harness** — env-gate on `SidecarBoolean::from_env()` with a **LOUD** skip
   (mirror fuzz_boxes — the whole test self-skips when absent). For each case:
   build the two BReps (skip `skipped_bad_input` if `BRep::new` errors), get
   `sidecar_direct = sb.boolean(...)` for the reference mesh (skip if it
   errors/times out), then `boolean(prim, box, op, sb)`: audit every `Ok`,
   classify every `Err`.
8. **Report + asserts** — print the histogram (by primitive, by op, totals);
   `assert_eq!(total_silent, 0, …)` with full case dump; assert every `Err` was
   classified (no "unknown" bucket). Per the CI-gate clause, if real silent-wrong
   cases are found, they are documented (seeds in module doc + commit msg) and the
   demonstrators `#[ignore]`d so the default suite stays green — the findings are
   the deliverable that drives the follow-up.

## 6. Roles (P5 — distinct sub-agents per role; all on `main`, no branches)

- **Spec (Manager)** — this document. Commit (docs).
- **RED (sub-agent A)** — author `crates/yang-rs/tests/fuzz_curved.rs` (TESTS
  ONLY). Run it with the sidecar. Report the taxonomy + any
  silent-wrong/unclassified findings verbatim. RED owns the harness and the
  `#[ignore]`/demonstrator structure. Commit (test).
- **GREEN (sub-agent B, ≠ A)** — ONLY IF RED surfaces a silent-wrong case that is
  genuinely out-of-scope AND a *minimal* loud classified `Err` in
  `crates/yang-rs/src/lib.rs` cleanly converts it: add that guard (justified,
  minimal, no hack-to-green). Otherwise GREEN independently re-runs, confirms
  correct-or-loud holds, and confirms no production change is warranted. The
  implementer must NOT edit the test file. Commit (green) only if a production
  guard was added.
- **Adversary (sub-agent C, ≠ A,B)** — independently audit that the harness
  genuinely catches silent-wrong: are the invariant checks real (χ from
  topology, on-surface ≤ `TAU_MODEL`, exact-param survival)? Can a silent-wrong
  slip past (e.g. a watertight + on-surface result that dropped a chunk → does
  the chord-band volume catch it)? Is determinism real? Verify any `#[ignore]`
  demonstrator truly reproduces a documented seed and isn't hiding a classifiable
  case. Non-destructive git only. Commit (adversary).

## 7. Files

- **NEW** `crates/yang-rs/tests/fuzz_curved.rs` — the harness (RED).
- **NEW** `specs/yang_pr_cf1_curved_boolean_fuzz.md` — this spec (Manager).
- **EDIT (only if RED finds a convertible silent-wrong)**
  `crates/yang-rs/src/lib.rs` — a minimal loud classified `Err` guard (GREEN).
  Default expectation: NO change.
- **EDIT** `docs/yang_functional_roadmap.md` — record PR-CF1: the
  correct/loud-Err/silent-wrong tallies + the `Err` taxonomy; silent-wrong
  findings (if any) become the next increment.

## 8. Verification / CI gate (FULL crate)

1. `cd /home/claude/workspace`
2. `cargo test -p yang-rs` — all prior tests unregressed + `fuzz_curved` compiles
   (fuzz is `#[ignore]`d so this stays green).
3. `CHERCHI2022_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
    cargo test -p yang-rs --test fuzz_curved -- --ignored --nocapture` — capture
   the histogram; confirm `SILENT_WRONG == 0` (or documented + demonstrators
   `#[ignore]`d) and every `Err` classified.
4. `cargo fmt -p yang-rs -- --check`
5. `cargo clippy -p yang-rs --all-targets -- -D warnings`
6. Update `docs/yang_functional_roadmap.md`; commit all phases; push `origin/main`.

**Done ::=** harness lands; `cargo test -p yang-rs` green; deep fuzz run produces
the taxonomy and holds correct-or-loud (or documents silent-wrong seeds +
`#[ignore]`s demonstrators); fmt + clippy clean; roadmap updated; pushed.
