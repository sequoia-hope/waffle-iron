# Wave 1: Scaffold

**Executor**: Opus (trait design) + Gemini workers (mechanical implementation)
**Blocks**: Wave 2 (all forks depend on these types)
**Estimated scope**: ~400 lines of new code

## Goal

Create the type spine that all Wave 2 forks compile against. Data structures,
trait definitions, typed index wrappers, and stubs. No solving logic yet.

## Design Decisions (locked)

- **nalgebra types internally**: `Point2<f64>`, `Vector2<f64>` for geometry math.
  `(f64, f64)` only at the `solve_sketch()` boundary (SolvedSketch contract).
- **nalgebra in public solver API**: `DMatrix`/`DVector` in `SolveOutcome`. Fine
  for internal crate; refactor at open-source extraction time.
- **Typed index wrappers**: `PointIdx`, `LineIdx`, `RadiusIdx` — zero-cost newtypes
  over `usize` that prevent index mixups and provide nalgebra read helpers.
- **Implicit arc radius**: Arcs have center/start/end as Points, no radius param.
  `Radius(arc)` → `DistancePP(center, start_point) - target = 0`.
- **In-crate feature gate** for render: `render` feature with optional `svg` + `resvg`.
- **SameOrientation = no-op**: existing behavior, confirmed by oracle test.
- **Arc-arc tangency supported**: `TangentArcArc` variant in solver, even though
  waffle-types `Tangent { line, curve }` can't express it yet. Builder maps
  existing variant to `TangentLineCircle`; arc-arc available for future wiring.

## Deliverables

### 1.1 New module structure

Create `crates/sketch-solver/src/core/` with:

```
core/
├── mod.rs          # Re-exports
├── types.rs        # PointIdx, LineIdx, RadiusIdx, ScaleType, SolveOptions, SolveOutcome
├── params.rs       # ParamLayout
├── constraint.rs   # ConstraintEq trait + ConstraintImpl enum
├── builder.rs      # SketchConstraint → ConstraintImpl dispatch
├── lm.rs           # Stub (fn signature only)
├── rank.rs         # Stub
└── status.rs       # Stub
```

### 1.2 Typed index wrappers (`core/types.rs`)

```rust
use nalgebra::{Point2, Vector2};

/// Index of a 2D point's x-coordinate in the parameter vector.
/// y-coordinate is always at self.0 + 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointIdx(pub usize);

impl PointIdx {
    pub fn x(self) -> usize { self.0 }
    pub fn y(self) -> usize { self.0 + 1 }
    pub fn read(self, params: &[f64]) -> Point2<f64> {
        Point2::new(params[self.0], params[self.0 + 1])
    }
}

/// A line segment defined by two point indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineIdx {
    pub start: PointIdx,
    pub end: PointIdx,
}

impl LineIdx {
    pub fn delta(self, params: &[f64]) -> Vector2<f64> {
        self.end.read(params) - self.start.read(params)
    }
    pub fn length(self, params: &[f64]) -> f64 {
        self.delta(params).norm()
    }
    pub fn length_sq(self, params: &[f64]) -> f64 {
        self.delta(params).norm_squared()
    }
}

/// Index of a radius parameter in the parameter vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadiusIdx(pub usize);

impl RadiusIdx {
    pub fn read(self, params: &[f64]) -> f64 {
        params[self.0]
    }
}

/// Whether a constraint equation measures distance (meters) or angle (radians).
/// Used to build D_row for Jacobian scaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleType { Distance, Angle }

pub struct SolveOptions {
    pub max_iterations: usize,  // 50
    pub tolerance: f64,         // 1e-7 (TAU_MODEL, A14)
    pub lambda_init: f64,       // 1e-3 (warm) or 1.0 (cold)
    pub spring_mu: f64,         // 1e-6
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            max_iterations: 50,
            tolerance: 1e-7,
            lambda_init: 1e-3,
            spring_mu: 1e-6,
        }
    }
}

pub struct SolveOutcome {
    pub params: Vec<f64>,
    pub converged: bool,
    pub iterations: usize,
    pub final_residual_norm: f64,
    pub jacobian_scaled: DMatrix<f64>,  // Scaled, un-augmented (for rank diagnostics)
    pub residual_scaled: DVector<f64>,  // Scaled residual (for conflict detection)
}
```

### 1.3 ParamLayout (`core/params.rs`)

Maps entities → parameter vector indices, returning typed wrappers.

```rust
pub struct ParamLayout {
    point_indices: HashMap<u32, PointIdx>,
    radius_indices: HashMap<u32, RadiusIdx>,
    num_params: usize,
}
```

**Building from entities:**
- Pass 1: allocate 2 params per Point → `PointIdx`
- Pass 2: allocate 1 param per Circle (radius only) → `RadiusIdx`
- Arcs: 0 own params (center, start, end are Points; radius is implicit)
- Lines: 0 own params (start, end are Points)
- Splines: skip (not solved)
- Gears: skip (expanded before solving)

**Key methods:**
- `from_entities(entities: &[SketchEntity]) → ParamLayout`
- `initial_params(&self, entities: &[SketchEntity]) → Vec<f64>`
- `point(&self, id: u32) → PointIdx`
- `radius(&self, id: u32) → RadiusIdx`
- `line(&self, line_id: u32, entities: &[SketchEntity]) → LineIdx`
  (looks up the line entity's start/end point IDs, returns their PointIdx pair)
- `num_params(&self) → usize`
- `extract_positions(&self, params: &[f64]) → HashMap<u32, (f64, f64)>`
  (converts Point2 back to tuples at the boundary)

### 1.4 ConstraintEq trait + ConstraintImpl enum (`core/constraint.rs`)

```rust
pub trait ConstraintEq {
    /// Number of scalar equations this constraint contributes.
    fn num_equations(&self) -> usize;

    /// Scale type per equation (Distance or Angle) for D_row construction.
    fn scale_types(&self) -> &[ScaleType];

    /// Compute residuals f(x). Should be zero when satisfied.
    /// `out` slice has length == num_equations().
    fn residuals(&self, params: &[f64], out: &mut [f64]);

    /// Append sparse Jacobian entries: (global_row, param_col, value).
    /// `eq_offset` is this constraint's starting row in the global Jacobian.
    fn jacobian(&self, params: &[f64], eq_offset: usize, out: &mut Vec<(usize, usize, f64)>);
}
```

The enum stores **pre-resolved typed indices** from ParamLayout:

```rust
pub enum ConstraintImpl {
    // ── Group 1: Linear (constant Jacobian) ──
    Coincident { p1: PointIdx, p2: PointIdx },
    Horizontal { line: LineIdx },
    Vertical { line: LineIdx },
    SymmetricH { p1: PointIdx, p2: PointIdx },
    SymmetricV { p1: PointIdx, p2: PointIdx },
    Midpoint { point: PointIdx, line: LineIdx },
    Dragged { point: PointIdx, target: Point2<f64> },
    Radius { r: RadiusIdx, target: f64 },
    Diameter { r: RadiusIdx, target: f64 },
    HDistance { p1x: usize, p2x: usize, d: f64 },  // raw x-indices (no PointIdx needed)
    VDistance { p1y: usize, p2y: usize, d: f64 },

    // ── Group 2: Nonlinear fundamentals ──
    DistancePP { p1: PointIdx, p2: PointIdx, d: f64 },
    EqualLength { l1: LineIdx, l2: LineIdx },
    Parallel { l1: LineIdx, l2: LineIdx },
    Perpendicular { l1: LineIdx, l2: LineIdx },
    Angle { l1: LineIdx, l2: LineIdx, value_rad: f64 },

    // ── Group 3: Point-on-entity ──
    OnLine { point: PointIdx, line: LineIdx },
    OnCircle { point: PointIdx, center: PointIdx, radius: RadiusIdx },

    // ── Group 4: Normalized point-line distance ──
    DistancePL { point: PointIdx, line: LineIdx, d: f64 },

    // ── Group 5: Tangent ──
    TangentLineCircle { line: LineIdx, center: PointIdx, radius: RadiusIdx },
    TangentArcArc {
        c1: PointIdx, r1: RadiusIdx,
        c2: PointIdx, r2: RadiusIdx,
        internal: bool,  // true = |r1-r2|, false = r1+r2
    },

    // ── Group 6: Symmetric about arbitrary line ──
    SymmetricLine { p1: PointIdx, p2: PointIdx, line: LineIdx },
    // (2 equations: perpendicularity [Angle-type] + midpoint-on-line [Distance-type])

    // ── Group 7: Compound ──
    EqualAngle { l1: LineIdx, l2: LineIdx, l3: LineIdx, l4: LineIdx },
    Ratio { l1: LineIdx, l2: LineIdx, k: f64 },
    EqualPointToLine { p1: PointIdx, p2: PointIdx, line: LineIdx },
    SameOrientation,  // no-op in 2D (matches existing behavior + oracle test)
    EqualRadius { r1: RadiusIdx, r2: RadiusIdx },
}
```

Wave 1 implements `num_equations()` and `scale_types()` for each variant
(trivial constants). `residuals()` and `jacobian()` are `todo!()` stubs —
filled in by Fork A.

### 1.5 Constraint builder (`core/builder.rs`)

```rust
pub fn build_constraints(
    constraints: &[SketchConstraint],
    entities: &[SketchEntity],
    layout: &ParamLayout,
) -> Vec<ConstraintImpl>;

/// Map each equation row back to its parent constraint index.
/// Multi-equation constraints (Coincident, Midpoint, etc.) produce
/// multiple rows pointing to the same constraint index.
pub fn build_eq_to_constraint_map(constraints: &[ConstraintImpl]) -> Vec<usize>;
```

**Entity-type dispatch logic:**
- `Equal { a, b }` → both lines: `EqualLength`; both circles: `EqualRadius`
- `Distance { a, b, value }` → both points: `DistancePP`; point+line: `DistancePL`
- `OnEntity { point, entity }` → line: `OnLine`; circle/arc: `OnCircle`
- `Tangent { line, curve }` → `TangentLineCircle`
  (arc-arc tangency: no waffle-types variant yet, but solver supports it)
- `Radius { entity, value }` → if circle: `Radius { r, target }`;
  if arc: `DistancePP(center, start_point, value)` (implicit radius)
- `Diameter { entity, value }` → same logic, `target = value / 2.0`
- `Angle { line_a, line_b, value_degrees }` → convert to radians
- `SameOrientation { .. }` → `SameOrientation` (no-op)

### 1.6 Cargo.toml changes

```toml
[dependencies]
waffle-types = { path = "../waffle-types" }
slvs = "0.6"  # keep until Wave 3
nalgebra = "0.33"
serde = { version = "1", features = ["derive"] }
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "1"

[features]
render = ["dep:svg", "dep:resvg"]

[dev-dependencies]
proptest = "1.6"

[dependencies.svg]
version = "0.17"
optional = true

[dependencies.resvg]
version = "0.44"
optional = true
```

### 1.7 Verification

- `cargo check -p sketch-solver` compiles (new + existing code)
- Existing 59 tests still pass (slvs solver still wired up)
- New modules: types, stubs, and `num_equations()`/`scale_types()` implemented
- `residuals()`/`jacobian()` are `todo!()` — filled in by Fork A

## Opus / Gemini Task Breakdown

### Opus (sequential, first)

**1a. Design trait + types + module wiring**
- Write `core/types.rs`: `PointIdx`, `LineIdx`, `RadiusIdx`, `ScaleType`,
  `SolveOptions`, `SolveOutcome`
- Write `core/constraint.rs`: `ConstraintEq` trait definition
- Write `core/mod.rs`: re-exports
- Update `src/lib.rs`: add `pub mod core;`
- Commit trait definitions so Gemini workers can compile against them

### Gemini workers (parallel, after Opus commits traits)

**W1-G1: ParamLayout** (`core/params.rs`)
- Full implementation of `ParamLayout` with typed index returns
- `from_entities()`, `initial_params()`, `point()`, `radius()`, `line()`,
  `num_params()`, `extract_positions()`
- Unit tests: build layout from entities, verify indices

**W1-G2: ConstraintImpl enum** (`core/constraint.rs`)
- All variant definitions with typed indices (as shown above)
- `impl ConstraintEq for ConstraintImpl`: `num_equations()` + `scale_types()`
  implemented for all variants, `residuals()` + `jacobian()` as `todo!()`
- Constant data: scale type tables per R4 classification

**W1-G3: Constraint builder** (`core/builder.rs`)
- `build_constraints()` with entity-type dispatch
- `build_eq_to_constraint_map()`
- Unit tests: build from sample SketchConstraints, verify correct variant + indices

**W1-G4: Cargo.toml + stubs**
- Cargo.toml changes (add nalgebra, feature flags, proptest)
- Stub files: `core/lm.rs`, `core/rank.rs`, `core/status.rs` with function
  signatures matching Fork B plan
- Verify: `cargo check -p sketch-solver` compiles, 59 tests still pass
