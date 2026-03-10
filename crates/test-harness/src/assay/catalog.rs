//! Assay catalog types — used by runner.rs for recipe-based test execution.
//!
//! The 400 hardcoded test cases have been replaced by the randomized assay system
//! in gen.rs + randomized_runner.rs. These types are retained for backward compatibility
//! with tests that use AssayRecipe-based execution.

/// Identifies the test category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssayCategory {
    SingleBoolean,
    ChainedBoolean,
    ExtrudeRevolve,
    EdgeCase,
    StressDegenerate,
    /// Rotated variants: same geometry on non-XY planes to catch axis assumptions.
    PlaneRotation,
}

/// Sketch profile for an operation.
///
/// COVERAGE GAP NOTE (Sprint 68): `Profile::Circle` produces a 16-sided polygon
/// via `circle_profile()` in `helpers.rs`, NOT a native `SketchEntity::Circle`.
/// The kernel extrudes polygons through the prism path → `box_box_boolean` or
/// `box_cyl_boolean`. The `cyl_cyl_boolean` path (reached only when BOTH solids
/// are native circles via `extrude_circle()`) is NOT exercised by the assay.
///
/// This caused the circle-cut-nobody bug (non-Z-axis cylinders returning
/// "no Z overlap") to go undetected. The fix was a frame rotation in
/// `cyl_cyl_boolean`, caught by ZR7-ZR9 in `cyl_cyl_cut_regression.rs`.
///
/// Future work: add a `TrueCircle` variant that dispatches to `true_circle_sketch()`
/// so the assay covers the native cylinder boolean path.
#[derive(Debug, Clone)]
pub enum Profile {
    /// Rectangle: center_x, center_y, width, height
    Rect { cx: f64, cy: f64, w: f64, h: f64 },
    /// Circle: center_x, center_y, radius (NOTE: produces polygon, not native circle — see above)
    Circle { cx: f64, cy: f64, r: f64 },
}

/// Boolean operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
}

/// A step in a chained boolean sequence.
#[derive(Debug, Clone)]
pub struct ChainStep {
    pub op: BoolOp,
    pub operand: Box<AssayRecipe>,
}

/// Declarative operation sequence describing how to build the solid.
#[derive(Debug, Clone)]
pub enum AssayRecipe {
    /// Extrude a profile along a direction.
    Extrude {
        profile: Profile,
        origin: [f64; 3],
        normal: [f64; 3],
        depth: f64,
    },
    /// Boolean of two sub-recipes.
    Boolean {
        a: Box<AssayRecipe>,
        b: Box<AssayRecipe>,
        op: BoolOp,
    },
    /// Revolve a profile around an axis.
    Revolve {
        profile: Profile,
        origin: [f64; 3],
        normal: [f64; 3],
        axis_origin: [f64; 3],
        axis_dir: [f64; 3],
        angle_rad: f64,
    },
    /// Chained boolean: initial solid + sequential steps.
    Chain {
        initial: Box<AssayRecipe>,
        steps: Vec<ChainStep>,
    },
}

/// Analytical ground truth for a test case.
#[derive(Debug, Clone)]
pub struct AssayExpected {
    /// Expected volume in cubic meters (from geometry, NOT from kernel).
    pub volume: Option<f64>,
    /// Tolerance for volume comparison (absolute).
    pub volume_tol: f64,
    /// Expected Euler characteristic V-E+F (usually 2 for genus-0).
    pub euler: Option<i64>,
    /// Expected face count.
    pub face_count: Option<usize>,
    /// Whether the result should be watertight (zero open edges).
    pub watertight: bool,
    /// Expected axis-aligned bounding box ([min], [max]).
    pub bbox: Option<([f64; 3], [f64; 3])>,
}

/// A single assay test case.
#[derive(Debug, Clone)]
pub struct AssayCase {
    /// Unique ID: "S001" through "S400".
    pub id: &'static str,
    /// Human-readable description.
    pub description: &'static str,
    /// Test category.
    pub category: AssayCategory,
    /// Declarative recipe to build the solid.
    pub recipe: AssayRecipe,
    /// Analytical ground truth.
    pub expected: AssayExpected,
}
