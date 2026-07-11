//! C-series complexity corpus generator (C0001–C0100).
//!
//! Spec: `/specs/assay_complexity_corpus.md`. Deterministic, hand-parameterized
//! cases targeting coverage gaps: in-boundary bug hunters (genus-N topology,
//! interleaved chains, non-convex CDT, near-degeneracy, dispatch-path
//! parameters) and milestone trackers (M8 coplanar residue, M5 degree-4,
//! KV6 revolve tail, KV7 multi-shell, CDT tail).
//!
//! Expected volumes are computed here from kernel-independent arithmetic:
//! an axis-aligned cell sweep over the boss/cut box chain ([`chain_volume`]),
//! shoelace polygon areas × depth, and Pappus for full revolves — never from
//! kernel output.
//!
//! Cases are written by [`write_c_case`], which deliberately does NOT apply
//! `fix_noop_operations` (the repair would move authored geometry out from
//! under the metas' exact volumes). The independent `assay_noop_guard` still
//! scans these files; cases are designed to satisfy it by construction.

use std::collections::HashMap;
use std::path::Path;

use uuid::Uuid;

use crate::helpers::{body_ref, datum_plane_ref, polygon_profile, rect_profile, ProfileData};
use feature_engine::types::{
    BooleanOp, BooleanParams, CombineMode, DepthMode, ExtrudeParams, Feature, FeatureTree,
    Operation, RevolveParams, SecondDirection,
};
use file_format::metadata::ProjectMetadata;
use file_format::save::save_project;
use waffle_types::{Sketch, SketchEntity, SketchPlaneBasis, SolveStatus};

use super::gen::{
    true_circle_profile, AssayMeta, ManifestEntry, OpMeta, OracleExpectations, GENERATOR_VERSION,
};

// ── Exact-volume machinery (kernel-independent) ────────────────────────────

/// Axis-aligned box in world coordinates.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Box3 {
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}

/// One step of an axis-aligned boss/cut/intersect chain.
#[derive(Debug, Clone, Copy)]
pub(crate) enum VOp {
    Add(Box3),
    Cut(Box3),
    /// Intersect: keep only material inside the box.
    Int(Box3),
}

impl VOp {
    fn boxref(&self) -> &Box3 {
        match self {
            VOp::Add(b) | VOp::Cut(b) | VOp::Int(b) => b,
        }
    }
}

/// Exact volume of a sequence of axis-aligned Add/Cut box operations,
/// evaluated by sweeping the coordinate grid the boxes induce: within each
/// grid cell membership is constant, so folding the op sequence per cell is
/// exact set arithmetic (no kernel involvement).
pub(crate) fn chain_volume(ops: &[VOp]) -> f64 {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut zs = Vec::new();
    for op in ops {
        let b = op.boxref();
        xs.push(b.lo[0]);
        xs.push(b.hi[0]);
        ys.push(b.lo[1]);
        ys.push(b.hi[1]);
        zs.push(b.lo[2]);
        zs.push(b.hi[2]);
    }
    for v in [&mut xs, &mut ys, &mut zs] {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.dedup();
    }
    let mut vol = 0.0;
    for i in 0..xs.len().saturating_sub(1) {
        let cx = 0.5 * (xs[i] + xs[i + 1]);
        for j in 0..ys.len().saturating_sub(1) {
            let cy = 0.5 * (ys[j] + ys[j + 1]);
            for k in 0..zs.len().saturating_sub(1) {
                let cz = 0.5 * (zs[k] + zs[k + 1]);
                let mut inside = false;
                for op in ops {
                    let b = op.boxref();
                    let contains = cx > b.lo[0]
                        && cx < b.hi[0]
                        && cy > b.lo[1]
                        && cy < b.hi[1]
                        && cz > b.lo[2]
                        && cz < b.hi[2];
                    match op {
                        VOp::Add(_) if contains => inside = true,
                        VOp::Cut(_) if contains => inside = false,
                        VOp::Int(_) if !contains => inside = false,
                        _ => {}
                    }
                }
                if inside {
                    vol += (xs[i + 1] - xs[i]) * (ys[j + 1] - ys[j]) * (zs[k + 1] - zs[k]);
                }
            }
        }
    }
    vol
}

/// Shoelace area of a simple polygon (positive for CCW winding).
pub(crate) fn shoelace_area(pts: &[(f64, f64)]) -> f64 {
    let n = pts.len();
    let mut a = 0.0;
    for i in 0..n {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    0.5 * a
}

/// World AABB of an extrude tool: rectangle `[umin,umin+w]×[vmin,vmin+h]`
/// in the sketch UV frame of (origin, normal), swept over `span` along the
/// normal. Exact for axis-aligned normals (the only ones the volume model
/// uses); mirrors the engine's `SketchPlaneBasis` by construction.
pub(crate) fn tool_box(
    origin: [f64; 3],
    normal: [f64; 3],
    umin: f64,
    vmin: f64,
    w: f64,
    h: f64,
    span: (f64, f64),
) -> Box3 {
    let basis = SketchPlaneBasis::from_origin_normal(origin, normal);
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &(u, v) in &[
        (umin, vmin),
        (umin + w, vmin),
        (umin + w, vmin + h),
        (umin, vmin + h),
    ] {
        for &s in &[span.0, span.1] {
            let p = basis.local_to_world(u, v);
            let q = [
                p[0] + s * basis.normal[0],
                p[1] + s * basis.normal[1],
                p[2] + s * basis.normal[2],
            ];
            for a in 0..3 {
                lo[a] = lo[a].min(q[a]);
                hi[a] = hi[a].max(q[a]);
            }
        }
    }
    Box3 { lo, hi }
}

// ── Case assembly ──────────────────────────────────────────────────────────

/// Everything needed to serialize one C-series case.
struct CCase {
    id: String,
    scale: f64,
    features: Vec<Feature>,
    ops: Vec<OpMeta>,
    /// Mirror of the op chain as world AABBs, for exact volume computation.
    /// Only maintained by the AABB-based families.
    vops: Vec<VOp>,
    op_counter: usize,
    uid_counter: usize,
}

impl CCase {
    fn new(id: &str) -> Self {
        CCase {
            id: id.to_string(),
            scale: 1.0,
            features: Vec::new(),
            ops: Vec::new(),
            vops: Vec::new(),
            op_counter: 0,
            uid_counter: 0,
        }
    }

    /// Deterministic UUID ("case-id:counter" hashed via FNV-1a into a
    /// version-4-shaped id) — regenerating the corpus is byte-stable, so
    /// `--complexity-only` reruns produce no spurious churn in the
    /// committed files.
    fn uid(&mut self) -> Uuid {
        self.uid_counter += 1;
        let key = format!("waffle-assay-{}:{}", self.id, self.uid_counter);
        let mut bytes = [0u8; 16];
        // Two independent FNV-1a streams (offset-basis tweaked for the
        // second half) fill the 16 bytes deterministically.
        for (half, seed) in [
            (0usize, 0xcbf2_9ce4_8422_2325u64),
            (1, 0x9e37_79b9_7f4a_7c15),
        ] {
            let mut h = seed;
            for b in key.bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0000_0100_0000_01B3);
            }
            bytes[half * 8..half * 8 + 8].copy_from_slice(&h.to_be_bytes());
        }
        // RFC 4122 version + variant bits so the id parses as a v4 UUID.
        bytes[6] = (bytes[6] & 0x0F) | 0x40;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        Uuid::from_bytes(bytes)
    }

    fn push_sketch(&mut self, origin: [f64; 3], normal: [f64; 3], profile: ProfileData) -> Uuid {
        let sketch_id = self.uid();
        let plane_uid = self.uid();
        let (entities, positions, profiles) = profile;
        self.features.push(Feature {
            id: sketch_id,
            name: format!("Sketch {}", self.op_counter + 1),
            operation: Operation::Sketch {
                sketch: Sketch {
                    id: sketch_id,
                    plane: datum_plane_ref(plane_uid),
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
        sketch_id
    }

    fn default_extrude_params(sketch_id: Uuid, depth: f64, cut: bool) -> ExtrudeParams {
        ExtrudeParams {
            combine: None,
            targets: None,
            sketch_id,
            profile_index: 0,
            depth,
            direction: None,
            symmetric: false,
            cut,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
        }
    }

    /// Push sketch + extrude with full param control. Returns the extrude
    /// feature id (for explicit-target refs).
    #[allow(clippy::too_many_arguments)]
    fn extrude_with(
        &mut self,
        origin: [f64; 3],
        normal: [f64; 3],
        profile: ProfileData,
        profile_type: &str,
        profile_size: f64,
        depth: f64,
        cut: bool,
        tweak: impl FnOnce(&mut ExtrudeParams),
    ) -> Uuid {
        let sketch_id = self.push_sketch(origin, normal, profile);
        let mut params = Self::default_extrude_params(sketch_id, depth, cut);
        tweak(&mut params);
        self.op_counter += 1;
        let fid = self.uid();
        self.features.push(Feature {
            id: fid,
            name: format!("Extrude {}", self.op_counter),
            operation: Operation::Extrude { params },
            suppressed: false,
            references: vec![],
        });
        self.ops.push(OpMeta {
            kind: "extrude".to_string(),
            profile_type: profile_type.to_string(),
            profile_size,
            depth_or_angle: depth,
            is_cut: cut,
            plane_origin: Some(origin),
            plane_normal: Some(normal),
        });
        fid
    }

    #[allow(clippy::too_many_arguments)]
    fn extrude(
        &mut self,
        origin: [f64; 3],
        normal: [f64; 3],
        profile: ProfileData,
        profile_type: &str,
        profile_size: f64,
        depth: f64,
        cut: bool,
    ) -> Uuid {
        self.extrude_with(
            origin,
            normal,
            profile,
            profile_type,
            profile_size,
            depth,
            cut,
            |_| {},
        )
    }

    /// Extrude a profile of an ALREADY-pushed sketch (multi-profile sketches,
    /// region extrudes). Plane metadata is not repeated in the OpMeta.
    #[allow(clippy::too_many_arguments)]
    fn extrude_existing(
        &mut self,
        sketch_id: Uuid,
        profile_index: usize,
        profile_type: &str,
        profile_size: f64,
        depth: f64,
        cut: bool,
        tweak: impl FnOnce(&mut ExtrudeParams),
    ) -> Uuid {
        let mut params = Self::default_extrude_params(sketch_id, depth, cut);
        params.profile_index = profile_index;
        tweak(&mut params);
        self.op_counter += 1;
        let fid = self.uid();
        self.features.push(Feature {
            id: fid,
            name: format!("Extrude {}", self.op_counter),
            operation: Operation::Extrude { params },
            suppressed: false,
            references: vec![],
        });
        self.ops.push(OpMeta {
            kind: "extrude".to_string(),
            profile_type: profile_type.to_string(),
            profile_size,
            depth_or_angle: depth,
            is_cut: cut,
            plane_origin: None,
            plane_normal: None,
        });
        fid
    }

    /// AABB-tracked boss: centered `w×h` rectangle at `origin` on an
    /// axis-aligned `normal` plane, extruded `depth` along +normal.
    fn vboss(&mut self, origin: [f64; 3], normal: [f64; 3], w: f64, h: f64, depth: f64) -> Uuid {
        let id = self.extrude(
            origin,
            normal,
            rect_profile(-w / 2.0, -h / 2.0, w, h),
            "rectangle",
            w,
            depth,
            false,
        );
        self.vops.push(VOp::Add(tool_box(
            origin,
            normal,
            -w / 2.0,
            -h / 2.0,
            w,
            h,
            (0.0, depth),
        )));
        id
    }

    /// AABB-tracked cut: centered `w×h` rectangle at `origin`, cutting
    /// `depth` along −normal (the engine aims the cut toward the body, which
    /// every family places on the −normal side of the cut sketch plane).
    fn vcut(&mut self, origin: [f64; 3], normal: [f64; 3], w: f64, h: f64, depth: f64) -> Uuid {
        let id = self.extrude(
            origin,
            normal,
            rect_profile(-w / 2.0, -h / 2.0, w, h),
            "rectangle",
            w,
            depth,
            true,
        );
        self.vops.push(VOp::Cut(tool_box(
            origin,
            normal,
            -w / 2.0,
            -h / 2.0,
            w,
            h,
            (-depth, 0.0),
        )));
        id
    }

    /// Off-center AABB-tracked cut: rectangle `[umin..][vmin..]` in the UV
    /// frame of (origin, normal), cutting along −normal.
    #[allow(clippy::too_many_arguments)]
    fn vcut_uv(
        &mut self,
        origin: [f64; 3],
        normal: [f64; 3],
        umin: f64,
        vmin: f64,
        w: f64,
        h: f64,
        depth: f64,
    ) -> Uuid {
        let id = self.extrude(
            origin,
            normal,
            rect_profile(umin, vmin, w, h),
            "rectangle",
            w,
            depth,
            true,
        );
        self.vops.push(VOp::Cut(tool_box(
            origin,
            normal,
            umin,
            vmin,
            w,
            h,
            (-depth, 0.0),
        )));
        id
    }

    #[allow(clippy::too_many_arguments)]
    fn revolve(
        &mut self,
        origin: [f64; 3],
        normal: [f64; 3],
        profile: ProfileData,
        profile_type: &str,
        profile_size: f64,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle: f64,
        cut: bool,
    ) -> Uuid {
        let sketch_id = self.push_sketch(origin, normal, profile);
        self.op_counter += 1;
        let fid = self.uid();
        self.features.push(Feature {
            id: fid,
            name: format!("Revolve {}", self.op_counter),
            operation: Operation::Revolve {
                params: RevolveParams {
                    combine: None,
                    targets: None,
                    sketch_id,
                    profile_index: 0,
                    axis_origin,
                    axis_direction,
                    angle,
                    cut,
                    merge: true,
                },
            },
            suppressed: false,
            references: vec![],
        });
        self.ops.push(OpMeta {
            kind: "revolve".to_string(),
            profile_type: profile_type.to_string(),
            profile_size,
            depth_or_angle: angle,
            is_cut: cut,
            plane_origin: Some(origin),
            plane_normal: Some(normal),
        });
        fid
    }

    fn boolean(&mut self, a: Uuid, b: Uuid, op: BooleanOp) -> Uuid {
        self.op_counter += 1;
        let fid = self.uid();
        self.features.push(Feature {
            id: fid,
            name: format!("Boolean {}", self.op_counter),
            operation: Operation::BooleanCombine {
                params: BooleanParams {
                    body_a: body_ref(a),
                    body_b: body_ref(b),
                    operation: op,
                },
            },
            suppressed: false,
            references: vec![],
        });
        self.ops.push(OpMeta {
            kind: "boolean".to_string(),
            profile_type: "rectangle".to_string(),
            profile_size: 0.0,
            depth_or_angle: 0.0,
            is_cut: matches!(op, BooleanOp::Subtract | BooleanOp::Intersect),
            plane_origin: None,
            plane_normal: None,
        });
        fid
    }

    fn chain_vol(&self) -> f64 {
        chain_volume(&self.vops)
    }
}

/// Oracle knobs a family sets per case.
struct Knobs {
    euler_target: i64,
    expected_volume: Option<f64>,
    vol_tol: Option<f64>,
    expected_solid_count: Option<usize>,
    max_bbox_extent: f64,
}

impl Knobs {
    fn solid(euler: i64, volume: f64, bbox: f64) -> Self {
        Knobs {
            euler_target: euler,
            expected_volume: Some(volume),
            vol_tol: None,
            expected_solid_count: None,
            max_bbox_extent: bbox,
        }
    }
    fn curved(euler: i64, volume: f64, bbox: f64) -> Self {
        Knobs {
            vol_tol: Some(0.05),
            ..Self::solid(euler, volume, bbox)
        }
    }
    fn tracker(euler: i64, bbox: f64) -> Self {
        Knobs {
            euler_target: euler,
            expected_volume: None,
            vol_tol: None,
            expected_solid_count: None,
            max_bbox_extent: bbox,
        }
    }
}

/// Serialize a case. NO no-op repair (see module docs).
fn write_c_case(
    output_dir: &Path,
    case: CCase,
    description: String,
    knobs: Knobs,
) -> ManifestEntry {
    let monotonicity = case
        .ops
        .iter()
        .map(|o| if o.is_cut { "decrease" } else { "increase" }.to_string())
        .collect();
    let meta = AssayMeta {
        id: case.id.clone(),
        description: description.clone(),
        master_seed: 0,
        test_seed: 0,
        scale: case.scale,
        log_scale: case.scale.log10(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        operations: case.ops,
        oracles: OracleExpectations {
            euler_target: knobs.euler_target,
            expect_watertight: true,
            max_bbox_extent: knobs.max_bbox_extent,
            expect_positive_volume: true,
            volume_monotonicity: monotonicity,
            expect_rebuild_error: false,
            expected_volume: knobs.expected_volume,
            expected_volume_tol_rel: knobs.vol_tol,
            expected_solid_count: knobs.expected_solid_count,
        },
        generator_version: GENERATOR_VERSION,
        featured: true,
    };

    let tree = FeatureTree {
        features: case.features,
        active_index: None,
        ..Default::default()
    };
    let metadata = ProjectMetadata::new(format!("Assay {}", case.id));
    let waffle_json = save_project(&tree, &metadata);

    let filename = format!("{}.waffle", case.id);
    let meta_filename = format!("{}.meta.json", case.id);
    std::fs::write(output_dir.join(&filename), waffle_json).expect("write .waffle");
    std::fs::write(
        output_dir.join(&meta_filename),
        serde_json::to_string_pretty(&meta).expect("serialize meta"),
    )
    .expect("write .meta.json");

    ManifestEntry {
        id: case.id,
        filename,
        meta_filename,
        description,
        featured: true,
    }
}

const Z: [f64; 3] = [0.0, 0.0, 1.0];
const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];

fn desc(case: &CCase, family: &str, detail: &str) -> String {
    format!(
        "{} ops, scale={:.2e}, {} — {}: {}",
        case.ops.len(),
        case.scale,
        summarize_ops(&case.ops),
        family,
        detail
    )
}

fn summarize_ops(ops: &[OpMeta]) -> String {
    let parts: Vec<String> = ops
        .iter()
        .map(|o| {
            format!(
                "{}({},{})",
                o.kind,
                o.profile_type,
                if o.is_cut { "cut" } else { "boss" }
            )
        })
        .collect();
    // Long chains get an abbreviated summary.
    if parts.len() > 6 {
        format!(
            "{}+…+{} [{} ops]",
            parts[0],
            parts[parts.len() - 1],
            parts.len()
        )
    } else {
        parts.join("+")
    }
}

// ── Group 1a: genus-N topology (C0001–C0012) ───────────────────────────────

/// Standard plate for the hole families: 4×4×0.5, centered, z ∈ [0, 0.5].
fn plate(case: &mut CCase) {
    case.vboss([0.0, 0.0, 0.0], Z, 4.0, 4.0, 0.5);
}

/// Standard through-hole: 0.4×0.4 centered square at (x, y), sketched at
/// z = 1 (strictly above the plate, no coplanar pair), depth 2 → z ∈ [−1, 1].
fn hole(case: &mut CCase, x: f64, y: f64) {
    case.vcut([x, y, 1.0], Z, 0.4, 0.4, 2.0);
}

fn genus_plate_case(dir: &Path, id: &str, holes: &[(f64, f64)], detail: &str) -> ManifestEntry {
    let mut c = CCase::new(id);
    plate(&mut c);
    for &(x, y) in holes {
        hole(&mut c, x, y);
    }
    let g = holes.len() as i64;
    let vol = c.chain_vol();
    let d = desc(&c, "genus-N plate", detail);
    write_c_case(dir, c, d, Knobs::solid(2 - 2 * g, vol, 9.0))
}

#[allow(clippy::vec_init_then_push)]
fn family_genus(dir: &Path) -> Vec<ManifestEntry> {
    let mut e = Vec::new();
    e.push(genus_plate_case(
        dir,
        "C0001",
        &[(-0.8, 0.0), (0.8, 0.0)],
        "2 through-holes (g=2, chi=-2)",
    ));
    e.push(genus_plate_case(
        dir,
        "C0002",
        &[(-1.2, 0.0), (0.0, 0.0), (1.2, 0.0)],
        "3 through-holes in a row (g=3, chi=-4)",
    ));
    e.push(genus_plate_case(
        dir,
        "C0003",
        &[(-1.5, 0.0), (-0.5, 0.0), (0.5, 0.0), (1.5, 0.0)],
        "4 through-holes in a row (g=4, chi=-6)",
    ));
    e.push(genus_plate_case(
        dir,
        "C0004",
        &[(-1.6, 0.0), (-0.8, 0.0), (0.0, 0.0), (0.8, 0.0), (1.6, 0.0)],
        "5 through-holes in a row (g=5, chi=-8)",
    ));
    e.push(genus_plate_case(
        dir,
        "C0005",
        &[(-0.8, -0.8), (-0.8, 0.8), (0.8, -0.8), (0.8, 0.8)],
        "2x2 hole grid (g=4, chi=-6)",
    ));
    {
        let grid: Vec<(f64, f64)> = [-1.2, 0.0, 1.2]
            .iter()
            .flat_map(|&x| [-1.2, 0.0, 1.2].iter().map(move |&y| (x, y)))
            .collect();
        e.push(genus_plate_case(
            dir,
            "C0006",
            &grid,
            "3x3 hole grid (g=9, chi=-16)",
        ));
    }
    // C0007: crossing orthogonal tunnels with OFFSET cross-sections (walls
    // NOT coplanar) — the in-boundary twin of the M8 tracker C0041.
    {
        let mut c = CCase::new("C0007");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 2.0); // cube (−1..1)² × [0,2]
        c.vcut([2.0, 0.0, 0.9], X, 0.6, 0.6, 3.5); // tunnel along −x, z-band [0.6,1.2]
        c.vcut([0.0, 2.0, 1.1], Y, 0.6, 0.6, 3.5); // tunnel along −y, z-band [0.8,1.4]
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "genus-N tunnels",
            "crossing orthogonal tunnels, offset sections (g=3, chi=-4)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(-4, vol, 6.0)));
    }
    // C0008: blind pocket + lateral through-hole below it (g=1).
    {
        let mut c = CCase::new("C0008");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 2.0);
        c.vcut([0.0, 0.0, 3.0], Z, 1.0, 1.0, 2.0); // pocket z ∈ [1,2]
        c.vcut([2.0, 0.0, 0.5], X, 0.4, 0.4, 3.5); // lateral tunnel z-band [0.3,0.7]
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "genus-N tunnels",
            "blind pocket + lateral through-hole below it (g=1, chi=0)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 6.0)));
    }
    // C0009: U-tunnel — 2 blind vertical bores + a blind lateral connector
    // (3 boundary openings, contractible tunnel network → chi = 4−2k = −2).
    {
        let mut c = CCase::new("C0009");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 2.0);
        c.vcut([-0.5, 0.0, 3.0], Z, 0.4, 0.4, 2.6); // bore z ∈ [0.4,2]
        c.vcut([0.5, 0.0, 3.0], Z, 0.4, 0.4, 2.6);
        c.vcut([2.0, 0.0, 0.55], X, 0.5, 0.5, 2.75); // connector x ∈ [−0.75,1]
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "genus-N tunnels",
            "U-tunnel: 2 bores + lateral connector, 3 openings (chi=-2)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(-2, vol, 6.0)));
    }
    // C0010: 3-level offset boss tower, one through-hole per level (g=3).
    {
        let mut c = CCase::new("C0010");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 1.0); // L1 (−1..1)² z[0,1]
        c.vboss([0.5, 0.5, 0.8], Z, 1.0, 1.0, 1.0); // L2 [0,1]² z[0.8,1.8]
        c.vboss([0.7, 0.7, 1.6], Z, 0.6, 0.6, 1.0); // L3 [0.4,1]² z[1.6,2.6]
        c.vcut([-0.5, -0.5, 3.0], Z, 0.3, 0.3, 4.5); // pierces L1 only
        c.vcut([0.2, 0.2, 3.0], Z, 0.24, 0.24, 4.5); // pierces L1+L2 column
        c.vcut([0.8, 0.8, 3.0], Z, 0.2, 0.2, 4.5); // pierces L1+L2+L3 column
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "genus-N tower",
            "3-level offset tower, one through-hole per level (g=3, chi=-4)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(-4, vol, 7.0)));
    }
    // C0011: plate with 2 holes, then a boss ARCHING over one hole: the
    // capped shaft becomes a blind pocket (its genus contribution vanishes)
    // → g=1, chi=0.
    {
        let mut c = CCase::new("C0011");
        plate(&mut c);
        hole(&mut c, -0.8, 0.0);
        hole(&mut c, 0.8, 0.0);
        c.vboss([-0.8, 0.0, 0.4], Z, 1.2, 0.6, 0.6); // arch caps hole 1 from z=0.4 up
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "genus-N plate",
            "2 holes then arch boss caps one shaft (g=1, chi=0)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 9.0)));
    }
    e.push(genus_plate_case(
        dir,
        "C0012",
        &[
            (0.0, 0.0),
            (1.1, 0.7),
            (-1.1, 0.7),
            (1.1, -0.7),
            (-1.1, -0.7),
        ],
        "5 staggered through-holes (g=5, chi=-8)",
    ));
    e
}

// ── Group 1b: interleaved boss/cut chains (C0013–C0020) ────────────────────

/// A long alternating boss/cut chain. The stack marches diagonally (both
/// axes drift, so consecutive side faces are never coplanar). Each blind
/// pocket lands in the CURRENT boss's trailing strip — the region every
/// subsequent boss (which keeps drifting away) can never cover, so no pocket
/// is ever sealed into an internal void (a sealed pocket would make the body
/// multi-shell and hit the KV7 wall — first caught by the generator gate).
/// `y_major` marches (0.18, 0.45) instead of (0.45, 0.18).
fn chain_case(dir: &Path, id: &str, op_count: usize, y_major: bool) -> ManifestEntry {
    let side = 1.3;
    let (dx, dy) = if y_major { (0.18, 0.45) } else { (0.45, 0.18) };
    let mut c = CCase::new(id);
    // Base slab.
    c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 1.0);
    let mut top = 1.0;
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut ops_done = 1;
    let mut step = 0usize;
    while ops_done < op_count {
        // Boss: fixed footprint, sketched below the current top so it
        // interpenetrates; drifts diagonally. Depth jitter keeps tops apart.
        let depth = 0.9 + 0.07 * (step as f64 % 3.0);
        cx += dx;
        cy += dy;
        let sketch_z = top - 0.15 - 0.01 * (step as f64 % 4.0);
        c.vboss([cx, cy, sketch_z], Z, side, side, depth);
        top = sketch_z + depth;
        ops_done += 1;
        if ops_done >= op_count {
            break;
        }
        // Blind pocket in the trailing strip along the major drift axis:
        // pocket half-size 0.15, centered 0.45 behind the boss center — the
        // next boss's near edge lands 0.25 clear of it and later bosses only
        // move further away.
        let (px, py) = if y_major {
            (cx, cy - 0.45)
        } else {
            (cx - 0.45, cy)
        };
        let reach = 0.3 + 0.05 * (step as f64 % 2.0);
        c.vcut([px, py, top + 1.0], Z, 0.3, 0.3, 1.0 + reach);
        ops_done += 1;
        step += 1;
    }
    let vol = c.chain_vol();
    let walk = if y_major {
        "y-major diagonal"
    } else {
        "x-major diagonal"
    };
    let d = desc(
        &c,
        "interleaved chain",
        &format!("{op_count} alternating boss/cut ops, {walk} staircase, exact chain volume"),
    );
    // All cuts are blind pockets in permanently exposed strips → chi stays 2.
    write_c_case(dir, c, d, Knobs::solid(2, vol, 30.0))
}

fn family_chains(dir: &Path) -> Vec<ManifestEntry> {
    vec![
        chain_case(dir, "C0013", 8, false),
        chain_case(dir, "C0014", 8, true),
        chain_case(dir, "C0015", 10, false),
        chain_case(dir, "C0016", 10, true),
        chain_case(dir, "C0017", 12, false),
        chain_case(dir, "C0018", 12, true),
        chain_case(dir, "C0019", 16, false),
        chain_case(dir, "C0020", 16, true),
    ]
}

// ── Group 1c: non-convex profile booleans (C0021–C0028) ────────────────────

/// N-point star polygon, CCW, outer/inner radii, rotated by `rot` radians.
fn star(points: usize, r_outer: f64, r_inner: f64, rot: f64) -> Vec<(f64, f64)> {
    (0..2 * points)
        .map(|i| {
            let ang = rot + std::f64::consts::PI * (i as f64) / (points as f64);
            let r = if i % 2 == 0 { r_outer } else { r_inner };
            (r * ang.cos(), r * ang.sin())
        })
        .collect()
}

/// Polygon boss at the origin plus through-cuts whose rectangular tools lie
/// entirely inside solid material (removed volume = area × depth exactly).
/// The cut sketches share the boss origin's (x, y) so the profile-relative
/// placement survives any plane-basis orientation.
fn nonconvex_case(
    dir: &Path,
    id: &str,
    poly: &[(f64, f64)],
    depth: f64,
    cuts: &[(f64, f64, f64, f64)], // (umin, vmin, w, h) in the shared UV frame
    through: bool,
    detail: &str,
) -> ManifestEntry {
    let mut c = CCase::new(id);
    let area = shoelace_area(poly);
    assert!(area > 0.0, "{id}: polygon must be CCW");
    c.extrude(
        [0.0, 0.0, 0.0],
        Z,
        polygon_profile(poly),
        "polygon",
        area.sqrt(),
        depth,
        false,
    );
    let mut removed = 0.0;
    for &(umin, vmin, w, h) in cuts {
        if through {
            // Sketch above the boss; overshoot both caps.
            c.extrude(
                [0.0, 0.0, depth + 0.5],
                Z,
                rect_profile(umin, vmin, w, h),
                "rectangle",
                w,
                depth + 1.5,
                true,
            );
            removed += w * h * depth;
        } else {
            // Blind pocket to half depth.
            c.extrude(
                [0.0, 0.0, depth + 0.5],
                Z,
                rect_profile(umin, vmin, w, h),
                "rectangle",
                w,
                0.5 + depth / 2.0,
                true,
            );
            removed += w * h * (depth / 2.0);
        }
    }
    let vol = (area * depth) - removed;
    let holes = if through { cuts.len() as i64 } else { 0 };
    let d = desc(&c, "non-convex boolean", detail);
    write_c_case(dir, c, d, Knobs::solid(2 - 2 * holes, vol, 12.0))
}

fn family_nonconvex(dir: &Path) -> Vec<ManifestEntry> {
    let mut e = Vec::new();
    e.push(nonconvex_case(
        dir,
        "C0021",
        &star(5, 1.5, 0.6, 0.0),
        0.5,
        &[(-0.25, -0.25, 0.5, 0.5)],
        true,
        "5-point star boss + centered square through-cut (chi=0)",
    ));
    e.push(nonconvex_case(
        dir,
        "C0022",
        &star(7, 1.4, 0.7, 0.1),
        0.5,
        &[(-0.2, -0.2, 0.4, 0.4)],
        true,
        "7-point star boss + centered square through-cut (chi=0)",
    ));
    // C0023: comb — 10 teeth, 20 reflex vertices; cut through the base strip.
    {
        let mut poly = vec![(0.0, 0.0), (4.0, 0.0), (4.0, 0.5)];
        for k in (0..10).rev() {
            let x0 = 0.4 * k as f64;
            let x1 = x0 + 0.2;
            poly.push((x1, 0.5));
            poly.push((x1, 1.5));
            poly.push((x0, 1.5));
            poly.push((x0, 0.5));
        }
        // Last pushed (0.0, 0.5) closes to (0.0, 0.0).
        e.push(nonconvex_case(
            dir,
            "C0023",
            &poly,
            0.5,
            &[(0.5, 0.1, 3.0, 0.3)],
            true,
            "10-tooth comb boss + through-slot in the base strip (chi=0)",
        ));
    }
    e.push(nonconvex_case(
        dir,
        "C0024",
        &[
            (-1.0, 0.0),
            (1.0, 0.0),
            (1.0, 1.5),
            (0.15, 1.5),
            (0.15, 2.7),
            (-0.15, 2.7),
            (-0.15, 1.5),
            (-1.0, 1.5),
        ],
        0.5,
        &[(-0.25, 0.35, 0.5, 0.5)],
        true,
        "keyhole boss (rect + narrow slot) + through-cut in the body (chi=0)",
    ));
    // C0025: C-ring (square annulus with a slit gap) + blind pocket in the
    // opposite wall — 4 reflex vertices, corridor width 0.75, gap 0.3.
    e.push(nonconvex_case(
        dir,
        "C0025",
        &[
            (-1.5, -1.5),
            (1.5, -1.5),
            (1.5, 1.5),
            (0.15, 1.5),
            (0.15, 0.75),
            (0.75, 0.75),
            (0.75, -0.75),
            (-0.75, -0.75),
            (-0.75, 0.75),
            (-0.15, 0.75),
            (-0.15, 1.5),
            (-1.5, 1.5),
        ],
        0.5,
        &[(-0.2, -1.3, 0.4, 0.4)],
        false,
        "C-ring boss (slit square annulus) + blind pocket in the far wall (chi=2)",
    ));
    // C0026: zigzag ribbon — vertical offset of a chevron graph (simple by
    // construction, area = width × x-span exactly).
    {
        let width = 0.3;
        let mut upper: Vec<(f64, f64)> = (0..=8)
            .map(|k| (0.5 * k as f64, if k % 2 == 0 { 0.0 } else { 0.8 }))
            .collect();
        let mut poly = upper.clone();
        upper.reverse();
        poly.extend(upper.iter().map(|&(x, y)| (x, y - width)));
        // Winding: upper left→right then lower right→left is CLOCKWISE
        // (interior below the upper path); reverse for CCW.
        poly.reverse();
        let area = shoelace_area(&poly);
        assert!((area - width * 4.0).abs() < 1e-12);
        e.push(nonconvex_case(
            dir,
            "C0026",
            &poly,
            0.5,
            // In the shared UV frame: centered at (0.25, 0.25), inside the
            // first ascending chevron's ribbon band for all u ∈ [0.2, 0.3].
            &[(0.2, 0.2, 0.1, 0.1)],
            true,
            "8-segment zigzag ribbon + small through-cut in one chevron (chi=0)",
        ));
    }
    // C0027: plus profile + 5 through-holes (one per arm + center) → g=5.
    e.push(nonconvex_case(
        dir,
        "C0027",
        &[
            (-0.5, -1.5),
            (0.5, -1.5),
            (0.5, -0.5),
            (1.5, -0.5),
            (1.5, 0.5),
            (0.5, 0.5),
            (0.5, 1.5),
            (-0.5, 1.5),
            (-0.5, 0.5),
            (-1.5, 0.5),
            (-1.5, -0.5),
            (-0.5, -0.5),
        ],
        0.4,
        &[
            (-0.15, -0.15, 0.3, 0.3),
            (-0.15, -1.25, 0.3, 0.3),
            (-0.15, 0.95, 0.3, 0.3),
            (0.95, -0.15, 0.3, 0.3),
            (-1.25, -0.15, 0.3, 0.3),
        ],
        true,
        "plus-shaped boss + 5 through-holes, one per arm + center (g=5, chi=-8)",
    ));
    // C0028: star boss + rotated-star through-cut (star-shaped hole).
    {
        let outer = star(5, 1.5, 0.6, 0.0);
        let inner = star(5, 0.45, 0.18, std::f64::consts::PI / 5.0);
        let a_outer = shoelace_area(&outer);
        let a_inner = shoelace_area(&inner);
        let mut c = CCase::new("C0028");
        c.extrude(
            [0.0, 0.0, 0.0],
            Z,
            polygon_profile(&outer),
            "polygon",
            a_outer.sqrt(),
            0.5,
            false,
        );
        c.extrude(
            [0.0, 0.0, 1.0],
            Z,
            polygon_profile(&inner),
            "polygon",
            a_inner.sqrt(),
            2.0,
            true,
        );
        let vol = (a_outer - a_inner) * 0.5;
        let d = desc(
            &c,
            "non-convex boolean",
            "5-star boss + rotated 5-star through-cut (star hole, chi=0)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 8.0)));
    }
    e
}

// ── Group 1d: near-degeneracy (C0029–C0040) ────────────────────────────────

fn family_near_degenerate(dir: &Path) -> Vec<ManifestEntry> {
    let mut e = Vec::new();
    // C0029–C0031: through-cut wall passing epsilon from the +x side face.
    for (id, eps) in [("C0029", 1e-3), ("C0030", 1e-5), ("C0031", 2e-6)] {
        let mut c = CCase::new(id);
        c.vboss([0.0, 0.0, 0.0], Z, 1.0, 1.0, 1.0);
        // Tool world x ∈ [0.1, 0.5−eps], y ∈ ±0.25 → UV u = −y, v = x.
        c.vcut_uv([0.0, 0.0, 2.0], Z, -0.25, 0.1, 0.5, 0.4 - eps, 3.0);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "near-degenerate sliver",
            &format!("through-cut leaves {eps:.0e} m wall at the side face (chi=0)"),
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 4.0)));
    }
    // C0032: needle boss, aspect 8000:1 (1e-4 × 0.8), standing proud.
    {
        let mut c = CCase::new("C0032");
        c.vboss([0.0, 0.0, 0.0], Z, 1.0, 1.0, 0.2);
        c.vboss([0.0, 0.0, 0.1], Z, 1e-4, 0.8, 0.3);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "near-degenerate sliver",
            "1e-4 m needle rib boss standing proud of a slab (chi=2)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 4.0)));
    }
    // C0033: needle slit cut fully interior (does not sever the body).
    {
        let mut c = CCase::new("C0033");
        c.vboss([0.0, 0.0, 0.0], Z, 1.0, 1.0, 0.2);
        c.vcut([0.0, 0.0, 1.0], Z, 1e-4, 0.8, 2.0);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "near-degenerate sliver",
            "1e-4 m through-slit inside a slab (slit hole, chi=0)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 4.0)));
    }
    // C0034: square tube with 1e-4 walls.
    {
        let mut c = CCase::new("C0034");
        c.vboss([0.0, 0.0, 0.0], Z, 1.0, 1.0, 1.0);
        c.vcut([0.0, 0.0, 2.0], Z, 1.0 - 2e-4, 1.0 - 2e-4, 3.0);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "near-degenerate thin wall",
            "square tube, 1e-4 m walls (chi=0)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 4.0)));
    }
    // C0035: U-channel with a 1e-4 floor. The cut sketch sits at z=2 over a
    // body spanning z∈[0,1], so the floor thickness is 2.0 − depth (NOT
    // 3.0 − depth, which was the original authoring error C0035-F1: depth
    // 3.0−1e-4 reached z=−0.9999, a geometric through-cut that contradicted
    // the χ=2 pin; the kernel handled both geometries correctly).
    {
        let mut c = CCase::new("C0035");
        c.vboss([0.0, 0.0, 0.0], Z, 1.0, 1.0, 1.0);
        c.vcut([0.0, 0.0, 2.0], Z, 0.8, 0.8, 2.0 - 1e-4);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "near-degenerate thin wall",
            "blind pocket leaves a 1e-4 m floor (chi=2)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 4.0)));
    }
    // C0036/C0037: second boss on a plane tilted by a hair off the first's
    // top plane, interpenetrating 1e-3 deep. The Stage-0 coplanar gate must
    // NOT fire; the union must succeed. Overlap = 1e-3 × 1 × 1 to O(theta²)
    // (the linear tilt term integrates to zero by symmetry).
    for (id, theta_deg) in [("C0036", 1e-3), ("C0037", 1e-4)] {
        let theta = theta_deg * std::f64::consts::PI / 180.0;
        let n = [theta.sin(), 0.0, theta.cos()];
        let mut c = CCase::new(id);
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 1.0);
        c.extrude(
            [0.0, 0.0, 0.999],
            n,
            rect_profile(-0.5, -0.5, 1.0, 1.0),
            "rectangle",
            1.0,
            0.8,
            false,
        );
        let vol = 4.0 + 0.8 - 1e-3;
        let d = desc(
            &c,
            "near-coplanar tilt",
            &format!("boss tilted {theta_deg}° off the top plane, 1e-3 interpenetration (gate must not fire)"),
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 6.0)));
    }
    // C0038: mixed scale — 1 m body, 10 µm through-hole. Volume cannot see
    // the hole (rel 1e-10); the Euler characteristic (chi=0) is the oracle.
    {
        let mut c = CCase::new("C0038");
        c.vboss([0.0, 0.0, 0.0], Z, 1.0, 1.0, 1.0);
        c.vcut([0.0, 0.0, 2.0], Z, 1e-5, 1e-5, 3.0);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "mixed scale",
            "1 m cube with a 10 µm square through-hole (chi=0 is the oracle)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 4.0)));
    }
    // C0039: 20 µm rib on a 1 m slab — a survival/validity probe (volume and
    // bbox cannot see the rib; the case exercises tolerance policy: 2e-5 is
    // 20× the 1e-6 feature floor and must not be welded away or crash).
    {
        let mut c = CCase::new("C0039");
        c.vboss([0.0, 0.0, 0.0], Z, 1.0, 1.0, 0.1);
        c.vboss([0.0, 0.0, 0.09], Z, 2e-5, 0.5, 0.05);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "mixed scale",
            "20 µm rib boss on a 1 m slab (survival/validity probe, chi=2)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 4.0)));
    }
    // C0040: 100 m slab with a 1 mm through-hole (5 orders of magnitude).
    {
        let mut c = CCase::new("C0040");
        c.scale = 100.0;
        c.vboss([0.0, 0.0, 0.0], Z, 100.0, 100.0, 1.0);
        c.vcut([0.0, 0.0, 2.0], Z, 1e-3, 1e-3, 3.0);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "mixed scale",
            "100 m slab with a 1 mm square through-hole (chi=0 is the oracle)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 300.0)));
    }
    e
}

// ── Group 2a: M8 coplanar residue trackers (C0041–C0050) ───────────────────

fn family_coplanar_m8(dir: &Path) -> Vec<ManifestEntry> {
    use std::f64::consts::PI;
    let mut e = Vec::new();
    // C0041: crossing tunnels with IDENTICAL sections — coplanar tunnel
    // walls between cut #2's tool and cut #1's walls (the M8 twin of C0007).
    {
        let mut c = CCase::new("C0041");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 2.0);
        c.vcut([2.0, 0.0, 1.0], X, 0.6, 0.6, 3.5);
        c.vcut([0.0, 2.0, 1.0], Y, 0.6, 0.6, 3.5);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "M8 coplanar",
            "crossing tunnels, IDENTICAL sections — coplanar tunnel walls (g=3) [M8]",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(-4, vol, 6.0)));
    }
    // C0042: externally rim-tangent cylinders, caps coplanar (point contact
    // on the shared cap plane + lateral tangency line).
    {
        let mut c = CCase::new("C0042");
        c.extrude(
            [0.0, 0.0, 0.0],
            Z,
            true_circle_profile(0.0, 0.0, 0.5),
            "circle",
            0.5,
            1.0,
            false,
        );
        c.extrude(
            [1.0, 0.0, 0.0],
            Z,
            true_circle_profile(0.0, 0.0, 0.5),
            "circle",
            0.5,
            1.0,
            false,
        );
        let d = desc(
            &c,
            "M8 coplanar",
            "externally rim-tangent discs, coplanar caps (tangent line contact — loud rejection acceptable) [M8/tangency]",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs::curved(2, 2.0 * PI * 0.25, 4.0),
        ));
    }
    // C0043: internal rim tangency — small cylinder inside the big one,
    // touching the rim from inside. Union equals the big cylinder BY DESIGN
    // (the degenerate tangency is the test).
    {
        let mut c = CCase::new("C0043");
        c.extrude(
            [0.0, 0.0, 0.0],
            Z,
            true_circle_profile(0.0, 0.0, 1.0),
            "circle",
            1.0,
            1.0,
            false,
        );
        c.extrude(
            [0.6, 0.0, 0.0],
            Z,
            true_circle_profile(0.0, 0.0, 0.4),
            "circle",
            0.4,
            1.0,
            false,
        );
        let d = desc(
            &c,
            "M8 coplanar",
            "internally rim-tangent discs, coplanar caps (union equals operand A by design) [M8/tangency]",
        );
        e.push(write_c_case(dir, c, d, Knobs::curved(2, PI, 4.0)));
    }
    // C0044: flush annular stack — cylinder on cylinder cap-to-cap, then an
    // axial bore through both → tube (chi=0).
    {
        let mut c = CCase::new("C0044");
        c.extrude(
            [0.0, 0.0, 0.0],
            Z,
            true_circle_profile(0.0, 0.0, 1.0),
            "circle",
            1.0,
            1.0,
            false,
        );
        c.extrude(
            [0.0, 0.0, 1.0],
            Z,
            true_circle_profile(0.0, 0.0, 1.0),
            "circle",
            1.0,
            1.0,
            false,
        );
        c.extrude(
            [0.0, 0.0, 3.0],
            Z,
            true_circle_profile(0.0, 0.0, 0.3),
            "circle",
            0.3,
            5.0,
            true,
        );
        let d = desc(
            &c,
            "M8 coplanar",
            "flush cap-to-cap cylinder stack + axial bore through both (tube, chi=0) [M8-annular]",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs::curved(0, PI * (1.0 - 0.09) * 2.0, 5.0),
        ));
    }
    // C0045: edge-only contact (1D) — two boxes sharing exactly one edge.
    {
        let mut c = CCase::new("C0045");
        c.vboss([-0.5, 0.0, 0.0], Z, 1.0, 2.0, 1.0);
        c.vboss([0.5, 0.0, 1.0], Z, 1.0, 2.0, 1.0);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "M8 coplanar",
            "boxes sharing exactly one edge (1D contact — legitimately non-manifold, loud rejection acceptable) [M8-edge]",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 6.0)));
    }
    // C0046: corner-only contact (0D).
    {
        let mut c = CCase::new("C0046");
        c.vboss([-0.5, -0.5, 0.0], Z, 1.0, 1.0, 1.0);
        c.vboss([0.5, 0.5, 1.0], Z, 1.0, 1.0, 1.0);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "M8 coplanar",
            "boxes sharing exactly one vertex (0D contact — legitimately non-manifold, loud rejection acceptable) [M8-corner]",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 6.0)));
    }
    // C0047: holed-disc partner — flush disc partially covering a bored
    // tube's annular top (the task-#54 class).
    {
        let mut c = CCase::new("C0047");
        c.extrude(
            [0.0, 0.0, 0.0],
            Z,
            true_circle_profile(0.0, 0.0, 1.0),
            "circle",
            1.0,
            1.0,
            false,
        );
        c.extrude(
            [0.0, 0.0, 2.0],
            Z,
            true_circle_profile(0.0, 0.0, 0.4),
            "circle",
            0.4,
            3.0,
            true,
        );
        c.extrude(
            [0.2, 0.0, 1.0],
            Z,
            true_circle_profile(0.0, 0.0, 0.7),
            "circle",
            0.7,
            0.5,
            false,
        );
        let vol = PI * (1.0 - 0.16) * 1.0 + PI * 0.49 * 0.5;
        let d = desc(
            &c,
            "M8 coplanar",
            "flush disc caps a bored tube's annular top off-center (hole becomes pocket, chi=2) [M8 holed-disc]",
        );
        e.push(write_c_case(dir, c, d, Knobs::curved(2, vol, 4.0)));
    }
    // C0048: chained swiss-cheese — two flush plates, offset hole rings
    // (upper holes counterbore 0.1 into the lower plate to avoid a second
    // interface-coplanar cut bottom).
    {
        let mut c = CCase::new("C0048");
        c.extrude(
            [0.0, 0.0, 0.0],
            Z,
            true_circle_profile(0.0, 0.0, 1.5),
            "circle",
            1.5,
            0.4,
            false,
        );
        for k in 0..3 {
            let ang = 2.0 * PI * k as f64 / 3.0;
            c.extrude(
                [0.8 * ang.cos(), 0.8 * ang.sin(), 1.0],
                Z,
                true_circle_profile(0.0, 0.0, 0.2),
                "circle",
                0.2,
                2.0,
                true,
            );
        }
        c.extrude(
            [0.0, 0.0, 0.4],
            Z,
            true_circle_profile(0.0, 0.0, 1.5),
            "circle",
            1.5,
            0.4,
            false,
        );
        for k in 0..3 {
            let ang = 2.0 * PI * (k as f64 + 0.5) / 3.0;
            c.extrude(
                [0.8 * ang.cos(), 0.8 * ang.sin(), 2.0],
                Z,
                true_circle_profile(0.0, 0.0, 0.2),
                "circle",
                0.2,
                1.7,
                true,
            );
        }
        let vol = PI * (2.25 * 0.8 - 3.0 * 0.04 * 0.4 - 3.0 * 0.04 * 0.5);
        let d = desc(
            &c,
            "M8 coplanar",
            "two flush swiss-cheese plates, offset hole rings (all pockets, chi=2) [M8 chained F0086-90]",
        );
        e.push(write_c_case(dir, c, d, Knobs::curved(2, vol, 7.0)));
    }
    // C0049: flush cut — tool side wall coplanar with the body side face.
    {
        let mut c = CCase::new("C0049");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 1.0);
        c.vcut_uv([0.0, 0.0, 2.0], Z, -0.3, 0.2, 0.6, 0.8, 3.0);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "M8 coplanar",
            "through-cut whose tool wall is flush with the body side face (notch, chi=2) [M8 flush-cut]",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 6.0)));
    }
    // C0050: staircase of partial coplanar top/bottom overlaps (F0002 class,
    // chained 3 deep).
    {
        let mut c = CCase::new("C0050");
        c.vboss([1.0, 1.0, 0.0], Z, 2.0, 2.0, 1.0);
        c.vboss([2.0, 1.0, 1.0], Z, 2.0, 2.0, 1.0);
        c.vboss([3.0, 1.0, 2.0], Z, 2.0, 2.0, 1.0);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "M8 coplanar",
            "3-box staircase, each flush-stacked with partial top overlap (chi=2) [M8 partial-overlap chain]",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 8.0)));
    }
    e
}

// ── Group 2b: degree-4 / tangency cyl×cyl trackers (C0051–C0058) ───────────

fn family_cyl_degree4(dir: &Path) -> Vec<ManifestEntry> {
    use std::f64::consts::PI;
    let mut e = Vec::new();
    // Helper: vertical cylinder A r=0.5, z ∈ [0,2].
    let base_cyl = |c: &mut CCase| {
        c.extrude(
            [0.0, 0.0, 0.0],
            Z,
            true_circle_profile(0.0, 0.0, 0.5),
            "circle",
            0.5,
            2.0,
            false,
        );
    };
    // C0051/C0052: unequal-radius perpendicular crossing, union then cut.
    for (id, cut) in [("C0051", false), ("C0052", true)] {
        let mut c = CCase::new(id);
        base_cyl(&mut c);
        c.extrude(
            [2.0, 0.0, 1.0],
            X,
            true_circle_profile(0.0, 0.0, 0.3),
            "circle",
            0.3,
            3.5,
            cut,
        );
        let d = desc(
            &c,
            "degree-4 cyl×cyl",
            &format!(
                "unequal radii (0.5 vs 0.3), perpendicular axes, {} [M5]",
                if cut {
                    "through-cut (chi=0)"
                } else {
                    "union (chi=2)"
                }
            ),
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            // 8.0: the horizontal tool cylinder spans x ∈ [−1.5, 2], so the
            // union's bbox diagonal is ≈ 6.4 (measured; the union WORKS —
            // an M5 boundary correction).
            Knobs::tracker(if cut { 0 } else { 2 }, 8.0),
        ));
    }
    // C0053: unequal radii at 45°.
    {
        let mut c = CCase::new("C0053");
        base_cyl(&mut c);
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let n = [s, 0.0, s];
        c.extrude(
            [2.0 * s, 0.0, 1.0 + 2.0 * s],
            n,
            true_circle_profile(0.0, 0.0, 0.3),
            "circle",
            0.3,
            4.0,
            true,
        );
        let d = desc(
            &c,
            "degree-4 cyl×cyl",
            "unequal radii, axes at 45°, through-cut (chi=0) [M5-oblique]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(0, 6.0)));
    }
    // C0054: skew axes (non-intersecting, offset 0.15 < r+R).
    {
        let mut c = CCase::new("C0054");
        base_cyl(&mut c);
        c.extrude(
            [2.0, 0.15, 1.0],
            X,
            true_circle_profile(0.0, 0.0, 0.3),
            "circle",
            0.3,
            3.5,
            true,
        );
        let d = desc(
            &c,
            "degree-4 cyl×cyl",
            "skew axes (0.15 offset), through-cut (chi=0) [M5-skew]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(0, 6.0)));
    }
    // C0055: parallel external tangency, caps NOT coplanar (pure lateral
    // tangency line — the parallel-lines SSI degenerate case).
    {
        let mut c = CCase::new("C0055");
        base_cyl(&mut c);
        c.extrude(
            [1.0, 0.0, 0.5],
            Z,
            true_circle_profile(0.0, 0.0, 0.5),
            "circle",
            0.5,
            1.3,
            false,
        );
        let d = desc(
            &c,
            "degree-4 cyl×cyl",
            "parallel cylinders, external lateral tangency line (loud rejection acceptable) [KV9-tangency]",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs::curved(2, PI * 0.25 * 2.0 + PI * 0.25 * 1.3, 5.0),
        ));
    }
    // C0056: internal lateral tangency, cut.
    {
        let mut c = CCase::new("C0056");
        c.extrude(
            [0.0, 0.0, 0.0],
            Z,
            true_circle_profile(0.0, 0.0, 1.0),
            "circle",
            1.0,
            1.0,
            false,
        );
        c.extrude(
            [0.5, 0.0, 1.4],
            Z,
            true_circle_profile(0.0, 0.0, 0.5),
            "circle",
            0.5,
            1.2,
            true,
        );
        let d = desc(
            &c,
            "degree-4 cyl×cyl",
            "internal lateral tangency, cut (wall thins to zero at the tangent line) [KV9-tangency]",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs::curved(2, PI * 1.0 - PI * 0.25 * 1.0, 4.0),
        ));
    }
    // C0057: near-tangent parallel (overlap width 1e-6).
    {
        let mut c = CCase::new("C0057");
        base_cyl(&mut c);
        c.extrude(
            [0.999999, 0.0, 0.3],
            Z,
            true_circle_profile(0.0, 0.0, 0.5),
            "circle",
            0.5,
            1.4,
            false,
        );
        let d = desc(
            &c,
            "degree-4 cyl×cyl",
            "parallel cylinders overlapping by 1e-6 (sliver lens union) [M5 near-tangent]",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs::curved(2, PI * 0.25 * 2.0 + PI * 0.25 * 1.4, 5.0),
        ));
    }
    // C0058: EQUAL radii at 30° — the dual-ellipse analytical solver's
    // oblique case (may already pass; boundary-mapping probe either way).
    // Steinmetz oblique: V_int = 16 r³ / (3 sin θ).
    {
        let mut c = CCase::new("C0058");
        c.extrude(
            [0.0, 0.0, 0.0],
            Z,
            true_circle_profile(0.0, 0.0, 0.4),
            "circle",
            0.4,
            2.0,
            false,
        );
        let th: f64 = 30.0 * std::f64::consts::PI / 180.0;
        let n = [th.sin(), 0.0, th.cos()];
        c.extrude(
            [-1.75 * n[0], 0.0, 1.0 - 1.75 * n[2]],
            n,
            true_circle_profile(0.0, 0.0, 0.4),
            "circle",
            0.4,
            3.5,
            false,
        );
        let v_int = 16.0 * 0.4f64.powi(3) / (3.0 * th.sin());
        let vol = PI * 0.16 * 2.0 + PI * 0.16 * 3.5 - v_int;
        let d = desc(
            &c,
            "degree-4 cyl×cyl",
            "EQUAL radii at 30° oblique crossing, union (dual-ellipse probe, Steinmetz volume) [M5-probe]",
        );
        e.push(write_c_case(dir, c, d, Knobs::curved(2, vol, 6.0)));
    }
    e
}

// ── Group 2c: revolve compositions (C0059–C0070) ───────────────────────────
//
// Revolve profiles live on the XZ plane (normal +Y, axis = world Z through
// the origin). For that plane the canonical basis maps u → −x, v → +z, so a
// UV rectangle [−r_hi..−r_lo]×[z_lo..z_hi] is the (r, z) rectangle
// [r_lo..r_hi]×[z_lo..z_hi] on the +x side (verified by unit test below).

fn rz_rect(r_lo: f64, r_hi: f64, z_lo: f64, z_hi: f64) -> ProfileData {
    rect_profile(-r_hi, z_lo, r_hi - r_lo, z_hi - z_lo)
}

fn rz_circle(rc: f64, zc: f64, radius: f64) -> ProfileData {
    true_circle_profile(-rc, zc, radius)
}

/// (r, z) polygon → UV polygon on the +Y-normal plane (u = −r, v = z),
/// re-wound CCW in UV.
fn rz_polygon(pts: &[(f64, f64)]) -> ProfileData {
    let mut uv: Vec<(f64, f64)> = pts.iter().map(|&(r, z)| (-r, z)).collect();
    if shoelace_area(&uv) < 0.0 {
        uv.reverse();
    }
    polygon_profile(&uv)
}

/// Exact ∫ π (a + b z)² dz over [z0, z1] (solid-of-revolution segment).
fn cone_segment_volume(a: f64, b: f64, z0: f64, z1: f64) -> f64 {
    let f = |z: f64| a * a * z + a * b * z * z + b * b * z * z * z / 3.0;
    std::f64::consts::PI * (f(z1) - f(z0))
}

fn family_revolve(dir: &Path) -> Vec<ManifestEntry> {
    use std::f64::consts::PI;
    let axis_o = [0.0, 0.0, 0.0];
    let axis_d = [0.0, 0.0, 1.0];
    let mut e = Vec::new();
    // C0059: 90° partial revolve + extrude cut near the start azimuth.
    {
        let mut c = CCase::new("C0059");
        c.revolve(
            [0.0; 3],
            Y,
            rz_rect(0.5, 1.0, 0.0, 0.5),
            "rectangle",
            0.5,
            axis_o,
            axis_d,
            90.0,
            false,
        );
        c.extrude(
            [0.75, 0.0, 2.0],
            Z,
            rect_profile(-0.15, -0.15, 0.3, 0.3),
            "rectangle",
            0.3,
            3.0,
            true,
        );
        let d = desc(
            &c,
            "revolve composition",
            "90° partial revolve arm + vertical bore at the start azimuth [KV6b partial+boolean]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(2, 4.0)));
    }
    // C0060: square-section ring (360°) + through-notch severing the band.
    {
        let mut c = CCase::new("C0060");
        c.revolve(
            [0.0; 3],
            Y,
            rz_rect(1.0, 1.5, 0.0, 0.5),
            "rectangle",
            0.5,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        c.extrude(
            [1.25, 0.0, 2.0],
            Z,
            rect_profile(-0.4, -0.4, 0.8, 0.8),
            "rectangle",
            0.8,
            3.0,
            true,
        );
        let d = desc(
            &c,
            "revolve composition",
            "square-section ring + through-notch severs the band (C-ring, chi=2) [KV6 ring+cut]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(2, 5.0)));
    }
    // C0061: shaft + full-turn rectangular groove ring (revolve CUT).
    {
        let mut c = CCase::new("C0061");
        c.revolve(
            [0.0; 3],
            Y,
            rz_rect(0.0, 0.5, 0.0, 2.0),
            "rectangle",
            0.5,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        c.revolve(
            [0.0; 3],
            Y,
            rz_rect(0.4, 0.6, 0.9, 1.1),
            "rectangle",
            0.2,
            axis_o,
            axis_d,
            360.0,
            true,
        );
        let vol = PI * 0.25 * 2.0 - PI * (0.25 - 0.16) * 0.2;
        let d = desc(
            &c,
            "revolve composition",
            "cylindrical shaft + full-turn rectangular groove ring (revolve cut, chi=2) [KV6 groove]",
        );
        e.push(write_c_case(dir, c, d, Knobs::curved(2, vol, 5.0)));
    }
    // C0062: coaxial revolve-on-revolve, axially interpenetrating (NOT flush).
    {
        let mut c = CCase::new("C0062");
        c.revolve(
            [0.0; 3],
            Y,
            rz_rect(0.0, 0.6, 0.0, 1.0),
            "rectangle",
            0.6,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        c.revolve(
            [0.0; 3],
            Y,
            rz_rect(0.0, 0.4, 0.8, 1.8),
            "rectangle",
            0.4,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        let vol = PI * (0.36 + 0.16 - 0.16 * 0.2);
        let d = desc(
            &c,
            "revolve composition",
            "coaxial revolve-on-revolve union, interpenetrating (chi=2) [KV6 revolve-on-revolve]",
        );
        e.push(write_c_case(dir, c, d, Knobs::curved(2, vol, 5.0)));
    }
    // C0063: full cone + OBLIQUE box cut (the named KV6c oblique wall).
    {
        let mut c = CCase::new("C0063");
        c.revolve(
            [0.0; 3],
            Y,
            rz_polygon(&[(0.0, 0.0), (0.8, 0.0), (0.0, 1.2)]),
            "polygon",
            0.8,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        let n = [0.3, 0.0, (1.0f64 - 0.09).sqrt()];
        c.extrude(
            [0.5, 0.0, 1.1],
            n,
            rect_profile(-1.0, -1.0, 2.0, 2.0),
            "rectangle",
            2.0,
            1.5,
            true,
        );
        let d = desc(
            &c,
            "revolve composition",
            "full cone + oblique slab cut (conic-bounded patch) [KV6c-oblique]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(2, 4.0)));
    }
    // C0064: three coaxial stacked frusta, each interpenetrating the last.
    // Union profile r(z) is piecewise linear; volume integrated exactly.
    {
        let mut c = CCase::new("C0064");
        c.revolve(
            [0.0; 3],
            Y,
            rz_polygon(&[(0.0, 0.0), (0.8, 0.0), (0.6, 0.5), (0.0, 0.5)]),
            "polygon",
            0.8,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        c.revolve(
            [0.0; 3],
            Y,
            rz_polygon(&[(0.0, 0.4), (0.55, 0.4), (0.35, 0.9), (0.0, 0.9)]),
            "polygon",
            0.55,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        c.revolve(
            [0.0; 3],
            Y,
            rz_polygon(&[(0.0, 0.8), (0.3, 0.8), (0.15, 1.3), (0.0, 1.3)]),
            "polygon",
            0.3,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        // Union r(z): F1 wins [0, 0.5], F2 wins [0.5, 0.9], F3 wins [0.9, 1.3].
        let vol = cone_segment_volume(0.8, -0.4, 0.0, 0.5)
            + cone_segment_volume(0.71, -0.4, 0.5, 0.9)
            + cone_segment_volume(0.54, -0.3, 0.9, 1.3);
        let d = desc(
            &c,
            "revolve composition",
            "3 coaxial stacked frusta, interpenetrating unions (chi=2) [KV6c frusta chain]",
        );
        e.push(write_c_case(dir, c, d, Knobs::curved(2, vol, 4.0)));
    }
    // C0065: full torus + through-notch severing the ring.
    {
        let mut c = CCase::new("C0065");
        c.revolve(
            [0.0; 3],
            Y,
            rz_circle(1.2, 0.5, 0.3),
            "circle",
            0.3,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        c.extrude(
            [1.2, 0.0, 2.0],
            Z,
            rect_profile(-0.25, -0.25, 0.5, 0.5),
            "rectangle",
            0.5,
            3.0,
            true,
        );
        let d = desc(
            &c,
            "revolve composition",
            "full torus + through-notch severs the ring (chi=2) [KV6d torus boolean]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(2, 5.0)));
    }
    // C0066: 90° partial torus + bore near the start azimuth.
    {
        let mut c = CCase::new("C0066");
        c.revolve(
            [0.0; 3],
            Y,
            rz_circle(1.0, 0.3, 0.25),
            "circle",
            0.25,
            axis_o,
            axis_d,
            90.0,
            false,
        );
        c.extrude(
            [1.0, 0.0, 2.0],
            Z,
            rect_profile(-0.12, -0.12, 0.24, 0.24),
            "rectangle",
            0.24,
            3.0,
            true,
        );
        let d = desc(
            &c,
            "revolve composition",
            "90° partial torus + vertical bore at the start azimuth [KV6d partial+boolean]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(2, 4.0)));
    }
    // C0067: sphere-degenerate revolve (circle centered ON the axis) + notch.
    // The revolve axis passes through the profile — the F0073/F0074 canary
    // class; a typed rejection is an acceptable boundary answer.
    {
        let mut c = CCase::new("C0067");
        c.revolve(
            [0.0; 3],
            Y,
            rz_circle(0.0, 0.5, 0.4),
            "circle",
            0.4,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        c.extrude(
            [0.0, 0.0, 2.0],
            Z,
            rect_profile(-0.15, -0.15, 0.3, 0.3),
            "rectangle",
            0.3,
            1.3,
            true,
        );
        let d = desc(
            &c,
            "revolve composition",
            "sphere via on-axis circle revolve + polar notch (axis-through-profile probe) [KV6 sphere]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(2, 3.0)));
    }
    // C0068: washer flange — genus-5: ring (g=1) + 4 through-bores.
    {
        let mut c = CCase::new("C0068");
        c.revolve(
            [0.0; 3],
            Y,
            rz_rect(0.8, 1.2, 0.0, 0.3),
            "rectangle",
            0.4,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        for (bx, by) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            c.extrude(
                [bx, by, 1.0],
                Z,
                rect_profile(-0.075, -0.075, 0.15, 0.15),
                "rectangle",
                0.15,
                2.0,
                true,
            );
        }
        let vol = PI * (1.44 - 0.64) * 0.3 - 4.0 * (0.15 * 0.15 * 0.3);
        let d = desc(
            &c,
            "revolve composition",
            "washer + 4 square through-bores in the band (g=5, chi=-8) [KV6 flange genus]",
        );
        e.push(write_c_case(dir, c, d, Knobs::curved(-8, vol, 5.0)));
    }
    // C0069: lathe part — shaft + 3 groove rings.
    {
        let mut c = CCase::new("C0069");
        c.revolve(
            [0.0; 3],
            Y,
            rz_rect(0.0, 0.5, 0.0, 2.0),
            "rectangle",
            0.5,
            axis_o,
            axis_d,
            360.0,
            false,
        );
        for (z0, z1) in [(0.4, 0.55), (0.95, 1.1), (1.5, 1.65)] {
            c.revolve(
                [0.0; 3],
                Y,
                rz_rect(0.4, 0.6, z0, z1),
                "rectangle",
                0.2,
                axis_o,
                axis_d,
                360.0,
                true,
            );
        }
        let vol = PI * 0.25 * 2.0 - 3.0 * (PI * (0.25 - 0.16) * 0.15);
        let d = desc(
            &c,
            "revolve composition",
            "lathe shaft with 3 full-turn groove rings (chi=2) [KV6 lathe chain]",
        );
        e.push(write_c_case(dir, c, d, Knobs::curved(2, vol, 5.0)));
    }
    // C0070: revolve about a TILTED axis ([1,1,1]/√3) — probes the
    // axis-aligned assumption in the KV6a polygon-revolve path.
    {
        let mut c = CCase::new("C0070");
        let s3 = 3.0f64.sqrt();
        let axis = [1.0 / s3, 1.0 / s3, 1.0 / s3];
        let s2 = std::f64::consts::FRAC_1_SQRT_2;
        let n = [s2, -s2, 0.0]; // plane containing the axis
        let basis = SketchPlaneBasis::from_origin_normal([0.0; 3], n);
        // Axis direction in UV; place a 0.4×0.4 profile 0.8 off-axis.
        let au = axis[0] * basis.x_axis[0] + axis[1] * basis.x_axis[1] + axis[2] * basis.x_axis[2];
        let av = axis[0] * basis.y_axis[0] + axis[1] * basis.y_axis[1] + axis[2] * basis.y_axis[2];
        let al = (au * au + av * av).sqrt();
        let (pu, pv) = (-av / al, au / al); // in-plane perpendicular to the axis
        c.revolve(
            [0.0; 3],
            n,
            rect_profile(0.8 * pu - 0.2, 0.8 * pv - 0.2, 0.4, 0.4),
            "rectangle",
            0.4,
            [0.0; 3],
            axis,
            360.0,
            false,
        );
        let d = desc(
            &c,
            "revolve composition",
            "full revolve about the tilted axis (1,1,1)/√3 (axis-alignment probe) [KV6a-tilted]",
        );
        // euler_target = 0 (R0099-precedent authoring rule): a full-turn
        // revolve of a simple profile strictly off-axis is a solid-torus
        // ring — genus 1, χ = 0. The original χ=2 was an authoring error;
        // the profile sits 0.6..1.0 off the axis, so no spindle forms.
        e.push(write_c_case(dir, c, d, Knobs::tracker(0, 4.0)));
    }
    e
}

// ── Group 2d: multi-shell re-entry trackers (C0071–C0074) ──────────────────
//
// Descriptions contain "internal-void" — the deliberate-void exemption the
// no-op guard honors (the enclosed cavity is the point of these cases).

fn family_multishell(dir: &Path) -> Vec<ManifestEntry> {
    let mut e = Vec::new();
    // Body with one enclosed void: 2×2×1 box minus 0.6×0.6×0.3 interior box.
    let void_body = |c: &mut CCase| {
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 1.0);
        c.vcut([0.0, 0.0, 0.65], Z, 0.6, 0.6, 0.3);
    };
    // C0071: breach the void from above (re-enters yang with a 2-shell operand).
    {
        let mut c = CCase::new("C0071");
        void_body(&mut c);
        c.vcut([0.0, 0.0, 2.0], Z, 0.2, 0.2, 1.6);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "multi-shell re-entry",
            "internal-void body, then a cut breaches the cavity (chi=2 after breach) [KV7]",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 6.0)));
    }
    // C0072: boss ON a void body (2 shells stay, chi = 4).
    {
        let mut c = CCase::new("C0072");
        void_body(&mut c);
        c.vboss([0.6, 0.6, 0.8], Z, 0.5, 0.5, 0.6);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "multi-shell re-entry",
            "internal-void body + external boss union (2 shells, chi=4) [KV7]",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(4, vol, 6.0)));
    }
    // C0073: TWO enclosed voids (3 shells, chi = 6).
    {
        let mut c = CCase::new("C0073");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 1.0);
        c.vcut([-0.5, 0.0, 0.65], Z, 0.6, 0.6, 0.3);
        c.vcut([0.5, 0.0, 0.65], Z, 0.6, 0.6, 0.3);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "multi-shell re-entry",
            "two internal-void cavities side by side (3 shells, chi=6) [KV7]",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(6, vol, 6.0)));
    }
    // C0074: void body ∩ half-slab (Intersect combine keeps the cavity).
    {
        let mut c = CCase::new("C0074");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 1.0);
        let void_cut = c.vcut([-0.5, 0.0, 0.65], Z, 0.6, 0.6, 0.3);
        c.extrude_with(
            [-0.75, 0.0, -0.5],
            Z,
            rect_profile(-0.75, -1.5, 1.5, 3.0),
            "rectangle",
            1.5,
            2.5,
            false,
            |p| {
                p.combine = Some(CombineMode::Intersect);
                // Explicit target: an interpenetrating slab shares no face,
                // so Auto (share-a-face) would resolve nothing (cf. C0081).
                p.targets = Some(vec![body_ref(void_cut)]);
            },
        );
        c.vops.push(VOp::Int(tool_box(
            [-0.75, 0.0, -0.5],
            Z,
            -0.75,
            -1.5,
            1.5,
            3.0,
            (0.0, 2.5),
        )));
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "multi-shell re-entry",
            "internal-void body intersected with a half-slab, cavity kept (2 shells, chi=4) [KV7 intersect]",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(4, vol, 6.0)));
    }
    e
}

// ── Group 2e: gear / CDT tail (C0075–C0078) ────────────────────────────────

fn gear_profile(teeth: u32, module: f64) -> (ProfileData, f64) {
    let params = waffle_types::GearParams {
        tooth_count: teeth,
        module,
        pressure_angle_deg: 20.0,
        ..Default::default()
    };
    let pitch_radius = (teeth as f64) * module / 2.0;
    (
        (
            vec![SketchEntity::Gear {
                id: 1,
                params,
                construction: false,
            }],
            HashMap::new(),
            vec![],
        ),
        pitch_radius,
    )
}

fn family_gear_cdt(dir: &Path) -> Vec<ManifestEntry> {
    let mut e = Vec::new();
    // C0075: two overlapping gears, union at scale 1.
    {
        let mut c = CCase::new("C0075");
        let (g1, p1) = gear_profile(12, 0.08);
        let (g2, p2) = gear_profile(12, 0.08);
        c.extrude([0.0, 0.0, 0.0], Z, g1, "gear", p1, 0.4, false);
        c.extrude([0.6, 0.0, 0.0], Z, g2, "gear", p2, 0.4, false);
        let d = desc(
            &c,
            "gear/CDT tail",
            "two 12-tooth gears overlapping, union at scale 1 [CDT gear×gear]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(2, 4.0)));
    }
    // C0076: ring gear — gear boss minus smaller coaxial gear cut.
    {
        let mut c = CCase::new("C0076");
        let (g1, p1) = gear_profile(20, 0.08);
        let (g2, p2) = gear_profile(10, 0.06);
        c.extrude([0.0, 0.0, 0.0], Z, g1, "gear", p1, 0.4, false);
        c.extrude([0.0, 0.0, 1.0], Z, g2, "gear", p2, 2.0, true);
        let d = desc(
            &c,
            "gear/CDT tail",
            "20-tooth gear minus coaxial 10-tooth gear through-cut (ring gear, chi=0) [CDT ring-gear]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(0, 4.0)));
    }
    // C0077: 40-tooth gear, single extrude (pure CDT stress, no boolean).
    {
        let mut c = CCase::new("C0077");
        let (g, p) = gear_profile(40, 0.05);
        c.extrude([0.0, 0.0, 0.0], Z, g, "gear", p, 0.3, false);
        let d = desc(
            &c,
            "gear/CDT tail",
            "40-tooth gear single extrude (pure non-convex CDT stress) [CDT-40t]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(2, 4.0)));
    }
    // C0078: gear + centered square through-cut at scale 1 (the fast variant
    // of the pathological gear-microscale R0007 class).
    {
        let mut c = CCase::new("C0078");
        let (g, p) = gear_profile(12, 0.08);
        c.extrude([0.0, 0.0, 0.0], Z, g, "gear", p, 0.4, false);
        c.extrude(
            [0.0, 0.0, 1.0],
            Z,
            rect_profile(-0.15, -0.15, 0.3, 0.3),
            "rectangle",
            0.3,
            2.0,
            true,
        );
        let d = desc(
            &c,
            "gear/CDT tail",
            "12-tooth gear + square through-cut at scale 1 (chi=0) [CDT gear+cut]",
        );
        e.push(write_c_case(dir, c, d, Knobs::tracker(0, 3.0)));
    }
    e
}

// ── Group 3a: explicit combine modes / targets (C0079–C0084) ───────────────

fn family_combine_modes(dir: &Path) -> Vec<ManifestEntry> {
    let mut e = Vec::new();
    // C0079: A + B(NewBody, disjoint) + C bridging with Add targets [A, B].
    {
        let mut c = CCase::new("C0079");
        let a = c.vboss([-1.5, 0.0, 0.0], Z, 1.0, 1.0, 1.0);
        let b = c.extrude_with(
            [1.5, 0.0, 0.0],
            Z,
            rect_profile(-0.5, -0.5, 1.0, 1.0),
            "rectangle",
            1.0,
            1.0,
            false,
            |p| p.combine = Some(CombineMode::NewBody),
        );
        c.vops.push(VOp::Add(tool_box(
            [1.5, 0.0, 0.0],
            Z,
            -0.5,
            -0.5,
            1.0,
            1.0,
            (0.0, 1.0),
        )));
        c.extrude_with(
            [0.0, 0.0, 0.25],
            Z,
            rect_profile(-0.25, -1.5, 0.5, 3.0),
            "rectangle",
            0.5,
            0.5,
            false,
            |p| {
                p.combine = Some(CombineMode::Add);
                p.targets = Some(vec![body_ref(a), body_ref(b)]);
            },
        );
        c.vops.push(VOp::Add(tool_box(
            [0.0, 0.0, 0.25],
            Z,
            -0.25,
            -1.5,
            0.5,
            3.0,
            (0.0, 0.5),
        )));
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "combine modes",
            "NewBody pair bridged by an Add extrude with explicit targets [A,B] (dumbbell, 1 body)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 8.0)));
    }
    // C0080: three bodies; Cut with explicit target hits body B only.
    {
        let mut c = CCase::new("C0080");
        c.vboss([-2.0, 0.0, 0.0], Z, 1.0, 1.0, 1.0);
        let b = c.extrude_with(
            [0.0, 0.0, 0.0],
            Z,
            rect_profile(-0.5, -0.5, 1.0, 1.0),
            "rectangle",
            1.0,
            1.0,
            false,
            |p| p.combine = Some(CombineMode::NewBody),
        );
        c.vops.push(VOp::Add(tool_box(
            [0.0, 0.0, 0.0],
            Z,
            -0.5,
            -0.5,
            1.0,
            1.0,
            (0.0, 1.0),
        )));
        c.extrude_with(
            [2.0, 0.0, 0.0],
            Z,
            rect_profile(-0.5, -0.5, 1.0, 1.0),
            "rectangle",
            1.0,
            1.0,
            false,
            |p| p.combine = Some(CombineMode::NewBody),
        );
        c.vops.push(VOp::Add(tool_box(
            [2.0, 0.0, 0.0],
            Z,
            -0.5,
            -0.5,
            1.0,
            1.0,
            (0.0, 1.0),
        )));
        c.extrude_with(
            [0.0, 0.0, 2.0],
            Z,
            rect_profile(-0.2, -0.2, 0.4, 0.4),
            "rectangle",
            0.4,
            1.5,
            true,
            |p| {
                p.combine = Some(CombineMode::Cut);
                p.targets = Some(vec![body_ref(b)]);
            },
        );
        c.vops.push(VOp::Cut(tool_box(
            [0.0, 0.0, 2.0],
            Z,
            -0.2,
            -0.2,
            0.4,
            0.4,
            (-1.5, 0.0),
        )));
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "combine modes",
            "three bodies; blind Cut with explicit target pockets body B only (3 bodies)",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs {
                expected_solid_count: Some(3),
                ..Knobs::solid(2, vol, 8.0)
            },
        ));
    }
    // C0081: Intersect combine mode — result is the overlap block. Explicit
    // target: the interpenetrating tool shares no face, so the Auto
    // (share-a-face) strategy would resolve nothing.
    {
        let mut c = CCase::new("C0081");
        let a = c.vboss([0.0, 0.0, 0.0], Z, 1.0, 1.0, 1.0);
        c.extrude_with(
            [0.3, 0.2, 0.4],
            Z,
            rect_profile(-0.5, -0.5, 1.0, 1.0),
            "rectangle",
            1.0,
            1.0,
            false,
            |p| {
                p.combine = Some(CombineMode::Intersect);
                p.targets = Some(vec![body_ref(a)]);
            },
        );
        c.vops.push(VOp::Int(tool_box(
            [0.3, 0.2, 0.4],
            Z,
            -0.5,
            -0.5,
            1.0,
            1.0,
            (0.0, 1.0),
        )));
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "combine modes",
            "Intersect combine-mode extrude — result is the overlap block (chi=2)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 4.0)));
    }
    // C0082: Add with explicit NON-most-recent target.
    {
        let mut c = CCase::new("C0082");
        let a = c.vboss([-1.5, 0.0, 0.0], Z, 1.0, 1.0, 1.0);
        c.extrude_with(
            [1.5, 0.0, 0.0],
            Z,
            rect_profile(-0.5, -0.5, 1.0, 1.0),
            "rectangle",
            1.0,
            1.0,
            false,
            |p| p.combine = Some(CombineMode::NewBody),
        );
        c.vops.push(VOp::Add(tool_box(
            [1.5, 0.0, 0.0],
            Z,
            -0.5,
            -0.5,
            1.0,
            1.0,
            (0.0, 1.0),
        )));
        c.extrude_with(
            [-1.5, 0.0, 0.8],
            Z,
            rect_profile(-0.3, -0.3, 0.6, 0.6),
            "rectangle",
            0.6,
            0.7,
            false,
            |p| {
                p.combine = Some(CombineMode::Add);
                p.targets = Some(vec![body_ref(a)]);
            },
        );
        c.vops.push(VOp::Add(tool_box(
            [-1.5, 0.0, 0.8],
            Z,
            -0.3,
            -0.3,
            0.6,
            0.6,
            (0.0, 0.7),
        )));
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "combine modes",
            "Add with explicit non-most-recent target (boss lands on body A; 2 bodies)",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs {
                expected_solid_count: Some(2),
                ..Knobs::solid(2, vol, 8.0)
            },
        ));
    }
    // C0083: NewBody OVERLAPPING the existing body — two co-located bodies;
    // volume oracle sums bodies (overlap counted twice by design).
    {
        let mut c = CCase::new("C0083");
        c.vboss([0.0, 0.0, 0.0], Z, 1.0, 1.0, 1.0);
        c.extrude_with(
            [0.3, 0.0, 0.5],
            Z,
            rect_profile(-0.4, -0.4, 0.8, 0.8),
            "rectangle",
            0.8,
            1.0,
            false,
            |p| p.combine = Some(CombineMode::NewBody),
        );
        let vol = 1.0 + 0.8 * 0.8 * 1.0;
        let d = desc(
            &c,
            "combine modes",
            "NewBody overlapping the existing body (2 independent bodies, summed volume)",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs {
                expected_solid_count: Some(2),
                ..Knobs::solid(2, vol, 4.0)
            },
        ));
    }
    // C0084: explicit BooleanCombine subtract feature (A − B).
    {
        let mut c = CCase::new("C0084");
        let a = c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 1.0);
        let b = c.extrude_with(
            [0.0, 0.0, -0.5],
            Z,
            rect_profile(-0.25, -1.5, 0.5, 3.0),
            "rectangle",
            0.5,
            1.0,
            false,
            |p| p.combine = Some(CombineMode::NewBody),
        );
        // Half-depth channel (z ∈ [−0.5, 0.5]) — the slab stays connected
        // through its upper half.
        c.vops.push(VOp::Cut(tool_box(
            [0.0, 0.0, -0.5],
            Z,
            -0.25,
            -1.5,
            0.5,
            3.0,
            (0.0, 1.0),
        )));
        c.boolean(a, b, BooleanOp::Subtract);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "combine modes",
            "explicit BooleanCombine subtract: through-bar tool notches a channel across the slab (chi=2)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 6.0)));
    }
    e
}

// ── Group 3b: depth modes / directions (C0085–C0090) ───────────────────────

fn family_depth_modes(dir: &Path) -> Vec<ManifestEntry> {
    let mut e = Vec::new();
    // C0085: symmetric boss (depth d each way).
    {
        let mut c = CCase::new("C0085");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 0.4);
        c.extrude_with(
            [0.0, 0.0, 0.2],
            Z,
            rect_profile(-0.15, -0.8, 0.3, 1.6),
            "rectangle",
            0.3,
            0.5,
            false,
            |p| p.symmetric = true,
        );
        c.vops.push(VOp::Add(tool_box(
            [0.0, 0.0, 0.2],
            Z,
            -0.15,
            -0.8,
            0.3,
            1.6,
            (-0.5, 0.5),
        )));
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "depth modes",
            "symmetric rib boss spanning both sides of the slab (chi=2)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 6.0)));
    }
    // C0086: symmetric through-cut.
    {
        let mut c = CCase::new("C0086");
        c.vboss([0.0, 0.0, 0.0], Z, 3.0, 3.0, 0.4);
        c.extrude_with(
            [0.0, 0.0, 0.2],
            Z,
            rect_profile(-0.25, -0.25, 0.5, 0.5),
            "rectangle",
            0.5,
            1.0,
            true,
            |p| p.symmetric = true,
        );
        c.vops.push(VOp::Cut(tool_box(
            [0.0, 0.0, 0.2],
            Z,
            -0.25,
            -0.25,
            0.5,
            0.5,
            (-1.0, 1.0),
        )));
        let vol = c.chain_vol();
        let d = desc(&c, "depth modes", "symmetric through-cut (chi=0)");
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 6.0)));
    }
    // C0087: second_direction Blind — asymmetric bidirectional boss.
    {
        let mut c = CCase::new("C0087");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 0.4);
        c.extrude_with(
            [0.0, 0.0, 0.35],
            Z,
            rect_profile(-0.15, -0.6, 0.3, 1.2),
            "rectangle",
            0.3,
            0.3,
            false,
            |p| p.second_direction = Some(SecondDirection::Blind { depth: 0.5 }),
        );
        c.vops.push(VOp::Add(tool_box(
            [0.0, 0.0, 0.35],
            Z,
            -0.15,
            -0.6,
            0.3,
            1.2,
            (-0.5, 0.3),
        )));
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "depth modes",
            "bidirectional boss, Blind second direction (0.3 up / 0.5 down, chi=2)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 6.0)));
    }
    // C0088: ThroughAll cut through a 2-level stack.
    {
        let mut c = CCase::new("C0088");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 1.0);
        c.vboss([0.4, 0.0, 0.8], Z, 1.2, 1.2, 1.0);
        c.extrude_with(
            [0.4, 0.0, 3.0],
            Z,
            rect_profile(-0.15, -0.15, 0.3, 0.3),
            "rectangle",
            0.3,
            4.0, // ignored by ThroughAll; sized so the no-op guard's Blind model sees the body
            true,
            |p| {
                p.depth_mode = DepthMode::ThroughAll;
                // ThroughAll resolves its extent along the given direction and
                // does NOT auto-reverse toward the body like a Blind cut —
                // aim it explicitly (sketch sits above the stack).
                p.direction = Some([0.0, 0.0, -1.0]);
            },
        );
        c.vops.push(VOp::Cut(tool_box(
            [0.4, 0.0, 3.0],
            Z,
            -0.15,
            -0.15,
            0.3,
            0.3,
            (-4.0, 0.0),
        )));
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "depth modes",
            "ThroughAll cut pierces the whole 2-level stack (chi=0)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 8.0)));
    }
    // C0089: explicit reversed direction boss (direction = −normal).
    {
        let mut c = CCase::new("C0089");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 0.4);
        c.extrude_with(
            [0.5, 0.0, 1.0],
            Z,
            rect_profile(-0.2, -0.2, 0.4, 0.4),
            "rectangle",
            0.4,
            0.8,
            false,
            |p| p.direction = Some([0.0, 0.0, -1.0]),
        );
        c.vops.push(VOp::Add(tool_box(
            [0.5, 0.0, 1.0],
            Z,
            -0.2,
            -0.2,
            0.4,
            0.4,
            (-0.8, 0.0),
        )));
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "depth modes",
            "boss with explicit reversed direction (−normal) reaching down into the slab (chi=2)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, vol, 6.0)));
    }
    // C0090: symmetric boss + ThroughAll cut combined.
    {
        let mut c = CCase::new("C0090");
        c.vboss([0.0, 0.0, 0.0], Z, 2.0, 2.0, 0.4);
        c.extrude_with(
            [0.0, 0.0, 0.2],
            Z,
            rect_profile(-0.2, -0.9, 0.4, 1.8),
            "rectangle",
            0.4,
            0.6,
            false,
            |p| p.symmetric = true,
        );
        c.vops.push(VOp::Add(tool_box(
            [0.0, 0.0, 0.2],
            Z,
            -0.2,
            -0.9,
            0.4,
            1.8,
            (-0.6, 0.6),
        )));
        c.extrude_with(
            [0.0, 0.0, 3.0],
            Z,
            rect_profile(-0.1, -0.1, 0.2, 0.2),
            "rectangle",
            0.2,
            4.5, // ignored by ThroughAll; guard-visible depth hint
            true,
            |p| {
                p.depth_mode = DepthMode::ThroughAll;
                p.direction = Some([0.0, 0.0, -1.0]); // see C0088
            },
        );
        c.vops.push(VOp::Cut(tool_box(
            [0.0, 0.0, 3.0],
            Z,
            -0.1,
            -0.1,
            0.2,
            0.2,
            (-4.5, 0.0),
        )));
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "depth modes",
            "symmetric rib + ThroughAll cut through rib and slab (chi=0)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 6.0)));
    }
    e
}

// ── Group 3c: holed / multi-profile sketches (C0091–C0096) ─────────────────

/// Append a rectangular loop (4 points + 4 lines) with the given id base.
/// `reversed` yields clockwise winding (the hole convention).
#[allow(clippy::too_many_arguments)]
fn rect_loop(
    id_base: u32,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    reversed: bool,
    entities: &mut Vec<SketchEntity>,
    positions: &mut HashMap<u32, (f64, f64)>,
) {
    let mut corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    if reversed {
        corners.reverse();
    }
    for (i, &(px, py)) in corners.iter().enumerate() {
        let id = id_base + i as u32;
        entities.push(SketchEntity::Point {
            id,
            x: px,
            y: py,
            construction: false,
        });
        positions.insert(id, (px, py));
    }
    for i in 0..4u32 {
        entities.push(SketchEntity::Line {
            id: id_base + 4 + i,
            start_id: id_base + i,
            end_id: id_base + (i + 1) % 4,
            construction: false,
        });
    }
}

/// Multi-loop profile data with hand-built `ClosedProfile`s (the
/// `rect_profile` convention: point ids as entity/vertex ids, `is_outer`
/// false for holes). Deterministic by construction — `extract_profiles`'s
/// unbounded-face filter drops only ONE exterior face, so DISJOINT loops
/// would leave a spurious CW "hole" behind.
fn loops_profile(loops: &[(f64, f64, f64, f64, bool)]) -> ProfileData {
    use waffle_types::ClosedProfile;
    let mut entities = Vec::new();
    let mut positions = HashMap::new();
    let mut profiles = Vec::new();
    for (i, &(x, y, w, h, hole)) in loops.iter().enumerate() {
        let base = 1 + 20 * i as u32;
        rect_loop(base, x, y, w, h, hole, &mut entities, &mut positions);
        profiles.push(ClosedProfile {
            entity_ids: (base..base + 4).collect(),
            is_outer: !hole,
            vertex_ids: (base..base + 4).collect(),
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        });
    }
    (entities, positions, profiles)
}

fn family_holed_profiles(dir: &Path) -> Vec<ManifestEntry> {
    let mut e = Vec::new();
    // C0091: one-op annular square (outer + concentric hole) → g=1.
    {
        let mut c = CCase::new("C0091");
        c.extrude(
            [0.0; 3],
            Z,
            loops_profile(&[(-1.0, -1.0, 2.0, 2.0, false), (-0.5, -0.5, 1.0, 1.0, true)]),
            "polygon",
            2.0,
            0.5,
            false,
        );
        let d = desc(
            &c,
            "holed profile",
            "one-op annular square extrude (KV14 holed profile, g=1, chi=0)",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs::solid(0, (4.0 - 1.0) * 0.5, 5.0),
        ));
    }
    // C0092: rectangle with THREE holes, one op → g=3.
    {
        let mut c = CCase::new("C0092");
        c.extrude(
            [0.0; 3],
            Z,
            loops_profile(&[
                (-1.5, -1.0, 3.0, 2.0, false),
                (-1.0, -0.25, 0.5, 0.5, true),
                (-0.25, -0.25, 0.5, 0.5, true),
                (0.5, -0.25, 0.5, 0.5, true),
            ]),
            "polygon",
            3.0,
            0.4,
            false,
        );
        let d = desc(
            &c,
            "holed profile",
            "one-op rectangle with 3 holes (g=3, chi=-4)",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs::solid(-4, (6.0 - 0.75) * 0.4, 6.0),
        ));
    }
    // C0093: two disjoint outer profiles in one sketch; extrude index 1 only.
    {
        let mut c = CCase::new("C0093");
        let profile = loops_profile(&[(-1.5, -0.5, 1.0, 1.0, false), (0.5, -0.5, 1.0, 1.0, false)]);
        let sketch_id = c.push_sketch([0.0; 3], Z, profile);
        c.extrude_existing(sketch_id, 1, "rectangle", 1.0, 0.5, false, |_| {});
        let d = desc(
            &c,
            "holed profile",
            "two disjoint profiles in one sketch; extrude profile_index=1 only (chi=2)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(2, 0.5, 4.0)));
    }
    // C0094: two extrudes off ONE sketch (index 0 and 1) — 2 bodies.
    {
        let mut c = CCase::new("C0094");
        let profile = loops_profile(&[(-1.5, -0.5, 1.0, 1.0, false), (0.5, -0.5, 1.0, 1.0, false)]);
        let sketch_id = c.push_sketch([0.0; 3], Z, profile);
        c.extrude_existing(sketch_id, 0, "rectangle", 1.0, 0.5, false, |_| {});
        // Depth differs from extrude 1: the no-op guard models both extrudes
        // with the whole-sketch frame (it cannot see profile_index), so equal
        // depths would read as a swallowed boss.
        c.extrude_existing(sketch_id, 1, "rectangle", 1.0, 0.7, false, |p| {
            p.combine = Some(CombineMode::NewBody);
        });
        let d = desc(
            &c,
            "holed profile",
            "one sketch, two extrudes (profile_index 0 and 1, NewBody — 2 bodies)",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs {
                expected_solid_count: Some(2),
                ..Knobs::solid(2, 0.5 + 0.7, 5.0)
            },
        ));
    }
    // C0095: holed boss + through-cut in the ring band → g=2.
    {
        let mut c = CCase::new("C0095");
        c.extrude(
            [0.0; 3],
            Z,
            loops_profile(&[(-1.5, -1.5, 3.0, 3.0, false), (-0.5, -0.5, 1.0, 1.0, true)]),
            "polygon",
            3.0,
            0.5,
            false,
        );
        c.extrude(
            [0.0, 0.0, 1.0],
            Z,
            rect_profile(0.8, -0.2, 0.4, 0.4),
            "rectangle",
            0.4,
            2.0,
            true,
        );
        let d = desc(
            &c,
            "holed profile",
            "annular-square boss + through-cut in the band (g=2, chi=-2)",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs::solid(-2, (9.0 - 1.0) * 0.5 - 0.16 * 0.5, 6.0),
        ));
    }
    // C0096: NON-CONVEX outer (L-shape) with a hole, one op.
    {
        let mut poly_entities = Vec::new();
        let mut poly_positions = HashMap::new();
        // L-shape: 3×3 square minus its upper-right 1.5×1.5 quadrant.
        let l_pts = [
            (-1.5, -1.5),
            (1.5, -1.5),
            (1.5, 0.0),
            (0.0, 0.0),
            (0.0, 1.5),
            (-1.5, 1.5),
        ];
        for (i, &(px, py)) in l_pts.iter().enumerate() {
            let id = 1 + i as u32;
            poly_entities.push(SketchEntity::Point {
                id,
                x: px,
                y: py,
                construction: false,
            });
            poly_positions.insert(id, (px, py));
        }
        let n = l_pts.len() as u32;
        for i in 0..n {
            poly_entities.push(SketchEntity::Line {
                id: 100 + i,
                start_id: 1 + i,
                end_id: 1 + (i + 1) % n,
                construction: false,
            });
        }
        rect_loop(
            41,
            -1.15,
            -1.15,
            0.8,
            0.8,
            true,
            &mut poly_entities,
            &mut poly_positions,
        );
        let profiles = vec![
            waffle_types::ClosedProfile {
                entity_ids: (1..=n).collect(),
                is_outer: true,
                vertex_ids: (1..=n).collect(),
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            },
            waffle_types::ClosedProfile {
                entity_ids: (41..45).collect(),
                is_outer: false,
                vertex_ids: (41..45).collect(),
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            },
        ];
        let mut c = CCase::new("C0096");
        c.extrude(
            [0.0; 3],
            Z,
            (poly_entities, poly_positions, profiles),
            "polygon",
            3.0,
            0.4,
            false,
        );
        let vol = (9.0 - 2.25 - 0.64) * 0.4;
        let d = desc(
            &c,
            "holed profile",
            "L-shaped outer with a square hole, one op (g=1, chi=0)",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 6.0)));
    }
    e
}

// ── Group 3d: region extrudes (C0097–C0100) ────────────────────────────────

fn circle_entities(
    specs: &[(u32, f64, f64, f64)], // (id_base, cx, cy, r)
) -> (Vec<SketchEntity>, HashMap<u32, (f64, f64)>) {
    let mut entities = Vec::new();
    let mut positions = HashMap::new();
    for &(id_base, cx, cy, r) in specs {
        entities.push(SketchEntity::Point {
            id: id_base,
            x: cx,
            y: cy,
            construction: true,
        });
        positions.insert(id_base, (cx, cy));
        entities.push(SketchEntity::Circle {
            id: id_base + 1,
            center_id: id_base,
            radius: r,
            construction: false,
        });
    }
    (entities, positions)
}

fn family_regions(dir: &Path) -> Vec<ManifestEntry> {
    use std::f64::consts::PI;
    use waffle_types::{compute_regions, regions::DEFAULT_CHORD_TOLERANCE};
    let mut e = Vec::new();
    // C0097: annulus region between concentric circles.
    {
        let (entities, positions) = circle_entities(&[(1, 0.0, 0.0, 1.0), (10, 0.0, 0.0, 0.45)]);
        let annulus = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE)
            .into_iter()
            .find(|r| !r.holes.is_empty())
            .expect("annulus region");
        let profiles = waffle_types::extract_profiles(&entities, &positions);
        let mut c = CCase::new("C0097");
        let sketch_id = c.push_sketch([0.0; 3], Z, (entities, positions, profiles));
        c.extrude_existing(sketch_id, 0, "circle", 1.0, 0.5, false, |p| {
            p.region = Some(annulus);
        });
        let vol = PI * (1.0 - 0.2025) * 0.5;
        let d = desc(
            &c,
            "region extrude",
            "annulus sub-region between concentric circles (g=1, chi=0)",
        );
        e.push(write_c_case(dir, c, d, Knobs::curved(0, vol, 4.0)));
    }
    // C0098: lens region of two crossing circles (smallest region).
    {
        let (entities, positions) = circle_entities(&[(1, -0.35, 0.0, 0.8), (10, 0.35, 0.0, 0.8)]);
        let lens = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE)
            .into_iter()
            .min_by(|a, b| a.area.partial_cmp(&b.area).unwrap())
            .expect("lens region");
        let profiles = waffle_types::extract_profiles(&entities, &positions);
        let mut c = CCase::new("C0098");
        let sketch_id = c.push_sketch([0.0; 3], Z, (entities, positions, profiles));
        c.extrude_existing(sketch_id, 0, "circle", 0.8, 0.4, false, |p| {
            p.region = Some(lens);
        });
        // Lens area, closed form: d = 0.7, r = 0.8.
        let (dd, r) = (0.7f64, 0.8f64);
        let lens_area =
            2.0 * r * r * (dd / (2.0 * r)).acos() - (dd / 2.0) * (4.0 * r * r - dd * dd).sqrt();
        let d = desc(
            &c,
            "region extrude",
            "lens sub-region of two crossing circles (chi=2)",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs::curved(2, lens_area * 0.4, 4.0),
        ));
    }
    // C0099: crescent region (circle minus lens).
    {
        let (entities, positions) = circle_entities(&[(1, -0.35, 0.0, 0.8), (10, 0.35, 0.0, 0.8)]);
        let (dd, r) = (0.7f64, 0.8f64);
        let lens_area =
            2.0 * r * r * (dd / (2.0 * r)).acos() - (dd / 2.0) * (4.0 * r * r - dd * dd).sqrt();
        let crescent_area = PI * r * r - lens_area;
        let crescent = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE)
            .into_iter()
            .filter(|reg| reg.holes.is_empty())
            .min_by(|a, b| {
                let da = (a.area - crescent_area).abs();
                let db = (b.area - crescent_area).abs();
                da.partial_cmp(&db).unwrap()
            })
            .expect("crescent region");
        assert!(
            (crescent.area - crescent_area).abs() / crescent_area < 0.02,
            "picked region area {} != analytic crescent {}",
            crescent.area,
            crescent_area
        );
        let profiles = waffle_types::extract_profiles(&entities, &positions);
        let mut c = CCase::new("C0099");
        let sketch_id = c.push_sketch([0.0; 3], Z, (entities, positions, profiles));
        c.extrude_existing(sketch_id, 0, "circle", 0.8, 0.4, false, |p| {
            p.region = Some(crescent);
        });
        let d = desc(
            &c,
            "region extrude",
            "crescent sub-region of two crossing circles (chi=2)",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs::curved(2, crescent_area * 0.4, 4.0),
        ));
    }
    // C0100: TWO adjacent sub-regions extruded as one body (the plural
    // `regions` path: 2D union before the extrude).
    {
        let mut entities = Vec::new();
        let mut positions = HashMap::new();
        rect_loop(
            1,
            -1.0,
            -0.5,
            2.0,
            1.0,
            false,
            &mut entities,
            &mut positions,
        );
        // Dividing line through x = 0.
        for (id, x, y) in [(30u32, 0.0, -0.5), (31, 0.0, 0.5)] {
            entities.push(SketchEntity::Point {
                id,
                x,
                y,
                construction: false,
            });
            positions.insert(id, (x, y));
        }
        entities.push(SketchEntity::Line {
            id: 32,
            start_id: 30,
            end_id: 31,
            construction: false,
        });
        let regions = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE);
        assert_eq!(regions.len(), 2, "split rectangle must yield 2 regions");
        let profiles = waffle_types::extract_profiles(&entities, &positions);
        let mut c = CCase::new("C0100");
        let sketch_id = c.push_sketch([0.0; 3], Z, (entities, positions, profiles));
        c.extrude_existing(sketch_id, 0, "rectangle", 2.0, 0.5, false, |p| {
            p.regions = regions;
        });
        let d = desc(
            &c,
            "region extrude",
            "two adjacent sub-regions extruded as ONE body (2D-union regions path, chi=2)",
        );
        e.push(write_c_case(
            dir,
            c,
            d,
            Knobs::solid(2, 2.0 * 1.0 * 0.5, 4.0),
        ));
    }
    e
}

// ── Entry point ────────────────────────────────────────────────────────────

/// Generate all 100 C-series cases into `output_dir`. Returns manifest
/// entries in id order (C0001–C0100, dense).
// ── Group 6: user-reported drivers (C0101–) ────────────────────────────────

/// Minimal deterministic replicas of user-reported failure configurations
/// (each cites its fixture + task). Unlike Groups 1/3 these may pin CURRENT
/// capability walls; their categories are tracked in `assay_kv2.rs`.
fn family_user_reported(dir: &Path) -> Vec<ManifestEntry> {
    let mut e = Vec::new();
    // C0101: flush bridge across two tower tops (user `error_coplanar.waffle`,
    // task #129, spec `m8_plane_group_nary_overlay`). The user's EXACT world
    // geometry (mm scale, unequal towers, all side planes corner-flush): a
    // 24.2×11.2×2 mm base slab extruded DOWN from z=0, two full-width towers
    // extruded 10 mm UP from base-top regions at the slab's ends, then a
    // 1 mm bridge slab spanning the full base footprint, its bottom flush
    // with BOTH tower tops — the bridge bottom face lands in TWO Stage-0
    // coplanar pairs (plus zero-area-overlap side-plane pairs), the M8
    // n-ary plane-group class. Result is a rectangular frame: g=1, chi=0.
    // (Round-number corner-flush variants of the TOWER unions trip the
    // separate pre-existing chiral `edge-not-2-directed` output wall; the
    // user's coordinates do not — the case pins the user's actual boundary.)
    {
        let mut c = CCase::new("C0101");
        c.scale = 0.024155844177585096;
        let x_lo = -0.012077922088792548;
        let x_hi = 0.012077922088792548;
        let y_half = 0.005603895318927243;
        let ta_lo = 0.003762989799724892; // tower A (+x end) inner wall
        let tb_hi = -0.005730517499614507; // tower B (−x end) inner wall
        let w = x_hi - x_lo;
        let h = 2.0 * y_half;
        // CENTERED profiles only (each sketch origin at its tool's center):
        // the independent `assay_noop_guard` models every sketch as a
        // centered box (hw = max |u|), so an off-center boss rectangle reads
        // as a full-span box and false-positives as `swallowed_boss`. The
        // ±ULP drift of the computed centers vs the user's raw corners is
        // immaterial — the near-coplanar scan band absorbs it (the user's
        // own bridge plane sat 2.2e-10 off the tower tops).
        // Base slab below z=0 (the user extruded the base downward).
        c.vboss([0.0, 0.0, -0.002], Z, w, h, 0.002);
        // Towers up from the shared z=0 plane (base-top regions).
        c.vboss([(ta_lo + x_hi) / 2.0, 0.0, 0.0], Z, x_hi - ta_lo, h, 0.01);
        c.vboss([(x_lo + tb_hi) / 2.0, 0.0, 0.0], Z, tb_hi - x_lo, h, 0.01);
        // Bridge slab flush on both tower tops.
        c.vboss([0.0, 0.0, 0.01], Z, w, h, 0.001);
        let vol = c.chain_vol();
        let d = desc(
            &c,
            "user-reported",
            "flush bridge across two tower tops — bridge bottom in TWO coplanar pairs (frame, g=1, chi=0) [M8 n-ary]",
        );
        e.push(write_c_case(dir, c, d, Knobs::solid(0, vol, 0.1)));
    }
    e
}

pub fn generate_complexity_cases(output_dir: &Path) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();
    entries.extend(family_genus(output_dir));
    entries.extend(family_chains(output_dir));
    entries.extend(family_nonconvex(output_dir));
    entries.extend(family_near_degenerate(output_dir));
    entries.extend(family_coplanar_m8(output_dir));
    entries.extend(family_cyl_degree4(output_dir));
    entries.extend(family_revolve(output_dir));
    entries.extend(family_multishell(output_dir));
    entries.extend(family_gear_cdt(output_dir));
    entries.extend(family_combine_modes(output_dir));
    entries.extend(family_depth_modes(output_dir));
    entries.extend(family_holed_profiles(output_dir));
    entries.extend(family_regions(output_dir));
    entries.extend(family_user_reported(output_dir));
    assert_eq!(entries.len(), 101, "C-series must be exactly 101 cases");
    for (i, en) in entries.iter().enumerate() {
        assert_eq!(
            en.id,
            format!("C{:04}", i + 1),
            "C-series ids must be dense and ordered"
        );
    }
    entries
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_volume_single_box() {
        let b = Box3 {
            lo: [0.0, 0.0, 0.0],
            hi: [2.0, 3.0, 0.5],
        };
        assert!((chain_volume(&[VOp::Add(b)]) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn chain_volume_overlap_cut_int() {
        let a = Box3 {
            lo: [0.0, 0.0, 0.0],
            hi: [1.0, 1.0, 1.0],
        };
        let b = Box3 {
            lo: [0.5, 0.0, 0.0],
            hi: [1.5, 1.0, 1.0],
        };
        // Union of overlapping unit boxes = 1.5.
        assert!((chain_volume(&[VOp::Add(a), VOp::Add(b)]) - 1.5).abs() < 1e-12);
        // Cut removes the overlap: 0.5 remains.
        assert!((chain_volume(&[VOp::Add(a), VOp::Cut(b)]) - 0.5).abs() < 1e-12);
        // Intersect keeps only the overlap: 0.5.
        assert!((chain_volume(&[VOp::Add(a), VOp::Int(b)]) - 0.5).abs() < 1e-12);
        // Cut then re-add part of it.
        let c = Box3 {
            lo: [0.5, 0.0, 0.0],
            hi: [1.0, 0.5, 1.0],
        };
        assert!((chain_volume(&[VOp::Add(a), VOp::Cut(b), VOp::Add(c)]) - 0.75).abs() < 1e-12);
    }

    #[test]
    fn tool_box_z_normal_centered() {
        let b = tool_box(
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            -0.2,
            -0.2,
            0.4,
            0.4,
            (-2.0, 0.0),
        );
        assert!((b.lo[0] + 0.2).abs() < 1e-12 && (b.hi[0] - 0.2).abs() < 1e-12);
        assert!((b.lo[1] + 0.2).abs() < 1e-12 && (b.hi[1] - 0.2).abs() < 1e-12);
        assert!((b.lo[2] + 1.0).abs() < 1e-12 && (b.hi[2] - 1.0).abs() < 1e-12);
    }

    /// The rz_* helpers assume the +Y-normal plane maps u → −x, v → +z.
    #[test]
    fn y_normal_basis_maps_u_to_negative_x() {
        let basis = SketchPlaneBasis::from_origin_normal([0.0; 3], [0.0, 1.0, 0.0]);
        let p = basis.local_to_world(-1.0, 0.5);
        assert!(
            (p[0] - 1.0).abs() < 1e-12,
            "u=-1 must land at x=+1, got {p:?}"
        );
        assert!(p[1].abs() < 1e-12);
        assert!((p[2] - 0.5).abs() < 1e-12, "v must map to +z, got {p:?}");
    }

    #[test]
    fn star_is_ccw_with_expected_area() {
        let s = star(5, 1.5, 0.6, 0.0);
        let a = shoelace_area(&s);
        // 5-star area = 5 · (isoceles triangles) — just sanity-check range.
        assert!(a > 1.0 && a < 7.0686, "star area {a} out of range");
    }

    #[test]
    fn generate_all_hundred_into_tempdir() {
        let dir = tempfile::tempdir().unwrap();
        let entries = generate_complexity_cases(dir.path());
        assert_eq!(entries.len(), 101);
        // C0001 meta: plate volume 8 − 2·(0.4·0.4·0.5) = 7.84, chi = −2.
        let meta: AssayMeta = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("C0001.meta.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(meta.oracles.euler_target, -2);
        let vol = meta
            .oracles
            .expected_volume
            .expect("C0001 has exact volume");
        assert!((vol - 7.84).abs() < 1e-9, "C0001 volume {vol}");
        // Every waffle file parses as JSON with a feature tree.
        for en in &entries {
            let json: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(dir.path().join(&en.filename)).unwrap(),
            )
            .unwrap();
            assert!(
                json["tabs"][0]["kind"]["features"]["features"].is_array(),
                "{}: waffle missing feature tree",
                en.id
            );
        }
    }
}
