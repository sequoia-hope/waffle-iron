//! Indirect geometric predicates for mesh arrangement.
//! Ref [#9] Cherchi et al. 2020, Sections 4.1-4.3.
//!
//! Intersection points are represented implicitly as unevaluated combinations
//! of input vertices. Predicates operate on these implicit representations
//! using multi-stage filtering (float → expansion) to guarantee exact results
//! without materializing coordinates.

/// Projection axis for 2D orientation tests.
/// Drop one coordinate to project 3D points onto a 2D plane.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ProjectionAxis {
    XY,
    YZ,
    ZX,
}

/// Coordinate axis for point comparison.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Axis {
    X,
    Y,
    Z,
}

/// Implicit point types per Cherchi 2020 Section 4.1.
///
/// - **Explicit**: Input vertex with known f64 coordinates.
/// - **LPI**: Line-Plane Intersection — edge (q1,q2) intersects plane (r,s,t).
///   Defined by 5 explicit points. Undefined when d_L = 0 (edge parallel to plane).
/// - **TPI**: Three-Plane Intersection — 3 non-coplanar triangles meet at a point.
///   Defined by 9 explicit points. Undefined when d_T = 0 (planes not independent).
#[derive(Debug, Clone)]
pub(crate) enum ImplicitPoint {
    /// Known explicit coordinates.
    Explicit([f64; 3]),

    /// Line-Plane Intersection: edge (q1→q2) crosses plane of triangle (r, s, t).
    LPI {
        q1: [f64; 3],
        q2: [f64; 3],
        r: [f64; 3],
        s: [f64; 3],
        t: [f64; 3],
    },

    /// Three-Plane Intersection: planes of triangles (v1,v2,v3), (w1,w2,w3), (u1,u2,u3).
    TPI {
        v1: [f64; 3],
        v2: [f64; 3],
        v3: [f64; 3],
        w1: [f64; 3],
        w2: [f64; 3],
        w3: [f64; 3],
        u1: [f64; 3],
        u2: [f64; 3],
        u3: [f64; 3],
    },
}

impl ImplicitPoint {
    /// Compute explicit coordinates for this implicit point.
    ///
    /// Returns `None` if the point is undefined (d_L = 0 for LPI, d_T = 0 for TPI).
    pub(crate) fn materialize(&self) -> Option<[f64; 3]> {
        match self {
            ImplicitPoint::Explicit(coords) => Some(*coords),
            ImplicitPoint::LPI { q1, q2, r, s, t } => materialize_lpi(q1, q2, r, s, t),
            ImplicitPoint::TPI {
                v1,
                v2,
                v3,
                w1,
                w2,
                w3,
                u1,
                u2,
                u3,
            } => materialize_tpi(v1, v2, v3, w1, w2, w3, u1, u2, u3),
        }
    }

    /// Check if this implicit point is well-defined.
    ///
    /// - Explicit: always defined
    /// - LPI: defined when d_L ≠ 0 (edge not parallel to plane)
    /// - TPI: defined when d_T ≠ 0 (planes linearly independent)
    pub(crate) fn is_defined(&self) -> bool {
        match self {
            ImplicitPoint::Explicit(_) => true,
            ImplicitPoint::LPI { q1, q2, r, s, t } => {
                let d_l = det3x3_lpi(q1, q2, r, s, t);
                d_l.abs() > 0.0
            }
            ImplicitPoint::TPI {
                v1,
                v2,
                v3,
                w1,
                w2,
                w3,
                u1,
                u2,
                u3,
            } => {
                let d_t = det3x3_tpi(v1, v2, v3, w1, w2, w3, u1, u2, u3);
                d_t.abs() > 0.0
            }
        }
    }
}

// ── LPI coordinate formulas (Cherchi 2020 Section 4.1) ───────────────────

/// Compute d_L = det|(q1-q2), (s-r), (t-r)| for an LPI point.
fn det3x3_lpi(q1: &[f64; 3], q2: &[f64; 3], r: &[f64; 3], s: &[f64; 3], t: &[f64; 3]) -> f64 {
    let a = [q1[0] - q2[0], q1[1] - q2[1], q1[2] - q2[2]];
    let b = [s[0] - r[0], s[1] - r[1], s[2] - r[2]];
    let c = [t[0] - r[0], t[1] - r[1], t[2] - r[2]];
    det3x3(&a, &b, &c)
}

/// Materialize LPI: edge (q1,q2) intersects plane(r,s,t).
///
/// ```text
/// d_L = det|(q1-q2), (s-r), (t-r)|
/// n   = det|(q1-r),  (s-r), (t-r)|
/// λ_Lx = d_L·q1x + n·(q2x - q1x)
/// λ_Ly = d_L·q1y + n·(q2y - q1y)
/// λ_Lz = d_L·q1z + n·(q2z - q1z)
/// coords = (λ_Lx/d_L, λ_Ly/d_L, λ_Lz/d_L)
/// ```
fn materialize_lpi(
    q1: &[f64; 3],
    q2: &[f64; 3],
    r: &[f64; 3],
    s: &[f64; 3],
    t: &[f64; 3],
) -> Option<[f64; 3]> {
    let d_l = det3x3_lpi(q1, q2, r, s, t);
    if d_l == 0.0 {
        return None; // Edge parallel to plane
    }

    let a_n = [q1[0] - r[0], q1[1] - r[1], q1[2] - r[2]];
    let b_n = [s[0] - r[0], s[1] - r[1], s[2] - r[2]];
    let c_n = [t[0] - r[0], t[1] - r[1], t[2] - r[2]];
    let n = det3x3(&a_n, &b_n, &c_n);

    let lambda_x = d_l * q1[0] + n * (q2[0] - q1[0]);
    let lambda_y = d_l * q1[1] + n * (q2[1] - q1[1]);
    let lambda_z = d_l * q1[2] + n * (q2[2] - q1[2]);

    Some([lambda_x / d_l, lambda_y / d_l, lambda_z / d_l])
}

// ── TPI coordinate formulas (Cherchi 2020 Section 4.1) ───────────────────

/// Compute d_T for a TPI point: determinant of the 3 triangle normals.
fn det3x3_tpi(
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
) -> f64 {
    let nv = cross_sub(v1, v2, v3);
    let nw = cross_sub(w1, w2, w3);
    let nu = cross_sub(u1, u2, u3);
    det3x3(&nv, &nw, &nu)
}

/// Materialize TPI: intersection of three planes.
///
/// ```text
/// nv = (v2-v1) × (v3-v2),  nw = (w2-w1) × (w3-w2),  nu = (u2-u1) × (u3-u2)
/// d_T = det|nv, nw, nu|
/// pv = nv · v1,  pw = nw · w1,  pu = nu · u1
/// λ_Tx = det|pv nvy nvz; pw nwy nwz; pu nuy nuz|
/// λ_Ty = det|nvx pv nvz; nwx pw nwz; nux pu nuz|
/// λ_Tz = det|nvx nvy pv; nwx nwy pw; nux nuy pu|
/// coords = (λ_Tx/d_T, λ_Ty/d_T, λ_Tz/d_T)
/// ```
fn materialize_tpi(
    v1: &[f64; 3],
    v2: &[f64; 3],
    v3: &[f64; 3],
    w1: &[f64; 3],
    w2: &[f64; 3],
    w3: &[f64; 3],
    u1: &[f64; 3],
    u2: &[f64; 3],
    u3: &[f64; 3],
) -> Option<[f64; 3]> {
    let nv = cross_sub(v1, v2, v3);
    let nw = cross_sub(w1, w2, w3);
    let nu = cross_sub(u1, u2, u3);

    let d_t = det3x3(&nv, &nw, &nu);
    if d_t == 0.0 {
        return None; // Planes not linearly independent
    }

    let pv = dot(&nv, v1);
    let pw = dot(&nw, w1);
    let pu = dot(&nu, u1);

    // Cramer's rule columns
    let lambda_x = det3x3(
        &[pv, nv[1], nv[2]],
        &[pw, nw[1], nw[2]],
        &[pu, nu[1], nu[2]],
    );
    let lambda_y = det3x3(
        &[nv[0], pv, nv[2]],
        &[nw[0], pw, nw[2]],
        &[nu[0], pu, nu[2]],
    );
    let lambda_z = det3x3(
        &[nv[0], nv[1], pv],
        &[nw[0], nw[1], pw],
        &[nu[0], nu[1], pu],
    );

    Some([lambda_x / d_t, lambda_y / d_t, lambda_z / d_t])
}

// ── Linear algebra helpers ───────────────────────────────────────────────

/// 3×3 determinant: det|row0, row1, row2| (rows are 3-element slices).
fn det3x3(r0: &[f64; 3], r1: &[f64; 3], r2: &[f64; 3]) -> f64 {
    r0[0] * (r1[1] * r2[2] - r1[2] * r2[1]) - r0[1] * (r1[0] * r2[2] - r1[2] * r2[0])
        + r0[2] * (r1[0] * r2[1] - r1[1] * r2[0])
}

/// Cross product of (b-a) × (c-b): triangle normal from vertices a, b, c.
fn cross_sub(a: &[f64; 3], b: &[f64; 3], c: &[f64; 3]) -> [f64; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let bc = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
    [
        ab[1] * bc[2] - ab[2] * bc[1],
        ab[2] * bc[0] - ab[0] * bc[2],
        ab[0] * bc[1] - ab[1] * bc[0],
    ]
}

/// Dot product of two 3-vectors.
fn dot(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// ── Orient2d: dispatch + implementation ──────────────────────────────────

/// Orient2d for implicit points projected onto a coordinate plane.
///
/// Returns the sign of the orientation determinant:
/// - Positive (+1.0) → counter-clockwise (CCW)
/// - Negative (-1.0) → clockwise (CW)
/// - Zero (0.0) → collinear
///
/// Phase 1 approach: materialize all implicit points and delegate to
/// `geometry_predicates::orient2d` (Shewchuk exact predicates). This is
/// correct but less efficient than true indirect predicates — the
/// materialization step can lose bits for deeply nested T-type points.
/// For L-type points the precision loss is negligible.
pub(crate) fn orient2d_indirect(
    a: &ImplicitPoint,
    b: &ImplicitPoint,
    c: &ImplicitPoint,
    proj: ProjectionAxis,
) -> f64 {
    let a_coords = match a.materialize() {
        Some(c) => c,
        None => return 0.0, // Undefined point → degenerate
    };
    let b_coords = match b.materialize() {
        Some(c) => c,
        None => return 0.0,
    };
    let c_coords = match c.materialize() {
        Some(c) => c,
        None => return 0.0,
    };

    // Project to 2D by selecting two coordinate axes
    let (i, j) = match proj {
        ProjectionAxis::XY => (0, 1),
        ProjectionAxis::YZ => (1, 2),
        ProjectionAxis::ZX => (2, 0),
    };

    // geometry_predicates::orient2d uses Shewchuk expansion arithmetic —
    // exact for the materialized f64 coordinates.
    geometry_predicates::orient2d(
        [a_coords[i], a_coords[j]],
        [b_coords[i], b_coords[j]],
        [c_coords[i], c_coords[j]],
    )
}

// ── Orient3d: dispatch + implementation ──────────────────────────────────

/// Orient3d for implicit points.
///
/// Returns the sign of the orientation determinant for 4 points in 3D:
/// - Positive → d is below the plane of (a,b,c)
/// - Negative → d is above
/// - Zero → coplanar
#[allow(dead_code)] // Will be used in Phase 2 cell extraction
pub(crate) fn orient3d_indirect(
    a: &ImplicitPoint,
    b: &ImplicitPoint,
    c: &ImplicitPoint,
    d: &ImplicitPoint,
) -> f64 {
    let a_c = match a.materialize() {
        Some(c) => c,
        None => return 0.0,
    };
    let b_c = match b.materialize() {
        Some(c) => c,
        None => return 0.0,
    };
    let c_c = match c.materialize() {
        Some(c) => c,
        None => return 0.0,
    };
    let d_c = match d.materialize() {
        Some(c) => c,
        None => return 0.0,
    };

    geometry_predicates::orient3d(a_c, b_c, c_c, d_c)
}

// ── Point comparison ─────────────────────────────────────────────────────

/// Compare two implicit points along a single coordinate axis.
///
/// Returns `Ordering` for lexicographic sorting of intersection points
/// along edges (Cherchi 2020 Section 4.3).
///
/// Phase 1 approach: materialize and compare f64 values. For well-separated
/// points this is exact. For coincident points at the limit of f64 precision,
/// the true indirect predicate (Phase 1b) will avoid the materialization
/// division and be more robust.
pub(crate) fn point_compare_on_axis(
    a: &ImplicitPoint,
    b: &ImplicitPoint,
    axis: Axis,
) -> std::cmp::Ordering {
    let a_coords = match a.materialize() {
        Some(c) => c,
        None => return std::cmp::Ordering::Equal,
    };
    let b_coords = match b.materialize() {
        Some(c) => c,
        None => return std::cmp::Ordering::Equal,
    };

    let idx = match axis {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
    };

    // Use total_cmp for deterministic ordering (NaN-safe)
    a_coords[idx].total_cmp(&b_coords[idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: orient2d with 3 explicit points must match Shewchuk orient2d.
    ///
    /// Triangle (0,0,0)→(1,0,0)→(0,1,0) is CCW on XY plane → positive sign.
    /// Swapping two points flips orientation → negative sign.
    /// Stub returns 0 for both → FAILS.
    #[test]
    fn test_orient2d_eee_matches_shewchuk() {
        let a = ImplicitPoint::Explicit([0.0, 0.0, 0.0]);
        let b = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let c = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);

        // CCW triangle on XY → positive
        let ccw = orient2d_indirect(&a, &b, &c, ProjectionAxis::XY);
        assert!(
            ccw > 0.0,
            "EEE orient2d of CCW triangle should be positive, got {ccw}"
        );

        // Swap b and c → CW → negative
        let cw = orient2d_indirect(&a, &c, &b, ProjectionAxis::XY);
        assert!(
            cw < 0.0,
            "EEE orient2d of CW triangle should be negative, got {cw}"
        );
    }

    /// Test 2: orient2d with one LPI point and two explicit points.
    ///
    /// Edge from (0,0,-1) to (0,0,1) crosses the plane of triangle
    /// (1,0,0),(0,1,0),(-1,0,0) at point (0,0,0). On XY projection,
    /// the LPI (0,0) with explicit points (1,0) and (0,1) forms a CCW
    /// triangle → positive sign. Stub returns 0 → FAILS.
    #[test]
    fn test_orient2d_lee_basic() {
        // LPI: edge along Z-axis crosses the XY-plane triangle
        let lpi = ImplicitPoint::LPI {
            q1: [0.0, 0.0, -1.0],
            q2: [0.0, 0.0, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, 0.0, 0.0],
        };
        // The LPI materializes at (0,0,0).
        // On XY: orient2d((0,0), (1,0), (0,1)) is CCW → positive.
        let e1 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([0.0, 1.0, 0.0]);

        let result = orient2d_indirect(&lpi, &e1, &e2, ProjectionAxis::XY);
        assert!(
            result > 0.0,
            "LEE orient2d should be positive (CCW), got {result}"
        );
    }

    /// Test 3: point_compare_on_axis must sort LPI points correctly.
    ///
    /// Three LPI points on the X-axis at x=2, x=5, x=8 (edge from origin
    /// to (10,0,0), intersecting planes at different X positions). Sorting
    /// by Axis::X should produce order [x2, x5, x8]. Stub returns Equal
    /// for all comparisons → sort is unstable → FAILS.
    #[test]
    fn test_point_compare_sorts_correctly() {
        // Edge along X-axis from (0,0,0) to (10,0,0).
        // Three planes perpendicular to X at x=2, x=5, x=8.
        // Each plane is defined by a triangle in the YZ plane offset along X.

        // Plane at x=2: triangle at (2,1,0),(2,0,1),(2,-1,0)
        let lpi_x2 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, 0.0],
            q2: [10.0, 0.0, 0.0],
            r: [2.0, 1.0, 0.0],
            s: [2.0, 0.0, 1.0],
            t: [2.0, -1.0, 0.0],
        };

        // Plane at x=5: triangle at (5,1,0),(5,0,1),(5,-1,0)
        let lpi_x5 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, 0.0],
            q2: [10.0, 0.0, 0.0],
            r: [5.0, 1.0, 0.0],
            s: [5.0, 0.0, 1.0],
            t: [5.0, -1.0, 0.0],
        };

        // Plane at x=8: triangle at (8,1,0),(8,0,1),(8,-1,0)
        let lpi_x8 = ImplicitPoint::LPI {
            q1: [0.0, 0.0, 0.0],
            q2: [10.0, 0.0, 0.0],
            r: [8.0, 1.0, 0.0],
            s: [8.0, 0.0, 1.0],
            t: [8.0, -1.0, 0.0],
        };

        // Sort the three points using point_compare_on_axis
        let mut points = vec![lpi_x8.clone(), lpi_x2.clone(), lpi_x5.clone()];
        points.sort_by(|a, b| point_compare_on_axis(a, b, Axis::X));

        // After sorting, order should be x2 < x5 < x8.
        // Verify by checking pairwise comparisons.
        assert_eq!(
            point_compare_on_axis(&lpi_x2, &lpi_x5, Axis::X),
            std::cmp::Ordering::Less,
            "x=2 should be less than x=5"
        );
        assert_eq!(
            point_compare_on_axis(&lpi_x5, &lpi_x8, Axis::X),
            std::cmp::Ordering::Less,
            "x=5 should be less than x=8"
        );
        assert_eq!(
            point_compare_on_axis(&lpi_x2, &lpi_x8, Axis::X),
            std::cmp::Ordering::Less,
            "x=2 should be less than x=8"
        );
    }

    /// Test 4: orient2d with near-collinear LPI point must use expansion fallback.
    ///
    /// An LPI point is constructed to land extremely close to the line between
    /// two explicit points (within ~1e-15). The float filter should fail
    /// (result within epsilon), requiring the expansion arithmetic fallback
    /// to determine the correct sign. Stub returns 0 → FAILS.
    #[test]
    fn test_lpi_orient2d_degenerate() {
        // Construct an LPI that is *almost* collinear with two explicit points.
        // Line from (0,0) to (1,0) on XY. The LPI should be at approximately
        // (0.5, 1e-15, 0) — barely above the line.
        //
        // Edge from (0.5, 1e-15, -1) to (0.5, 1e-15, 1) crosses the XY plane
        // at (0.5, 1e-15, 0).
        let lpi_near_line = ImplicitPoint::LPI {
            q1: [0.5, 1e-15, -1.0],
            q2: [0.5, 1e-15, 1.0],
            r: [1.0, 0.0, 0.0],
            s: [0.0, 1.0, 0.0],
            t: [-1.0, 0.0, 0.0],
        };

        let e1 = ImplicitPoint::Explicit([0.0, 0.0, 0.0]);
        let e2 = ImplicitPoint::Explicit([1.0, 0.0, 0.0]);

        // The LPI at (0.5, ~1e-15) is barely above the line from (0,0) to (1,0).
        // orient2d((0.5, 1e-15), (0,0), (1,0)) should be negative (CW)
        // because the point is above the left-to-right line.
        // The float filter will likely fail due to near-collinearity,
        // but expansion arithmetic must give the correct sign.
        let result = orient2d_indirect(&lpi_near_line, &e1, &e2, ProjectionAxis::XY);
        assert!(
            result != 0.0,
            "Near-collinear LPI orient2d must not return 0 (collinear); \
             expansion fallback should determine the correct sign, got {result}"
        );
    }
}
