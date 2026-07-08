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

use crate::helpers::{datum_plane_ref, polygon_profile, rect_profile, ProfileData};
use feature_engine::types::{
    DepthMode, ExtrudeParams, Feature, FeatureTree, Operation, RevolveParams,
};
use file_format::metadata::ProjectMetadata;
use file_format::save::save_project;
use waffle_types::{CircleProfile, ClosedProfile, Sketch, SketchEntity, SolveStatus};

/// Generator version — bump when output format changes.
pub const GENERATOR_VERSION: u32 = 4;

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
    /// If true, the rebuild is expected to produce an engine error (e.g., disjoint union).
    /// When set, the runner skips mesh oracle checks and passes if errors are present.
    #[serde(default)]
    pub expect_rebuild_error: bool,
    /// Exact analytic volume of the final body/bodies (C-series). Computed at
    /// generation time from kernel-independent arithmetic (shoelace × depth,
    /// axis-aligned inclusion–exclusion) so the check is not circular.
    /// `None` (all legacy metas) ⇒ only the magnitude-range check applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_volume: Option<f64>,
    /// Relative tolerance for `expected_volume`; default 1e-3 when absent.
    /// Curved-profile cases set 0.05 (tessellation chord error).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_volume_tol_rel: Option<f64>,
    /// Deliberate multi-body cases declare their intended final body count.
    /// `None` ⇒ the legacy "multi-op ⇒ 1 merged solid" check applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_solid_count: Option<usize>,
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
        if !(1e-12..=1.0).contains(&len_sq) {
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
            // Gear: 8-24 teeth — stored as compact GearParams, expanded on demand
            let teeth: u32 = rng.gen_range(8..=24);
            let module_val = scale * 0.05; // makes pitch_radius ~ teeth * module / 2
            let pitch_radius = (teeth as f64) * module_val / 2.0;
            let params = waffle_types::GearParams {
                tooth_count: teeth,
                module: module_val,
                pressure_angle_deg: 20.0,
                ..Default::default()
            };
            let data = (
                vec![waffle_types::SketchEntity::Gear {
                    id: 1,
                    params,
                    construction: false,
                }],
                std::collections::HashMap::new(),
                vec![],
            );
            (data, "gear".to_string(), pitch_radius)
        }
    }
}

/// Build a true circle profile (not a polygon approximation).
///
/// Creates a center Point (construction) and a Circle entity.
pub(crate) fn true_circle_profile(cx: f64, cy: f64, radius: f64) -> ProfileData {
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
        arc_segments: vec![],
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

// ── Euler Target Computation ──────────────────────────────────────────────

/// Compute the expected Euler characteristic from the operation list.
///
/// A cut operation that fully penetrates the body creates a through-hole
/// (genus += 1), so χ = 2 - 2g. Detection heuristic: a same-plane extrude cut
/// whose depth is >= the first boss's depth creates a through-hole.
///
/// The through-hole PERSISTS through any number of trailing CUT operations —
/// cuts remove material but do not re-close the ring. We only bail to the
/// genus-0 default (χ=2) when a later BOSS appears, since a boss can refill
/// the hole (topology we cannot model from metadata alone).
///
/// **Why the equality oracle demands this** (R0099, 2026-07-08): the euler
/// oracle checks EXACT equality against this target. An earlier version
/// returned χ=2 for every 3+-op case on the assumption that over-predicting
/// was "lenient" — but for a boss + through-cut + (revolve-)cut sequence the
/// TRUE topology is genus-1 (χ=0), so predicting 2 REJECTS correct geometry.
/// R0099 (`extrude(circle,boss)+extrude(circle,through-cut)+revolve(rect,cut)`)
/// is exactly this class: the concentric through-cut opens a genus-1 tube and
/// the partial revolve-cut only notches the wall (verified genus-1 by
/// point-in-solid analysis of the kernel output). See the
/// `assay_baseline_207_r0099_wrong` investigation.
///
/// We still avoid predicting genus changes from:
/// - Revolve cuts (angle vs depth comparison is meaningless — a revolve does
///   not create a predictable through-hole; it only ever REMOVES material,
///   which is why it cannot re-close an existing hole)
/// - Multi-plane cuts (cut axis misaligned with boss → penetration unknown)
/// - Any boss after the first (may refill a hole)
pub fn compute_euler_target(ops: &[OpMeta]) -> i64 {
    // Must start with a boss.
    let Some(boss) = ops.first() else {
        return 2;
    };
    if boss.is_cut {
        return 2;
    }

    // Scan the trailing ops for a same-plane penetrating extrude-cut. A boss
    // appearing after a detected through-hole may refill it → indeterminate.
    let mut through_hole = false;
    for op in &ops[1..] {
        if !op.is_cut {
            // A later boss can fill the hole we just opened — bail to the
            // genus-0 default. (A boss before any hole is harmless additive
            // geometry that keeps χ=2.)
            if through_hole {
                return 2;
            }
            continue;
        }

        // A cut. Only a same-plane extrude cut that fully penetrates the boss
        // opens a through-hole; revolve/multi-plane cuts do not (and, being
        // material-removal, never re-close an existing hole either).
        let same_plane = boss.plane_normal.is_none() && op.plane_normal.is_none();
        if op.kind == "extrude" && same_plane && op.depth_or_angle >= boss.depth_or_angle {
            through_hole = true;
        }
    }

    if through_hole {
        0 // genus=1 through-hole: χ = 2 - 2(1) = 0
    } else {
        2
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
                    projected: Vec::new(),
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
                        combine: None,
                        targets: None,
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
                        region: None,
                        regions: Vec::new(),
                    },
                },
                suppressed: false,
                references: vec![],
            }
        } else {
            // Revolve: compute in-plane tangent vector for axis direction.
            // Use the same algorithm as compute_plane_basis: cross normal with
            // the least-aligned world axis to get a vector that lies in the sketch plane.
            let tangent = {
                let n = op_normal;
                let helper = if n[0].abs() < n[1].abs().min(n[2].abs()) {
                    [1.0, 0.0, 0.0]
                } else if n[1].abs() < n[2].abs() {
                    [0.0, 1.0, 0.0]
                } else {
                    [0.0, 0.0, 1.0]
                };
                let tx = n[1] * helper[2] - n[2] * helper[1];
                let ty = n[2] * helper[0] - n[0] * helper[2];
                let tz = n[0] * helper[1] - n[1] * helper[0];
                let len = (tx * tx + ty * ty + tz * tz).sqrt();
                [tx / len, ty / len, tz / len]
            };
            // PR15: offset axis_origin along the in-plane perpendicular to the
            // axis tangent, NOT along the tangent itself. PR14 Phase A finding
            // (Defect 2): `tangent` IS the axis direction, so `axis_origin +
            // k*tangent` parameterizes the same line — geometrically a no-op
            // for line position. Yang 2025 §4.1 requires the revolve profile
            // to lie entirely on one side of the axis (bijective face↔mesh
            // tessellation precondition). The correct offset direction is
            // `cross(op_normal, tangent)`, which lies in the sketch plane and
            // is perpendicular to the axis line.
            let perp_in_plane = cross3(op_normal, tangent);
            let perp_len = (perp_in_plane[0] * perp_in_plane[0]
                + perp_in_plane[1] * perp_in_plane[1]
                + perp_in_plane[2] * perp_in_plane[2])
                .sqrt();
            let perp_unit = if perp_len > 1e-12 {
                [
                    perp_in_plane[0] / perp_len,
                    perp_in_plane[1] / perp_len,
                    perp_in_plane[2] / perp_len,
                ]
            } else {
                perp_in_plane
            };
            let axis_offset = profile_size * 1.5;
            Feature {
                id: Uuid::new_v4(),
                name: format!("Revolve {}", i + 1),
                operation: Operation::Revolve {
                    params: RevolveParams {
                        combine: None,
                        targets: None,
                        sketch_id,
                        profile_index: 0,
                        axis_origin: [
                            op_origin[0] + axis_offset * perp_unit[0],
                            op_origin[1] + axis_offset * perp_unit[1],
                            op_origin[2] + axis_offset * perp_unit[2],
                        ],
                        axis_direction: tangent,
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

    // PR-ASSAY-NOOP (2026-06-12): repair no-op operations so every op
    // CHANGES the result volume (a union swallowed by an existing body or a
    // cut through pure free space tests nothing). Deterministic, consumes
    // NO randomness — unaffected cases stay byte-identical.
    fix_noop_operations(&mut features, &mut op_metas);

    let tree = FeatureTree {
        features,
        active_index: None,
        ..Default::default()
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

    // Revolve operations sweep profiles around an axis, potentially creating
    // geometry with diameter much larger than the profile scale. Use a larger
    // multiplier when revolves are present.
    //
    // Gear profiles extend to the tooth TIP (addendum) radius, ~pitch·(1+2/teeth)
    // — up to ~25% beyond the pitch radius the rest of the heuristic is scaled
    // from — so a valid gear union can exceed the plain 3.0× envelope (e.g. R0082,
    // a circle∪gear union, overshoots by ~2.7%). The bbox bound is a LOOSE
    // sanity check (catch a units bug, not measure addendum), so widen it for
    // gear cases rather than flag a geometrically-valid result as wrong.
    let has_revolve = op_metas.iter().any(|o| o.kind == "revolve");
    let has_gear = op_metas.iter().any(|o| o.profile_type == "gear");
    let bbox_multiplier = if has_revolve {
        10.0
    } else if has_gear {
        4.0
    } else {
        3.0
    };
    let max_bbox_extent = scale * bbox_multiplier;

    let meta = AssayMeta {
        id: case_id.clone(),
        description: description.clone(),
        master_seed,
        test_seed,
        scale,
        log_scale,
        plane_origin,
        plane_normal,
        operations: op_metas.clone(),
        oracles: OracleExpectations {
            euler_target: compute_euler_target(&op_metas),
            expect_watertight: true,
            max_bbox_extent,
            expect_positive_volume: true,
            volume_monotonicity,
            expect_rebuild_error: false,
            expected_volume: None,
            expected_volume_tol_rel: None,
            expected_solid_count: None,
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

// ═══════════════════════════════════════════════════════════════════════════
// No-op operation repair (PR-ASSAY-NOOP, 2026-06-12)
// ═══════════════════════════════════════════════════════════════════════════
//
// Two no-op classes the random placement produces:
//
// 1. **Swallowed boss** — a later extrude fully inside the union of prior
//    bosses (single-plane cases stack concentric profiles from one plane, so
//    a smaller/shorter second profile vanishes into the first). Fix: flip
//    the extrude to the other side of its sketch plane
//    (`direction = Some(−n̂)`); if the flipped tool is STILL contained,
//    re-anchor its plane origin onto the prior-body AABB boundary so the
//    tool half-protrudes.
// 2. **Free-space cut** — a cut whose (auto-aimed) tool prism misses every
//    prior boss (multi-plane cases scatter origins over [−scale, scale]³).
//    Flipping cannot fix a lateral miss (the engine already aims the sweep
//    at the body's midpoint side), so the fix re-anchors the cut's plane
//    origin so the tool is centered on the prior-body AABB center —
//    guaranteed intersection.
//
// Detection is CONSERVATIVE in the safe direction: an op is only repaired
// when its no-op-ness is certain for the decidable shape classes —
// boss containment tests the tool's oriented-bbox CORNERS (overestimate)
// against an exact (rect/circle) or inscribed (gear root-circle) prior
// body, and skips when any prior cut or revolve makes the local geometry
// undecidable; cut disjointness uses world-AABB separation (overestimates
// both). Revolve ops are never repaired and make subsequent decisions
// undecidable (skip).

/// The engine's sketch-plane basis (`tangent_x_from_normal` in
/// feature-engine rebuild.rs + `v = n × u`) — the fix-up must model tool
/// placement with the SAME frame the replay uses.
fn engine_plane_basis(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let ref_vec = if n[2].abs() < 0.99 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let cx = [
        ref_vec[1] * n[2] - ref_vec[2] * n[1],
        ref_vec[2] * n[0] - ref_vec[0] * n[2],
        ref_vec[0] * n[1] - ref_vec[1] * n[0],
    ];
    let len = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    let u = if len > 1e-12 {
        [cx[0] / len, cx[1] / len, cx[2] / len]
    } else {
        [1.0, 0.0, 0.0]
    };
    let v = [
        n[1] * u[2] - n[2] * u[1],
        n[2] * u[0] - n[0] * u[2],
        n[0] * u[1] - n[1] * u[0],
    ];
    (u, v)
}

/// In-plane footprint of a sketch profile, centered on the plane origin
/// (the random generator always centers profiles).
#[derive(Clone, Copy, Debug)]
enum Footprint {
    /// Half-extents of an axis-aligned (in-plane) rectangle.
    Rect { hw: f64, hh: f64 },
    /// Full circle of `r`.
    Circle { r: f64 },
    /// Gear: outer bounding radius + inscribed (root) radius.
    Gear { r_out: f64, r_in: f64 },
}

impl Footprint {
    /// Conservative OUTER half-extents (bounding box of the footprint).
    fn outer_half_extents(self) -> (f64, f64) {
        match self {
            Footprint::Rect { hw, hh } => (hw, hh),
            Footprint::Circle { r } => (r, r),
            Footprint::Gear { r_out, .. } => (r_out, r_out),
        }
    }

    /// Is the in-plane point (x, y) inside the footprint's INSCRIBED region
    /// (exact for Rect/Circle; root circle for Gear)?
    fn inscribed_contains(self, x: f64, y: f64) -> bool {
        match self {
            Footprint::Rect { hw, hh } => x.abs() <= hw && y.abs() <= hh,
            Footprint::Circle { r } => x * x + y * y <= r * r,
            Footprint::Gear { r_in, .. } => x * x + y * y <= r_in * r_in,
        }
    }
}

/// World AABB (`(lo, hi)`).
type Aabb = ([f64; 3], [f64; 3]);

/// A revolve boss's conservative full-turn AABB + an interior anchor point.
type RevolveBound = (Aabb, [f64; 3]);

/// One placed extrude in world space, modelled with the engine's frame.
#[derive(Clone, Debug)]
struct PlacedExtrude {
    origin: [f64; 3],
    u: [f64; 3],
    v: [f64; 3],
    n: [f64; 3],
    fp: Footprint,
    /// Axial span relative to the plane along `n` (resolved direction).
    span: (f64, f64),
}

impl PlacedExtrude {
    fn world(&self, x: f64, y: f64, h: f64) -> [f64; 3] {
        [
            self.origin[0] + x * self.u[0] + y * self.v[0] + h * self.n[0],
            self.origin[1] + x * self.u[1] + y * self.v[1] + h * self.n[1],
            self.origin[2] + x * self.u[2] + y * self.v[2] + h * self.n[2],
        ]
    }

    /// World AABB of the tool's oriented bounding box.
    fn world_aabb(&self) -> Aabb {
        let (hw, hh) = self.fp.outer_half_extents();
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for &sx in &[-hw, hw] {
            for &sy in &[-hh, hh] {
                for &sh in &[self.span.0, self.span.1] {
                    let p = self.world(sx, sy, sh);
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
            }
        }
        (lo, hi)
    }

    /// Is the WORLD point strictly inside this body's inscribed solid?
    fn contains_point(&self, p: [f64; 3]) -> bool {
        let d = [
            p[0] - self.origin[0],
            p[1] - self.origin[1],
            p[2] - self.origin[2],
        ];
        let x = d[0] * self.u[0] + d[1] * self.u[1] + d[2] * self.u[2];
        let y = d[0] * self.v[0] + d[1] * self.v[1] + d[2] * self.v[2];
        let h = d[0] * self.n[0] + d[1] * self.n[1] + d[2] * self.n[2];
        h >= self.span.0 && h <= self.span.1 && self.fp.inscribed_contains(x, y)
    }
}

fn aabb_overlap(a: &Aabb, b: &Aabb) -> bool {
    (0..3).all(|k| a.0[k] <= b.1[k] && b.0[k] <= a.1[k])
}

fn aabb_union(a: &Aabb, b: &Aabb) -> Aabb {
    (
        [a.0[0].min(b.0[0]), a.0[1].min(b.0[1]), a.0[2].min(b.0[2])],
        [a.1[0].max(b.1[0]), a.1[1].max(b.1[1]), a.1[2].max(b.1[2])],
    )
}

/// Extract the centered footprint from an op's sketch feature.
fn footprint_of(sketch: &Sketch, profile_type: &str) -> Option<Footprint> {
    match profile_type {
        "rectangle" => {
            let (mut hw, mut hh) = (0.0f64, 0.0f64);
            for &(_, (x, y)) in &collect_positions(sketch) {
                hw = hw.max(x.abs());
                hh = hh.max(y.abs());
            }
            (hw > 0.0 && hh > 0.0).then_some(Footprint::Rect { hw, hh })
        }
        "circle" => sketch
            .solved_profiles
            .first()
            .and_then(|p| p.circle.as_ref())
            .map(|c| Footprint::Circle { r: c.radius }),
        "gear" => {
            let (mut r_out, mut r_in) = (0.0f64, f64::INFINITY);
            for &(_, (x, y)) in &collect_positions(sketch) {
                let r = (x * x + y * y).sqrt();
                r_out = r_out.max(r);
                r_in = r_in.min(r);
            }
            (r_out > 0.0 && r_in.is_finite()).then_some(Footprint::Gear { r_out, r_in })
        }
        _ => None,
    }
}

fn collect_positions(sketch: &Sketch) -> Vec<(u32, (f64, f64))> {
    sketch
        .solved_positions
        .iter()
        .map(|(&id, &p)| (id, p))
        .collect()
}

/// Repair no-op operations in a generated random case (see the section
/// comment). Mutates extrude `direction` flags and (for re-anchored ops)
/// the sketch feature's `plane_origin` + the matching `OpMeta.plane_origin`.
#[allow(clippy::needless_range_loop)] // i indexes BOTH features (2i, 2i+1) and op_metas
fn fix_noop_operations(features: &mut [Feature], op_metas: &mut [OpMeta]) {
    let op_count = op_metas.len();
    // Feature layout in the random path: [sketch_0, op_0, sketch_1, op_1, …].
    debug_assert_eq!(features.len(), 2 * op_count);

    // Model pass state: placed bosses (decidable shapes), whether anything
    // undecidable (revolve, gear-as-tool fallback, prior cut for the
    // containment test) has been placed.
    let mut bosses: Vec<PlacedExtrude> = Vec::new();
    let mut cuts: Vec<PlacedExtrude> = Vec::new();
    // Revolve bosses, as a conservative full-turn AABB OVERESTIMATE plus an
    // interior anchor (the profile centroid at θ=0): enough for the
    // cut-disjointness test (disjoint from an overestimate is certain) and
    // for re-anchoring a free-space cut onto material; never used for
    // containment (that needs underestimates).
    let mut revolve_aabbs: Vec<RevolveBound> = Vec::new();
    let mut undecidable_body = false;

    for i in 0..op_count {
        let sketch_idx = 2 * i;
        let op_idx = 2 * i + 1;

        // Only extrudes are modelled / repaired; revolve BOSSES contribute a
        // conservative AABB, revolve cuts make later decisions undecidable.
        let is_extrude = matches!(features[op_idx].operation, Operation::Extrude { .. });
        if !is_extrude {
            if let (Operation::Revolve { params }, Operation::Sketch { sketch }) =
                (&features[op_idx].operation, &features[sketch_idx].operation)
            {
                if !params.cut {
                    if let Some(rb) = revolve_full_turn_aabb(sketch, params) {
                        revolve_aabbs.push(rb);
                        continue;
                    }
                }
            }
            undecidable_body = true;
            continue;
        }
        let (origin, n, fp) = {
            let Operation::Sketch { sketch } = &features[sketch_idx].operation else {
                undecidable_body = true;
                continue;
            };
            let nl = sketch.plane_normal;
            let len = (nl[0] * nl[0] + nl[1] * nl[1] + nl[2] * nl[2]).sqrt();
            let n = [nl[0] / len, nl[1] / len, nl[2] / len];
            let fp = footprint_of(sketch, &op_metas[i].profile_type);
            (sketch.plane_origin, n, fp)
        };
        let Some(fp) = fp else {
            undecidable_body = true;
            continue;
        };
        let (u, v) = engine_plane_basis(n);
        let (depth, is_cut, flipped) = match &features[op_idx].operation {
            Operation::Extrude { params } => (params.depth, params.cut, params.direction.is_some()),
            _ => unreachable!(),
        };

        let body_aabb = bosses
            .iter()
            .map(PlacedExtrude::world_aabb)
            .reduce(|a, b| aabb_union(&a, &b));

        let make = |origin: [f64; 3], span: (f64, f64)| PlacedExtrude {
            origin,
            u,
            v,
            n,
            fp,
            span,
        };

        if !is_cut {
            // Boss span: +n̂ by default, −n̂ when an explicit flipped
            // direction is set.
            let span = if flipped { (-depth, 0.0) } else { (0.0, depth) };
            let mut tool = make(origin, span);
            // Swallowed-boss test: certain only when no prior cut or
            // undecidable body could have carved the containing region.
            let contained = |tool: &PlacedExtrude| -> bool {
                if undecidable_body || !cuts.is_empty() || bosses.is_empty() {
                    return false;
                }
                let (hw, hh) = tool.fp.outer_half_extents();
                let mut corners = Vec::with_capacity(8);
                for &sx in &[-hw, hw] {
                    for &sy in &[-hh, hh] {
                        for &sh in &[tool.span.0, tool.span.1] {
                            corners.push(tool.world(sx, sy, sh));
                        }
                    }
                }
                bosses
                    .iter()
                    .any(|b| corners.iter().all(|&c| b.contains_point(c)))
            };
            // Class 3 (the inverse swallow): the new boss fully CONTAINS
            // every prior boss — the final volume would equal this single
            // extrude's. Same certainty rules as class 1.
            let swallows_body = |tool: &PlacedExtrude| -> bool {
                if undecidable_body || !cuts.is_empty() || bosses.is_empty() {
                    return false;
                }
                bosses.iter().all(|b| {
                    let (hw, hh) = b.fp.outer_half_extents();
                    let mut corners = Vec::with_capacity(8);
                    for &sx in &[-hw, hw] {
                        for &sy in &[-hh, hh] {
                            for &sh in &[b.span.0, b.span.1] {
                                corners.push(b.world(sx, sy, sh));
                            }
                        }
                    }
                    corners.iter().all(|&c| tool.contains_point(c))
                })
            };
            if i > 0 && (contained(&tool) || swallows_body(&tool)) {
                // Fix 1: flip to the other side of the sketch plane.
                let flipped_tool = make(origin, (-depth, 0.0));
                if !contained(&flipped_tool) && !swallows_body(&flipped_tool) {
                    set_extrude_direction(&mut features[op_idx], [-n[0], -n[1], -n[2]]);
                    tool = flipped_tool;
                } else if let Some(bb) = body_aabb {
                    // Fix 2: re-anchor so the tool's axial midpoint sits on
                    // the body AABB's max corner along n̂ — half in, half out.
                    let target = project_to_aabb_boundary(&bb, n);
                    let new_origin = [
                        target[0] - n[0] * (depth * 0.5),
                        target[1] - n[1] * (depth * 0.5),
                        target[2] - n[2] * (depth * 0.5),
                    ];
                    set_sketch_origin(&mut features[sketch_idx], new_origin);
                    if op_metas[i].plane_origin.is_some() {
                        op_metas[i].plane_origin = Some(new_origin);
                    }
                    tool = make(new_origin, (0.0, depth));
                }
            }
            bosses.push(tool);
        } else {
            // Cut span: the engine auto-aims the sweep at the body's
            // midpoint side of the sketch plane.
            let resolve_span = |origin: [f64; 3]| -> (f64, f64) {
                if let Some(bb) = &body_aabb {
                    let mid = [
                        (bb.0[0] + bb.1[0]) * 0.5,
                        (bb.0[1] + bb.1[1]) * 0.5,
                        (bb.0[2] + bb.1[2]) * 0.5,
                    ];
                    let side = (mid[0] - origin[0]) * n[0]
                        + (mid[1] - origin[1]) * n[1]
                        + (mid[2] - origin[2]) * n[2];
                    if side < 0.0 {
                        (-depth, 0.0)
                    } else {
                        (0.0, depth)
                    }
                } else {
                    (-depth, 0.0)
                }
            };
            let mut tool = make(origin, resolve_span(origin));
            let disjoint = |tool: &PlacedExtrude| -> bool {
                if undecidable_body || (bosses.is_empty() && revolve_aabbs.is_empty()) {
                    return false;
                }
                let ta = tool.world_aabb();
                bosses.iter().all(|b| !aabb_overlap(&ta, &b.world_aabb()))
                    && revolve_aabbs.iter().all(|(rb, _)| !aabb_overlap(&ta, rb))
            };
            // Class 4 (PR-ASSAY-VOID): INTERIOR cut — the tool is fully
            // enclosed by one boss, so the subtraction carves an internal
            // void: a valid 2-shell body, but visually indistinguishable
            // from a failed cut in the assay browser. Certain only when no
            // prior cut could have carved a path from the tool to the
            // surface (and no undecidable body). Deliberate void coverage
            // lives in the FEATURED internal-void cases instead.
            let enclosing_boss = |tool: &PlacedExtrude| -> Option<usize> {
                if undecidable_body || !cuts.is_empty() || bosses.is_empty() {
                    return None;
                }
                let (hw, hh) = tool.fp.outer_half_extents();
                let mut corners = Vec::with_capacity(8);
                for &sx in &[-hw, hw] {
                    for &sy in &[-hh, hh] {
                        for &sh in &[tool.span.0, tool.span.1] {
                            corners.push(tool.world(sx, sy, sh));
                        }
                    }
                }
                bosses
                    .iter()
                    .position(|b| corners.iter().all(|&c| b.contains_point(c)))
            };
            if let Some(bi) = enclosing_boss(&tool) {
                // Repair: slide the sketch plane along n̂ to just past the
                // enclosing boss's support, so the engine's auto-aimed cut
                // breaches the surface — a VISIBLE pocket (or through hole).
                // The plane sits depth/4 beyond the support; the remaining
                // 3·depth/4 digs in. Lateral position is unchanged (the
                // tool was enclosed, so its footprint stays over material).
                let b = &bosses[bi];
                let (bhw, bhh) = b.fp.outer_half_extents();
                let mut s_max = f64::NEG_INFINITY;
                for &sx in &[-bhw, bhw] {
                    for &sy in &[-bhh, bhh] {
                        for &sh in &[b.span.0, b.span.1] {
                            let c = b.world(sx, sy, sh);
                            let d = (c[0] - origin[0]) * n[0]
                                + (c[1] - origin[1]) * n[1]
                                + (c[2] - origin[2]) * n[2];
                            s_max = s_max.max(d);
                        }
                    }
                }
                let shift = s_max + depth * 0.25;
                let new_origin = [
                    origin[0] + n[0] * shift,
                    origin[1] + n[1] * shift,
                    origin[2] + n[2] * shift,
                ];
                set_sketch_origin(&mut features[sketch_idx], new_origin);
                if op_metas[i].plane_origin.is_some() {
                    op_metas[i].plane_origin = Some(new_origin);
                }
                tool = make(new_origin, resolve_span(new_origin));
            }
            if disjoint(&tool) {
                // Re-anchor: center the tool on a point KNOWN to be on/in
                // material — the first boss's solid center, else the first
                // revolve's profile centroid.
                let target = bosses
                    .first()
                    .map(|b| b.world(0.0, 0.0, (b.span.0 + b.span.1) * 0.5))
                    .or_else(|| revolve_aabbs.first().map(|(_, p)| *p));
                if let Some(mid) = target {
                    let new_origin = [
                        mid[0] + n[0] * (depth * 0.5),
                        mid[1] + n[1] * (depth * 0.5),
                        mid[2] + n[2] * (depth * 0.5),
                    ];
                    set_sketch_origin(&mut features[sketch_idx], new_origin);
                    if op_metas[i].plane_origin.is_some() {
                        op_metas[i].plane_origin = Some(new_origin);
                    }
                    tool = make(new_origin, resolve_span(new_origin));
                }
            }
            cuts.push(tool);
        }
    }
}

/// Conservative full-turn AABB of a revolve boss (overestimates partial
/// sweeps) + an interior anchor point (the profile centroid at θ = 0).
fn revolve_full_turn_aabb(sketch: &Sketch, params: &RevolveParams) -> Option<RevolveBound> {
    let nl = sketch.plane_normal;
    let len = (nl[0] * nl[0] + nl[1] * nl[1] + nl[2] * nl[2]).sqrt();
    if len <= 1e-12 || !len.is_finite() {
        return None;
    }
    let n = [nl[0] / len, nl[1] / len, nl[2] / len];
    let (u, v) = engine_plane_basis(n);
    let positions = collect_positions(sketch);
    if positions.is_empty() {
        return None;
    }
    let a0 = params.axis_origin;
    let ad = params.axis_direction;
    let al = (ad[0] * ad[0] + ad[1] * ad[1] + ad[2] * ad[2]).sqrt();
    if al <= 1e-12 || !al.is_finite() {
        return None;
    }
    let a = [ad[0] / al, ad[1] / al, ad[2] / al];

    // Per-vertex world position; axial coordinate t and radial distance r
    // from the axis. The full revolution sweeps an annular slab: along the
    // axis [t_min, t_max], radially up to r_max in every perpendicular
    // direction — its AABB is computable per world axis.
    let mut t_min = f64::INFINITY;
    let mut t_max = f64::NEG_INFINITY;
    let mut r_max = 0.0f64;
    let mut centroid2 = [0.0f64; 2];
    for &(_, (x, y)) in &positions {
        centroid2[0] += x;
        centroid2[1] += y;
        let w = [
            sketch.plane_origin[0] + x * u[0] + y * v[0],
            sketch.plane_origin[1] + x * u[1] + y * v[1],
            sketch.plane_origin[2] + x * u[2] + y * v[2],
        ];
        let d = [w[0] - a0[0], w[1] - a0[1], w[2] - a0[2]];
        let t = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
        let rad = [d[0] - t * a[0], d[1] - t * a[1], d[2] - t * a[2]];
        t_min = t_min.min(t);
        t_max = t_max.max(t);
        r_max = r_max.max((rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt());
    }
    if !t_min.is_finite() {
        return None;
    }
    let k = positions.len() as f64;
    centroid2[0] /= k;
    centroid2[1] /= k;
    let interior = [
        sketch.plane_origin[0] + centroid2[0] * u[0] + centroid2[1] * v[0],
        sketch.plane_origin[1] + centroid2[0] * u[1] + centroid2[1] * v[1],
        sketch.plane_origin[2] + centroid2[0] * u[2] + centroid2[1] * v[2],
    ];
    // AABB of the slab: per world axis k, extent = |a_k|·t-extent +
    // sqrt(1 − a_k²)·r_max.
    let mut lo = [0.0f64; 3];
    let mut hi = [0.0f64; 3];
    for kx in 0..3 {
        let perp = (1.0f64 - a[kx] * a[kx]).max(0.0).sqrt();
        let c0 = a0[kx] + a[kx] * t_min - perp * r_max;
        let c1 = a0[kx] + a[kx] * t_max + perp * r_max;
        lo[kx] = c0.min(c1) - 0.0;
        hi[kx] = c0.max(c1) + 0.0;
    }
    Some(((lo, hi), interior))
}

/// The point on the AABB boundary where the +n̂ ray from the box center
/// exits (the re-anchor target for half-protruding bosses).
fn project_to_aabb_boundary(bb: &Aabb, n: [f64; 3]) -> [f64; 3] {
    let mid = [
        (bb.0[0] + bb.1[0]) * 0.5,
        (bb.0[1] + bb.1[1]) * 0.5,
        (bb.0[2] + bb.1[2]) * 0.5,
    ];
    let half = [
        (bb.1[0] - bb.0[0]) * 0.5,
        (bb.1[1] - bb.0[1]) * 0.5,
        (bb.1[2] - bb.0[2]) * 0.5,
    ];
    // Smallest t with |mid + t·n − mid|_k hitting a face: t = min over axes
    // of half_k / |n_k|.
    let mut t = f64::INFINITY;
    for k in 0..3 {
        if n[k].abs() > 1e-12 {
            t = t.min(half[k] / n[k].abs());
        }
    }
    if !t.is_finite() {
        t = 0.0;
    }
    [mid[0] + n[0] * t, mid[1] + n[1] * t, mid[2] + n[2] * t]
}

fn set_extrude_direction(op: &mut Feature, dir: [f64; 3]) {
    if let Operation::Extrude { params } = &mut op.operation {
        params.direction = Some(dir);
    }
}

fn set_sketch_origin(sketch_feature: &mut Feature, origin: [f64; 3]) {
    if let Operation::Sketch { sketch } = &mut sketch_feature.operation {
        sketch.plane_origin = origin;
    }
}

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
                    projected: Vec::new(),
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
                    combine: None,
                    targets: None,
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
                    region: None,
                    regions: Vec::new(),
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
                    projected: Vec::new(),
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
                    combine: None,
                    targets: None,
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
                    region: None,
                    regions: Vec::new(),
                },
            },
            suppressed: false,
            references: vec![],
        });

        // PR-ASSAY-NOOP: same repair as the random corpus (several specs
        // stack a smaller second boss fully inside the first).
        let mut spec_metas = vec![
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
        ];
        fix_noop_operations(&mut features, &mut spec_metas);

        let tree = FeatureTree {
            features,
            active_index: None,
            ..Default::default()
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
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
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

    // Append boolean-path-targeting cases (F0026-F0061)
    entries.extend(generate_circle_boss_cases(output_dir));
    entries.extend(generate_box_minus_cyl_cases(output_dir));
    entries.extend(generate_cyl_minus_box_cases(output_dir));
    entries.extend(generate_cyl_cyl_parallel_cases(output_dir));
    entries.extend(generate_mixed_cross_plane_cases(output_dir));
    entries.extend(generate_scale_extreme_cases(output_dir));
    entries.extend(generate_cyl_cyl_angled_cases(output_dir));

    // Append gear-profile-targeting case (F0061)
    entries.extend(generate_gear_cut_cases(output_dir));

    // Append box-through-hole case (F0062)
    entries.extend(generate_box_through_hole_cases(output_dir));

    // Append chained extrude cases (F0063-F0072)
    entries.extend(generate_chained_extrude_cases(output_dir));

    // Append revolve self-intersection cases (F0073-F0075)
    entries.extend(generate_revolve_self_intersection_cases(output_dir));

    // Append off-axis chained extrude cases (F0076-F0085)
    entries.extend(generate_off_axis_chained_cases(output_dir));

    // Append swiss cheese disc cases (F0086-F0090)
    entries.extend(generate_swiss_cheese_disc_cases(output_dir));

    // Append face-to-face stacked union case (F0091)
    entries.extend(generate_face_to_face_union_cases(output_dir));

    // Append deliberate internal-void cases (F0092-F0093)
    entries.extend(generate_internal_void_cases(output_dir));

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
                    projected: Vec::new(),
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
                    combine: None,
                    targets: None,
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
                    region: None,
                    regions: Vec::new(),
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
                    projected: Vec::new(),
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
                    combine: None,
                    targets: None,
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
                    region: None,
                    regions: Vec::new(),
                },
            },
            suppressed: false,
            references: vec![],
        });

        let tree = FeatureTree {
            features,
            active_index: None,
            ..Default::default()
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
                // PR16: removed AABB-disjoint heuristic. Pre-PR13 (commit aee32d8),
                // the kernel errored on disjoint operands; commit ef70415
                // (2026-03-28) hand-edited F0011-F0015's oracle to false after
                // PR13's AABB-disjoint short-circuit at topology_extract.rs:1504-1523
                // made disjoint booleans produce trivial correct results
                // (Yang 2025 §4.5.5: A∪B with A∩B=∅ is a compound solid, not an
                // error; A−B with A∩B=∅ is A unchanged; A∩B with A∩B=∅ is empty).
                // PR16 codifies that hand-edit in the generator.
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
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
                        projected: Vec::new(),
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
                        combine: None,
                        targets: None,
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
                        region: None,
                        regions: Vec::new(),
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
            ..Default::default()
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
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
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
    mut features: Vec<Feature>,
    mut meta: AssayMeta,
) -> ManifestEntry {
    // PR-ASSAY-NOOP: the curated cases get the same no-op repair as the
    // random corpus (several early featured specs stacked a smaller boss
    // fully inside the first — testing nothing). Applies only when the
    // feature layout is the standard [sketch, op]× pairing the repair
    // models; deliberate touching/coplanar configurations are unaffected
    // (they are not containments).
    let standard_layout = features.len() == 2 * meta.operations.len()
        && features.chunks(2).all(|w| {
            matches!(w[0].operation, Operation::Sketch { .. })
                && !matches!(w[1].operation, Operation::Sketch { .. })
        });
    // PR-ASSAY-VOID: deliberate internal-void cases are EXEMPT from the
    // repair — the enclosed cavity is their point (the repair would
    // re-anchor the cut to breach a face, destroying the coverage).
    let deliberate_void = meta.description.contains("internal-void");
    if standard_layout && !deliberate_void {
        fix_noop_operations(&mut features, &mut meta.operations);
    }

    let tree = FeatureTree {
        features,
        active_index: None,
        ..Default::default()
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
                projected: Vec::new(),
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
                combine: None,
                targets: None,
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
                region: None,
                regions: Vec::new(),
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
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
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
                // PR-ASSAY-VOID: the interior-cut repair re-anchors this
                // family's enclosed cut to breach the top face — a VISIBLE
                // pocket, single shell, χ = 2 (was 4 when the cut stayed
                // fully interior and carved a hidden cavity). Deliberate
                // cavity coverage lives in F0092/F0093 instead.
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 3.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "decrease".to_string()],
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
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
                // PR-ASSAY-VOID: the interior-cut repair re-anchors this
                // family's enclosed cut to breach the top face — a VISIBLE
                // pocket, single shell, χ = 2 (was 4 when the cut stayed
                // fully interior and carved a hidden cavity). Deliberate
                // cavity coverage lives in F0092/F0093 instead.
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 3.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "decrease".to_string()],
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
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
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
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
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
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
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
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
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        entries.push(write_featured_case(output_dir, &case_id, features, meta));
    }

    entries
}

/// F0061: Gear boss with through circular cut — exercises gear polygon extrude + boolean subtract.
///
/// Gear profile (12 teeth, module=0.05) extruded as boss, then a circle (r=0.1)
/// cut through the full depth. Circle radius (0.1) < dedendum radius (0.2375),
/// so the hole is fully enclosed within the gear.
fn generate_gear_cut_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    let origin = [0.0, 0.0, 0.0];
    let normal = [0.0, 0.0, 1.0];

    let case_id = "F0061";
    let gear_depth = 0.2;
    let hole_radius = 0.1;
    // Cut depth > gear depth to ensure through-hole
    let cut_depth = 0.3;

    let mut features = Vec::new();

    // Op 1: Gear sketch + boss extrude
    let teeth: u32 = 12;
    let module_val: f64 = 0.05;
    let pitch_radius = (teeth as f64) * module_val / 2.0; // 0.3
    let params = waffle_types::GearParams {
        tooth_count: teeth,
        module: module_val,
        pressure_angle_deg: 20.0,
        ..Default::default()
    };
    let gear_profile_data: ProfileData = (
        vec![waffle_types::SketchEntity::Gear {
            id: 1,
            params,
            construction: false,
        }],
        std::collections::HashMap::new(),
        vec![],
    );
    let (_, gear_feats) = build_sketch_extrude(
        "Gear Sketch",
        "Gear Extrude",
        origin,
        normal,
        gear_profile_data,
        gear_depth,
        false,
        false,
    );
    features.extend(gear_feats);

    // Op 2: Circle sketch + cut extrude (through-hole)
    let (_, cut_feats) = build_sketch_extrude(
        "Hole Sketch",
        "Hole Cut",
        origin,
        normal,
        true_circle_profile(0.0, 0.0, hole_radius),
        cut_depth,
        true,
        false,
    );
    features.extend(cut_feats);

    let description =
        "2 ops, scale=1.00e0, extrude(gear,boss)+extrude(circle,cut) — Gear with through hole"
            .to_string();

    let meta = AssayMeta {
        id: case_id.to_string(),
        description: description.clone(),
        master_seed: 0,
        test_seed: 10001,
        scale: 1.0,
        log_scale: 0.0,
        plane_origin: origin,
        plane_normal: normal,
        operations: vec![
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "gear".to_string(),
                profile_size: pitch_radius,
                depth_or_angle: gear_depth,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "circle".to_string(),
                profile_size: hole_radius,
                depth_or_angle: cut_depth,
                is_cut: true,
                plane_origin: None,
                plane_normal: None,
            },
        ],
        oracles: OracleExpectations {
            euler_target: 0, // Through-hole: genus=1, χ=2-2(1)=0
            expect_watertight: true,
            max_bbox_extent: 3.0,
            expect_positive_volume: true,
            volume_monotonicity: vec!["increase".to_string(), "decrease".to_string()],
            expect_rebuild_error: false,
            expected_volume: None,
            expected_volume_tol_rel: None,
            expected_solid_count: None,
        },
        generator_version: GENERATOR_VERSION,
        featured: true,
    };

    entries.push(write_featured_case(output_dir, case_id, features, meta));
    entries
}

/// F0062: Box with through-hole — centroid-only classification reproduction case.
///
/// 1×1×0.2 box centered at (0.5, 0.5) minus a through-hole cylinder (r=0.1,
/// depth=0.3) centered at the same point. The cylinder extends beyond the box
/// in Z, so the box top face (Z=0.2) is NOT coplanar with anything — it routes
/// through the centroid-only path in `classify_face`. Because the face centroid
/// falls inside the cylinder, the entire top face is incorrectly discarded.
fn generate_box_through_hole_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    let origin = [0.0, 0.0, 0.0];
    let normal = [0.0, 0.0, 1.0];

    let case_id = "F0062";
    let box_w = 1.0;
    let box_h = 1.0;
    let box_d = 0.2;
    let hole_radius = 0.1;
    let cut_depth = 0.3; // extends beyond box top

    let mut features = Vec::new();

    // Op 1: Box sketch + boss extrude (centered at 0.5, 0.5)
    let (_, box_feats) = build_sketch_extrude(
        "Box Sketch",
        "Box Extrude",
        origin,
        normal,
        rect_profile(0.0, 0.0, box_w, box_h),
        box_d,
        false,
        false,
    );
    features.extend(box_feats);

    // Op 2: Circle sketch + cut extrude (through-hole at center)
    let (_, cut_feats) = build_sketch_extrude(
        "Hole Sketch",
        "Hole Cut",
        origin,
        normal,
        true_circle_profile(box_w / 2.0, box_h / 2.0, hole_radius),
        cut_depth,
        true,
        false,
    );
    features.extend(cut_feats);

    let description =
        "2 ops, scale=1.00e0, extrude(rectangle,boss)+extrude(circle,cut) — Box with through hole (centroid bug repro)"
            .to_string();

    let meta = AssayMeta {
        id: case_id.to_string(),
        description: description.clone(),
        master_seed: 0,
        test_seed: 10002,
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
                profile_size: hole_radius,
                depth_or_angle: cut_depth,
                is_cut: true,
                plane_origin: None,
                plane_normal: None,
            },
        ],
        oracles: OracleExpectations {
            euler_target: 0, // Through-hole: genus=1, χ=2-2(1)=0
            expect_watertight: true,
            max_bbox_extent: 3.0,
            expect_positive_volume: true,
            volume_monotonicity: vec!["increase".to_string(), "decrease".to_string()],
            expect_rebuild_error: false,
            expected_volume: None,
            expected_volume_tol_rel: None,
            expected_solid_count: None,
        },
        generator_version: GENERATOR_VERSION,
        featured: true,
    };

    entries.push(write_featured_case(output_dir, case_id, features, meta));
    entries
}

// ── Chained Extrude Cases (F0063-F0072) ───────────────────────────────────

/// Generate 10 chained extrude cases with 5-20 stacked boss extrusions.
///
/// Each step sketches a closed shape on the top face of the previous extrusion
/// and extrudes upward. Profiles are varied (L-shape, T-shape, notched rectangle,
/// plus/cross) but all contain the 2D origin so every union is non-disjoint.
///
/// Tests the boolean merge pipeline under long sequential chains.
fn generate_chained_extrude_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();

    // (case_number, chain_length, seed)
    let specs: [(u64, usize, u64); 10] = [
        (63, 5, 10001),
        (64, 5, 10002),
        (65, 8, 10003),
        (66, 8, 10004),
        (67, 10, 10005),
        (68, 12, 10006),
        (69, 15, 10007),
        (70, 15, 10008),
        (71, 20, 10009),
        (72, 20, 10010),
    ];

    for &(case_num, chain_length, seed) in &specs {
        let case_id = format!("F{:04}", case_num);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);

        let normal = [0.0, 0.0, 1.0];
        let mut cumulative_z = 0.0f64;
        let mut features: Vec<Feature> = Vec::new();
        let mut op_metas: Vec<OpMeta> = Vec::new();
        let mut volume_monotonicity: Vec<String> = Vec::new();
        let mut total_extrudes = 0usize;

        for step in 0..chain_length {
            let origin = [0.0, 0.0, cumulative_z];
            let depth: f64 = rng.gen_range(0.1..0.3);

            // Pick a profile shape — all centered on origin so they overlap the Z-axis
            let scale_frac: f64 = rng.gen_range(0.15..0.5);
            let (profile_data, profile_type, profile_size) =
                chained_profile_shape(&mut rng, scale_frac, step);

            let (_, pair) = build_sketch_extrude(
                &format!("Sketch {}", step + 1),
                &format!("Extrude {}", step + 1),
                origin,
                normal,
                profile_data,
                depth,
                false, // boss, not cut
                false, // not symmetric
            );
            features.extend(pair);

            op_metas.push(OpMeta {
                kind: "extrude".to_string(),
                profile_type,
                profile_size,
                depth_or_angle: depth,
                is_cut: false,
                plane_origin: Some(origin),
                plane_normal: Some(normal),
            });

            volume_monotonicity.push("increase".to_string());
            cumulative_z += depth;
            total_extrudes += 1;
        }

        let max_bbox_extent = 3.0 + (chain_length as f64) * 0.5;
        let description = format!(
            "{} ops, scale=1.00e0, {} chained extrudes (stacked Z) — seed {}",
            chain_length, chain_length, seed
        );

        let meta = AssayMeta {
            id: case_id.clone(),
            description: description.clone(),
            master_seed: 0,
            test_seed: seed,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: normal,
            operations: op_metas,
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent,
                expect_positive_volume: true,
                volume_monotonicity,
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        entries.push(write_featured_case(output_dir, &case_id, features, meta));
        let _ = total_extrudes; // used in stats below
    }

    entries
}

/// Generate a profile shape for a chained extrude step.
///
/// 7 shape types, all centered so they contain the 2D origin (0,0):
/// - L-shape (6 vertices)
/// - T-shape (8 vertices)
/// - Notched rectangle (8 vertices)
/// - Plus/cross (12 vertices)
/// - Rectangle (simple)
/// - Circle (true circle entity)
/// - Gear (8-24 teeth)
///
/// Returns `(profile_data, profile_type, profile_size)`.
fn chained_profile_shape(rng: &mut impl Rng, s: f64, step: usize) -> (ProfileData, String, f64) {
    let shape = (step + rng.gen_range(0..7usize)) % 7;
    match shape {
        0 => {
            // L-shape: full rectangle minus top-right quadrant
            // Outer boundary: 6 vertices, CCW
            let hw = s * rng.gen_range(0.8..1.2);
            let hh = s * rng.gen_range(0.8..1.2);
            let cut_w = hw * rng.gen_range(0.3..0.6);
            let cut_h = hh * rng.gen_range(0.3..0.6);
            (
                polygon_profile(&[
                    (-hw, -hh),
                    (hw, -hh),
                    (hw, hh - cut_h),
                    (hw - cut_w, hh - cut_h),
                    (hw - cut_w, hh),
                    (-hw, hh),
                ]),
                "polygon".to_string(),
                s,
            )
        }
        1 => {
            // T-shape: rectangle body + tab on top center
            let bw = s * rng.gen_range(0.6..1.0); // body half-width
            let bh = s * rng.gen_range(0.3..0.5); // body half-height
            let tw = s * rng.gen_range(0.2..0.4); // tab half-width (< bw)
            let th = s * rng.gen_range(0.2..0.4); // tab height
            let tw = tw.min(bw * 0.8); // ensure tab narrower than body
            (
                polygon_profile(&[
                    (-bw, -bh),
                    (bw, -bh),
                    (bw, bh),
                    (tw, bh),
                    (tw, bh + th),
                    (-tw, bh + th),
                    (-tw, bh),
                    (-bw, bh),
                ]),
                "polygon".to_string(),
                s,
            )
        }
        2 => {
            // Notched rectangle: rectangle with a rectangular notch on the right side
            let hw = s * rng.gen_range(0.7..1.1);
            let hh = s * rng.gen_range(0.7..1.1);
            let nw = hw * rng.gen_range(0.2..0.4); // notch width
            let nh = hh * rng.gen_range(0.2..0.5); // notch half-height
            (
                polygon_profile(&[
                    (-hw, -hh),
                    (hw, -hh),
                    (hw, -nh),
                    (hw - nw, -nh),
                    (hw - nw, nh),
                    (hw, nh),
                    (hw, hh),
                    (-hw, hh),
                ]),
                "polygon".to_string(),
                s,
            )
        }
        3 => {
            // Plus/cross shape: 12 vertices
            let arm_w = s * rng.gen_range(0.15..0.3); // arm half-width
            let arm_l = s * rng.gen_range(0.5..0.9); // arm half-length
            (
                polygon_profile(&[
                    (-arm_w, -arm_l),
                    (arm_w, -arm_l),
                    (arm_w, -arm_w),
                    (arm_l, -arm_w),
                    (arm_l, arm_w),
                    (arm_w, arm_w),
                    (arm_w, arm_l),
                    (-arm_w, arm_l),
                    (-arm_w, arm_w),
                    (-arm_l, arm_w),
                    (-arm_l, -arm_w),
                    (-arm_w, -arm_w),
                ]),
                "polygon".to_string(),
                s,
            )
        }
        4 => {
            // Rectangle: simple centered rectangle
            let w = s * rng.gen_range(0.5..1.2);
            let h = s * rng.gen_range(0.5..1.2);
            (
                crate::helpers::rect_profile(-w / 2.0, -h / 2.0, w, h),
                "rectangle".to_string(),
                w.max(h),
            )
        }
        5 => {
            // Circle: true circle profile centered on origin
            let radius = s * rng.gen_range(0.3..0.8);
            (
                true_circle_profile(0.0, 0.0, radius),
                "circle".to_string(),
                radius,
            )
        }
        _ => {
            // Gear: 8-24 teeth
            let teeth: u32 = rng.gen_range(8..=24);
            let module_val = s * 0.08;
            let pitch_radius = (teeth as f64) * module_val / 2.0;
            let params = waffle_types::GearParams {
                tooth_count: teeth,
                module: module_val,
                pressure_angle_deg: 20.0,
                ..Default::default()
            };
            let data = (
                vec![waffle_types::SketchEntity::Gear {
                    id: 1,
                    params,
                    construction: false,
                }],
                std::collections::HashMap::new(),
                vec![],
            );
            (data, "gear".to_string(), pitch_radius)
        }
    }
}

// ── Geometry Helpers ──────────────────────────────────────────────────────

/// 3D cross product.
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Rotate a unit normal by a random angle up to `max_angle_deg` using Rodrigues' formula.
///
/// Returns the new unit normal.
fn rotate_normal(normal: [f64; 3], rng: &mut impl Rng, max_angle_deg: f64) -> [f64; 3] {
    let theta = rng.gen_range(0.0..max_angle_deg).to_radians();

    // Find a random axis perpendicular to `normal` via rejection sampling
    let k = loop {
        let candidate = [
            rng.gen_range(-1.0..1.0f64),
            rng.gen_range(-1.0..1.0f64),
            rng.gen_range(-1.0..1.0f64),
        ];
        let c = cross3(normal, candidate);
        let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        if len > 1e-6 {
            break [c[0] / len, c[1] / len, c[2] / len];
        }
    };

    // Rodrigues: v_rot = v*cos(θ) + (k×v)*sin(θ) + k*(k·v)*(1-cos(θ))
    let cos_t = theta.cos();
    let sin_t = theta.sin();
    let kxv = cross3(k, normal);
    let kdv = k[0] * normal[0] + k[1] * normal[1] + k[2] * normal[2];

    let r = [
        normal[0] * cos_t + kxv[0] * sin_t + k[0] * kdv * (1.0 - cos_t),
        normal[1] * cos_t + kxv[1] * sin_t + k[1] * kdv * (1.0 - cos_t),
        normal[2] * cos_t + kxv[2] * sin_t + k[2] * kdv * (1.0 - cos_t),
    ];

    // Re-normalize
    let len = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
    [r[0] / len, r[1] / len, r[2] / len]
}

/// Generate non-overlapping hole positions inside a disc via polar rejection sampling.
///
/// Returns up to `n_holes` positions as (x, y) pairs. May return fewer if placement fails.
fn random_hole_positions(
    rng: &mut impl Rng,
    disc_radius: f64,
    hole_radius: f64,
    n_holes: usize,
) -> Vec<(f64, f64)> {
    let max_r = disc_radius - hole_radius;
    if max_r <= 0.0 {
        return vec![];
    }
    let max_attempts = n_holes * 1000;
    let mut positions: Vec<(f64, f64)> = Vec::with_capacity(n_holes);
    let mut attempts = 0;

    while positions.len() < n_holes && attempts < max_attempts {
        attempts += 1;
        let r = rng.gen_range(0.0..max_r);
        let angle = rng.gen_range(0.0..std::f64::consts::TAU);
        let x = r * angle.cos();
        let y = r * angle.sin();

        // Check non-overlap with all previously placed holes
        let overlaps = positions.iter().any(|&(px, py)| {
            let dx = x - px;
            let dy = y - py;
            (dx * dx + dy * dy).sqrt() < 2.0 * hole_radius
        });
        if !overlaps {
            positions.push((x, y));
        }
    }
    positions
}

// ── Off-Axis Chained Extrude Cases (F0076-F0085) ────────────────────────

/// Generate 10 off-axis chained extrude cases.
///
/// Like `generate_chained_extrude_cases` but each step tilts the extrusion
/// normal by 0-5° from the previous step, creating near-coplanar boolean faces.
fn generate_off_axis_chained_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();

    let specs: [(u64, usize, u64); 10] = [
        (76, 5, 20001),
        (77, 5, 20002),
        (78, 8, 20003),
        (79, 8, 20004),
        (80, 10, 20005),
        (81, 12, 20006),
        (82, 15, 20007),
        (83, 15, 20008),
        (84, 20, 20009),
        (85, 20, 20010),
    ];

    for &(case_num, chain_length, seed) in &specs {
        let case_id = format!("F{:04}", case_num);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);

        let mut current_normal = [0.0, 0.0, 1.0];
        let mut current_origin = [0.0, 0.0, 0.0];
        let mut features: Vec<Feature> = Vec::new();
        let mut op_metas: Vec<OpMeta> = Vec::new();
        let mut volume_monotonicity: Vec<String> = Vec::new();

        for step in 0..chain_length {
            // Tilt normal by 0-5° from current
            current_normal = rotate_normal(current_normal, &mut rng, 5.0);

            let depth: f64 = rng.gen_range(0.1..0.3);
            let scale_frac: f64 = rng.gen_range(0.15..0.5);
            let (profile_data, profile_type, profile_size) =
                chained_profile_shape(&mut rng, scale_frac, step);

            let (_, pair) = build_sketch_extrude(
                &format!("Sketch {}", step + 1),
                &format!("Extrude {}", step + 1),
                current_origin,
                current_normal,
                profile_data,
                depth,
                false,
                false,
            );
            features.extend(pair);

            op_metas.push(OpMeta {
                kind: "extrude".to_string(),
                profile_type,
                profile_size,
                depth_or_angle: depth,
                is_cut: false,
                plane_origin: Some(current_origin),
                plane_normal: Some(current_normal),
            });

            volume_monotonicity.push("increase".to_string());

            // Advance origin along current (tilted) normal
            current_origin = [
                current_origin[0] + current_normal[0] * depth,
                current_origin[1] + current_normal[1] * depth,
                current_origin[2] + current_normal[2] * depth,
            ];
        }

        let max_bbox_extent = 4.0 + (chain_length as f64) * 0.5;
        let description = format!(
            "{} ops, off-axis chained extrudes (0-5° tilt/step) — seed {}",
            chain_length, seed
        );

        let meta = AssayMeta {
            id: case_id.clone(),
            description: description.clone(),
            master_seed: 0,
            test_seed: seed,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            operations: op_metas,
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent,
                expect_positive_volume: true,
                volume_monotonicity,
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        entries.push(write_featured_case(output_dir, &case_id, features, meta));
    }

    entries
}

// ── Swiss Cheese Disc Cases (F0086-F0090) ────────────────────────────────

/// Generate 5 swiss cheese disc cases: circular disc + many random holes.
fn generate_swiss_cheese_disc_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();

    // (case_num, n_holes, n_through, n_blind, seed)
    let specs: [(u64, usize, usize, usize, u64); 5] = [
        (86, 5, 3, 2, 30001),
        (87, 10, 5, 5, 30002),
        (88, 15, 8, 7, 30003),
        (89, 20, 10, 10, 30004),
        (90, 30, 15, 15, 30005),
    ];

    let origin = [0.0, 0.0, 0.0];
    let normal = [0.0, 0.0, 1.0];

    for &(case_num, n_holes, n_through, n_blind, seed) in &specs {
        let case_id = format!("F{:04}", case_num);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(seed);

        let disc_radius: f64 = rng.gen_range(1.0..2.0);
        let disc_depth: f64 = rng.gen_range(0.2..0.5);

        // Scale hole radius down for high hole counts
        let base_hole_radius: f64 = rng.gen_range(0.02..0.1);
        let hole_radius = if n_holes >= 20 {
            base_hole_radius / ((n_holes as f64) / 10.0).sqrt()
        } else {
            base_hole_radius
        };

        // Op 1: disc boss
        let mut features: Vec<Feature> = Vec::new();
        let mut op_metas: Vec<OpMeta> = Vec::new();
        let mut volume_monotonicity: Vec<String> = Vec::new();

        let disc_profile = true_circle_profile(0.0, 0.0, disc_radius);
        let (_, disc_pair) = build_sketch_extrude(
            "Disc Sketch",
            "Disc Extrude",
            origin,
            normal,
            disc_profile,
            disc_depth,
            false,
            false,
        );
        features.extend(disc_pair);
        op_metas.push(OpMeta {
            kind: "extrude".to_string(),
            profile_type: "circle".to_string(),
            profile_size: disc_radius,
            depth_or_angle: disc_depth,
            is_cut: false,
            plane_origin: Some(origin),
            plane_normal: Some(normal),
        });
        volume_monotonicity.push("increase".to_string());

        // Place holes
        let positions = random_hole_positions(&mut rng, disc_radius, hole_radius, n_holes);
        let actual_holes = positions.len();
        let actual_through = n_through.min(actual_holes);
        let _actual_blind = n_blind.min(actual_holes.saturating_sub(actual_through));

        for (i, &(hx, hy)) in positions.iter().enumerate() {
            let is_through = i < actual_through;
            let hole_depth = if is_through {
                disc_depth * rng.gen_range(1.5..3.0) // penetrates fully
            } else {
                disc_depth * rng.gen_range(0.3..0.8) // blind pocket
            };

            let hole_profile = true_circle_profile(hx, hy, hole_radius);
            let (_, hole_pair) = build_sketch_extrude(
                &format!("Hole {} Sketch", i + 1),
                &format!("Hole {} Cut", i + 1),
                origin,
                normal,
                hole_profile,
                hole_depth,
                true, // cut
                false,
            );
            features.extend(hole_pair);
            op_metas.push(OpMeta {
                kind: "extrude".to_string(),
                profile_type: "circle".to_string(),
                profile_size: hole_radius,
                depth_or_angle: hole_depth,
                is_cut: true,
                plane_origin: Some(origin),
                plane_normal: Some(normal),
            });
            volume_monotonicity.push("decrease".to_string());
        }

        let euler_target = 2 - 2 * (actual_through as i64);
        let max_bbox_extent =
            (8.0 * disc_radius * disc_radius + disc_depth * disc_depth).sqrt() + 0.5;
        let description = format!(
            "{} ops, swiss cheese disc (R={:.2}, {} through + {} blind holes) — seed {}",
            1 + actual_holes,
            disc_radius,
            actual_through,
            actual_holes - actual_through,
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
            operations: op_metas,
            oracles: OracleExpectations {
                euler_target,
                expect_watertight: true,
                max_bbox_extent,
                expect_positive_volume: true,
                volume_monotonicity,
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };

        entries.push(write_featured_case(output_dir, &case_id, features, meta));
    }

    entries
}

// ── Face-to-Face Stacked Union (F0091) ─────────────────────────────────────

/// Generate the face-to-face stacked union case (F0091).
///
/// A 1-unit cube extruded directly ON the top face of a 2-unit cube: big
/// cube (−1..1)² × z∈[0,2], small cube (−0.5..0.5)² sketched at z=2 and
/// extruded to z∈[2,3]. The small cube's bottom face lies strictly INSIDE
/// the big cube's top face — a TRUE coplanar face-to-face contact where
/// neither operand swallows the other (unlike the coincident-coplanar
/// F0002-family or the offset-boss interpenetrating unions). The union must
/// be ONE 3-unit-tall body of volume 2³ + 1³ = 9.
fn generate_face_to_face_union_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let origin = [0.0, 0.0, 0.0];
    let normal = [0.0, 0.0, 1.0];
    let case_id = "F0091";

    let mut features = Vec::new();
    let (_, base_feats) = build_sketch_extrude(
        "Sketch 1",
        "Extrude 1",
        origin,
        normal,
        rect_profile(-1.0, -1.0, 2.0, 2.0),
        2.0,
        false,
        false,
    );
    features.extend(base_feats);
    let (_, boss_feats) = build_sketch_extrude(
        "Sketch 2",
        "Extrude 2",
        [0.0, 0.0, 2.0],
        normal,
        rect_profile(-0.5, -0.5, 1.0, 1.0),
        1.0,
        false,
        false,
    );
    features.extend(boss_feats);

    let description = "2 ops, scale=1.00e0, extrude(rectangle,boss)+extrude(rectangle,boss) \
         — Face-to-face stacked union: 1u cube on 2u cube top face (3u tall, volume 9)"
        .to_string();

    let meta = AssayMeta {
        id: case_id.to_string(),
        description: description.clone(),
        master_seed: 0,
        test_seed: 0,
        scale: 1.0,
        log_scale: 0.0,
        plane_origin: origin,
        plane_normal: normal,
        operations: vec![
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 2.0,
                depth_or_angle: 2.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 1.0,
                depth_or_angle: 1.0,
                is_cut: false,
                plane_origin: Some([0.0, 0.0, 2.0]),
                plane_normal: Some(normal),
            },
        ],
        oracles: OracleExpectations {
            euler_target: 2,
            expect_watertight: true,
            // bbox is 2×2×3 → diagonal √17 ≈ 4.123
            max_bbox_extent: 6.0,
            expect_positive_volume: true,
            volume_monotonicity: vec!["increase".to_string(), "increase".to_string()],
            expect_rebuild_error: false,
            expected_volume: None,
            expected_volume_tol_rel: None,
            expected_solid_count: None,
        },
        generator_version: GENERATOR_VERSION,
        featured: true,
    };

    vec![write_featured_case(output_dir, case_id, features, meta)]
}

/// PR-ASSAY-VOID: DELIBERATE internal-void cases (F0092 cylinder cavity,
/// F0093 box cavity). The random generator's interior-cut repair re-anchors
/// accidental enclosed cuts to breach a face (they look like failed cuts in
/// the assay browser); enclosed-cavity coverage lives HERE instead, named
/// explicitly. Each is a 2×2×1 box minus a tool fully inside it: result is
/// a TWO-SHELL body (outer + cavity), per-shell χ = 2 → euler_target 4.
/// Invisible from outside BY DESIGN — verify via volume + shell topology.
fn generate_internal_void_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let origin = [0.0, 0.0, 0.0];
    let normal = [0.0, 0.0, 1.0];
    let mut entries = Vec::new();

    struct VoidSpec {
        id: &'static str,
        tool_desc: &'static str,
        profile_type: &'static str,
        profile: ProfileData,
        profile_size: f64,
    }
    let specs = [
        VoidSpec {
            id: "F0092",
            tool_desc: "cylindrical cavity r=0.4",
            profile_type: "circle",
            profile: true_circle_profile(0.0, 0.0, 0.4),
            profile_size: 0.4,
        },
        VoidSpec {
            id: "F0093",
            tool_desc: "rectangular cavity 0.6×0.6",
            profile_type: "rectangle",
            profile: rect_profile(-0.3, -0.3, 0.6, 0.6),
            profile_size: 0.6,
        },
    ];
    for spec in specs {
        let mut features = Vec::new();
        let (_, base_feats) = build_sketch_extrude(
            "Sketch 1",
            "Extrude 1",
            origin,
            normal,
            rect_profile(-1.0, -1.0, 2.0, 2.0),
            1.0,
            false,
            false,
        );
        features.extend(base_feats);
        // Cut sketch INSIDE the box (z = 0.3), depth 0.4 → the engine
        // auto-aims toward the body midpoint side; either resolution keeps
        // the tool's span inside z ∈ [0, 1]: an enclosed cavity.
        let (_, cut_feats) = build_sketch_extrude(
            "Sketch 2",
            "Extrude 2",
            [0.0, 0.0, 0.3],
            normal,
            spec.profile,
            0.4,
            true,
            false,
        );
        features.extend(cut_feats);

        let description = format!(
            "2 ops, scale=1.00e0, extrude(rectangle,boss)+extrude({},cut) —              internal-void (deliberate): {} fully enclosed in a 2×2×1 box;              expect 2 shells (outer + cavity), invisible from outside",
            spec.profile_type, spec.tool_desc
        );
        let meta = AssayMeta {
            id: spec.id.to_string(),
            description: description.clone(),
            master_seed: 0,
            test_seed: 0,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: origin,
            plane_normal: normal,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: 2.0,
                    depth_or_angle: 1.0,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: spec.profile_type.to_string(),
                    profile_size: spec.profile_size,
                    depth_or_angle: 0.4,
                    is_cut: true,
                    plane_origin: Some([0.0, 0.0, 0.3]),
                    plane_normal: Some(normal),
                },
            ],
            oracles: OracleExpectations {
                // Two closed genus-0 shells: χ_total = 4.
                euler_target: 4,
                expect_watertight: true,
                max_bbox_extent: 4.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "decrease".to_string()],
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };
        entries.push(write_featured_case(output_dir, spec.id, features, meta));
    }
    entries
}

// ── Revolve Self-Intersection Cases (F0073-F0075) ─────────────────────────

/// Generate 3 revolve self-intersection test cases:
/// - F0073: axis through profile center (error expected)
/// - F0074: axis barely inside profile (error expected)
/// - F0075: valid revolve with properly offset axis (regression guard)
fn generate_revolve_self_intersection_cases(output_dir: &std::path::Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    let origin = [0.0, 0.0, 0.0];
    let normal = [0.0, 0.0, 1.0];

    // F0073: Rect boss + revolve with axis through profile center → error
    {
        let mut features = Vec::new();
        let (_, box_feats) = build_sketch_extrude(
            "Box Sketch",
            "Box Extrude",
            origin,
            normal,
            rect_profile(0.0, 0.0, 1.0, 1.0),
            0.5,
            false,
            false,
        );
        features.extend(box_feats);

        // Revolve sketch + revolve with axis at origin (through profile center)
        let sketch_id = Uuid::new_v4();
        let (entities, positions, profiles) = rect_profile(0.0, 0.0, 0.4, 0.4);
        features.push(Feature {
            id: sketch_id,
            name: "Revolve Sketch".to_string(),
            operation: Operation::Sketch {
                sketch: Sketch {
                    id: sketch_id,
                    plane: datum_plane_ref(Uuid::new_v4()),
                    plane_origin: origin,
                    plane_normal: normal,
                    entities,
                    constraints: vec![],
                    solve_status: SolveStatus::FullyConstrained,
                    solved_positions: positions,
                    solved_profiles: profiles,
                    projected: Vec::new(),
                },
            },
            suppressed: false,
            references: vec![],
        });
        features.push(Feature {
            id: Uuid::new_v4(),
            name: "Revolve Center".to_string(),
            operation: Operation::Revolve {
                params: RevolveParams {
                    combine: None,
                    targets: None,
                    sketch_id,
                    profile_index: 0,
                    axis_origin: [0.0, 0.0, 0.0], // through profile center
                    axis_direction: [0.0, 1.0, 0.0],
                    angle: 180.0,
                    cut: false,
                    merge: true,
                },
            },
            suppressed: false,
            references: vec![],
        });

        let meta = AssayMeta {
            id: "F0073".to_string(),
            description: "2 ops, scale=1.00e0, extrude(rectangle,boss)+revolve(rectangle,axis-through-center) — self-intersection error".to_string(),
            master_seed: 0,
            test_seed: 20001,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: origin,
            plane_normal: normal,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: 1.0,
                    depth_or_angle: 0.5,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "revolve".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: 0.4,
                    depth_or_angle: 180.0,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 10.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "error".to_string()],
                expect_rebuild_error: true,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };
        entries.push(write_featured_case(output_dir, "F0073", features, meta));
    }

    // F0074: Circle boss + revolve with axis barely inside profile → error
    {
        let mut features = Vec::new();
        let (_, box_feats) = build_sketch_extrude(
            "Box Sketch",
            "Box Extrude",
            origin,
            normal,
            rect_profile(0.0, 0.0, 1.0, 1.0),
            0.5,
            false,
            false,
        );
        features.extend(box_feats);

        // Revolve sketch with axis offset only 0.01 from center (still inside profile)
        let sketch_id = Uuid::new_v4();
        let (entities, positions, profiles) = rect_profile(0.0, 0.0, 0.4, 0.4);
        features.push(Feature {
            id: sketch_id,
            name: "Revolve Sketch".to_string(),
            operation: Operation::Sketch {
                sketch: Sketch {
                    id: sketch_id,
                    plane: datum_plane_ref(Uuid::new_v4()),
                    plane_origin: origin,
                    plane_normal: normal,
                    entities,
                    constraints: vec![],
                    solve_status: SolveStatus::FullyConstrained,
                    solved_positions: positions,
                    solved_profiles: profiles,
                    projected: Vec::new(),
                },
            },
            suppressed: false,
            references: vec![],
        });
        features.push(Feature {
            id: Uuid::new_v4(),
            name: "Revolve Near".to_string(),
            operation: Operation::Revolve {
                params: RevolveParams {
                    combine: None,
                    targets: None,
                    sketch_id,
                    profile_index: 0,
                    axis_origin: [0.0, 0.0, 0.0], // axis through vertex at (0, 0) — first corner
                    axis_direction: [0.0, 0.0, 1.0], // along Z — vertex (0,0) has perp dist 0
                    angle: 270.0,
                    cut: false,
                    merge: true,
                },
            },
            suppressed: false,
            references: vec![],
        });

        let meta = AssayMeta {
            id: "F0074".to_string(),
            description: "2 ops, scale=1.00e0, extrude(rectangle,boss)+revolve(rectangle,axis-near-vertex) — self-intersection error".to_string(),
            master_seed: 0,
            test_seed: 20002,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: origin,
            plane_normal: normal,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: 1.0,
                    depth_or_angle: 0.5,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "revolve".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: 0.4,
                    depth_or_angle: 270.0,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 10.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "error".to_string()],
                expect_rebuild_error: true,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };
        entries.push(write_featured_case(output_dir, "F0074", features, meta));
    }

    // F0075: Rect boss + valid revolve with properly offset axis (regression guard)
    {
        let mut features = Vec::new();
        let (_, box_feats) = build_sketch_extrude(
            "Box Sketch",
            "Box Extrude",
            origin,
            normal,
            rect_profile(0.0, 0.0, 1.0, 1.0),
            0.5,
            false,
            false,
        );
        features.extend(box_feats);

        // Revolve sketch with axis well offset from profile
        let sketch_id = Uuid::new_v4();
        let (entities, positions, profiles) = rect_profile(0.0, 0.0, 0.4, 0.4);
        features.push(Feature {
            id: sketch_id,
            name: "Revolve Sketch".to_string(),
            operation: Operation::Sketch {
                sketch: Sketch {
                    id: sketch_id,
                    plane: datum_plane_ref(Uuid::new_v4()),
                    plane_origin: origin,
                    plane_normal: normal,
                    entities,
                    constraints: vec![],
                    solve_status: SolveStatus::FullyConstrained,
                    solved_positions: positions,
                    solved_profiles: profiles,
                    projected: Vec::new(),
                },
            },
            suppressed: false,
            references: vec![],
        });
        features.push(Feature {
            id: Uuid::new_v4(),
            name: "Revolve Offset".to_string(),
            operation: Operation::Revolve {
                params: RevolveParams {
                    combine: None,
                    targets: None,
                    sketch_id,
                    profile_index: 0,
                    axis_origin: [1.0, 0.0, 0.0], // well clear of profile (closest vertex at 0.8)
                    axis_direction: [0.0, 1.0, 0.0],
                    angle: 180.0,
                    cut: false,
                    merge: true,
                },
            },
            suppressed: false,
            references: vec![],
        });

        let meta = AssayMeta {
            id: "F0075".to_string(),
            description: "2 ops, scale=1.00e0, extrude(rectangle,boss)+revolve(rectangle,offset-axis) — valid revolve".to_string(),
            master_seed: 0,
            test_seed: 20003,
            scale: 1.0,
            log_scale: 0.0,
            plane_origin: origin,
            plane_normal: normal,
            operations: vec![
                OpMeta {
                    kind: "extrude".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: 1.0,
                    depth_or_angle: 0.5,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
                OpMeta {
                    kind: "revolve".to_string(),
                    profile_type: "rectangle".to_string(),
                    profile_size: 0.4,
                    depth_or_angle: 180.0,
                    is_cut: false,
                    plane_origin: None,
                    plane_normal: None,
                },
            ],
            oracles: OracleExpectations {
                euler_target: 2,
                expect_watertight: true,
                max_bbox_extent: 20.0,
                expect_positive_volume: true,
                volume_monotonicity: vec!["increase".to_string(), "increase".to_string()],
                expect_rebuild_error: false,
                expected_volume: None,
                expected_volume_tol_rel: None,
                expected_solid_count: None,
            },
            generator_version: GENERATOR_VERSION,
            featured: true,
        };
        entries.push(write_featured_case(output_dir, "F0075", features, meta));
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
        // F0041-F0045: 5×2=10, F0046-F0050: 5×2=10, F0051-F0055: 5×2=10, F0056-F0060: 5×2=10,
        // F0061: 1×2=2, F0062: 1×2=2 = 139
        // F0063-F0072: 5+5+8+8+10+12+15+15+20+20 = 118
        extrude_count: extrude_count + 139 + 118,
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
    fn featured_case_ids_f0026_to_f0090() {
        let dir = tempfile::tempdir().unwrap();
        let entries = generate_featured_cases(dir.path());
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        // Check that F0026-F0090 are all present
        for n in 26..=90 {
            let expected = format!("F{:04}", n);
            assert!(
                ids.contains(&expected.as_str()),
                "missing featured case {}",
                expected
            );
        }
        // Total: F0001-F0010 (10) + F0011-F0015 (5) + F0016-F0025 (10) + F0026-F0062 (37)
        //      + F0063-F0072 (10) + F0073-F0075 (3) + F0076-F0085 (10) + F0086-F0090 (5)
        //      + F0091 (1) + F0092-F0093 internal-void (2) = 93
        assert_eq!(entries.len(), 93, "expected 93 featured cases");
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
    fn generator_version_is_4() {
        assert_eq!(GENERATOR_VERSION, 4);
    }

    #[test]
    fn euler_target_no_cuts_is_2() {
        let ops = vec![
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 1.0,
                depth_or_angle: 1.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "circle".to_string(),
                profile_size: 0.5,
                depth_or_angle: 0.5,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
        ];
        assert_eq!(compute_euler_target(&ops), 2);
    }

    #[test]
    fn euler_target_through_hole_same_plane() {
        // Cut deeper than boss on same plane → through-hole (χ=0)
        let ops = vec![
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 1.0,
                depth_or_angle: 1.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "circle".to_string(),
                profile_size: 0.5,
                depth_or_angle: 2.0,
                is_cut: true,
                plane_origin: None,
                plane_normal: None,
            },
        ];
        assert_eq!(compute_euler_target(&ops), 0);
    }

    #[test]
    fn euler_target_multi_plane_cut_returns_2() {
        // Cut on different plane → can't predict through-hole → χ=2
        let ops = vec![
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 1.0,
                depth_or_angle: 1.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "circle".to_string(),
                profile_size: 0.5,
                depth_or_angle: 2.0,
                is_cut: true,
                plane_origin: None,
                plane_normal: Some([0.0, 1.0, 0.0]),
            },
        ];
        assert_eq!(compute_euler_target(&ops), 2);
    }

    #[test]
    fn euler_target_three_ops_returns_2() {
        // 3 operations → can't predict (subsequent boss may fill hole) → χ=2
        let ops = vec![
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 1.0,
                depth_or_angle: 1.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "circle".to_string(),
                profile_size: 0.5,
                depth_or_angle: 2.0,
                is_cut: true,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 0.8,
                depth_or_angle: 1.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
        ];
        assert_eq!(compute_euler_target(&ops), 2);
    }

    #[test]
    fn euler_target_boss_throughcut_revolvecut_persists_zero() {
        // R0099 class: boss + same-plane penetrating extrude-cut (through-hole)
        // + revolve-cut. The revolve-cut only removes material; it cannot
        // re-close the genus-1 tube → χ stays 0. (Point-in-solid analysis of
        // the kernel output confirms the ring is intact — the partial revolve
        // merely notches the wall.)
        let ops = vec![
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "circle".to_string(),
                profile_size: 3.125,
                depth_or_angle: 1.945,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "circle".to_string(),
                profile_size: 2.24,
                depth_or_angle: 4.612,
                is_cut: true,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "revolve".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 6.24,
                depth_or_angle: 323.7,
                is_cut: true,
                plane_origin: None,
                plane_normal: None,
            },
        ];
        assert_eq!(compute_euler_target(&ops), 0);
    }

    #[test]
    fn euler_target_throughcut_then_boss_bails_to_2() {
        // A boss AFTER a through-cut can refill the hole → indeterminate → 2.
        let ops = vec![
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 1.0,
                depth_or_angle: 1.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "circle".to_string(),
                profile_size: 0.5,
                depth_or_angle: 2.0,
                is_cut: true,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "circle".to_string(),
                profile_size: 0.4,
                depth_or_angle: 2.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
        ];
        assert_eq!(compute_euler_target(&ops), 2);
    }

    #[test]
    fn euler_target_revolve_cut_returns_2() {
        // Revolve cut → can't compare angle to depth → χ=2
        let ops = vec![
            OpMeta {
                kind: "extrude".to_string(),
                profile_type: "rectangle".to_string(),
                profile_size: 1.0,
                depth_or_angle: 1.0,
                is_cut: false,
                plane_origin: None,
                plane_normal: None,
            },
            OpMeta {
                kind: "revolve".to_string(),
                profile_type: "circle".to_string(),
                profile_size: 0.5,
                depth_or_angle: 180.0,
                is_cut: true,
                plane_origin: None,
                plane_normal: None,
            },
        ];
        assert_eq!(compute_euler_target(&ops), 2);
    }

    /// Geometric invariant: a revolve's `axis_origin` must be offset from the
    /// sketch plane origin in a direction perpendicular to `axis_direction`.
    ///
    /// Rationale: for a line in 3D, only the perpendicular component of an
    /// offset shifts the line. Adding `k * axis_direction` to `axis_origin`
    /// leaves the axis line unchanged — the parametric family
    /// `axis_origin + t * axis_direction` is invariant under such offsets.
    /// Therefore offsetting the axis origin along `axis_direction` itself
    /// is geometrically a no-op and fails to translate the revolution axis
    /// off the sketched profile, causing the profile to self-intersect the
    /// axis at θ=0 (PR14 Phase A engineer-a Defect 2; see
    /// `specs/revolve_self_intersection.md` Generator Status section).
    ///
    /// Currently FAILS at gen.rs:614-628, which sets
    ///   `axis_origin = op_origin + axis_offset * tangent`
    ///   `axis_direction = tangent`
    /// so `(axis_origin - op_origin) · axis_direction = axis_offset` ≈ 1.5 *
    /// profile_size, far above the 1e-9 threshold.
    #[test]
    fn test_revolve_axis_offset_perpendicular_to_tangent() {
        // Scan a window of cases to find at least one revolve operation.
        // R0002 (index 1) is known to have revolves, but we scan a range
        // to be robust to seed/index shifts and to exercise more samples.
        let mut checked = 0usize;
        for index in 0..20 {
            let case = generate_case(42, index);
            let (tree, _meta) =
                file_format::load_project(&case.waffle_json).expect("waffle_json should parse");

            // Build a sketch_id -> plane_origin map from sketch features.
            let mut sketch_origin: HashMap<Uuid, [f64; 3]> = HashMap::new();
            for f in &tree.features {
                if let Operation::Sketch { sketch } = &f.operation {
                    sketch_origin.insert(sketch.id, sketch.plane_origin);
                }
            }

            for f in &tree.features {
                if let Operation::Revolve { params } = &f.operation {
                    let op_origin = *sketch_origin
                        .get(&params.sketch_id)
                        .expect("revolve sketch_id must reference a sketch in the same case");
                    let tangent = params.axis_direction;
                    let offset = [
                        params.axis_origin[0] - op_origin[0],
                        params.axis_origin[1] - op_origin[1],
                        params.axis_origin[2] - op_origin[2],
                    ];
                    let dot =
                        offset[0] * tangent[0] + offset[1] * tangent[1] + offset[2] * tangent[2];
                    assert!(
                        dot.abs() < 1e-9,
                        "case R{:04}: axis_origin offset must be perpendicular to \
                         axis_direction, but dot(offset, axis_direction) = {} \
                         (|offset| ≈ axis_offset, indicating offset is parallel to \
                         axis_direction — see gen.rs:614-628). \
                         offset = {:?}, axis_direction = {:?}",
                        index + 1,
                        dot,
                        offset,
                        tangent,
                    );
                    checked += 1;
                }
            }
        }
        assert!(
            checked > 0,
            "test scanned 20 cases but found no revolve operations — \
             generator distribution may have shifted; widen the scan range"
        );
    }

    /// PR16: F0011-F0015 oracles must have `expect_rebuild_error: false`.
    ///
    /// These five "oblique-plane" cases each contain two extrudes on different
    /// random oblique planes. PR13 (commit `aee32d8`) added an AABB-disjoint
    /// short-circuit to `yang_boolean_pipeline` (`topology_extract.rs:1504-1523`)
    /// that returns trivial results without erroring when operands are spatially
    /// disjoint:
    ///
    /// - `Union`: two side-by-side bodies (compound solid, two shells)
    /// - `Subtract`: A unchanged
    /// - `Intersect`: empty
    ///
    /// This matches Yang 2025 §4.5.5 / Cherchi 2020 §5.1 semantics: A∪B with
    /// A∩B=∅ is a valid compound solid, not an error. The Cherchi 2022
    /// pipeline trivially handles disjoint inputs at the AABB pre-filter
    /// stage — no triangle pairs intersect, so Cherchi 2020 §5.3 segment
    /// insertion has nothing to do.
    ///
    /// Commit ef70415 (2026-03-28) hand-edited F0011-F0015's `.meta.json` from
    /// `expect_rebuild_error=true` to `false` to reflect this. PR15's corpus
    /// regeneration reverted those hand-edits because the generator at
    /// `gen.rs:1216-1256` still computes `expect_rebuild_error: disjoint`
    /// from `aabb_disjoint_3d`. PR16 removes that obsolete heuristic so the
    /// generator unconditionally emits `false`, codifying ef70415's intent.
    ///
    /// This test FAILS on the current generator (red phase): `disjoint=true`
    /// for all five cases at the seeded RNG values 7001-7005, so the oracle
    /// is `true` instead of the required `false`.
    #[test]
    fn test_f0011_to_f0015_oracle_no_disjoint_error() {
        let dir = tempfile::tempdir().unwrap();
        let entries = generate_oblique_plane_cases(dir.path());

        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        for expected in ["F0011", "F0012", "F0013", "F0014", "F0015"] {
            assert!(
                ids.contains(&expected),
                "generate_oblique_plane_cases must produce {}; got {:?}",
                expected,
                ids
            );
        }

        for entry in &entries {
            assert!(
                ["F0011", "F0012", "F0013", "F0014", "F0015"].contains(&entry.id.as_str()),
                "unexpected case id from generate_oblique_plane_cases: {}",
                entry.id
            );

            let meta_path = dir.path().join(&entry.meta_filename);
            let meta_json = fs::read_to_string(&meta_path)
                .unwrap_or_else(|e| panic!("failed to read {}: {}", meta_path.display(), e));
            let meta: AssayMeta = serde_json::from_str(&meta_json)
                .unwrap_or_else(|e| panic!("failed to parse meta for {}: {}", entry.id, e));

            assert!(
                !meta.oracles.expect_rebuild_error,
                "{} oracle expect_rebuild_error must be false (PR13 AABB-disjoint \
                 short-circuit handles disjoint operands as a valid compound solid \
                 per Yang 2025 §4.5.5; obsolete `disjoint` heuristic at gen.rs:1256 \
                 produced true). See ef70415 (2026-03-28) for the original hand-edit \
                 this test codifies.",
                entry.id
            );
        }
    }

    /// PR16 regression guard: F0073 and F0074 must keep `expect_rebuild_error: true`.
    ///
    /// These two cases pair an extrude with a revolve whose axis intersects the
    /// profile interior (F0073) or passes through a profile vertex (F0074).
    /// Both genuinely produce a self-intersecting solid that the kernel rejects
    /// at the revolve operation (PR14 Check 3). The `true` value is hard-coded
    /// at `gen.rs:3175` and `gen.rs:3273` — it is NOT produced by the obsolete
    /// `disjoint` heuristic and must remain `true` after PR16 removes that
    /// heuristic.
    ///
    /// This test PASSES on the current generator and must continue to pass
    /// after PR16's implementation phase.
    #[test]
    fn test_f0073_to_f0074_oracle_revolve_self_intersection_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let entries = generate_revolve_self_intersection_cases(dir.path());

        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        for expected in ["F0073", "F0074"] {
            assert!(
                ids.contains(&expected),
                "generate_revolve_self_intersection_cases must produce {}; got {:?}",
                expected,
                ids
            );
        }

        for entry in entries
            .iter()
            .filter(|e| e.id == "F0073" || e.id == "F0074")
        {
            let meta_path = dir.path().join(&entry.meta_filename);
            let meta_json = fs::read_to_string(&meta_path)
                .unwrap_or_else(|e| panic!("failed to read {}: {}", meta_path.display(), e));
            let meta: AssayMeta = serde_json::from_str(&meta_json)
                .unwrap_or_else(|e| panic!("failed to parse meta for {}: {}", entry.id, e));

            assert!(
                meta.oracles.expect_rebuild_error,
                "{} oracle expect_rebuild_error must remain true after PR16 — \
                 this case genuinely errors at the revolve operation due to \
                 self-intersection (PR14 Check 3), independent of the `disjoint` \
                 heuristic that PR16 removes. The `true` is hard-coded at \
                 gen.rs:3175 (F0073) / gen.rs:3273 (F0074).",
                entry.id
            );
        }
    }
}
