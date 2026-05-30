# PR-YR8 (P2c) — yang-rs first curved boolean: cylinder ∪ box (mesh-approximate)

> Spec of record for PR-YR8. Role-separated FIP cycle (Spec → RED → GREEN → Adversary).
> Plan of record: `docs/yang_functional_roadmap.md` M5 / Phase 2. This PR hits the
> Phase-2 exit example (`cylinder ∪ box`) **minus exact edges** (those are P3 / `ssi-rs`).

## 1. Objective

Run a curved solid through the WHOLE `yang-rs` pipeline for the first time —
`boolean(cylinder_brep, box_brep, BoolOp::Union, &backend)` — and prove:

1. curved geometry flows through Stage 2 (sidecar arrangement) → Stage 5/6 reassembly, and
2. the **analytic surface survives**: a kept patch on the cylinder's lateral face emits a
   `BRepFace` carrying `Surface::Cylinder` with the **input cylinder's exact parameters**
   (governance A15 — the mesh is a tool; the analytic surface is the truth).

Intersection edges remain mesh-approximate (`Curve::LineSegment` polylines). Replacing them with
exact `ssi-rs` curves is **P3** and is OUT OF SCOPE here.

## 2. Hard scope limits

- **No `ssi-rs` import or call.** Intersection edges stay `Curve::LineSegment`.
- **Cylinder + box only.** No sphere (P2b), no cone, no two-curved-solids case.
- **Union is the required, asserted case.** Subtract/Intersect may fall out for free but are not
  required; do not add cavity/subtracted curved-face *sense* handling (deferred).
- Reuse the existing Stage-2 sidecar path (`cherchi-sidecar-rs` / `SidecarBoolean`) and the
  existing Stage-5/6 `reconstruct_topology`. EXTEND them for curved faces; do NOT rewrite the
  planar path. Sphere/Cone stay loudly rejected everywhere.

## 3. Current state (verified against `crates/yang-rs/src/lib.rs`)

- `boolean()` (≈1279–1530): Stage 2 sidecar `labeled_arrangement` → I6 weld → `keep_set(op)` +
  `flip_for_op` → compact sub-mesh → **Stage-6 geometric face resolution** (1395–1509) →
  `reconstruct_topology` (1512). XOR is loudly `UnsupportedOp`.
- `signed_distance_to_surface` (1094–1123): `Plane`=`n·x+d`; `Cylinder`=`dist_to_axis − r`;
  `Sphere`/`Cone` → `Err(CurvedSurfaceNotYetSupported{face: usize::MAX})`.
- Stage-1 cylinder tessellation (PR-YR7) already produces a watertight, chord-bounded
  (`d_ε = 1e-2 × analytic AABB diag`) lateral mesh; all lateral vertices lie exactly on the
  analytic cylinder.
- `reconstruct_topology` (1560–1705) is **planar-only**: line 1597–1606 loudly rejects every
  curved inherited surface (incl. `Cylinder`); the loop classification (single plane normal +
  Newell signed area + cavity-sense flip + E3 `positive_count==1`) is meaningless for a barrel.
- Constants: `TAU_WORK = 1e-12`, `MIN_FEATURE_SIZE = 1e-6` (`cad-primitives`).

## 4. The two blockers and their honest fixes

### Blocker 1 — Stage-6 face-resolution tolerance is planar-exact (1421–1506)

The rule attributes a kept triangle to the unique face of the sidecar-labeled solid whose surface
contains the **centroid** within an absolute `TAU_WORK = 1e-12`. Correct for planes (a planar
triangle's centroid is exactly on its plane). **Wrong for tessellated curved faces:** a
lateral-cylinder triangle's centroid is chordally inside the analytic cylinder by up to
`d_ε ≈ 1e-2 × AABB_diag` (~1e10× `TAU_WORK`), so every lateral triangle would `FaceResolutionFailed`.

**Honest fix (NOT tolerance widening — A15 / A14.3):** the membership tolerance is the surface's
own Stage-1 tessellation chord bound. A `Plane` has zero chord error → `TAU_WORK`. A `Cylinder`
face is a `d_ε`-approximation **by construction** → test membership at `d_ε`, the *same* bound
Stage 1 guarantees. Generalize the existing "exactly one face within tolerance" rule to a
**per-face tolerance**:

```
tol_i = TAU_WORK            if face_i.surface is Plane
tol_i = d_ε(labeled_solid)  if face_i.surface is curved (Cylinder)
dist_i = |signed_distance_to_surface(face_i.surface, centroid)|
attribute ⟺ exactly one face_i with dist_i < tol_i
  0 matches  → FaceResolutionFailed (F3)
  ≥2 matches → FaceResolutionFailed (F3 tie)
```

- For **all-planar inputs this is byte-for-byte the existing behavior** (`tol_i ≡ TAU_WORK` ⟺
  "min < TAU_WORK ∧ second ≥ TAU_WORK"). Planar tests (incl. the 900-case box fuzz) MUST stay green.
- `d_ε(solid)` MUST come from the **same** code Stage 1 uses (`1e-2 × analytic AABB diag` over the
  solid's `Curve::Circle` rims). Refactor that into ONE shared helper used by both Stage 1 and
  face resolution (A14.3 — single source of the constant, no divergent epsilon). A solid with no
  curved faces exposes no `d_ε` band; all its faces use `TAU_WORK`.
- The degenerate-sliver branch (1459–1476) keeps `TAU_WORK`.
- **Non-overlap (must hold for the canonical config):** for the cylinder solid {lateral,
  top-cap, bottom-cap}, the `d_ε` barrel band and the `TAU_WORK` cap bands do not overlap (cap
  fan-centroids sit at radius ≤ 2r/3, ≥ r/3 ≫ d_ε from the barrel; lateral centroids sit a
  height-fraction from each cap). The implementer must EMPIRICALLY confirm no F3-tie (STOP cond. 1).

### Blocker 2 — `reconstruct_topology` rejects curved; loop logic is planar-only (1596–1701)

Add a **curved-surface branch** that runs before the planar normal/Newell/flip machinery:

- `Surface::Cylinder` → inherit the surface **unchanged** (Union has no cavity → **no sense flip**;
  curved cavity-sense for Subtract is explicitly deferred — note it).
- Reuse `patch_boundary_cycle` (surface-agnostic) for all boundary cycles.
- Build `BRepFace { surface: Cylinder, outer_loop, inner_loops }` **without** the single-plane
  Newell classification. Deterministic loop assignment: cycle with the most edges (tie-break:
  lowest min start-vertex index) = `outer_loop`; the rest = `inner_loops`. Edges =
  `Curve::LineSegment` (mesh polyline; exact `Curve::Circle` is P3). Keep the E2 degenerate-loop
  guard (Newell magnitude < `MIN_FEATURE_SIZE²` → `NonManifoldOutput`); DROP E3/flip for curved.
- `Surface::Sphere`/`Cone` → STILL loudly reject (`CurvedSurfaceNotYetSupported`).
- Planar patches → unchanged.

The **output mesh** (`kept_submesh`) — what the watertight / mesh-parity oracles check — stays
produced by the unchanged weld + `keep_set` machinery.

## 5. STOP-and-report conditions (P9/P10 — do NOT improvise a fix)

For the canonical `cylinder ∪ box` (axis through a box face), HALT and report the specific gap if:
1. Face resolution yields an F3 tie / zero-match on a kept lateral triangle that the
   `d_ε`/`TAU_WORK` per-face rule cannot resolve without inflating `d_ε` beyond the Stage-1 bound.
2. `patch_boundary_cycle` returns `NonManifoldOutput` (T-junction/dead-end) on the lateral patch.
3. The output mesh is not watertight for a correct union.
No tolerance widening, no fallback paths, no fake closure/snapping, no wrong shell.

## 6. Oracle (RED contract) — `tests/yr8_curved_boolean.rs`

Config: cylinder axis passing **through** a box face (real curved intersection). Provide BOTH a
sidecar-gated end-to-end path AND sidecar-independent direct tests (the sidecar may be unbuilt
locally). Reuse fixtures: `cylinder_brep(...)` (`tests/yr7_cylinder.rs`),
`unit_cube_brep_offset_at(...)`/`SidecarBoolean::from_env()` (`tests/end_to_end.rs`),
analytic oracles (`signed_volume`, `unpaired_half_edges`, `euler_characteristic`) from
`tests/m3_adversary.rs`/`end_to_end.rs`.

1. **Runs & Ok** (E2E, env-gated): `boolean(&cyl,&box,Union,&sb)` → `Ok`, no panic.
2. **Analytic surface survival** (E2E + direct): output `BRep` has ≥1 face
   `Surface::Cylinder` whose `axis_point`/`axis_dir`/`radius` `==` the input cylinder's (exact, not
   re-fit). Box patches are `Surface::Plane`.
3. **Sidecar mesh-parity** (E2E, env-gated): output mesh == sidecar `mesh_booleans` Union of the
   SAME two Stage-1 tessellations (canonicalized compare). Skip + `log` if binary absent — never
   silently pass.
4. **Geometric soundness** (E2E + direct): every output-mesh vertex of a cylinder-lateral triangle
   lies within `d_ε` of the analytic cylinder; box-face vertices within `TAU_WORK` of their planes.
5. **2-manifold / watertight** (E2E, env-gated): 0 unpaired half-edges and Euler V−E+F=2 — OR an
   asserted `NonManifoldOutput`/STOP with the specific deferred reason (§5).
6. **Determinism** (direct): identical inputs → identical output `BRep` + mesh.
7. **Sphere/Cone still loud** (direct): an unsupported curved inherited surface still returns
   `CurvedSurfaceNotYetSupported`.

**Direct (no-sidecar) path:** build a synthetic watertight trimmed-tube mesh + hand-built
`TriangleAttributionMap` attributing barrel tris to a cylinder face, call
`reconstruct_topology(&mesh,&attr,&a,&b)` directly (mirrors the existing `m3_*` direct tests) and
assert items 2/6/7. For Blocker 1, exercise face resolution on tessellated-cylinder centroids
(~d_ε off) via a small mock `MeshBoolean`/`LabeledArrangement` (see `m3_adversary` mock) or the
env-gated E2E path, stating the dependency explicitly.

**Faithful contract migration:** locate existing tests asserting the cylinder path returns
`CurvedSurfaceNotYetSupported` (search `yr7_*`, `yr6_adversary`, `end_to_end`). Migrate ONLY the
expected outcome, preserving every structural assertion. Sphere/Cone rejection tests are unchanged.
The Adversary independently verifies the migration was not weakened.

## 7. CI gate

`cargo test -p yang-rs` (FULL crate), `cargo fmt -p yang-rs -- --check`,
`cargo clippy -p yang-rs --all-targets -- -D warnings` — all clean. If the sidecar can't build in
this environment, env-gated oracles SKIP-with-log; the mesh-parity / 2-manifold-via-sidecar oracle
is then closed only on a sidecar-equipped runner, and the direct-`reconstruct_topology` /
face-resolution tests are the in-environment GREEN gate.

## 8. Deviations from Yang 2025

Same interim deviation as M3 (Stage-2 labels from the C++ sidecar, not a native arrangement).
Intersection edges are faceted polylines pending Stage-3 SSI (P3). No new deviation introduced.
