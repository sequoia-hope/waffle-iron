# PR-YR14 — yang-rs curved Subtract THROUGH-HOLE: box − cylinder tunnel (genus 1)

Context: PR-YR13 implemented `box − cylinder` as a **blind pocket** (genus 0,
χ=2) with curved cavity-sense via `BRepFace.reversed` (set from `op == Subtract
&& info.input == B`, the same `flip_for_op` signal as the mesh winding). This PR
extends to a **THROUGH-hole**: the cylinder passes fully through the box → a
cylindrical tunnel, which is **genus 1 (χ = 0)**.

## The crux — generalize the per-shell Euler gate
`check_watertight_2manifold` (lib.rs ~1721) asserts **`V − E + F == 2` per
connected shell** ("each must be a sphere, χ=2") and returns
`NonManifoldOutput` otherwise. A through-hole box is a single genus-1 shell with
**χ = 0**, so today it is WRONGLY rejected. Generalize:
- A closed orientable 2-manifold shell has **χ = 2 − 2g** for integer genus
  `g ≥ 0` ⇒ χ is **even and ≤ 2**. Accept any such χ; compute `g = (2−χ)/2`.
- **Keep the gate strict:** the directed edge-pairing check (lib.rs ~1715, every
  half-edge paired) is genus-independent and STAYS — it catches true
  non-manifold. Additionally **reject odd χ or χ > 2** (impossible for a closed
  orientable manifold → a real defect). Do NOT just delete the χ check.
- Through-hole → χ=0 (g=1) now passes; sphere χ=2 (g=0) still passes.

## What's new vs YR13 (blind pocket)
- Genus-1 topology (χ=0), single shell.
- **Two** rim `Circle` intersection edges: cylinder ∩ box-top AND cylinder ∩
  box-bottom (both exact, via the existing P3/Stage-4, op-agnostic).
- The tube **cavity wall** (cylinder lateral) spans the full box thickness; **no
  pocket floor** (the cylinder's caps lie outside the box, not on the result
  boundary). `reversed` reused unchanged (the tube wall is a cavity → `reversed
  = true`, effective outward normal toward the axis).

## Scope
- **`box − cylinder` through-hole only** (cylinder perpendicular, taller than the
  box, fully penetrating). Deferred: sphere/cone cavities, the box-subtracted
  case, oblique through-holes. `Cone` still rejects loudly.
- Union + planar Subtract + YR13 blind-pocket paths must stay byte-for-byte.

## Oracle (RED contract)
1. **Through-hole succeeds** and is **watertight 2-manifold with χ = 0** (assert
   genus 1 explicitly; 0 unpaired half-edges).
2. **Gate not weakened** (adversary): a defect-injected mesh with odd χ or χ > 2,
   or an unpaired half-edge, still returns `NonManifoldOutput`.
3. **Cavity-sense**: the cylinder tube wall is `Surface::Cylinder` (input params),
   `reversed == true`; sampled effective outward normal points toward the axis.
4. **Two exact `Circle` rim edges** (top + bottom), each on both the cylinder and
   its box-face plane to `TAU_MODEL`.
5. **Sidecar `Subtract` mesh-parity** (env-gated, LOUD skip); determinism;
   YR13 blind-pocket + curved Union (YR8–YR12) + planar `fuzz_boxes` unregressed.
   Sidecar-independent direct path for the GREEN gate.

**STOP-and-report (P9/P10)** if the genus-1 output can't be made valid without
weakening the manifold/χ gate, or the two-rim reassembly can't close honestly.

## CI gate (FULL crate)
`cargo test -p yang-rs` (whole crate — the χ-gate change is shared code that can
regress any genus-0 test; do NOT scope to the new file), `cargo fmt -p yang-rs --
--check`, `cargo clippy -p yang-rs --all-targets -- -D warnings`. Watch for a
cross-suite contract conflict (a sibling test asserting the old strict `χ == 2`
shape — migrate faithfully).

On completion: update `docs/yang_functional_roadmap.md` (PR-YR14 — through-hole
genus-1 Subtract; per-shell Euler gate generalized to χ=2−2g). Remaining:
sphere/cone cavities, the side-face/corner guard.
