# cad-primitives — Scope Rules

This crate is the foundation of the clean-sheet kernel rewrite (see root `CLAUDE.md` §"Kernel Rewrite In Progress"). It holds **types and constants only**.

## What belongs here

- Geometric primitive types: `Point3`, `Vector3` (and any future `Quaternion`, `Mat3`)
- Distance / angle tolerance constants — the existing `TAU_MODEL = 1e-7`, `MIN_FEATURE_SIZE = 1e-6`, `TAU_NORMALIZE`, etc.
- Boolean operation enum: `BoolOp { Union, Intersect, Subtract, Xor }` — `Xor` added in PR-CSR1 to match upstream Cherchi 2022 CLI vocabulary.
- The shared `KernelError` type (cross-crate error category enum)

## What does NOT belong here

- Mesh types — those live in `cherchi-rs` (its own indexed mesh) or `yang-rs` (its own intermediate pipeline mesh)
- B-Rep types — those live in `kernel-v2` (its own half-edge arena)
- Any algorithm — predicates, intersections, projections, tessellation, ANYTHING with computation
- Anything `pub fn` that does work beyond constructors / accessors / trivial arithmetic

If a function has more than ~10 lines of math, it belongs in its consumer crate, not here.

## Hard rules

1. ZERO workspace dependencies. This crate must be at the bottom of the dependency DAG.
2. No `unsafe`.
3. All types are `Copy` / `Clone` / `Debug` / `PartialEq`. No interior mutability.
4. Adding a function here requires explicit justification (it must be needed by 2+ consumer crates AND have no business in any specific consumer's domain).

## When working on this crate

You may only read files inside `crates/cad-primitives/` and this `CLAUDE.md`. Do not read other crates' code while extending this one. If a consumer crate needs a type, the type's design is determined by the consumer's needs — not by inspecting how the old `crates/kernel/` did it.
