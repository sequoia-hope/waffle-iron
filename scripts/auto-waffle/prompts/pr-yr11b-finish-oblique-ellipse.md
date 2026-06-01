# PR-YR11 finish — contained oblique fixture + corner loud-STOP (Option 1, approved)

Context: PR-YR11 spec + RED + GREEN (WIP) are already committed on `main`
(`1d8b2af3`, `3c5cecb5`, `1a695eda`). The oblique-ellipse Stage-4 relocation
WORKS for clean ellipses — the 4 direct (non-sidecar) ellipse oracles pass, and
the N3 degenerate-tangent fix + circle/perpendicular case + planar `fuzz_boxes`
are all green. **Only one test fails: `t5_e2e_oblique_cylinder_union_box_on_ellipse`**
(`crates/yang-rs/tests/yr11_stage4_ellipse.rs`), the real-sidecar E2E. The
canonical oblique config pokes out the box's **x=1 SIDE face**, so an
intersection vertex lands on a side plane (residual 0.0078 at
`[1.0, 0.251, 1.008]`) — a corner/triple-point case **outside** the approved
"plane∩cylinder → single ellipse" scope.

**This is a small, focused FINISH cycle. Option 1 is APPROVED — do exactly this:**

1. **Redesign the `t5` fixture to a CONTAINED single-ellipse oblique cap.** Choose
   an oblique cylinder + box where the cylinder crosses **one cap face** in a
   single ellipse that lies **fully inside that face**, with **no side-face exit**
   (e.g. enlarge the box and/or shorten / reposition / reduce the tilt of the
   cylinder so its body stays within the box's x-y extent over the box's
   z-range). This is a faithful contract migration of `t5` (the RED author's
   recommendation): keep every structural assertion (Ok, watertight χ=2, ≥1
   `Curve::Ellipse`, on-both-surfaces to `TAU_MODEL`, chord-deviation drop,
   determinism); only the *fixture geometry* changes so it stays in scope.
   **Iterate against the real sidecar** (`CHERCHI2022_BIN`) until t5 passes
   honestly — do NOT loosen `TAU_MODEL` or weaken an assertion to force it.
2. **Add a corner / side-face-exit LOUD-STOP guard** in Stage 4: when an
   intersection edge's relocation would place a vertex off the intended cap plane
   (i.e. the oblique cut meets a side face / produces an ellipse∩line corner or
   ellipse∩ellipse triple-point), return a loud `Err` (e.g.
   `Stage4RegionInvalid { … }` with a clear reason) rather than emitting a
   wrong-plane vertex. Add a small test asserting the out-of-scope config (the
   *old* side-face-exit geometry) now returns that loud STOP — so the boundary is
   documented by a test, not silently mishandled.
3. **Preserve the N3 fix** (degenerate-tangent ⇒ reversal) and the circle case —
   do not regress them.

Then run a distinct **Adversary** pass (independent geometric-validity audit of
the contained-ellipse relocation + the corner-STOP guard), commit (GREEN finish +
adversary), and push.

CI gate (FULL crate): `cargo test -p yang-rs` (whole crate — must be fully green,
0 failures), `cargo fmt -p yang-rs -- --check`, `cargo clippy -p yang-rs
--all-targets -- -D warnings`. If a contained single-ellipse oblique config that
passes honestly cannot be found, **STOP and report** (do not force t5 green) —
that would itself be a finding.

On completion: update `docs/yang_functional_roadmap.md` (PR-YR11 done — oblique
cylinder∪box conforms to the exact ellipse; side-face-exit/corner is a loud STOP,
deferred) and note the resolved scope in `docs/yang_deviations.md` if apt.
