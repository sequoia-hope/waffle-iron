# PR-YR13 — yang-rs curved Subtract: box − cylinder blind pocket (cavity-sense)

Context: curved **Union** works end-to-end (cylinder/sphere tessellation YR7/YR12;
`cylinder ∪ box` exact edges + relocation YR8–YR11). `Subtract` works at the
**mesh** level — `flip_for_op` (lib.rs ~2221) already flips the cavity-bounding
kept triangles' winding for `BoolOp::Subtract` (mirroring Cherchi
`boolSubtraction`), so the output MESH is correct. The gap is the **B-Rep face
sense for CURVED cavity walls**: `reconstruct_topology` inherits the curved
`Surface` UNCHANGED (lib.rs ~3399, the Union assumption), and curved `Surface`
has **no way to encode "cavity"** (its outward side is canonically the
solid-primitive side — away from center/axis). So curved Subtract today silently
gives a cavity wall the WRONG (outward) analytic sense. This PR fixes that.

**Plan approved (Option A — explicit sense flag).** Do exactly this.

## Scope
- **`box − cylinder`, BLIND POCKET only** (genus 0, χ=2): the cylinder's open top
  is at/above the box top, its closed bottom cap sits inside the box, so
  `box − cylinder` is a box with a cylindrical pocket. Cavity walls = the
  **cylinder lateral** (curved — the new thing) + the cylinder bottom cap (planar
  pocket floor, handled by the existing planar mechanism). One exact `Circle` rim
  edge (cylinder ∩ box-top plane).
- **Deferred (do NOT attempt):** through-hole (genus 1, χ=0 — different topology),
  sphere/cone cavities, the case where the box is the subtracted solid. `Cone`
  still rejects loudly. Union + planar Subtract paths must stay byte-for-byte.
- No new `ssi-rs` work (the rim edge uses the existing P3/Stage-4, op-agnostic).

## What to build
1. **`BRepFace.reversed: bool`** (new field). Semantics: when `true`, the face's
   *effective* outward normal (outward from the result solid) is the **negation**
   of the surface's canonical analytic outward normal. Document on the struct.
   This is a public-struct change → **faithful constructor migration**: every
   `BRepFace { … }` literal (fixtures, helpers, reconstruct) gains
   `reversed: false` (canonical) — preserve all existing behavior; only curved
   cavity walls set `true`. (Planar faces keep encoding sense in the `Plane`
   normal as today; `reversed` stays `false` for them — do not double-flip.)
2. **Set `reversed` in `reconstruct_topology` for curved cavity walls.** Reuse the
   SAME signal that drives the mesh flip: a kept curved patch whose triangles were
   flipped by `flip_for_op` (i.e. a `Subtract` cavity wall from the subtracted
   solid) → `reversed = true`. Do not invent a new classification — derive it from
   the `inside`/`flip_for_op` labels already in hand, so the mesh winding and the
   B-Rep face sense are guaranteed consistent.
3. **Consumers honor `reversed`.** Anywhere the analytic curved outward normal is
   used (Stage-1 winding orientation, face resolution, the oracle), negate it when
   `reversed`. Keep planar faces on their existing `Plane`-normal path.

## Oracle (RED contract)
1. **Cavity-sense correct**: for the surviving cylinder-lateral cavity wall, the
   *effective* outward normal (analytic away-from-axis, negated because
   `reversed`) points **toward the axis** (out of the result solid / into the
   pocket) — sampled at several points and asserted, NOT the canonical
   away-from-axis.
2. **Watertight 2-manifold, χ=2** (blind pocket is genus 0): 0 unpaired
   half-edges, Euler `V−E+F=2`.
3. **Analytic surface survives**: the cavity wall is `Surface::Cylinder` with the
   input cylinder's exact params, `reversed == true`. The box outer faces are
   `Plane`, `reversed == false`.
4. **Sidecar mesh-parity** (env-gated, LOUD skip): output mesh == sidecar
   `Subtract` of the two Stage-1 tessellations.
5. **Exact rim edge**: the cylinder ∩ box-top section is a `Curve::Circle`.
6. **Determinism; planar `fuzz_boxes` (incl. planar Subtract) unregressed; the
   curved Union tests (YR8–YR12) unregressed.** Sidecar-independent direct path
   for the GREEN gate (hand-built attributed cavity mesh → reconstruct → assert
   `reversed` + sense); env-gate the E2E.

**STOP-and-report (P9/P10)** if the blind-pocket topology or the cavity detection
can't be made correct without faking it (e.g. the `flip_for_op` signal doesn't
cleanly identify the curved cavity patch) — report the specific gap, do not
guess a sense.

## CI gate (FULL crate)
`cargo test -p yang-rs` (whole crate — a `BRepFace` field + reconstruct change can
regress Union/planar; do NOT scope to the new file), `cargo fmt -p yang-rs --
--check`, `cargo clippy -p yang-rs --all-targets -- -D warnings`. Watch for
cross-suite contract conflicts (a sibling test asserting the old no-`reversed`
shape — migrate faithfully).

On completion: update `docs/yang_functional_roadmap.md` (PR-YR13 — curved Subtract
box − cylinder, cavity-sense via `BRepFace.reversed`) and resolve the banked
"subtracted/cavity curved-face sense" deferral note in `lib.rs`. Remaining:
through-hole genus-1, sphere/cone cavities, the side-face/corner guard.
