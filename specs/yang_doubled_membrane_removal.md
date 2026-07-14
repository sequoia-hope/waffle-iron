# Yang output — doubled-membrane removal at the Stage-4 shell gate

Status: SPEC (2026-07-14, task #146 χ=3 sub-layer). Corpus drivers: R0051
(revolve auto-union) and F0064 (extrude auto-union) — both today
`NonManifoldOutput` from the `s4-shell-euler` gate with the impossible odd
`χ=3` (R0051: `v=133 e=392 f=262`; F0064: `v=1300 e=3893 f=2596`). This is
the χ=3 layer that N40 (backtrack-spike operand normalization) exposed
beneath F0064's retired `s4-halfedge-pairing` wall, and the class R0051 has
sat in since before N40.

## 0. Goal

The mesh boolean can mint a **doubled membrane**: two triangles with the
IDENTICAL vertex set and OPPOSITE winding — a zero-thickness "fin" whose apex
is a spur vertex just off a real edge (a leftover of a backtrack-spike /
near-tangent junction). The fin contributes NOTHING to the represented
solid, but each of its three shared edges gains one surplus `fwd` + one
surplus `rev` directed half-edge. The `fwd == rev` watertight pairing still
holds, so the halfedge gate passes; but the shell's Euler characteristic
reads the topologically impossible odd `χ = 3` (exactly one double-cover
edge: `3f − 2e = 2`) and the per-shell Euler gate stops loud.

Measured (probe `DOUBLECOVER_EDGE_PROBE`):

- **R0051** op-3: membrane `{116,117,132}` on edge `(116,132)`; apex `117`
  (triangle area 2.4e-9) is referenced by NOTHING but the two fin copies.
- **F0064** op-4: membrane `{1237,1282,1290}`; apex `1290` likewise
  tri-incidence 2.

After this slice: a **doubled-membrane removal** pass runs on the output mesh
before the shell gate; every opposite-winding coincident-triangle pair is
dropped, the shell heals to an honest even `χ`, and the spur apex is left
dangling for `compact_unreferenced_verts`.

## 1. Parameters

No new tunables. The removal is purely combinatorial (vertex-set identity +
winding parity); no positional tolerance is involved.

## 2. Branch table

| Output-mesh triangle group (same sorted vertex triple) | Today | After |
|---|---|---|
| singleton (ordinary face) | passes | byte-identical |
| opposite-winding pair (doubled membrane / fin) | χ gate failure (loud) | **remove both copies** |
| same-winding duplicate (distinct defect) | loud gate failure | unchanged (loud — the guard) |
| triple with a repeated vertex (degenerate) | loud gate failure | unchanged (loud) |

## 3. Invariants

- **I1 (honest cancellation):** a triangle is removed ONLY as part of an
  opposite-winding same-vertex-set pair. Same-winding surplus and degenerate
  triples keep today's loud error (P9 — the pass must never mask a genuine
  defect).
- **I2 (volume / point-set):** removal is exact zero-volume; no point moves
  and the represented solid is unchanged.
- **I3 (edge balance):** each removed pair drops `fwd` and `rev` by one on all
  three shared edges, preserving the `fwd == rev` watertight invariant; an
  edge that reaches zero vanishes. The pass can never open a new boundary.
- **I4 (Euler):** each doubled membrane raises `χ` by 1; removing it lowers
  `χ` by 1 toward the honest even value. Removal is unconditional on parity, so
  a two-membrane shell (which would read an even `χ` and silently masquerade
  as a higher genus) is also healed.
- **I5 (no-op on manifold outputs):** any output with no doubled membrane is
  BYTE-IDENTICAL through the pass (the entire green corpus).
- **I6 (downstream):** the spur apex is dropped by the caller's
  `compact_unreferenced_verts`; stages 5/6 consume the healed mesh.
- **I7 (determinism):** grouping and removal in sorted-triple + triangle-index
  order.

## 4. Oracles

- **Canonical (yang-rs unit):** a tetrahedron (χ=2) with a doubled-membrane
  fin on one edge reads χ=3 and the shell gate fails; after removal the shell
  is a valid χ=2 manifold and the gate passes
  (`doubled_membrane_heals_odd_chi_shell`).
- **No-op (I5):** a clean tetrahedron is byte-identical through the pass
  (`clean_shell_is_byte_identical`).
- **P9 guard / mutation:** a same-winding coincident pair is NOT removed
  (`same_winding_duplicate_is_left_for_the_gate` — kills a mutant that ignores
  the winding sign); `membrane_orientation_sign_is_parity` pins the sign.
- **Corpus (P9 gate):** R0051 and F0064 lose the `s4-shell-euler χ=3` wall
  (each shell heals to a valid χ). Both remain ERROR as MULTI-LAYER cases —
  they advance to deeper pre-existing walls (F0064 → Stage-6
  `s6-planar-positive-count`; R0051 → kernel-v2 Newell-normal re-entry) — so
  the assay totals are byte-identical minus the χ=3 sub-layer, with zero
  SUPPORTED_CORRECT lost.

## 5. Failure modes

- Same-winding coincident triangles: unchanged loud `NonManifoldOutput` with
  the `NONMANIFOLD_SITE_PROBE` site preserved.
- Any residual double-cover after membrane removal (a genuine higher-order
  defect): unchanged loud gate.

## 6. Research basis

- [#23 Mäntylä] manifold B-Rep data structures represent degenerate /
  self-touching point-set solids by topological cleanup; a zero-thickness fin
  is not a face of the solid and is removed. This is the mesh-level dual of the
  tangency pinch-VERTEX split (spec `yang_tangency_pinch_split.md`, task #86).
- [#24 Yang 2025 §4.4] the pipeline's output is required to be a clean
  2-manifold B-Rep; this pass restores that contract after a degenerate-fin
  artifact of the exact mesh boolean.

## 7. Analytical vs. approximate

The removal is exact and combinatorial — no tolerance, no geometry change.
The represented solid (volume, surfaces, curves) is untouched; only a
zero-contribution topological artifact is deleted.
