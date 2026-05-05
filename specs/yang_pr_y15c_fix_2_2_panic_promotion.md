# PR-Y15c-fix-2.2 — Promote A15.5 surface_map miss to panic

**Status:** SPEC (FIP §8). **Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` 0a. **Wrong-anchor: 0/3.**

## 1. Defect statement

Audit `docs/audits/pr_y15c_fix_2_1_diagnostic.md` headline (verbatim): **"0 `[a15-5-fallback]` fires across the full 190-case corpus"** (745 successful `result_topology_to_waffle_solid` invocations × 0 fallback hits). A15.5 (`governance/ARCHITECTURAL_INVARIANTS.md:453-472`) verbatim: *"Boolean operations must preserve surface tier for unmodified faces. […] **Implementation**: The boolean pipeline's face classification step must carry forward the original `SurfaceGeom` when assembling unmodified faces into the result solid."* The Branch Table in `specs/yang_face_geometry_propagation.md` prescribes lookup-first; PR-Y15c-fix-2.1 proved the fallback arm is structurally unreachable.

## 2. Hardening rationale

Silent `unwrap_or_else` Newell-fallback masked an A15.5 violation (cylindrical-tag erosion on F0031–F0040) for ~2 months until adversary-6 caught it. PR-Y15c-fix-2 fixed lookup-first; PR-Y15c-fix-2.1 audited (0/190); PR-Y15c-fix-2.2 deletes the fallback arm so any future drift in `surface_map` population or `face_provenance` shape trips loudly. **Contract-hardening PR, not a defect-currently-firing PR.** Per FIP P9, hard panic > silent planar-fallback (panic surfaces data loss; fallback masks it).

## 3. Fix anchor site

`crates/kernel/src/boolean/yang_integration.rs:244-281` inside `result_topology_to_waffle_solid`'s `face_geometry` loop. Delete L248-258 (audit probe, 11 LOC) + L259-281 (Newell-fallback + degenerate-skip guards, 23 LOC); replace with `surface_map.get(...).unwrap_or_else(|| panic!(...))`. Net ~−24 LOC kernel.

## 4. Expected invariant

**I1.** For every `(face_idx, source)` in `result.face_provenance.iter()`, `surface_map.get(&(source.mesh_id, source.face_idx))` returns `Some`. Violation → panic with diagnostic message containing `face_idx`, `source.mesh_id`, `source.face_idx`, `surface_map.len()`, audit-memo path (`docs/audits/pr_y15c_fix_2_1_diagnostic.md`), and spec link (`specs/yang_pr_y15c_fix_2_a15_5_surface_preservation.md`). **I2.** Corpus pass count ≥11 (no new panics).

## 5. Test plan (FIP §4.2)

test-author-b (NEW per FIP §1 + §8) adds ONE `#[should_panic(expected = "A15.5")]` test `test_a15_5_panic_on_missing_surface_map_entry` in the existing `#[cfg(test)] mod tests` of `yang_integration.rs` (~L1504+). Construct minimal `ResultTopology` with one `face_provenance` entry whose `(mesh_id, face_idx)` is absent from an empty `surface_map`; call `result_topology_to_waffle_solid`; assert panic. **RED-phase demonstration:** on current (post-PR-Y15c-fix-2.1) code the fallback silently Planars → `#[should_panic]` catches the missing panic and FAILS. After fix, panic fires → test PASSES. **GREEN regression:** existing 5 tests in `crates/test-harness/tests/pr_y15c_fix_2_surface_preservation.rs` MUST continue to pass unmodified (per 0/190 audit, panic does not fire on real cases). Constructor caveat: test-author-b reads existing tests at L1504+ for the `ResultTopology` field shape; if blocked, escalate to team-lead before constructing a fragile mock. test-author-b does NOT touch any kernel logic.

## 6. Adversarial scope (FIP §4)

adversary-9 (NEW; full role rotation per `feedback_oracle_credibility_via_role_separation.md` — NOT spec-writer-j / test-author-b / implementer-m / any prior PR-Y15c-fix adversary) writes `docs/audits/pr_y15c_fix_2_2_validation.md`. Verifies: (a) **mutation test** — revert fix; the `#[should_panic]` test FAILS (panic is load-bearing); (b) **full 190-case corpus sweep** — no new panics, pass count ≥11; (c) **panic-message inspection** — face_idx + source + map_size + audit-memo path + spec link present; (d) **WASM panic propagation** — verify panic downcasts cleanly through `catch_unwind` at `crates/wasm-bridge/src/wasm_api.rs:52` so JS-side error is human-readable. Per `feedback_adversary_recommendations_need_canary.md`, any next-step recommendation MUST be self-canary-verified.

## 7. Anchor pre-verification canary

Per `feedback_anchor_before_fix.md`, implementer-m adds at `yang_integration.rs:210` BEFORE writing fix code:

```rust
eprintln!("[fix22-canary] result_topology_to_waffle_solid invoked face_count={}", result.face_provenance.len());
```

Run `YANG_BOOLEAN=1 cargo test -p test-harness --test pr_y15c_fix_2_surface_preservation -- --ignored --nocapture`; verify per-case fires. **ABORT (P10) if 0 fires for any case** — invalidates the PR-Y15c-fix-2 → fix-2.1 → fix-2.2 chain; report to team-lead. Risk is exceedingly low (PR-Y15c-fix-2.1's canary proved this path hot at 745 invocations) but discipline is mandatory. Remove canary BEFORE fix lands (byte-clean diff, DoD §6).

## 8. FIP role table

| Sub-phase | Agent | Writes |
|---|---|---|
| 0a Spec | spec-writer-j (NEW) | THIS spec |
| 0b Test | test-author-b (NEW; NOT spec-writer-j / implementer-m per FIP §1 + §4.1) | `#[should_panic]` test in `yang_integration.rs` test mod |
| 0c Fix | implementer-m (NEW; NOT spec-writer-j / test-author-b) | Canary @ L210, verify, remove; fix @ L244-281 |
| 0d Adversary | adversary-9 (NEW; full role rotation) | `docs/audits/pr_y15c_fix_2_2_validation.md`; mutation test; corpus sweep |
| 0e Commit | team-lead | WASM rebuild + memory + git commit + push |
