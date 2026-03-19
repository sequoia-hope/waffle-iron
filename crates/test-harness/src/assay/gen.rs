//! Randomized test case generator for Assay v3.
//!
//! Produces `.waffle` CAD files + `.meta.json` sidecar metadata files
//! for property-based testing through the full LoadProject engine path.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::helpers::{datum_plane_ref, gear_profile, rect_profile, ProfileData};
use feature_engine::types::{
    DepthMode, ExtrudeParams, Feature, FeatureTree, Operation, RevolveParams,
};
use file_format::metadata::ProjectMetadata;
use file_format::save::save_project;
use waffle_types::{CircleProfile, ClosedProfile, Sketch, SketchEntity, SolveStatus};

/// Generator version — bump when output format changes.
pub const GENERATOR_VERSION: u32 = 2;

// ── Configuration ──────────────────────────────────────────────────────────

/// Configuration for corpus generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusConfig {
    /// Master seed for reproducibility.
    pub master_seed: u64,
    /// Number of test cases to generate.
    pub case_count: usize,
    /// Output directory for generated files.
    pub output_dir: PathBuf,
}

/// Statistics from corpus generation.
#[derive(Debug, Clone)]
pub struct CorpusStats {
    pub count: usize,
    pub extrude_count: usize,
    pub revolve_count: usize,
}

// ── Metadata Types ─────────────────────────────────────────────────────────

/// Sidecar metadata for a generated test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssayMeta {
    /// Case identifier (e.g. "R0001").
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Master seed used for the corpus.
    pub master_seed: u64,
    /// Per-case seed derived from master + index.
    pub test_seed: u64,
    /// Scale factor applied to all geometry.
    pub scale: f64,
    /// Log10 of scale factor.
    pub log_scale: f64,
    /// Sketch plane origin in 3D.
    pub plane_origin: [f64; 3],
    /// Sketch plane normal in 3D.
    pub plane_normal: [f64; 3],
    /// Metadata for each operation in the feature tree.
    pub operations: Vec<OpMeta>,
    /// Expected oracle outcomes.
    pub oracles: OracleExpectations,
    /// Generator version that produced this case.
    pub generator_version: u32,
    /// Whether this is a featured (curated) test case.
    #[serde(default)]
    pub featured: bool,
}

/// Metadata for a single operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpMeta {
    /// Operation kind: "extrude" or "revolve".
    pub kind: String,
    /// Profile type: "rectangle", "circle", or "gear".
    pub profile_type: String,
    /// Profile characteristic size (width for rect, radius for circle, pitch_radius for gear).
    pub profile_size: f64,
    /// Depth (meters) for extrude, angle (degrees) for revolve.
    pub depth_or_angle: f64,
    /// Whether this is a cut operation.
    pub is_cut: bool,
    /// Per-operation sketch plane origin (if different from case-level plane).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane_origin: Option<[f64; 3]>,
    /// Per-operation sketch plane normal (if different from case-level plane).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane_normal: Option<[f64; 3]>,
}

/// Expected oracle outcomes for a test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleExpectations {
    /// Expected Euler characteristic (V-E+F) of the final solid.
    pub euler_target: i64,
    /// Whether the final mesh should be watertight.
    pub expect_watertight: bool,
    /// Maximum bounding box extent (diagonal) in meters.
    pub max_bbox_extent: f64,
    /// Whether the final solid should have positive volume.
    /// Vestigial: always true. Volume positivity is now checked unconditionally
    /// by `check_positive_signed_volume()` in `run_all_mesh_checks()`.
    pub expect_positive_volume: bool,
    /// Per-step volume monotonicity: "increase" for boss, "decrease" for cut.
    pub volume_monotonicity: Vec<String>,
}

/// A generated test case ready to be written to disk.
pub struct GeneratedCase {
    /// Case identifier.
    pub id: String,
    /// Serialized .waffle file content.
    pub waffle_json: String,
    /// Sidecar metadata.
    pub meta: AssayMeta,
}

/// Manifest entry for a single case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub id: String,
    pub filename: String,
    pub meta_filename: String,
    pub description: String,
    /// Whether this is a featured (curated) test case.
    #[serde(default)]
    pub featured: bool,
}

/// Manifest for the entire corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusManifest {
    pub master_seed: u64,
    pub count: usize,
    pub generator_version: u32,
    pub cases: Vec<ManifestEntry>,
}

// ── Seed Derivation ────────────────────────────────────────────────────────

/// Derive a per-case seed from master seed and index using Knuth multiplicative hash.
pub fn derive_seed(master: u64, index: usize) -> u64 {
    master
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(index as u64)
}

// ── Random Generators ──────────────────────────────────────────────────────

/// Generate a log-uniform scale factor in [1e-4, 1e4].
pub fn random_scale(rng: &mut impl Rng) -> f64 {
    let exponent: f64 = rng.gen_range(-4.0..4.0);
    10f64.powf(exponent)
}

/// Generate a random unit normal via rejection sampling on the unit ball.
pub fn random_unit_normal(rng: &mut impl Rng) -> [f64; 3] {
    loop {
        let x: f64 = rng.gen_range(-1.0..1.0);
        let y: f64 = rng.gen_range(-1.0..1.0);
        let z: f64 = rng.gen_range(-1.0..1.0);
        let len_sq = x * x + y * y + z * z;
        if len_sq < 1e-12 || len_sq > 1.0 {
            continue;
        }
        let len = len_sq.sqrt();
        return [x / len, y / len, z / len];
    }
}

/// Generate N well-separated unit normals with at least `min_angle_rad` between any pair.
///
/// Uses rejection sampling: each candidate must satisfy `|dot(candidate, existing)| ≤ cos(min_angle)`
/// for all existing normals. For N ≤ 4, converges quickly.
pub fn generate_well_separated_normals(
    rng: &mut impl Rng,
    n: usize,
    min_angle_rad: f64,
) -> Vec<[f64; 3]> {
    let max_dot = min_angle_rad.cos();
    let mut normals: Vec<[f64; 3]> = Vec::with_capacity(n);

    while normals.len() < n {
        let candidate = random_unit_normal(rng);
        let well_separated = normals.iter().all(|existing| {
            let dot = candidate[0] * existing[0]
                + candidate[1] * existing[1]
                + candidate[2] * existing[2];
            dot.abs() <= max_dot
        });
        if well_separated {
            normals.push(candidate);
        }
    }

    normals
}

/// Generate a random sketch plane: origin and normal.
///
/// Picks 3 random points in [-scale, scale]^3, computes the normal via cross product.
/// Rejects degenerate (collinear) configurations.
pub fn random_plane(rng: &mut impl Rng, scale: f64) -> ([f64; 3], [f64; 3]) {
    loop {
        let p0: [f64; 3] = [
            rng.gen_range(-scale..scale),
            rng.gen_range(-scale..scale),
            rng.gen_range(-scale..scale),
        ];
        let p1: [f64; 3] = [
            rng.gen_range(-scale..scale),
            rng.gen_range(-scale..scale),
            rng.gen_range(-scale..scale),
        ];
        let p2: [f64; 3] = [
            rng.gen_range(-scale..scale),
            rng.gen_range(-scale..scale),
            rng.gen_range(-scale..scale),
        ];

        // Vectors from p0 to p1 and p0 to p2
        let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let v = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];

        // Cross product
        let nx = u[1] * v[2] - u[2] * v[1];
        let ny = u[2] * v[0] - u[0] * v[2];
        let nz = u[0] * v[1] - u[1] * v[0];

        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len < 1e-12 {
            continue; // collinear, retry
        }

        let normal = [nx / len, ny / len, nz / len];
        return (p0, normal);
    }
}

/// Generate a random sketch primitive. Returns (ProfileData, profile_type_name, characteristic_size).
///
/// Weighted 33/33/33: rectangle, true circle, gear.
pub fn random_sketch_primitive(rng: &mut impl Rng, scale: f64) -> (ProfileData, String, f64) {
    let choice: u32 = rng.gen_range(0..3);
    match choice {
        0 => {
            // Rectangle: size 20-80% of scale
            let frac = rng.gen_range(0.2..0.8);
            let w = scale * frac;
            let h = scale * rng.gen_range(0.2..0.8);
            let data = rect_profile(-w / 2.0, -h / 2.0, w, h);
            (data, "rectangle".to_string(), w)
        }
        1 => {
            // True circle: radius 10-40% of scale
            let radius = scale * rng.gen_range(0.1..0.4);
            let data = true_circle_profile(0.0, 0.0, radius);
            (data, "circle".to_string(), radius)
        }
        _ => {
            // Gear: 8-24 teeth
            let teeth: u32 = rng.gen_range(8..=24);
            let module_val = scale * 0.05; // makes pitch_radius ~ teeth * module / 2
            let data = gear_profile(teeth, module_val, 20.0);
            let pitch_radius = (teeth as f64) * module_val / 2.0;
            (data, "gear".to_string(), pitch_radius)
        }
    }
}

/// Build a true circle profile (not a polygon approximation).
///
/// Creates a center Point (construction) and a Circle entity.
fn true_circle_profile(cx: f64, cy: f64, radius: f64) -> ProfileData {
    let center_id = 1u32;
    let circle_id = 2u32;

    let entities = vec![
        SketchEntity::Point {
            id: center_id,
            x: cx,
            y: cy,
            construction: true,
        },
        SketchEntity::Circle {
            id: circle_id,
            center_id,
            radius,
            construction: false,
        },
    ];

    let mut positions = HashMap::new();
    positions.insert(center_id, (cx, cy));

    let profiles = vec![ClosedProfile {
        entity_ids: vec![circle_id],
        is_outer: true,
        vertex_ids: vec![],
        circle: Some(CircleProfile {
            center_u: cx,
            center_v: cy,
            radius,
        }),
        spline_segments: vec![],
    }];

    (entities, positions, profiles)
}

/// Generate a random operation type.
///
/// 70% extrude, 30% revolve. First operation is always boss (not cut).
/// Subsequent operations are 50/50 boss/cut.
pub fn random_operation(rng: &mut impl Rng, scale: f64, is_first: bool) -> (String, f64, bool) {
    let is_extrude = rng.gen_range(0..10) < 7;
    let is_cut = if is_first { false } else { rng.gen_bool(0.5) };

    if is_extrude {
        let depth = scale * rng.gen_range(0.1..0.8);
        ("extrude".to_string(), depth, is_cut)
    } else {
        let angle = rng.gen_range(30.0..360.0); // degrees
        ("revolve".to_string(), angle, is_cut)
    }
}

// ── Case Generation ────────────────────────────────────────────────────────

/// Generate a single test case from a master seed and index.
pub fn generate_case(master_seed: u64, index: usize) -> GeneratedCase {
    let test_seed = derive_seed(master_seed, index);
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(test_seed);

    let scale = random_scale(&mut rng);
    let log_scale = scale.log10();
    let (plane_origin, plane_normal) = random_plane(&mut rng, scale);

    // 50% of cases use per-operation planes for cross-plane boolean coverage
    let multi_plane = rng.gen_bool(0.5);

    // 2-3 operations per case
    let op_count: usize = rng.gen_range(2..=3);

    // Pre-generate well-separated normals and independent origins for multi-plane cases.
    // We need op_count-1 additional normals that are well-separated from the case-level
    // normal AND from each other. Build by seeding with the case normal, then taking the rest.
    let min_angle = std::f64::consts::FRAC_PI_6; // 30°
    let per_op_normals = if multi_plane && op_count > 1 {
        // Generate op_count normals total (including the case-level one), then skip first
        let mut all_normals = vec![plane_normal];
        let max_dot = min_angle.cos();
        while all_normals.len() < op_count {
            let candidate = random_unit_normal(&mut rng);
            let well_separated = all_normals.iter().all(|existing| {
                let dot = candidate[0] * existing[0]
                    + candidate[1] * existing[1]
                    + candidate[2] * existing[2];
                dot.abs() <= max_dot
            });
            if well_separated {
                all_normals.push(candidate);
            }
        }
        all_normals
    } else {
        vec![]
    };
    let per_op_origins: Vec<[f64; 3]> = if multi_plane {
        (0..op_count)
            .map(|_| {
                [
                    rng.gen_range(-scale..scale),
                    rng.gen_range(-scale..scale),
                    rng.gen_range(-scale..scale),
                ]
            })
            .collect()
    } else {
        vec![]
    };

    let mut features: Vec<Feature> = Vec::new();
    let mut op_metas: Vec<OpMeta> = Vec::new();
    let mut volume_monotonicity: Vec<String> = Vec::new();

    for i in 0..op_count {
        let is_first = i == 0;
        let (primitive_data, profile_type, profile_size) = random_sketch_primitive(&mut rng, scale);
        let (op_kind, depth_or_angle, is_cut) = random_operation(&mut rng, scale, is_first);

        // Use per-op plane when multi_plane and not the first operation
        let (op_origin, op_normal) = if multi_plane && i > 0 {
            (per_op_origins[i], per_op_normals[i])
        } else {
            (plane_origin, plane_normal)
        };

        let (entities, positions, profiles) = primitive_data;

        // Create sketch feature
        let sketch_id = Uuid::new_v4();
        let sketch_feature = Feature {
            id: sketch_id,
            name: format!("Sketch {}", i + 1),
            operation: Operation::Sketch {
                sketch: Sketch {
                    id: sketch_id,
                    plane: datum_plane_ref(Uuid::new_v4()),
                    plane_origin: op_origin,
                    plane_normal: op_normal,
                    entities,
                    constraints: vec![],
                    solve_status: SolveStatus::FullyConstrained,
                    solved_positions: positions,
                    solved_profiles: profiles,
                },
            },
            suppressed: false,
            references: vec![],
        };
        features.push(sketch_feature);

        // Create operation feature
        let op_feature = if op_kind == "extrude" {
            Feature {
                id: Uuid::new_v4(),
                name: format!("Extrude {}", i + 1),
                operation: Operation::Extrude {
                    params: ExtrudeParams {
                        sketch_id,
                        profile_index: 0,
                        depth: depth_or_angle,
                        direction: None,
                        symmetric: false,
                        cut: is_cut,
                        merge: true,
                        target_body: None,
                        depth_mode: DepthMode::Blind,
                        second_direction: None,
                    },
                },
                suppressed: false,
                references: vec![],
            }
        } else {
            // Revolve: place axis offset from profile center to avoid self-intersection
            let axis_offset = profile_size * 1.5;
            Feature {
                id: Uuid::new_v4(),
                name: format!("Revolve {}", i + 1),
                operation: Operation::Revolve {
                    params: RevolveParams {
                        sketch_id,
                        profile_index: 0,
                        axis_origin: [
                            op_origin[0] + axis_offset * op_normal[1],
                            op_origin[1] - axis_offset * op_normal[0],
                            op_origin[2],
                        ],
                        axis_direction: [op_normal[1], -op_normal[0], 0.0],
                        angle: depth_or_angle,
                        cut: is_cut,
                        merge: true,
                    },
                },
                suppressed: false,
                references: vec![],
            }
        };
        features.push(op_feature);

        op_metas.push(OpMeta {
            kind: op_kind.clone(),
            profile_type,
            profile_size,
            depth_or_angle,
            is_cut,
            plane_origin: if multi_plane && i > 0 {
                Some(op_origin)
            } else {
                None
            },
            plane_normal: if multi_plane && i > 0 {
                Some(op_normal)
            } else {
                None
            },
        });

        volume_monotonicity.push(if is_cut {
            "decrease".to_string()
        } else {
            "increase".to_string()
        });
    }

    let tree = FeatureTree {
        features,
        active_index: None,
    };

    let metadata = ProjectMetadata::new(format!("Assay R{:04}", index + 1));
    let waffle_json = save_project(&tree, &metadata);

    let case_id = format!("R{:04}", index + 1);
    let description = format!(
        "{} ops, scale={:.2e}, {}",
        op_count,
        scale,
        op_metas
            .iter()
            .map(|o| format!(
                "{}({},{})",
                o.kind,
                o.profile_type,
                if o.is_cut { "cut" } else { "boss" }
            ))
            .collect::<Vec<_>>()
            .join("+")
    );

    let max_bbox_extent = scale * 3.0; // conservative upper bound

    let meta = AssayMeta {
        id: case_id.clone(),
        description: description.clone(),
        master_seed,
        test_seed,
        scale,
        log_scale,
        plane_origin,
        plane_normal,
        operations: op_metas,
        oracles: OracleExpectations {
            euler_target: 2, // Euler characteristic for a single solid
            expect_watertight: true,
            max_bbox_extent,
            expect_positive_volume: true,
            volume_monotonicity,
        },
        generator_version: GENERATOR_VERSION,
        featured: false,
    };

    GeneratedCase {
        id: case_id,
        waffle_json,
        meta,
    }
}

// ── Featured Case Generation ──────────────────────────────────────────────

/// Specification for a single featured test case.
struct FeaturedSpec {
    id: &'static str,
    scale: f64,
    w1: f64,
    h1: f64,
    d1: f64,
    w2: f64,
    h2: f64,
    d2: f64,
    description: &'static str,
}

/// 10 curated rect-boss + rect-boss test cases.
const FEATURED_SPECS: [FeaturedSpec; 10] = [
    FeaturedSpec {
        id: "F0001",
        scale: 1.0,
        w1: 0.5,
        h1: 0.5,
        d1: 0.3,
        w2: 0.5,
        h2: 0.5,
        d2: 0.3,
        description: "Identical squares",
    },
    FeaturedSpec {
        id: "F0002",
        scale: 0.01,
        w1: 0.006,
        h1: 0.002,
        d1: 0.004,
        w2: 0.002,
        h2: 0.006,
        d2: 0.004,
        description: "Small cross-shaped",
    },
    FeaturedSpec {
        id: "F0003",
        scale: 100.0,
        w1: 60.0,
        h1: 40.0,
        d1: 30.0,
        w2: 40.0,
        h2: 60.0,
        d2: 20.0,
        description: "Large, swapped aspect",
    },
    FeaturedSpec {
        id: "F0004",
        scale: 1.0,
        w1: 0.8,
        h1: 0.2,
        d1: 0.5,
        w2: 0.2,
        h2: 0.8,
        d2: 0.5,
        description: "Thin cross",
    },
    FeaturedSpec {
        id: "F0005",
        scale: 1.0,
        w1: 0.3,
        h1: 0.3,
        d1: 0.1,
        w2: 0.3,
        h2: 0.3,
        d2: 0.8,
        description: "Same rect, different depths",
    },
    FeaturedSpec {
        id: "F0006",
        scale: 0.001,
        w1: 0.0004,
        h1: 0.0004,
        d1: 0.0003,
        w2: 0.0006,
        h2: 0.0002,
        d2: 0.0003,
        description: "Micro scale",
    },
    FeaturedSpec {
        id: "F0007",
        scale: 10.0,
        w1: 6.0,
        h1: 6.0,
        d1: 4.0,
        w2: 3.0,
        h2: 3.0,
        d2: 2.0,
        description: "Concentric squares",
    },
    FeaturedSpec {
        id: "F0008",
        scale: 1.0,
        w1: 0.4,
        h1: 0.4,
        d1: 0.2,
        w2: 0.6,
        h2: 0.6,
        d2: 0.4,
        description: "Nested squares, deep",
    },
    FeaturedSpec {
        id: "F0009",
        scale: 0.1,
        w1: 0.06,
        h1: 0.04,
        d1: 0.03,
        w2: 0.04,
        h2: 0.06,
        d2: 0.02,
        description: "Medium-small, swapped",
    },
    FeaturedSpec {
        id: "F0010",
        scale: 1.0,
        w1: 0.5,
        h1: 0.3,
        d1: 0.01,
        w2: 0.3,
        h2: 0.5,
        d2: 1.0,
        description: "Shallow vs very deep",
    },
];

/// Generate 10 curated featured test cases (rect-boss + rect-boss).
///
/// Writes `.waffle` and `.meta.json` files to `output_dir`.
/// Returns manifest entries for inclusion in the corpus manifest.
pub fn generate_featured_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();

    for spec in &FEATURED_SPECS {
        let plane_origin = [0.0, 0.0, 0.0];
        let plane_normal = [0.0, 0.0, 1.0];

        let mut features: Vec<Feature> = Vec::new();

        // Operation 1: extrude(rect, boss)
        let sketch1_id = Uuid::new_v4();
        let (entities1, positions1, profiles1) =
            rect_profile(-spec.w1 / 2.0, -spec.h1 / 2.0, spec.w1, spec.h1);
        features.push(Feature {
            id: sketch1_id,
            name: "Sketch 1".to_string(),
            operation: Operation::Sketch {
                sketch: Sketch {
                    id: sketch1_id,
                    plane: datum_plane_ref(Uuid::new_v4()),
                    plane_origin,
                    plane_normal,
                    entities: entities1,
                    constraints: vec![],
                    solve_status: SolveStatus::FullyConstrained,
                    solved_positions: positions1,
                    solved_profiles: profiles1,
                },
            },
            suppressed: false,
            references: vec![],
        });
        features.push(Feature {
            id: Uuid::new_v4(),
            name: "Extrude 1".to_string(),
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: sketch1_id,
                    profile_index: 0,
                    depth: spec.d1,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: true,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                },
            },
            suppressed: false,
            references: vec![],
        });

        // Operation 2: extrude(rect, boss)
        let sketch2_id = Uuid::new_v4();
        let (entities2, positions2, profiles2) =
            rect_profile(-spec.w2 / 2.0, -spec.h2 / 2.0, spec.w2, spec.h2);
        features.push(Feature {
            id: sketch2_id,
            name: "Sketch 2".to_string(),
            operation: Operation::Sketch {
                sketch: Sketch {
                    id: sketch2_id,
                    plane: datum_plane_ref(Uuid::new_v4()),
                    plane_origin,
                    plane_normal,
                    entities: entities2,
                    constraints: vec![],
                    solve_status: SolveStatus::FullyConstrained,
                    solved_positions: positions2,
                    solved_profiles: profiles2,
                },
            },
            suppressed: false,
            references: vec![],
        });
        features.push(Feature {
            id: Uuid::new_v4(),
            name: "Extrude 2".to_string(),
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: sketch2_id,
                    profile_index: 0,
                    depth: spec.d2,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: true,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                },
            },
            suppressed: false,
            references: vec![],
        });

        let tree = FeatureTree {
            features,
            active_index: None,
        };

        let metadata = ProjectMetadata::new(format!("Assay {}", spec.id));
        let waffle_json = save_project(&tree, &metadata);

        let description = format!(
            "2 ops, scale={:.2e}, extrude(rectangle,boss)+extrude(rectangle,boss) — {}",
            spec.scale, spec.description
        );

        let max_bbox_extent = spec.scale * 3.0;

        let meta = AssayMeta {
            id: spec.id.to_string(),
            description: description.clone(),
            master_seed: 0,
            test_seed: 0,
            scale: spec.scale,
            log_scale: spec.scale.log10(),
            plane_origin,
            plane_normal,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: spec.w1,
                    depth_or_angle: spec.d1,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: spec.w2,
                    depth_or_angle: spec.d2,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "increase".to_string()],
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        let waffle_filename = format!("{}.waffle", spec.id);
        let meta_filename = format!("{}.meta.json", spec.id);

        let waffle_path = output_dir.join(&waffle_filename);
        fs::write(&waffle_path, &waffle_json).unwrap_or_else(|e| {
            panic!("failed to write {}: {}", waffle_path.display(), e);
        });

        let meta_path = output_dir.join(&meta_filename);
        let meta_json = serde_json::to_string_pretty(&meta).expect("meta serialization failed");
        fs::write(&meta_path, meta_json).unwrap_or_else(|e| {
            panic!("failed to write {}: {}", meta_path.display(), e);
        });

        entries.push(ManifestEntry {
            id: spec.id.to_string(),
            filename: waffle_filename,
            meta_filename,
            description,
            featured: true,
        });
    }

    // Append oblique-plane featured cases (F0011-F0015)
    entries.extend(generate_oblique_plane_cases(output_dir));

    // Append intersecting multi-extrude oblique cases (F0016-F0025)
    entries.extend(generate_intersecting_oblique_cases(output_dir));

    // Append boolean-path-targeting cases (F0026-F0060)
    entries.extend(generate_circle_boss_cases(output_dir));
    entries.extend(generate_box_minus_cyl_cases(output_dir));
    entries.extend(generate_cyl_minus_box_cases(output_dir));
    entries.extend(generate_cyl_cyl_parallel_cases(output_dir));
    entries.extend(generate_mixed_cross_plane_cases(output_dir));
    entries.extend(generate_scale_extreme_cases(output_dir));
    entries.extend(generate_cyl_cyl_angled_cases(output_dir));

    entries
}

/// Generate 5 oblique-plane featured cases (F0011-F0015).
///
/// Each case has 2 extrude operations on *different* random oblique planes,
/// testing the kernel's ability to boolean-merge solids along non-aligned directions.
fn generate_oblique_plane_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();

    for i in 0..5u64 {
        let case_id = format!("F{:04}", 11 + i);
        let seed = 7001 + i;
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);

        let scale = 1.0;
        let (origin1, normal1) = random_plane(&mut rng, scale);
        let (origin2, normal2) = random_plane(&mut rng, scale);

        let mut features: Vec<Feature> = Vec::new();

        // Operation 1: extrude(rect, boss) on plane 1
        let w1 = rng.gen_range(0.2..0.6);
        let h1 = rng.gen_range(0.2..0.6);
        let d1 = rng.gen_range(0.1..0.5);
        let sketch1_id = Uuid::new_v4();
        let (entities1, positions1, profiles1) = rect_profile(-w1 / 2.0, -h1 / 2.0, w1, h1);
        features.push(Feature {
            id: sketch1_id,
            name: "Sketch 1".to_string(),
            operation: Operation::Sketch {
                sketch: Sketch {
                    id: sketch1_id,
                    plane: datum_plane_ref(Uuid::new_v4()),
                    plane_origin: origin1,
                    plane_normal: normal1,
                    entities: entities1,
                    constraints: vec![],
                    solve_status: SolveStatus::FullyConstrained,
                    solved_positions: positions1,
                    solved_profiles: profiles1,
                },
            },
            suppressed: false,
            references: vec![],
        });
        features.push(Feature {
            id: Uuid::new_v4(),
            name: "Extrude 1".to_string(),
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: sketch1_id,
                    profile_index: 0,
                    depth: d1,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: true,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                },
            },
            suppressed: false,
            references: vec![],
        });

        // Operation 2: extrude(rect, boss) on plane 2
        let w2 = rng.gen_range(0.2..0.6);
        let h2 = rng.gen_range(0.2..0.6);
        let d2 = rng.gen_range(0.1..0.5);
        let sketch2_id = Uuid::new_v4();
        let (entities2, positions2, profiles2) = rect_profile(-w2 / 2.0, -h2 / 2.0, w2, h2);
        features.push(Feature {
            id: sketch2_id,
            name: "Sketch 2".to_string(),
            operation: Operation::Sketch {
                sketch: Sketch {
                    id: sketch2_id,
                    plane: datum_plane_ref(Uuid::new_v4()),
                    plane_origin: origin2,
                    plane_normal: normal2,
                    entities: entities2,
                    constraints: vec![],
                    solve_status: SolveStatus::FullyConstrained,
                    solved_positions: positions2,
                    solved_profiles: profiles2,
                },
            },
            suppressed: false,
            references: vec![],
        });
        features.push(Feature {
            id: Uuid::new_v4(),
            name: "Extrude 2".to_string(),
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: sketch2_id,
                    profile_index: 0,
                    depth: d2,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: true,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                },
            },
            suppressed: false,
            references: vec![],
        });

        let tree = FeatureTree {
            features,
            active_index: None,
        };

        let metadata = ProjectMetadata::new(format!("Assay {}", case_id));
        let waffle_json = save_project(&tree, &metadata);

        let description = format!(
            "2 ops, scale=1.00e0, extrude(rectangle,boss)+extrude(rectangle,boss) — Oblique planes (seed {})",
            seed
        );

        let meta = AssayMeta {
            id: case_id.clone(),
            description: description.clone(),
            master_seed: 0,
            test_seed: seed,
            scale,
            log_scale: 0.0,
            plane_origin: origin1,
            plane_normal: normal1,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: w1,
                    depth_or_angle: d1,
                    is_cut: false,
                    plane_origin: Some(origin1),
                    plane_normal: Some(normal1),
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: w2,
                    depth_or_angle: d2,
                    is_cut: false,
                    plane_origin: Some(origin2),
                    plane_normal: Some(normal2),
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 3.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "increase".to_string()],
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        let waffle_filename = format!("{}.waffle", case_id);
        let meta_filename = format!("{}.meta.json", case_id);

        let waffle_path = output_dir.join(&waffle_filename);
        fs::write(&waffle_path, &waffle_json).unwrap_or_else(|e| {
            panic!("failed to write {}: {}", waffle_path.display(), e);
        });

        let meta_path = output_dir.join(&meta_filename);
        let meta_json = serde_json::to_string_pretty(&meta).expect("meta serialization failed");
        fs::write(&meta_path, meta_json).unwrap_or_else(|e| {
            panic!("failed to write {}: {}", meta_path.display(), e);
        });

        entries.push(ManifestEntry {
            id: case_id,
            filename: waffle_filename,
            meta_filename,
            description,
            featured: true,
        });
    }

    entries
}

/// Generate 10 intersecting multi-extrude oblique-plane cases (F0016-F0025).
///
/// F0016-F0020: 3-extrude chains, F0021-F0025: 4-extrude chains.
/// All extrudes share origin [0,0,0] with symmetric=true, guaranteeing
/// pairwise intersection (every solid's interior contains the origin).
/// Normals have ≥30° angular separation via rejection sampling.
fn generate_intersecting_oblique_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    let min_angle = std::f64::consts::FRAC_PI_6; // 30°

    // (case_index, extrude_count, seed)
    let specs: [(u64, usize, u64); 10] = [
        (16, 3, 8001),
        (17, 3, 8002),
        (18, 3, 8003),
        (19, 3, 8004),
        (20, 3, 8005),
        (21, 4, 8006),
        (22, 4, 8007),
        (23, 4, 8008),
        (24, 4, 8009),
        (25, 4, 8010),
    ];

    for &(case_num, extrude_count, seed) in &specs {
        let case_id = format!("F{:04}", case_num);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);

        let normals = generate_well_separated_normals(&mut rng, extrude_count, min_angle);
        let plane_origin = [0.0, 0.0, 0.0];

        let mut features: Vec<Feature> = Vec::new();
        let mut op_metas: Vec<OpMeta> = Vec::new();
        let mut volume_monotonicity: Vec<String> = Vec::new();

        for (j, normal) in normals.iter().enumerate() {
            let w: f64 = rng.gen_range(0.15..0.5);
            let h: f64 = rng.gen_range(0.15..0.5);
            let d: f64 = rng.gen_range(0.2..0.6);

            let sketch_id = Uuid::new_v4();
            let (entities, positions, profiles) = rect_profile(-w / 2.0, -h / 2.0, w, h);

            features.push(Feature {
                id: sketch_id,
                name: format!("Sketch {}", j + 1),
                operation: Operation::Sketch {
                    sketch: Sketch {
                        id: sketch_id,
                        plane: datum_plane_ref(Uuid::new_v4()),
                        plane_origin,
                        plane_normal: *normal,
                        entities,
                        constraints: vec![],
                        solve_status: SolveStatus::FullyConstrained,
                        solved_positions: positions,
                        solved_profiles: profiles,
                    },
                },
                suppressed: false,
                references: vec![],
            });

            features.push(Feature {
                id: Uuid::new_v4(),
                name: format!("Extrude {}", j + 1),
                operation: Operation::Extrude {
                    params: ExtrudeParams {
                        sketch_id,
                        profile_index: 0,
                        depth: d,
                        direction: None,
                        symmetric: true,
                        cut: false,
                        merge: true,
                        target_body: None,
                        depth_mode: DepthMode::Blind,
                        second_direction: None,
                    },
                },
                suppressed: false,
                references: vec![],
            });

            op_metas.push(OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: w,
                depth_or_angle: d,
                is_cut: false,
                plane_origin: Some(plane_origin),
                plane_normal: Some(*normal),
            });

            volume_monotonicity.push("increase".to_string());
        }

        let tree = FeatureTree {
            features,
            active_index: None,
        };

        let metadata = ProjectMetadata::new(format!("Assay {}", case_id));
        let waffle_json = save_project(&tree, &metadata);

        let description = format!(
            "{} ops, scale=1.00e0, {} — Intersecting oblique (seed {})",
            extrude_count,
            (0..extrude_count)
                .map(|_| "extrude(rectangle,boss)")
                .collect::<Vec<_>>()
                .join("+"),
            seed
        );

        let meta = AssayMeta {
            id: case_id.clone(),
            description: description.clone(),
            master_seed: 0,
            test_seed: seed,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin,
            plane_normal: normals[0],
            operations: op_metas,
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 4.0,
                expect_positive_volume: true,
                volume_monotonicity,
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        let waffle_filename = format!("{}.waffle", case_id);
        let meta_filename = format!("{}.meta.json", case_id);

        let waffle_path = output_dir.join(&waffle_filename);
        fs::write(&waffle_path, &waffle_json).unwrap_or_else(|e| {
            panic!("failed to write {}: {}", waffle_path.display(), e);
        });

        let meta_path = output_dir.join(&meta_filename);
        let meta_json = serde_json::to_string_pretty(&meta).expect("meta serialization failed");
        fs::write(&meta_path, meta_json).unwrap_or_else(|e| {
            panic!("failed to write {}: {}", meta_path.display(), e);
        });

        entries.push(ManifestEntry {
            id: case_id,
            filename: waffle_filename,
            meta_filename,
            description,
            featured: true,
        });
    }

    entries
}

// ── Boolean-Path-Targeting Featured Cases (F0026-F0055) ───────────────────

/// Helper: write a featured case to disk and return its manifest entry.
fn write_featured_case(
    output_dir: &std::path::Path,
    case_id: &str,
    features: Vec<Feature>,
    meta: AssayMeta,
) -> ManifestEntry {
    let tree = FeatureTree {
        features,
        active_index: None,
    };
    let metadata = ProjectMetadata::new(format!("Assay {}", case_id));
    let waffle_json = save_project(&tree, &metadata);

    let waffle_filename = format!("{}.waffle", case_id);
    let meta_filename = format!("{}.meta.json", case_id);

    let waffle_path = output_dir.join(&waffle_filename);
    fs::write(&waffle_path, &waffle_json)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", waffle_path.display(), e));

    let meta_path = output_dir.join(&meta_filename);
    let meta_json = serde_json::to_string_pretty(&meta).expect("meta serialization failed");
    fs::write(&meta_path, meta_json)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", meta_path.display(), e));

    ManifestEntry {
        id: case_id.to_string(),
        filename: waffle_filename,
        meta_filename,
        description: meta.description,
        featured: true,
    }
}

/// Helper: build a sketch + extrude feature pair.
#[allow(clippy::too_many_arguments)]
fn build_sketch_extrude(
    sketch_name: &str,
    extrude_name: &str,
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    profile_data: ProfileData,
    depth: f64,
    cut: bool,
    symmetric: bool,
) -> (Uuid, Vec<Feature>) {
    let sketch_id = Uuid::new_v4();
    let (entities, positions, profiles) = profile_data;

    let sketch_feature = Feature {
        id: sketch_id,
        name: sketch_name.to_string(),
        operation: Operation::Sketch {
            sketch: Sketch {
                id: sketch_id,
                plane: datum_plane_ref(Uuid::new_v4()),
                plane_origin,
                plane_normal,
                entities,
                constraints: vec![],
                solve_status: SolveStatus::FullyConstrained,
                solved_positions: positions,
                solved_profiles: profiles,
            },
        },
        suppressed: false,
        references: vec![],
    };

    let extrude_feature = Feature {
        id: Uuid::new_v4(),
        name: extrude_name.to_string(),
        operation: Operation::Extrude {
            params: ExtrudeParams {
                sketch_id,
                profile_index: 0,
                depth,
                direction: None,
                symmetric,
                cut,
                merge: true,
                target_body: None,
                depth_mode: DepthMode::Blind,
                second_direction: None,
            },
        },
        suppressed: false,
        references: vec![],
    };

    (sketch_id, vec![sketch_feature, extrude_feature])
}

/// F0026-F0030: Circle boss on top of box — exercises `box_cyl_boolean` Union path.
///
/// Box base (rect extrude), then circle boss extending above top face.
/// Circle radius < min(box_w, box_h)/2 ensures cylinder is enclosed in XY.
fn generate_circle_boss_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    let origin = [0.0, 0.0, 0.0];
    let normal = [0.0, 0.0, 1.0];

    for i in 0..5u64 {
        let case_id = format!("F{:04}", 26 + i);
        let seed = 9001 + i;
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);

        let box_w: f64 = rng.gen_range(0.3..0.6);
        let box_h: f64 = rng.gen_range(0.3..0.6);
        let box_d: f64 = rng.gen_range(0.2..0.5);
        let max_radius = box_w.min(box_h) / 2.0 * 0.8;
        let cyl_r: f64 = rng.gen_range(0.05..max_radius);
        let cyl_d: f64 = rng.gen_range(0.1..0.4);

        let mut features = Vec::new();
        let (_, box_feats) = build_sketch_extrude(
            "Sketch 1",
            "Extrude 1",
            origin,
            normal,
            rect_profile(-box_w / 2.0, -box_h / 2.0, box_w, box_h),
            box_d,
            false,
            false,
        );
        features.extend(box_feats);
        let boss_origin = [origin[0], origin[1], origin[2] + box_d];
        let (_, cyl_feats) = build_sketch_extrude(
            "Sketch 2",
            "Extrude 2",
            boss_origin,
            normal,
            true_circle_profile(0.0, 0.0, cyl_r),
            cyl_d,
            false,
            false,
        );
        features.extend(cyl_feats);

        let description = format!(
            "2 ops, scale=1.00e0, extrude(rectangle,boss)+extrude(circle,boss) — Circle boss (seed {})",
            seed
        );

        let meta = AssayMeta {
            id: case_id.clone(),
            description: description.clone(),
            master_seed: 0,
            test_seed: seed,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: origin,
            plane_normal: normal,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: box_w,
                    depth_or_angle: box_d,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "circle".to_string(),
                    profile_size: cyl_r,
                    depth_or_angle: cyl_d,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 3.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "increase".to_string()],
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        entries.push(write_featured_case(output_dir, &case_id, features, meta));
    }

    entries
}

/// F0031-F0035: Box minus enclosed cylinder — exercises `build_box_minus_enclosed_cyl`.
///
/// Box base, then symmetric circle cut through box. Circle radius < min(box_w, box_h)/2
/// ensures the cylinder is fully enclosed in XY projection.
fn generate_box_minus_cyl_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    let origin = [0.0, 0.0, 0.0];
    let normal = [0.0, 0.0, 1.0];

    for i in 0..5u64 {
        let case_id = format!("F{:04}", 31 + i);
        let seed = 9006 + i;
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);

        let box_w: f64 = rng.gen_range(0.4..0.8);
        let box_h: f64 = rng.gen_range(0.4..0.8);
        let box_d: f64 = rng.gen_range(0.3..0.6);
        let max_radius = box_w.min(box_h) / 2.0 * 0.7;
        let cyl_r: f64 = rng.gen_range(0.05..max_radius);
        // Cylinder must be fully enclosed in box Z-range for build_box_minus_enclosed_cyl
        let cyl_d: f64 = box_d * rng.gen_range(0.5..0.9);

        let mut features = Vec::new();
        let (_, box_feats) = build_sketch_extrude(
            "Sketch 1",
            "Extrude 1",
            origin,
            normal,
            rect_profile(-box_w / 2.0, -box_h / 2.0, box_w, box_h),
            box_d,
            false,
            false,
        );
        features.extend(box_feats);
        // Center cylinder in box Z-range so it's fully enclosed
        let z_offset = (box_d - cyl_d) / 2.0;
        let cut_origin = [origin[0], origin[1], origin[2] + z_offset];
        let (_, cyl_feats) = build_sketch_extrude(
            "Sketch 2",
            "Extrude 2",
            cut_origin,
            normal,
            true_circle_profile(0.0, 0.0, cyl_r),
            cyl_d,
            true,
            false,
        );
        features.extend(cyl_feats);

        let description = format!(
            "2 ops, scale=1.00e0, extrude(rectangle,boss)+extrude(circle,cut) — Box-minus-cyl (seed {})",
            seed
        );

        let meta = AssayMeta {
            id: case_id.clone(),
            description: description.clone(),
            master_seed: 0,
            test_seed: seed,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: origin,
            plane_normal: normal,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: box_w,
                    depth_or_angle: box_d,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "circle".to_string(),
                    profile_size: cyl_r,
                    depth_or_angle: cyl_d,
                    is_cut: true,
                    plane_origin: None,
                    plane_normal: None,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 3.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "decrease".to_string()],
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        entries.push(write_featured_case(output_dir, &case_id, features, meta));
    }

    entries
}

/// F0036-F0040: Cylinder minus enclosed box — exercises `cyl_minus_box_boolean`.
///
/// Circle base, then rectangle cut. Box diagonal < cylinder diameter ensures
/// the box is fully enclosed in the cylinder's XY projection.
fn generate_cyl_minus_box_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    let origin = [0.0, 0.0, 0.0];
    let normal = [0.0, 0.0, 1.0];

    for i in 0..5u64 {
        let case_id = format!("F{:04}", 36 + i);
        let seed = 9011 + i;
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);

        let cyl_r: f64 = rng.gen_range(0.3..0.6);
        let cyl_d: f64 = rng.gen_range(0.3..0.6);
        // Box diagonal must be < cylinder diameter (2*r)
        // For a box w×h centered at origin, diagonal = sqrt(w²+h²) < 2r
        // Use w=h=r*0.8 → diagonal = r*0.8*sqrt(2) ≈ r*1.13 < 2r ✓
        let box_max = cyl_r * 0.8;
        let box_w: f64 = rng.gen_range(box_max * 0.4..box_max);
        let box_h: f64 = rng.gen_range(box_max * 0.4..box_max);
        // Box must be fully enclosed in cylinder Z-range for build_cyl_minus_enclosed_box
        let box_d: f64 = cyl_d * rng.gen_range(0.5..0.9);

        let mut features = Vec::new();
        let (_, cyl_feats) = build_sketch_extrude(
            "Sketch 1",
            "Extrude 1",
            origin,
            normal,
            true_circle_profile(0.0, 0.0, cyl_r),
            cyl_d,
            false,
            false,
        );
        features.extend(cyl_feats);
        // Center box in cylinder Z-range so it's fully enclosed
        let z_offset = (cyl_d - box_d) / 2.0;
        let cut_origin = [origin[0], origin[1], origin[2] + z_offset];
        let (_, box_feats) = build_sketch_extrude(
            "Sketch 2",
            "Extrude 2",
            cut_origin,
            normal,
            rect_profile(-box_w / 2.0, -box_h / 2.0, box_w, box_h),
            box_d,
            true,
            false,
        );
        features.extend(box_feats);

        let description = format!(
            "2 ops, scale=1.00e0, extrude(circle,boss)+extrude(rectangle,cut) — Cyl-minus-box (seed {})",
            seed
        );

        let meta = AssayMeta {
            id: case_id.clone(),
            description: description.clone(),
            master_seed: 0,
            test_seed: seed,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: origin,
            plane_normal: normal,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "circle".to_string(),
                    profile_size: cyl_r,
                    depth_or_angle: cyl_d,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: box_w,
                    depth_or_angle: box_d,
                    is_cut: true,
                    plane_origin: None,
                    plane_normal: None,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 3.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "decrease".to_string()],
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        entries.push(write_featured_case(output_dir, &case_id, features, meta));
    }

    entries
}

/// F0041-F0045: Cylinder-cylinder parallel — exercises `cyl_cyl_boolean` path.
///
/// Both profiles are circles on the XY plane (parallel axes).
/// Mixed boss/cut configurations.
fn generate_cyl_cyl_parallel_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    let origin = [0.0, 0.0, 0.0];
    let normal = [0.0, 0.0, 1.0];

    for i in 0..5u64 {
        let case_id = format!("F{:04}", 41 + i);
        let seed = 9016 + i;
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);

        let r1: f64 = rng.gen_range(0.2..0.5);
        let d1: f64 = rng.gen_range(0.2..0.5);
        let r2: f64 = rng.gen_range(0.1..0.4);
        let d2: f64 = rng.gen_range(0.2..0.5);
        // Offset second cylinder so they overlap but aren't concentric
        let offset_x: f64 = rng.gen_range(0.0..(r1 + r2) * 0.6);
        let offset_y: f64 = rng.gen_range(0.0..(r1 + r2) * 0.3);
        // Alternate boss/cut: even = boss+boss, odd = boss+cut
        let is_cut_2 = i % 2 == 1;

        let mut features = Vec::new();
        let (_, cyl1_feats) = build_sketch_extrude(
            "Sketch 1",
            "Extrude 1",
            origin,
            normal,
            true_circle_profile(0.0, 0.0, r1),
            d1,
            false,
            false,
        );
        features.extend(cyl1_feats);
        let (cyl2_origin, cyl2_d, cyl2_symmetric) = if is_cut_2 {
            // Cut: place second cylinder centered in first's Z-range, fully enclosed
            let cd = d1 * rng.gen_range(0.5..0.9);
            let z_off = (d1 - cd) / 2.0;
            ([origin[0], origin[1], origin[2] + z_off], cd, false)
        } else {
            // Boss: overlap by 20% so boolean detects Z overlap
            ([origin[0], origin[1], origin[2] + d1 * 0.8], d2, false)
        };
        let (_, cyl2_feats) = build_sketch_extrude(
            "Sketch 2",
            "Extrude 2",
            cyl2_origin,
            normal,
            true_circle_profile(offset_x, offset_y, r2),
            cyl2_d,
            is_cut_2,
            cyl2_symmetric,
        );
        features.extend(cyl2_feats);

        let cut_label = if is_cut_2 { "cut" } else { "boss" };
        let description = format!(
            "2 ops, scale=1.00e0, extrude(circle,boss)+extrude(circle,{}) — Cyl-cyl parallel (seed {})",
            cut_label, seed
        );

        let mono_2 = if is_cut_2 { "decrease" } else { "increase" };
        let meta = AssayMeta {
            id: case_id.clone(),
            description: description.clone(),
            master_seed: 0,
            test_seed: seed,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: origin,
            plane_normal: normal,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "circle".to_string(),
                    profile_size: r1,
                    depth_or_angle: d1,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "circle".to_string(),
                    profile_size: r2,
                    depth_or_angle: d2,
                    is_cut: is_cut_2,
                    plane_origin: None,
                    plane_normal: None,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 3.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), mono_2.to_string()],
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        entries.push(write_featured_case(output_dir, &case_id, features, meta));
    }

    entries
}

/// F0046-F0050: Mixed cross-plane — rect on plane1, circle on plane2.
///
/// Exercises `box_cyl_boolean` with frame rotation from non-aligned planes.
/// Uses well-separated normals and shared origin with symmetric extrudes.
fn generate_mixed_cross_plane_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    let min_angle = std::f64::consts::FRAC_PI_6; // 30°

    for i in 0..5u64 {
        let case_id = format!("F{:04}", 46 + i);
        let seed = 9021 + i;
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);

        let normals = generate_well_separated_normals(&mut rng, 2, min_angle);
        let origin = [0.0, 0.0, 0.0];
        let normal1 = normals[0];
        let normal2 = normals[1];

        let box_w: f64 = rng.gen_range(0.3..0.6);
        let box_h: f64 = rng.gen_range(0.3..0.6);
        let box_d: f64 = rng.gen_range(0.2..0.5);
        let cyl_r: f64 = rng.gen_range(0.1..0.3);
        let cyl_d: f64 = rng.gen_range(0.2..0.5);

        let mut features = Vec::new();
        let (_, box_feats) = build_sketch_extrude(
            "Sketch 1",
            "Extrude 1",
            origin,
            normal1,
            rect_profile(-box_w / 2.0, -box_h / 2.0, box_w, box_h),
            box_d,
            false,
            true,
        );
        features.extend(box_feats);
        let (_, cyl_feats) = build_sketch_extrude(
            "Sketch 2",
            "Extrude 2",
            origin,
            normal2,
            true_circle_profile(0.0, 0.0, cyl_r),
            cyl_d,
            false,
            true,
        );
        features.extend(cyl_feats);

        let description = format!(
            "2 ops, scale=1.00e0, extrude(rectangle,boss)+extrude(circle,boss) — Mixed cross-plane (seed {})",
            seed
        );

        let meta = AssayMeta {
            id: case_id.clone(),
            description: description.clone(),
            master_seed: 0,
            test_seed: seed,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: origin,
            plane_normal: normal1,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: box_w,
                    depth_or_angle: box_d,
                    is_cut: false,
                    plane_origin: Some(origin),
                    plane_normal: Some(normal1),
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "circle".to_string(),
                    profile_size: cyl_r,
                    depth_or_angle: cyl_d,
                    is_cut: false,
                    plane_origin: Some(origin),
                    plane_normal: Some(normal2),
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 3.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "increase".to_string()],
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        entries.push(write_featured_case(output_dir, &case_id, features, meta));
    }

    entries
}

/// F0051-F0055: Scale extremes — tessellation tolerance scaling at 1e-4 and 1e4.
///
/// Tests that boolean + tessellation works at extreme scales.
fn generate_scale_extreme_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    let origin = [0.0, 0.0, 0.0];
    let normal = [0.0, 0.0, 1.0];

    // (case_id_offset, scale, profile1, profile2, cut2)
    let specs: [(u64, f64, &str, &str, bool); 5] = [
        (51, 1e-4, "rectangle", "rectangle", false),
        (52, 1e-4, "circle", "circle", false),
        (53, 1e4, "rectangle", "rectangle", false),
        (54, 1e4, "circle", "rectangle", true),
        (55, 1e4, "rectangle", "circle", true),
    ];

    for &(case_num, scale, prof1, prof2, is_cut_2) in &specs {
        let case_id = format!("F{:04}", case_num);

        let base_size = scale * 0.3;
        let second_size = scale * 0.15;
        let depth1 = scale * 0.2;
        let depth2 = scale * 0.15;

        let profile1_data = if prof1 == "rectangle" {
            rect_profile(-base_size / 2.0, -base_size / 2.0, base_size, base_size)
        } else {
            true_circle_profile(0.0, 0.0, base_size)
        };
        let profile2_data = if prof2 == "rectangle" {
            rect_profile(
                -second_size / 2.0,
                -second_size / 2.0,
                second_size,
                second_size,
            )
        } else {
            true_circle_profile(0.0, 0.0, second_size)
        };

        let mut features = Vec::new();
        let (_, feats1) = build_sketch_extrude(
            "Sketch 1",
            "Extrude 1",
            origin,
            normal,
            profile1_data,
            depth1,
            false,
            false,
        );
        features.extend(feats1);
        let (_, feats2) = build_sketch_extrude(
            "Sketch 2",
            "Extrude 2",
            origin,
            normal,
            profile2_data,
            depth2,
            is_cut_2,
            false,
        );
        features.extend(feats2);

        let cut_label = if is_cut_2 { "cut" } else { "boss" };
        let description = format!(
            "2 ops, scale={:.2e}, extrude({},boss)+extrude({},{}) — Scale extreme",
            scale, prof1, prof2, cut_label
        );

        let size1 = base_size;
        let size2 = second_size;
        let mono_2 = if is_cut_2 { "decrease" } else { "increase" };

        let meta = AssayMeta {
            id: case_id.clone(),
            description: description.clone(),
            master_seed: 0,
            test_seed: 0,
            scale,
            log_scale: scale.log10(),
            plane_origin: origin,
            plane_normal: normal,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: prof1.to_string(),
                    profile_size: size1,
                    depth_or_angle: depth1,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: prof2.to_string(),
                    profile_size: size2,
                    depth_or_angle: depth2,
                    is_cut: is_cut_2,
                    plane_origin: None,
                    plane_normal: None,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: scale * 3.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), mono_2.to_string()],
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        entries.push(write_featured_case(output_dir, &case_id, features, meta));
    }

    entries
}

/// F0056-F0060: Perpendicular cylinder-cylinder booleans (non-parallel axes).
///
/// Tests the analytical SSI solver for equal-radius cylinders at 90°.
/// Two cylinders extruded along different axis-aligned directions.
#[allow(clippy::type_complexity)]
fn generate_cyl_cyl_angled_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();

    // Each case: two circles on perpendicular planes → cyl-cyl boolean at 90°.
    // (case_id_offset, plane1_normal, plane2_normal, r, d, is_cut)
    let specs: [(u64, [f64; 3], [f64; 3], f64, f64, bool); 5] = [
        (56, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 0.3, 0.4, false), // XZ perp, boss+boss
        (57, [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], 0.25, 0.35, false), // YZ perp, boss+boss
        (58, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 0.2, 0.3, true),  // XZ perp, boss+cut
        (59, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], 0.35, 0.25, false), // XY perp, boss+boss
        (60, [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], 0.3, 0.3, true),  // YZ perp, boss+cut
    ];

    for &(case_num, n1, n2, r, d, is_cut_2) in &specs {
        let case_id = format!("F{:04}", case_num);
        let origin = [0.0, 0.0, 0.0];

        let mut features = Vec::new();
        let (_, feats1) = build_sketch_extrude(
            "Sketch 1",
            "Extrude 1",
            origin,
            n1,
            true_circle_profile(0.0, 0.0, r),
            d,
            false,
            true, // symmetric so both cyls pass through origin
        );
        features.extend(feats1);
        let (_, feats2) = build_sketch_extrude(
            "Sketch 2",
            "Extrude 2",
            origin,
            n2,
            true_circle_profile(0.0, 0.0, r),
            d,
            is_cut_2,
            true,
        );
        features.extend(feats2);

        let cut_label = if is_cut_2 { "cut" } else { "boss" };
        let mono_2 = if is_cut_2 { "decrease" } else { "increase" };
        let description = format!(
            "2 ops, scale=1.00e0, extrude(circle,boss)+extrude(circle,{}) — Cyl-cyl angled 90° (F{})",
            cut_label, case_num
        );

        let meta = AssayMeta {
            id: case_id.clone(),
            description: description.clone(),
            master_seed: 0,
            test_seed: 0,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: origin,
            plane_normal: n1,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "circle".to_string(),
                    profile_size: r,
                    depth_or_angle: d,
                    is_cut: false,
                    plane_origin: Some(origin),
                    plane_normal: Some(n1),
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "circle".to_string(),
                    profile_size: r,
                    depth_or_angle: d,
                    is_cut: is_cut_2,
                    plane_origin: Some(origin),
                    plane_normal: Some(n2),
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 3.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), mono_2.to_string()],
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        entries.push(write_featured_case(output_dir, &case_id, features, meta));
    }

    entries
}

// ── Corpus Generation ──────────────────────────────────────────────────────

/// Generate a full corpus of test cases and write them to disk.
pub fn generate_corpus(config: &CorpusConfig) -> CorpusStats {
    fs::create_dir_all(&config.output_dir).expect("failed to create output directory");

    let mut manifest_entries = Vec::new();
    let mut extrude_count = 0usize;
    let mut revolve_count = 0usize;

    for i in 0..config.case_count {
        let case = generate_case(config.master_seed, i);

        // Count operation types
        for op in &case.meta.operations {
            match op.kind.as_str() {
                "extrude" => extrude_count += 1,
                "revolve" => revolve_count += 1,
                _ => {}
            }
        }

        let waffle_filename = format!("{}.waffle", case.id);
        let meta_filename = format!("{}.meta.json", case.id);

        // Write .waffle file
        let waffle_path = config.output_dir.join(&waffle_filename);
        fs::write(&waffle_path, &case.waffle_json).unwrap_or_else(|e| {
            panic!("failed to write {}: {}", waffle_path.display(), e);
        });

        // Write .meta.json sidecar
        let meta_path = config.output_dir.join(&meta_filename);
        let meta_json =
            serde_json::to_string_pretty(&case.meta).expect("meta serialization failed");
        fs::write(&meta_path, meta_json).unwrap_or_else(|e| {
            panic!("failed to write {}: {}", meta_path.display(), e);
        });

        manifest_entries.push(ManifestEntry {
            id: case.id,
            filename: waffle_filename,
            meta_filename,
            description: case.meta.description,
            featured: false,
        });
    }

    // Generate featured cases and append to manifest
    let featured_entries = generate_featured_cases(&config.output_dir);
    let featured_count = featured_entries.len();
    manifest_entries.extend(featured_entries);

    // Write manifest.json
    let manifest = CorpusManifest {
        master_seed: config.master_seed,
        count: config.case_count + featured_count,
        generator_version: GENERATOR_VERSION,
        cases: manifest_entries,
    };
    let manifest_path = config.output_dir.join("manifest.json");
    let manifest_json =
        serde_json::to_string_pretty(&manifest).expect("manifest serialization failed");
    fs::write(&manifest_path, manifest_json).unwrap_or_else(|e| {
        panic!("failed to write {}: {}", manifest_path.display(), e);
    });

    CorpusStats {
        count: config.case_count + featured_count,
        // F0001-F0010: 10×2=20, F0011-F0015: 5×2=10, F0016-F0020: 5×3=15, F0021-F0025: 5×4=20,
        // F0026-F0030: 5×2=10, F0031-F0035: 5×2=10, F0036-F0040: 5×2=10,
        // F0041-F0045: 5×2=10, F0046-F0050: 5×2=10, F0051-F0055: 5×2=10, F0056-F0060: 5×2=10 = 135
        extrude_count: extrude_count + 135,
        revolve_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_seed_deterministic() {
        let s1 = derive_seed(42, 0);
        let s2 = derive_seed(42, 0);
        assert_eq!(s1, s2);
    }

    #[test]
    fn derive_seed_varies_by_index() {
        let s0 = derive_seed(42, 0);
        let s1 = derive_seed(42, 1);
        let s2 = derive_seed(42, 2);
        assert_ne!(s0, s1);
        assert_ne!(s1, s2);
    }

    #[test]
    fn random_scale_in_range() {
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(123);
        for _ in 0..100 {
            let s = random_scale(&mut rng);
            assert!(s >= 1e-4 && s <= 1e4, "scale {} out of range", s);
        }
    }

    #[test]
    fn random_plane_produces_unit_normal() {
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(456);
        for _ in 0..50 {
            let (_, normal) = random_plane(&mut rng, 1.0);
            let len =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-10,
                "normal should be unit length, got {}",
                len
            );
        }
    }

    #[test]
    fn true_circle_profile_structure() {
        let (entities, positions, profiles) = true_circle_profile(0.0, 0.0, 5.0);
        assert_eq!(entities.len(), 2); // center point + circle
        assert_eq!(positions.len(), 1); // just center
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].circle.is_some());
        let circle = profiles[0].circle.as_ref().unwrap();
        assert!((circle.radius - 5.0).abs() < 1e-10);
    }

    #[test]
    fn generate_case_deterministic() {
        let case1 = generate_case(42, 0);
        let case2 = generate_case(42, 0);
        assert_eq!(case1.id, case2.id);
        assert_eq!(case1.meta.test_seed, case2.meta.test_seed);
        assert_eq!(case1.meta.scale, case2.meta.scale);
        assert_eq!(case1.meta.operations.len(), case2.meta.operations.len());
    }

    #[test]
    fn generate_case_produces_valid_waffle() {
        let case = generate_case(42, 0);
        // Should be valid JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&case.waffle_json).expect("waffle should be valid JSON");
        assert_eq!(parsed["format"], "waffle-iron");
        assert_eq!(parsed["version"], 3);
        // v3 uses tabs instead of top-level features
        assert!(parsed["tabs"].is_array());
        let tab = &parsed["tabs"][0];
        assert_eq!(tab["kind"]["type"], "Part");
        assert!(tab["kind"]["features"]["features"].is_array());
    }

    #[test]
    fn generate_case_has_2_or_3_operations() {
        for i in 0..20 {
            let case = generate_case(42, i);
            let op_count = case.meta.operations.len();
            assert!(
                op_count >= 2 && op_count <= 3,
                "case {} has {} ops, expected 2-3",
                i,
                op_count
            );
        }
    }

    #[test]
    fn first_operation_is_always_boss() {
        for i in 0..20 {
            let case = generate_case(42, i);
            assert!(
                !case.meta.operations[0].is_cut,
                "case {} first op should be boss",
                i
            );
        }
    }

    #[test]
    fn meta_serde_roundtrip() {
        let case = generate_case(42, 0);
        let json = serde_json::to_string_pretty(&case.meta).unwrap();
        let deserialized: AssayMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, case.meta.id);
        assert_eq!(deserialized.test_seed, case.meta.test_seed);
        assert_eq!(deserialized.operations.len(), case.meta.operations.len());
    }

    #[test]
    fn multi_plane_distribution() {
        // Over 100 cases, roughly 50% should have per-op plane data (multi_plane=true)
        let mut multi_count = 0;
        for i in 0..100 {
            let case = generate_case(42, i);
            let has_per_op_plane = case
                .meta
                .operations
                .iter()
                .any(|op| op.plane_origin.is_some());
            if has_per_op_plane {
                multi_count += 1;
            }
        }
        // Expect roughly 50% ± margin. With 100 samples, 30-70 is very safe.
        assert!(
            multi_count >= 25 && multi_count <= 75,
            "multi-plane count {} out of expected range [25, 75]",
            multi_count
        );
    }

    #[test]
    fn multi_plane_cases_have_separated_normals() {
        for i in 0..100 {
            let case = generate_case(42, i);
            let ops_with_planes: Vec<_> = case
                .meta
                .operations
                .iter()
                .filter_map(|op| op.plane_normal)
                .collect();
            if ops_with_planes.len() >= 2 {
                // Check angular separation between per-op normals and case-level normal
                for per_op_n in &ops_with_planes {
                    let cn = case.meta.plane_normal;
                    let dot = per_op_n[0] * cn[0] + per_op_n[1] * cn[1] + per_op_n[2] * cn[2];
                    // min 30° separation means |dot| <= cos(30°) ≈ 0.866
                    assert!(
                        dot.abs() <= 0.87,
                        "case R{:04} per-op normal not well-separated: dot={}",
                        i + 1,
                        dot
                    );
                }
            }
        }
    }

    #[test]
    fn featured_case_ids_f0026_to_f0060() {
        let dir = tempfile::tempdir().unwrap();
        let entries = generate_featured_cases(dir.path());
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        // Check that F0026-F0060 are all present
        for n in 26..=60 {
            let expected = format!("F{:04}", n);
            assert!(
                ids.contains(&expected.as_str()),
                "missing featured case {}",
                expected
            );
        }
        // Total: F0001-F0010 (10) + F0011-F0015 (5) + F0016-F0025 (10) + F0026-F0060 (35) = 60
        assert_eq!(entries.len(), 60, "expected 60 featured cases");
    }

    #[test]
    fn circle_boss_cases_have_correct_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let entries = generate_circle_boss_cases(dir.path());
        assert_eq!(entries.len(), 5);
        for entry in &entries {
            let meta_path = dir.path().join(&entry.meta_filename);
            let meta_json = fs::read_to_string(meta_path).unwrap();
            let meta: AssayMeta = serde_json::from_str(&meta_json).unwrap();
            assert_eq!(meta.operations[0].profile_type, "rectangle");
            assert_eq!(meta.operations[1].profile_type, "circle");
            assert!(!meta.operations[0].is_cut);
            assert!(!meta.operations[1].is_cut);
        }
    }

    #[test]
    fn box_minus_cyl_cases_have_cut_flag() {
        let dir = tempfile::tempdir().unwrap();
        let entries = generate_box_minus_cyl_cases(dir.path());
        assert_eq!(entries.len(), 5);
        for entry in &entries {
            let meta_path = dir.path().join(&entry.meta_filename);
            let meta_json = fs::read_to_string(meta_path).unwrap();
            let meta: AssayMeta = serde_json::from_str(&meta_json).unwrap();
            assert_eq!(meta.operations[0].profile_type, "rectangle");
            assert_eq!(meta.operations[1].profile_type, "circle");
            assert!(meta.operations[1].is_cut);
        }
    }

    #[test]
    fn cyl_minus_box_cases_geometry() {
        let dir = tempfile::tempdir().unwrap();
        let entries = generate_cyl_minus_box_cases(dir.path());
        assert_eq!(entries.len(), 5);
        for entry in &entries {
            let meta_path = dir.path().join(&entry.meta_filename);
            let meta_json = fs::read_to_string(meta_path).unwrap();
            let meta: AssayMeta = serde_json::from_str(&meta_json).unwrap();
            assert_eq!(meta.operations[0].profile_type, "circle");
            assert_eq!(meta.operations[1].profile_type, "rectangle");
            assert!(meta.operations[1].is_cut);
        }
    }

    #[test]
    fn generator_version_is_2() {
        assert_eq!(GENERATOR_VERSION, 2);
    }
}
