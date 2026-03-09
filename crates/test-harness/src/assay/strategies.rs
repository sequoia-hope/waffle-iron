//! Proptest strategies for generating CAD scenarios at multiple levels.
//!
//! Strategies are composable: higher levels build on lower levels.

use std::fmt;

/// Level 1: Rectangular sketch profile parameters.
#[derive(Debug, Clone)]
pub struct RectProfile {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Level 1: Circular sketch profile parameters.
#[derive(Debug, Clone)]
pub struct CircleProfile {
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
}

/// Level 1: A sketch profile (either rect or circle).
#[derive(Debug, Clone)]
pub enum SketchProfile {
    Rect(RectProfile),
    Circle(CircleProfile),
}

/// Level 2: A complete extruded solid body specification.
#[derive(Debug, Clone)]
pub struct SolidBodySpec {
    pub profile: SketchProfile,
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    pub depth: f64,
}

/// Boolean operation type (mirrors feature_engine::types::BooleanOp).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
}

impl fmt::Display for BoolOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoolOp::Union => write!(f, "Union"),
            BoolOp::Subtract => write!(f, "Subtract"),
            BoolOp::Intersect => write!(f, "Intersect"),
        }
    }
}

/// Level 3: A boolean scenario with two bodies and an operation.
#[derive(Debug, Clone)]
pub struct BooleanScenario {
    pub body_a: SolidBodySpec,
    pub body_b: SolidBodySpec,
    pub op: BoolOp,
}

/// Degeneracy family for controlled degenerate scenarios.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DegeneracyFamily {
    /// Faces of both bodies lie in the same plane.
    CoplanarFaces,
    /// An edge of one body lies on an edge of the other.
    CoincidentEdge,
    /// A vertex of one body lies on a face of the other.
    VertexOnFace,
    /// Bodies touch tangentially (share a tangent plane at contact).
    Tangential,
}

impl fmt::Display for DegeneracyFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DegeneracyFamily::CoplanarFaces => write!(f, "CoplanarFaces"),
            DegeneracyFamily::CoincidentEdge => write!(f, "CoincidentEdge"),
            DegeneracyFamily::VertexOnFace => write!(f, "VertexOnFace"),
            DegeneracyFamily::Tangential => write!(f, "Tangential"),
        }
    }
}

/// Level 4: A degenerate boolean scenario.
#[derive(Debug, Clone)]
pub struct DegenerateScenario {
    pub scenario: BooleanScenario,
    pub family: DegeneracyFamily,
}

/// Level 5: A chain of boolean operations.
#[derive(Debug, Clone)]
pub struct BooleanChain {
    pub initial: SolidBodySpec,
    pub steps: Vec<(SolidBodySpec, BoolOp)>,
}

// ── Strategy functions ────────────────────────────────────────────────

/// Proptest strategies for generating CAD scenarios.
pub mod strats {
    use super::*;
    use proptest::prelude::*;

    /// Level 0: Generate a dimension in a reasonable CAD range.
    pub fn dim_range() -> impl Strategy<Value = f64> {
        0.5f64..50.0
    }

    /// Level 0: Generate a small offset for positioning.
    pub fn offset_range() -> impl Strategy<Value = f64> {
        -25.0f64..25.0
    }

    prop_compose! {
        /// Level 1: Generate a rectangular profile.
        pub fn rect_profile_strategy()
            (x in offset_range(), y in offset_range(),
             w in dim_range(), h in dim_range())
        -> RectProfile {
            RectProfile { x, y, w, h }
        }
    }

    prop_compose! {
        /// Level 1: Generate a circular profile.
        pub fn circle_profile_strategy()
            (cx in offset_range(), cy in offset_range(), r in dim_range())
        -> CircleProfile {
            CircleProfile { cx, cy, r }
        }
    }

    /// Level 1: Generate any sketch profile.
    pub fn sketch_profile() -> impl Strategy<Value = SketchProfile> {
        prop_oneof![
            rect_profile_strategy().prop_map(SketchProfile::Rect),
            circle_profile_strategy().prop_map(SketchProfile::Circle),
        ]
    }

    prop_compose! {
        /// Level 2: Generate a solid body spec with XY plane at origin.
        pub fn solid_body_xy()
            (profile in sketch_profile(), depth in dim_range())
        -> SolidBodySpec {
            SolidBodySpec {
                profile,
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                depth,
            }
        }
    }

    prop_compose! {
        /// Level 2: Generate a solid body spec with offset origin on XY plane.
        pub fn solid_body_offset()
            (profile in sketch_profile(), depth in dim_range(),
             ox in offset_range(), oy in offset_range(), oz in offset_range())
        -> SolidBodySpec {
            SolidBodySpec {
                profile,
                origin: [ox, oy, oz],
                normal: [0.0, 0.0, 1.0],
                depth,
            }
        }
    }

    /// Generate a boolean operation.
    pub fn bool_op() -> impl Strategy<Value = BoolOp> {
        prop_oneof![
            Just(BoolOp::Union),
            Just(BoolOp::Subtract),
            Just(BoolOp::Intersect),
        ]
    }

    prop_compose! {
        /// Level 3: Generate a boolean scenario (union).
        pub fn boolean_scenario_union()
            (a in solid_body_xy(), b in solid_body_offset())
        -> BooleanScenario {
            BooleanScenario { body_a: a, body_b: b, op: BoolOp::Union }
        }
    }

    prop_compose! {
        /// Level 3: Generate a boolean scenario (subtract).
        pub fn boolean_scenario_subtract()
            (a in solid_body_xy(), b in solid_body_offset())
        -> BooleanScenario {
            BooleanScenario { body_a: a, body_b: b, op: BoolOp::Subtract }
        }
    }

    prop_compose! {
        /// Level 3: Generate a boolean scenario (intersect).
        pub fn boolean_scenario_intersect()
            (a in solid_body_xy(), b in solid_body_offset())
        -> BooleanScenario {
            BooleanScenario { body_a: a, body_b: b, op: BoolOp::Intersect }
        }
    }

    prop_compose! {
        /// Level 3: Generate a boolean scenario with any operation.
        pub fn boolean_scenario_any()
            (a in solid_body_xy(), b in solid_body_offset(), op in bool_op())
        -> BooleanScenario {
            BooleanScenario { body_a: a, body_b: b, op }
        }
    }

    // ── Rect-only variants (for box-box tests that require WaffleKernel) ─

    prop_compose! {
        /// Level 2: Solid body with rect-only profile at XY origin.
        pub fn solid_body_xy_rect()
            (profile in rect_profile_strategy().prop_map(SketchProfile::Rect),
             depth in dim_range())
        -> SolidBodySpec {
            SolidBodySpec {
                profile,
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                depth,
            }
        }
    }

    prop_compose! {
        /// Level 2: Solid body with rect-only profile at offset origin.
        pub fn solid_body_offset_rect()
            (profile in rect_profile_strategy().prop_map(SketchProfile::Rect),
             depth in dim_range(),
             ox in offset_range(), oy in offset_range(), oz in offset_range())
        -> SolidBodySpec {
            SolidBodySpec {
                profile,
                origin: [ox, oy, oz],
                normal: [0.0, 0.0, 1.0],
                depth,
            }
        }
    }

    prop_compose! {
        /// Level 3: Rect-only boolean scenario (union).
        pub fn boolean_scenario_union_rect()
            (a in solid_body_xy_rect(), b in solid_body_offset_rect())
        -> BooleanScenario {
            BooleanScenario { body_a: a, body_b: b, op: BoolOp::Union }
        }
    }

    prop_compose! {
        /// Level 3: Rect-only boolean scenario (subtract).
        pub fn boolean_scenario_subtract_rect()
            (a in solid_body_xy_rect(), b in solid_body_offset_rect())
        -> BooleanScenario {
            BooleanScenario { body_a: a, body_b: b, op: BoolOp::Subtract }
        }
    }

    prop_compose! {
        /// Level 3: Rect-only boolean scenario (intersect).
        pub fn boolean_scenario_intersect_rect()
            (a in solid_body_xy_rect(), b in solid_body_offset_rect())
        -> BooleanScenario {
            BooleanScenario { body_a: a, body_b: b, op: BoolOp::Intersect }
        }
    }

    prop_compose! {
        /// Level 4: Coplanar — body_b shares the same Z=0 base plane.
        pub fn coplanar_scenario()
            (a in solid_body_xy(), mut b in solid_body_xy(), op in bool_op())
        -> DegenerateScenario {
            // Force b to same origin Z so bottom faces are coplanar
            b.origin = [0.0, 0.0, 0.0];
            DegenerateScenario {
                scenario: BooleanScenario { body_a: a, body_b: b, op },
                family: DegeneracyFamily::CoplanarFaces,
            }
        }
    }

    prop_compose! {
        /// Level 4: Coincident edge — body_b offset to share an edge with body_a.
        pub fn coincident_edge_scenario()
            (a_rect in rect_profile_strategy(), depth_a in dim_range(),
             b_rect in rect_profile_strategy(), depth_b in dim_range(),
             op in bool_op())
        -> DegenerateScenario {
            let a = SolidBodySpec {
                profile: SketchProfile::Rect(a_rect.clone()),
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                depth: depth_a,
            };
            // Place body_b so its left edge coincides with body_a's right edge
            let b = SolidBodySpec {
                profile: SketchProfile::Rect(RectProfile {
                    x: a_rect.x + a_rect.w, // abutting
                    y: b_rect.y,
                    w: b_rect.w,
                    h: b_rect.h,
                }),
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                depth: depth_b,
            };
            DegenerateScenario {
                scenario: BooleanScenario { body_a: a, body_b: b, op },
                family: DegeneracyFamily::CoincidentEdge,
            }
        }
    }

    prop_compose! {
        /// Level 4: Vertex-on-face — body_b's corner touches body_a's face.
        pub fn vertex_on_face_scenario()
            (a_rect in rect_profile_strategy(), depth_a in dim_range(),
             b_rect in rect_profile_strategy(), depth_b in dim_range(),
             op in bool_op())
        -> DegenerateScenario {
            let a = SolidBodySpec {
                profile: SketchProfile::Rect(a_rect.clone()),
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                depth: depth_a,
            };
            // Place body_b corner at the center of body_a's top face
            let mid_x = a_rect.x + a_rect.w / 2.0;
            let mid_y = a_rect.y + a_rect.h / 2.0;
            let b = SolidBodySpec {
                profile: SketchProfile::Rect(RectProfile {
                    x: 0.0,
                    y: 0.0,
                    w: b_rect.w,
                    h: b_rect.h,
                }),
                origin: [mid_x, mid_y, depth_a], // sits on top face
                normal: [0.0, 0.0, 1.0],
                depth: depth_b,
            };
            DegenerateScenario {
                scenario: BooleanScenario { body_a: a, body_b: b, op },
                family: DegeneracyFamily::VertexOnFace,
            }
        }
    }

    /// Level 4: Generate any degenerate scenario.
    pub fn degenerate_scenario() -> impl Strategy<Value = DegenerateScenario> {
        prop_oneof![
            coplanar_scenario(),
            coincident_edge_scenario(),
            vertex_on_face_scenario(),
        ]
    }

    prop_compose! {
        /// Level 5: Generate a chain of 2-5 boolean operations.
        pub fn boolean_chain()
            (initial in solid_body_xy(),
             steps in proptest::collection::vec(
                 (solid_body_offset(), bool_op()), 2..=5
             ))
        -> BooleanChain {
            BooleanChain { initial, steps }
        }
    }
}
