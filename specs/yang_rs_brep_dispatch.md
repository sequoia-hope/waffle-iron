# `yang-rs::boolean` — Spike PR-YR1

## Goal

Stand up `yang-rs` as a working crate. Define the public input/output
type (`BRep`), the error type (`YangError`), and the single-entry
`boolean()` function — even though no Yang 2025 pipeline stages are
implemented yet.

The function delegates entirely to a `&dyn MeshBoolean` backend (from
PR-CSR1). Today the backend is `cherchi_sidecar_rs::SidecarBoolean`;
eventually it'll be the native cherchi-rs implementation. The
dispatch is degenerate but the public surface is the right shape for
future stages to land non-breakingly inside.

Per `crates/yang-rs/CLAUDE.md:44-54`, this implements **step 1**
("Define yang-rs's BRep input/output type") plus the degenerate
**step 4** ("Stage 2 wiring") so the crate has a working end-to-end
boolean from PR-1, gated only on a `MeshBoolean` backend.

After PR-YR1, future PRs add the actual Yang pipeline stages:
- PR-YR2: Stage 1 — bijective tessellation + `TessellationMap`
- PR-YR3: Stages 5/6 — patch segmentation + B-Rep reassembly
- PR-YR4: Stages 3/4 — SSI refinement + mesh updating
- PR-YR5: Stage 0 — coplanar preprocessing (last per CLAUDE.md)

## Public API

```rust
// crates/yang-rs/src/lib.rs

pub use cad_primitives::BoolOp;
pub use cherchi_rs::Mesh;

/// Boundary-Representation solid for yang-rs's pipeline I/O.
///
/// PR-YR1 ships a degenerate `BRep` that wraps a `Mesh`. Adding
/// fields later (faces, edges, tessellation map) is non-breaking
/// because external access goes through `as_mesh` / `into_mesh`.
#[derive(Clone, Debug, PartialEq)]
pub struct BRep { /* private mesh: Mesh */ }

impl BRep {
    pub fn from_mesh(mesh: Mesh) -> Self;
    pub fn as_mesh(&self) -> &Mesh;
    pub fn into_mesh(self) -> Mesh;
    pub fn num_verts(&self) -> usize;
    pub fn num_tris(&self) -> usize;
}

/// Errors from the yang-rs pipeline.
#[derive(Debug)]
pub enum YangError {
    /// Input is not 2-manifold. (Detection deferred; defined here for
    /// future use.)
    NonManifoldInput,
    /// Reassembly would produce a non-2-manifold result. (Stages 5/6
    /// will surface this; defined here for forward compatibility.)
    NonManifoldOutput,
    /// The mesh boolean backend (sidecar or native) failed.
    MeshBooleanFailed(Box<dyn std::error::Error + Send + Sync>),
}

impl std::error::Error for YangError {}
impl std::fmt::Display for YangError {}

/// Boolean operation on two B-Rep solids via a `MeshBoolean` backend.
///
/// PR-YR1: extracts meshes from the inputs, dispatches to the
/// backend, wraps the result in a fresh `BRep`. Future PRs insert
/// Stages 0-6 around this call.
pub fn boolean(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
) -> Result<BRep, YangError>;
```

## Algorithm

PR-YR1 is degenerate dispatch:

```text
1. result_mesh = backend.boolean(a.as_mesh(), b.as_mesh(), op)
2. if Err(e) → return YangError::MeshBooleanFailed(e)
3. else → return BRep::from_mesh(result_mesh)
```

Future PRs interpose the 6 Yang stages between input and the
`backend.boolean(...)` call (and after it, for stages 5+).

## Invariants

1. **Mesh round-trip identity**: `BRep::from_mesh(m).as_mesh() == &m`
   and `BRep::from_mesh(m).into_mesh() == m`.
2. **Counts delegate**: `BRep::from_mesh(m).num_verts() == m.verts.len()`,
   `BRep::from_mesh(m).num_tris() == m.tris.len()`.
3. **Boolean delegation**: for any backend `B`, op `op`, and inputs
   `a, b`:
   `boolean(a, b, op, &B).map(|r| r.into_mesh()) ==
    B.boolean(a.as_mesh(), b.as_mesh(), op).map_err(...)`.
   I.e., the wrapper adds no behavior beyond mesh extraction +
   re-wrapping + error mapping.
4. **No `panic!`**: all errors flow through `Result<_, YangError>`.
5. **Backend errors preserved**: `YangError::MeshBooleanFailed(e)`
   has `source() == Some(&*e)` so callers can downcast.

## Error contract

- `YangError::MeshBooleanFailed` is the only variant returned by
  PR-YR1. The other two (`NonManifoldInput`, `NonManifoldOutput`)
  are defined for forward compatibility; future PRs return them.
- `YangError: std::error::Error` so it composes with `?` upstream.
- `source()` returns the inner backend error for downcasting.

## Deliberate Deviations from Yang 2025

PR-YR1 implements **none of** the 6 Yang stages. The crate-doc and
spec call this out explicitly. Future PRs add stages per the
CLAUDE.md-recommended order:

1. Step 3 → PR-YR2: Stage 1 (bijective tessellation)
2. Step 5 → PR-YR3: Stages 5/6 (reassembly)
3. Step 6 → PR-YR4: Stages 3/4 (SSI refinement)
4. Step 7 → PR-YR5: Stage 0 (coplanar preprocessing)

These deviations are temporary, not permanent. As stages land,
this section shrinks and the Yang 2025 reference becomes the spec.

## Test plan (~10 tests, 4 groups)

### Group 1 — BRep construction + accessors (3 tests)

- `from_mesh / as_mesh` round-trip
- `into_mesh` returns the wrapped mesh
- `num_verts / num_tris` delegate

### Group 2 — YangError contract (2 tests)

- `Display` produces non-empty messages
- `Error::source` returns inner error for `MeshBooleanFailed`

### Group 3 — boolean() dispatch via mock (3 tests)

- Mock backend `Ok(Mesh::empty())` → `boolean()` returns `Ok(BRep)`
- Mock backend `Err(_)` → `boolean()` returns `Err(MeshBooleanFailed(_))`
- All 4 `BoolOp` variants dispatch successfully through the mock

### Group 4 — End-to-end via sidecar (~2 tests)

- `yang_rs::boolean(cube_a, cube_b, Intersect, &SidecarBoolean::from_env()?)`
  returns non-empty BRep
- Same with Union (multi-op smoke)
- Self-skip when binary missing

## Sidecar exercise

**Yes, one integration test** in `tests/end_to_end.rs` exercises the
real `mesh_booleans` binary via the `SidecarBoolean` impl. Other
tests use a mock impl. The integration test self-skips when
`from_env()` returns `Err`.

## Banked for future PRs

- **PR-YR2 (Stage 1)**: `TessellationMap` type; per-face analytical
  surface storage; bijective tessellation for planar faces first.
- **PR-YR3 (Stages 5/6)**: Patch segmentation via flood-fill; B-Rep
  reassembly from labeled mesh. Produces a mesh-approximate B-Rep
  output.
- **PR-YR4 (Stages 3/4)**: SSI refinement (calls `ssi-rs`); mesh
  updating via CDT to restore bijection.
- **PR-YR5 (Stage 0)**: Coplanar preprocessing. Last per CLAUDE.md
  ordering — the legacy yang port tangled here first.
- **Non-manifold input detection**: at `boolean()` entry. Returns
  `YangError::NonManifoldInput` for non-2-manifold inputs.
- **`IntermediateMesh`**: non-manifold-supporting internal mesh
  type for stages 1-4. Defer until Stage 1 needs it.

## References

- Yang et al. 2025 — "A robust hybrid Boolean operations method for
  mesh-and-surface hybrid models." `refs/text/yang2025_hybrid_boolean.txt`
- `crates/yang-rs/CLAUDE.md:44-54` — stage development order
- `crates/cherchi-rs/src/boolean.rs` — `MeshBoolean` trait
- `crates/cherchi-rs/src/mesh.rs` — `Mesh` type
- `memory/cherchi_sidecar_rs_pr_csr1.md` — `SidecarBoolean` (today's backend)
