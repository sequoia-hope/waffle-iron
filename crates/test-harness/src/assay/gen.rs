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
pub const GENERATOR_VERSION: u32 = 1;

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

    // 2-3 operations per case
    let op_count: usize = rng.gen_range(2..=3);
    let mut features: Vec<Feature> = Vec::new();
    let mut op_metas: Vec<OpMeta> = Vec::new();
    let mut volume_monotonicity: Vec<String> = Vec::new();

    for i in 0..op_count {
        let is_first = i == 0;
        let (primitive_data, profile_type, profile_size) = random_sketch_primitive(&mut rng, scale);
        let (op_kind, depth_or_angle, is_cut) = random_operation(&mut rng, scale, is_first);

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
                            plane_origin[0] + axis_offset * plane_normal[1],
                            plane_origin[1] - axis_offset * plane_normal[0],
                            plane_origin[2],
                        ],
                        axis_direction: [plane_normal[1], -plane_normal[0], 0.0],
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
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: spec.w2,
                    depth_or_angle: spec.d2,
                    is_cut: false,
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
        extrude_count: extrude_count + featured_count * 2, // each featured has 2 extrudes
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
        assert_eq!(parsed["version"], 2);
        assert!(parsed["features"]["features"].is_array());
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
}
