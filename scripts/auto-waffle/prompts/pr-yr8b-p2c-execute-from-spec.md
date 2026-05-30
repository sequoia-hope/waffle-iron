# PR-YR8 (P2c) — EXECUTE from the existing spec: first curved boolean (cylinder ∪ box)

**A complete spec of record already exists at
`specs/yang_pr_yr8_curved_boolean.md`** (committed `4b040724`). It was written in
a prior planning pass that ran out of the planning-phase time budget *after*
finishing the spec but *before* starting implementation. **READ that spec and
execute it. Do NOT re-derive, rewrite, or second-guess it** — it is the plan of
record. Spend your planning turn confirming the spec against the current code and
presenting a SHORT execution plan, then proceed straight to the role-separated
FIP cycle. The slow part (analysis) is already done; your budget is for RED +
GREEN + Adversary.

The spec is self-contained; in particular follow:
- **§3** current state (verified line numbers in `crates/yang-rs/src/lib.rs`).
- **§4** the two blockers and their HONEST fixes:
  - Blocker 1: per-face face-resolution tolerance — `TAU_WORK` for `Plane`,
    the Stage-1 chord bound `d_ε` for curved faces; refactor `d_ε` into ONE
    shared helper used by both Stage 1 and face resolution (A14.3). All-planar
    inputs MUST stay byte-for-byte identical.
  - Blocker 2: a curved-surface branch in `reconstruct_topology` that inherits
    `Surface::Cylinder` unchanged (Union → no sense flip), reuses
    `patch_boundary_cycle`, assigns outer/inner loops deterministically, edges =
    `Curve::LineSegment`. Sphere/Cone still loudly reject.
- **§5** STOP-and-report conditions — honor them exactly (P9/P10). If the
  canonical `cylinder ∪ box` hits an F3 tie, a non-manifold lateral patch, or a
  non-watertight union that the spec's fixes don't resolve, **HALT and report the
  specific gap** — no tolerance widening, no fake closure, no wrong shell.
- **§6** the oracle (`tests/yr8_curved_boolean.rs`) — implement ALL 7 items,
  including the **sidecar-independent direct path** (call `reconstruct_topology`
  / face resolution directly with hand-built fixtures) so the in-environment
  GREEN gate does not depend on the sidecar binary. Env-gate the sidecar E2E
  oracles with a logged skip — never silently pass. Do the **faithful contract
  migration** of existing tests that assert the cylinder path returns
  `CurvedSurfaceNotYetSupported` (preserve structure; Adversary verifies not
  weakened).

Hard scope (unchanged from the spec): **no `ssi-rs` import/call**, intersection
edges stay `Curve::LineSegment` (exact curves are P3), cylinder+box only, Union
is the required asserted case, sphere/cone stay loudly rejected, do not rewrite
the planar path.

CI gate: **FULL** `cargo test -p yang-rs` (a Stage-5/6 change regresses the planar
boolean if wrong — run the whole crate, not just the new file) + `cargo fmt -p
yang-rs -- --check` + `cargo clippy -p yang-rs --all-targets -- -D warnings`, all
clean.

On completion: update `docs/yang_functional_roadmap.md` (PR-YR8/P2c done — first
curved boolean, cylinder ∪ box, mesh-approximate, analytic `Surface::Cylinder`
survives; remaining Phase-2 work = P2b sphere + P3 Stage-3 ssi-rs wiring for exact
edges). If you hit a §5 STOP condition, record the gap there instead and commit
the partial honestly.
