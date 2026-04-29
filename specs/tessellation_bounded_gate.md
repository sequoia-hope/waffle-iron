# Tessellation Bounded-Path Gate

Investigation note characterizing the gate that decides whether a solid takes
the bijective "bounded" tessellation path (shared edge-discretization vertices,
watertight by construction) versus the legacy fan path (per-face vertices,
post-hoc position welding via `weld_mesh_vertices`). PR2 will widen this gate;
PR3 will then drop `weld_mesh_vertices` (audit D-10).

## 1. The gate (verbatim)

`crates/kernel/src/tessellation/mod.rs:217-235`:

```
    if cylinder_params.is_none()
        && revolve_params.is_none()
        && sphere_params.is_none()
        && cone_params.is_none()
        && torus_params.is_none()
        && !is_polygon_soup
    {
        let has_arcs = _edge_geometry
            .values()
            .any(|e| matches!(e, CurveGeom::Arc(_)));
        if !has_arcs {
            return tessellate_solid_bounded(arena, face_map, face_geometry, _edge_geometry);
        }
        // Arc-edge boolean results: fall through to fan path below ...
        needs_fan_welding = true;
    }
```

`has_arcs` is computed at lines 224–226. `is_polygon_soup` is a parameter
(`mod.rs:198`) sourced from `WaffleSolid::is_polygon_soup`
(`waffle_kernel.rs:46`), set `true` at every polygon-clipping branch in
`waffle_kernel.rs:1122-1318` and forced `true` by Yang integration's stub
solid (`boolean/yang_integration.rs:3393`).

## 2. What each clause screens out

**Five primitive-params clauses** (`*_params.is_none()`): out of PR2 scope.
Solids built from a single primitive carry parametric description and are
dispatched to dedicated tessellators (`tessellate_sphere_solid`, etc., or
the per-face cylindrical/revolve branches starting at line 244+). Bounded
path is not designed to subsume these.

**`has_arcs` clause (lines 224–226)**: matches `CurveGeom::Arc(_)` only. The
other variants (`Linear`, `Circular`, `Elliptical`, `geometry/curve.rs:13-18`)
do NOT trigger; full circles in particular are not arcs. `discretize_edges`
(`mod.rs:2665-2744`) already handles all four variants conformally
(Arc segment count = `ceil(circle_segments × sweep/τ).max(4)`, full
Circular/Elliptical = `circle_segments` points). The blocker is documented
inline at lines 203–206: parallel cyl-cyl booleans produce arc-trimmed
cylindrical face topology that bounded path's ring-building logic in
`tessellate_cylindrical_face_bounded` (`mod.rs:3008+`, esp. lines 3033–3054)
does not yet handle. Real downstream limitation, not a stale check.

**`is_polygon_soup` clause (line 222)**: set `true` when a boolean result
came from S-H polygon clipping rather than analytical SSI / planar-exact /
Yang. Polygon-soup B-Rep may carry `SurfaceGeom::Cylindrical` tags on faces
whose boundary is piecewise-linear. Comment at lines 207–211: such inputs
"may contain internal faces; bounded tessellation's shared vertices make
these indistinguishable from external faces, preventing removal. The fan
path's per-face vertices allow `remove_isolated_triangles` to identify
and remove internal face fragments." So the clause protects downstream
cleanup, not because bounded path can't tessellate the input.

## 3. Three concrete example inputs

| # | Input | Gate result | Confidence |
|---|---|---|---|
| 1 | **Cube** (`make_box`): 6 planar faces, 12 linear edges, all `*_params=None`, `is_polygon_soup=false` | Bounded path | Verified — `waffle_kernel.rs:367` sets `is_polygon_soup: false`; edges are `CurveGeom::Linear`. |
| 2 | **Analytic cylinder primitive** (`make_cylinder`): top/bottom planar caps + side cylindrical, two `CurveGeom::Circular` edges | First clause fails (`cylinder_params=Some`); bounded path NOT entered. Per-face cylindrical/cap tessellation in fan-path body. | Verified — `make_cylinder` sets `cylinder_params: Some(_)`. |
| 3 | **Boolean output** (`extrude(rect) - extrude(small_rect)`, polygon-clipping path): all-planar, `is_polygon_soup=true` | `is_polygon_soup` clause matches; fan path | Verified — `waffle_kernel.rs:1297,1317-1319` sets `polygon_soup = true` for general polygon approximation. |
| 4 | **Cyl-cyl parallel-axis subtract** (analytical SSI succeeds): arc-trimmed cylindrical lateral faces, `CurveGeom::Arc` edges, `is_polygon_soup=false` | `has_arcs` matches; fan path with `needs_fan_welding=true` | Hypothesized from gate comment + `discretize_edges` Arc branch; not traced end-to-end. |

## 4. PR2 punch-list

### Lifting `has_arcs`

`discretize_edges` already produces conformal Arc samples. The blocker is
`tessellate_cylindrical_face_bounded`: its ring-building logic at
`mod.rs:3056+` assumes full-circle rings, not arc-bounded trimmed-cylinder
loops.

- **(a)** Extend the ring builder to accept arc-bounded loops (open ring
  segments stitched along generator lines). Medium scope, one function.
  Risk: arc endpoints already align across mating faces via shared
  `disc.positions`, but interior generator lines must be added consistently.
- **(b)** Add a second cylindrical-face-bounded tessellator for arc-bounded
  trimmed cylinders, dispatched on `has_arcs`. Smaller blast radius; some
  code duplication.
- **(c)** Leave `has_arcs` blocked; fix only planar-face cases. Still
  removes welding for many F-series cases without arc edges.

### Lifting `is_polygon_soup`

Bounded path needs B-Rep edges; polygon-soup output already has linear-edge
B-Rep, so `discretize_edges` works. Real issue is internal-fragment removal.

- **(a)** Eliminate internal fragments upstream in `polygon_approx_boolean`,
  so bounded path receives a clean B-Rep. Large scope; touches the
  deprecated S-H pipeline that A15.6 marks for removal.
- **(b)** Scope the clause to *render-stage* tessellation; ensure Yang's
  internal tessellation calls (which never produce polygon-soup input) hit
  bounded path unconditionally. Small scope; concentrates gate on render path.
- **(c)** Add a polygon-soup-aware bounded variant using input vertex IDs
  as-is, without welding, treating internal-fragment removal as caller's job.
  Medium scope; introduces a new path PR3's `weld_mesh_vertices` removal must
  also satisfy.

PR2 picks among (a)/(b)/(c) per clause. This note characterizes; does not
recommend.

## 5. References

- Yang 2025 §4.1.1 (`yang2025_hybrid_boolean.txt:518-591`) — bijective
  error-bounded triangulation, surface-to-mesh distance ≤ d_ε.
- Yang 2025 §4.4 (`yang2025_hybrid_boolean.txt:952-1010`) — watertightness
  depends on bijectivity preserved through pipeline.
- `docs/audits/cherchi_port_audit.md` D-10 (line 662) — `weld_mesh_vertices`
  is an A15.6 violation; remediation is "fix upstream tessellation".
- `docs/audits/cherchi_port_audit.md` Cluster I (line 81) — defensive guards
  mask upstream bugs; same anti-pattern as gate-induced welding.
- `governance/ARCHITECTURAL_INVARIANTS.md` §A15.6.
- `specs/yang_global_edge_conformality.md` — related mesh-arrangement-stage
  conformality gap.
