# PR-YR10 (Stage 4, re-scoped) — yang-rs: relocate + repair mesh onto exact curves (Yang §4.4.1/§4.5)

Context: P3 (PR-YR9) gave `cylinder ∪ box` EXACT analytical intersection edges
(`Curve::Circle`/`Ellipse`). Stage 4 (Yang §4.4.1) updates the MESH so it conforms
to those exact curves. **A prior attempt (insert-NEW-on-arc points + local fan)
was DISPROVEN** — on the real cylinder∪box all 22 intersection edges inverted the
local fan and the impl silently skipped them (a no-op / anti-fallback violation).
That approach **diverged from the paper**. Do NOT repeat it. This PR implements
what Yang ACTUALLY prescribes.

**What Yang §4.4.1 + §4.5 actually do (the spec — read it before coding):**
- `refs/text/yang2025_hybrid_boolean.txt` §4.4.1 (~lines 605–610) + §4.5.1–4.5.3
  (~lines 654–728). The mesh boolean (Cherchi) already produced intersection
  vertices where the two input meshes CROSS. Yang **RELOCATES those existing
  crossing points onto the exact surface-surface intersection curve** (§4.3
  optimization). It does NOT insert fresh on-arc points. Relocating "essentially
  breaks bijectivity, causing gaps or self-intersections" (§4.4.1), so the mesh is
  repaired, **inheriting watertightness from the mesh-boolean output** — NOT
  rebuilt via a global CDT.
- The fan/sequence **inversion is a NAMED, expected case** — §4.5.3 "Correction of
  reversed intersection": insufficient local resolution makes relocated points
  reverse order. Fix: **detect the reversal by comparing the discrete tangent
  `t̃ = p_b p_r/|·| + p_r p_n/|·|` to the curve tangent, remove the reversed
  point, reconnect** (Fig. 15). Where relocate+correct still can't keep a valid
  1:1 mapping, §4.5.2 **increases mesh resolution locally** and re-does the local
  intersection, iterating (guaranteed to terminate as resolution rises).

## What to build (re-scoped)

1. **Relocate, don't insert.** Identify the existing mesh intersection vertices on
   each P3 exact-curve edge (the mesh-boolean crossing points). Project each onto
   the exact `Curve` — **closed-form** for our quadric curves (nearest point on a
   circle/ellipse; NO Newton). Move the vertex to that projected position.
   **First confirm against the actual cylinder∪box mesh** that these are
   relocatable crossing points (the paper's assumption); if our mesh instead has
   intersection vertices already exactly on a surface (e.g. rim points), adapt the
   relocation accordingly and document it.
2. **Reversed-point correction (§4.5.3).** After relocation, walk each intersection
   loop; detect any vertex whose discrete tangent disagrees with the exact curve's
   tangent (reversal); remove it and reconnect to the next correct point.
3. **Repair, inheriting watertightness.** Keep the mesh watertight 2-manifold by
   construction (relocation + reconnection are local; do NOT rebuild globally). If
   a region still can't be made valid by relocate+correct, apply **local
   resolution increase** (§4.5.2) on just that region and retry; if THAT still
   fails to converge for cylinder∪box, **STOP and report** the specific region
   (P9/P10) — never skip an edge (the disproven no-op) and never a global CDT.
4. Update the `TessellationMap` for relocated vertices (`BRepEdge { edge, t }` on
   the exact curve).

Operate on yang-rs's own mesh. No `unsafe`/panic in production paths. Sphere/Cone
still reject loudly. Do not touch Stage 1/2 or the planar path.

## Oracle (RED contract)
1. **Relocated points on the exact curve to `TAU`** (`TAU_MODEL`).
2. **Chord deviation strictly decreases**: max distance from the mesh intersection
   polyline to the exact curve is smaller after Stage 4 than before (real work).
3. **Watertight 2-manifold**: 0 unpaired half-edges, Euler V−E+F=2.
4. **No reversed/inverted/degenerate triangles**: positive area, winding agrees
   with the analytic surface normal; loop vertex order matches the curve tangent
   (the §4.5.3 invariant).
5. **Bijection round-trips**; **determinism**; **planar `fuzz_boxes` unregressed**;
   scope held. Sidecar-independent direct path for the GREEN gate (hand-built
   crossing-point mesh + exact curve → relocate+repair → assert); env-gate the
   sidecar E2E with a LOUD skip.

## CI gate (FULL crate)
`cargo test -p yang-rs`, `cargo fmt -p yang-rs -- --check`, `cargo clippy -p
yang-rs --all-targets -- -D warnings`, all clean.

On completion: update `docs/yang_functional_roadmap.md` — PR-YR10 (Stage 4:
relocate mesh intersection points onto exact curves + §4.5.3 reversed-point
correction + §4.5.2 local refinement, watertightness inherited; per Yang
§4.4.1/§4.5, NOT a global CDT). Note the superseded insert-and-fan attempt
(branch `wip/yr10-insert-fan-disproven`) and remaining work (general CDT only if
ever needed, P2b sphere, curved Subtract).
