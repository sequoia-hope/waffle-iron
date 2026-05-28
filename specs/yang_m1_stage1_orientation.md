# Spec: M1 — yang-rs Stage 1 emits Cherchi-`inputcheck`-clean meshes

**Status:** active (roadmap `docs/yang_functional_roadmap.md` M1)
**Feature cycle:** yang-m1
**Roles (P5):** Spec Writer = Manager; Test Author and Implementer are distinct agents.

## Goal

Yang Stage 1 (bijective tessellation) must produce a triangle mesh that satisfies
the Cherchi 2022 §3 input axioms, so the downstream mesh boolean does not exhibit
undefined behavior (the malformed-input infinite loop that burned ~6 h on F0002).
Concretely: for a well-formed closed B-Rep of convex planar faces, `BRep::new`'s
output mesh must pass all five `mesh_booleans_inputcheck` checks.

Empirically, today's Stage 1 already passes 4/5 for the canonical cube; the lone
failure is **Global Orientation** (the mesh is consistently wound but inside-out,
because fan-triangulation ignores the face's stated outward normal). This cycle
fixes that by orienting each face's triangle winding to agree with its analytic
surface normal.

## Scope

- **In:** planar faces (`Surface::Plane`), convex `outer_loop`s, closed solids.
- **Out (banked):** non-convex faces, inner loops/holes, curved surfaces
  (PR-YR2b–d). Non-planar/non-convex input is not made inputcheck-clean here.

## Parameters / inputs

`BRep::new(verts: Vec<BRepVertex>, edges: Vec<BRepEdge>, faces: Vec<BRepFace>)`.
Each `BRepFace` carries `surface: Surface::Plane { normal, d }` where `normal` is
the **outward** normal (existing documented contract, lib.rs Surface doc) and
`outer_loop` is the face's edge cycle.

## Branch table

The new behavior is per-face winding canonicalization. Normalize early (§7): one
decision per face, no downstream orientation branching.

| # | Condition (per face) | Action | Oracle |
|---|---|---|---|
| B1 | Newell polygon normal `N`, `dot(N, surface.normal) > 0` | keep loop order | triangle normals agree with `surface.normal` |
| B2 | `dot(N, surface.normal) < 0` | reverse loop before fan-triangulating | triangle normals agree with `surface.normal` (post-flip) |
| B3 | `‖N‖ < MIN_FEATURE_SIZE` (zero-area / collinear / degenerate face) | return `Err(YangError::DegenerateFace { face: f_idx })` | error variant returned; no mesh produced |

Every branch must have ≥1 test (P4).

## Invariants

- **I1 (orientation):** every output triangle's geometric normal has positive dot
  with its source face's `surface.normal` (outward). Pure-Rust checkable.
- **I2 (inputcheck-clean):** for the canonical closed solids (unit cube,
  tetrahedron), the Stage-1 mesh passes all five `inputcheck` axioms — manifold,
  watertight, local orientation, global orientation, intersection-free.
- **I3 (Euler):** for a genus-0 closed solid, `V − E + F = 2` over the output
  triangle mesh (structural cross-check; V = mesh verts, F = tris, E = undirected
  edges).
- **I4 (bijection preserved):** orientation canonicalization does not change the
  `TessellationMap` (vertex→B-Rep-feature 1:1 mapping) or vertex count; only
  triangle winding (and possibly per-triangle vertex order) changes.
- **I5 (determinism):** identical input → identical mesh (no hashing/ordering
  nondeterminism); §8.

## Oracles

- **Primary (I2):** the external C++ `mesh_booleans_inputcheck` binary, invoked
  via the new `cherchi_sidecar_rs::inputcheck` harness. This oracle is outside
  our control and cannot be weakened (P9). NB: it writes its verdict to
  **stdout**, and its **exit code is 0 regardless of pass/fail** — parse stdout.
- **Secondary (I1, I3):** pure-Rust structural assertions in yang-rs tests.

## Failure modes / expected errors

- Degenerate/zero-area face → `YangError::DegenerateFace { face }` (new variant).
- Existing `MalformedTopology` (index out of range, <3 edges) unchanged.
- Non-convex face: out of scope; Stage 1 does not guarantee inputcheck-clean
  output. Not an error in M1 (documented limitation), but tests must not assert
  cleanliness for non-convex input.

## Research basis

- **Yang, Jia & Yan (SIGGRAPH 2025)** [#24] §4.1 — bijective tessellation;
  the tessellation must preserve the B-Rep surface orientation.
- **Cherchi et al. (2022)** §3 — the boolean pipeline's input axioms (manifold,
  watertight, oriented, intersection-free); malformed input is UB.
- **Newell's method** (Sutherland, Sproull & Schumacker 1974; Foley et al.,
  *Computer Graphics*) — robust polygon normal for a (near-)planar polygon, used
  to decide winding vs the analytic normal. Cite in code comment.
- **Governance A15.5** — analytic surface normal is authoritative (this is why
  we orient to the stated normal rather than deriving from topology).

## Definition of Done (DoD §1)

Spec (this file) ✓; RED→GREEN with separate commits; every branch (B1–B3) tested;
numeric/structural oracles (I1–I3), not "no panic"; canonical case (cube) + edge
case (tetrahedron) + degenerate case (B3); determinism (I5); no test weakened;
no new clippy warnings / fmt clean; no regression in sibling crates.
