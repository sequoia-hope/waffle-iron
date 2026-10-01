# SI5 — exact ingestion of analytic STEP into the kernel-v2 arena

**Status:** IN PROGRESS — **C1, C2, C3 and C4a landed 2026-10-01, next is the
tripwire-tier increment (§5.5) then C4b.**
Nothing is reachable from the app yet; the mesh tier still serves every import.
Created 2026-10-01. Checkpoints and their state: §7.
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
./scripts/si5_seam_shape.py     /tmp/abc/chunk0000 400
```

All these probes are pure text scans over the exchange file — no truck, no kernel,
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
| degenerate `VERTEX_LOOP` | 275 | 5.1 % | **refused** — the reader drops it silently (§5.3) |
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

Over 400 corpus files containing a cylinder or cone (6 316 such faces), the
form distribution is in §5.1's table. The headline: **58.4 % are partial arc
patches and 37.9 % are full bands, every one of which needs a minted seam.**

A loop-size census (120 files, `si5_face_topology.py`) was taken first and read
backwards — its 50.9 % `CCLL` share was recorded here as "already kernel-v2's
canonical lateral form", which would have made seam minting a minority concern.
It is the opposite: `CCLL` is the *partial patch*, because the two line edges are
distinct records and the two circle edges are open arcs, neither of which an
edge-count signature can see. §5.1 carries the refutation and the probe
(`si5_seam_shape.py`) that made it.

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

### 2.5 What the extractor actually achieves (measured at C2)

§2.1's 54.0 % is a gate on the raw exchange file, so it is an **upper bound**.
What `step_import::parse_step_analytic` achieves is a separate, independent
measurement — it gates on what truck actually parsed — and comparing the two is
the cheapest coherence check available:

```sh
ABC_DIR=/tmp/abc/chunk0000 ABC_N=400 cargo test -p test-harness \
  --test si5_analytic --release -- --ignored --nocapture extractor_eligibility
```

Over 400 models (2026-10-01): **197 fully eligible (49.3 %)**, 191 rejected by
both, **97.0 % agreement, and all 12 disagreements in the same direction** —
census eligible, extractor stricter. Zero cases of the extractor accepting
something the census rejected, which is the direction that would indicate a
leak in the gate.

The 4.7-point gap between 54.0 % and 49.3 % is fully accounted for:

| cause | models | verdict |
|---|---|---|
| source declares a `VERTEX_LOOP`, which truck's reader **silently drops** | 9 | deliberate refusal to avoid a silently-wrong solid — §5.3 |
| spindle torus (`minor ≥ major`) | 1 | correct refusal — out of the kernel's vocabulary |
| file contains no solid or shell | 2 | correct refusal — the census cannot see this from entity names |

The `VERTEX_LOOP` row is the one that matters, and it is the reason this
measurement moved: it was **50.5 % at 98.2 %** before the guard existed, when
those files were being extracted from a topology the reader had quietly
truncated. §5.3 has the mechanism and the verification. Giving up 2.3 points of
reach to turn a class of silent wrongs into a loud stop is the trade this
project takes every time (P9/P10).

Real-world OCC geometry, by contrast, extracts completely:

| fixture | result |
|---|---|
| `R_0603.step` | fully eligible; 26 faces; `{cylindrical, planar}` |
| `USB_C.step` | **34/34 shells eligible**; 515 faces; cones and tori included |

Both run the extraction's own oracle — every boundary vertex of every face
re-checked against the surface *as extracted*, within 1e-9 relative. That is
what makes the parameter extraction trustworthy rather than merely compiled:
a transcription error in any axis, radius or apex shows up as a residual.

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

#### The loop-signature census was one level too shallow — and said the opposite

This section first recorded `./scripts/si5_face_topology.py`'s loop-size
signatures and concluded from them that the dominant `CCLL` form (one loop of
circle, circle, line, line) "already matches the target topology", so that seam
minting was the minority's problem. **That inference is wrong.** A signature
counts edges per loop; it cannot see an edge's own endpoints or its record
identity, and both distinctions are exactly where the canonical form lives.
`./scripts/si5_seam_shape.py /tmp/abc/chunk0000 400` goes the level deeper
(400 corpus files containing a cylinder or cone, 6 316 such faces) and finds, in
the `CCLL` faces:

- the two `L` edges are two **distinct `EDGE_CURVE` records**, never one seam
  traversed twice — 1 305 of 1 305 measured;
- the two `C` edges are **open** (`edge_start != edge_end`), i.e. circular
  **arcs** — 2 610 of 2 610.

So `CCLL` is not a full band with a seam. It is the *partial* patch — a fillet,
a half-round, a rounded corner — bounded by two arcs and two rulings. The
canonical full band arrives only in the 2×single-closed-circle form, and that
form always needs a minted seam. Dumping one `CCLL` face confirms it edge by
edge (`00007847_…_step_000.step` face `#35`: a half-cylinder of radius 0.01
whose "rims" are two diametral arcs and whose "seams" are `#224` and `#226`,
different records).

Re-measured on the honest taxonomy — **full band** = every loop is one closed
circle; **arc patch** = no closed circle anywhere:

| form of a `CYLINDRICAL`/`CONICAL_SURFACE` face | faces | share | what it needs |
|---|---|---|---|
| arc patch | 3 686 | 58.4 % | `Curve::Arc` mapping + the unrolled-domain outer-loop test; no boolean re-entry |
| full band, 2 rim loops | 2 397 | 37.9 % | **a minted seam**; full boolean capability |
| mixed (closed circle + open edges in one face) | 172 | 2.7 % | refusal — the arena has no such face |
| 1 / 3 / 4 / 6 / 10 rim loops | 38 | 0.6 % | refusal (holed band, unclosed band) |

Per **model** — which is what decides reach, since one unsupportable face sends
the whole file to the mesh tier — of the 400 cylinder/cone-bearing models:

| | models | share |
|---|---|---|
| outside the C4 surface/curve vocabulary (b-splines, …) | 202 | 50.5 % |
| in vocabulary, **full bands only** | 81 | **20.2 %** |
| in vocabulary, both forms | 71 | 17.8 % |
| in vocabulary, arc patches only | 32 | 8.0 % |
| in vocabulary, some face form outside both | 14 | 3.5 % |

That table is the checkpoint split (§7): the full-band tier alone is 20.2 % of
cylinder-bearing models **with full boolean capability**, and it is self-contained
— a model whose curved faces are all full bands cannot contain a planar arc
either, because an arc edge's other face is always a curved patch.
`extrude_circle` (`construct/extrude.rs:266-425`) is the assembly template and
`recover.rs:30-40` is the precedent for minting an exact on-circle seam foot
where none exists.

#### Minting a seam means choosing where to cut a circle that has no cut

The minted seam must be a **ruling** (`validate_cylinder_face`'s
`cyl-seam-not-ruling` check), so the two rims' anchor vertices have to sit at
one azimuth about the axis. They do not always:

- **82.4 %** of the 2 397 full bands already have their two anchors aligned to
  < 1e-9 rad; the rest are spread to p90 = π/2, max = π.
- A rim anchor is named by another edge in **4 of 4 794** cases (0.08 %).

So minting also has to **re-anchor** a rim. That is not a repair of geometry and
not a tolerance move: the anchor of a *closed* edge is pure representation gauge
— Stroud's fake edge — and sliding it along its own circle changes no point of
the boundary, because the loop is the entire circle either way. It is admissible
exactly when the vertex is load-bearing for nothing else, which is why the 0.08 %
is measured rather than assumed, and those faces are refused instead.

The re-anchored point is computed as `c₁ + r₁·ĝ₀` — the other rim's own centre
and radius, along the kept anchor's unit radial direction — so it lands exactly
on the circle the file declared, and therefore on both surfaces that circle
bounds. The import-tier on-surface gate certifies it like any other vertex.

Alignment is not pairwise: a stepped shaft chains bands rim → band → rim, and
two bands sharing a rim are necessarily coaxial (a shared full circle is each
surface's own rim). So the constraint is **per connected component of
rims-joined-by-bands**: pick one anchor direction per component and re-anchor the
rest to it. A component containing two *pinned* rims (anchors shared with other
edges) whose directions disagree is a refusal.

#### Which way a rim circle is traversed is derived, never read

A closed rim gives no orientation clue from its endpoints — both traversals run
start → start, and `AnalyticCurve::interior` does not separate them either (a
full circle passes through every one of its own points in both directions). The
file's circle axis would say, but it is exactly the kind of sign that
`same_sense` taught us not to replay (truck inverts curves during parsing, the
`Processor` carries its own orientation flag, and `EDGE_CURVE.same_sense` is a
third). So C4 derives it from the surface's own material law instead:

- on the lateral, `validate_cylinder_face`/`validate_cone_face`'s rule — with
  `reversed == false` each rim's traversal axis points **toward** the other rim,
  with `reversed == true` away — and `reversed = !same_sense`;
- on the face across that rim, the negation (twins carry negated circle normals).

The file's declared circle axis is then used only as a geometric consistency
check (`|n̂·â| ≈ 1`), and the oriented-edge flags only for the thing pass 1d
already verifies: that the rim's two uses run opposite ways. Nothing in the
result depends on an absolute sign we cannot trust.

Closed torus (`V=1,E=2,F=1,G=1`, loop `aba⁻¹b⁻¹`) and closed sphere
(`V=2,E=1,F=1`, one meridian `Arc` twin pair) have their own canonical
assemblies at `construct/revolve/closed.rs:321` and `:449`.

A note on a wrong turn, kept because it is the kind of mistake this spec exists
to prevent: C2 first saw faces arriving with **no bounds at all** and recorded
them as legitimately seamless spheres and tori, i.e. as early customers for seam
minting. They are not. Those files *do* declare a bound — `FACE_BOUND` →
`VERTEX_LOOP` — which truck's reader discards (§5.3). The symptom was the
reader's, not the format's, and the fix belongs in §5.3, not here. Verified on
`00000052_666139e3bff64d4e8a6ce183_step_001`: `#264 = FACE_BOUND('', #473, .T.)`
with `#473 = VERTEX_LOOP('', #613)`.

One constraint that turns out **not** to bind, and it is worth saying why: the
minor-arc rule. `from_yang_brep` rejects a near-half arc as ambiguous
(`boolean::ARC_MINOR_AMBIGUITY_BAND = 1e-6`) because it must *derive* the arc's
directional normal from its two endpoints, where a half turn is genuinely
undecidable. The arena's `Curve::Arc` itself has no such limit —
`geom::ccw_sweep` is well defined on all of `(0, 2π]` once the normal is given —
and an imported arc arrives with the file's own axis plus
`AnalyticCurve::interior`, which pins the side. That matters, because exact and
near half turns are **11 %** of arc-patch arcs in the corpus (539 exact + 207 in
179–180°, of ~6 456). C4b may therefore take them; it must not re-derive.

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
(`construct/revolve/on_axis.rs:481`) is the existing precedent, and
[`AnalyticLoop::Vertex`] is the contract slot for it.

**Blocked upstream, and it is a SILENT loss — discovered at C2.** truck's
reader has no `vertex_loop` table at all. `FaceBound.bound` is typed `EdgeLoop`,
with the upstream comment *"For now, we are going with the policy of accepting
nothing but edgeloop"* (`truck-stepio/src/in/mod.rs:2626-2630`), and a bound
that fails to resolve is `filter_map`'d away
(`truck-stepio/src/in/convert.rs:60,106`). So a face whose ring is a
`VERTEX_LOOP` comes back **missing that ring**, with nothing in the compressed
shell to record that a boundary was dropped.

For the mesh tier that is cosmetic. For an exact ingest it is a **silent wrong
answer** — we would assemble a solid that disagrees with the file about its own
boundary, and `validate_solid` would happily accept it, because a solid missing
a ring is still a valid solid. So `parse_step_analytic` refuses any file whose
text contains `VERTEX_LOOP`, file-wide, and the mesh tier serves it. That is the
P9/P10 trade taken deliberately: give up the 5.1 % of the corpus that has a
degenerate loop rather than ship a class of silently-wrong solids.

Ways out, for whichever checkpoint first needs that 5.1 %: patch truck, take a
second pass over `Table` to read the `VERTEX_LOOP` entity directly (the
`FACE_BOUND` → `VERTEX_LOOP` → `VERTEX_POINT` chain is in the exchange file we
already hold), or own the reader (§10). Not something to improvise inside the
extractor.

### 5.4 Inner loops and orientation

Rings become `LoopKind::Inner` and must satisfy the **exact** signed-area rule:
`geom::planar_loop_signed_area(n, &pts, &curves) > 0` for the outer loop, `< 0`
for every ring (`validate/faces.rs:34-118`, closed form over chords plus each
arc's segment term). STEP's own loop orientation plus `CompressedFace::orientation`
plus `CompressedEdgeIndex::orientation` determine this; SI5 must compute the
signed area and reject a disagreement rather than flip to taste.

**Which loop is the outer one is not in the data — SECOND SILENT LOSS, found
at C3.** STEP marks the outer boundary with a *subtype* (`FACE_OUTER_BOUND`
rather than `FACE_BOUND`), never with a position in the list, and truck parses
both entities into the **same** `face_bound` table (`truck-stepio/src/in/mod.rs:
256-263`; the struct's own doc says "`FACE_OUTER_BOUNDS` is also parsed to this
struct"). So the marker is gone before extraction, and C2's contract field
`AnalyticFace::loops` originally promised "outer loop first" — a promise
nothing in the pipeline could keep. Measured: **7 of the 28 polyhedral models
in a 400-model ABC sample have at least one face whose first loop is a ring**,
and C3's winding check caught every one as a refusal (25 % of the tier, which
is how it was found rather than shipped).

The fix is a determination, not a convention, and it belongs to the
**consumer**: about the face's outward normal exactly one loop has positive
exact signed area (ISO 10303-42 winds a bound with the material on its left),
and that one is the outer boundary. Zero means the face's declared sense
contradicts its own boundary; two or more means the loops do not describe a
single region — both refusals. It is the consumer's job because the test needs
the surface's own law: a curved patch needs its parametric domain, not a 3-D
area, which is knowledge the kernel has and a neutral data contract does not.
`AnalyticFace::loops` now documents the loss and carries no `outer_loop()`
accessor. This is the same shape of defect as the dropped `VERTEX_LOOP`
(§5.3) — invisible to a mesh tier whose triangulator re-derives hole nesting
anyway, a silently-wrong solid for an exact one — and it is a second argument
for §10's "own the reader" axis.

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

#### The premise above is false in one dimension nobody had measured (C4a)

"No reconciliation is needed at all" rested on §2.2, which measured the residual
of a vertex against its incident **surfaces**. A vertex also has to lie on its
incident **curves**, and that is independent data in the exchange file. Measured
at C4a over 4 932 closed rim anchors in 400 corpus models
(`si5_seam_shape.py` question 6, |anchor − its own `CIRCLE`|):

| | |
|---|---|
| p50 | 8.7e-19 |
| p90 | 9.9e-17 |
| p99 | **4.0e-11** |
| max | **5.0e-11** |
| within `CURVED_SURFACE_DEBUG_TOLERANCE` (1e-12) | 94.3 % |
| within `import_band` (1e-9) | **100 %** |

The cause is visible in the text. ABC `00000007_…_step_000` writes
`CIRCLE('', #107, 0.0910485145000000)` — ten significant digits, zero-padded —
and the anchor vertex `0.0910485144535982`, fifteen. The radius record was
already coarse before it was formatted. Neither number is "the wrong one"; they
are two independent roundings of one quantity, and the file disagrees with
itself by 4.6e-11.

**What this is not.** It is not a defect population to refuse: 100 % of it is
inside the band this tier documents, measures itself against, and already uses
for arcs. It is not noise to absorb either: the arena's law is that a
`Curve::Circle` half-edge's origin lies *on* its circle, exactly.

**What it is.** `CURVED_SURFACE_DEBUG_TOLERANCE` is documented at
`validate.rs:220-228` as a **construction-bug tripwire** — "curved geometry is
exact by construction (the assembler places rim anchors at `center + r·û`), so
this is … not a production gate". C4a is the first producer of a full-circle
edge that was **not constructed**, so the tripwire's own stated premise does not
hold for its input. The validator has been distinguishing the two tiers by a
proxy — `Curve::Arc` endpoints get `import_band`, `Curve::Circle` anchors get the
construction band, because until now only imports and boolean outputs made arcs —
and C4a is where that proxy runs out.

Measured consequence, the whole class and nothing else: of 400 models,
**11 in-vocabulary models are refused by this tripwire**, one site each, at
1.0e-11 – 4.6e-11 against a 1e-12 band — `planar-circle-anchor` ×9,
`cyl-canonical-vertex` ×1, `cone-vertex` ×1. That is the entire gap between
C4a's 85.3 % in-vocabulary success and 100 %. It fires only under
`strict-validation` / `debug_assertions`, so the test tier and a release app
would disagree about the same file, which is its own reason not to leave it to a
`cfg`.

**The fix is provenance, not a band**, and it is its own increment because it
changes a kernel-v2 core type. **DONE 2026-10-01** —
`specs/si5_geometry_provenance_tier.md`, which also records the one thing this
section got wrong: the production on-curve gate implied below turned out to be
unreachable in C4a's vocabulary (the rim-radius agreement and the seam-anchor
reconciliation already bracket it, measured as a sweep), so it was dropped and
deferred to C4b. The design as written: a solid must record whether its geometry was
constructed or asserted, and the curved tripwires must band at the tier that
produced them. Every production gate is unaffected — orientation, Newell,
Euler–Poincaré, twin-curve consistency, the self-intersection gate and SI5's own
import-tier on-surface gate all run on an ingested solid exactly as before.
Reconciling the anchor onto its circle instead (`c + r·ĝ`, which the seam
alignment already computes) would fix the `planar-circle-anchor` site, but it
cannot fix `cone-vertex`: there the rim radius, the half-angle and the vertex are
*three* independent roundings, and no choice of anchor makes all three agree to
1e-12. Reconciliation is the wrong lever for a tier mismatch.

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

### 5.7 Extraction ORDER is not canonical — a C6 prerequisite

Measured at C3: `parse_step_analytic` run **twice on the same text** can return
the same shells in a **different order**. truck's `Table` is built on
`HashMap`s, so a multi-solid file's shell list follows a per-process-seeded
iteration order (ABC `00000005`, 10 identical boxes: two parses differ, and
differ *only* up to shell order — within a shell the vertex, edge and face
tables came out identical on every sample).

Consequences, in increasing order of importance:

- The categorized census's refusal *labels* can move between runs (which face
  walls first), while its counts do not.
- An ingested arena's ids — and therefore its face `Pid`s — are a function of
  that order, so two imports of one file can hand the same geometry different
  persistent ids. The mesh tier has exactly this exposure today, so this is
  not a C3 regression, but the exact tier is where a user's sketch-on-face or
  selection reference would be expected to survive a rebuild.
- `kernel_v2::ingest_analytic` is a deterministic function of the
  `AnalyticShellData` it is given, which is what makes it testable; the
  nondeterminism is entirely upstream of it.

Not fixed at C3 deliberately: the fix is a canonical ordering, and *what* the
canonical key is belongs with the C6 question "what identity does an imported
face have across rebuilds?" — the file's own entity ids would be the natural
answer and truck discards those too. A sort on a geometric key (point-multiset
lexicographic) is the cheap version and should land with C6.

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

- **C1 — this spec + the measurement tooling. DONE 2026-10-01.**
  `scripts/{fetch-abc-corpus.sh,si5_census.py,si5_exactness.py,si5_census_report.py,si5_face_topology.py}`
  (`si5_seam_shape.py` joined them at C4, and refuted `si5_face_topology.py`'s
  reading — §5.1),
  §2 recorded, `docs/step_import_roadmap.md` §6 ledger superseded. No Rust.
- **C2 — the analytic extraction contract, gated off. DONE 2026-10-01.**
  `waffle_types::kernel::analytic` — `AnalyticSurface` / `AnalyticCurve` /
  `AnalyticEdge` / `OrientedEdge` / `AnalyticLoop` / `AnalyticFace` /
  `AnalyticShellData`, full parameters, orientation preserved as orientation.
  `step_import::parse_step_analytic` → `AnalyticImport` (one verdict per shell)
  with a typed `Ineligible` naming the entity and index that failed. Pure data:
  no arena, no kernel dependency, nothing reachable from the app.
  Measured in §2.5. Three things the design gained from contact with the data:
  - **`AnalyticCurve` carries an `interior` point.** Two endpoints on a circle
    define two arcs and the half-turn case is genuinely ambiguous, so endpoints
    alone would let a consumer silently build the complementary arc. The point
    comes from the middle of the file's own parameter range.
  - **`same_sense` is measured, not replayed.** truck may `invert()` a surface
    during parsing, the `Processor` carries its own orientation flag, and
    `CompressedFace` carries a second one that an `ORIENTED_CLOSED_SHELL` flips
    without touching the surface. Evaluating the normal and comparing it to the
    natural outward direction is independent of all three.
  - **Extraction is exact, not sampled.** Every parameter is read through
    truck's public accessors (`RevolutedCurve::origin/axis`, `Line`'s endpoints,
    `Torus::large_radius`, the placement matrix's columns) — no trigonometry and
    no fitting. A non-uniform scale (which would mean an ellipsoid) is a typed
    refusal, not a silent average.
  New fixtures: `crates/step-import/tests/fixtures/analytic/{cylinder,cone,sphere,torus,drilled_block}.step`,
  generated by **our own** `kernel_v2::step_export` (the truck-written ones
  cannot carry a `CYLINDRICAL_SURFACE` at all, §4.3) and golden-pinned in
  `crates/test-harness/tests/si5_analytic.rs`.
- **C3 — arena assembly, planar-only. DONE 2026-10-01.**
  `kernel_v2::ingest_analytic` (`crates/kernel-v2/src/ingest.rs`) assembles an
  `AnalyticShellData` whose every face is a `PLANE` and every edge a `LINE`
  into a real arena solid, in `from_yang_brep`'s three-pass shape: pass 1
  validates everything before the first mutation, pass 2 assembles, pass 3 is
  the production gate (`finalize_solid` → `validate_solid`, the
  self-intersection gate, and outward orientation). Twins are wired by the
  **file's own edge index** rather than a `(v_min, v_max, curve)` key: the
  exchange file shares one `EDGE_CURVE` between the two faces that meet along
  it, so identity is given rather than derived — which also means a writer
  that emitted two coincident edge records is refused as an unpaired edge
  instead of welded silently. Five typed refusal classes
  (`AnalyticIngestUnsupportedSurface` / `…Curve` / `AnalyticIngestUnsupported`
  / `InvalidAnalyticShell` / `AnalyticVertexOffSurface`), `KV2_INGEST_PROBE`
  for the site dump. Gated off: nothing in the app or the adapter calls it.
  Measured by `ingestion_over_the_corpus` (named `c3_planar_ingestion_over_the_corpus`
  until C4a generalized it), which reports two
  different things on purpose — raw reach, and success *within* the
  vocabulary, where a miss is a finding rather than a missing capability:

  | sample | ingested | solids / faces | in vocabulary | of those ingested | text census says polyhedral |
  |---|---|---|---|---|---|
  | 400 models | 28 (7.0 %) | 208 / 4 669 | 28 | **28 (100 %)** | 29 |
  | 1 000 models | 84 (8.4 %) | 416 / 6 936 | 85 | **84 (98.8 %)** | 87 |

  The gap between the census's polyhedral count and `in vocabulary` is C2's
  file-wide `VERTEX_LOOP` refusal (1 model, then 2) — the two measurements are
  independent and agree to that.

  The single in-vocabulary miss at the 1 000-model scale is the on-surface gate
  doing its job: ABC `00000648` declares a face whose boundary vertex is
  **5.2e-9 m** off that face's own plane against a 1.5e-9 band, on 0.5 m-scale
  geometry — three orders of magnitude worse than the ~1e-13 the rest of the
  corpus shows, so it is a defect in the file, not noise. One refusal in 85 is
  consistent with §2.2's 98.9 % of models being inside `import_band`. It is a
  roadmap row with a number attached (§5.5), not a band to widen.

  C3 also found the §5.4 `FACE_OUTER_BOUND` loss and the §5.7 order
  nondeterminism.
- **C4 — cylinders and cones.** §5.1's re-measurement splits this in two, along
  the line the corpus itself draws: a face is a **full band** (every loop one
  closed circle) or an **arc patch** (no closed circle anywhere), and the two
  need disjoint machinery. The split is not a convenience — the first half is
  the one that gains boolean capability, and it is self-contained.
  - **C4a — the full-band tier. DONE 2026-10-01.** Closed-circle edges only:
    `Curve::Circle` mapping, the derived rim traversal sense, seam minting with
    per-component re-anchoring, and the circle-bounded planar cap/ring loop.
    Every ingested cylinder presents exactly two full-circle rims, so these
    solids are `to_yang_brep`-eligible — **the first increment where an imported
    model gains full boolean capability** (pinned by
    `an_ingested_cylinder_is_boolean_eligible`). An open (arc) circle edge is a
    typed refusal naming C4b. It also accepts the arena's own canonical seamed
    lateral, which no corpus writer emits but `kernel_v2::step_export` does —
    that is what makes the export → extract → ingest fixed point (§8.7) an
    acceptance oracle for curved solids as well as planar ones. New fixture
    `frustum` (a `CONICAL_SURFACE` band, the form 413 corpus conical faces take);
    `cylinder` and `drilled_block` moved from the refusal list to the acceptance
    list in this commit, and the apex `cone` stayed, named.

    | sample | ingested | solids / faces | in vocabulary | of those ingested |
    |---|---|---|---|---|
    | 400 models, C3 | 28 (7.0 %) | 208 / 4 669 | 28 | 28 (100 %) |
    | 400 models, **C4a** | **64 (16.0 %)** | **256 / 5 533** | 75 | **64 (85.3 %)** |

    The 11 in-vocabulary misses are **one class with one cause**, measured and
    named in §5.5: a construction-bug tripwire banded at 1e-12 applied to
    geometry that was not constructed, where the file's own records disagree by
    1.0e-11 – 4.6e-11. Fixing it is the next increment because it changes a
    kernel-v2 core type (a solid's provenance); widening the band instead is
    refused.
  - **C4a′ — the geometry-provenance tier. DONE 2026-10-01**, spec
    `specs/si5_geometry_provenance_tier.md` (its own increment, as §5.5 said:
    it changes a kernel-v2 core type). `Solid` records a
    `GeometryProvenance` — `Constructed` (the kernel placed these coordinates)
    or `Asserted` (an exchange file did) — the join is taken through booleans
    and transforms, and every debug-tier on-surface band FLOORS at the solid's
    tier instead of inferring the tier from the curve form. Reach 64 → **75 of
    400 (18.8 %)**, in-vocabulary success 85.3 % → **100 %**, and the whole
    `VertexOffSurface` refusal class is gone. Its own finding: the production
    on-curve gate this spec's §5.5 implied was needed is **unreachable inside
    C4a's vocabulary** — a measured sweep shows the rim-radius agreement and
    the seam-anchor reconciliation own every residual that matters — so it was
    removed rather than shipped as fake coverage, and belongs to C4b, where an
    arc endpoint can sit on its plane yet off its own arc.
  - **C4b — the arc-patch tier. DONE 2026-10-01**, spec
    `specs/si5_c4b_arc_patch_tier.md` (ellipse edges included, per the user's
    scoping call). Reach **75 → 148 of 400 (18.8 % → 37.0 %)**, 518 solids /
    10 233 faces. Measured first (C4b-M), which redirected the work twice: the
    cone-section-ellipse wall costs ZERO reach (no corpus model puts an ellipse
    on a cone), and the unrolled-domain outer-loop ranking was NOT built because
    1 111 of 1 113 arc patches have exactly one boundary loop — the windowed
    patch is a named refusal instead. The increment's own discovery was a
    `signed_volume` gap no boolean output could ever reach: a face mixing a full
    circle with arc chains fitted neither the exact-rational path nor the f64
    arc path, and the fix was recognizing a closed circle as the Δθ = 2π case of
    the arc formula already there (29 models, reach 114 → 148). Original text
    for the record:
  - **C4b (as designed).** `Curve::Arc` from the file's axis plus
    `interior` (never re-derived — §5.1), the outer-loop determination in the
    **unrolled** `(θ, h)` domain rather than by 3-D signed area, and planar faces
    with arc edges. Adds 25.8 points of model reach at the tessellate/measure/
    export tier; these faces bounce off `to_yang_brep` by design. Whether
    `ELLIPSE` edges (oblique cuts: 406 of 3 686 arc patches) come in here or
    wait is a C4b scoping call, not a C4a one.
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
   **Live in production since C3** on the ingest path
   (`ingest::on_surface_band`, `TAU_EVAL·(1 + ‖p‖∞)`, typed
   `AnalyticVertexOffSurface` naming the face, residual and band dumped by
   `KV2_INGEST_PROBE`). Already partly live at C2 — the extraction's test oracle
   (`step-import/src/analytic.rs`, `assert_vertices_on_surfaces`) already
   re-derives each surface from the parameters as extracted and checks every
   boundary vertex against it at 1e-9 relative, which is what makes the
   parameter extraction trustworthy rather than merely compiled. C3 promotes it
   from a test helper to a production gate on the ingest path.

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

- **Seam minting is the whole game** — and the C4 re-measurement (§5.1) raised
  the stake rather than lowering it: the form that "already matched" the
  canonical lateral turned out to be the partial patch, so EVERY full band needs
  a minted seam and 17.6 % of them need a re-anchored rim as well. If choosing
  seam anchors interacts badly with yang's rim-ring caching, C4a stalls and only
  the planar tier lands. That is still worth shipping: it is the polyhedral share
  of the corpus, exactly, with full booleans.
- **A loop-signature census is not a topology census.** The §5.1 error — reading
  `CCLL` as a seamed band because it has the right edge counts — cost nothing
  only because the deeper probe ran before the code did. Any further "the corpus
  already gives us X" claim in this spec must be measured at the level of
  identities (which record, which vertex), not of counts.
- **Canonical-form mismatch is a capability tail, not a bug.** Expect a
  population of ingested solids that validate and render but refuse booleans with
  `UnsupportedCurvedBoolean`. Each is a typed, measured roadmap row.
- **Truck's panic class reaches the browser** (§5.6). Not SI5's to fix, but SI5
  increases our exposure to truck's output surface, so the pre-flight screen
  should land near C6.
- **`ImportedSurface` must not be the gate.** It is a 6-way classification with
  no parameters and it already mislabels our own cylinder fixture as `Freeform`.
  The gate belongs on truck's `Surface`/`Curve3D` variants, upstream.
  **Closed at C2** — `analytic.rs` gates on the parsed variant, and the module
  header says why.
- **Bundle size: measured, not a risk.** C2 left the WASM bundle 3 KB *smaller*
  and gzip-identical at 4.14 MB. The extraction path is dead-code-eliminated
  while nothing in the bridge calls it, so the cost arrives with C6, not before.
  Re-measure there.
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
- 2026-10-01 — **C2 DONE.** Contract in `waffle-types`, extractor in
  `step-import`, five first-party analytic fixtures, 11 new tests (2 of them
  `refs-fixture`-gated, 1 corpus-gated). Measured (§2.5): **49.3 %** of 400 ABC
  models fully extract, against the text census's 54.0 % upper bound, at
  **97.0 % agreement with every disagreement in the stricter direction** — the
  coherence check the plan asked for. Real OCC geometry extracts completely:
  `R_0603` fully, `USB_C` 34/34 shells, 515 faces, every boundary vertex
  verified on its extracted surface within 1e-9 relative.
  The gap to the census is itemized in §2.5, and finding it surfaced a
  **silent-wrong class**: truck's reader discards a `FACE_BOUND -> VERTEX_LOOP`
  without reporting it, so a face can arrive missing a ring and
  `validate_solid` would accept the resulting solid happily. Such files are now
  refused file-wide (§5.3), which cost 2.3 points of reach — 50.5 % → 49.3 % —
  and is the right trade. The first reading of that symptom was a
  misdiagnosis ("a seamless sphere needs a minted seam"), corrected in §5.1.
  Next: **C3**, planar-only arena assembly.
- 2026-10-01 — **C3 DONE.** `kernel_v2::ingest_analytic`, 12 unit tests, two
  new first-party planar fixtures (`block`, `slotted_block` — the second is
  genus 1 with ring loops, so the back-solve and the exact ring winding are
  both covered), 4 new cross-crate tests and a corpus census. The acceptance
  oracle is differential and free: the Euler-operator-built solid that WROTE a
  fixture and the solid ingested back from it agree on `(V, E, F, R, S, G)`
  exactly and on volume bitwise, and ingestion is a fixed point across
  export → extract → ingest. Reach measured in §7's C3 entry: 28/28 of the
  in-vocabulary models at 400, 84/85 at 1 000, the one miss being a file whose
  vertex is 5.2e-9 m off its own plane.
  Three findings, two of them silent-wrong classes in the layer BELOW the
  assembler:
  the lost `FACE_OUTER_BOUND` marker (§5.4 — 7 of 28 polyhedral models have a
  ring-first face; the contract's "outer loop first" promise was removed and
  the determination moved to the consumer, where the surface law lives), and
  non-canonical extraction order (§5.7, a C6 prerequisite). Neither was
  reachable by reading the code: the first surfaced as a 25 % refusal rate in
  the corpus census, the second as a census label that moved between runs.
  The third is a blind spot CLOSED rather than found in the wild: the
  loop-sign determination cannot see an inverted sense on a holed face (both
  signs flip, so ring and perimeter swap consistently, and `validate_solid`
  would accept the swap). Containment settles it exactly — a face's net signed
  area over all its loops is positive — and it costs no corpus reach.
  Next: **C4**, cylinders and cones with seam minting.
- 2026-10-01 — **C4a′ (the provenance tier) DONE.** Spec
  `specs/si5_geometry_provenance_tier.md`. A `Solid` now records WHO placed its
  coordinates, the join is taken through booleans / transforms / splits, and the
  five face validators floor their debug-tier bands at that tier rather than
  inferring it from the curve form. Reach **64 → 75 of 400 (16.0 % → 18.8 %)**,
  273 solids / 5 708 faces, in-vocabulary success **85.3 % → 100 %**; the
  bookkeeping closes exactly (12 models tripped the mis-tiered tripwire, the 11
  in-vocabulary ones all ingest, the twelfth moves on to its real C4b wall) and
  `ingestion_over_the_corpus` now ASSERTS 100 %.
  The increment's own finding is a refutation of its first design: the
  production on-curve gate it began with is **unreachable inside C4a's
  vocabulary**. A sweep over both shapes of the real defect (anchor pushed off
  its circle, radius record coarsened) shows the rim-radius agreement owns the
  record from 1e-11 and the seam-anchor reconciliation owns both from 5e-9, so
  the new gate could never fire. It was removed with its error variant rather
  than shipped — a gate that cannot fire reads like coverage without being any
  — and the bracket that does hold the claim is now pinned as a test. It lands
  for real at C4b, where an arc endpoint can lie on its plane and off its arc
  with nothing else to catch it.
  Next: **C4b**, the arc-patch tier (+25.8 points of model reach).
- 2026-10-01 — **C4b DONE.** `specs/si5_c4b_arc_patch_tier.md`. Open `CIRCLE`
  and `ELLIPSE` edges become `Curve::Arc` / `Curve::EllipseArc` with their side
  READ from the file's own `interior` point; 1c admits mixed arc/chord chains
  (including the 2-edge loop a half-disc needs); a single-loop curved patch
  takes its only loop as the outer boundary. Reach **18.8 % → 37.0 %** of 400
  models, 518 solids / 10 233 faces.
  Three things worth carrying forward. (1) **Measuring first changed the plan
  twice** — ellipses turned out free (no corpus cone carries one) and the
  unrolled-domain ranking turned out to have 2 customers in 1 113, so it was
  replaced by a named refusal; both decisions are the C4a′ lesson applied
  forward. (2) **The real work was not where the design expected it**: 29 of the
  39 models still refused after the ingestion work stopped at a `signed_volume`
  limitation — a face mixing a full circle with arc chains fits neither of its
  two paths — which no boolean output had ever been able to produce. One term
  fixed it, because a closed circle is the Δθ = 2π case of the arc formula that
  was already there. (3) One finding is left standing and anchored by name:
  `00000062_…_step_003` face 35, a CDT ring rejection at the render tier.
  Next: **C5** (spheres and tori — 27 models in the sample), then C6/C7.
