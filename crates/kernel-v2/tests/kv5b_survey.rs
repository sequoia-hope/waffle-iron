//! PR-KV5b survey tool (`#[ignore]`d; study artifact, not a gate): what does
//! yang-rs's NATIVE pipeline actually emit for cylinder×box booleans?
//! Surfaces, curve variants, full-circle vs arc edges, loop compositions,
//! and whether outputs can re-enter `BRep::new` (Stage 1).
//!
//! Findings (2026-06-11, drove the KV5b reassembly vocabulary — see
//! kv5b_curved_boolean.rs module docs for the summary):
//! - outputs carry `Plane` + `Cylinder` surfaces; `LineSegment` + `Circle`
//!   ARC (`start != end`) edges; NEVER full (`start == end`) circles;
//! - cylinder faces are partial patches (wrapping rim cycles + window
//!   cycles + chord polylines); original input rims come back faceted
//!   (untagged `LineSegment` chords at yang's Stage-1 facet count);
//! - cylinder×cylinder fails inside yang Stage 3
//!   (`SsiRefinementFailed`/AmbiguousCurve — degree-4 wall);
//! - outputs canNOT re-enter `BRep::new` ("cylinder lateral must have
//!   exactly 2 Circle rim edges") — chained curved booleans are a typed
//!   re-entry wall in kernel-v2.
//!
//! Run: cargo test -p kernel-v2 --test kv5b_survey -- --ignored --nocapture

use std::collections::BTreeMap;

use cad_primitives::{BoolOp, Point3, Vector3};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let v = |x: f64, y: f64, z: f64| BRepVertex { point: p(x, y, z) };
    let verts = vec![
        v(lo[0], lo[1], lo[2]),
        v(hi[0], lo[1], lo[2]),
        v(hi[0], hi[1], lo[2]),
        v(lo[0], hi[1], lo[2]),
        v(lo[0], lo[1], hi[2]),
        v(hi[0], lo[1], hi[2]),
        v(hi[0], hi[1], hi[2]),
        v(lo[0], hi[1], hi[2]),
    ];
    // 6 faces, 4 edges each, CCW from outside.
    let loops: [([usize; 4], [f64; 3]); 6] = [
        ([0, 3, 2, 1], [0.0, 0.0, -1.0]),
        ([4, 5, 6, 7], [0.0, 0.0, 1.0]),
        ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
        ([1, 2, 6, 5], [1.0, 0.0, 0.0]),
        ([2, 3, 7, 6], [0.0, 1.0, 0.0]),
        ([3, 0, 4, 7], [-1.0, 0.0, 0.0]),
    ];
    let mut edges = Vec::new();
    let mut faces = Vec::new();
    for (cyc, n) in loops {
        let base = edges.len() as u32;
        for k in 0..4 {
            edges.push(BRepEdge {
                start: cyc[k] as u32,
                end: cyc[(k + 1) % 4] as u32,
                curve: Curve::LineSegment,
            });
        }
        let p0 = verts[cyc[0]].point;
        let d = -(n[0] * p0.x() + n[1] * p0.y() + n[2] * p0.z());
        faces.push(BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(n[0], n[1], n[2]),
                d,
            },
            outer_loop: (base..base + 4).collect(),
            inner_loops: Vec::new(),
            reversed: false,
        });
    }
    BRep::new(verts, edges, faces).expect("box brep")
}

fn cylinder_brep(axis_point: [f64; 3], radius: f64, height: f64) -> BRep {
    // z-axis cylinder, seam at +x.
    let bc = axis_point;
    let tc = [bc[0], bc[1], bc[2] + height];
    let verts = vec![
        BRepVertex {
            point: p(bc[0] + radius, bc[1], bc[2]),
        },
        BRepVertex {
            point: p(tc[0] + radius, tc[1], tc[2]),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(bc[0], bc[1], bc[2]),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(tc[0], tc[1], tc[2]),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius,
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
                axis_point: p(bc[0], bc[1], bc[2]),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: bc[2],
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -tc[2],
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("cylinder brep")
}

fn survey(name: &str, r: &Result<BRep, yang_rs::YangError>) {
    println!("== {name} ==");
    match r {
        Err(e) => println!("  ERR: {e}"),
        Ok(out) => {
            let mut surf: BTreeMap<&str, usize> = BTreeMap::new();
            for f in out.faces() {
                let k = match f.surface {
                    Surface::Plane { .. } => "Plane",
                    Surface::Cylinder { .. } => "Cylinder",
                    Surface::Sphere { .. } => "Sphere",
                    Surface::Cone { .. } => "Cone",
                    Surface::Torus { .. } => "Torus",
                };
                *surf.entry(k).or_default() += 1;
            }
            let mut reversed = 0;
            for f in out.faces() {
                if f.reversed {
                    reversed += 1;
                }
            }
            let mut curves: BTreeMap<String, usize> = BTreeMap::new();
            for e in out.edges() {
                let k = match e.curve {
                    Curve::LineSegment => "LineSegment".to_string(),
                    Curve::Circle { .. } => {
                        if e.start == e.end {
                            "Circle(FULL start==end)".to_string()
                        } else {
                            "Circle(ARC)".to_string()
                        }
                    }
                    Curve::Ellipse { .. } => "Ellipse".to_string(),
                    Curve::Parabola { .. } => "Parabola".to_string(),
                    Curve::Hyperbola { .. } => "Hyperbola".to_string(),
                };
                *curves.entry(k).or_default() += 1;
            }
            // Loop sizes of curved faces + inner loop counts.
            let mut cyl_loops = Vec::new();
            let mut planar_with_inner = 0;
            for f in out.faces() {
                if matches!(f.surface, Surface::Cylinder { .. }) {
                    cyl_loops.push((f.outer_loop.len(), f.inner_loops.len(), f.reversed));
                } else if !f.inner_loops.is_empty() {
                    planar_with_inner += 1;
                }
            }
            println!(
                "  verts {} edges {} faces {} (reversed {reversed})",
                out.vertices().len(),
                out.edges().len(),
                out.faces().len()
            );
            println!("  surfaces: {surf:?}");
            println!("  curves:   {curves:?}");
            println!("  cylinder faces (outer_len, n_inner, reversed): {cyl_loops:?}");
            println!("  planar faces with inner loops: {planar_with_inner}");
            // Per-face loop curve composition for curved + holed faces.
            for (i, f) in out.faces().iter().enumerate() {
                let comp = |lp: &Vec<u32>| {
                    let mut m: BTreeMap<&str, usize> = BTreeMap::new();
                    for &ei in lp {
                        let k = match out.edges()[ei as usize].curve {
                            Curve::LineSegment => "L",
                            Curve::Circle { .. } => "C",
                            _ => "other",
                        };
                        *m.entry(k).or_default() += 1;
                    }
                    format!("{m:?}")
                };
                if matches!(f.surface, Surface::Cylinder { .. }) || !f.inner_loops.is_empty() {
                    println!(
                        "    face {i} {:?} rev={} outer {} inner {:?}",
                        match f.surface {
                            Surface::Plane { .. } => "Plane",
                            Surface::Cylinder { .. } => "Cyl",
                            _ => "?",
                        },
                        f.reversed,
                        comp(&f.outer_loop),
                        f.inner_loops.iter().map(comp).collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}

#[test]
#[ignore]
fn survey_cylinder_box_outputs() {
    let nb = yang_rs::native_backend().expect("native backend");

    // yr8 config: cylinder through unit box top+bottom? axis z, r=0.25,
    // z in [-0.5, 1.5]; box (0..1)^3 — cylinder pokes out both caps.
    let cyl = cylinder_brep([0.5, 0.5, -0.5], 0.25, 2.0);
    let bx = box_brep([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    survey(
        "union cyl∪box (yr8 cfg)",
        &boolean(&cyl, &bx, BoolOp::Union, &nb),
    );
    survey(
        "subtract box−cyl through-hole",
        &boolean(&bx, &cyl, BoolOp::Subtract, &nb),
    );
    survey(
        "intersect box∩cyl",
        &boolean(&bx, &cyl, BoolOp::Intersect, &nb),
    );

    // Blind pocket: cylinder from z=0.4 through top (z>1).
    let cyl_pocket = cylinder_brep([0.5, 0.5, 0.4], 0.25, 1.1);
    survey(
        "subtract box−cyl blind pocket",
        &boolean(&bx, &cyl_pocket, BoolOp::Subtract, &nb),
    );
    survey(
        "union cyl∪box (pocket cfg, cyl pokes top only)",
        &boolean(&cyl_pocket, &bx, BoolOp::Union, &nb),
    );

    // Side overlap: cylinder axis z, overlapping box laterally (axis outside box).
    let cyl_side = cylinder_brep([1.1, 0.5, -0.5], 0.25, 2.0);
    survey(
        "union cyl∪box side overlap",
        &boolean(&cyl_side, &bx, BoolOp::Union, &nb),
    );

    // cylinder × cylinder (degree-4 wall expected).
    let cyl2 = cylinder_brep([0.6, 0.5, -0.7], 0.3, 2.5);
    survey(
        "union cyl∪cyl offset",
        &boolean(&cyl, &cyl2, BoolOp::Union, &nb),
    );
    survey(
        "subtract cyl−cyl",
        &boolean(&cyl, &cyl2, BoolOp::Subtract, &nb),
    );

    // Disjoint union (multi-shell curved).
    let cyl_far = cylinder_brep([5.0, 5.0, 0.0], 0.25, 1.0);
    survey(
        "union cyl∪box disjoint",
        &boolean(&cyl_far, &bx, BoolOp::Union, &nb),
    );

    // CHAINED: can a curved output round-trip through BRep::new (Stage 1)?
    // This is the kernel-v2 → yang re-entry path for a result solid.
    let out = boolean(&cyl, &bx, BoolOp::Union, &nb).expect("union ok");
    match BRep::new(
        out.vertices().to_vec(),
        out.edges().to_vec(),
        out.faces().to_vec(),
    ) {
        Ok(rebuilt) => {
            println!("== chained BRep::new(output topology): OK ==");
            let bx2 = box_brep([0.3, -0.8, 0.2], [0.7, 0.3, 0.6]);
            survey(
                "chained (cyl∪box)∪box2",
                &boolean(&rebuilt, &bx2, BoolOp::Union, &nb),
            );
        }
        Err(e) => println!("== chained BRep::new(output topology): ERR: {e} =="),
    }
    // And: directly chaining the output BRep (carries its own mesh).
    let bx2 = box_brep([0.3, -0.8, 0.2], [0.7, 0.3, 0.6]);
    survey(
        "chained direct output∪box2",
        &boolean(&out, &bx2, BoolOp::Union, &nb),
    );
}
