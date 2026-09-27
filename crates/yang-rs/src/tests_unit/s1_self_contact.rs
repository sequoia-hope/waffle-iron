//! Stage-1 operand self-contact guard (spec `yang_stage1_self_contact_guard`,
//! 2026-09-07 — R0032 op 3): an operand whose chord band reaches ANOTHER
//! face of the same solid (Yang §4.1.1 broken; §4.2.1 Case IV inside one
//! operand) is refined on the face that caused the contact — a torus patch
//! face by its own chord bound, a rim-sampled face by the shared rim N —
//! until the arrangement's exact tri–tri classification finds nothing, or
//! becomes the typed loud stop. Two planar faces in contact have no chord
//! band to refine: a genuine B-Rep self-intersection, loud at once.

use super::*;
use std::collections::BTreeMap;

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn once(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
) -> (Stage1Tess, Option<usize>) {
    let empty: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    let no_demands: BTreeMap<u32, f64> = BTreeMap::new();
    let mut n_used = None;
    let (tess, _) = stage1_tessellate_once(
        verts,
        edges,
        faces,
        &empty,
        &empty,
        &empty,
        &crate::stage1_tessellate::FaceConstraints::new(),
        &no_demands,
        None,
        &mut n_used,
    )
    .expect("one Stage-1 pass");
    (tess, n_used)
}

fn sag(r: f64, n: usize) -> f64 {
    r * (1.0 - (std::f64::consts::PI / n as f64).cos())
}

/// A lone planar quad face (a pocket wall) appended to `verts`/`edges`/
/// `faces`: `corners` in order, `normal` its plane normal.
fn push_quad(
    verts: &mut Vec<BRepVertex>,
    edges: &mut Vec<BRepEdge>,
    faces: &mut Vec<BRepFace>,
    corners: [Point3; 4],
    normal: Vector3,
) {
    let v0 = verts.len() as u32;
    for c in corners {
        verts.push(BRepVertex { point: c });
    }
    let e0 = edges.len() as u32;
    for i in 0..4u32 {
        edges.push(BRepEdge {
            start: v0 + i,
            end: v0 + (i + 1) % 4,
            curve: Curve::LineSegment,
        });
    }
    let n = normal.as_array();
    let c = corners[0].as_array();
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal,
            d: -(n[0] * c[0] + n[1] * c[1] + n[2] * c[2]),
        },
        outer_loop: (e0..e0 + 4).collect(),
        inner_loops: vec![],
        reversed: false,
    });
}

// ───────────────────────────────────────────────────────────────────
// Primitives
// ───────────────────────────────────────────────────────────────────

#[test]
fn halving_the_sagitta_derives_the_next_rim_n() {
    // r = 10 at N = 14: sag 0.2506 → target 0.1253 → N = 20 (sag 0.1231;
    // N = 19 still 0.1364).
    assert_eq!(rim_n_halving_sagitta(10.0, 14), Some(20));
    assert!(sag(10.0, 20) <= sag(10.0, 14) / 2.0 && sag(10.0, 19) > sag(10.0, 14) / 2.0);
    assert_eq!(rim_n_halving_sagitta(0.0, 14), None);
    assert_eq!(rim_n_halving_sagitta(10.0, 2), None);
}

#[test]
fn contact_point_lies_on_the_crossing_segment_and_is_none_for_disjoint_pairs() {
    // A horizontal triangle in z = 0 pierced by a vertical one along x = 0.5.
    let a = [pt(0.0, 0.0, 0.0), pt(2.0, 0.0, 0.0), pt(0.0, 2.0, 0.0)];
    let b = [pt(0.5, 0.2, -1.0), pt(0.5, 0.2, 1.0), pt(0.5, 1.0, 1.0)];
    let p = contact_point(a, b).expect("crossing pair has a contact point");
    let q = p.as_array();
    assert!((q[0] - 0.5).abs() < 1e-12 && q[2].abs() < 1e-12, "{q:?}");
    assert!(q[1] > 0.2 - 1e-12 && q[1] < 1.0 + 1e-12, "{q:?}");
    let far = [pt(5.0, 5.0, 5.0), pt(6.0, 5.0, 5.0), pt(5.0, 6.0, 5.0)];
    assert_eq!(contact_point(a, far), None);
}

/// A re-entering output's cap and lateral meet along a rim sampled TWICE
/// (positional twins, no shared index) — the exact sweep reports every such
/// touch; the guard must see adjacency, not a contact. Likewise a
/// T-junction touch (a triangle standing on another's edge interior) and an
/// endpoint touch derive no contact point.
#[test]
fn twin_adjacent_and_touching_pairs_are_not_contacts() {
    let floor = [pt(0.0, 0.0, 0.0), pt(2.0, 0.0, 0.0), pt(0.0, 2.0, 0.0)];
    // Twin seam: the wall's foot duplicates the floor's edge positions
    // (one of them a few ulp off, as the pocket operand's rims are).
    let wall = [pt(0.0, 0.0, 0.0), pt(2.0, 1e-15, 0.0), pt(1.0, 0.0, 2.0)];
    // T-junction: a wall standing on the floor edge's interior.
    let tee = [pt(0.5, 0.0, 0.0), pt(1.5, 0.0, 0.0), pt(1.0, 0.0, 2.0)];
    assert_eq!(
        contact_point(floor, tee),
        None,
        "an endpoint touch is not a crossing"
    );
    let verts: Vec<Point3> = floor
        .iter()
        .chain(wall.iter())
        .chain(tee.iter())
        .copied()
        .collect();
    let tess = Stage1Tess {
        verts,
        sources: vec![TessellationSource::Unknown; 9],
        tris: vec![[0, 1, 2], [3, 4, 5], [6, 7, 8]],
        face_tri_ranges: vec![0..1, 1..2, 2..3],
        chains: BTreeMap::new(),
    };
    let plane = |normal: Vector3, d: f64| BRepFace {
        surface: Surface::Plane { normal, d },
        outer_loop: vec![],
        inner_loops: vec![],
        reversed: false,
    };
    let faces = vec![
        plane(Vector3::new(0.0, 0.0, 1.0), 0.0),
        plane(Vector3::new(0.0, -1.0, 0.0), 0.0),
        plane(Vector3::new(0.0, -1.0, 0.0), 0.0),
    ];
    assert!(
        !cherchi_rs::detect_improper_contacts(&tess.verts, &tess.tris).is_clean(),
        "the exact sweep does report these touches"
    );
    assert_eq!(
        scan_self_contacts(&tess, &[], &faces, None, &BTreeMap::new()),
        None
    );
}

// ───────────────────────────────────────────────────────────────────
// Rim-N channel: a cylinder skin 0.1 thick over a planar pocket wall
// ───────────────────────────────────────────────────────────────────

/// `rt_cylinder(0, 4, 10)` plus a pocket wall in the plane `x = 10 − depth`
/// (|y| ≤ 1, 1 ≤ z ≤ 3): the lateral's chords at the rim N the chord bound
/// derives (N = 14, sag 0.25) dip through the wall.
fn cylinder_over_pocket(depth: f64) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let (mut verts, mut edges, mut faces) = boolean_functional::rt_cylinder(0.0, 4.0, 10.0);
    let x = 10.0 - depth;
    push_quad(
        &mut verts,
        &mut edges,
        &mut faces,
        [
            pt(x, -1.0, 1.0),
            pt(x, 1.0, 1.0),
            pt(x, 1.0, 3.0),
            pt(x, -1.0, 3.0),
        ],
        Vector3::new(1.0, 0.0, 0.0),
    );
    (verts, edges, faces)
}

#[test]
fn a_cylinder_skin_thinner_than_its_sagitta_reports_the_pocket_with_a_rim_demand() {
    let (verts, edges, faces) = cylinder_over_pocket(0.1);
    let (tess, n_used) = once(&verts, &edges, &faces);
    let n = n_used.expect("the pass chose a rim N");
    assert!(
        sag(10.0, n) > 0.1,
        "the fixture must be coarse: N={n} sag={}",
        sag(10.0, n)
    );
    let report = scan_self_contacts(&tess, &edges, &faces, n_used, &BTreeMap::new())
        .expect("the lateral pierces the pocket wall");
    assert!(report.pairs >= 1 && report.unresolved == 0, "{report:?}");
    assert_eq!(
        report.first,
        (0, 3),
        "cylinder lateral × pocket wall: {report:?}"
    );
    assert!(
        report.face_bounds.is_empty(),
        "no torus face here: {report:?}"
    );
    assert_eq!(
        report.demand_n,
        rim_n_halving_sagitta(10.0, n),
        "{report:?}"
    );
}

#[test]
fn the_driver_refines_the_cylinder_until_its_chords_clear_the_pocket() {
    let (verts, edges, faces) = cylinder_over_pocket(0.1);
    let (coarse, n0) = once(&verts, &edges, &faces);
    let tess = stage1_tessellate(&verts, &edges, &faces).expect("refined tessellation");
    let contacts = cherchi_rs::detect_improper_contacts(&tess.verts, &tess.tris);
    assert!(contacts.is_clean(), "{:?}", contacts.improper_pairs);
    // The lateral was re-sampled finer (each rim ring gained segments) and
    // its chords now sag less than the skin: N ≥ 29 clears 0.1 at r = 10.
    let lateral = |t: &Stage1Tess| t.face_tri_ranges[0].len();
    assert!(
        lateral(&tess) > lateral(&coarse),
        "{} vs {}",
        lateral(&tess),
        lateral(&coarse)
    );
    let n_final = lateral(&tess) / 2; // the strip is two triangles per rim segment
    assert!(
        n_final >= 29 && sag(10.0, n_final) < 0.1,
        "final N {n_final} (started at {n0:?})"
    );
    // Every lateral vertex still on the cylinder; the pocket wall untouched.
    let cyl = faces[0].surface;
    for &t in &tess.tris[tess.face_tri_ranges[0].clone()] {
        for &v in &t {
            let d = signed_distance_to_surface(cyl, tess.verts[v as usize]).unwrap();
            assert!(d.abs() < 1e-9, "lateral vertex {v} off the cylinder: {d:e}");
        }
    }
    assert_eq!(
        tess.face_tri_ranges[3].len(),
        coarse.face_tri_ranges[3].len()
    );
}

#[test]
fn a_clean_operand_is_byte_identical_through_the_guard() {
    let (verts, edges, faces) = boolean_functional::rt_cylinder(0.0, 4.0, 10.0);
    let (coarse, _) = once(&verts, &edges, &faces);
    let tess = stage1_tessellate(&verts, &edges, &faces).expect("cylinder");
    assert_eq!(tess.tris, coarse.tris);
    assert_eq!(tess.verts, coarse.verts);
}

// ───────────────────────────────────────────────────────────────────
// Torus PATCH channel: a torus skin over a planar fin (R0032's class)
// ───────────────────────────────────────────────────────────────────

const TR: f64 = 45.0;
const TR_MINOR: f64 = 30.0;

fn torus_eval(u: f64, v: f64) -> Point3 {
    // Axis z, centre at the origin: u poloidal (about the tube), v toroidal.
    let rad = TR + TR_MINOR * u.cos();
    pt(rad * v.cos(), rad * v.sin(), TR_MINOR * u.sin())
}

fn torus_surface() -> Surface {
    Surface::Torus {
        center: pt(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        major_radius: TR,
        minor_radius: TR_MINOR,
    }
}

/// A lone torus DISK patch (R0032 face 593's class — a chord polygon with no
/// analytic rim): the (u, v) rectangle |u| ≤ 0.3, |v| ≤ 0.3 on the outer
/// equator sampled 8 chords per side, wound CCW about the outward normal.
fn torus_patch_face() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let (hu, hv, per_side) = (0.3_f64, 0.3_f64, 8usize);
    let mut uv: Vec<(f64, f64)> = Vec::new();
    for k in 0..per_side {
        let s = k as f64 / per_side as f64;
        uv.push((-hu, -hv + 2.0 * hv * s)); // bottom edge, v rising
    }
    for k in 0..per_side {
        let s = k as f64 / per_side as f64;
        uv.push((-hu + 2.0 * hu * s, hv)); // right side, u rising
    }
    for k in 0..per_side {
        let s = k as f64 / per_side as f64;
        uv.push((hu, hv - 2.0 * hv * s)); // top edge, v falling
    }
    for k in 0..per_side {
        let s = k as f64 / per_side as f64;
        uv.push((hu - 2.0 * hu * s, -hv)); // left side, u falling
    }
    let mut pts: Vec<Point3> = uv.iter().map(|&(u, v)| torus_eval(u, v)).collect();
    // Material-left = CCW about the outward normal (+x at (0, 0)): flip if
    // the loop's Newell normal opposes it.
    let mut nrm = [0.0f64; 3];
    for i in 0..pts.len() {
        let a = pts[i].as_array();
        let b = pts[(i + 1) % pts.len()].as_array();
        nrm[0] += (a[1] - b[1]) * (a[2] + b[2]);
        nrm[1] += (a[2] - b[2]) * (a[0] + b[0]);
        nrm[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    if nrm[0] < 0.0 {
        pts.reverse();
    }
    let n = pts.len() as u32;
    let verts: Vec<BRepVertex> = pts.into_iter().map(|point| BRepVertex { point }).collect();
    let edges: Vec<BRepEdge> = (0..n)
        .map(|i| BRepEdge {
            start: i,
            end: (i + 1) % n,
            curve: Curve::LineSegment,
        })
        .collect();
    let faces = vec![BRepFace {
        surface: torus_surface(),
        outer_loop: (0..n).collect(),
        inner_loops: vec![],
        reversed: false,
    }];
    (verts, edges, faces)
}

/// The deepest point of the patch mesh below the torus (over triangle
/// centroids and edge midpoints) and its depth.
fn deepest_point(tess: &Stage1Tess, range: std::ops::Range<usize>) -> (Point3, f64) {
    let surf = torus_surface();
    let mut best = (pt(0.0, 0.0, 0.0), 0.0f64);
    for t in &tess.tris[range] {
        let p: Vec<[f64; 3]> = t
            .iter()
            .map(|&v| tess.verts[v as usize].as_array())
            .collect();
        let mut cands = vec![[
            (p[0][0] + p[1][0] + p[2][0]) / 3.0,
            (p[0][1] + p[1][1] + p[2][1]) / 3.0,
            (p[0][2] + p[1][2] + p[2][2]) / 3.0,
        ]];
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            cands.push([
                (p[i][0] + p[j][0]) / 2.0,
                (p[i][1] + p[j][1]) / 2.0,
                (p[i][2] + p[j][2]) / 2.0,
            ]);
        }
        for c in cands {
            let q = pt(c[0], c[1], c[2]);
            let d = signed_distance_to_surface(surf, q).unwrap();
            if -d > best.1 {
                best = (q, -d);
            }
        }
    }
    best
}

/// The patch plus a small vertical FIN (a planar pocket wall standing
/// radially inside the tube) whose top edge passes through the coarse
/// mesh's deepest chord point `q`: the fin spans depths `d = depth/2` to
/// `d + 2` along the radial line through `q`, so `q` itself lies on the fin
/// and the triangle carrying `q` crosses it — a contact certain by
/// construction (the deepest point is never a vertex). The fin's 4-unit top
/// edge is a chord under the tube (the surface falls away ≤ 2²/(2·30) =
/// 0.067 over its half-length, below every `d` this fixture produces).
fn torus_over_fin() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>, f64) {
    let (mut verts, mut edges, mut faces) = torus_patch_face();
    let (coarse, _) = once(&verts, &edges, &faces);
    let (q, depth) = deepest_point(&coarse, coarse.face_tri_ranges[0].clone());
    assert!(
        depth > 0.2,
        "the patch mesh must be coarse: deepest {depth}"
    );
    let qa = q.as_array();
    // Closest tube centre, outward normal, a tangent frame at q's projection.
    let rho = (qa[0] * qa[0] + qa[1] * qa[1]).sqrt();
    let tube_c = [TR * qa[0] / rho, TR * qa[1] / rho, 0.0];
    let w = [qa[0] - tube_c[0], qa[1] - tube_c[1], qa[2] - tube_c[2]];
    let wl = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
    let nrm = [w[0] / wl, w[1] / wl, w[2] / wl];
    let s = [
        tube_c[0] + TR_MINOR * nrm[0],
        tube_c[1] + TR_MINOR * nrm[1],
        tube_c[2] + TR_MINOR * nrm[2],
    ];
    let (t1, t2) = ortho_basis(Vector3::new(nrm[0], nrm[1], nrm[2]));
    let (t1, t2) = (t1.as_array(), t2.as_array());
    let d = depth / 2.0;
    let at = |depth_here: f64, along: f64| {
        pt(
            s[0] - depth_here * nrm[0] + along * t2[0],
            s[1] - depth_here * nrm[1] + along * t2[1],
            s[2] - depth_here * nrm[2] + along * t2[2],
        )
    };
    let h = 2.0;
    push_quad(
        &mut verts,
        &mut edges,
        &mut faces,
        [at(d, -h), at(d, h), at(d + 2.0, h), at(d + 2.0, -h)],
        Vector3::new(t1[0], t1[1], t1[2]),
    );
    (verts, edges, faces, d)
}

#[test]
fn a_torus_skin_thinner_than_its_chord_sag_reports_the_fin_with_a_halved_bound() {
    let (verts, edges, faces, _) = torus_over_fin();
    let (tess, n_used) = once(&verts, &edges, &faces);
    let report = scan_self_contacts(&tess, &edges, &faces, n_used, &BTreeMap::new())
        .expect("the torus chords pierce the fin");
    assert!(report.pairs >= 1, "{report:?}");
    assert_eq!(report.first, (0, 1), "{report:?}");
    assert_eq!(
        report.demand_n, None,
        "a torus patch is not rim-sampled: {report:?}"
    );
    let own = torus_chord_bound(TR, TR_MINOR);
    assert_eq!(
        report.face_bounds.get(&0).copied(),
        Some(own / 2.0),
        "{report:?}"
    );
    // A second round halves the bound in force, not the surface's own.
    let mut cur = BTreeMap::new();
    cur.insert(0u32, own / 2.0);
    let again = scan_self_contacts(&tess, &edges, &faces, n_used, &cur).expect("same mesh");
    assert_eq!(again.face_bounds.get(&0).copied(), Some(own / 4.0));
}

#[test]
fn the_driver_refines_the_torus_patch_until_its_chords_clear_the_fin() {
    let (verts, edges, faces, d) = torus_over_fin();
    let (coarse, _) = once(&verts, &edges, &faces);
    let tess = stage1_tessellate(&verts, &edges, &faces).expect("refined tessellation");
    let contacts = cherchi_rs::detect_improper_contacts(&tess.verts, &tess.tris);
    assert!(contacts.is_clean(), "{:?}", contacts.improper_pairs);
    assert!(
        tess.face_tri_ranges[0].len() > coarse.face_tri_ranges[0].len(),
        "the torus patch was refined: {} vs {}",
        tess.face_tri_ranges[0].len(),
        coarse.face_tri_ranges[0].len()
    );
    // On-surface vertices, and the refined mesh dips less than the fin's top
    // edge depth `d` over the whole patch (the coarse mesh reached `2d`).
    let surf = torus_surface();
    for &t in &tess.tris[tess.face_tri_ranges[0].clone()] {
        for &v in &t {
            let d = signed_distance_to_surface(surf, tess.verts[v as usize]).unwrap();
            assert!(d.abs() < 1e-9, "torus vertex {v} off the tube: {d:e}");
        }
    }
    let (_, deepest) = deepest_point(&tess, tess.face_tri_ranges[0].clone());
    assert!(
        deepest < 2.0 * d,
        "deepest {deepest} vs the coarse mesh's {}",
        2.0 * d
    );
    // The lone patch is byte-identical through the guard (no pocket).
    let (pv, pe, pf) = torus_patch_face();
    let (alone_once, _) = once(&pv, &pe, &pf);
    let alone = stage1_tessellate(&pv, &pe, &pf).expect("patch alone");
    assert_eq!(alone.tris, alone_once.tris);
}

// ───────────────────────────────────────────────────────────────────
// No channel: two planar faces crossing are a genuine self-intersection
// ───────────────────────────────────────────────────────────────────

#[test]
fn two_planar_faces_crossing_are_the_loud_stop_at_once() {
    let mut verts = Vec::new();
    let mut edges = Vec::new();
    let mut faces = Vec::new();
    push_quad(
        &mut verts,
        &mut edges,
        &mut faces,
        [
            pt(-1.0, -1.0, 0.0),
            pt(1.0, -1.0, 0.0),
            pt(1.0, 1.0, 0.0),
            pt(-1.0, 1.0, 0.0),
        ],
        Vector3::new(0.0, 0.0, 1.0),
    );
    push_quad(
        &mut verts,
        &mut edges,
        &mut faces,
        [
            pt(0.0, -1.0, -1.0),
            pt(0.0, 1.0, -1.0),
            pt(0.0, 1.0, 1.0),
            pt(0.0, -1.0, 1.0),
        ],
        Vector3::new(1.0, 0.0, 0.0),
    );
    match stage1_tessellate(&verts, &edges, &faces) {
        Err(YangError::Stage1SelfContact {
            face_a: 0,
            face_b: 1,
            pairs,
            unresolved: 0,
            rounds: 0,
        }) => assert!(pairs >= 1),
        Err(e) => panic!("expected the typed self-contact stop, got {e:?}"),
        Ok(_) => panic!("expected the typed self-contact stop, got a tessellation"),
    }
}
