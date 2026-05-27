# `cherchi-sidecar-rs` + `MeshBoolean` trait — Spike PR-CSR1

## Goal

Stand up a new workspace crate `cherchi-sidecar-rs` that wraps the
Cherchi 2022 `mesh_booleans` C++ binary as a Rust `MeshBoolean` impl.
Concurrently introduce the cross-backend `MeshBoolean` trait + `Mesh`
type in cherchi-rs, plus a `BoolOp` enum (with new `Xor` variant) in
cad-primitives.

This unblocks yang-rs (PR-YR1) and kernel-v2 (PR-K1): both can begin
once a working `MeshBoolean` impl exists, without waiting on the LGPL
`Indirect_Predicates` decision for the native cherchi-rs port.
cherchi-sidecar-rs is **not WASM-compatible** (subprocess +
filesystem); the eventual native cherchi-rs port will be the WASM
story. Both impl the same trait; consumers swap via `dyn MeshBoolean`.

## Architecture

```
cad-primitives    →  BoolOp { Union, Intersect, Subtract, Xor }
cherchi-rs        →  Mesh { verts, tris } + MeshBoolean trait
                     (depends on cad-primitives)
cherchi-sidecar-rs →  SidecarBoolean impl + SidecarError
                     (depends on cad-primitives + cherchi-rs)
yang-rs (future)  →  uses MeshBoolean via Box<dyn ...>
```

## Public API

### `cad-primitives::BoolOp`

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Intersect,
    Subtract,
    Xor,
}
```

Matches workspace naming precedent (`kernel/src/boolean/mod.rs:75-79`
uses `Intersect`/`Subtract`, not `Intersection`/`Subtraction`). The
sidecar maps to CLI strings via a private helper.

### `cherchi-rs::Mesh`

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub verts: Vec<Point3>,
    pub tris: Vec<[u32; 3]>,
}

impl Mesh {
    pub const fn empty() -> Self;
    pub fn new(verts: Vec<Point3>, tris: Vec<[u32; 3]>) -> Self;
}
```

`u32` triangle indices cap meshes at 4B triangles — adequate for any
realistic CAD workload. Separate from cherchi-rs's internal
`FastTrimesh` (which has adjacency for arrangement); `Mesh` is the
public lingua franca for boolean inputs and outputs.

### `cherchi-rs::MeshBoolean`

```rust
pub trait MeshBoolean {
    fn boolean(
        &self,
        a: &Mesh,
        b: &Mesh,
        op: BoolOp,
    ) -> Result<Mesh, Box<dyn std::error::Error + Send + Sync>>;
}
```

`Box<dyn Error + Send + Sync>` allows backend-specific error types
without forcing cherchi-rs to know about subprocess errors,
file-not-found, LGPL-predicate-failures, etc. Callers downcast for
specific handling. The trait is object-safe → `dyn MeshBoolean`
works (for yang-rs's runtime backend dispatch).

### `cherchi-sidecar-rs::SidecarBoolean`

```rust
pub struct SidecarBoolean {
    bin_path: PathBuf,
    timeout: Duration,
}

impl SidecarBoolean {
    /// Resolve binary via CHERCHI2022_BIN env var or
    /// /home/claude/cherchi2022/.../build/mesh_booleans default.
    pub fn from_env() -> Result<Self, SidecarError>;

    pub fn new(bin_path: PathBuf, timeout: Duration) -> Self;
}

impl MeshBoolean for SidecarBoolean { ... }
```

### `cherchi-sidecar-rs::SidecarError`

```rust
#[derive(Debug)]
pub enum SidecarError {
    BinaryNotFound { path: PathBuf },
    SpawnFailed { source: io::Error },
    TimedOut { after: Duration },
    NonZeroExit { status: ExitStatus, stderr: String },
    ObjIo { source: io::Error },
    ObjParse { source: io::Error },
}

impl std::error::Error for SidecarError { ... }
impl std::fmt::Display for SidecarError { ... }
```

### `cherchi-sidecar-rs::obj` (public submodule)

```rust
pub fn write_obj(path: &Path, mesh: &Mesh) -> io::Result<()>;
pub fn read_obj(path: &Path) -> io::Result<Mesh>;
```

Public for debugging, fixture capture, hand-crafted-input feeds. u32
overflow on read → `io::ErrorKind::InvalidData`.

## Algorithm

`SidecarBoolean::boolean(a, b, op)`:

1. Resolve a fresh tempdir under `std::env::temp_dir()` (`uuid` or
   pid-based to avoid concurrent-call races).
2. `write_obj(tempdir/a.obj, a)` and `write_obj(tempdir/b.obj, b)`.
3. Build `Command::new(&self.bin_path)` with args `[op.cli_arg(),
   a.obj_path, b.obj_path, out.obj_path]`.
4. `run_with_timeout(cmd, self.timeout)`:
   - Spawn child with stdout+stderr piped.
   - Poll `child.try_wait()` at 1-sec intervals.
   - On timeout: `child.kill()` + `child.wait()` → return `TimedOut`.
   - On normal exit: collect output.
5. If exit status non-zero → `NonZeroExit { status, stderr }`.
6. `read_obj(tempdir/out.obj)` → `Mesh`.
7. Map any error to `SidecarError`; box it for the trait return.

CLI arg mapping (private):
```rust
fn cli_arg(op: BoolOp) -> &'static str {
    match op {
        BoolOp::Union => "union",
        BoolOp::Intersect => "intersection",
        BoolOp::Subtract => "subtraction",
        BoolOp::Xor => "xor",
    }
}
```

## Invariants

1. **Empty mesh in → behaviour follows upstream binary** — upstream may error on degenerate input; we propagate.
2. **`from_env()` is the only constructor that touches env+disk** — pure construction via `new()` doesn't validate the path (validates on first call).
3. **No `eprintln!` from library code** — all observable output via `Result`. Test sites print their own SKIP messages.
4. **No `unsafe`.**
5. **`SidecarBoolean: Send + Sync`** — no interior mutability; safe to share across threads. Concurrent `boolean()` calls use distinct tempdirs.

## Error Contract

- `SidecarBoolean::from_env()` → `Result<Self, SidecarError>`.
- `SidecarBoolean::boolean()` → `Result<Mesh, Box<dyn Error + Send + Sync>>` (trait signature). The Box wraps `SidecarError` concretely.
- `obj::read_obj` / `write_obj` → `io::Result<...>` (matches std).
- No `panic!` in production paths.

## Deliberate Deviations from Existing Test Helpers

The existing `crates/cherchi-rs/tests/common/sidecar.rs` and `obj.rs`
are the algorithmic templates. PR-CSR1 ports them with these changes
for public-API readiness:

1. **`cherchi_bin()` → `from_env() -> Result`** — no `eprintln!`; structured error.
2. **`TimedRun` enum → `run_with_timeout() -> Result<Output, SidecarError>`** — no public enum exposing the success / timeout / spawn-failed branches.
3. **`TriMesh = (Vec, Vec)` → `Mesh` struct** — named fields; public API surface.
4. **`obj::read_obj` checks usize → u32 overflow** — silent narrowing would mis-truncate >4G-vertex meshes; map to `InvalidData`.

`crates/cherchi-rs/tests/common/` stays as-is — duplicated but
deliberately independent (preserves "external oracle" property of
the cherchi-rs smoke test).

## Test Plan (6 groups, ~16-18 tests)

### Group 1 — cad-primitives BoolOp (3 tests)
- All 4 variants distinct (`assert_ne!` matrix)
- `Debug` formatting
- `Copy + Clone`

### Group 2 — cherchi-rs Mesh (3 tests)
- `Mesh::empty()` returns zero-sized
- `Mesh::new(verts, tris)` stores both
- `Clone + Debug + PartialEq`

### Group 3 — cherchi-rs MeshBoolean trait (1 test)
- Object-safe: `let _: Box<dyn MeshBoolean> = Box::new(NoopBoolean);` compiles

### Group 4 — cherchi-sidecar-rs SidecarError (2 tests)
- `BinaryNotFound { path }` carries path
- `Display + std::error::Error` impls

### Group 5 — cherchi-sidecar-rs OBJ I/O (3 tests)
- Round-trip: `write_obj(p, &m)` → `read_obj(p)` == `m`
- Non-triangle face line → `io::ErrorKind::InvalidData`
- u32 overflow guard

### Group 6 — Integration smoke (~5 tests, `tests/smoke.rs`)
- intersection / union / subtraction / xor on overlapping unit cubes → non-empty result
- `CHERCHI2022_BIN=/nonexistent` → `from_env()` returns `Err` (test self-skips)

All op-tests use `let Ok(sb) = SidecarBoolean::from_env() else { return; };` for binary self-skip.

## CLAUDE.md amendments

### `crates/cad-primitives/CLAUDE.md` line 9

Before:
```
- Boolean operation enum: `BoolOp { Union, Intersect, Subtract }`
```

After:
```
- Boolean operation enum: `BoolOp { Union, Intersect, Subtract, Xor }` — `Xor` added in PR-CSR1 to match upstream Cherchi 2022 CLI vocabulary.
```

### `crates/cherchi-sidecar-rs/CLAUDE.md` (new)

Hard rules:
1. Workspace deps: `cad-primitives`, `cherchi-rs`. No others.
2. External crates: `std` only for v1.
3. **NOT WASM-compatible.** Documented at crate-doc level.
4. No `unsafe`.
5. All errors via `Result<>`. No `panic!` in production paths; no `eprintln!` side effects from library functions.
6. `SidecarError` is concrete; trait method boxes it as `Box<dyn Error + Send + Sync>`.

## References

- Cherchi et al. 2022 — `mesh_booleans` upstream binary.
- `docs/sidecar/cherchi2022_build_guide.md` — binary path + CLI.
- `crates/cherchi-rs/tests/common/sidecar.rs` — algorithmic template.
- `crates/cherchi-rs/tests/common/obj.rs` — OBJ I/O template.
- `crates/kernel/src/boolean/mod.rs:75-79` — `BoolOp` naming precedent.
- `crates/cad-primitives/CLAUDE.md:9` — canonical `BoolOp` location.
- `crates/cad-primitives/CLAUDE.md:14` — Mesh types belong in cherchi-rs.

## Banked for Future Work

- `thiserror` dep when `SidecarError` grows beyond ~6 variants
- Faster I/O protocol (stdin/stdout binary) replacing OBJ round-trip
- Native cherchi-rs `MeshBoolean` impl (gated on Stage 2-4 + LGPL decision)
- yang-rs PR-YR1 (first consumer)
