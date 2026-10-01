# SI5 — exact ingestion of analytic STEP into the kernel-v2 arena

**Status:** DESIGN. Checkpoint 1 of the structural fix (design spec → gated-off
primitive with tests → wired increments). Created 2026-10-01.
**Plan of record it serves:** `docs/step_import_roadmap.md` §4 milestone SI5.
**Companion:** `specs/boolean_hardening_external_corpus.md` §7 deferred this
scope fork to the user; the user chose SI5 (2026-10-01).

---

## 1. What SI5 is, and what it is not

The import pipeline has three stages. Only the third changes.

```
STEP text ──truck-stepio──▶ CompressedShell<Point3, Curve3D, Surface>   (exact, analytic)
                            ╎
          current (SI1)     ╎──truck-meshalgo robust_triangulation──▶ per-face mesh
                            ╎                                         → ImportedBody (NOT in the arena)
          SI5               ╰──analytic extraction + arena assembly──▶ BrepArena SolidId (exact)
```

SI5 **replaces the tessellator** for models whose geometry is inside the
kernel's exact vocabulary. Truck still does the parsing and the assembly walk.
Writing a first-party STEP reader is a *different* axis and is explicitly **not**
in scope here (see §10).

SI5 is **not** a repair stage. It either ingests a model exactly or refuses it
loudly and the existing mesh-backed path serves it unchanged. The mesh path
remains for the freeform remainder forever; SI5 is an added tier, not a
replacement.

### Why the tessellator is the thing worth deleting

`crates/test-harness/tests/abc_probe.rs` measured (2026-09-30, recorded in
`specs/boolean_hardening_external_corpus.md` §10) that truck's **per-face**
tessellation is non-conformal: two faces sharing a curve sample it a different
number of times, so a welded import carries boundary edges proportional to face
count (p50 ≈ 0.94 per face, p90 ≈ 20.7) and closes at only 67 % for
analytic-only models, 10 % with freeform.

**Careful — two different 67 %s, do not conflate.** The one just quoted is the
*closure rate among* analytic-only models (179/268). §2.1's 67.3 % is the
*share of the corpus* that is analytic-only (6 728/10 000). The quantities are
unrelated and coincidentally almost equal.

The weld sweep in the same ledger showed loosening the weld tolerance
*corrupts* (inclusion–exclusion violations 0 → 1 → 2 as weld goes
1e-9 → 1e-7 → 1e-6) while closure does not improve.
There is no tolerance that fixes a vertex-count mismatch.

Exact analytic faces never re-sample a shared curve. The failure mode is not
mitigated, it is structurally absent.

---

## 2. Measurements

Measured over **ABC chunk 0000 = 10 000 models, 13.7 GB** (re-fetched
2026-10-01; the earlier local copy had been reaped). Reproduce with:

```sh
./scripts/fetch-abc-corpus.sh 0000 /tmp/abc
JOBS=14 ./scripts/si5_census.py    /tmp/abc/chunk0000 > /tmp/si5_census.tsv
JOBS=14 ./scripts/si5_exactness.py /tmp/abc/chunk0000 > /tmp/si5_exactness.tsv
./scripts/si5_census_report.py /tmp/si5_census.tsv /tmp/si5_exactness.tsv
./scripts/si5_face_topology.py  /tmp/abc/chunk0000 120
```

All three probes are pure text scans over the exchange file — no truck, no kernel,
no tessellation — so they measure the **input**, not our handling of it. That
independence is the point: it is why Gate 1 below can be checked against the
truck-based probe's figure and why a disagreement localizes to one of the two.
`si5_census.py` ends with a leak check that must print 0 (§2.4).

### 2.1 Ingestibility base rate

Ingestible means **every** surface in `{plane, cylinder, cone, sphere, torus}`
**and every** edge curve in `{line, circle, ellipse}`. The 2026-09-30 probe
measured only the first half; the curve half is an independent gate and it costs
more than a quarter of what the surface gate admits.

| gate | models | share |
|---|---|---|
| all 10 000 | 10 000 | 100 % |
| **Gate 1** — analytic surfaces only | 6 728 | **67.3 %** |
| **Gate 2** — Gate 1 + analytic edge curves | **5 397** | **54.0 %** (80.2 % of Gate 1) |
| lost between the gates | 1 331 | 13.3 % |

Gate 1 reproduces the probe's independently-measured 67.3 % **exactly**, which
is what gives confidence in the corrected scanner (§2.4 method note).

What the curve gate costs: `B_SPLINE_CURVE_WITH_KNOTS` in 1 330 models,
`RATIONAL_B_SPLINE_CURVE` in 98, `TRIMMED_CURVE` in 67. Supporting
`PARABOLA`/`HYPERBOLA` edges would add **zero** models — neither appears
anywhere in the corpus, so `Curve::HyperbolaArc` needs no ingestion arm and the
missing parabola variant costs nothing.

Topology SI5's assembler must handle, over the 5 397 ingestible models:

| shape | models | share | consequence |
|---|---|---|---|
| inner loops (`FACE_BOUND`) | 4 270 | **79.1 %** | `LoopKind::Inner` + exact ring winding is not optional (§5.4) |
| multi-solid (>1 `MANIFOLD_SOLID_BREP`) | 1 471 | 27.3 % | split into bodies |
| has cone | 921 | 17.1 % | single-nappe + apex rules |
| planes only (polyhedral) | 846 | **15.7 %** | the C3 beachhead: no seams, no curves |
| has torus | 723 | 13.4 % | ring-torus-only check |
| has sphere | 368 | 6.8 % | canonical z-up re-seam |
| degenerate `VERTEX_LOOP` | 275 | 5.1 % | `LoopBoundary::Lone` (§5.3) |
| voids (`BREP_WITH_VOIDS`) | 55 | 1.0 % | multi-shell solids (§5.2) |
| no solid at all (shell/geom set) | 11 | 0.2 % | refuse |
| `SURFACE_CURVE`/`SEAM_CURVE` wrapper | **0** | 0 % | no pcurve indirection to unwrap |

Size of the ingestible subset: faces p50 = 25, p90 = 149, p99 = 1 416, max
146 520; bytes p50 = 52 KB, p90 = 338 KB, p99 = 3.1 MB, max 321 MB.

### 2.2 Is the input actually exact?

This is the load-bearing question, because the arena wants every vertex on
every incident surface and the file supplies vertices and surfaces as
*independently rounded* decimals.

`si5_exactness.py` is a standalone mini-reader for the analytic subset. For
every (face, boundary vertex) incidence it computes the exact residual
|distance(vertex, face's analytic surface)| — so a vertex shared by three faces
is checked against all three, and for a planar face the residual *is* the
planarity defect.

Over the **5 397 ingestible models — 6 183 003 incidences measured**:

| per-model max residual | models | cumulative |
|---|---|---|
| ≤ 1e-15 | 3 174 | 58.8 % |
| ≤ 1e-12 (`CURVED_SURFACE_DEBUG_TOLERANCE`) | 4 272 | 79.2 % |
| **≤ 1e-9 (`TAU_EVAL` / `import_band`)** | **5 337** | **98.9 %** |
| ≤ 1e-7 (`TAU_MODEL`) | 5 382 | 99.7 % |
| ≤ 1e-6 (`MIN_FEATURE_SIZE`) | 5 396 | 100.0 % |

Distribution of the per-model max: p50 = 1.7e-16, p90 = 3.5e-11, p99 = 1.6e-9,
p99.9 = 6.0e-7, max = 1.6e-4.

**So 98.9 % of ingestible models are already exact to the arena's own
production band, and 53.4 % of the whole corpus is both in-vocabulary and
exact.** The 60 models (1.11 %) that exceed `import_band` are the loud-refusal
population §5.5 prescribes — not a reason to widen a band.

Measuring the *whole* corpus instead gives a much worse-looking 91.7 % ≤ 1e-9,
and the worst outlier (1.3e-3 m) turned out to be a planar face bounded by a
`B_SPLINE_CURVE_WITH_KNOTS` — a model Gate 2 rejects anyway. Residual
statistics are only meaningful on the subset SI5 would actually accept; quoting
the all-models figure would be measuring a population we never ingest.

### 2.3 Cylindrical face topology (the §5.1 input)

Over 120 corpus files containing cylinders (1 609 cylindrical faces), the
distribution of loop/edge shape is in §5.1's table. The headline: **50.9 % of
cylindrical faces already arrive in kernel-v2's canonical 4-edge `CCLL` lateral
form**, 40.5 % as two single-circle loops needing a minted seam. This was assumed
backwards before being measured, and it moves seam minting from "the biggest
piece" to "needed for a large minority".

Units: all 10 000 models declare `SI_UNIT($, .METRE.)` alongside a
`CONVERSION_BASED_UNIT('METRE', …)` complex instance. `units.rs:29-43` handles
the bare-`.METRE.` form, so the corpus needs no unit work — but note the
default-to-millimetres fallback (`units.rs:48-51`) would be a silent 1000×
error on any file whose unit entity we fail to parse, which is worth a loud
refusal rather than a warning once SI5 cares about exactness.

### 2.4 This overturns the 2026-07-11 decision

`docs/step_import_roadmap.md` §0 conclusion 1 and §6 ledger say exact ingestion
is impossible in general because *"OCC-written files are only closed to OCC
tolerance — they would not pass `validate_solid` exactness invariants anyway"*,
and `crates/kernel-v2/src/imported.rs:9-12` repeats it as the module's reason
for existing.

The premise conflated a file's **declared** uncertainty with its **actual**
residual. `UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-06))` is a
conformance declaration, not a measurement of the geometry in the file. Measured
on the three OCC/KiCad fixtures in `refs/step/` — the exact files the 2026-07-11
decision was formed on:

| fixture | writer | faces | incidences | p50 | p90 | p99 | max | declared |
|---|---|---|---|---|---|---|---|---|
| `R_0603.step` | OCC (KiCad) | 26 | 272 | 0 | 4.2e-17 | 4.2e-17 | **4.2e-17** | 1e-7 |
| `SOT-23.step` | OCC (KiCad) | 53 | 540 | 6.9e-19 | 4.8e-15 | 3.6e-13 | **3.6e-13** | 1e-7 |
| `USB_C.step` | OCC (KiCad) | 515 | 4972 | 0 | 1.4e-15 | 3.3e-13 | **8.8e-13** | 2e-7 |

Max residual 8.8e-13 m, five orders below the declared 2e-7 and inside
kernel-v2's `CURVED_SURFACE_DEBUG_TOLERANCE = 1e-12`, let alone `TAU_EVAL =
1e-9` and `validate.rs:714 import_band ≈ 1e-9·(1+scale)`.

**The conclusion that survives is the narrower one:** b-splines are out of
vocabulary, so a *general* import cannot be an arena solid. Closure tolerance
was never the obstacle. SI5's gate is the surface/curve vocabulary, nothing else.

> **Method note — a fabricated finding, caught.** The first run of
> `si5_exactness.py` reported a 9.9 m max residual on `USB_C.step`. That was my
> number regex failing to match OCC's `-1.E-02` form (digits, point, no
> fractional digits, exponent) as one token: it split one coordinate into two
> and fabricated a fourth. Onshape writes full digits, so the bug was invisible
> on all 10 000 ABC models and only showed on the KiCad files. Had I recorded
> the 9.9 m figure it would have "confirmed" the 2026-07-11 premise. The census
> had a mirror-image bug: `= NAME(` misses rational b-splines, which are written
> *only* as complex instances (`#9 = ( BOUNDED_SURFACE() B_SPLINE_SURFACE(…) …)`),
> inflating the analytic share — found by reconciling against the probe's
> independent 67.3 % figure. Both scanners now token-scan, and the blind-spot
> sweep in `si5_census.py` prints any unclassified geometric entity name so the
> census cannot silently miss a type we have never seen.

---

## 3. Where the analytic data is, and that nothing needs re-parsing

`crates/step-import/src/convert.rs:144` is the single line where exactness dies:

```rust
let meshed: MeshedCShell = shell.robust_triangulation(tol);
```

`CompressedShell<Point3, PolylineCurve<Point3>, Option<PolygonMesh>>` keeps the
topology **verbatim** (`truck-meshalgo/src/tessellation/triangulation.rs:158-189`:
`vertices.clone()`, `edge.vertices` indices, `boundaries`, `orientation`) and
replaces exactly two things: per-edge `Curve3D` → polyline, per-face `Surface` →
mesh. `parse_step_impl` still holds the analytic `Vec<CShell>` in scope at that
moment (`convert.rs:30`, `:40-43`) and already reads back from it for
classification (`:216`). **SI5's extraction point is right there, with no
re-parse and no second truck invocation.**

What truck hands us is already a half-edge structure in index form
(`truck-topology/src/compress.rs:16-93`):

```rust
pub struct CompressedEdge<C>      { pub vertices: (usize, usize), pub curve: C }
pub struct CompressedEdgeIndex    { pub index: usize, pub orientation: bool }
pub struct CompressedFace<S>      { pub boundaries: Vec<Vec<CompressedEdgeIndex>>,
                                    pub orientation: bool, pub surface: S }
pub struct CompressedShell<P,C,S> { pub vertices: Vec<P>, pub edges: Vec<CompressedEdge<C>>,
                                    pub faces: Vec<CompressedFace<S>> }
```

Two faces adjacent across an edge reference the **same** `edges[i]` with
opposite `orientation`. So SI5 needs no geometric stitching, no coordinate
dedup, and no adjacency derivation — the three things `ImportedBody` currently
does by hand (`crates/kernel-v2/src/imported.rs:112` interns vertices by a
bit-exact O(n) linear scan).

Three things ARE lost before `convert_shell` and must be captured upstream of it:

1. **Shell→solid grouping.** `collect_placed_shells` flattens
   `CompressedSolid::boundaries` into a flat `Vec<CShell>` (`convert.rs:76`), so
   which shell was the outer boundary and which were voids is gone. SI5 needs it
   (§5.2).
2. **Loop grouping and per-edge orientation.** `edge_indices` is built as a flat
   deduplicated `Vec<u32>` that never reads `ei.orientation` (`convert.rs:202-213`).
3. **Face orientation folded into geometry.** `orientation == false` inverts the
   polygon and negates the plane normal (`convert.rs:171-174`, `:252-253`). SI5
   must keep orientation as orientation — it decides `Surface::*::reversed`.

---

## 4. Vocabulary mapping

### 4.1 Target

`crates/kernel-v2/src/arena.rs` — both enums are `#[non_exhaustive]`, `Copy`:

```rust
pub enum Surface {                                                      // arena.rs:166
    Plane(Plane),                                  // { point, normal }
    Cylinder { axis_point: Point3, axis_dir: UnitVector3, radius: f64, reversed: bool },
    Cone     { apex: Point3, axis_dir: UnitVector3, half_angle: f64, reversed: bool },
    Torus    { center: Point3, axis_dir: UnitVector3, major_radius: f64,
               minor_radius: f64, reversed: bool },
    Sphere   { center: Point3, radius: f64, reversed: bool },
}
pub enum Curve {                                                        // arena.rs:283
    LineSegment,
    SurfacePair { a: PairSurface, b: PairSurface },
    Circle      { center: Point3, normal: UnitVector3, radius: f64 },
    Arc         { center: Point3, normal: UnitVector3, radius: f64 },
    EllipseArc  { center, normal, major_axis, major_radius, minor_radius },
    HyperbolaArc{ center, normal, major_axis, semi_transverse, semi_conjugate },
}
```

### 4.2 The map

| STEP entity | truck type | kernel-v2 | notes |
|---|---|---|---|
| `PLANE` | `ElementarySurface::Plane` | `Surface::Plane` | `plane.rs:18,21,34` give `u_axis/v_axis/normal` |
| `CYLINDRICAL_SURFACE` | `Processor<RevolutedCurve<Line<Point3>>, Matrix4>` | `Surface::Cylinder` | axis = `RevolutedCurve::origin()/axis()`; radius = distance from axis to the generating `Line` |
| `CONICAL_SURFACE` | same `Processor<RevolutedCurve<Line>>` | `Surface::Cone` | apex + half-angle from the slant line; **single nappe, `half_angle ∈ (0, π/2)`**, on-surface requires `(p−apex)·axis > 0` (`arena.rs:209-228`) |
| `SPHERICAL_SURFACE` | `Processor<Sphere, Matrix4>` | `Surface::Sphere` | `Sphere { center, radius }`, private fields, accessors only |
| `TOROIDAL_SURFACE` | `Processor<Torus, Matrix4>` | `Surface::Torus` | **ring torus only**: `major > minor` enforced (`validate/faces.rs:752-756`) |
| `LINE` | `Curve3D::Line` | `Curve::LineSegment` | range from `BoundedCurve::range_tuple()` |
| `CIRCLE` | `Curve3D::Conic(Conic3D::Ellipse)` with equal radii | `Curve::Circle` (closed) / `Curve::Arc` | STEP has no separate circle type in truck — a `CIRCLE` is a `UnitCircle` under a `Processor` matrix |
| `ELLIPSE` | `Conic3D::Ellipse` | `Curve::EllipseArc` | |
| `HYPERBOLA` | `Conic3D::Hyperbola` | `Curve::HyperbolaArc` | measured: 0 occurrences in the corpus |
| `PARABOLA` | `Conic3D::Parabola` | **none** | no arena variant → gate rejects |
| b-spline / NURBS / swept / offset, `POLYLINE`, `TRIMMED_CURVE`, `PCURVE` | `BSplineSurface`/`NurbsSurface`/`SweptCurve`, `Curve3D::{BSplineCurve,NurbsCurve,PCurve,Polyline}` | **none** | the gate condition |

The analytic parameters are all recoverable through public accessors —
`Processor::{entity,transform,orientation}` (`decorators/processor.rs:27,31,35`),
`RevolutedCurve::{origin,axis,entity_curve}` (`decorators/revolved_curve.rs:262,265,256`),
`Torus::{large_radius,small_radius}`, `TrimmedCurve { curve, range }`
(`decorators/mod.rs:285`, where the range of a circle **is** its angular span).

### 4.3 Two traps

**The committed cylinder fixture is not representative.**
`crates/step-import/src/lib.rs:168` (`parse_cylinder_fixture_classifies_lateral`)
asserts the lateral face of our own `tests/fixtures/cylinder.step` is *not*
planar, with the comment "truck writes the full-turn lateral as swept/rotated
geometry". That fixture was **written by truck's own `out` module** (roadmap §5),
which emits `SURFACE_OF_REVOLUTION` rather than `CYLINDRICAL_SURFACE`. So our
only committed curved fixture would be rejected by SI5's gate while every real
cylinder passes. SI5 needs fixtures from real writers — see §7.

**Transforms must be applied to the analytic shell, not after extraction.**
`place_shell` (`convert.rs:114-131`) already does this exactly, via truck's
`TransformedM4` on `e.curve.transform_by` and `f.surface.transform_by`. It is
the only path on which curve geometry is ever transformed, and it is the model to
follow: transform the `CShell`, then extract. By contrast
`ImportedBodyData::apply_placement` (`waffle-types/src/kernel/import.rs:135`)
has a transform arm only for `Plane`, because the other descriptors are
parameterless today. (Note the roadmap's promised `transform_imported` never
shipped under that name.)

---

## 5. The hard parts, in dependency order

These are *capability*, not tolerance. Ranked by how much structure each needs.

### 5.1 Seams and periodicity

kernel-v2 requires Stroud 2006 §3.1.4's single-fake-edge form
(`arena.rs:36-57`): a closed cylinder is `V=2, E=3, F=3, R=0, S=1, G=0` — two
seam anchor vertices, two closed rim `Circle` half-edges, one straight seam
`LineSegment`, and a **lateral loop of four half-edges**
`[rim_b, seam_up, rim_t, seam_dn]` in which the seam appears twice, once in
each direction.

**Measured, not assumed** — `./scripts/si5_face_topology.py /tmp/abc/chunk0000 120`
(deterministic seeded sample: 120 corpus files containing cylinders, 1 609
cylindrical faces):

| loop/edge signature of a `CYLINDRICAL_SURFACE` face | curves | faces | meaning |
|---|---|---|---|
| one loop of 4 edges | `CCLL` | 819 (50.9 %) | **already kernel-v2's canonical lateral** |
| two loops of 1 edge | `C` + `C` | 651 (40.5 %) | full band, **seam must be minted** |
| one loop of 4 | `EELL` / `CELL` | 112 loops | obliquely cut — `EllipseArc` rims |
| 5/6/8-edge loops, `(1,4)`, `(1,8)`, … | mixed | ~140 (8.6 %) | partial patches — the tail below |

So the dominant real-world form **already matches the target topology**, which
is the opposite of what this section assumed before it was measured. Seam
minting is needed for the two-circle-loop case (~40 % of cylindrical faces), not
for the majority. `extrude_circle` (`construct/extrude.rs:266-425`) is the
assembly template and `recover.rs:30-40` is the precedent for minting an exact
on-circle seam foot where none exists.

Closed torus (`V=1,E=2,F=1,G=1`, loop `aba⁻¹b⁻¹`) and closed sphere
(`V=2,E=1,F=1`, one meridian `Arc` twin pair) have their own canonical
assemblies at `construct/revolve/closed.rs:321` and `:449`.

One constraint to respect while mapping the `CCLL` form: `Arc`/`EllipseArc` are
**minor arcs only** (sweep < π) and a near-half arc is *rejected* as ambiguous
rather than guessed (`boolean::ARC_MINOR_AMBIGUITY_BAND = 1e-6`). A STEP rim
written as one full `CIRCLE` maps to the closed `Curve::Circle`; a rim split
into two half-arcs does not, and must be recognized and re-joined.

Two of those canonical forms are **required for boolean re-entry**, not merely
for validation:
- a cylinder must present **exactly two full-circle rims** or `to_yang_brep`
  rejects it with `UnsupportedCurvedBoolean` (`boolean/mod.rs:23-28`);
- the sphere's seam frame is **canonically world-z-up regardless of
  construction** (`arena.rs:266-275`), because yang Stage 1's parameterization
  is fixed z-up. A STEP sphere placed on another axis must be re-seamed.

A partially-trimmed curved face (a cylinder cut by non-circular edges) can be
made to validate and tessellate but will bounce off `to_yang_brep`. **Decision
for SI5: ingest it anyway and let the typed wall speak.** A tessellation-only
tier that renders, measures, and exports but refuses booleans is strictly better
than today's mesh-backed body, and the wall is already typed and loud.

### 5.2 Shells, voids, genus

`Shell::genus` is **stored state with no derivation helper** — only `kfmrh`
increments it in the Euler path. STEP's `CLOSED_SHELL` gives no genus. Copy
`from_yang_brep`'s back-solve: connected components over faces by shared
undirected edge, then per component `g = (2 − lhs)/2`, rejecting an odd
characteristic with a typed error (`boolean/from_yang.rs:917-955`).

`BREP_WITH_VOIDS` needs the outer shell plus void shells in one `Solid`, which
means capturing `CompressedSolid::boundaries` before `convert.rs:76` flattens it.

### 5.3 Degenerate loops

`VERTEX_LOOP` (a loop that is a single vertex — cone apex, sphere pole) maps to
`LoopBoundary::Lone(VertexId)`, which the arena already has (`arena.rs:502`).
The apex-cone assembler `build_on_axis_apex_cone`
(`construct/revolve/on_axis.rs:481`) is the existing precedent.

### 5.4 Inner loops and orientation

Rings become `LoopKind::Inner` and must satisfy the **exact** signed-area rule:
`geom::planar_loop_signed_area(n, &pts, &curves) > 0` for the outer loop, `< 0`
for every ring (`validate/faces.rs:34-118`, closed form over chords plus each
arc's segment term). STEP's own loop orientation plus `CompressedFace::orientation`
plus `CompressedEdgeIndex::orientation` determine this; SI5 must compute the
signed area and reject a disagreement rather than flip to taste.

Curved faces have their own orientation law — the unrolled-winding rule for
cylinder patches (`validate/faces.rs:889`, `:540-555`) and "each rim's traversal
axis points toward the opposite rim" for canonical laterals. `Curve::Circle::normal`
is **directional** (the axis about which *this* half-edge runs CCW) and twins
must carry **exactly negated** normals — `curves_twin_consistent` compares with
exact `==` (`validate.rs:514-594`).

### 5.5 Reconciliation — bounded, vetoing, never repairing

§2.2 says residuals are ~1e-13 and the arena's production-tier gates are ~1e-9,
so for the measured corpus **no reconciliation is needed at all**. SI5 must
still decide what happens to a model that is worse, and the answer is the
existing house pattern, not a new band:

- `canonicalize_vertices_to_planes` (`boolean/canonicalize.rs:98`) re-derives an
  all-planar vertex from its incident planes in **exact rational arithmetic**
  (dashu `RBig`; Cramer for ≥3 planes, exact projection for 2), adopts the move
  only if it is `≤ TAU_WORK·(1+|coord|)` per component, **vetoes the vertex if
  any incident face is curved**, and floors conditioning at `DET_FLOOR = 1e-9`.
- `relocate_onto_implicit_pair/_triple` (`yang-rs/src/stage4_relocate.rs:259,333`)
  are the curved analogue — "reconcile a vertex against its 2 or 3 incident
  surfaces", residuals ≤ `TAU_MODEL`, `None` = loud STOP on tangency or
  non-convergence. They are `pub(crate)` to yang today; SI5 wants them promoted.

**Rule for SI5: measure the residual, and if it exceeds the arena's own
production band, REFUSE the model with a typed error naming the face and the
residual.** Do not widen a band to admit it (P9/P10). The mesh path still serves
it. A refusal is a roadmap item with a measurement attached, which is exactly
what the P-series promotion path consumes.

### 5.6 Resource safety — and the panic that reaches the browser

Measured 2026-09-30 (`abc_probe.rs:182-190`): an uncapped in-process import loop
reached **41 GB RSS and 14 CPU-minutes on a single model**; two models out of 400
were 57 % of all import CPU. Real STEP has a 540 MB tail.

Separately, **truck panics rather than erroring** on some real input
(`truck-geometry/src/specifieds/sphere.rs:134`, "tolerance must be no less than
1e-6"), 2 models in 400. Natively we build `panic = "unwind"` (workspace
`Cargo.toml:51`) and the probe wraps import in `catch_unwind`. **On wasm32 we
build panic=abort deliberately** (`.cargo/config.toml` carries only the 4 MB
stack flag; the former `panic=unwind` + `-Zbuild-std` two-step died with the
legacy kernel) — so that panic is an unrecoverable module abort in the user's
browser, on a file we handed truck. SI5 does not fix this and must not be
credited with fixing it; it is tracked here because SI5 is the first consumer
that reads *more* of truck's output surface. Mitigation is a separate increment:
a pre-flight entity/validity screen in our own code before the text reaches
truck, which SI5's census scanner is already most of.

---

## 6. What SI5 collapses

Counted in `crates/kernel-v2/src/adapter.rs`: **13 `self.imported_slot_of(…)`
branch points** (lines 481, 827, 852, 933, 956, 990, 1508, 1523, 1545, 1557,
1569, 1584, 1876), three imported-only helpers (`:160`, `:166`, `:192`), and the
`TAG_IMPORTED_*` id namespace threaded through the signature and listing paths —
plus the whole 285-line parallel store `crates/kernel-v2/src/imported.rs`.
Several further methods reject imported ids only by falling through to a
catch-all (`entity_axis` at `:1758`, `face_provenance` at `:1933`). An ingested
model stops being a special case and gains, for free, everything the branches
currently refuse: `transform_body` (`:827`), `mirror_body` (`:852`),
`export_step_bodies` (`:933`), `solid_volume` (`:1508`),
`solid_surface_area` (`:1545`), `entity_axis` for imported cylinders (`:1758`,
where the arena arms at `:1765-1801` already work), `face_provenance` (`:1933`),
and the boolean wall itself (`run_boolean_solid`, `:478-486`).

`MockKernel::import_body` (`waffle-types/src/kernel/mock.rs:873`) already takes
this approach — it synthesizes ordinary entities and therefore has **zero**
imported branches elsewhere. That is the shape to converge on.

SI5 does **not** delete `imported.rs`: the freeform tier still needs it. The
goal is that the branch is taken by freeform models only.

---

## 7. Checkpoints

Each is an atomic, committable increment. Nothing after C1 touches app code.

- **C1 — this spec + the measurement tooling.** Promote `si5_census.py` and
  `si5_exactness.py` to `scripts/`, record §2, update
  `docs/step_import_roadmap.md` §6 ledger to supersede the 2026-07-11 "exact
  ingestion impossible" entry with the measurement that overturns it. No Rust.
- **C2 — the analytic extraction contract, gated off.** A new
  `AnalyticBrepData` in `waffle-types` (shell/face/loop/edge/vertex index tables
  carrying `AnalyticSurface` + `AnalyticCurve` with full parameters and
  orientation preserved) plus `step_import::extract_analytic(&CShell) ->
  Result<AnalyticBrepData, Ineligible>` with a typed `Ineligible` naming the
  entity that failed the gate. Pure data, no arena, no kernel dependency. Tests:
  the two committed fixtures plus new real-writer fixtures; assert the cylinder
  fixture's `SURFACE_OF_REVOLUTION` lateral is reported `Ineligible`, not
  silently coerced (§4.3).
- **C3 — arena assembly, planar-only.** `kernel_v2::ingest_analytic` for models
  whose every face is a `PLANE` (measured: the polyhedral share of the
  ingestible subset, §2.1) — no seams, no curved surfaces, `Curve::LineSegment`
  only. Follow `from_yang_brep`'s pass structure: validate everything *before*
  the first arena mutation, pre-allocate ids, wire twins via a
  `(v_min, v_max, curve_key)` table, exit through `finalize_solid`. Gate: not
  reachable from the app yet. Oracles per §8.
- **C4 — cylinders and cones, with seam minting.** §5.1. The first increment
  where a model gains full boolean capability.
- **C5 — spheres and tori.** Including the canonical z-up re-seam.
- **C6 — wire it.** `import_body` tries `ingest_analytic` first and falls back to
  the mesh-backed body on `Ineligible`; the fallback must be visible as a feature
  warning, never silent. Collapse the adapter branches that the arena path makes
  dead for analytic models.
- **C7 — corpus gate.** Promote a sampled SI5 tier into the assay/prospector
  path so a regression in ingestion is a red test, per
  `specs/boolean_hardening_external_corpus.md`.

A C-step that turns out to need a design decision of its own stops and gets its
own spec, the way #137 and #168 did.

---

## 8. Oracles

An ingested solid is epistemically on the **boolean** path, not the constructor
path: its geometry is not "guaranteed by construction", so the debug-only
on-surface tripwire is not enough. Every ingestion must run, in production:

1. `validate_solid` (`validate.rs:276`) — twin pairing, loop closure, vertex
   manifoldness, per-surface orientation, and the exact integer Euler–Poincaré
   `V − E + F − R == 2(S − G)`.
2. `validate_boolean_output_planarity` (`validate.rs:168`) — every planar face's
   loop vertices within `TAU_EVAL·(1+max|coord|)`.
3. `validate_boolean_output_self_intersection` (`validate/selfx.rs:53`).
4. An **import-tier on-surface gate**: every vertex against every incident
   surface within `import_band` (`validate.rs:714`), typed, naming face and
   residual. This is §2.2's measurement promoted to a runtime check.

Independent cross-checks, reusing what exists rather than inventing:

5. **Volume agreement between the two paths.** The same file ingested exactly and
   tessellated by truck must agree in volume to the chord-error bound. This is a
   genuine differential oracle and it is free — both paths already exist, and
   `abc_probe.rs:78` already has a divergence-theorem `mesh_volume`.
6. **`solid_volume` vs the harness's independent volume oracle**
   (`test-harness/src/assay/volume_oracle.rs`, exact in z, measured band).
7. **Round-trip through `kernel_v2::step_export`** — an analytic AP214 export of
   an ingested solid, re-imported, must be the same solid. The truck round-trip
   oracle in `wasm-bridge/tests/step_export_roundtrip.rs` is the existing
   harness.
8. **Inclusion–exclusion** on booleans between two ingested models
   (`|A∪B| + |A∩B| == |A| + |B|`), the identity that caught the weld corruption.

---

## 9. Risks

- **Seam minting is the whole game.** If choosing seam anchors interacts badly
  with yang's rim-ring caching, C4 stalls and only the planar tier lands. That is
  still worth shipping: it is the polyhedral share of the corpus, exactly, with
  full booleans.
- **Canonical-form mismatch is a capability tail, not a bug.** Expect a
  population of ingested solids that validate and render but refuse booleans with
  `UnsupportedCurvedBoolean`. Each is a typed, measured roadmap row.
- **Truck's panic class reaches the browser** (§5.6). Not SI5's to fix, but SI5
  increases our exposure to truck's output surface, so the pre-flight screen
  should land near C6.
- **`ImportedSurface` must not be the gate.** It is a 6-way classification with
  no parameters and it already mislabels our own cylinder fixture as `Freeform`.
  The gate belongs on truck's `Surface`/`Curve3D` variants, upstream.
- **ABC is one corpus from one writer.** Onshape and OCC both measured clean, but
  "every writer is this clean" is not established. The import-tier gate (oracle 4)
  is what makes a dirty writer a loud refusal instead of a silent wrong answer.

---

## 10. Explicitly not in scope: a first-party STEP reader

Owning the parser is a different axis — it replaces stage 1, where the measured
defects are *not*: 393 of 400 models parsed cleanly, and the thing costing us
two thirds of the corpus is per-face tessellation. The arguments for owning it
(truck's panic-instead-of-error reaching a panic=abort WASM build; a git pin to
an unreleased master rev on the one component every external model passes
through; 1.51 → 1.92 MB gzipped of bundle) are real but none is urgent, and
SI5's extraction work is the half that survives either decision: exactness
reconciliation, loop→half-edge assembly, and `validate_solid` are
representation-independent.

Revisit when freeform surfaces must be exact, the git pin becomes unavailable,
or bundle size forces it. By then SI5 will have established exactly which entity
subset a reader would need — and `si5_census.py` already measures it.

---

## 11. Ledger

- 2026-10-01 — **C1 DONE.** Spec created. ABC chunk 0000 re-fetched
  (`scripts/fetch-abc-corpus.sh`); census and exactness probes written and
  promoted to `scripts/`; two scanner bugs found and fixed (§2.4 method note).
  Findings: the edge-curve gate takes SI5's reach to 54.0 % (not 67 %); 98.9 %
  of admitted models are already exact to `import_band`; the 2026-07-11 "OCC
  tolerance" premise is refuted; and 51 % of cylindrical faces already arrive in
  kernel-v2's canonical `CCLL` lateral form, which §5.1 had assumed backwards.
  Checkpoints C1–C7 defined. Next: **C2**, the analytic extraction contract —
  pure data, gated off, no arena.
