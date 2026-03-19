# Wave 1: Scaffold

**Executor**: Opus (sequential, architectural)
**Blocks**: Wave 2 (all forks depend on these types)
**Estimated scope**: ~300 lines of new code

## Goal

Create the type spine that all Wave 2 forks compile against. No solving logic
yet — just the data structures and trait definitions.

## Deliverables

### 1.1 New module structure

Create `crates/sketch-solver/src/core/` with:

```
core/
├── mod.rs          # Re-exports
├── params.rs       # ParamLayout
├── constraint.rs   # Constraint trait + enum dispatch
├── newton.rs       # Stub (fn signature only)
├── lm.rs           # Stub
├── rank.rs         # Stub
└── drag.rs         # Stub
```

### 1.2 ParamLayout (`core/params.rs`)

Maps entities → parameter vector indices.

```rust
pub struct ParamLayout {
    /// Map: entity_id → (start_index, count) into the parameter vector
    entries: HashMap<u32, ParamEntry>,
    /// Total parameter count
    pub num_params: usize,
}

pub struct ParamEntry {
    pub start: usize,
    pub count: usize,    // 2 for Point, 1 for Circle radius
    pub kind: ParamKind,
}

pub enum ParamKind {
    PointXY,      // 2 params: x, y
    CircleRadius, // 1 param: radius (center is a separate Point)
}
```

**Building from entities:**
- Pass 1: allocate 2 params per Point
- Pass 2: allocate 1 param per Circle (radius only — center is a Point)
- Arcs: 0 own params (center, start, end are Points)
- Lines: 0 own params (start, end are Points)
- Splines: skip (not solved)
- Gears: skip (expanded before solving)

**Key methods:**
- `from_entities(&[SketchEntity]) → ParamLayout`
- `initial_params(&self, &[SketchEntity]) → Vec<f64>` — fill x₀ from entity positions
- `point_indices(&self, id: u32) → (usize, usize)` — (x_idx, y_idx)
- `radius_index(&self, id: u32) → usize`
- `extract_positions(&self, x: &[f64]) → HashMap<u32, (f64, f64)>`

### 1.3 Constraint trait (`core/constraint.rs`)

```rust
/// A single constraint equation (or group of equations) in the system.
pub trait ConstraintEq {
    /// Number of scalar equations this constraint contributes.
    fn num_equations(&self) -> usize;

    /// Compute residual vector. Should be zero when constraint is satisfied.
    /// `out` slice has length == num_equations().
    fn residuals(&self, params: &[f64], out: &mut [f64]);

    /// Append sparse Jacobian entries: (equation_offset + local_eq, param_idx, value).
    /// `eq_offset` is this constraint's starting row in the global Jacobian.
    fn jacobian(&self, params: &[f64], eq_offset: usize, out: &mut Vec<(usize, usize, f64)>);
}
```

Plus an enum wrapper for dispatch:

```rust
pub enum ConstraintImpl {
    Coincident { px: usize, py: usize, qx: usize, qy: usize },
    Horizontal { y_start: usize, y_end: usize },
    Vertical { x_start: usize, x_end: usize },
    // ... all 21 variants, storing param indices (not entity IDs)
}
```

Each variant stores **pre-resolved param indices** from ParamLayout, not entity
IDs. This means constraint construction does the lookup once, and residual/jacobian
evaluation is pure index math — no HashMap lookups in the hot loop.

### 1.4 Constraint builder

```rust
/// Convert SketchConstraints into ConstraintImpls using the ParamLayout.
pub fn build_constraints(
    constraints: &[SketchConstraint],
    entities: &[SketchEntity],
    layout: &ParamLayout,
) -> Vec<ConstraintImpl>;
```

This replaces `constraint_mapping.rs` — instead of mapping to slvs types, we
map to our own `ConstraintImpl` with resolved param indices.

### 1.5 Cargo.toml changes

Add to `crates/sketch-solver/Cargo.toml`:
```toml
nalgebra = "0.33"

[features]
render = ["svg", "resvg"]

[dev-dependencies]
proptest = "1.6"

[dependencies.svg]
version = "0.17"
optional = true

[dependencies.resvg]
version = "0.44"
optional = true
```

Keep `slvs` for now — remove in Wave 3.

### 1.6 Verification

- `cargo check -p sketch-solver` compiles
- Existing 59 tests still pass (slvs solver still wired up)
- New modules have no logic yet, just type definitions and stubs

## Gemini Workers

None — this is pure architecture work, needs Opus judgment for trait design
decisions that affect everything downstream.
