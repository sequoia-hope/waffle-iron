#[allow(unused_imports)]
use super::*;

// The Case-III graze / pinch guard (spec `yang_172_case_iii_graze_guard`)
// was RETIRED 2026-10-15 with its unit tests (cleanup step 2); this module
// keeps the cylinder fixture other tests share.

/// Axis-generic cylinder B-Rep (the `guard_cyl` shape with a free axis):
/// base rim at `base`, top rim at `base + h·axis`.
pub(crate) fn graze_cyl(base: [f64; 3], axis: [f64; 3], r: f64, h: f64) -> BRep {
    let ax = Vector3::new(axis[0], axis[1], axis[2]);
    let (e1, _) = ortho_basis(ax);
    let b = Point3::new(base[0], base[1], base[2]);
    let top = Point3::new(
        base[0] + h * axis[0],
        base[1] + h * axis[1],
        base[2] + h * axis[2],
    );
    let seam = |c: Point3| Point3::new(c.x() + r * e1.x(), c.y() + r * e1.y(), c.z() + r * e1.z());
    let verts = vec![
        BRepVertex { point: seam(b) },
        BRepVertex { point: seam(top) },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: b,
                normal: Vector3::new(-axis[0], -axis[1], -axis[2]),
                radius: r,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: top,
                normal: ax,
                radius: r,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: b,
                axis_dir: ax,
                radius: r,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(-axis[0], -axis[1], -axis[2]),
                d: -(b.x() * axis[0] + b.y() * axis[1] + b.z() * axis[2]),
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: ax,
                d: top.x() * axis[0] + top.y() * axis[1] + top.z() * axis[2],
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("graze guard cylinder")
}

// ── N75: the finite-extent Case-III depth (spec
// `yang_p0021_case_iii_finite_extent_depth`, P0021) ──────────────────

// ── Case V: the skew-axis near-tangent PINCH (P0034, 2026-10-11) ──────
