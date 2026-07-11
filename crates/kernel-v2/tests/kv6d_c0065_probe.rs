//! KV6d C0065 probe (`#[ignore]`, run with `-- --ignored --nocapture`):
//! replicate the corpus chain — closed torus (R=1.2, r=0.3, axis +z through
//! (0,0,0), tube plane z=0.5) minus the vertical square shaft at the +x
//! azimuth — and dump the output face census plus any inter-face triangle
//! penetrations (the assay `no_self_intersection` oracle's failure on
//! faces (0,2)).

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, Profile};

#[test]
#[ignore = "diagnostic probe, not an oracle"]
fn c0065_replica_face_census_and_penetrations() {
    let mut arena = BrepArena::new();
    // Sketch plane normal +y (u along +z, v along +x); circle center
    // world (−1.2, 0, 0.5), radius 0.3; axis +z through the origin.
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(1.0, 0.0, 0.0),
        Point2::new(0.5, -1.2),
        0.3,
    )
    .expect("circle profile");
    let r = revolve(
        &mut arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0 * std::f64::consts::PI,
    )
    .expect("closed torus");

    // Shaft: x∈[0.95,1.45], y∈[−0.25,0.25], z∈[−1,2].
    let shaft_profile = Profile::new(
        Point3::new(0.0, 0.0, -1.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.95, -0.25),
            Point2::new(1.45, -0.25),
            Point2::new(1.45, 0.25),
            Point2::new(0.95, 0.25),
        ],
        vec![],
    )
    .expect("shaft profile");
    let shaft =
        extrude(&mut arena, &shaft_profile, Vector3::new(0.0, 0.0, 3.0), 3.0).expect("shaft box");

    // Yang-level view: run the same boolean at the yang boundary and dump
    // the output faces (which stage births the spurious lens lump?).
    {
        let ya = kernel_v2::to_yang_brep(&arena, r.solid).expect("torus → yang");
        let yb = kernel_v2::to_yang_brep(&arena, shaft.solid).expect("shaft → yang");
        let backend = yang_rs::native_backend().expect("native backend");
        let yout = match yang_rs::boolean(&ya, &yb, cad_primitives::BoolOp::Subtract, &backend) {
            Ok(y) => y,
            Err(e) => {
                println!("yang subtract stops typed (expected since the bounded-face guard): {e}");
                return;
            }
        };
        println!("yang output: {} faces", yout.faces().len());
        for (k, f) in yout.faces().iter().enumerate() {
            let surf = match &f.surface {
                yang_rs::Surface::Plane { normal, d } => format!(
                    "Plane(n=({:.2},{:.2},{:.2}) d={d:.3})",
                    normal.x(),
                    normal.y(),
                    normal.z()
                ),
                yang_rs::Surface::Torus { .. } => "Torus".to_string(),
                other => format!("{other:?}"),
            };
            let ext = |edges: &[u32]| -> (f64, f64) {
                let mut ymin = f64::MAX;
                let mut ymax = f64::MIN;
                for &e in edges {
                    let ed = &yout.edges()[e as usize];
                    for v in [ed.start, ed.end] {
                        let p = yout.vertices()[v as usize].point;
                        ymin = ymin.min(p.y());
                        ymax = ymax.max(p.y());
                    }
                }
                (ymin, ymax)
            };
            let (ymin, ymax) = ext(&f.outer_loop);
            println!(
                "  y[{k}] {surf} rev={} outer={} inners={} outer-vtx-y∈[{ymin:.3},{ymax:.3}]",
                f.reversed,
                f.outer_loop.len(),
                f.inner_loops.len()
            );
        }
    }

    let out = boolean_op(&mut arena, r.solid, shaft.solid, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("subtract failed: {e:?}"));
    let report = validate_solid(&arena, out).expect("output validates");
    println!(
        "output: V={} E={} F={} R={} shells={} genus={} chi={}",
        report.vertices,
        report.edges,
        report.faces,
        report.rings,
        report.shells,
        report.genus,
        report.euler_lhs
    );

    let mesh = tessellate(&arena, out).expect("output tessellates");
    println!("faces in mesh order:");
    for (k, fr) in mesh.face_ranges.iter().enumerate() {
        let face = arena.face(fr.face).expect("face");
        let hes = arena.loop_half_edges(face.outer_loop).expect("loop").len();
        println!(
            "  [{k}] {:?} tris={} outer_hes={} inners={} surface={:?}",
            fr.face,
            fr.count / 3,
            hes,
            face.inner_loops.len(),
            face.surface.as_ref().map(|s| match s {
                kernel_v2::Surface::Plane(p) => format!(
                    "Plane(n=({:.2},{:.2},{:.2}) p=({:.3},{:.3},{:.3}))",
                    p.normal.x,
                    p.normal.y,
                    p.normal.z,
                    p.point.x(),
                    p.point.y(),
                    p.point.z()
                ),
                kernel_v2::Surface::Cylinder { radius, .. } => format!("Cylinder(r={radius})"),
                kernel_v2::Surface::Cone { half_angle, .. } => format!("Cone(ha={half_angle})"),
                kernel_v2::Surface::Torus {
                    major_radius,
                    minor_radius,
                    ..
                } => format!("Torus(R={major_radius}, r={minor_radius})"),
                other => format!("{other:?}"),
            })
        );
    }

    // Inter-face penetration scan (same shape as the assay oracle, without
    // the grazing threshold subtleties — report segment-level overlaps).
    let pos = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let q = |x: f64| (x / 1e-9).round() as i64;
    let quant = |i: u32| {
        let p = pos(i);
        (q(p[0]), q(p[1]), q(p[2]))
    };
    let face_tris: Vec<Vec<[u32; 3]>> = mesh
        .face_ranges
        .iter()
        .map(|fr| {
            mesh.indices[fr.start as usize..(fr.start + fr.count) as usize]
                .chunks_exact(3)
                .map(|t| [t[0], t[1], t[2]])
                .collect()
        })
        .collect();
    let mut reported = 0;
    for i in 0..face_tris.len() {
        for j in (i + 1)..face_tris.len() {
            for ta in &face_tris[i] {
                for tb in &face_tris[j] {
                    let qa = [quant(ta[0]), quant(ta[1]), quant(ta[2])];
                    let qb = [quant(tb[0]), quant(tb[1]), quant(tb[2])];
                    if qa.iter().filter(|v| qb.contains(v)).count() >= 1 {
                        continue;
                    }
                    let pa = [pos(ta[0]), pos(ta[1]), pos(ta[2])];
                    let pb = [pos(tb[0]), pos(tb[1]), pos(tb[2])];
                    if tri_tri_overlap(&pa, &pb, 1.5e-4) && reported < 8 {
                        println!("PENETRATION faces=({i},{j})\n  a={pa:?}\n  b={pb:?}");
                        reported += 1;
                    }
                }
            }
        }
    }
    println!("penetrations reported: {reported}");
}

/// Control: the same shaft against a 350° PARTIAL torus (the pre-existing
/// KV6d-5a path — caps at the −x seam azimuth, far from the +x shaft). If
/// the spurious lens lump appears here too, the wall pre-dates the closed
/// torus and is a torus-boolean classification gap, not a closed-torus
/// regression.
#[test]
#[ignore = "diagnostic probe, not an oracle"]
fn c0065_partial_torus_control() {
    let mut arena = BrepArena::new();
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(1.0, 0.0, 0.0),
        Point2::new(0.5, -1.2),
        0.3,
    )
    .expect("circle profile");
    let r = revolve(
        &mut arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        350.0_f64.to_radians(),
    )
    .expect("partial torus");
    let shaft_profile = Profile::new(
        Point3::new(0.0, 0.0, -1.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.95, -0.25),
            Point2::new(1.45, -0.25),
            Point2::new(1.45, 0.25),
            Point2::new(0.95, 0.25),
        ],
        vec![],
    )
    .expect("shaft profile");
    let shaft =
        extrude(&mut arena, &shaft_profile, Vector3::new(0.0, 0.0, 3.0), 3.0).expect("shaft box");
    match boolean_op(&mut arena, r.solid, shaft.solid, BoolOp::Subtract) {
        Ok(out) => {
            let report = validate_solid(&arena, out).expect("output validates");
            println!(
                "partial control: V={} E={} F={} shells={} genus={} chi={}",
                report.vertices,
                report.edges,
                report.faces,
                report.shells,
                report.genus,
                report.euler_lhs
            );
            for (k, fr) in tessellate(&arena, out)
                .expect("tessellates")
                .face_ranges
                .iter()
                .enumerate()
            {
                let face = arena.face(fr.face).expect("face");
                println!(
                    "  [{k}] {:?} tris={} surface={:?}",
                    fr.face,
                    fr.count / 3,
                    face.surface.as_ref().map(|s| match s {
                        kernel_v2::Surface::Plane(p) => format!(
                            "Plane(n=({:.2},{:.2},{:.2}))",
                            p.normal.x, p.normal.y, p.normal.z
                        ),
                        kernel_v2::Surface::Torus { .. } => "Torus".to_string(),
                        other => format!("{other:?}"),
                    })
                );
            }
        }
        Err(e) => println!("partial control boolean FAILED: {e:?}"),
    }
}

/// Coarse tri-tri penetration test: do the triangles' planes split each
/// other's vertices beyond `thr`, with overlapping AABBs (sufficient for
/// probe triage; the assay oracle has the exact segment test).
fn tri_tri_overlap(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3], thr: f64) -> bool {
    for d in 0..3 {
        let (amin, amax) = a.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p[d]), hi.max(p[d]))
        });
        let (bmin, bmax) = b.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p[d]), hi.max(p[d]))
        });
        if amax < bmin - thr || bmax < amin - thr {
            return false;
        }
    }
    let n = |t: &[[f64; 3]; 3]| {
        let e1 = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
        let e2 = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
        let c = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt().max(1e-300);
        [c[0] / l, c[1] / l, c[2] / l]
    };
    let side = |t: &[[f64; 3]; 3], p: &[f64; 3], nn: &[f64; 3]| {
        (p[0] - t[0][0]) * nn[0] + (p[1] - t[0][1]) * nn[1] + (p[2] - t[0][2]) * nn[2]
    };
    let na = n(a);
    let nb = n(b);
    let sb: Vec<f64> = b.iter().map(|p| side(a, p, &na)).collect();
    let sa: Vec<f64> = a.iter().map(|p| side(b, p, &nb)).collect();
    let split = |s: &[f64]| s.iter().any(|&x| x > thr) && s.iter().any(|&x| x < -thr);
    split(&sb) && split(&sa)
}
