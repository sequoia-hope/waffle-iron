# PR-YR20 — Stage-6 tiered face-resolution tie-break

Spec of record for the role-separated FIP cycle. Manager (this file) → RED
sub-agent → GREEN sub-agent → Adversary sub-agent. Test author never writes
production code; implementer never edits tests. All work stays on `main`.

## 1. Problem

The curved fuzz's largest **non-cone** refusal bucket is `FaceResolutionFailed`.
A driver investigation (env-gated prints, since reverted) found 12/12 sampled
cases share ONE uniform root cause, and it is NOT a no-match.

Stage-6 geometric face resolution (`crates/yang-rs/src/lib.rs`, the
non-degenerate branch ≈ lines 3217–3241) attributes a kept triangle to the input
face whose surface contains the triangle **centroid** within that face's per-face
tolerance `tol_for` — `TAU_WORK` for a `Plane`, the Stage-1 chord band `d_ε` for
`Cylinder` / `Sphere` / `Cone`. It counts hits: exactly 1 → attribute; 0 or ≥2 →
`FaceResolutionFailed`.

Every curved-fuzz `FaceResolutionFailed` is an `n_hits == 2` tie of one shape:

```
Plane     dist = 5.5e-17   tol = 1.0e-12 (TAU_WORK)   HIT  ← centroid EXACTLY on a cap plane
Cylinder  dist = 7.6e-3    tol = 2.4e-2  (d_ε)        HIT  ← also within the loose curved band
```

A triangle lying **exactly on a planar cap near the rim** also falls inside the
curved lateral's necessarily-loose chord band → spurious second hit → tie → F3.
The rule wrongly treats an **exact** `TAU_WORK` planar hit and an **approximate**
`d_ε` chord-band hit as equal weight. The triangle's true face is the cap.

## 2. The fix — tiered tie-break (NOT tolerance widening; P9/P10)

Rank hits by **tier**, where the tier is determined by how tight a tolerance the
centroid satisfies:

- **EXACT tier**: `dist < TAU_WORK` — the centroid lies ON the surface to working
  precision.
- **BAND tier**: `TAU_WORK ≤ dist < tol_for(face)` — within the surface's Stage-1
  chord band but not exactly on it.

Attribute to the **unique hit at the minimum populated tier**:

| Condition | Result |
|---|---|
| exactly one EXACT-tier hit | attribute to it (dominates any BAND hits) |
| zero EXACT hits and exactly one BAND-tier hit | attribute to it |
| ≥2 EXACT hits | `FaceResolutionFailed` (genuine coplanar / multi-solid tie) |
| 0 EXACT + ≥2 BAND hits | `FaceResolutionFailed` (genuine curved tie) |
| 0 EXACT + 0 BAND hits | `FaceResolutionFailed` (no match) |

This is the natural generalization of the existing rule — **each face still uses
its own A14.3 single-source band** (`tol_for` is untouched). We only break ties by
the exact-vs-band tier the centroid satisfies.

### 2.1 Parameters

- `TAU_WORK` (`cad_primitives::TAU_WORK`, currently `1e-12`) — the EXACT/BAND tier
  boundary. NOT a new constant; it is the existing planar tolerance.
- `tol_for(fi, surface)` — the per-face A14.3 band. **Untouched.** Planar →
  `TAU_WORK`; curved → its Stage-1 chord bound (`cone_chord_bound`,
  `sphere_chord_bound`, cylinder band).

## 3. Why tier-by-distance, NOT the ratio variant

The brief offered two formulations ("unique minimum `dist/tol` ratio" OR "explicit
exact-tier-beats-band-tier"). **Tier-by-distance is the only one that preserves the
all-planar byte-identity**: a `dist/tol` ratio would distinguish two planar hits at
*different* sub-`TAU_WORK` distances (ratios 0.3 vs 0.7) and pick the closer one,
silently converting a current planar F3 into an attribution — breaking the safety
property. Tier-by-distance puts every planar hit in the same EXACT tier, so the
genuine planar tie still raises F3.

## 4. Invariants

### 4.1 Byte-identity for all-planar inputs (the critical non-regression)

For a `Plane` face `tol_for == TAU_WORK`, so a hit (`dist < tol`) means
`dist < TAU_WORK` ⇒ **always EXACT tier**; the BAND tier is unreachable for planar
faces. Therefore for an all-planar input:

- BAND tier is always empty (`n_band == 0`);
- `n_exact` == today's hit count;
- "exactly one EXACT hit" ≡ today's "exactly one face within `TAU_WORK`";
- "≥2 EXACT hits" ≡ today's "≥2 within `TAU_WORK`" → still F3 (genuine coplanar /
  multi-solid tie, correctly deferred to M8);
- "0 hits" → F3.

→ **byte-for-byte identical** to the current rule. The box fuzz, the m3
coplanar-tie tests, and the yr5c planar-sliver tests are unaffected. The fix can
**only** change a mixed exact-planar-vs-curved-band tie.

### 4.2 Degenerate-sliver branch (≈ lib.rs 3187–3216) — left UNCHANGED, justified

The brief asks to "consider whether" the degenerate branch needs the same tiering.
**Decision: leave it unchanged.** Rationale: (1) the finding is exclusively about
**non-degenerate** `n_hits == 2` ties; (2) the degenerate branch already never
raises F3 for a tie — it deterministically takes the lowest-index face within its
own tolerance, which its own comment documents as geometrically harmless for a
zero-area sliver; (3) minimal scope = minimal regression surface (P7; "Do NOT
change the attributed face of any triangle that is not currently a tie"). Changing
it is neither required for the metric nor risk-free, so we don't.

## 5. Production change (`crates/yang-rs/src/lib.rs`, non-degenerate branch only)

Replace the single `(n_hits, hit)` counter with two tier counters:

```rust
// PR-YR20 tiered tie-break: an EXACT membership (centroid within TAU_WORK of
// the surface — it lies ON it) dominates a within-chord-band membership. Each
// face still uses its own A14.3 band via tol_for; we only rank the tie by tier.
// For all-planar inputs every hit is EXACT (planar tol == TAU_WORK), so this is
// byte-for-byte the old "exactly one face within TAU_WORK" rule.
let mut exact_hit: Option<u32> = None;
let mut n_exact = 0usize;
let mut band_hit: Option<u32> = None;
let mut n_band = 0usize;
for (fi, f) in input_brep.faces().iter().enumerate() {
    let d = plane_dist(fi, f)?;
    if d < tol_for(fi, f.surface)? {
        if d < cad_primitives::TAU_WORK {
            n_exact += 1;
            if n_exact == 1 { exact_hit = Some(fi as u32); }
        } else {
            n_band += 1;
            if n_band == 1 { band_hit = Some(fi as u32); }
        }
    }
}
match (n_exact, exact_hit, n_band, band_hit) {
    (1, Some(fi), _, _) => fi,        // unique exact-tier hit dominates
    (0, _, 1, Some(fi)) => fi,        // no exact hit; unique band-tier hit
    _ => return Err(YangError::FaceResolutionFailed { tri: compact_t }),
}
```

`tol_for`, `plane_dist`, the band values, and the YR18/YR19 intersection-edge path
are **untouched**.

## 6. Oracles / calibrated success metric (avoids the "moved-the-failure" trap)

- Total `FaceResolutionFailed` → **~0** on the curved fuzz.
- **Cylinder `ok_correct` MUST rise** (clearing the cylinder cap-tie unblocks it).
- **Cone `ok_correct` stays 0 is EXPECTED** — cone is still blocked downstream by
  the deferred `AmbiguousCurve` analytic conics (`Parabola` / `Hyperbola`); a cone
  triangle that stops being an F3 tie simply refuses later for that deferred
  reason. That is correct.
- **ZERO new silent-wrong**, **no new `NonManifoldOutput`**.

## 7. RED contract (test-author sub-agent)

New `crates/yang-rs/tests/yr20_tiered_tiebreak.rs`, **deterministic** (no `rand`,
no system time, no FS). Model the fixture on an existing closed-cylinder
`LabelMock` driver (e.g. `tests/yr10_stage4_relocate.rs` / `tests/yr13_*` /
`tests/yr7_cylinder.rs` `cylinder_brep` + a `LabeledArrangement` mock).

A single triangle cannot be watertight, so the fixture is a **closed cylinder
boolean** in which one near-rim lateral-adjacent triangle has its centroid within
`TAU_WORK` of a cap plane AND within the lateral's `d_ε` band — i.e. an
`n_hits == 2` cap-vs-curved tie that currently raises `FaceResolutionFailed`.

Assertions (post-fix behaviour, these FAIL today):

1. `boolean()` returns `Ok`.
2. The identified near-rim triangle's `triangle_attribution` resolves to the
   **cap plane** face index (not the cylinder lateral).
3. Output is watertight 2-manifold; Euler `χ = 2 − 2g` holds for the expected
   genus; **`signed_volume > 0`** (orientation witness — hand-built mocks can pass
   watertight+χ while globally inside-out; memory `yang_mock_orientation_witness`).

Also include the **safety canary**: a genuine **all-planar coplanar tie** (model on
`m3_adversary.rs::a6_equidistant_two_planes_tie_fails_resolution`) that MUST still
raise `FaceResolutionFailed` — proving the EXACT-tier tie path is intact.

RED author runs the new test against unmodified `src`, records that the cap-tie
case currently fails with `FaceResolutionFailed` (the RED state) and the canary
already passes.

## 8. GREEN (a DIFFERENT sub-agent — implementer)

Apply §5 exactly. No other production edits; no test edits. Must make the RED test
pass while keeping the full crate suite green. If the diagnosis turns out wrong or
a genuine conflict appears, **STOP and report** (P9/P10) — no improvised
alternative, no tolerance widening, no fallback path, no special-casing.

## 9. Adversary (a THIRD sub-agent)

Independently verify, **without editing production code**:

- **Byte-identity**: `cargo test -p yang-rs` whole crate green; specifically the
  all-planar coplanar-tie / m3 / yr5c sliver / `fuzz_boxes` tests behave
  identically. Argue from the code why all-planar is byte-identical.
- **No weakened migration**: confirm no prior test's structural assertions were
  loosened (only the curved-tie *expected outcome* may change; planar F3 tests
  unchanged). Confirm the strict-F3 tests that survive (`lib.rs` off-all-planes;
  `yr19_adversary` T1) are `n_hits==0` no-match cases, hence still F3.
- **Anti-fabrication**: include the verbatim `git diff` of the production change.
- **Curved-fuzz delta**: if the Cherchi sidecar runs in-container, report total
  `FaceResolutionFailed → ~0`, cylinder `ok_correct` rise, zero new silent-wrong.
  If the sidecar zombies out (known in-container blocker, memory
  `curved_fuzz_sidecar_zombie_blocker`), say so honestly and do NOT fabricate
  numbers — the driver reproduces the delta post-merge.
- May add one extra adversarial fixture (e.g. a 0-EXACT + 2-BAND genuine curved
  tie that MUST still F3) if not already covered.

## 10. Verification / CI gate (FULL crate)

- `cargo test -p yang-rs` (whole crate; all prior tests unregressed).
- `cargo fmt -p yang-rs -- --check`.
- `cargo clippy -p yang-rs --all-targets -- -D warnings`.

Sidecar (driver-only if it runs):
`CHERCHI2022_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`
`CHERCHI2022_INPUTCHECK_BIN=…/mesh_booleans_inputcheck`

## 11. STOP conditions (P9/P10)

If the RED fixture cannot reproduce an `n_hits==2` cap-vs-curved tie
deterministically, or the GREEN change does not flip it to a cap attribution
without a second edit, or any planar test changes byte-output — **STOP and report**.

## 12. Deviation record

`docs/yang_deviations.md` gains **N12** (the tiered exact-over-band Stage-6
face-resolution tie-break — a tie-ranking generalization of the single-band
membership test, not a new looser constant; cross-references N4, the centroid
attribution it refines). `docs/yang_functional_roadmap.md` records PR-YR20 under
M5 (the all-planar byte-identical argument; the calibrated metric — cylinder
`ok_correct` rises, cone stays deferred on analytic conics).
