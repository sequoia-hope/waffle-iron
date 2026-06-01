# PR-YR11 FINISH — contained oblique fixture + corner loud-STOP (Option 1)

> Manager spec of record for the role-separated FIP cycle (Constitution P5):
> Spec (this doc) → RED (test-author sub-agent) → GREEN (implementer sub-agent)
> → Adversary (fourth sub-agent). The implementer NEVER edits tests; the test
> author NEVER writes production code; the adversary uses independent oracles
> that share no code with GREEN. Stay on `main`; commit each phase; push at end.
> Paper citations are line ranges in `refs/text/yang2025_hybrid_boolean.txt`.
> Supersedes nothing in `specs/yang_pr_yr11_stage4_oblique_ellipse.md`; this
> doc only resolves the single remaining out-of-scope `t5` failure.

## 1. Where PR-YR11 stands

PR-YR11 (Stage-4 oblique-ellipse relocation) is committed and **the whole
`yang-rs` crate is GREEN except one test**. The Stage-4 ellipse relocation, the
`eval_source` `Curve::Ellipse` arm, the `is_reversed` ellipse tangent, the N3
degenerate-tangent fix, and the yr10 ellipse-migration are all implemented and
passing. Verified at execute start:

- `cargo test -p yang-rs`: `t1`–`t4` green; `t5_e2e_oblique_cylinder_union_box_on_ellipse`
  **FAILS** at vertex `[1.0, 0.251, 1.0078]`, plane residual `0.0078 > TAU_MODEL`.
  (Note: the sidecar binary exists at its default path, so `t5` is *exercised*,
  not skipped, under plain `cargo test` — the env-gate only LOUD-skips when the
  binary is absent. The CI gate is therefore stronger than the plan assumed.)

## 2. Root cause (confirmed — NOT a production bug)

The canonical oblique fixture `oblique_cylinder()` uses `dir = unit([0.5,0,1])`,
whose axis drifts `+0.5` in `x` across `z ∈ [0,1]`. The top-cap ellipse center
lands at `x ≈ 1.026` — **outside** the unit box — so the cylinder pokes out the
`x = 1` side face. The failing vertex `[1.0, 0.251, 1.0078]` is a **side-face
ellipse** point that production *correctly* relocated onto `x = 1` (giving
`z = 1.0078`), but the test's cap-only oracle measures `plane_residual` against
`z = 1` and flags it.

This **side-face-exit / corner case is outside the approved scope** of "plane ∩
cylinder → single cap ellipse". It is genuine §4.5.2 local-refinement territory
(a triple point where two intersection curves on *different* cutting planes
meet), deferred — not a regression.

## 3. Outcome (Option 1, approved)

Two coordinated changes:

1. **Migrate `t5` to a *contained* single-ellipse oblique fixture** (in scope).
   Faithful contract migration: only the fixture geometry changes; **every
   structural assertion is preserved** (`Ok`, watertight χ=2, ≥1 `Curve::Ellipse`
   edge, both on-surface residuals ≤ `TAU_MODEL`, determinism, loud skip).
2. **Add a Stage-4 LOUD-STOP guard** so the out-of-scope side-face-exit/corner
   case returns a clear `Err` instead of a wrong-plane vertex.

Supporting facts (verified):

- **SSI classifies any non-trivial tilt as `Ellipse`** (`crates/ssi-rs/src/lib.rs`
  C2 branch fires whenever `|cos| ≤ 1 − TAU_MODEL`). So a reduced-tilt,
  recentered cylinder still yields ellipse caps → the contained fixture keeps the
  **unit box** and changes only the cylinder. Cap planes stay `z = 0` / `z = 1`.
- The Stage-4 relocate lives in `stage4_relocate_and_correct`
  (`crates/yang-rs/src/lib.rs:2530`), which already builds a per-vertex
  `vert_ellipse: BTreeMap<u32, EllipseReloc>` from the incidence map and already
  has a **precedent guard** at `lib.rs:2637-2644` ("vertex in both `vert_circle`
  and `vert_ellipse` ⇒ loud STOP"). The new corner guard is a sibling of that.
- `Stage4InvalidReason` enum is at `crates/yang-rs/src/lib.rs:1840`.

## 4. Fixed contract (decouples RED from GREEN)

The new error variant name is **fixed up front** so RED can assert it before
GREEN exists:

```
Stage4InvalidReason::IntersectionCornerUnsupported
```

returned as
`Err(YangError::Stage4RegionInvalid { reason: Stage4InvalidReason::IntersectionCornerUnsupported, .. })`.

Doc'd as a P9/P10 loud stop: an ellipse intersection edge meets another
intersection curve on a *different* cutting plane (a corner / triple-point
needing §4.5.2-style local refinement; out of scope for oblique cyl ∪ box).

## 5. RED phase — `crates/yang-rs/tests/yr11_stage4_ellipse.rs` (sub-agent A only)

1. **Add a contained oblique fixture**, reusing the unit `canonical_box()`:
   - `contained_oblique_cylinder()`: tilt `dir = unit([0.25,0,1])` (clearly
     oblique → SSI `Ellipse`), `r ≈ 0.18`, axis **centered** through the box
     (axis passes `(0.5, 0.5)` at `z = 0.5`), `h ≥ 3` so it protrudes top &
     bottom. Both cap ellipses then lie fully inside `[0,1]²` with margin and the
     lateral wall never reaches `x = 0` / `x = 1` over `z ∈ [0,1]`. (RED author
     tunes the exact numbers; verify containment numerically in the fixture.)
   - Contained-config residual oracles (`contained_cyl_radial_residual`,
     `contained_plane_residual`) referencing the contained axis/dir/radius.
     `cap_plane` / `cap_z_of` reused unchanged (caps still `z = 0` / `z = 1`).
2. **Rewrite `t5`** to use the contained fixture. **Preserve every structural
   assertion**: `Ok`, watertight χ=2 (`unpaired_half_edges == 0`,
   `euler_characteristic == 2`), ≥1 `Curve::Ellipse` edge, both on-surface
   residuals ≤ `TAU_MODEL`, determinism. Keep the env-gate + **loud skip**.
   **Iterate against the real sidecar until it passes honestly** — no
   `TAU_MODEL` loosening, no assertion weakening.
3. **Add `t6_e2e_side_face_exit_corner_loud_stop`** using the **old** side-exit
   geometry (`oblique_cylinder()` + `canonical_box()`) via the real sidecar,
   asserting `boolean(...)` returns
   `Err(YangError::Stage4RegionInvalid { reason: Stage4InvalidReason::IntersectionCornerUnsupported, .. })`.
   Env-gated, loud skip. (RED state: currently returns `Ok` → fails until GREEN.)

Do NOT weaken `t1`–`t4`, the yr10 circle/migration tests, or planar `fuzz_boxes`.

## 6. GREEN phase — `crates/yang-rs/src/lib.rs` (sub-agent B only)

1. **Instrument-first (anchor verification before coding).** With the sidecar
   set, dump every ellipse edge's incident cutting plane + endpoints for **both**
   the side-exit (canonical) and contained configs. Confirm a detection that
   **provably fires** on side-exit and **provably does NOT** fire on:
   - the contained config (two *disjoint* cap-plane ellipses — two distinct
     planes but no shared vertex),
   - the `t1`–`t4` mocks, the circle path, and planar `fuzz_boxes`.
2. **Add `Stage4InvalidReason::IntersectionCornerUnsupported`** (doc'd as above).
3. **Add the guard** in `stage4_relocate_and_correct`'s conic-collection loop
   (sibling to the existing circle∩ellipse ambiguity guard). Candidate detection
   (confirm via step 1): a mesh vertex that is an endpoint of intersection edges
   lying on **≥2 distinct cutting planes** ⇒ return the loud `Err`. Must NOT fire
   on disjoint two-cap configs (whose shared vertex set across the two cap planes
   is empty).
4. **Preserve** the N3 degenerate-tangent reversal (commit `a0ba8f59`) and the
   circle branch byte-for-byte. Keep the now-unused `EllipseProjectionUnsupported`
   variant (it is `pub`, harmless).
5. **STOP and report** if no principled detection cleanly separates contained
   from side-exit — do NOT invent a config-specific "is-this-a-cap" heuristic
   (P9/P10).

## 7. Adversary phase (sub-agent C — independent oracles, no shared code with GREEN)

- Re-derive the contained ellipse from first principles (hand-computed
  `a = r/|cos tilt|`, `b = r`, `major_axis`, center = axis ∩ plane) — NOT via
  `ssi_rs::intersect` — and assert every relocated crossing lies on it + the true
  cylinder/plane ≤ `TAU_MODEL`.
- In-plane fold disproof (winding/signed-area sweep) + simple once-wrapping
  tangent-ordered ring on the contained cap.
- Independently confirm the guard fires on side-exit and not on contained /
  circle / planar.
- Verify the yr10 migration and `t1`–`t4` structural assertions were not weakened.

## 8. Verification / CI gate (Manager)

1. `cargo test -p yang-rs` — fully green, 0 failures (exercises `t5`/`t6` since
   the sidecar is present at its default path).
2. Re-run with `CHERCHI2022_BIN` explicitly set (belt-and-suspenders).
3. `cargo fmt -p yang-rs -- --check`.
4. `cargo clippy -p yang-rs --all-targets -- -D warnings`.

## 9. Honesty guardrails

No `TAU_MODEL` widening, no assertion weakening, no fallback path. If a contained
config cannot pass honestly, or the guard cannot be cleanly localized to a
principled signal → **STOP and report** (that is itself the finding). P9/P10.
