# KV16b — Same-type cone-ELLIPSE junction routing (N2 epic increment 7)

**Milestone:** N2 epic (roadmap §N2 trail), the `vert_cone_ellipse` sibling of
KV16's item 1 (`specs/kv16_hyperbola_arc_vocabulary.md` — "Named residue").
**Corpus drivers:** R0004, R0009, R0091, R0100 — all four fail
`InvalidBooleanOutput("output ellipse-arc endpoint does not lie on its
ellipse")` at kernel-v2's import certification.

## Diagnosis (measured on R0004, 2026-07-11)

R0004's failing subtract (box ∖ 1008-vert revolved body) carries a vertex
(mesh v430) at the junction of TWO cone-ellipse curves of ONE narrow cone
(half-angle 2.53°, far apex) sectioned by two different planes:

- edge (292,430) → `Ellipse` from cone ∩ planeC (a=5.171, b=1.886)
- edge (430,431) → `Ellipse` from cone ∩ planeD (a=7.236, b=1.882)

`vert_surfs[430] = {cone, planeC, planeD}` (exactly 3), position off the
cone by 1.06e-3 (a chord vertex — NOT exactness-certified). Both curves'
Stage-4 scan arms execute `vert_cone_ellipse.insert(v430, …)`; the second
**silently overwrites** the first, so `n_maps == 1`, the increment-5
triple-junction trigger never fires, and the single-curve relocation moves
v430 onto only the SURVIVING ellipse (planeD's). The other output edge's
endpoint is left ~8e-5 off planeC's ellipse → kernel-v2 rejects. This is
byte-for-byte the KV16 item-1 mechanism in the sibling map.

## Research basis

- [#24] Yang et al. 2025 §4.3.3 / §4.4.1 — junction vertices lie on ALL
  incident intersection curves; relocation must respect every constraint.
- KV16 item 1 (`vert_cone_hyperbola` same-type detection) — the recipe this
  increment mirrors field-for-field.
- `specs/yang_stage4_conic_triple_junction.md` — the increment-5
  `relocate_onto_implicit_triple` consumer (`same_type_junction` routing).

## Fix

At the `vert_cone_ellipse` insert site (cone+plane arm of the Stage-4 scan),
detect a SECOND, DIFFERENT descriptor for the same vertex — differing in any
of `apex` / `axis_dir` / `half_angle` / `plane_n` / `plane_d` — and insert
the vertex into `same_type_junction`, which the existing multi-curve trigger
honors (n_maps < 2 but same-type ⇒ triple relocation; 3-surface vertices
relocate onto the exact cone∩planeC∩planeD point; ≥4-surface keep today's
loud audits). The insert still proceeds (bookkeeping identical to KV16's
hyperbola arm — the triple pass removes the vertex from every map).

## Branch table

| # | input | behavior |
|---|---|---|
| 1 | first descriptor for v | insert (unchanged) |
| 2 | second descriptor, IDENTICAL fields | insert/overwrite (same curve twice — unchanged, two arcs of one split ellipse) |
| 3 | second descriptor, ANY field differs | `same_type_junction.insert(v)` + insert (new) |
| 4 | routed vertex with `vert_surfs` = 3 | `relocate_onto_implicit_triple` (existing increment-5 arm) |
| 5 | routed vertex with `vert_surfs` ≠ 3 | untouched — today's loud audits (I2) |

## Invariants / Oracles

- Unit (yang-rs `rim_junction_insertion.rs`): 30° frustum ∖ 45°-rotated
  square prism ("diamond prism" along x). Every prism face is 45° to the
  cone axis (> 30° half-angle) so ALL its sections are ellipses; the prism's
  corner edges pierce the cone lateral → same-map ellipse×ellipse junctions.
  Oracles: subtract completes, watertight, Simpson-referenced volume, and
  **every output `Ellipse` edge endpoint on its own ellipse at the kernel
  import band** (the discriminating check — a pierce vertex relocated onto
  only one plane's ellipse fails it by a facet-sagitta-scale residual).
- Corpus: R0004/R0009/R0091/R0100 advance out of the ellipse-endpoint
  reject (mutation oracle: routing off ⇒ reject returns).
- Assay: 0 WRONG, zero-lost.

## Failure modes

- Newton divergence in the triple relocation → vertex stays in its maps →
  today's loud over-determined audits (unchanged).
- ≥4-surface same-type junction → loud audits (unchanged, I2).
- `vert_parabola` keeps the SAME latent trap — still no corpus driver
  (generator-parallel double-section is measure-zero); documented residue.
