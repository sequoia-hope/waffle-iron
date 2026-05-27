# Yang-rs Stage 1 — bijective tessellation (planar) — Spike PR-YR2

## Goal

Port Yang 2025 Stage 1 (§4.1) — **bijective tessellation** — for the
simplest case: **planar B-Reps with convex faces**. Every output mesh
vertex maps to a unique B-Rep feature (vertex / edge / face) via
`TessellationMap`. This is the first actual Yang stage in yang-rs.

PR-YR1 shipped a degenerate `BRep::from_mesh` path. PR-YR2 adds the
real `BRep::new(verts, edges, faces)` constructor that:
1. Validates planar topology (face count ≥ 3, indices in range)
2. Fan-triangulates each face from its first vertex
3. Records the bijection: every mesh vertex's source B-Rep feature

`boolean()` remains unchanged in PR-YR2; PR-YR3 (Stages 5/6) will
rewire it to consume `TessellationMap` outputs.

## Architectural prerequisite — `Vector3` in cad-primitives

`Surface::Plane` needs a normal vector. `cad-primitives/CLAUDE.md:7`
already lists `Vector3` as an allowed type but it's not yet
implemented. PR-YR2 adds it as a minimal type (Copy, 24 bytes, no
algorithms beyond accessors), mirroring `Point3`.

## Public API

### cad-primitives — new `Vector3`

```rust
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Vector3 { coords: [f64; 3] }

impl Vector3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self;
    pub fn x(&self) -> f64;
    pub fn y(&self) -> f64;
    pub fn z(&self) -> f64;
    pub fn as_array(&self) -> [f64; 3];
}
impl From<[f64; 3]> for Vector3 { ... }
impl From<Vector3> for [f64; 3] { ... }
```

No `cross` / `dot` / `normalize` — those belong in consumer crates.

### yang-rs — Surface, Curve, B-Rep topology, TessellationMap

```rust
pub enum Surface {
    /// Plane: n·x + d = 0. Normal points OUTWARD from the solid.
    Plane { normal: Vector3, d: f64 },
}

pub enum Curve {
    /// Line segment between the edge's start/end vertices.
    LineSegment,
}

pub struct BRepVertex { pub point: Point3 }

pub struct BRepEdge {
    pub start: u32,
    pub end: u32,
    pub curve: Curve,
}

pub struct BRepFace {
    pub surface: Surface,
    pub outer_loop: Vec<u32>, // CCW edge indices
}

pub enum TessellationSource {
    BRepVertex(u32),
    BRepEdge { edge: u32, t: f64 },
    BRepFace { face: u32, u: f64, v: f64 },
    Unknown, // from_mesh degenerate path
}

pub struct TessellationMap { sources: Vec<TessellationSource> }
```

### Expanded `BRep`

```rust
pub struct BRep {
    vertices: Vec<BRepVertex>,
    edges: Vec<BRepEdge>,
    faces: Vec<BRepFace>,
    mesh: Mesh,
    tessellation: TessellationMap,
}

impl BRep {
    pub fn new(verts, edges, faces) -> Result<Self, YangError>; // eager Stage 1
    pub fn from_mesh(mesh: Mesh) -> Self; // degenerate (Unknown map)
    pub fn vertices() -> &[BRepVertex];
    pub fn edges() -> &[BRepEdge];
    pub fn faces() -> &[BRepFace];
    pub fn as_mesh() -> &Mesh;
    pub fn into_mesh(self) -> Mesh;
    pub fn tessellation_map() -> &TessellationMap;
    pub fn num_verts() -> usize;
    pub fn num_tris() -> usize;
}
```

### YangError

```rust
pub enum YangError {
    NonManifoldInput,
    NonManifoldOutput,
    MeshBooleanFailed(Box<dyn Error + Send + Sync>),
    MalformedTopology(String), // NEW in PR-YR2
}
```

## Algorithm — Stage 1 planar fan-triangulation

1. Validate: edge indices into verts; face edge indices into edges; face outer_loop.len() ≥ 3.
2. Mesh vertices = B-Rep vertices, 1:1 (no Steiner points).
3. TessellationMap sources = `[BRepVertex(0), BRepVertex(1), ...]`.
4. For each face, walk `outer_loop` collecting each edge's `.start`, then fan-triangulate from `face_verts[0]`.

Produces a mesh with exactly `verts.len()` vertices and `sum_over_faces(outer_loop.len() - 2)` triangles.

## Invariants

1. `BRep::new(verts, edges, faces).num_verts() == verts.len()`
2. `tessellation_map.lookup(i) == BRepVertex(i)` for every mesh vertex
3. `from_mesh(m).num_verts() == m.num_verts()`, all map entries `Unknown`
4. Output mesh is a valid `cherchi_rs::Mesh` (deterministic; no NaN, no infinite)

## Error contract

`BRep::new` returns `Err(MalformedTopology(String))` for:
- Face with `outer_loop.len() < 3`
- Out-of-range edge index in any face
- Out-of-range vertex index in any edge

## Deliberate deviations from Yang 2025 §4.1

1. **Convex faces only** (fan-triangulation). Banked: PR-YR2b.
2. **No inner loops** (no holes). Banked: PR-YR2c.
3. **Planar surfaces only** (no Cylinder/Sphere/etc.). Banked: PR-YR2d.
4. **No Steiner points** — output has exactly `verts.len()` vertices. Banked: PR-YR2e.
5. **No CDT at shared edges** — works for planar adjacency only. Banked: PR-YR2f.
6. **Topology validation is minimal** — no cycle continuity check, no 2-manifold check. Banked.

## Test plan (~18 tests, 5 groups)

**Group 1** — cad-primitives `Vector3` (3): construction + accessors, `From<[f64;3]>` round-trip, equality.

**Group 2** — yang-rs type construction (5): Surface::Plane, Curve::LineSegment, BRepVertex/Edge/Face, TessellationSource variants, TessellationMap::empty().

**Group 3** — Degenerate `from_mesh` path (3): mesh round-trip, map length = num_verts, all entries Unknown.

**Group 4** — `BRep::new` Stage 1 happy paths (5): single triangle, quad, tetrahedron, unit cube, BRepVertex(i) lookup invariant.

**Group 5** — Error paths (2): face with <3 edges, out-of-range edge index.

## Banked for future PRs

- **PR-YR2b**: ear-cutting (non-convex faces)
- **PR-YR2c**: inner loops (holes)
- **PR-YR2d**: curved surfaces (Cylinder, Sphere, NURBS) + §4.1.1 adaptive subdivision
- **PR-YR2e**: Steiner points + dε tolerance
- **PR-YR2f**: CDT at shared edges
- **PR-YR3**: rewire `boolean()` to use `tessellate()` outputs (Stage 5/6)
- **Topology validation**: cycle continuity, 2-manifoldness

## References

- Yang et al. 2025 §4.1 — `refs/text/yang2025_hybrid_boolean.txt:286-407`
- `crates/yang-rs/CLAUDE.md:42-54` — stage development order
- `crates/yang-rs/src/lib.rs` (PR-YR1) — existing API
- `crates/cad-primitives/src/lib.rs:29-71` — `Point3` template for `Vector3`
