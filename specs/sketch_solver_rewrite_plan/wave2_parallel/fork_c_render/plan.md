# Wave 2 / Fork C: SVG Render Pipeline

**Executor**: Claude fork (worktree), with Gemini workers
**Depends on**: Wave 1 (ParamLayout — but only loosely, mainly needs waffle-types)
**Parallel with**: Fork A (constraints), Fork B (numerics)
**Independent**: Can merge at any time — doesn't block Wave 3
**Estimated scope**: ~600 lines

## Goal

Build a feature-gated SVG/PNG rendering pipeline for visual verification of
solved sketches. This enables:
1. Human review of solver output (SVG files in test artifacts)
2. Gemini-assisted visual review of rendered constraint systems
3. Snapshot testing for regression detection

## Feature Gate

```toml
[features]
render = ["dep:svg", "dep:resvg", "dep:tiny-skia"]
```

All render code behind `#[cfg(feature = "render")]`. The solver crate compiles
and works without rendering — rendering is a dev/test/debug tool.

## Worker Breakdown

### Worker C1: SVG generation (`render/svg.rs`)

```rust
#[cfg(feature = "render")]
pub fn render_sketch_svg(
    sketch: &Sketch,
    solved: &SolvedSketch,
) -> String;
```

**Layers (drawn in order):**

1. **Grid background**
   - Light gray grid at 10mm intervals (0.01m in kernel units)
   - Darker grid at 100mm intervals (0.1m)
   - X/Y axes in dark gray
   - Scale: auto-fit to sketch bounding box with 20% padding

2. **Entities**
   - Points: small filled circles (r=3px)
   - Lines: stroke between solved endpoint positions
   - Circles: stroke circle at solved center + radius
   - Arcs: SVG arc path between solved start/end through center
   - Construction geometry: dashed stroke

3. **DOF coloring**
   - `FullyConstrained` → entities in green (#2ecc71)
   - `UnderConstrained` → entities in amber (#f39c12)
   - `OverConstrained` → entities in red (#e74c3c)
   - `SolveFailed` → entities in red, dashed

4. **Constraint annotations** (simplified — not full CAD constraint display)
   - Distance: thin line between points with value label
   - Angle: arc between lines with degree label
   - Horizontal/Vertical: small "H"/"V" badge near line midpoint
   - Parallel: "//" badge
   - Perpendicular: "⊥" badge
   - Coincident: concentric circle marker
   - Other: small "C" badge with constraint type abbreviation

5. **Profile highlighting**
   - Closed profiles: light blue fill (#3498db, 15% opacity)
   - Outer vs hole: different fill opacity

**Coordinate transform:**
- Sketch coords (meters, Y-up) → SVG coords (pixels, Y-down)
- Auto viewBox from bounding box of solved positions

### Worker C2: PNG rasterization (`render/png.rs`)

```rust
#[cfg(feature = "render")]
pub fn render_sketch_png(
    sketch: &Sketch,
    solved: &SolvedSketch,
    width: u32,
    height: u32,
) -> Vec<u8>;
```

Uses `resvg` to rasterize SVG string to PNG bytes. Simple wrapper:
1. Generate SVG via `render_sketch_svg()`
2. Parse with `resvg::usvg::Tree::from_str()`
3. Render to `tiny_skia::Pixmap`
4. Encode to PNG

### Worker C3: Test fixtures + example

- `examples/render_sketch.rs`: CLI that reads sketch JSON from stdin, solves,
  writes SVG to stdout and PNG to a file. Usage:
  ```
  echo '{"entities": [...], "constraints": [...]}' | cargo run --example render_sketch --features render > sketch.svg
  ```

- Test fixtures: create 3-4 canonical sketch JSON files in
  `tests/fixtures/` (rectangle, circle, triangle with constraints)

- Integration test: for each fixture, solve + render SVG, assert SVG contains
  expected element counts (correct number of `<line>`, `<circle>`, `<text>`
  elements)

## Deliverables

- `src/render/mod.rs`: feature-gated module
- `src/render/svg.rs`: SVG generation
- `src/render/png.rs`: PNG rasterization
- `examples/render_sketch.rs`: CLI tool
- `tests/fixtures/*.json`: canonical sketch files
- `tests/render_tests.rs`: structural SVG assertions

## Verification

- `cargo test -p sketch-solver --features render -- render` — all pass
- `cargo run --example render_sketch --features render < tests/fixtures/rectangle.json > /tmp/test.svg` — produces valid SVG
- Visual inspection of generated SVGs for canonical cases
