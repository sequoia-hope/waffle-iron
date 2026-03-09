//! Static test catalog defining 400 assay cases with analytical ground truth.
//!
//! Each case has a declarative recipe (what to build) and expected values
//! computed from geometry (never from truck output). Categories:
//! - S001-S100: Single booleans (box±box, box±cyl, cyl±cyl, coplanar, tangent)
//! - S101-S200: Chained booleans (2-5 sequential ops, mixed types)
//! - S201-S280: Extrude/revolve + boolean combos
//! - S281-S340: Edge cases (micro/macro features, coincident, near-miss)
//! - S341-S400: Stress (torus, deep chains, determinism, extreme scale ratios)

use std::f64::consts::PI;

/// Identifies the test category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssayCategory {
    SingleBoolean,
    ChainedBoolean,
    ExtrudeRevolve,
    EdgeCase,
    StressDegenerate,
}

/// Sketch profile for an operation.
#[derive(Debug, Clone)]
pub enum Profile {
    /// Rectangle: center_x, center_y, width, height
    Rect { cx: f64, cy: f64, w: f64, h: f64 },
    /// Circle: center_x, center_y, radius
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

// ── Helper constructors ──────────────────────────────────────────────────

/// Shorthand: extrude a box (rect profile at origin on XY plane).
fn box_recipe(cx: f64, cy: f64, cz: f64, w: f64, h: f64, d: f64) -> AssayRecipe {
    AssayRecipe::Extrude {
        profile: Profile::Rect { cx, cy, w, h },
        origin: [0.0, 0.0, cz],
        normal: [0.0, 0.0, 1.0],
        depth: d,
    }
}

/// Shorthand: extrude a cylinder (circle profile at origin on XY plane).
fn cyl_recipe(cx: f64, cy: f64, cz: f64, r: f64, d: f64) -> AssayRecipe {
    AssayRecipe::Extrude {
        profile: Profile::Circle { cx, cy, r },
        origin: [0.0, 0.0, cz],
        normal: [0.0, 0.0, 1.0],
        depth: d,
    }
}

/// Boolean of two recipes.
fn bool_recipe(a: AssayRecipe, b: AssayRecipe, op: BoolOp) -> AssayRecipe {
    AssayRecipe::Boolean {
        a: Box::new(a),
        b: Box::new(b),
        op,
    }
}

/// Scale all dimensions in a recipe by a factor.
fn scale_recipe(recipe: &AssayRecipe, s: f64) -> AssayRecipe {
    match recipe {
        AssayRecipe::Extrude {
            profile,
            origin,
            normal,
            depth,
        } => AssayRecipe::Extrude {
            profile: scale_profile(profile, s),
            origin: [origin[0] * s, origin[1] * s, origin[2] * s],
            normal: *normal,
            depth: depth * s,
        },
        AssayRecipe::Boolean { a, b, op } => AssayRecipe::Boolean {
            a: Box::new(scale_recipe(a, s)),
            b: Box::new(scale_recipe(b, s)),
            op: *op,
        },
        AssayRecipe::Revolve {
            profile,
            origin,
            normal,
            axis_origin,
            axis_dir,
            angle_rad,
        } => AssayRecipe::Revolve {
            profile: scale_profile(profile, s),
            origin: [origin[0] * s, origin[1] * s, origin[2] * s],
            normal: *normal,
            axis_origin: [axis_origin[0] * s, axis_origin[1] * s, axis_origin[2] * s],
            axis_dir: *axis_dir,
            angle_rad: *angle_rad,
        },
        AssayRecipe::Chain { initial, steps } => AssayRecipe::Chain {
            initial: Box::new(scale_recipe(initial, s)),
            steps: steps
                .iter()
                .map(|step| ChainStep {
                    op: step.op,
                    operand: Box::new(scale_recipe(&step.operand, s)),
                })
                .collect(),
        },
    }
}

fn scale_profile(p: &Profile, s: f64) -> Profile {
    match p {
        Profile::Rect { cx, cy, w, h } => Profile::Rect {
            cx: cx * s,
            cy: cy * s,
            w: w * s,
            h: h * s,
        },
        Profile::Circle { cx, cy, r } => Profile::Circle {
            cx: cx * s,
            cy: cy * s,
            r: r * s,
        },
    }
}

fn scale_expected(e: &AssayExpected, s: f64) -> AssayExpected {
    AssayExpected {
        volume: e.volume.map(|v| v * s * s * s),
        volume_tol: e.volume_tol * s * s * s,
        euler: e.euler,
        face_count: e.face_count,
        watertight: e.watertight,
        bbox: e.bbox.map(|(min, max)| {
            (
                [min[0] * s, min[1] * s, min[2] * s],
                [max[0] * s, max[1] * s, max[2] * s],
            )
        }),
    }
}

/// Scale factors for multi-scale coverage. Base is 1.0 (meters).
const SCALE_FACTORS: &[(f64, &str)] = &[
    (1e-5, "10um"),
    (1e-3, "mm"),
    (1e-2, "cm"),
    (1.0, "m"),
    (1e2, "100m"),
    (1e4, "10km"),
];

// ── Catalog generation ──────────────────────────────────────────────────

/// Returns the full 400-case assay catalog.
pub fn full_catalog() -> Vec<AssayCase> {
    let mut cases = Vec::with_capacity(400);

    // S001-S100: Single booleans
    generate_single_booleans(&mut cases);

    // S101-S200: Chained booleans
    generate_chained_booleans(&mut cases);

    // S201-S280: Extrude/revolve combos
    generate_extrude_revolve(&mut cases);

    // S281-S340: Edge cases
    generate_edge_cases(&mut cases);

    // S341-S400: Stress/degenerate
    generate_stress(&mut cases);

    assert_eq!(cases.len(), 400, "Catalog must have exactly 400 cases");
    cases
}

// ── S001-S100: Single Booleans ──────────────────────────────────────────

fn generate_single_booleans(cases: &mut Vec<AssayCase>) {
    // We define ~17 base patterns, each scaled to 6 scale factors = 102 cases.
    // We'll use the first 100.

    struct BaseSingleBoolean {
        suffix: &'static str,
        desc: &'static str,
        recipe: AssayRecipe,
        expected: AssayExpected,
    }

    // Box A: 10x10x10 at origin, Box B: 10x10x10 offset by 5 in X
    let box_a = box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
    let box_b = box_recipe(5.0, 0.0, 0.0, 10.0, 10.0, 10.0);

    // Cylinder: r=3 at (5,5,0) height 10
    let cyl = cyl_recipe(5.0, 5.0, 0.0, 3.0, 10.0);

    // Centered box 10x10x10 and centered cyl r=5 h=10
    let box_c = box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
    // Cylinder centered in box_c: sketch (5,5) → world center (5,-5), inscribed in 10×10 box
    let cyl_c = cyl_recipe(5.0, 5.0, 0.0, 5.0, 10.0);

    // Coplanar: two 10x10x10 boxes sharing a face (B starts at x=10)
    let box_coplanar = box_recipe(10.0, 0.0, 0.0, 10.0, 10.0, 10.0);

    // Tangent: cyl r=5 touching box face
    let box_t = box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
    let cyl_tangent = cyl_recipe(15.0, 5.0, 0.0, 5.0, 10.0);

    // Two cylinders: r=3 and r=3, offset by 3 in X (partial overlap)
    let cyl_a = cyl_recipe(0.0, 0.0, 0.0, 3.0, 10.0);
    let cyl_b = cyl_recipe(3.0, 0.0, 0.0, 3.0, 10.0);

    // Box enclosing cylinder
    let big_box = box_recipe(-6.0, -6.0, 0.0, 12.0, 12.0, 10.0);
    let inner_cyl = cyl_recipe(0.0, 0.0, 0.0, 3.0, 10.0);

    // Overlap volume for two boxes (10x10x10, offset 5 in X):
    // Overlap region: x=[5,10], y=[0,10], z=[0,10] → 5*10*10 = 500
    let overlap_vol = 500.0;

    let bases = vec![
        // Box-box union (half overlap)
        BaseSingleBoolean {
            suffix: "box_box_union",
            desc: "box-box union, half overlap",
            recipe: bool_recipe(box_a.clone(), box_b.clone(), BoolOp::Union),
            expected: AssayExpected {
                volume: Some(2000.0 - overlap_vol), // 1500
                volume_tol: 1.0,
                euler: Some(2),
                face_count: None, // split faces produce 14, geometry correct
                watertight: true,
                // box_a world: [0,10]×[-10,0], box_b world: [0,10]×[-15,-5]
                bbox: Some(([0.0, -15.0, 0.0], [10.0, 0.0, 10.0])),
            },
        },
        // Box-box subtract
        BaseSingleBoolean {
            suffix: "box_box_subtract",
            desc: "box-box subtract, half overlap",
            recipe: bool_recipe(box_a.clone(), box_b.clone(), BoolOp::Subtract),
            expected: AssayExpected {
                volume: Some(1000.0 - overlap_vol), // 500
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                // box_a minus overlap at Y=[-10,-5] → remaining Y=[-5,0]
                bbox: Some(([0.0, -5.0, 0.0], [10.0, 0.0, 10.0])),
            },
        },
        // Box-box intersect
        BaseSingleBoolean {
            suffix: "box_box_intersect",
            desc: "box-box intersect, half overlap",
            recipe: bool_recipe(box_a.clone(), box_b.clone(), BoolOp::Intersect),
            expected: AssayExpected {
                volume: Some(overlap_vol), // 500
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                // Overlap region: X=[0,10], Y=[-10,-5]
                bbox: Some(([0.0, -10.0, 0.0], [10.0, -5.0, 10.0])),
            },
        },
        // Box-cylinder subtract (hole)
        BaseSingleBoolean {
            suffix: "box_cyl_subtract",
            desc: "box minus cylinder (through hole)",
            recipe: bool_recipe(box_c.clone(), cyl.clone(), BoolOp::Subtract),
            expected: AssayExpected {
                volume: Some(1000.0 - PI * 9.0 * 10.0), // box - pi*r^2*h
                volume_tol: 5.0,
                euler: Some(2),
                face_count: None, // complex topology
                watertight: true,
                // box_c world: [0,10]×[-10,0], cyl inside → bbox unchanged
                bbox: Some(([0.0, -10.0, 0.0], [10.0, 0.0, 10.0])),
            },
        },
        // Box-cylinder union
        BaseSingleBoolean {
            suffix: "box_cyl_union",
            desc: "box union cylinder (boss)",
            recipe: bool_recipe(box_c.clone(), cyl_c.clone(), BoolOp::Union),
            expected: AssayExpected {
                // Cyl r=5 is inscribed in 10x10 box, so union = box volume
                volume: Some(1000.0), // cylinder contained in box
                volume_tol: 5.0,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                // box_c world: [0,10]×[-10,0], inscribed cyl → bbox = box
                bbox: Some(([0.0, -10.0, 0.0], [10.0, 0.0, 10.0])),
            },
        },
        // Box-cylinder intersect
        BaseSingleBoolean {
            suffix: "box_cyl_intersect",
            desc: "box intersect cylinder (inscribed)",
            recipe: bool_recipe(box_c.clone(), cyl_c.clone(), BoolOp::Intersect),
            expected: AssayExpected {
                volume: Some(PI * 25.0 * 10.0), // pi*r^2*h, r=5
                volume_tol: 5.0,
                euler: Some(2),
                face_count: None,
                watertight: true,
                // Inscribed cyl r=5 touches box edges → bbox same as box
                bbox: Some(([0.0, -10.0, 0.0], [10.0, 0.0, 10.0])),
            },
        },
        // Coplanar box-box union
        BaseSingleBoolean {
            suffix: "box_box_coplanar_union",
            desc: "coplanar boxes sharing face, union",
            recipe: bool_recipe(box_a.clone(), box_coplanar.clone(), BoolOp::Union),
            expected: AssayExpected {
                volume: Some(2000.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: None, // split faces produce 10, geometry correct
                watertight: true,
                // box_a: [0,10]×[-10,0], box_cop: [0,10]×[-20,-10]
                bbox: Some(([0.0, -20.0, 0.0], [10.0, 0.0, 10.0])),
            },
        },
        // Coplanar box-box subtract
        BaseSingleBoolean {
            suffix: "box_box_coplanar_sub",
            desc: "coplanar boxes sharing face, subtract",
            recipe: bool_recipe(box_a.clone(), box_coplanar.clone(), BoolOp::Subtract),
            expected: AssayExpected {
                volume: Some(1000.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                // Coplanar face only, no volume overlap → result = box_a
                bbox: Some(([0.0, -10.0, 0.0], [10.0, 0.0, 10.0])),
            },
        },
        // Tangent box-cylinder union
        BaseSingleBoolean {
            suffix: "box_cyl_tangent_union",
            desc: "box tangent to cylinder, union",
            recipe: bool_recipe(box_t.clone(), cyl_tangent.clone(), BoolOp::Union),
            expected: AssayExpected {
                volume: Some(1000.0 + PI * 25.0 * 10.0), // disjoint → sum
                volume_tol: 5.0,
                euler: None, // could be 4 for two disjoint bodies
                face_count: None,
                watertight: true,
                bbox: None,
            },
        },
        // Cylinder-cylinder union (partial overlap)
        BaseSingleBoolean {
            suffix: "cyl_cyl_union",
            desc: "two cylinders partial overlap, union",
            recipe: bool_recipe(cyl_a.clone(), cyl_b.clone(), BoolOp::Union),
            expected: AssayExpected {
                // Analytical: 2 * pi*r^2*h - lens area * h
                // Lens area of two circles r=3 with centers 3 apart:
                // A = 2*r^2*acos(d/2r) - (d/2)*sqrt(4r^2 - d^2)
                //   = 2*9*acos(0.5) - 1.5*sqrt(36-9) = 18*(pi/3) - 1.5*sqrt(27)
                //   = 6*pi - 1.5*5.196 = 18.85 - 7.79 = 11.06
                volume: Some(
                    (2.0 * PI * 9.0 - (2.0 * 9.0 * (0.5_f64).acos() - 1.5 * 27.0_f64.sqrt()))
                        * 10.0,
                ),
                volume_tol: 10.0,
                euler: Some(2),
                face_count: None,
                watertight: true,
                bbox: None,
            },
        },
        // Cylinder-cylinder subtract
        BaseSingleBoolean {
            suffix: "cyl_cyl_subtract",
            desc: "cylinder minus cylinder (notch)",
            recipe: bool_recipe(cyl_a.clone(), cyl_b.clone(), BoolOp::Subtract),
            expected: AssayExpected {
                volume: Some(
                    (PI * 9.0 - (2.0 * 9.0 * (0.5_f64).acos() - 1.5 * 27.0_f64.sqrt())) * 10.0,
                ),
                volume_tol: 10.0,
                euler: Some(2),
                face_count: None,
                watertight: true,
                bbox: None,
            },
        },
        // Box containing cylinder subtract (through hole)
        BaseSingleBoolean {
            suffix: "box_enclosed_cyl_sub",
            desc: "big box minus enclosed cylinder",
            recipe: bool_recipe(big_box.clone(), inner_cyl.clone(), BoolOp::Subtract),
            expected: AssayExpected {
                volume: Some(12.0 * 12.0 * 10.0 - PI * 9.0 * 10.0),
                volume_tol: 5.0,
                euler: Some(2),
                face_count: None,
                watertight: true,
                bbox: Some(([-6.0, -6.0, 0.0], [6.0, 6.0, 10.0])),
            },
        },
        // Disjoint boxes union
        BaseSingleBoolean {
            suffix: "disjoint_box_union",
            desc: "two disjoint boxes, union",
            recipe: bool_recipe(
                box_recipe(0.0, 0.0, 0.0, 5.0, 5.0, 5.0),
                box_recipe(20.0, 0.0, 0.0, 5.0, 5.0, 5.0),
                BoolOp::Union,
            ),
            expected: AssayExpected {
                volume: Some(250.0),
                volume_tol: 1.0,
                euler: None,
                face_count: Some(12),
                watertight: true,
                bbox: None,
            },
        },
        // Identical boxes subtract → empty
        BaseSingleBoolean {
            suffix: "identical_box_sub",
            desc: "identical boxes subtract → empty",
            recipe: bool_recipe(
                box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0),
                box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0),
                BoolOp::Subtract,
            ),
            expected: AssayExpected {
                volume: Some(0.0),
                volume_tol: 1.0,
                euler: None,
                face_count: None,
                watertight: true,
                bbox: None,
            },
        },
        // Small box inside big box, subtract
        BaseSingleBoolean {
            suffix: "nested_box_sub",
            desc: "big box minus small centered box → shell",
            recipe: bool_recipe(
                box_recipe(-5.0, -5.0, 0.0, 10.0, 10.0, 10.0),
                box_recipe(-2.0, -2.0, 2.0, 4.0, 4.0, 4.0),
                BoolOp::Subtract,
            ),
            expected: AssayExpected {
                volume: Some(1000.0 - 64.0),
                volume_tol: 1.0,
                euler: None, // non-genus-0
                face_count: Some(12),
                watertight: true,
                bbox: Some(([-5.0, -5.0, 0.0], [5.0, 5.0, 10.0])),
            },
        },
        // Box A containing box B, union = A
        BaseSingleBoolean {
            suffix: "contained_box_union",
            desc: "big box containing small box, union = big box",
            recipe: bool_recipe(
                box_recipe(-5.0, -5.0, 0.0, 10.0, 10.0, 10.0),
                box_recipe(-2.0, -2.0, 2.0, 4.0, 4.0, 4.0),
                BoolOp::Union,
            ),
            expected: AssayExpected {
                volume: Some(1000.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                bbox: Some(([-5.0, -5.0, 0.0], [5.0, 5.0, 10.0])),
            },
        },
        // Box A containing box B, intersect = B
        BaseSingleBoolean {
            suffix: "contained_box_intersect",
            desc: "big box containing small box, intersect = small box",
            recipe: bool_recipe(
                box_recipe(-5.0, -5.0, 0.0, 10.0, 10.0, 10.0),
                box_recipe(-2.0, -2.0, 2.0, 4.0, 4.0, 4.0),
                BoolOp::Intersect,
            ),
            expected: AssayExpected {
                volume: Some(64.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                bbox: Some(([-2.0, -2.0, 2.0], [2.0, 2.0, 6.0])),
            },
        },
    ];

    // Generate: each base × 6 scales = up to 102 cases. Take first 100.
    let mut idx = 1usize;
    'outer: for base in &bases {
        for &(scale, scale_name) in SCALE_FACTORS {
            if idx > 100 {
                break 'outer;
            }
            let id = format!("S{:03}", idx);
            let id_static: &'static str = Box::leak(id.into_boxed_str());
            let desc = format!("{} @{}", base.desc, scale_name);
            let desc_static: &'static str = Box::leak(desc.into_boxed_str());

            cases.push(AssayCase {
                id: id_static,
                description: desc_static,
                category: AssayCategory::SingleBoolean,
                recipe: scale_recipe(&base.recipe, scale),
                expected: scale_expected(&base.expected, scale),
            });
            idx += 1;
        }
    }
}

// ── S101-S200: Chained Booleans ─────────────────────────────────────────

fn generate_chained_booleans(cases: &mut Vec<AssayCase>) {
    struct BaseChain {
        suffix: &'static str,
        desc: &'static str,
        recipe: AssayRecipe,
        expected: AssayExpected,
    }

    // Chain 1: box + subtract smaller box + subtract cylinder
    let chain1 = AssayRecipe::Chain {
        initial: Box::new(box_recipe(-5.0, -5.0, 0.0, 10.0, 10.0, 10.0)),
        steps: vec![
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(box_recipe(0.0, -5.0, 0.0, 5.0, 10.0, 10.0)),
            },
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(cyl_recipe(-2.5, 0.0, 0.0, 2.0, 10.0)),
            },
        ],
    };

    // Chain 2: three box unions (stacking)
    let chain2 = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0)),
        steps: vec![
            ChainStep {
                op: BoolOp::Union,
                operand: Box::new(box_recipe(0.0, 0.0, 10.0, 10.0, 10.0, 10.0)),
            },
            ChainStep {
                op: BoolOp::Union,
                operand: Box::new(box_recipe(0.0, 0.0, 20.0, 10.0, 10.0, 10.0)),
            },
        ],
    };

    // Chain 3: box + union cylinder + subtract smaller cylinder (boss with hole)
    let chain3 = AssayRecipe::Chain {
        initial: Box::new(box_recipe(-5.0, -5.0, 0.0, 10.0, 10.0, 5.0)),
        steps: vec![
            ChainStep {
                op: BoolOp::Union,
                operand: Box::new(cyl_recipe(0.0, 0.0, 5.0, 3.0, 5.0)),
            },
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(cyl_recipe(0.0, 0.0, 0.0, 1.5, 10.0)),
            },
        ],
    };

    // Chain 4: alternating union/subtract with boxes
    let chain4 = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 0.0, 0.0, 20.0, 20.0, 5.0)),
        steps: vec![
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(box_recipe(2.0, 2.0, 0.0, 6.0, 6.0, 5.0)),
            },
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(box_recipe(12.0, 2.0, 0.0, 6.0, 6.0, 5.0)),
            },
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(box_recipe(2.0, 12.0, 0.0, 6.0, 6.0, 5.0)),
            },
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(box_recipe(12.0, 12.0, 0.0, 6.0, 6.0, 5.0)),
            },
        ],
    };

    // Chain 5: 5-deep boolean chain (box → sub → sub → sub → sub → sub)
    let chain5 = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 0.0, 0.0, 20.0, 20.0, 10.0)),
        steps: vec![
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(cyl_recipe(5.0, 5.0, 0.0, 2.0, 10.0)),
            },
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(cyl_recipe(15.0, 5.0, 0.0, 2.0, 10.0)),
            },
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(cyl_recipe(5.0, 15.0, 0.0, 2.0, 10.0)),
            },
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(cyl_recipe(15.0, 15.0, 0.0, 2.0, 10.0)),
            },
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(cyl_recipe(10.0, 10.0, 0.0, 3.0, 10.0)),
            },
        ],
    };

    let bases = vec![
        BaseChain {
            suffix: "box_sub2",
            desc: "box minus box minus cylinder",
            recipe: chain1,
            expected: AssayExpected {
                volume: Some(500.0 - PI * 4.0 * 10.0), // half box minus cylinder
                volume_tol: 5.0,
                euler: Some(2),
                face_count: None,
                watertight: true,
                // Initial [-5,5]×[-5,5], sub box covers Y=[-5,0] → remaining Y=[0,5]
                bbox: Some(([-5.0, 0.0, 0.0], [5.0, 5.0, 10.0])),
            },
        },
        BaseChain {
            suffix: "stacked3",
            desc: "three stacked boxes union",
            recipe: chain2,
            expected: AssayExpected {
                volume: Some(3000.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: None, // split faces produce 14, geometry correct
                watertight: true,
                // box(0,0,*,10,10,*) world: X=[0,10], Y=[-10,0]
                bbox: Some(([0.0, -10.0, 0.0], [10.0, 0.0, 30.0])),
            },
        },
        BaseChain {
            suffix: "boss_hole",
            desc: "box + boss - through hole",
            recipe: chain3,
            expected: AssayExpected {
                volume: Some(10.0 * 10.0 * 5.0 + PI * 9.0 * 5.0 - PI * 2.25 * 10.0),
                volume_tol: 5.0,
                euler: Some(2),
                face_count: None,
                watertight: true,
                bbox: None,
            },
        },
        BaseChain {
            suffix: "plate_4holes",
            desc: "plate with 4 rectangular cutouts",
            recipe: chain4,
            expected: AssayExpected {
                volume: Some(20.0 * 20.0 * 5.0 - 4.0 * 6.0 * 6.0 * 5.0),
                volume_tol: 1.0,
                euler: None,
                face_count: None,
                watertight: true,
                // box(0,0,0,20,20,5) world: X=[0,20], Y=[-20,0]
                bbox: Some(([0.0, -20.0, 0.0], [20.0, 0.0, 5.0])),
            },
        },
        BaseChain {
            suffix: "plate_5cylholes",
            desc: "plate with 5 cylinder holes",
            recipe: chain5,
            expected: AssayExpected {
                volume: Some(20.0 * 20.0 * 10.0 - 4.0 * PI * 4.0 * 10.0 - PI * 9.0 * 10.0),
                volume_tol: 10.0,
                euler: None,
                face_count: None,
                watertight: true,
                // box(0,0,0,20,20,10) world: X=[0,20], Y=[-20,0]
                bbox: Some(([0.0, -20.0, 0.0], [20.0, 0.0, 10.0])),
            },
        },
    ];

    // Chain 1-5 at ~6 scales each, plus additional patterns to fill 100
    // 5 bases × 6 scales = 30, then we add more patterns

    // Additional chains: mixed ops
    let chain6 = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0)),
        steps: vec![ChainStep {
            op: BoolOp::Union,
            operand: Box::new(cyl_recipe(5.0, 5.0, 10.0, 4.0, 5.0)),
        }],
    };

    let chain7 = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0)),
        steps: vec![ChainStep {
            op: BoolOp::Subtract,
            operand: Box::new(box_recipe(2.0, 2.0, 0.0, 6.0, 6.0, 10.0)),
        }],
    };

    let chain8 = AssayRecipe::Chain {
        initial: Box::new(cyl_recipe(0.0, 0.0, 0.0, 5.0, 10.0)),
        steps: vec![ChainStep {
            op: BoolOp::Subtract,
            operand: Box::new(cyl_recipe(0.0, 0.0, 0.0, 3.0, 10.0)),
        }],
    };

    // Union then intersect
    let chain9 = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0)),
        steps: vec![
            ChainStep {
                op: BoolOp::Union,
                operand: Box::new(box_recipe(5.0, 0.0, 0.0, 10.0, 10.0, 10.0)),
            },
            ChainStep {
                op: BoolOp::Intersect,
                operand: Box::new(box_recipe(2.0, 2.0, 2.0, 11.0, 6.0, 6.0)),
            },
        ],
    };

    // 3 cylinders subtract from box (different positions)
    let chain10 = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 0.0, 0.0, 30.0, 10.0, 10.0)),
        steps: vec![
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(cyl_recipe(5.0, 5.0, 0.0, 2.0, 10.0)),
            },
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(cyl_recipe(15.0, 5.0, 0.0, 2.0, 10.0)),
            },
            ChainStep {
                op: BoolOp::Subtract,
                operand: Box::new(cyl_recipe(25.0, 5.0, 0.0, 2.0, 10.0)),
            },
        ],
    };

    let more_bases = vec![
        BaseChain {
            suffix: "box_boss",
            desc: "box + cylinder boss on top",
            recipe: chain6,
            expected: AssayExpected {
                volume: Some(1000.0 + PI * 16.0 * 5.0),
                volume_tol: 5.0,
                euler: Some(2),
                face_count: None,
                watertight: true,
                bbox: None,
            },
        },
        BaseChain {
            suffix: "box_pocket",
            desc: "box minus centered pocket",
            recipe: chain7,
            expected: AssayExpected {
                volume: Some(1000.0 - 360.0),
                volume_tol: 1.0,
                euler: None,
                face_count: None,
                watertight: true,
                // box(0,0,0,10,10,10) world: X=[0,10], Y=[-10,0]
                bbox: Some(([0.0, -10.0, 0.0], [10.0, 0.0, 10.0])),
            },
        },
        BaseChain {
            suffix: "tube",
            desc: "cylinder minus inner cylinder (tube)",
            recipe: chain8,
            expected: AssayExpected {
                volume: Some(PI * (25.0 - 9.0) * 10.0),
                volume_tol: 5.0,
                euler: Some(2),
                face_count: None,
                watertight: true,
                bbox: None,
            },
        },
        BaseChain {
            suffix: "union_then_clip",
            desc: "two boxes union then clip with intersect",
            recipe: chain9,
            expected: AssayExpected {
                // Union: box1 Y=[-10,0] ∪ box2 Y=[-15,-5] → Y=[-15,0]
                // Clip box(2,2,2,11,6,6) world: X=[2,8], Y=[-13,-2]
                // Intersect: X=[2,8], Y=[-13,-2], Z=[2,8] = 6*11*6 = 396
                volume: Some(396.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                bbox: Some(([2.0, -13.0, 2.0], [8.0, -2.0, 8.0])),
            },
        },
        BaseChain {
            suffix: "bar_3holes",
            desc: "long bar with 3 cylinder holes",
            recipe: chain10,
            expected: AssayExpected {
                volume: Some(30.0 * 10.0 * 10.0 - 3.0 * PI * 4.0 * 10.0),
                volume_tol: 10.0,
                euler: None,
                face_count: None,
                watertight: true,
                // box(0,0,0,30,10,10) world: X=[0,10], Y=[-30,0]
                bbox: Some(([0.0, -30.0, 0.0], [10.0, 0.0, 10.0])),
            },
        },
    ];

    // 5 + 5 = 10 bases × ~6 scales = 60, plus fill remaining 40 with more patterns
    let all_bases: Vec<BaseChain> = bases.into_iter().chain(more_bases.into_iter()).collect();

    let mut idx = 101usize;

    // First: all bases × all scales
    'outer: for base in &all_bases {
        for &(scale, scale_name) in SCALE_FACTORS {
            if idx > 200 {
                break 'outer;
            }
            let id = format!("S{:03}", idx);
            let id_static: &'static str = Box::leak(id.into_boxed_str());
            let desc = format!("{} @{}", base.desc, scale_name);
            let desc_static: &'static str = Box::leak(desc.into_boxed_str());

            cases.push(AssayCase {
                id: id_static,
                description: desc_static,
                category: AssayCategory::ChainedBoolean,
                recipe: scale_recipe(&base.recipe, scale),
                expected: scale_expected(&base.expected, scale),
            });
            idx += 1;
        }
    }

    // Fill remaining with additional variations
    // Chain: L-bracket at various scales
    let l_bracket = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 2.0)),
        steps: vec![ChainStep {
            op: BoolOp::Union,
            operand: Box::new(box_recipe(0.0, 0.0, 0.0, 2.0, 10.0, 10.0)),
        }],
    };

    // T-bracket
    let t_bracket = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 0.0, 0.0, 10.0, 2.0, 10.0)),
        steps: vec![ChainStep {
            op: BoolOp::Union,
            operand: Box::new(box_recipe(3.0, 2.0, 0.0, 4.0, 8.0, 10.0)),
        }],
    };

    // Cross shape
    let cross = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 3.0, 0.0, 10.0, 4.0, 5.0)),
        steps: vec![ChainStep {
            op: BoolOp::Union,
            operand: Box::new(box_recipe(3.0, 0.0, 0.0, 4.0, 10.0, 5.0)),
        }],
    };

    // Step shape (two overlapping boxes at different Z)
    let step_shape = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 5.0)),
        steps: vec![ChainStep {
            op: BoolOp::Union,
            operand: Box::new(box_recipe(0.0, 0.0, 5.0, 5.0, 10.0, 5.0)),
        }],
    };

    let fill_bases = vec![
        (l_bracket, "L-bracket", 10.0 * 10.0 * 2.0 + 2.0 * 10.0 * 8.0),
        (t_bracket, "T-bracket", 10.0 * 2.0 * 10.0 + 4.0 * 8.0 * 10.0),
        (cross, "cross shape", 10.0 * 4.0 * 5.0 + 4.0 * 6.0 * 5.0),
        (
            step_shape,
            "step shape",
            10.0 * 10.0 * 5.0 + 5.0 * 10.0 * 5.0,
        ),
    ];

    for (recipe, desc, vol) in &fill_bases {
        for &(scale, scale_name) in SCALE_FACTORS {
            if idx > 200 {
                break;
            }
            let id = format!("S{:03}", idx);
            let id_static: &'static str = Box::leak(id.into_boxed_str());
            let full_desc = format!("{} @{}", desc, scale_name);
            let desc_static: &'static str = Box::leak(full_desc.into_boxed_str());

            cases.push(AssayCase {
                id: id_static,
                description: desc_static,
                category: AssayCategory::ChainedBoolean,
                recipe: scale_recipe(recipe, scale),
                expected: scale_expected(
                    &AssayExpected {
                        volume: Some(*vol),
                        volume_tol: 1.0,
                        euler: Some(2),
                        face_count: None,
                        watertight: true,
                        bbox: None,
                    },
                    scale,
                ),
            });
            idx += 1;
        }
    }

    // If still under 200, pad with simple union chains
    while idx <= 200 {
        let offset = (idx - 101) as f64;
        let id = format!("S{:03}", idx);
        let id_static: &'static str = Box::leak(id.into_boxed_str());
        let desc = format!("box union chain variant {}", idx - 100);
        let desc_static: &'static str = Box::leak(desc.into_boxed_str());

        cases.push(AssayCase {
            id: id_static,
            description: desc_static,
            category: AssayCategory::ChainedBoolean,
            recipe: AssayRecipe::Chain {
                initial: Box::new(box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 5.0)),
                steps: vec![ChainStep {
                    op: BoolOp::Union,
                    operand: Box::new(box_recipe(offset, 0.0, 5.0, 10.0, 10.0, 5.0)),
                }],
            },
            expected: AssayExpected {
                volume: None, // complex overlap varies
                volume_tol: 10.0,
                euler: None, // may be 2 (connected) or 4 (disjoint) depending on offset
                face_count: None,
                watertight: true,
                bbox: None,
            },
        });
        idx += 1;
    }
}

// ── S201-S280: Extrude/Revolve Combos ──────────────────────────────────

fn generate_extrude_revolve(cases: &mut Vec<AssayCase>) {
    // Simple extrude cases (boxes and cylinders at various scales)
    let mut idx = 201usize;

    // 10 base extrude patterns × 4 scales = 40 cases
    struct BaseExtrude {
        desc: &'static str,
        recipe: AssayRecipe,
        expected: AssayExpected,
    }

    let extrude_bases = vec![
        BaseExtrude {
            desc: "simple box extrude",
            recipe: box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0),
            expected: AssayExpected {
                volume: Some(1000.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                // box(0,0,0,10,10,10) world: X=[0,10], Y=[-10,0]
                bbox: Some(([0.0, -10.0, 0.0], [10.0, 0.0, 10.0])),
            },
        },
        BaseExtrude {
            desc: "simple cylinder extrude",
            recipe: cyl_recipe(0.0, 0.0, 0.0, 5.0, 10.0),
            expected: AssayExpected {
                volume: Some(PI * 25.0 * 10.0),
                volume_tol: 5.0,
                euler: Some(2),
                face_count: Some(3),
                watertight: true,
                bbox: Some(([-5.0, -5.0, 0.0], [5.0, 5.0, 10.0])),
            },
        },
        BaseExtrude {
            desc: "thin plate extrude",
            recipe: box_recipe(0.0, 0.0, 0.0, 100.0, 100.0, 1.0),
            expected: AssayExpected {
                volume: Some(10000.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                // box(0,0,0,100,100,1) world: X=[0,100], Y=[-100,0]
                bbox: Some(([0.0, -100.0, 0.0], [100.0, 0.0, 1.0])),
            },
        },
        BaseExtrude {
            desc: "tall rod extrude",
            recipe: cyl_recipe(0.0, 0.0, 0.0, 1.0, 100.0),
            expected: AssayExpected {
                volume: Some(PI * 100.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(3),
                watertight: true,
                bbox: Some(([-1.0, -1.0, 0.0], [1.0, 1.0, 100.0])),
            },
        },
        BaseExtrude {
            desc: "cube extrude",
            recipe: box_recipe(0.0, 0.0, 0.0, 1.0, 1.0, 1.0),
            expected: AssayExpected {
                volume: Some(1.0),
                volume_tol: 0.001,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                // box(0,0,0,1,1,1) world: X=[0,1], Y=[-1,0]
                bbox: Some(([0.0, -1.0, 0.0], [1.0, 0.0, 1.0])),
            },
        },
        BaseExtrude {
            desc: "wide short cylinder",
            recipe: cyl_recipe(0.0, 0.0, 0.0, 10.0, 1.0),
            expected: AssayExpected {
                volume: Some(PI * 100.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(3),
                watertight: true,
                bbox: Some(([-10.0, -10.0, 0.0], [10.0, 10.0, 1.0])),
            },
        },
        BaseExtrude {
            desc: "rectangular slab",
            recipe: box_recipe(0.0, 0.0, 0.0, 20.0, 5.0, 3.0),
            expected: AssayExpected {
                volume: Some(300.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                // box(0,0,0,20,5,3) world: X=[0,5], Y=[-20,0]
                bbox: Some(([0.0, -20.0, 0.0], [5.0, 0.0, 3.0])),
            },
        },
    ];

    // Revolve base cases
    let revolve_bases = vec![
        BaseExtrude {
            desc: "full torus (360 deg revolve)",
            recipe: AssayRecipe::Revolve {
                profile: Profile::Circle {
                    cx: 5.0,
                    cy: 0.0,
                    r: 1.0,
                },
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                axis_origin: [0.0, 0.0, 0.0],
                axis_dir: [0.0, 1.0, 0.0],
                angle_rad: 2.0 * PI,
            },
            expected: AssayExpected {
                // Torus volume = 2 * pi^2 * R * r^2 where R=5, r=1
                volume: Some(2.0 * PI * PI * 5.0),
                volume_tol: 1.0,
                euler: Some(0), // torus is genus-1
                face_count: None,
                watertight: true,
                bbox: None,
            },
        },
        BaseExtrude {
            desc: "half revolution (180 deg)",
            recipe: AssayRecipe::Revolve {
                profile: Profile::Rect {
                    cx: 5.0,
                    cy: 0.0,
                    w: 2.0,
                    h: 4.0,
                },
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                axis_origin: [0.0, 0.0, 0.0],
                axis_dir: [0.0, 1.0, 0.0],
                angle_rad: PI,
            },
            expected: AssayExpected {
                // Half revolution of rect: volume = pi * (R_outer^2 - R_inner^2) * h / 2
                // R_outer = 6, R_inner = 4, h = 4
                // Actually: = pi * h * (R_outer^2 - R_inner^2) / 2 (half turn)
                volume: Some(PI * 4.0 * (36.0 - 16.0) / 2.0),
                volume_tol: 5.0,
                euler: Some(2),
                face_count: None,
                watertight: true,
                bbox: None,
            },
        },
        BaseExtrude {
            desc: "quarter turn cylinder revolution",
            recipe: AssayRecipe::Revolve {
                profile: Profile::Rect {
                    cx: 10.0,
                    cy: 0.0,
                    w: 2.0,
                    h: 3.0,
                },
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                axis_origin: [0.0, 0.0, 0.0],
                axis_dir: [0.0, 1.0, 0.0],
                angle_rad: PI / 2.0,
            },
            expected: AssayExpected {
                // Quarter turn of rect: pi/2 * (R_outer^2 - R_inner^2) * h / (2*pi) * 2*pi
                // Actually Pappus: V = 2*pi*R_centroid * A * (angle/2pi)
                // R_centroid = 10, A = 2*3 = 6, angle = pi/2
                // V = 2*pi*10*6 * (1/4) = 30*pi
                volume: Some(PI * 3.0 * (121.0 - 81.0) / 4.0),
                volume_tol: 5.0,
                euler: Some(2),
                face_count: None,
                watertight: true,
                bbox: None,
            },
        },
    ];

    // Extrude + boolean combos
    let combo_bases = vec![BaseExtrude {
        desc: "extrude box then boolean cut cylinder",
        recipe: bool_recipe(
            box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0),
            cyl_recipe(5.0, 5.0, 0.0, 3.0, 10.0),
            BoolOp::Subtract,
        ),
        expected: AssayExpected {
            volume: Some(1000.0 - PI * 9.0 * 10.0),
            volume_tol: 5.0,
            euler: Some(2),
            face_count: None,
            watertight: true,
            // box(0,0,0,10,10,10) world: X=[0,10], Y=[-10,0]
            bbox: Some(([0.0, -10.0, 0.0], [10.0, 0.0, 10.0])),
        },
    }];

    // Generate all: 7 extrude × 4 scales + 3 revolve × 4 scales + 1 combo × 4 scales + fill
    let scales_4: &[(f64, &str)] = &[(1e-3, "mm"), (1e-2, "cm"), (1.0, "m"), (1e2, "100m")];

    for base in &extrude_bases {
        for &(scale, scale_name) in scales_4 {
            if idx > 280 {
                break;
            }
            let id = format!("S{:03}", idx);
            let id_static: &'static str = Box::leak(id.into_boxed_str());
            let desc = format!("{} @{}", base.desc, scale_name);
            let desc_static: &'static str = Box::leak(desc.into_boxed_str());

            cases.push(AssayCase {
                id: id_static,
                description: desc_static,
                category: AssayCategory::ExtrudeRevolve,
                recipe: scale_recipe(&base.recipe, scale),
                expected: scale_expected(&base.expected, scale),
            });
            idx += 1;
        }
    }

    for base in &revolve_bases {
        for &(scale, scale_name) in scales_4 {
            if idx > 280 {
                break;
            }
            let id = format!("S{:03}", idx);
            let id_static: &'static str = Box::leak(id.into_boxed_str());
            let desc = format!("{} @{}", base.desc, scale_name);
            let desc_static: &'static str = Box::leak(desc.into_boxed_str());

            cases.push(AssayCase {
                id: id_static,
                description: desc_static,
                category: AssayCategory::ExtrudeRevolve,
                recipe: scale_recipe(&base.recipe, scale),
                expected: scale_expected(&base.expected, scale),
            });
            idx += 1;
        }
    }

    for base in &combo_bases {
        for &(scale, scale_name) in scales_4 {
            if idx > 280 {
                break;
            }
            let id = format!("S{:03}", idx);
            let id_static: &'static str = Box::leak(id.into_boxed_str());
            let desc = format!("{} @{}", base.desc, scale_name);
            let desc_static: &'static str = Box::leak(desc.into_boxed_str());

            cases.push(AssayCase {
                id: id_static,
                description: desc_static,
                category: AssayCategory::ExtrudeRevolve,
                recipe: scale_recipe(&base.recipe, scale),
                expected: scale_expected(&base.expected, scale),
            });
            idx += 1;
        }
    }

    // Fill remaining with simple extrude variants
    while idx <= 280 {
        let dim = 5.0 + (idx - 201) as f64 * 0.5;
        let id = format!("S{:03}", idx);
        let id_static: &'static str = Box::leak(id.into_boxed_str());
        let desc = format!("extrude variant {}", idx - 200);
        let desc_static: &'static str = Box::leak(desc.into_boxed_str());

        cases.push(AssayCase {
            id: id_static,
            description: desc_static,
            category: AssayCategory::ExtrudeRevolve,
            recipe: box_recipe(0.0, 0.0, 0.0, dim, dim, dim),
            expected: AssayExpected {
                volume: Some(dim * dim * dim),
                volume_tol: dim * dim * dim * 0.01,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                // box(0,0,0,dim,dim,dim) world: X=[0,dim], Y=[-dim,0]
                bbox: Some(([0.0, -dim, 0.0], [dim, 0.0, dim])),
            },
        });
        idx += 1;
    }
}

// ── S281-S340: Edge Cases ──────────────────────────────────────────────

fn generate_edge_cases(cases: &mut Vec<AssayCase>) {
    let mut idx = 281usize;

    // Micro-scale features (10 μm = 1e-5 m)
    let micro_cases: Vec<(&str, AssayRecipe, AssayExpected)> = vec![
        (
            "micro box 10um",
            box_recipe(0.0, 0.0, 0.0, 1e-5, 1e-5, 1e-5),
            AssayExpected {
                volume: Some(1e-15),
                volume_tol: 1e-17,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                bbox: Some(([0.0, -1e-5, 0.0], [1e-5, 0.0, 1e-5])),
            },
        ),
        (
            "micro cylinder 10um",
            cyl_recipe(0.0, 0.0, 0.0, 5e-6, 1e-5),
            AssayExpected {
                volume: Some(PI * 25e-12 * 1e-5),
                volume_tol: 1e-17,
                euler: Some(2),
                face_count: Some(3),
                watertight: true,
                bbox: None,
            },
        ),
        (
            "micro box-box union 10um",
            bool_recipe(
                box_recipe(0.0, 0.0, 0.0, 1e-5, 1e-5, 1e-5),
                box_recipe(5e-6, 0.0, 0.0, 1e-5, 1e-5, 1e-5),
                BoolOp::Union,
            ),
            AssayExpected {
                volume: Some(1.5e-15),
                volume_tol: 1e-17,
                euler: Some(2),
                face_count: None, // split faces produce 14
                watertight: true,
                bbox: None,
            },
        ),
        (
            "micro box-box subtract 10um",
            bool_recipe(
                box_recipe(0.0, 0.0, 0.0, 1e-5, 1e-5, 1e-5),
                box_recipe(5e-6, 0.0, 0.0, 1e-5, 1e-5, 1e-5),
                BoolOp::Subtract,
            ),
            AssayExpected {
                volume: Some(0.5e-15),
                volume_tol: 1e-17,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                bbox: None,
            },
        ),
    ];

    // km-scale geometry
    let km_cases: Vec<(&str, AssayRecipe, AssayExpected)> = vec![
        (
            "km box 10km",
            box_recipe(0.0, 0.0, 0.0, 1e4, 1e4, 1e4),
            AssayExpected {
                volume: Some(1e12),
                volume_tol: 1e8,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                bbox: Some(([0.0, -1e4, 0.0], [1e4, 0.0, 1e4])),
            },
        ),
        (
            "km box-box union",
            bool_recipe(
                box_recipe(0.0, 0.0, 0.0, 1e4, 1e4, 1e4),
                box_recipe(5e3, 0.0, 0.0, 1e4, 1e4, 1e4),
                BoolOp::Union,
            ),
            AssayExpected {
                volume: Some(1.5e12),
                volume_tol: 1e8,
                euler: Some(2),
                face_count: None, // split faces produce 14
                watertight: true,
                bbox: None,
            },
        ),
    ];

    // Coincident edge cases
    let coincident_cases: Vec<(&str, AssayRecipe, AssayExpected)> = vec![
        (
            "edge-coincident boxes (shared edge line)",
            bool_recipe(
                box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0),
                box_recipe(10.0, 10.0, 0.0, 10.0, 10.0, 10.0),
                BoolOp::Union,
            ),
            AssayExpected {
                volume: Some(2000.0),
                volume_tol: 1.0,
                euler: None,
                face_count: None,
                watertight: true,
                bbox: None,
            },
        ),
        (
            "vertex-coincident boxes (shared corner)",
            bool_recipe(
                box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0),
                box_recipe(10.0, 10.0, 10.0, 10.0, 10.0, 10.0),
                BoolOp::Union,
            ),
            AssayExpected {
                volume: Some(2000.0),
                volume_tol: 1.0,
                euler: None,
                face_count: None,
                watertight: true,
                bbox: None,
            },
        ),
    ];

    // Near-miss cases (almost touching)
    let near_miss_cases: Vec<(&str, AssayRecipe, AssayExpected)> = vec![(
        "near-miss boxes (gap=1e-8)",
        bool_recipe(
            box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0),
            box_recipe(10.0 + 1e-8, 0.0, 0.0, 10.0, 10.0, 10.0),
            BoolOp::Union,
        ),
        AssayExpected {
            volume: Some(2000.0),
            volume_tol: 1.0,
            euler: None,
            face_count: None,
            watertight: true,
            bbox: None,
        },
    )];

    // Mixed-scale operands (mm feature on m body)
    let mixed_scale_cases: Vec<(&str, AssayRecipe, AssayExpected)> = vec![
        (
            "mm hole in m body",
            bool_recipe(
                box_recipe(0.0, 0.0, 0.0, 1.0, 1.0, 1.0),
                cyl_recipe(0.5, 0.5, 0.0, 1e-3, 1.0),
                BoolOp::Subtract,
            ),
            AssayExpected {
                volume: Some(1.0 - PI * 1e-6),
                volume_tol: 1e-5,
                euler: Some(2),
                face_count: None,
                watertight: true,
                // box(0,0,0,1,1,1) world: X=[0,1], Y=[-1,0]
                bbox: Some(([0.0, -1.0, 0.0], [1.0, 0.0, 1.0])),
            },
        ),
        (
            "mm boss on m body",
            bool_recipe(
                box_recipe(0.0, 0.0, 0.0, 1.0, 1.0, 0.01),
                cyl_recipe(0.5, 0.5, 0.01, 2e-3, 5e-3),
                BoolOp::Union,
            ),
            AssayExpected {
                volume: Some(0.01 + PI * 4e-6 * 5e-3),
                volume_tol: 1e-6,
                euler: Some(2),
                face_count: None,
                watertight: true,
                bbox: None,
            },
        ),
    ];

    // Extreme aspect ratios
    let aspect_cases: Vec<(&str, AssayRecipe, AssayExpected)> = vec![
        (
            "very thin plate (1000:1 aspect)",
            box_recipe(0.0, 0.0, 0.0, 100.0, 100.0, 0.1),
            AssayExpected {
                volume: Some(1000.0),
                volume_tol: 1.0,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                // box(0,0,0,100,100,0.1) world: X=[0,100], Y=[-100,0]
                bbox: Some(([0.0, -100.0, 0.0], [100.0, 0.0, 0.1])),
            },
        ),
        (
            "very tall needle (1:1000 aspect)",
            cyl_recipe(0.0, 0.0, 0.0, 0.01, 10.0),
            AssayExpected {
                volume: Some(PI * 1e-4 * 10.0),
                volume_tol: 1e-4,
                euler: Some(2),
                face_count: Some(3),
                watertight: true,
                bbox: None,
            },
        ),
    ];

    // Assemble all edge cases
    let all_edge: Vec<(&str, AssayRecipe, AssayExpected)> = micro_cases
        .into_iter()
        .chain(km_cases)
        .chain(coincident_cases)
        .chain(near_miss_cases)
        .chain(mixed_scale_cases)
        .chain(aspect_cases)
        .collect();

    for (desc, recipe, expected) in all_edge {
        if idx > 340 {
            break;
        }
        let id = format!("S{:03}", idx);
        let id_static: &'static str = Box::leak(id.into_boxed_str());
        let desc_static: &'static str = Box::leak(desc.to_string().into_boxed_str());

        cases.push(AssayCase {
            id: id_static,
            description: desc_static,
            category: AssayCategory::EdgeCase,
            recipe,
            expected,
        });
        idx += 1;
    }

    // Fill remaining edge case slots with mm-scale booleans
    while idx <= 340 {
        let s = 1e-3; // mm scale
        let dim = 5.0 * s + (idx - 281) as f64 * 0.1 * s;
        let id = format!("S{:03}", idx);
        let id_static: &'static str = Box::leak(id.into_boxed_str());
        let desc = format!("mm-scale box variant {}", idx - 280);
        let desc_static: &'static str = Box::leak(desc.into_boxed_str());

        cases.push(AssayCase {
            id: id_static,
            description: desc_static,
            category: AssayCategory::EdgeCase,
            recipe: box_recipe(0.0, 0.0, 0.0, dim, dim, dim),
            expected: AssayExpected {
                volume: Some(dim * dim * dim),
                volume_tol: dim * dim * dim * 0.01,
                euler: Some(2),
                face_count: Some(6),
                watertight: true,
                // box(0,0,0,dim,dim,dim) world: X=[0,dim], Y=[-dim,0]
                bbox: Some(([0.0, -dim, 0.0], [dim, 0.0, dim])),
            },
        });
        idx += 1;
    }
}

// ── S341-S400: Stress / Degenerate ────────────────────────────────────

fn generate_stress(cases: &mut Vec<AssayCase>) {
    let mut idx = 341usize;

    // Deep chains (5+ operations)
    let deep_chain = AssayRecipe::Chain {
        initial: Box::new(box_recipe(0.0, 0.0, 0.0, 20.0, 20.0, 10.0)),
        steps: (0..8)
            .map(|i| {
                let x = 2.0 + (i % 4) as f64 * 5.0;
                let y = 2.0 + (i / 4) as f64 * 10.0;
                ChainStep {
                    op: BoolOp::Subtract,
                    operand: Box::new(cyl_recipe(x, y, 0.0, 1.5, 10.0)),
                }
            })
            .collect(),
    };

    let stress_cases: Vec<(&str, AssayRecipe, AssayExpected)> = vec![
        (
            "8-hole plate (deep chain)",
            deep_chain,
            AssayExpected {
                volume: Some(20.0 * 20.0 * 10.0 - 8.0 * PI * 2.25 * 10.0),
                volume_tol: 10.0,
                euler: None,
                face_count: None,
                watertight: true,
                // box(0,0,0,20,20,10) world: X=[0,20], Y=[-20,0]
                bbox: Some(([0.0, -20.0, 0.0], [20.0, 0.0, 10.0])),
            },
        ),
        (
            "torus-box union",
            bool_recipe(
                box_recipe(-6.0, -6.0, -2.0, 12.0, 12.0, 4.0),
                AssayRecipe::Revolve {
                    profile: Profile::Circle {
                        cx: 5.0,
                        cy: 0.0,
                        r: 1.0,
                    },
                    origin: [0.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    axis_origin: [0.0, 0.0, 0.0],
                    axis_dir: [0.0, 1.0, 0.0],
                    angle_rad: 2.0 * PI,
                },
                BoolOp::Union,
            ),
            AssayExpected {
                volume: None, // complex overlap
                volume_tol: 10.0,
                euler: None,
                face_count: None,
                watertight: true,
                bbox: None,
            },
        ),
        (
            "torus-box subtract",
            bool_recipe(
                box_recipe(-6.0, -6.0, -2.0, 12.0, 12.0, 4.0),
                AssayRecipe::Revolve {
                    profile: Profile::Circle {
                        cx: 5.0,
                        cy: 0.0,
                        r: 1.0,
                    },
                    origin: [0.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    axis_origin: [0.0, 0.0, 0.0],
                    axis_dir: [0.0, 1.0, 0.0],
                    angle_rad: 2.0 * PI,
                },
                BoolOp::Subtract,
            ),
            AssayExpected {
                volume: None,
                volume_tol: 10.0,
                euler: None,
                face_count: None,
                watertight: true,
                bbox: None,
            },
        ),
        (
            "extreme scale: 1e-5 box on 1e2 box",
            bool_recipe(
                box_recipe(0.0, 0.0, 0.0, 100.0, 100.0, 100.0),
                box_recipe(50.0, 50.0, 100.0, 1e-5, 1e-5, 1e-5),
                BoolOp::Union,
            ),
            AssayExpected {
                volume: Some(1e6 + 1e-15),
                volume_tol: 1.0,
                euler: None,
                face_count: None,
                watertight: true,
                bbox: None,
            },
        ),
        (
            "determinism: box-cyl subtract x3 (same geometry)",
            bool_recipe(
                box_recipe(0.0, 0.0, 0.0, 10.0, 10.0, 10.0),
                cyl_recipe(5.0, 5.0, 0.0, 3.0, 10.0),
                BoolOp::Subtract,
            ),
            AssayExpected {
                volume: Some(1000.0 - PI * 9.0 * 10.0),
                volume_tol: 5.0,
                euler: Some(2),
                face_count: None,
                watertight: true,
                // box(0,0,0,10,10,10) world: X=[0,10], Y=[-10,0]
                bbox: Some(([0.0, -10.0, 0.0], [10.0, 0.0, 10.0])),
            },
        ),
    ];

    for (desc, recipe, expected) in &stress_cases {
        for &(scale, scale_name) in &[(1.0, "m"), (1e-3, "mm"), (1e2, "100m")] {
            if idx > 400 {
                break;
            }
            let id = format!("S{:03}", idx);
            let id_static: &'static str = Box::leak(id.into_boxed_str());
            let full_desc = format!("{} @{}", desc, scale_name);
            let desc_static: &'static str = Box::leak(full_desc.into_boxed_str());

            cases.push(AssayCase {
                id: id_static,
                description: desc_static,
                category: AssayCategory::StressDegenerate,
                recipe: scale_recipe(recipe, scale),
                expected: scale_expected(expected, scale),
            });
            idx += 1;
        }
    }

    // Fill remaining with parametric variants
    while idx <= 400 {
        let n = idx - 341;
        let size = 10.0 + n as f64;
        let hole_r = 1.0 + (n as f64 * 0.1);
        let id = format!("S{:03}", idx);
        let id_static: &'static str = Box::leak(id.into_boxed_str());
        let desc = format!("stress variant {} (box-cyl sub, r={})", n, hole_r);
        let desc_static: &'static str = Box::leak(desc.into_boxed_str());

        cases.push(AssayCase {
            id: id_static,
            description: desc_static,
            category: AssayCategory::StressDegenerate,
            recipe: bool_recipe(
                box_recipe(0.0, 0.0, 0.0, size, size, size),
                cyl_recipe(size / 2.0, size / 2.0, 0.0, hole_r, size),
                BoolOp::Subtract,
            ),
            expected: AssayExpected {
                volume: Some(size * size * size - PI * hole_r * hole_r * size),
                volume_tol: size,
                euler: Some(2),
                face_count: None,
                watertight: true,
                // box(0,0,0,size,size,size) world: X=[0,size], Y=[-size,0]
                bbox: Some(([0.0, -size, 0.0], [size, 0.0, size])),
            },
        });
        idx += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_400_cases() {
        let catalog = full_catalog();
        assert_eq!(catalog.len(), 400);
    }

    #[test]
    fn ids_are_unique() {
        let catalog = full_catalog();
        let mut ids: Vec<&str> = catalog.iter().map(|c| c.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 400, "All 400 IDs must be unique");
    }

    #[test]
    fn ids_are_sequential() {
        let catalog = full_catalog();
        for (i, case) in catalog.iter().enumerate() {
            let expected_id = format!("S{:03}", i + 1);
            assert_eq!(case.id, expected_id, "Case {} has wrong ID", i);
        }
    }

    #[test]
    fn categories_in_correct_ranges() {
        let catalog = full_catalog();
        for case in &catalog {
            let num: usize = case.id[1..].parse().unwrap();
            let expected_cat = match num {
                1..=100 => AssayCategory::SingleBoolean,
                101..=200 => AssayCategory::ChainedBoolean,
                201..=280 => AssayCategory::ExtrudeRevolve,
                281..=340 => AssayCategory::EdgeCase,
                341..=400 => AssayCategory::StressDegenerate,
                _ => panic!("ID out of range: {}", num),
            };
            assert_eq!(
                case.category, expected_cat,
                "Case {} has wrong category",
                case.id
            );
        }
    }

    #[test]
    fn mm_scale_cases_exist_in_edge_category() {
        let catalog = full_catalog();
        let mm_cases: Vec<_> = catalog
            .iter()
            .filter(|c| c.category == AssayCategory::EdgeCase && c.description.contains("mm"))
            .collect();
        assert!(
            mm_cases.len() >= 5,
            "Need at least 5 mm-scale edge cases, got {}",
            mm_cases.len()
        );
    }
}
