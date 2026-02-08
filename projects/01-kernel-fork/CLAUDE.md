# 01 — Kernel Fork: Agent Instructions

You are working on **kernel-fork**. Read ARCHITECTURE.md in this directory first.

## Your Job

Wrap truck's API behind the `Kernel` and `KernelIntrospect` traits defined in the top-level `INTERFACES.md`. No truck types may be exposed to other crates — everything goes through the trait interface.

## Critical Rules

1. **NEVER expose truck types directly.** Other crates import `Kernel`, `KernelIntrospect`, `KernelSolidHandle`, `KernelId` — never `truck_topology::Solid` or `truck_modeling::builder`.
2. **MockKernel is as important as TruckKernel.** Other teams (feature-engine, modeling-ops) depend on MockKernel for their tests. Build and test it thoroughly.
3. **Boolean performance is a known crisis.** If you encounter performance data, document it. Don't spend excessive time optimizing — document findings for future work.
4. **Tessellation must produce face-range metadata.** three.js needs to map picked triangles back to logical faces.

## Build & Test

```bash
cargo test -p kernel-fork
cargo clippy -p kernel-fork
```

## Key Files

- `src/lib.rs` — Public API (re-exports trait impls)
- `src/truck_kernel.rs` — TruckKernel implementation
- `src/truck_introspect.rs` — TruckIntrospect implementation
- `src/mock_kernel.rs` — MockKernel implementation
- `src/primitives.rs` — Higher-level primitive builders
- `src/tessellation.rs` — Tessellation wrapper with face-range tracking
- `src/types.rs` — KernelSolidHandle, KernelId, KernelError

## Dependencies

- truck-topology, truck-geometry, truck-modeling, truck-shapeops, truck-meshalgo
- No dependencies on other Waffle Iron crates (this is a leaf dependency)
