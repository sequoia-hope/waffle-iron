# PR-YR10 (Stage 4, re-scoped) — yang-rs: relocate mesh intersection points onto exact curves + §4.5.3 reversed-point correction

> Manager spec of record for the role-separated FIP cycle (P5): Spec (this doc)
> → RED (test-author sub-agent) → GREEN (implementer sub-agent) → Adversary
> (third sub-agent). The implementer never edits tests; the test author never
> writes production code. Stay on `main`; commit each phase; push at end.
> Paper citations are line ranges in `refs/text/yang2025_hybrid_boolean.txt`.

## 1. Objective

PR-YR9 (P3) gave `cylinder ∪ box` **exact analytical intersection edges**:
`reconstruct_topology` attaches an exact `Curve::Circle`/`Ellipse` to each output
`BRepEdge` (via `build_intersection_curves`, `crates/yang-rs/src/lib.rs:1312`).
But the **mesh** is still the faceted mesh-boolean output — its intersection-edge
vertices sit on the polygon chords, *inside* the true circle by up to the Stage-1
chord bound `d_ε`. Stage 4 (Yang §4.4.1 + §4.5) updates the mesh so it conforms
to those exact curves.

A prior attempt (insert NEW on-arc points + local fan) was **disproven** — on the
real `cylinder ∪ box` all 22 intersection edges inverted the local fan and the
impl silently no-op'd (an anti-fallback violation). It is preserved on branch
`wip/yr10-insert-fan-disproven` (commit `46980456`) and must NOT be repeated.

What Yang actually prescribes:
- **§4.4.1 (txt mesh-updating block):** RELOCATE the existing mesh crossing points
  onto the exact SSI curve. Relocation "essentially breaks bijectivity, causing
  gaps or self-intersections"; **watertightness is inherited from the mesh-boolean
  output** and repaired locally — *not* a global rebuild/CDT.
- **§4.5.3 "Correction of reversed intersection":** discrete tangent
  `t̃_pr = (p_r−p_b)/‖·‖ + (p_n−p_r)/‖·‖` vs exact curve tangent `t_pr = n_A × n_B`;
  if the angle ∈ (45°,135°) → reversal: remove the next point, reconnect, repeat;
  if none resolves, remove `p_r` and restart from `p_b`. **Collinear `t̃`
  (degenerate) is the *healthy* case — skip the angle test** (paper: "if the
  consecutive points exhibit reversal are collinear, t̃ is almost degenerate. In
  such a case, we directly detect [no] reversal, avoiding the angle comparisons").
  Angular tolerance is `10⁻⁶ rad` (paper §5); distance tolerance `d_p = 10⁻⁷` =
  `TAU_MODEL`.
- **§4.5.2 "Local refinement":** local resolution increase + re-do local
  intersection if relocate+correct still fails (guaranteed to terminate under
  refinement).

## 2. Scope decisions (confirmed defaults — align with brief + governance P9/P10)

- **Circle projection only.** Closed-form radial projection (no Newton). An
  `Ellipse` intersection edge returns
  `Err(Stage4RegionInvalid{ reason: EllipseProjectionUnsupported })` — honest; the
  axis-aligned `cylinder ∪ box` yields Circles only, so this does not block the
  target fixture. Ellipse relocation (a quartic) is a later PR.
- **§4.5.2 = STOP-and-report.** Fully implement §4.4.1 relocate + §4.5.3
  reversed-point correction. If a region still can't be made valid, return
  `Err(Stage4RegionInvalid{ reason: LocalRefinementRequired })` **loudly**. Genuine
  §4.5.2 (re-invoking the Stage-2 backend on a refined sub-mesh) is beyond the
  current `MeshBoolean` surface and lands in a later PR. Expected: `cylinder ∪ box`
  never triggers it.
- **Sphere/Cone** still reject loudly (unchanged). **Planar path unchanged**
  (byte-for-byte; the 900-case `fuzz_boxes` corpus has only `LineSegment` edges).
- **No Stage 1/Stage 2 edits.** No change to `BRep::new`, the sidecar, or the
  arrangement. Only Stage 4 (post-arrangement, in `reconstruct_topology` +
  `boolean()`).

## 3. Current state (verified against `crates/yang-rs/src/lib.rs`)

- `reconstruct_topology(mesh: &Mesh, attribution, a, b)` (`src/lib.rs:1922`) takes
  `&Mesh` (immutable). It builds Phase-A structure: `triangle_adjacency` (`:2202`)
  → `flood_fill_patches` (`:2228`) → per-patch `cycles` via `patch_boundary_cycle`
  (`:2283`, ordered directed `(s,e)` loops) + `incidence` map (`:1980`) +
  `intersection_curves = build_intersection_curves(..)` (`:1993`). Then Phase-B
  emission (`:1999`-`:2189`, the Cylinder branch + the planar Newell/flip/E2/E3
  branch) writes `edges`/`faces`, looking up each edge's curve in
  `intersection_curves`.
- `boolean()` (`:1601`) ends by calling `reconstruct_topology(&kept_submesh, …)`
  then builds the blanket `sources: (0..n).map(BRepVertex)` (`:1879-1882`).
- Helpers available for reuse: `ortho_basis(normal)` (`:816`, the SAME basis
  Stage-1 sampling + `eval_source` use — `t` must match), `normalize3` (`:793`),
  `curved_chord_bound(edges)` (`:1088`, the `d_ε` source), `signed_distance_to_surface`
  (`:1128`). Constants: `TAU_MODEL=1e-7`, `MIN_FEATURE_SIZE=1e-6`, `TAU_WORK=1e-12`.
- `eval_source` (`:681`) inverts a `BRepEdge{edge,t}` source for a `Curve::Circle`
  via `center + r·(cos t·e1 + sin t·e2)` with `ortho_basis(normal)` — so a relocated
  vertex tagged `BRepEdge{edge, t=atan2(v,u)}` round-trips.
- `Mesh { pub verts: Vec<Point3>, pub tris: Vec<[u32;3]> }` (cherchi-rs).

## 4. Implementation design (all in `crates/yang-rs/src/lib.rs`)

### 4.1 Splice point (seam A1)

Refactor `reconstruct_topology` to take `&mut Mesh`. After it builds the Phase-A
structures (through the `intersection_curves` line, `:1993`): **if** any
intersection edge carries an analytic conic (`Curve::Circle` **or**
`Curve::Ellipse`), call the new `stage4_relocate_and_correct`. (Entering on
*any* conic — not just `Circle` — is required so an ellipse-only fixture reaches
the loud `EllipseProjectionUnsupported` STOP instead of silently passing an
un-relocated ellipse mesh; circles relocate, ellipses STOP — see §4.5 step 1.)
Then run the existing Phase-B emission **unchanged**
— it re-validates the relocated mesh for free (Newell area, E2/E3 degeneracy,
winding-vs-normal). When there are no conic edges (all `LineSegment`), Stage 4 returns immediately
and everything runs exactly as today (planar byte-identity).

`reconstruct_topology` gains a `Vec<TessellationSource>` return component (default
`BRepVertex(i)` for every mesh vert, overridden to `BRepEdge{edge,t}` for relocated
verts). `boolean()` passes `&mut kept_submesh` and uses that source vector instead
of the blanket `(0..n).map(BRepVertex)`.

Rationale: Stage 4 needs the exact curves + ordered loops + patch/incidence
structure Phase A already computes — re-deriving them in `boolean()` would risk
classification drift.

> If recomputing Phase-A after a collapse forces a control-flow shape the
> existing function can't express cleanly, that is a localization signal, not a
> license to improvise: STOP and report (P10).

### 4.2 New error types

Add to `enum YangError` (`:1413`), with `Display` arms (`:1481`); `Error::source`
(`:1530`) unchanged:

- `Stage4ReversalUnresolved { edge: (u32,u32), vertex: u32 }`
- `Stage4RegionInvalid { vertex: u32, reason: Stage4InvalidReason }`

New `#[derive(Debug, Clone, Copy, PartialEq)] enum Stage4InvalidReason`:
`OffCurveBeyondChordBand`, `OnAxis`, `EllipseProjectionUnsupported`,
`InvertedTriangle`, `DegenerateTriangle`, `LoopTooSmall`, `LocalRefinementRequired`.

### 4.3 `project_onto_circle` (near the curved helpers, after `:1157`)

```
fn project_onto_circle(p: Point3, center: Point3, normal: Vector3, radius: f64)
    -> Result<(Point3, f64 /* t */), Stage4InvalidReason>
```
Closed-form radial projection (no Newton): `(e1,e2) = ortho_basis(normal)` (REUSE
`:816` so `t` matches Stage-1 sampling + `eval_source`); `w = p − center`;
`u = w·e1`, `v = w·e2`; `rho = hypot(u,v)`; guard `rho < MIN_FEATURE_SIZE` →
`Err(OnAxis)`; `t = atan2(v,u)`; `proj = center + r·(cos t·e1 + sin t·e2)`. Exact.

### 4.4 `check_watertight_2manifold`

```
fn check_watertight_2manifold(mesh: &Mesh) -> Result<(), YangError>
```
Directed half-edge multiset (every `(a,b)` has exactly one `(b,a)`) + Euler χ=2
per connected shell. Returns `Err(NonManifoldOutput)` on failure. Run once at the
end of Stage 4. (Phase-B emission re-validates topology too, but this is the
explicit Stage-4 watertightness gate per §4.4.3.)

### 4.5 `stage4_relocate_and_correct`

```
fn stage4_relocate_and_correct(
    mesh: &mut Mesh,
    infos: &[PatchInfo],            // ordered, oriented loops (cycles) + inherited surface
    incidence: &BTreeMap<(u32,u32), Vec<(InputId, Surface)>>,
    curves: &BTreeMap<(u32,u32), Curve>,
) -> Result<Vec<(u32 /* vertex */, TessellationSource)>, YangError>
```

1. **Collect + classify** (`BTreeMap<vertex, Curve>`, deterministic): every
   endpoint of every Circle intersection edge. Residual `ρ = max(|axial|,
   |radial − r|)` to the curve.
   - `ρ ≤ TAU_WORK` → already on curve: **no move**, but **retag** the source to
     `BRepEdge{edge, t}` and **mark processed** (NOT a skip).
   - `TAU_WORK < ρ ≤ d_ε` (from `curved_chord_bound`, `:1088`) → relocate.
   - `ρ > d_ε` → STOP `Stage4RegionInvalid{reason: OffCurveBeyondChordBand}`.
   - Ellipse edge endpoint → STOP `Stage4RegionInvalid{reason:
     EllipseProjectionUnsupported}`.
2. **Relocate** all candidates via `project_onto_circle`; write new positions into
   `mesh.verts` and record `(vertex, BRepEdge{edge, t})` sources.
3. **§4.5.3 sweep** per Circle loop: obtain the ordered, oriented loop from the
   owning patch's boundary `cycle` (`PatchInfo.cycles`; REUSE `patch_boundary_cycle`
   ordering — no re-derivation). At interior `p_r` (prev `p_b`, next `p_n`):
   - `t̃ = normalize(p_r − p_b) + normalize(p_n − p_r)`. If `|t̃| < TAU_WORK` →
     collinear → healthy → skip the angle test.
   - Curve tangent at `p_r`: `tC = −sin(t_r)·e1 + cos(t_r)·e2` (derivative of the
     `eval_source` circle parameterization; equivalently `n_A × n_B` up to sign —
     use the parameterization form, which is sign-consistent with the stored `t`).
   - reversal ⟺ unsigned `angle(t̃, tC) ∈ (45°, 135°)` (1e-6 rad slack on the
     bounds).
   - On reversal: **edge-collapse** `p_n` onto its surviving neighbor (replace its
     index everywhere in `mesh.tris`; the two now-degenerate tris drop by the
     existing sliver rule, `:1671-1673`, preserving half-edge pairing), reconnect,
     repeat at `p_r`; if no next point resolves, remove `p_r` and restart from
     `p_b`. Loop `< 3` verts → STOP `Stage4RegionInvalid{reason: LoopTooSmall}`;
     truly stuck → STOP `Stage4ReversalUnresolved`.
   - **After ANY collapse, recompute Phase A (adjacency/patches/cycles/incidence)
     and re-sweep** (collapse changes the loops). If recomputation is impractical
     within the function contract, STOP and report (P10) rather than sweeping stale
     loops.
4. **Validate**: per relocated triangle, signed normal vs analytic surface normal
   (`dot > 0`) and area `≥ MIN_FEATURE_SIZE²` (else
   `Stage4RegionInvalid{InvertedTriangle}` / `{DegenerateTriangle}`); then
   `check_watertight_2manifold`.
5. **No-skip audit (anti-disproven-attempt)**: maintain `processed: HashSet<u32>`;
   require it equals the relocation-set keys, else STOP. **Never `continue` past a
   Circle edge endpoint.**

## 5. RED test contract (oracles — the failing spec)

A **sidecar-independent direct path** is the GREEN gate: hand-build a
crossing-point mesh (vertices genuinely OFF a known exact `Curve::Circle`, e.g. on
chords inside the circle) + the exact curve → relocate + repair → assert. Env-gate
the real `cylinder ∪ box` E2E on `CHERCHI2022_BIN` with a LOUD `eprintln!` skip
(existing pattern, e.g. `tests/yr9_stage3_ssi.rs:1324`). Oracles:

1. **On-curve to `TAU_MODEL`**: every relocated vertex's residual `ρ ≤ TAU_MODEL`
   on the exact curve.
2. **Chord deviation strictly decreases**: max distance from the mesh intersection
   polyline to the exact curve is smaller after Stage 4 than before (proves real
   work, not a no-op) — the fixture must have off-curve crossing points.
3. **Watertight 2-manifold**: 0 unpaired half-edges; Euler χ = V−E+F = 2 (reuse
   the `unpaired_half_edges` / `euler_characteristic` helpers,
   `tests/fuzz_boxes.rs:277`, `tests/yr9_stage3_ssi.rs:319`).
4. **No reversed/inverted/degenerate triangles**: positive area; winding agrees
   with the analytic surface normal; loop vertex order matches the curve tangent
   (the §4.5.3 invariant). Include a **synthetic reversed-loop fixture** that
   exercises the collapse and confirms watertightness is preserved.
5. **TessellationMap**: relocated verts become `BRepEdge{edge, t}`; the angle `t`
   round-trips through `eval_source` (`:692/712`) to the relocated position within
   `TAU_MODEL`.
6. **Loud errors**: an ellipse edge, an on-axis projection, and an off-band
   residual each return the specific `Stage4*` error (no silent snap).
7. **Determinism**: identical inputs → byte-identical output. **Bijection
   round-trips.**
8. **Planar `fuzz_boxes` unregressed**: the planar corpus output `BRep` is
   byte-identical pre/post-PR (Stage 4 strict no-op when no Circle edges). Scope
   held (no Stage 1/2 or planar-path edits; Sphere/Cone still reject loudly).

## 6. CI gate (FULL crate, must all be clean)

```
cargo test -p yang-rs
cargo fmt -p yang-rs -- --check
cargo clippy -p yang-rs --all-targets -- -D warnings
```

## 7. On completion (close-out)

Update `docs/yang_functional_roadmap.md`: PR-YR10 done (Stage 4 — relocate mesh
intersection points onto exact curves + §4.5.3 reversed-point correction;
watertightness inherited; per Yang §4.4.1/§4.5, **not** a global CDT). Note the
superseded insert-and-fan attempt (branch `wip/yr10-insert-fan-disproven`) and
remaining work: §4.5.2 real local refinement, ellipse projection, P2b sphere,
curved Subtract. Commit + push `origin/main`.

## 8. STOP conditions (P9/P10)

If the plan's diagnosis turns out wrong (e.g. relocate cannot inherit
watertightness without a global rebuild on the real fixture, or the §4.5.3 sweep
cannot terminate on the canonical config), STOP and report what was learned. Do
NOT improvise an alternative (no global CDT, no tolerance widening, no silent
no-op). Plans are cheap; reverting hacks is expensive.
