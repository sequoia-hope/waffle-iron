# N4 — Coincident-cylinder Stage-0 provenance (`tri_face` emission)

Status: **IN PROGRESS (2026-07-01).** Increment of the N4 face-provenance
campaign (`docs/yang_deviations.md` N4; roadmap M8). Predecessors: 1a
(cherchi `source`), 1b (non-coplanar provenance), 2a (planar coplanar
overlay `tri_face`), 2b-1 (same-normal wall lifted). This closes the LAST
Stage-0 producer that still leaves `tri_face` empty — the coincident-cylinder
membrane path (`stage0::coincident_cylinder_stage0`).

## 0. Goal

`boolean()` Stage-6 attributes each kept arrangement triangle to a B-Rep face
via cherchi per-triangle provenance (`la.source` → `tri_face`), falling back to
geometric centroid-proximity only where `tri_face` is empty
(`crates/yang-rs/src/lib.rs` ~7016-7028). Today `coincident_cylinder_stage0`
returns `tri_face_a/b: Vec::new()`, so every coincident-cylinder case
(`m8cyl_plug_in_bore`, `gear_flange_union`, user `err.waffle`) is attributed
geometrically. This spec makes that path emit a complete per-triangle → face
map for the re-tessellated `mesh_a`/`mesh_b`, so provenance becomes primary
there too — one more producer toward retiring geometric attribution entirely.

## 1. Parameters (inputs)

Unchanged public surface. Internal: `coincident_cylinder_stage0(a, b)` already
produces `mesh_a`, `mesh_b`. New outputs: `tri_face_a`, `tri_face_b`
(`Vec<u32>`, 1:1 with the respective mesh `tris`).

The two produced meshes:
- **cont_mesh** — the FULL Stage-1 re-tessellation of the contained solid
  (`Mesh::new(cont_tess.verts, cont_tess.tris)`), untouched.
- **outer_mesh** — the outer solid's tessellation with its coincident-cylinder
  cluster faces' triangles replaced by a re-banded full-θ strip
  (`build_conformal_outer_mesh`).

## 2. Branch table

| Case | cont_mesh tri_face | outer_mesh tri_face |
|------|--------------------|---------------------|
| any coincident-cylinder handled case | invert `cont_tess.face_tri_ranges` (every tri → its owning face) | see below |
| outer, non-cluster triangle | — | owning face via `outer_tess.face_tri_ranges` |
| outer, band-strip tri, **single** cluster face (`outer_faces.len()==1`) | — | that one cluster face |
| outer, band-strip tri, **multi** arc-patch cluster (`outer_faces.len()>1`) | — | arc-patch face whose azimuth arc contains the strip column's midpoint azimuth |
| outer, band-strip tri, no covering arc found (fp anomaly at a seam) | — | `u32::MAX` sentinel → geometric fallback (loud-safe) |
| early-return conformal outer (no interior ring inserted) | — | invert `outer_tess.face_tri_ranges` (mesh == outer_tess, unchanged) |

The `mesh_a` ↔ `mesh_b` assignment follows `outer_is_a`
(outer_mesh's map → `tri_face_a` iff `outer_is_a`, else `tri_face_b`).

## 3. Invariants

- **I1 (1:1):** `tri_face_a.len() == mesh_a.tris.len()` and likewise for B.
- **I2 (validity):** every entry is either a valid face index
  (`< brep.faces().len()` for that input) or the `u32::MAX` sentinel.
- **I3 (cont completeness):** the contained mesh's map has NO sentinel — every
  contained triangle is a real Stage-1 face triangle.
- **I4 (band-strip azimuth consistency):** a band-strip triangle assigned to a
  multi-patch cluster face has its centroid azimuth (axis frame) inside that
  face's rim-vertex azimuth arc. Columns never straddle a seam (the aggregated
  ring includes seam vertices), so the assignment is unambiguous.
- **I5 (no behavioral regression):** because provenance and geometric
  attribution must both be correct on these cases, the boolean OUTPUT is
  watertight, orientable, and correct-volume exactly as before (the existing
  `plug_in_bore` / `gear_flange` oracles stay green).

## 4. Oracles

- `plug_in_bore_union_is_watertight` — unchanged (single-cluster path); union
  watertight + orientable + positive volume with provenance now primary.
- `gear_flange_union` — unchanged (multi-patch path); union watertight +
  orientable + correct bbox/volume with provenance now primary.
- New yang-rs test: `coincident_cylinder_stage0` on a plug/tube fixture emits
  `tri_face_a/b` satisfying I1–I4 (1:1, valid, cont complete, band-strip
  azimuth-consistent). Includes a SEAMED-cylinder (≥2 arc-patch) outer fixture
  to exercise the azimuth partition (I4) directly, fast.

## 5. Failure modes

- No covering arc for a strip column midpoint → `u32::MAX` → geometric fallback
  (never a wrong face; P9-safe). Not an error.
- `coincident_cylinder_stage0` returning `None` (out of scope: >1 group,
  same-normal, partial overlap, non-2-ring) is unaffected — no `tri_face`.
- The sentinel is consumed by `provenance_face`, which must treat `u32::MAX` as
  `None` (→ geometric). Any other invalid index would be a bug caught by I2.

## 6. Research Basis

- **[#24] Yang et al. 2025** §4.2.3 — map each intersection/output triangle to
  its source surface via the arrangement's intrinsic provenance rather than a
  geometric proximity heuristic. This increment supplies that provenance for the
  §4.5.5 coincident-cylinder membrane (the curved analog of the planar coplanar
  overlay).
- The azimuth-partition assignment is the natural inverse of the full-θ ring
  re-banding (`cluster_rim_rings` aggregates arc-patch faces into full rings;
  the assignment recovers the per-arc-patch face by the column's azimuth). No
  new algorithm — arc patches tile the circle and share seam vertices, so a
  smallest-covering-arc containment test is exact.

## 7. Analytical vs. Approximate

Method: **exact provenance mapping** (index bookkeeping over the exact
re-tessellation). No SSI change. The coincident-cylinder overlap geometry is
unchanged; only its per-triangle face labeling gains an exact source, replacing
the geometric proximity fallback.
