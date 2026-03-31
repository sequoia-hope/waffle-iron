//! Tolerance and units constants for the geometry kernel.
//!
//! All distances are in meters. The kernel supports features down to 1 micrometer.
//! These constants are the single source of truth for tolerance policy (A14).
//!
//! Tolerance hierarchy (coarsest → finest):
//!   MIN_FEATURE_SIZE > TAU_PARALLEL > TAU_MODEL > TAU_COINCIDENT > TAU_WORK > TAU_NORMALIZE

/// Default model tolerance: 1e-7 meters (0.1 micrometers).
/// Used for coincidence decisions, join/weld admissibility, vertex welding.
pub const TAU_MODEL: f64 = 1e-7;

/// Reciprocal of TAU_MODEL (1e7). Used to quantize vertex positions to the
/// model-tolerance grid for welding (position-map keys in tessellation).
pub const TAU_MODEL_RECIP: f64 = 1.0 / TAU_MODEL;

/// Minimum feature size: 1e-6 meters (1 micrometer).
/// Features smaller than this may be collapsed.
pub const MIN_FEATURE_SIZE: f64 = 1e-6;

/// Numeric working precision floor: 1e-12 meters.
/// Used for iterative solver convergence, degenerate-geometry detection (area/length),
/// and precision floor for cross-product / normal-length checks.
pub const TAU_WORK: f64 = 1e-12;

/// Zero-length vector guard: 1e-15.
/// Used to prevent division by zero in vector normalization.
/// Also used as TAU_NORMALIZE² ≈ 1e-30 for cross-product magnitude checks.
pub const TAU_NORMALIZE: f64 = 1e-15;

/// Near-parallel / coplanar threshold: 1e-6.
/// Used for dot-product checks against 1.0 (parallel) or 0.0 (perpendicular).
pub const TAU_PARALLEL: f64 = 1e-6;

/// Point-on-surface / SSI coincidence tolerance: 1e-9.
/// Used in SSI solvers for height-range containment, point-in-solid classification,
/// and Z-range comparisons. Finer than TAU_MODEL to avoid premature snapping,
/// coarser than TAU_WORK to tolerate floating-point accumulation.
pub const TAU_COINCIDENT: f64 = 1e-9;

/// Relative weld factor for scale-adaptive tolerance: 1e-7.
/// tau_weld = (model_diagonal * TAU_WELD_FACTOR).clamp(TAU_WELD_MIN, TAU_WELD_MAX).
pub const TAU_WELD_FACTOR: f64 = 1e-7;

/// Minimum weld tolerance (clamp floor): 1e-12.
pub const TAU_WELD_MIN: f64 = 1e-12;

/// Maximum weld tolerance (clamp ceiling): 1e-4.
pub const TAU_WELD_MAX: f64 = 1e-4;

/// Relative grid factor for f32 vertex deduplication in tessellation: 1e-5.
/// Applied as (max_abs * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN).
pub const TAU_TESS_GRID_FACTOR: f64 = 1e-5;

/// Minimum tessellation grid size: 1e-10.
pub const TAU_TESS_GRID_MIN: f64 = 1e-10;

/// Relative mesh dedup factor for f32 oracle grid: 2e-6.
/// Used in tessellation watertightness oracle as (max_abs * TAU_ORACLE_FACTOR).max(TAU_ORACLE_MIN).
pub const TAU_ORACLE_FACTOR: f32 = 2e-6;

/// Minimum oracle grid size: 1e-4.
pub const TAU_ORACLE_MIN: f32 = 1e-4;

/// Relative quantization step for intersection cache: 1e-3.
/// Applied as (tau * TAU_CACHE_STEP_FACTOR).max(TAU_NORMALIZE).
pub const TAU_CACHE_STEP_FACTOR: f64 = 1e-3;

/// Relative snap grid for polygon clipping coordinates: 1e-4.
/// Applied as tau * TAU_SNAP_FACTOR.
pub const TAU_SNAP_FACTOR: f64 = 1e-4;

/// Minimum clamp for scale-adaptive tessellation welding: 1e-8.
pub const TAU_TESS_WELD_MIN: f64 = 1e-8;

/// Maximum clamp for scale-adaptive tessellation welding: 1e-2.
pub const TAU_TESS_WELD_MAX: f64 = 1e-2;

/// Fraction of tau_model used to derive tau_weld in BooleanOptions: 0.4.
/// tau_weld = tau_model * TAU_WELD_MODEL_RATIO.
/// Ensures weld tolerance is sub-model-tolerance to avoid premature vertex merging
/// while remaining coarser than working precision.
pub const TAU_WELD_MODEL_RATIO: f64 = 0.4;

/// Down-scaling factor from tau_weld to polygon-clipping intersection tolerance: 0.01.
/// The clip tolerance must be much tighter than vertex welding to avoid
/// misclassifying near-coincident intersection points.
pub const TAU_CLIP_FACTOR: f64 = 0.01;

// ── Topological & geometric classification thresholds ──────────────────

/// Cosine threshold for "nearly perpendicular" face classification: 0.1.
/// A dot product below this means the face normal is within ~84° of perpendicular
/// to the reference direction. Used in boolean analytical dispatch.
pub const COS_NEAR_PERPENDICULAR: f64 = 0.1;

/// Maximum unpaired half-edge ratio in strict stitching mode: 5%.
/// S-H clipping creates small T-junction gaps from independent floating-point
/// intersection computation; up to this ratio is tolerated.
pub const STITCH_UNPAIRED_STRICT: f64 = 0.05;

/// Maximum unpaired half-edge ratio in tolerant stitching mode: 60%.
/// Polygon approximation and SSI fallback may produce higher unpaired counts;
/// tessellation hole-filling repairs small boundary gaps.
pub const STITCH_UNPAIRED_TOLERANT: f64 = 0.60;

/// Generous tolerance for classifying polygon-clipping vertices as on-ellipse: 0.1.
/// Normalized (u²+v²) distance from 1.0; generous because polygon approximation
/// introduces chord error relative to the true ellipse.
pub const ELLIPSE_ON_CURVE_TOL: f64 = 0.1;

/// Angular margin (radians) for detecting full-circle cylinder sweeps: 0.1 (~5.7°).
/// If total swept angle exceeds TAU - this margin, treat as full cylinder.
pub const FULL_CIRCLE_MARGIN: f64 = 0.1;

/// Wider angular margin (radians) for detecting full-circle cone ring sweeps: 0.3 (~17.2°).
/// Cone vertex rings may have wider angular gaps than cylinder rings due to
/// non-uniform parametric spacing near the apex.
pub const FULL_CIRCLE_MARGIN_CONE: f64 = 0.3;

/// T-junction snap radius as fraction of tessellation grid cell: 0.6.
/// Slightly more than half a grid cell ensures vertices near edge midpoints snap.
pub const TJUNCTION_GRID_FRACTION: f64 = 0.6;

/// Minimum triangle area for T-junction split as fraction of TAU_TESS_GRID_MIN: 0.1.
/// Both sub-triangles must exceed this area to avoid creating degenerate geometry.
pub const TJUNCTION_AREA_FRACTION: f64 = 0.1;

/// Minimum tau_weld-to-tau_model ratio for BooleanOptions validation: 0.1.
/// Ensures the weld tolerance is at least 10% of model tolerance.
pub const TAU_WELD_MODEL_MIN_RATIO: f64 = 0.1;

// ── Winding-number classification thresholds ───────────────────────────

/// Winding-number threshold above which a point is classified as "inside": 0.5.
/// Ref #7: Jacobson et al. — generalized winding numbers.
pub const WINDING_INSIDE_THRESHOLD: f64 = 0.5;

/// Winding-number threshold below which a point is classified as "outside": 0.3.
/// Values in [WINDING_OUTSIDE_THRESHOLD, WINDING_INSIDE_THRESHOLD] are ambiguous.
pub const WINDING_OUTSIDE_THRESHOLD: f64 = 0.3;

// ── Duplicate-face detection thresholds ────────────────────────────────

/// Cosine threshold for near-parallel normal check in duplicate-face removal: 0.99.
/// Used in clip.rs polygon deduplication.
pub const COS_NEAR_PARALLEL: f64 = 0.99;

/// Relative area tolerance for duplicate-face detection: 1%.
/// Two faces with area difference exceeding this ratio are not duplicates.
pub const DUPLICATE_FACE_AREA_TOL: f64 = 0.01;

// ── SSI solver heuristic thresholds ────────────────────────────────────

/// Relative radii difference threshold for cylinder-cylinder SSI: 1%.
/// Radii differing by more than this are considered unequal.
pub const SSI_RADII_RELATIVE_TOL: f64 = 0.01;

/// Relative skew distance threshold for cylinder-cylinder SSI: 5% of radius.
/// Axes separated by more than this fraction of radius are considered skew.
pub const SSI_SKEW_FACTOR: f64 = 0.05;

/// Absolute tolerance for point-on-surface checks in SSI sampling: 0.05.
/// Currently unused — all sampling stubs have been replaced with NotSupported
/// per A15.2. Retained for potential use in future validation/testing.
#[allow(dead_code)]
pub const SSI_SAMPLE_ON_SURFACE_TOL: f64 = 0.05;

// ── Cone-cone SSI solver thresholds ──────────────────────────────────

/// Coefficient degeneracy threshold for same-apex cone-cone detection: 1e-6.
/// When quadratic coefficients (c_const, m_a, m_b) are all below this,
/// the solver treats the cones as sharing an apex.
pub const SSI_CONE_COEFF_DEGENERATE: f64 = 1e-6;

/// Perpendicular distance threshold for near-collinear cone axes: 0.01.
/// Axes separated by less than this are approximated as coaxial.
pub const SSI_CONE_COLLINEAR_DIST: f64 = 0.01;

/// Half-angle tangent difference threshold for same-direction coaxial cones: 0.01.
/// Cones with tangent difference below this have nearly-identical half-angles.
pub const SSI_CONE_TAN_DIFF: f64 = 0.01;

/// Height range margin for coaxial cone intersection checks: 0.1.
/// Allows intersection points slightly outside the strict z-range.
pub const SSI_CONE_Z_MARGIN: f64 = 0.1;

/// Maximum half-angle (radians) for near-cylindrical cone fallback: 0.09 (~5°).
/// Cones with half-angle below this are treated as nearly cylindrical
/// for the parallel-offset sampling path.
pub const SSI_CONE_NARROW_HALF_ANGLE: f64 = 0.09;

/// Angular clustering tolerance (radians) for theta grouping in cone-cone SSI: 0.1 (~5.7°).
/// Nearby intersection angles within this window are merged into a single cluster.
pub const SSI_THETA_CLUSTER_TOL: f64 = 0.1;

/// Bisection convergence tolerance for cone-cone SSI refinement: 1e-14.
/// Controls when the iterative bisection solver declares convergence.
/// Tighter than TAU_WORK because this operates in squared-distance space.
pub const SSI_BISECTION_CONV: f64 = 1e-14;

/// Surface error bound for near-miss cone-cone intersection: 1e-5.
/// Limits the parametric range to keep surface deviation below this threshold.
pub const SSI_SURFACE_ERROR_BOUND: f64 = 1e-5;

/// Minimum residual factor floor for near-miss cone-cone height limiting: 0.01.
/// Prevents division-by-near-zero when computing error-bounded height ranges.
pub const SSI_RESIDUAL_FACTOR_FLOOR: f64 = 0.01;

/// Near-parallel cone axis threshold: 1e-4 (~0.01 rad).
/// Wider than TAU_PARALLEL for approximate coaxial handling in cone-cone SSI.
/// Axes with |dot| > 1.0 − this are treated as near-parallel.
pub const SSI_CONE_NEAR_PARALLEL: f64 = 1e-4;

/// Angular half-width (radians) for theta range in near-miss cone-cone SSI: 0.001.
/// Restricts height-limited curves to a narrow angular band around the
/// intersection angle center.
pub const SSI_CONE_THETA_HALF_WIDTH: f64 = 0.001;

/// Validation tolerance for sampled SSI points on cone surfaces: 1e-5.
/// Checks that sampled points on intersection curves lie within this
/// distance of both cone surfaces (proportional to h × √r).
pub const SSI_CONE_VALIDATE_TOL: f64 = 1e-5;

// ── Mock kernel thresholds ──────────────────────────────────────────────

/// Cosine threshold for face-normal similarity in mock kernel face merging: 0.9.
/// Used in MockKernel boolean simulation to detect duplicate/opposite face pairs.
pub const COS_MOCK_FACE_SIMILARITY: f64 = 0.9;

// ── Analytical boolean dispatch threshold ──────────────────────────────

/// Curvature subdivision threshold for polygon-to-triangle decomposition: 5%.
/// When a face polygon's max vertex deviation from the first vertex exceeds
/// this fraction of face size, the polygon is subdivided into triangles.
/// Handles revolve lateral faces that are not planar.
pub const CURVATURE_SUBDIV_THRESHOLD: f64 = 0.05;

/// Relative radius tolerance for on-cylinder vertex classification: 2%.
/// Used in boolean post-processing to re-tag planar faces as cylindrical
/// when all vertices lie within this fraction of the expected radius.
pub const ON_CYLINDER_RADIUS_TOL: f64 = 0.02;

/// Normal z-component threshold for top/bottom cap face classification: 0.5.
/// A planar face with |normal.z| > this value is classified as a cap face
/// in boolean analytical dispatch. Corresponds to ~60° from horizontal.
pub const CAP_FACE_NORMAL_Z: f64 = 0.5;

/// Cosine threshold for "nearly parallel" cap normal in box-cylinder dispatch: 0.95.
/// A box cap normal must have dot product > this with the cylinder axis to be
/// considered aligned (within ~18°).
pub const COS_NEAR_PARALLEL_CAP: f64 = 0.95;

// ── Geometric basis construction ───────────────────────────────────────

/// Dot-product threshold for axis-hint selection in plane basis construction: 0.9.
/// When the reference axis has |dot| > this with the normal, an alternate axis is chosen.
pub const BASIS_AXIS_ALIGNMENT: f64 = 0.9;

// ── Hole-fill coherence thresholds ───────────────────────────────

/// Minimum boundary cycle length for normal-coherence skip: 8 edges.
/// Cycles shorter than this are always filled (likely S-H artifact gaps).
pub const HOLE_FILL_COHERENCE_MIN_EDGES: usize = 8;

/// Cosine threshold for coherent-normal boundary cycle detection: 0.999.
/// If ALL adjacent triangle normals dot the average normal above this,
/// the cycle is an intentional face opening (through-hole), not an artifact.
/// Set very strict because through-holes are on planar faces (normals ~identical),
/// while S-H gaps on curved surfaces have measurable normal variation.
pub const COS_HOLE_COHERENCE: f64 = 0.999;

/// Maximum out-of-plane distance as fraction of mean radius for through-hole
/// planarity detection: 0.1%.
/// Through-hole boundaries lie perfectly in-plane (float noise only).
/// Curved-surface gaps deviate proportionally to curvature × span.
pub const HOLE_PLANARITY_RATIO: f64 = 0.001;

/// Maximum coefficient of variation of vertex-to-centroid distances for
/// through-hole circularity detection: 5%.
/// Through-hole boundaries from cylinder subtract are nearly perfect circles.
/// Revolve caps and S-H gaps are irregular (wedge-shaped, non-circular).
pub const HOLE_CIRCULARITY_CV: f64 = 0.05;

// ── Boolean classification multipliers ───────────────────────────────

/// Inward-offset / coplanar-proximity multiplier applied to tau: 100×.
/// Used in face classification (classify.rs) and coplanar detection (clip.rs)
/// to probe whether a face centroid lies inside the opposing solid by offsetting
/// along the inward normal, and to classify near-coplanar face pairs.
pub const TAU_CLASSIFY_FACTOR: f64 = 100.0;

/// S-H divergence tolerance multiplier applied to tau: 1000×.
/// Sutherland-Hodgman clipping of adjacent faces produces independent
/// floating-point intersection points that can diverge by much more than tau.
/// Used in edge-split T-junction detection (clip.rs).
pub const TAU_SH_DIVERGENCE_FACTOR: f64 = 1000.0;

// ── Intersection cache quantization ──────────────────────────────────

/// Scale factor for normal vector quantization in IntersectionCache: 1e3.
/// Unit normals (components in [-1, 1]) are multiplied by this factor before
/// integer quantization, giving ~0.001 angular resolution in the cache key.
pub const CACHE_NORMAL_QUANTIZE_SCALE: f64 = 1e3;

// ── Tessellation vertex welding ──────────────────────────────────────

/// Cosine tolerance for normal similarity in vertex welding: 0.02.
/// When deduplicating co-located vertices, normals must differ by less than
/// this cosine distance to be merged. Preserves hard edges on curved surfaces.
pub const COS_NORMAL_SIMILARITY: f32 = 0.02;
