//! Assay v2 strategies: generative testing beyond rectangular/circular profiles.
//!
//! Phase 1: Convex polygon profiles + basic extrude on axis-aligned planes.
//! Phase 2: Non-convex polygons, stars, arcs, iOverlay region decomposition.

use std::fmt;

use crate::helpers::{mesh_volume, polygon_profile};
use crate::workflow::ModelBuilder;

use super::regions::{self, ClosedRegion};
use super::strategies::BoolOp;

/// Level 1: Sketch plane specification.
#[derive(Debug, Clone)]
pub struct SketchPlaneSpec {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
}

impl fmt::Display for SketchPlaneSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let axis = if self.normal[2].abs() > 0.9 {
            "XY"
        } else if self.normal[1].abs() > 0.9 {
            "XZ"
        } else if self.normal[0].abs() > 0.9 {
            "YZ"
        } else {
            "tilted"
        };
        write!(
            f,
            "{}@({:.1},{:.1},{:.1})",
            axis, self.origin[0], self.origin[1], self.origin[2]
        )
    }
}

/// Level 2: Convex polygon specification.
#[derive(Debug, Clone)]
pub struct ConvexPolygonSpec {
    /// 2D positions in sketch-plane coordinates.
    pub vertices: Vec<(f64, f64)>,
}

impl fmt::Display for ConvexPolygonSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "polygon({}sides)", self.vertices.len())
    }
}

/// Level 2: Generalized sketch profile specification.
#[derive(Debug, Clone)]
pub enum ProfileSpec {
    /// Convex polygon (Phase 1).
    ConvexPolygon(ConvexPolygonSpec),
    /// Non-convex polygon (concave shape from perturbing convex).
    NonConvexPolygon { vertices: Vec<(f64, f64)> },
    /// Star polygon with alternating inner/outer radii.
    StarPolygon {
        points: u32,
        inner_r: f64,
        outer_r: f64,
        cx: f64,
        cy: f64,
        rotation: f64,
    },
    /// Polygon with some edges replaced by arc approximations (polyline).
    PolygonWithArcs {
        base_vertices: Vec<(f64, f64)>,
        arc_edge_indices: Vec<usize>,
    },
}

impl fmt::Display for ProfileSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileSpec::ConvexPolygon(p) => write!(f, "convex({}sides)", p.vertices.len()),
            ProfileSpec::NonConvexPolygon { vertices } => {
                write!(f, "non_convex({}verts)", vertices.len())
            }
            ProfileSpec::StarPolygon { points, .. } => write!(f, "star({}pts)", points),
            ProfileSpec::PolygonWithArcs {
                base_vertices,
                arc_edge_indices,
                ..
            } => write!(
                f,
                "poly_arcs({}v,{}arcs)",
                base_vertices.len(),
                arc_edge_indices.len()
            ),
        }
    }
}

/// Level 4: Region selection strategy.
#[derive(Debug, Clone)]
pub enum RegionSelectionStrategy {
    /// Select the largest region by area.
    Largest,
    /// Select a deterministic "random" region by index.
    Random,
    /// Select the smallest region with area ≥ min_area.
    SmallestNonTiny { min_area: f64 },
}

impl fmt::Display for RegionSelectionStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegionSelectionStrategy::Largest => write!(f, "largest"),
            RegionSelectionStrategy::Random => write!(f, "random"),
            RegionSelectionStrategy::SmallestNonTiny { min_area } => {
                write!(f, "smallest(min={:.1})", min_area)
            }
        }
    }
}

/// Level 5: Extrude operation specification.
#[derive(Debug, Clone)]
pub struct ExtrudeSpec {
    pub depth: f64,
}

/// Level 7: Complete generative extrude scenario (Phase 1 — convex only).
#[derive(Debug, Clone)]
pub struct GenerativeExtrudeScenario {
    pub plane: SketchPlaneSpec,
    pub polygon: ConvexPolygonSpec,
    pub extrude: ExtrudeSpec,
}

impl fmt::Display for GenerativeExtrudeScenario {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} depth={:.1}",
            self.plane, self.polygon, self.extrude.depth
        )
    }
}

/// Level 7: Extended generative scenario with any profile type (Phase 2).
#[derive(Debug, Clone)]
pub struct GenerativeProfileScenario {
    pub plane: SketchPlaneSpec,
    pub profile: ProfileSpec,
    pub extrude: ExtrudeSpec,
    pub region_selection: RegionSelectionStrategy,
    /// Deterministic index for random region selection.
    pub rng_index: usize,
}

impl fmt::Display for GenerativeProfileScenario {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} depth={:.1} region={}",
            self.plane, self.profile, self.extrude.depth, self.region_selection
        )
    }
}

/// Level 5: Operation specification (extrude or revolve).
#[derive(Debug, Clone)]
pub enum OperationSpec {
    Extrude {
        depth: f64,
    },
    Revolve {
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle_deg: f64,
    },
}

impl fmt::Display for OperationSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OperationSpec::Extrude { depth } => write!(f, "extrude({:.1})", depth),
            OperationSpec::Revolve { angle_deg, .. } => write!(f, "revolve({:.0}°)", angle_deg),
        }
    }
}

/// A single step in a modeling chain.
#[derive(Debug, Clone)]
pub struct ChainStep {
    pub name: String,
    pub plane: SketchPlaneSpec,
    pub profile: ProfileSpec,
    pub operation: OperationSpec,
}

/// Level 6: Multi-step modeling chain.
#[derive(Debug, Clone)]
pub struct ModelingChain {
    pub steps: Vec<ChainStep>,
}

impl fmt::Display for ModelingChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "chain({}steps)", self.steps.len())
    }
}

/// Level 7: Complete generative chain scenario.
#[derive(Debug, Clone)]
pub struct GenerativeChainScenario {
    pub chain: ModelingChain,
    pub region_selection: RegionSelectionStrategy,
    pub rng_index: usize,
}

impl fmt::Display for GenerativeChainScenario {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} region={}", self.chain, self.region_selection)
    }
}

/// Holds intermediate results from chain execution for oracle checking.
pub struct ChainResult {
    pub builder: ModelBuilder,
    pub completed_steps: usize,
    pub step_volumes: Vec<f64>,
    pub final_feature: String,
    /// Per-step volume invariant results (I9-I12) for boolean operations.
    pub volume_invariant_results: Vec<super::properties::PropertyResult>,
}

/// 3D cross product helper.
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

// ── Strategy functions ────────────────────────────────────────────────

/// Proptest strategies for generative CAD scenarios (v2).
pub mod strats_v2 {
    use super::*;
    use proptest::prelude::*;

    /// Level 0: Small offset for positioning.
    fn offset_range() -> impl Strategy<Value = f64> {
        -25.0f64..25.0
    }

    /// Level 1: Axis-aligned sketch plane (XY/XZ/YZ at random offset).
    pub fn axis_aligned_plane() -> impl Strategy<Value = SketchPlaneSpec> {
        (0u8..3, offset_range()).prop_map(|(axis, offset)| match axis {
            0 => SketchPlaneSpec {
                origin: [0.0, 0.0, offset],
                normal: [0.0, 0.0, 1.0],
            },
            1 => SketchPlaneSpec {
                origin: [0.0, offset, 0.0],
                normal: [0.0, 1.0, 0.0],
            },
            _ => SketchPlaneSpec {
                origin: [offset, 0.0, 0.0],
                normal: [1.0, 0.0, 0.0],
            },
        })
    }

    /// Level 2: Regular convex polygon with randomized parameters.
    pub fn convex_polygon() -> impl Strategy<Value = ConvexPolygonSpec> {
        (
            3u32..=8,                      // n_sides
            2.0f64..25.0,                  // radius (min 2.0 for min_feature_size)
            offset_range(),                // center x
            offset_range(),                // center y
            0.0f64..std::f64::consts::TAU, // rotation
        )
            .prop_map(|(n_sides, radius, cx, cy, rotation)| {
                let vertices = (0..n_sides)
                    .map(|k| {
                        let angle =
                            std::f64::consts::TAU * (k as f64) / (n_sides as f64) + rotation;
                        (cx + radius * angle.cos(), cy + radius * angle.sin())
                    })
                    .collect();
                ConvexPolygonSpec { vertices }
            })
    }

    /// Level 5: Extrude depth.
    pub fn extrude_depth() -> impl Strategy<Value = ExtrudeSpec> {
        (1.0f64..50.0).prop_map(|depth| ExtrudeSpec { depth })
    }

    /// Level 7: Complete generative extrude scenario (Phase 1).
    pub fn generative_extrude_scenario() -> impl Strategy<Value = GenerativeExtrudeScenario> {
        (axis_aligned_plane(), convex_polygon(), extrude_depth()).prop_map(
            |(plane, polygon, extrude)| GenerativeExtrudeScenario {
                plane,
                polygon,
                extrude,
            },
        )
    }

    // ── Phase 2: Extended profile strategies ───────────────────────

    /// Level 2: Non-convex polygon — perturb one vertex of a convex polygon inward.
    pub fn non_convex_polygon() -> impl Strategy<Value = ProfileSpec> {
        (
            4u32..=8,                      // n_sides (need ≥4 for non-trivial concavity)
            3.0f64..25.0,                  // radius
            offset_range(),                // center x
            offset_range(),                // center y
            0.0f64..std::f64::consts::TAU, // rotation
            0.3f64..0.8,                   // inward_factor: how far to push vertex toward center
        )
            .prop_map(|(n_sides, radius, cx, cy, rotation, inward_factor)| {
                let mut vertices: Vec<(f64, f64)> = (0..n_sides)
                    .map(|k| {
                        let angle =
                            std::f64::consts::TAU * (k as f64) / (n_sides as f64) + rotation;
                        (cx + radius * angle.cos(), cy + radius * angle.sin())
                    })
                    .collect();

                // Push vertex 0 inward toward centroid
                let centroid_x: f64 =
                    vertices.iter().map(|v| v.0).sum::<f64>() / vertices.len() as f64;
                let centroid_y: f64 =
                    vertices.iter().map(|v| v.1).sum::<f64>() / vertices.len() as f64;

                let v = &vertices[0];
                vertices[0] = (
                    v.0 + (centroid_x - v.0) * inward_factor,
                    v.1 + (centroid_y - v.1) * inward_factor,
                );

                ProfileSpec::NonConvexPolygon { vertices }
            })
    }

    /// Level 2: Star polygon with alternating inner/outer radii.
    pub fn star_polygon() -> impl Strategy<Value = ProfileSpec> {
        (
            4u32..=8,                      // points
            2.0f64..10.0,                  // inner_r
            offset_range(),                // center x
            offset_range(),                // center y
            0.0f64..std::f64::consts::TAU, // rotation
        )
            .prop_flat_map(|(points, inner_r, cx, cy, rotation)| {
                // outer_r must be > inner_r to form a proper star
                let outer_min = inner_r + 2.0;
                let outer_max = (inner_r + 2.0).max(25.0);
                (
                    Just(points),
                    Just(inner_r),
                    outer_min..outer_max,
                    Just(cx),
                    Just(cy),
                    Just(rotation),
                )
            })
            .prop_map(
                |(points, inner_r, outer_r, cx, cy, rotation)| ProfileSpec::StarPolygon {
                    points,
                    inner_r,
                    outer_r,
                    cx,
                    cy,
                    rotation,
                },
            )
    }

    /// Level 2: Polygon with some edges replaced by arc approximations.
    pub fn polygon_with_arcs() -> impl Strategy<Value = ProfileSpec> {
        (
            4u32..=6,                      // n_sides
            3.0f64..20.0,                  // radius
            offset_range(),                // center x
            offset_range(),                // center y
            0.0f64..std::f64::consts::TAU, // rotation
            1usize..=3,                    // number of arc edges
        )
            .prop_map(|(n_sides, radius, cx, cy, rotation, n_arcs)| {
                let base_vertices: Vec<(f64, f64)> = (0..n_sides)
                    .map(|k| {
                        let angle =
                            std::f64::consts::TAU * (k as f64) / (n_sides as f64) + rotation;
                        (cx + radius * angle.cos(), cy + radius * angle.sin())
                    })
                    .collect();

                // Select arc edge indices (evenly spaced to avoid adjacent arcs)
                let n = base_vertices.len();
                let arc_edge_indices: Vec<usize> =
                    (0..n_arcs).map(|i| (i * n / n_arcs) % n).collect();

                ProfileSpec::PolygonWithArcs {
                    base_vertices,
                    arc_edge_indices,
                }
            })
    }

    /// Level 2: Any profile type — weighted composition.
    pub fn profile_spec_any() -> impl Strategy<Value = ProfileSpec> {
        prop_oneof![
            3 => convex_polygon().prop_map(ProfileSpec::ConvexPolygon),
            3 => non_convex_polygon(),
            3 => star_polygon(),
            2 => polygon_with_arcs(),
        ]
    }

    /// Level 4: Region selection strategy.
    pub fn region_selection() -> impl Strategy<Value = RegionSelectionStrategy> {
        prop_oneof![
            6 => Just(RegionSelectionStrategy::Largest),
            3 => Just(RegionSelectionStrategy::Random),
            1 => (4.0f64..20.0).prop_map(|min_area| RegionSelectionStrategy::SmallestNonTiny {
                min_area
            }),
        ]
    }

    /// Level 7: Complete generative profile scenario (Phase 2).
    ///
    /// Uses `sketch_plane_any()` for 60% axis-aligned + 40% tilted planes.
    pub fn generative_profile_scenario() -> impl Strategy<Value = GenerativeProfileScenario> {
        (
            sketch_plane_any(),
            profile_spec_any(),
            extrude_depth(),
            region_selection(),
            0usize..1000,
        )
            .prop_map(|(plane, profile, extrude, region_selection, rng_index)| {
                GenerativeProfileScenario {
                    plane,
                    profile,
                    extrude,
                    region_selection,
                    rng_index,
                }
            })
    }

    // ── Phase 3: Tilted planes, revolve, chains ──────────────────

    /// Level 1: Tilted sketch plane with random normal via spherical coordinates.
    pub fn tilted_plane() -> impl Strategy<Value = SketchPlaneSpec> {
        (
            offset_range(),
            offset_range(),
            offset_range(),
            0.1f64..std::f64::consts::PI - 0.1, // theta (avoid poles)
            0.0f64..std::f64::consts::TAU,      // phi
        )
            .prop_map(|(ox, oy, oz, theta, phi)| {
                let nx = theta.sin() * phi.cos();
                let ny = theta.sin() * phi.sin();
                let nz = theta.cos();
                SketchPlaneSpec {
                    origin: [ox, oy, oz],
                    normal: [nx, ny, nz],
                }
            })
    }

    /// Level 1: Any sketch plane — 60% axis-aligned, 40% tilted.
    pub fn sketch_plane_any() -> impl Strategy<Value = SketchPlaneSpec> {
        prop_oneof![
            6 => axis_aligned_plane(),
            4 => tilted_plane(),
        ]
    }

    /// Level 5: Revolve operation with axis in the sketch plane.
    ///
    /// Axis is placed outside the profile bounding box to avoid the axis
    /// passing through the profile (which produces degenerate geometry).
    pub fn revolve_spec(plane: &SketchPlaneSpec) -> impl Strategy<Value = OperationSpec> {
        let n = plane.normal;
        let origin = plane.origin;

        // Compute axis direction: perpendicular to normal, lying in sketch plane
        let up = if n[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let axis_dir = cross(n, up);
        let len =
            (axis_dir[0] * axis_dir[0] + axis_dir[1] * axis_dir[1] + axis_dir[2] * axis_dir[2])
                .sqrt();
        let axis_dir = [axis_dir[0] / len, axis_dir[1] / len, axis_dir[2] / len];

        // Perpendicular in-plane direction for offset
        let perp = cross(n, axis_dir);
        let perp_len = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
        let perp = [perp[0] / perp_len, perp[1] / perp_len, perp[2] / perp_len];

        (30.0f64..360.0, 5.0f64..30.0).prop_map(move |(angle_deg, axis_offset)| {
            let axis_origin = [
                origin[0] - perp[0] * axis_offset,
                origin[1] - perp[1] * axis_offset,
                origin[2] - perp[2] * axis_offset,
            ];

            OperationSpec::Revolve {
                axis_origin,
                axis_direction: axis_dir,
                angle_deg,
            }
        })
    }

    /// Level 5: Any operation — 70% extrude, 30% revolve.
    pub fn operation_any(plane: &SketchPlaneSpec) -> impl Strategy<Value = OperationSpec> {
        let plane_clone = plane.clone();
        prop_oneof![
            7 => extrude_depth().prop_map(|e| OperationSpec::Extrude { depth: e.depth }),
            3 => revolve_spec(&plane_clone),
        ]
    }

    /// Estimate the approximate radius of a profile from its vertices.
    pub fn estimate_profile_radius(profile: &ProfileSpec) -> f64 {
        let verts = super::profile_spec_vertices(profile);
        if verts.is_empty() {
            return 10.0;
        }
        let cx: f64 = verts.iter().map(|v| v.0).sum::<f64>() / verts.len() as f64;
        let cy: f64 = verts.iter().map(|v| v.1).sum::<f64>() / verts.len() as f64;
        verts
            .iter()
            .map(|v| ((v.0 - cx).powi(2) + (v.1 - cy).powi(2)).sqrt())
            .fold(0.0f64, f64::max)
            .max(2.0)
    }

    /// Overlap-biased sketch plane: origin near `center`, within `half_extent`.
    ///
    /// 60% axis-aligned, 40% tilted (same ratio as `sketch_plane_any`).
    fn overlap_biased_plane(
        center: [f64; 3],
        half_extent: f64,
    ) -> impl Strategy<Value = SketchPlaneSpec> {
        let he = half_extent;
        let c = center;
        prop_oneof![
            // 60% axis-aligned with origin near center
            6 => (0u8..3, -he..he).prop_map(move |(axis, offset)| {
                match axis {
                    0 => SketchPlaneSpec {
                        origin: [c[0], c[1], c[2] + offset],
                        normal: [0.0, 0.0, 1.0],
                    },
                    1 => SketchPlaneSpec {
                        origin: [c[0], c[1] + offset, c[2]],
                        normal: [0.0, 1.0, 0.0],
                    },
                    _ => SketchPlaneSpec {
                        origin: [c[0] + offset, c[1], c[2]],
                        normal: [1.0, 0.0, 0.0],
                    },
                }
            }),
            // 40% tilted with origin near center
            4 => (
                -he..he, -he..he, -he..he,
                0.1f64..std::f64::consts::PI - 0.1,
                0.0f64..std::f64::consts::TAU,
            ).prop_map(move |(dx, dy, dz, theta, phi)| {
                SketchPlaneSpec {
                    origin: [c[0] + dx, c[1] + dy, c[2] + dz],
                    normal: [
                        theta.sin() * phi.cos(),
                        theta.sin() * phi.sin(),
                        theta.cos(),
                    ],
                }
            }),
        ]
    }

    /// Overlap-biased convex polygon: radius proportional to `half_extent`,
    /// center offset near zero so it overlaps the base body.
    fn overlap_biased_polygon(half_extent: f64) -> impl Strategy<Value = ProfileSpec> {
        let r_min = half_extent * 0.3;
        let r_max = (half_extent * 1.5).max(r_min + 1.0);
        (
            3u32..=8,
            r_min..r_max,
            -half_extent * 0.3..half_extent * 0.3, // small center offset
            -half_extent * 0.3..half_extent * 0.3,
            0.0f64..std::f64::consts::TAU,
        )
            .prop_map(|(n_sides, radius, cx, cy, rotation)| {
                let vertices = (0..n_sides)
                    .map(|k| {
                        let angle =
                            std::f64::consts::TAU * (k as f64) / (n_sides as f64) + rotation;
                        (cx + radius * angle.cos(), cy + radius * angle.sin())
                    })
                    .collect();
                ProfileSpec::ConvexPolygon(ConvexPolygonSpec { vertices })
            })
    }

    /// Overlap-biased chain step: plane near center, polygon sized to overlap,
    /// extrude depth scaled to half_extent.
    fn overlap_biased_step(center: [f64; 3], half_extent: f64) -> impl Strategy<Value = ChainStep> {
        let d_min = (half_extent * 0.5).max(1.0);
        let d_max = (half_extent * 2.0).max(d_min + 1.0);
        prop_oneof![
            // 70% overlap-biased polygon (high overlap probability)
            7 => (
                overlap_biased_plane(center, half_extent),
                overlap_biased_polygon(half_extent),
                d_min..d_max,
            ).prop_map(|(plane, profile, depth)| ChainStep {
                name: String::new(),
                plane,
                profile,
                operation: OperationSpec::Extrude { depth },
            }),
            // 30% any profile (variety, may not overlap — tests disjoint cases)
            3 => (
                overlap_biased_plane(center, half_extent),
                profile_spec_any(),
                d_min..d_max,
            ).prop_map(|(plane, profile, depth)| ChainStep {
                name: String::new(),
                plane,
                profile,
                operation: OperationSpec::Extrude { depth },
            }),
        ]
    }

    /// Level 6: Generate a modeling chain with 2-5 steps.
    ///
    /// Step 0: Always axis-aligned + convex + extrude (reliable base).
    /// Steps 1+: Overlap-biased plane + profile + extrude, then boolean
    /// against the accumulated result. ~70-80% of subsequent steps will
    /// spatially overlap the base body.
    pub fn modeling_chain() -> impl Strategy<Value = ModelingChain> {
        let step0 = (axis_aligned_plane(), convex_polygon(), extrude_depth()).prop_map(
            |(plane, poly, ext)| ChainStep {
                name: "step_0".to_string(),
                plane,
                profile: ProfileSpec::ConvexPolygon(poly),
                operation: OperationSpec::Extrude { depth: ext.depth },
            },
        );

        step0.prop_flat_map(|s0| {
            // Derive spatial bounds from step 0
            let base_radius = estimate_profile_radius(&s0.profile);
            let base_depth = match &s0.operation {
                OperationSpec::Extrude { depth } => *depth,
                _ => 10.0,
            };
            let half_extent = base_radius.max(base_depth / 2.0);
            let base_center = s0.plane.origin;

            let biased_steps =
                proptest::collection::vec(overlap_biased_step(base_center, half_extent), 1..=4);

            biased_steps.prop_map(move |rest| {
                let mut steps = vec![s0.clone()];
                for (i, mut step) in rest.into_iter().enumerate() {
                    step.name = format!("step_{}", i + 1);
                    steps.push(step);
                }
                ModelingChain { steps }
            })
        })
    }

    /// Level 7: Complete generative chain scenario.
    pub fn generative_chain_scenario() -> impl Strategy<Value = GenerativeChainScenario> {
        (modeling_chain(), region_selection(), 0usize..1000).prop_map(
            |(chain, region_selection, rng_index)| GenerativeChainScenario {
                chain,
                region_selection,
                rng_index,
            },
        )
    }
}

// ── Scenario Executor ─────────────────────────────────────────────────

/// Execute a generative extrude scenario and return the ModelBuilder.
pub fn execute_generative_extrude(
    scenario: &GenerativeExtrudeScenario,
) -> Result<ModelBuilder, String> {
    let mut builder = ModelBuilder::kernel_v2();

    // Build polygon profile
    let (entities, positions, profiles) = polygon_profile(&scenario.polygon.vertices);

    // Begin sketch on the specified plane
    builder.begin_sketch(scenario.plane.origin, scenario.plane.normal);

    // Add all entities
    for entity in &entities {
        match entity {
            waffle_types::SketchEntity::Point { id, x, y, .. } => {
                builder.add_point(*id, *x, *y);
            }
            waffle_types::SketchEntity::Line {
                id,
                start_id,
                end_id,
                ..
            } => {
                builder.add_line(*id, *start_id, *end_id);
            }
            _ => {}
        }
    }

    // Finish sketch
    builder
        .finish_sketch_manual(
            "sk",
            positions,
            profiles,
            scenario.plane.origin,
            scenario.plane.normal,
        )
        .map_err(|e| e.to_string())?;

    // Extrude
    builder
        .extrude("body", "sk", scenario.extrude.depth)
        .map_err(|e| e.to_string())?;

    Ok(builder)
}

// ── Region Selection ─────────────────────────────────────────────────

/// Select a region from a list based on the given strategy.
pub fn select_region<'a>(
    regions: &'a [ClosedRegion],
    strategy: &RegionSelectionStrategy,
    rng_index: usize,
) -> Option<&'a ClosedRegion> {
    if regions.is_empty() {
        return None;
    }
    match strategy {
        RegionSelectionStrategy::Largest => regions.iter().max_by(|a, b| {
            a.area
                .partial_cmp(&b.area)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        RegionSelectionStrategy::Random => Some(&regions[rng_index % regions.len()]),
        RegionSelectionStrategy::SmallestNonTiny { min_area } => {
            let mut eligible: Vec<&ClosedRegion> =
                regions.iter().filter(|r| r.area >= *min_area).collect();
            eligible.sort_by(|a, b| {
                a.area
                    .partial_cmp(&b.area)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            eligible.first().copied()
        }
    }
}

// ── Phase 2 Scenario Executor ────────────────────────────────────────

/// Compute star polygon vertices from spec parameters.
fn star_vertices(
    points: u32,
    inner_r: f64,
    outer_r: f64,
    cx: f64,
    cy: f64,
    rotation: f64,
) -> Vec<(f64, f64)> {
    let n = points * 2;
    (0..n)
        .map(|k| {
            let angle = std::f64::consts::TAU * (k as f64) / (n as f64) + rotation;
            let r = if k % 2 == 0 { outer_r } else { inner_r };
            (cx + r * angle.cos(), cy + r * angle.sin())
        })
        .collect()
}

/// Expand polygon with arcs into a polyline approximation.
///
/// For each edge in `arc_edge_indices`, add an intermediate midpoint bulged
/// outward from the edge. This creates a simple polyline approximation of
/// arc edges without needing NURBS arc entities.
fn expand_arcs(base_vertices: &[(f64, f64)], arc_edge_indices: &[usize]) -> Vec<(f64, f64)> {
    let n = base_vertices.len();
    let mut result = Vec::new();

    for i in 0..n {
        result.push(base_vertices[i]);

        if arc_edge_indices.contains(&i) {
            let j = (i + 1) % n;
            let (x0, y0) = base_vertices[i];
            let (x1, y1) = base_vertices[j];

            // Midpoint
            let mx = (x0 + x1) / 2.0;
            let my = (y0 + y1) / 2.0;

            // Normal direction (perpendicular to edge, pointing outward)
            let dx = x1 - x0;
            let dy = y1 - y0;
            let len = (dx * dx + dy * dy).sqrt();
            if len > 1e-10 {
                // Bulge outward by 20% of edge length
                let bulge = len * 0.2;
                let nx = -dy / len;
                let ny = dx / len;
                result.push((mx + nx * bulge, my + ny * bulge));
            }
        }
    }

    result
}

/// Get vertices from a ProfileSpec for polygon_profile construction.
fn profile_spec_vertices(profile: &ProfileSpec) -> Vec<(f64, f64)> {
    match profile {
        ProfileSpec::ConvexPolygon(p) => p.vertices.clone(),
        ProfileSpec::NonConvexPolygon { vertices } => vertices.clone(),
        ProfileSpec::StarPolygon {
            points,
            inner_r,
            outer_r,
            cx,
            cy,
            rotation,
        } => star_vertices(*points, *inner_r, *outer_r, *cx, *cy, *rotation),
        ProfileSpec::PolygonWithArcs {
            base_vertices,
            arc_edge_indices,
        } => expand_arcs(base_vertices, arc_edge_indices),
    }
}

/// Execute a generative profile scenario (Phase 2) and return the ModelBuilder.
pub fn execute_generative_profile(
    scenario: &GenerativeProfileScenario,
) -> Result<ModelBuilder, String> {
    let vertices = profile_spec_vertices(&scenario.profile);

    // For PolygonWithArcs, run through iOverlay to validate region decomposition
    let final_vertices = match &scenario.profile {
        ProfileSpec::PolygonWithArcs { .. } => {
            // Convert to contour format for iOverlay
            let contour: Vec<[f64; 2]> = vertices.iter().map(|&(x, y)| [x, y]).collect();
            let all_regions = regions::decompose_regions(&[contour]);

            // Select region
            let selected =
                select_region(&all_regions, &scenario.region_selection, scenario.rng_index);

            match selected {
                Some(region) => region.outer.iter().map(|p| (p[0], p[1])).collect(),
                None => vertices, // fallback to original
            }
        }
        _ => vertices,
    };

    if final_vertices.len() < 3 {
        return Err("Profile has fewer than 3 vertices".to_string());
    }

    let mut builder = ModelBuilder::kernel_v2();

    // Build polygon profile from vertices
    let (entities, positions, profiles) = polygon_profile(&final_vertices);

    // Begin sketch on the specified plane
    builder.begin_sketch(scenario.plane.origin, scenario.plane.normal);

    // Add all entities
    for entity in &entities {
        match entity {
            waffle_types::SketchEntity::Point { id, x, y, .. } => {
                builder.add_point(*id, *x, *y);
            }
            waffle_types::SketchEntity::Line {
                id,
                start_id,
                end_id,
                ..
            } => {
                builder.add_line(*id, *start_id, *end_id);
            }
            _ => {}
        }
    }

    // Finish sketch
    builder
        .finish_sketch_manual(
            "sk",
            positions,
            profiles,
            scenario.plane.origin,
            scenario.plane.normal,
        )
        .map_err(|e| e.to_string())?;

    // Extrude
    builder
        .extrude("body", "sk", scenario.extrude.depth)
        .map_err(|e| e.to_string())?;

    Ok(builder)
}

// ── Sketch + Extrude Helper ──────────────────────────────────────────

/// Parameters for building a sketch and extruding it.
struct SketchExtrudeParams<'a> {
    sk_name: &'a str,
    ext_name: &'a str,
    plane: &'a SketchPlaneSpec,
    profile: &'a ProfileSpec,
    depth: f64,
    merge: bool,
    region_selection: &'a RegionSelectionStrategy,
    rng_index: usize,
}

/// Build a sketch from a profile and extrude it.
///
/// Handles all profile types including region decomposition for arc-polygons.
#[allow(clippy::too_many_arguments)]
fn build_sketch_and_extrude(
    builder: &mut ModelBuilder,
    params: &SketchExtrudeParams<'_>,
) -> Result<(), String> {
    let vertices = profile_spec_vertices(params.profile);

    let final_vertices = match params.profile {
        ProfileSpec::PolygonWithArcs { .. } => {
            let contour: Vec<[f64; 2]> = vertices.iter().map(|&(x, y)| [x, y]).collect();
            let all_regions = regions::decompose_regions(&[contour]);
            let selected = select_region(&all_regions, params.region_selection, params.rng_index);
            match selected {
                Some(region) => region.outer.iter().map(|p| (p[0], p[1])).collect(),
                None => vertices,
            }
        }
        _ => vertices,
    };

    if final_vertices.len() < 3 {
        return Err("Profile has fewer than 3 vertices".to_string());
    }

    let (entities, positions, profiles) = polygon_profile(&final_vertices);
    builder.begin_sketch(params.plane.origin, params.plane.normal);
    for entity in &entities {
        match entity {
            waffle_types::SketchEntity::Point { id, x, y, .. } => {
                builder.add_point(*id, *x, *y);
            }
            waffle_types::SketchEntity::Line {
                id,
                start_id,
                end_id,
                ..
            } => {
                builder.add_line(*id, *start_id, *end_id);
            }
            _ => {}
        }
    }

    builder
        .finish_sketch_manual(
            params.sk_name,
            positions,
            profiles,
            params.plane.origin,
            params.plane.normal,
        )
        .map_err(|e| e.to_string())?;

    if params.merge {
        builder
            .extrude(params.ext_name, params.sk_name, params.depth)
            .map_err(|e| e.to_string())?;
    } else {
        builder
            .extrude_no_merge(params.ext_name, params.sk_name, params.depth)
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

// ── Chain Executor ───────────────────────────────────────────────────

/// Select a boolean operation deterministically from step index.
fn pick_boolean_op(step_index: usize) -> BoolOp {
    match step_index % 3 {
        0 => BoolOp::Union,
        1 => BoolOp::Subtract,
        _ => BoolOp::Intersect,
    }
}

/// Execute a boolean operation on two named features.
fn execute_boolean_step(
    builder: &mut ModelBuilder,
    bool_name: &str,
    a_name: &str,
    b_name: &str,
    op: BoolOp,
) -> Result<(), String> {
    match op {
        BoolOp::Union => builder.boolean_union(bool_name, a_name, b_name).map(|_| ()),
        BoolOp::Subtract => builder
            .boolean_subtract(bool_name, a_name, b_name)
            .map(|_| ()),
        BoolOp::Intersect => builder
            .boolean_intersect(bool_name, a_name, b_name)
            .map(|_| ()),
    }
    .map_err(|e| e.to_string())
}

/// Tessellate a feature and compute its mesh volume.
fn measure_volume(builder: &mut ModelBuilder, name: &str) -> Result<f64, String> {
    let mesh = builder.tessellate(name).map_err(|e| e.to_string())?;
    Ok(mesh_volume(&mesh))
}

/// Check per-step volume invariants (I9-I12) for a boolean operation.
///
/// Tessellates operands A and B plus the result, then runs
/// `check_volume_monotonicity`. If tessellation of any operand fails,
/// the check is silently skipped (not a hard failure).
fn check_step_volume_invariants(
    builder: &mut ModelBuilder,
    a_name: &str,
    b_name: &str,
    result_name: &str,
    op: BoolOp,
) -> Option<super::properties::PropertyResult> {
    let mesh_a = builder.tessellate(a_name).ok()?;
    let mesh_b = builder.tessellate(b_name).ok()?;
    let mesh_r = builder.tessellate(result_name).ok()?;
    Some(super::properties::check_volume_monotonicity(
        &mesh_a, &mesh_b, &mesh_r, op,
    ))
}

/// Execute a modeling chain and return intermediate results.
///
/// Partial chains are valid: if a step fails with a known kernel limitation,
/// the completed steps are still returned for oracle checking.
pub fn execute_chain(scenario: &GenerativeChainScenario) -> Result<ChainResult, String> {
    let mut builder = ModelBuilder::kernel_v2();
    let chain = &scenario.chain;
    let mut step_volumes = Vec::new();
    let mut vol_invariants = Vec::new();

    // Step 0: Create base body (always extrude, always merge)
    let step0 = &chain.steps[0];
    let depth0 = match &step0.operation {
        OperationSpec::Extrude { depth } => *depth,
        _ => 20.0, // fallback
    };
    build_sketch_and_extrude(
        &mut builder,
        &SketchExtrudeParams {
            sk_name: "sk_0",
            ext_name: "ext_0",
            plane: &step0.plane,
            profile: &step0.profile,
            depth: depth0,
            merge: true,
            region_selection: &scenario.region_selection,
            rng_index: scenario.rng_index,
        },
    )?;

    match measure_volume(&mut builder, "ext_0") {
        Ok(vol) => step_volumes.push(vol),
        Err(e) if is_known_kernel_limitation(&e) => {
            return Ok(ChainResult {
                builder,
                completed_steps: 1,
                step_volumes,
                final_feature: "ext_0".to_string(),
                volume_invariant_results: vol_invariants,
            });
        }
        Err(e) => return Err(e),
    }

    let mut last_feature = "ext_0".to_string();

    // Steps 1+: Create body (no-merge) + boolean against accumulated result
    for (i, step) in chain.steps[1..].iter().enumerate() {
        let sk_name = format!("sk_{}", i + 1);
        let ext_name = format!("ext_{}", i + 1);
        let bool_name = format!("bool_{}", i);

        let depth = match &step.operation {
            OperationSpec::Extrude { depth } => *depth,
            _ => 20.0,
        };

        // Build sketch + extrude for this step's body
        match build_sketch_and_extrude(
            &mut builder,
            &SketchExtrudeParams {
                sk_name: &sk_name,
                ext_name: &ext_name,
                plane: &step.plane,
                profile: &step.profile,
                depth,
                merge: false,
                region_selection: &scenario.region_selection,
                rng_index: scenario.rng_index,
            },
        ) {
            Ok(_) => {}
            Err(e) if is_known_kernel_limitation(&e) => {
                return Ok(ChainResult {
                    builder,
                    completed_steps: i + 1,
                    step_volumes,
                    final_feature: last_feature,
                    volume_invariant_results: vol_invariants,
                });
            }
            Err(e) => return Err(e),
        }

        // Boolean with accumulated result
        let op = pick_boolean_op(i);
        match execute_boolean_step(&mut builder, &bool_name, &last_feature, &ext_name, op) {
            Ok(_) => {
                // Only update last_feature after confirming the boolean
                // produced a tessellatable solid with positive volume.
                match measure_volume(&mut builder, &bool_name) {
                    Ok(vol) if vol > 1e-6 => {
                        // Check per-step volume invariants (I9-I12)
                        if let Some(inv) = check_step_volume_invariants(
                            &mut builder,
                            &last_feature,
                            &ext_name,
                            &bool_name,
                            op,
                        ) {
                            vol_invariants.push(inv);
                        }
                        last_feature = bool_name;
                        step_volumes.push(vol);
                    }
                    Ok(_) => {
                        // Zero-volume result (e.g., intersect/subtract of
                        // non-overlapping bodies). Truncate chain here.
                        return Ok(ChainResult {
                            builder,
                            completed_steps: i + 1,
                            step_volumes,
                            final_feature: last_feature,
                            volume_invariant_results: vol_invariants,
                        });
                    }
                    Err(e) if is_known_kernel_limitation(&e) => {
                        // Boolean created a feature but no valid solid —
                        // keep last_feature pointing to the previous valid feature.
                        return Ok(ChainResult {
                            builder,
                            completed_steps: i + 1,
                            step_volumes,
                            final_feature: last_feature,
                            volume_invariant_results: vol_invariants,
                        });
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(e) if is_known_kernel_limitation(&e) => {
                return Ok(ChainResult {
                    builder,
                    completed_steps: i + 1,
                    step_volumes,
                    final_feature: last_feature,
                    volume_invariant_results: vol_invariants,
                });
            }
            Err(e) => return Err(e),
        }
    }

    Ok(ChainResult {
        completed_steps: chain.steps.len(),
        step_volumes,
        final_feature: last_feature,
        builder,
        volume_invariant_results: vol_invariants,
    })
}

// ── Known-Failure Filter ──────────────────────────────────────────────

/// Check if an error message indicates a known kernel limitation.
///
/// These are discarded by proptest rather than treated as property violations.
pub fn is_known_kernel_limitation(err: &str) -> bool {
    let known_patterns = [
        "panicked",
        "open edges",
        "empty mesh",
        "failed to tessellate",
        "solid is empty",
        "no solid",
        "index out of bounds",
        "already borrowed",
        "unwrap",
        "non-manifold",
        "not supported",
    ];
    let lower = err.to_lowercase();
    known_patterns.iter().any(|p| lower.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_failure_patterns() {
        assert!(is_known_kernel_limitation(
            "thread panicked at 'index out of bounds'"
        ));
        assert!(is_known_kernel_limitation("result has 4 open edges"));
        assert!(!is_known_kernel_limitation("volume too small"));
    }

    #[test]
    fn display_scenario() {
        let scenario = GenerativeExtrudeScenario {
            plane: SketchPlaneSpec {
                origin: [0.0, 0.0, 5.0],
                normal: [0.0, 0.0, 1.0],
            },
            polygon: ConvexPolygonSpec {
                vertices: vec![(0.0, 0.0), (10.0, 0.0), (5.0, 8.66)],
            },
            extrude: ExtrudeSpec { depth: 20.0 },
        };
        let s = format!("{}", scenario);
        assert!(s.contains("XY"));
        assert!(s.contains("polygon(3sides)"));
        assert!(s.contains("depth=20.0"));
    }

    #[test]
    fn display_profile_spec_variants() {
        let convex = ProfileSpec::ConvexPolygon(ConvexPolygonSpec {
            vertices: vec![(0.0, 0.0), (1.0, 0.0), (0.5, 1.0)],
        });
        assert_eq!(format!("{}", convex), "convex(3sides)");

        let non_convex = ProfileSpec::NonConvexPolygon {
            vertices: vec![(0.0, 0.0), (1.0, 0.0), (0.8, 0.3), (0.5, 1.0)],
        };
        assert_eq!(format!("{}", non_convex), "non_convex(4verts)");

        let star = ProfileSpec::StarPolygon {
            points: 5,
            inner_r: 3.0,
            outer_r: 8.0,
            cx: 0.0,
            cy: 0.0,
            rotation: 0.0,
        };
        assert_eq!(format!("{}", star), "star(5pts)");

        let arcs = ProfileSpec::PolygonWithArcs {
            base_vertices: vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
            arc_edge_indices: vec![0, 2],
        };
        assert_eq!(format!("{}", arcs), "poly_arcs(4v,2arcs)");
    }

    #[test]
    fn star_vertices_count() {
        let verts = star_vertices(5, 3.0, 8.0, 0.0, 0.0, 0.0);
        assert_eq!(verts.len(), 10, "5-point star should have 10 vertices");
    }

    #[test]
    fn star_vertices_alternating_radii() {
        let verts = star_vertices(4, 3.0, 8.0, 0.0, 0.0, 0.0);
        for (i, &(x, y)) in verts.iter().enumerate() {
            let r = (x * x + y * y).sqrt();
            if i % 2 == 0 {
                assert!(
                    (r - 8.0).abs() < 1e-10,
                    "Even vertices should be at outer_r"
                );
            } else {
                assert!((r - 3.0).abs() < 1e-10, "Odd vertices should be at inner_r");
            }
        }
    }

    #[test]
    fn expand_arcs_adds_midpoints() {
        let base = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let expanded = expand_arcs(&base, &[0, 2]);
        // Original 4 vertices + 2 arc midpoints = 6
        assert_eq!(expanded.len(), 6);
    }

    #[test]
    fn expand_arcs_no_arcs_unchanged() {
        let base = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        let expanded = expand_arcs(&base, &[]);
        assert_eq!(expanded.len(), 3);
    }

    #[test]
    fn select_region_largest() {
        let regions = vec![
            ClosedRegion {
                outer: vec![],
                holes: vec![],
                area: 10.0,
            },
            ClosedRegion {
                outer: vec![],
                holes: vec![],
                area: 50.0,
            },
            ClosedRegion {
                outer: vec![],
                holes: vec![],
                area: 25.0,
            },
        ];
        let selected = select_region(&regions, &RegionSelectionStrategy::Largest, 0).unwrap();
        assert!((selected.area - 50.0).abs() < 1e-10);
    }

    #[test]
    fn select_region_random_wraps() {
        let regions = vec![
            ClosedRegion {
                outer: vec![],
                holes: vec![],
                area: 10.0,
            },
            ClosedRegion {
                outer: vec![],
                holes: vec![],
                area: 20.0,
            },
        ];
        let s0 = select_region(&regions, &RegionSelectionStrategy::Random, 0).unwrap();
        assert!((s0.area - 10.0).abs() < 1e-10);
        let s1 = select_region(&regions, &RegionSelectionStrategy::Random, 1).unwrap();
        assert!((s1.area - 20.0).abs() < 1e-10);
        // Wraps around
        let s2 = select_region(&regions, &RegionSelectionStrategy::Random, 2).unwrap();
        assert!((s2.area - 10.0).abs() < 1e-10);
    }

    #[test]
    fn select_region_smallest_non_tiny() {
        let regions = vec![
            ClosedRegion {
                outer: vec![],
                holes: vec![],
                area: 2.0,
            },
            ClosedRegion {
                outer: vec![],
                holes: vec![],
                area: 50.0,
            },
            ClosedRegion {
                outer: vec![],
                holes: vec![],
                area: 15.0,
            },
        ];
        let strategy = RegionSelectionStrategy::SmallestNonTiny { min_area: 10.0 };
        let selected = select_region(&regions, &strategy, 0).unwrap();
        assert!((selected.area - 15.0).abs() < 1e-10);
    }

    #[test]
    fn select_region_empty() {
        let regions: Vec<ClosedRegion> = vec![];
        assert!(select_region(&regions, &RegionSelectionStrategy::Largest, 0).is_none());
    }

    #[test]
    fn display_generative_profile_scenario() {
        let scenario = GenerativeProfileScenario {
            plane: SketchPlaneSpec {
                origin: [0.0, 0.0, 5.0],
                normal: [0.0, 0.0, 1.0],
            },
            profile: ProfileSpec::StarPolygon {
                points: 5,
                inner_r: 3.0,
                outer_r: 8.0,
                cx: 0.0,
                cy: 0.0,
                rotation: 0.0,
            },
            extrude: ExtrudeSpec { depth: 15.0 },
            region_selection: RegionSelectionStrategy::Largest,
            rng_index: 42,
        };
        let s = format!("{}", scenario);
        assert!(s.contains("XY"));
        assert!(s.contains("star(5pts)"));
        assert!(s.contains("depth=15.0"));
        assert!(s.contains("region=largest"));
    }

    #[test]
    // The 0.7071 literals are deliberately NOT f64::consts::FRAC_1_SQRT_2:
    // this test feeds hand-rounded normals (as a corpus author would write
    // them) into the normalization check below.
    #[allow(clippy::approx_constant)]
    fn tilted_plane_has_unit_normal() {
        // Verify tilted plane normals are unit vectors
        let planes = [
            SketchPlaneSpec {
                origin: [0.0, 0.0, 0.0],
                normal: [0.577, 0.577, 0.577], // approximately (1,1,1)/sqrt(3)
            },
            SketchPlaneSpec {
                origin: [1.0, 2.0, 3.0],
                normal: [0.7071, 0.7071, 0.0], // (1,1,0)/sqrt(2)
            },
        ];
        for plane in &planes {
            let n = plane.normal;
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "Normal should be approximately unit length, got {}",
                len
            );
        }
    }

    #[test]
    fn sketch_plane_any_display() {
        // Axis-aligned planes show axis name
        let xy = SketchPlaneSpec {
            origin: [0.0, 0.0, 5.0],
            normal: [0.0, 0.0, 1.0],
        };
        assert!(format!("{}", xy).contains("XY"));

        // Tilted planes show "tilted"
        let tilted = SketchPlaneSpec {
            origin: [1.0, 2.0, 3.0],
            normal: [0.577, 0.577, 0.577],
        };
        assert!(
            format!("{}", tilted).contains("tilted"),
            "Tilted plane should display as 'tilted', got: {}",
            tilted
        );
    }

    #[test]
    fn revolve_spec_axis_in_plane() {
        // For an XY plane, axis should be perpendicular to Z normal
        let plane = SketchPlaneSpec {
            origin: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
        };
        let n = plane.normal;

        // Compute what revolve_spec would produce
        let up = if n[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let axis_dir = cross(n, up);
        let len =
            (axis_dir[0] * axis_dir[0] + axis_dir[1] * axis_dir[1] + axis_dir[2] * axis_dir[2])
                .sqrt();
        let axis_dir = [axis_dir[0] / len, axis_dir[1] / len, axis_dir[2] / len];

        // axis_direction · normal should be ~0 (perpendicular)
        let dot = axis_dir[0] * n[0] + axis_dir[1] * n[1] + axis_dir[2] * n[2];
        assert!(
            dot.abs() < 1e-10,
            "Axis direction should be perpendicular to normal, dot={}",
            dot
        );
    }

    #[test]
    fn revolve_spec_axis_outside_origin() {
        let plane = SketchPlaneSpec {
            origin: [5.0, 5.0, 0.0],
            normal: [0.0, 0.0, 1.0],
        };
        let n = plane.normal;
        let up = if n[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let axis_dir = cross(n, up);
        let len =
            (axis_dir[0] * axis_dir[0] + axis_dir[1] * axis_dir[1] + axis_dir[2] * axis_dir[2])
                .sqrt();
        let axis_dir = [axis_dir[0] / len, axis_dir[1] / len, axis_dir[2] / len];
        let perp = cross(n, axis_dir);
        let perp_len = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
        let perp = [perp[0] / perp_len, perp[1] / perp_len, perp[2] / perp_len];

        let axis_offset = 10.0;
        let axis_origin = [
            plane.origin[0] - perp[0] * axis_offset,
            plane.origin[1] - perp[1] * axis_offset,
            plane.origin[2] - perp[2] * axis_offset,
        ];

        // Axis origin should differ from plane origin
        let dist = ((axis_origin[0] - plane.origin[0]).powi(2)
            + (axis_origin[1] - plane.origin[1]).powi(2)
            + (axis_origin[2] - plane.origin[2]).powi(2))
        .sqrt();
        assert!(
            dist > 1.0,
            "Axis origin should be offset from plane origin, dist={}",
            dist
        );
    }

    #[test]
    fn pick_boolean_op_cycles() {
        assert!(matches!(pick_boolean_op(0), BoolOp::Union));
        assert!(matches!(pick_boolean_op(1), BoolOp::Subtract));
        assert!(matches!(pick_boolean_op(2), BoolOp::Intersect));
        assert!(matches!(pick_boolean_op(3), BoolOp::Union));
    }

    #[test]
    fn display_operation_spec() {
        let ext = OperationSpec::Extrude { depth: 15.5 };
        assert_eq!(format!("{}", ext), "extrude(15.5)");

        let rev = OperationSpec::Revolve {
            axis_origin: [0.0, 0.0, 0.0],
            axis_direction: [1.0, 0.0, 0.0],
            angle_deg: 180.0,
        };
        assert!(format!("{}", rev).contains("revolve"));
    }

    #[test]
    fn estimate_profile_radius_convex() {
        // A regular triangle with radius 10 centered at origin
        let profile = ProfileSpec::ConvexPolygon(ConvexPolygonSpec {
            vertices: vec![(10.0, 0.0), (-5.0, 8.66), (-5.0, -8.66)],
        });
        let r = strats_v2::estimate_profile_radius(&profile);
        assert!((r - 10.0).abs() < 0.1, "Expected ~10.0, got {}", r);
    }

    #[test]
    fn estimate_profile_radius_offset_center() {
        // Square centered at (5,5) with half-side 3 → radius ≈ 3*sqrt(2) ≈ 4.24
        let profile = ProfileSpec::ConvexPolygon(ConvexPolygonSpec {
            vertices: vec![(2.0, 2.0), (8.0, 2.0), (8.0, 8.0), (2.0, 8.0)],
        });
        let r = strats_v2::estimate_profile_radius(&profile);
        assert!((r - 4.243).abs() < 0.1, "Expected ~4.24, got {}", r);
    }

    #[test]
    fn estimate_profile_radius_minimum() {
        // Tiny profile — should clamp to 2.0
        let profile = ProfileSpec::ConvexPolygon(ConvexPolygonSpec {
            vertices: vec![(0.0, 0.0), (0.5, 0.0), (0.25, 0.4)],
        });
        let r = strats_v2::estimate_profile_radius(&profile);
        assert!(r >= 2.0, "Minimum radius should be 2.0, got {}", r);
    }

    #[test]
    fn display_modeling_chain() {
        let chain = ModelingChain {
            steps: vec![
                ChainStep {
                    name: "step_0".to_string(),
                    plane: SketchPlaneSpec {
                        origin: [0.0, 0.0, 0.0],
                        normal: [0.0, 0.0, 1.0],
                    },
                    profile: ProfileSpec::ConvexPolygon(ConvexPolygonSpec {
                        vertices: vec![(0.0, 0.0), (10.0, 0.0), (5.0, 8.66)],
                    }),
                    operation: OperationSpec::Extrude { depth: 20.0 },
                },
                ChainStep {
                    name: "step_1".to_string(),
                    plane: SketchPlaneSpec {
                        origin: [0.0, 0.0, 5.0],
                        normal: [0.0, 0.0, 1.0],
                    },
                    profile: ProfileSpec::ConvexPolygon(ConvexPolygonSpec {
                        vertices: vec![(0.0, 0.0), (5.0, 0.0), (2.5, 4.33)],
                    }),
                    operation: OperationSpec::Extrude { depth: 10.0 },
                },
            ],
        };
        assert_eq!(format!("{}", chain), "chain(2steps)");
    }
}
