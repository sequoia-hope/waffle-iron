//! Untouched-shell passthrough, inc-1 (spec `specs/untouched_shell_passthrough.md`).
//!
//! Operands whose AABBs overlap but whose exact surfaces never meet. The
//! mesh pipeline handed Stage 6 a WHOLE closed sphere/torus face, which has
//! no mesh boundary edge to segment along, and STOPped
//! (`s6-curved-empty-cycles`, P0030) — on 12 of these 21 pairs, among them a
//! hollow ball and a box with a spherical cavity. The answer is set algebra
//! on whole shells, gated by yang's §4.3.1 contact census; the census must
//! still call a sub-sagitta GRAZE a contact (exact surfaces crossing while
//! the meshes stay apart) and certify a sub-sagitta GAP as clear.

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, to_yang_brep, validate_solid, BrepArena,
    KernelV2Error, Profile, RenderMesh, SolidId, Surface,
};

fn box_solid(a: &mut BrepArena, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x.0, y.0),
            Point2::new(x.1, y.0),
            Point2::new(x.1, y.1),
            Point2::new(x.0, y.1),
        ],
        vec![],
    )
    .expect("box profile");
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .expect("box extrude")
        .solid
}

fn cylinder(a: &mut BrepArena, c: (f64, f64), r: f64, z: (f64, f64)) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(c.0, c.1),
        r,
    )
    .expect("circle profile");
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .expect("cylinder extrude")
        .solid
}

/// Closed ring torus about the x axis: tube centre radius `rr`, tube `r`.
fn torus(a: &mut BrepArena, rr: f64, r: f64) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, rr),
        r,
    )
    .expect("torus profile");
    revolve(
        a,
        &p,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("closed torus")
    .solid
}

/// Closed sphere centred at the origin.
fn sphere(a: &mut BrepArena, r: f64) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        r,
    )
    .expect("sphere profile");
    revolve(
        a,
        &p,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("closed sphere")
    .solid
}

/// A 1×1×1 box whose face nearest the origin lies in the plane `n·x = h`,
/// `n` a unit direction chosen OFF the sphere tessellation's lattice, so a
/// sub-sagitta offset is invisible to the meshes.
fn tilted_box(a: &mut BrepArena, h: f64) -> SolidId {
    let n = [1.0_f64, 0.37, 0.53];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let n = [n[0] / l, n[1] / l, n[2] / l];
    // An in-plane frame: u ⊥ n, v = n × u.
    let u = {
        let t = [-n[1], n[0], 0.0];
        let tl = (t[0] * t[0] + t[1] * t[1]).sqrt();
        [t[0] / tl, t[1] / tl, 0.0]
    };
    let v = [
        n[1] * u[2] - n[2] * u[1],
        n[2] * u[0] - n[0] * u[2],
        n[0] * u[1] - n[1] * u[0],
    ];
    let p = Profile::new(
        Point3::new(h * n[0], h * n[1], h * n[2]),
        Vector3::new(u[0], u[1], u[2]),
        Vector3::new(v[0], v[1], v[2]),
        vec![
            Point2::new(-0.5, -0.5),
            Point2::new(0.5, -0.5),
            Point2::new(0.5, 0.5),
            Point2::new(-0.5, 0.5),
        ],
        vec![],
    )
    .expect("tilted box profile");
    extrude(a, &p, Vector3::new(n[0], n[1], n[2]), 1.0)
        .expect("tilted box extrude")
        .solid
}

fn volume(m: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = i as usize * 3;
        [m.positions[k], m.positions[k + 1], m.positions[k + 2]]
    };
    let mut six = 0.0;
    for t in m.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six / 6.0
}

/// What the boolean of the pair must be, in terms of the operands' own
/// render volumes (the kept shells are COPIES, so their render meshes are
/// the operands' — the volumes compose exactly).
#[derive(Debug, Clone, Copy)]
enum Want {
    /// `shells`, volume `ka·vA + kb·vB`.
    Solid {
        shells: usize,
        ka: f64,
        kb: f64,
    },
    Empty,
}

type Build = fn(&mut BrepArena) -> (SolidId, SolidId);

fn check(name: &str, build: Build, op: BoolOp, want: Want) {
    let mut a = BrepArena::new();
    let (x, y) = build(&mut a);
    let vx = volume(&tessellate(&a, x).expect("tessellate A"));
    let vy = volume(&tessellate(&a, y).expect("tessellate B"));
    let got = boolean_op(&mut a, x, y, op);
    match (want, got) {
        (Want::Empty, Err(KernelV2Error::EmptyBooleanResult)) => {}
        (Want::Solid { shells, ka, kb }, Ok(s)) => {
            let report = validate_solid(&a, s)
                .unwrap_or_else(|e| panic!("{name} {op:?}: output invalid: {e:?}"));
            assert_eq!(report.shells, shells, "{name} {op:?}: shell count");
            let v = volume(&tessellate(&a, s).expect("tessellate output"));
            let expect = ka * vx + kb * vy;
            assert!(
                (v - expect).abs() <= 1e-9 * expect.abs().max(1.0),
                "{name} {op:?}: volume {v}, want {expect} (vA {vx}, vB {vy})"
            );
        }
        (w, g) => panic!("{name} {op:?}: want {w:?}, got {g:?}"),
    }
}

const fn solid(shells: usize, ka: f64, kb: f64) -> Want {
    Want::Solid { shells, ka, kb }
}

/// The census of the session that found P0030, as pins: per pair, the
/// union / subtract / intersect answers.
#[test]
fn surface_disjoint_pairs_compose_as_whole_shells() {
    let cases: [(&str, Build, [Want; 3]); 7] = [
        (
            "torus with a box through its hole",
            |a| {
                (
                    torus(a, 3.0, 1.0),
                    box_solid(a, (-5.0, 5.0), (-1.0, 1.0), (-1.0, 1.0)),
                )
            },
            [solid(2, 1.0, 1.0), solid(1, 1.0, 0.0), Want::Empty],
        ),
        (
            "box through a torus's hole",
            |a| {
                (
                    box_solid(a, (-5.0, 5.0), (-1.0, 1.0), (-1.0, 1.0)),
                    torus(a, 3.0, 1.0),
                )
            },
            [solid(2, 1.0, 1.0), solid(1, 1.0, 0.0), Want::Empty],
        ),
        (
            "sphere around a box",
            |a| {
                (
                    sphere(a, 3.0),
                    box_solid(a, (-1.0, 1.0), (-1.0, 1.0), (-1.0, 1.0)),
                )
            },
            // A hollow ball: the box survives COMPLEMENTED, as the cavity.
            [solid(1, 1.0, 0.0), solid(2, 1.0, -1.0), solid(1, 0.0, 1.0)],
        ),
        (
            "box around a sphere",
            |a| {
                (
                    box_solid(a, (-4.0, 4.0), (-4.0, 4.0), (-4.0, 4.0)),
                    sphere(a, 1.0),
                )
            },
            [solid(1, 1.0, 0.0), solid(2, 1.0, -1.0), solid(1, 0.0, 1.0)],
        ),
        (
            "box around a torus",
            |a| {
                (
                    box_solid(a, (-6.0, 6.0), (-6.0, 6.0), (-6.0, 6.0)),
                    torus(a, 3.0, 1.0),
                )
            },
            [solid(1, 1.0, 0.0), solid(2, 1.0, -1.0), solid(1, 0.0, 1.0)],
        ),
        (
            "sphere beside a box in its AABB corner",
            |a| {
                (
                    sphere(a, 3.0),
                    box_solid(a, (2.5, 3.5), (2.5, 3.5), (-0.5, 0.5)),
                )
            },
            [solid(2, 1.0, 1.0), solid(1, 1.0, 0.0), Want::Empty],
        ),
        (
            "cylinder beside a box in its AABB corner",
            |a| {
                (
                    cylinder(a, (0.0, 0.0), 1.0, (0.0, 2.0)),
                    box_solid(a, (0.9, 2.0), (0.9, 2.0), (0.5, 1.5)),
                )
            },
            [solid(2, 1.0, 1.0), solid(1, 1.0, 0.0), Want::Empty],
        ),
    ];
    for (name, build, wants) in cases {
        for (op, want) in [BoolOp::Union, BoolOp::Subtract, BoolOp::Intersect]
            .into_iter()
            .zip(wants)
        {
            check(name, build, op, want);
        }
    }
}

/// The cavity a complemented shell makes carries the INVERTED sense on every
/// face: a sphere cavity's surface is `reversed`, a box cavity's planes face
/// into the void.
#[test]
fn a_complemented_shell_faces_into_the_cavity() {
    let mut a = BrepArena::new();
    let body = box_solid(&mut a, (-4.0, 4.0), (-4.0, 4.0), (-4.0, 4.0));
    let tool = sphere(&mut a, 1.0);
    let out = boolean_op(&mut a, body, tool, BoolOp::Subtract).expect("box − inner sphere");
    let solid = a.solid(out).expect("output solid");
    assert_eq!(solid.shells.len(), 2);
    let cavity = a.shell(solid.shells[1]).expect("cavity shell");
    assert_eq!(cavity.faces.len(), 1, "the sphere's one face");
    match a.face(cavity.faces[0]).expect("cavity face").surface {
        Some(Surface::Sphere { reversed, .. }) => assert!(reversed, "cavity wall faces inward"),
        other => panic!("cavity face surface {other:?}"),
    }
}

/// Every output face has a persistent id, and the journal's last entry is the
/// boolean, recording each copied face as `Same` from its operand face.
#[test]
fn passthrough_faces_carry_operand_lineage() {
    let mut a = BrepArena::new();
    let body = sphere(&mut a, 3.0);
    let tool = box_solid(&mut a, (-1.0, 1.0), (-1.0, 1.0), (-1.0, 1.0));
    let operand_pids: Vec<_> = [body, tool]
        .iter()
        .flat_map(|&s| a.solid(s).expect("operand").shells.clone())
        .flat_map(|sh| a.shell(sh).expect("shell").faces.clone())
        .map(|f| a.face_pid(f).expect("operand face pid"))
        .collect();
    let out = boolean_op(&mut a, body, tool, BoolOp::Subtract).expect("hollow ball");
    let out_faces: Vec<_> = a
        .solid(out)
        .expect("output")
        .shells
        .clone()
        .into_iter()
        .flat_map(|sh| a.shell(sh).expect("shell").faces.clone())
        .collect();
    assert_eq!(out_faces.len(), 7, "the sphere and the box's six faces");
    let evo = a.journal.last().expect("journal entry");
    assert!(matches!(
        evo.op,
        kernel_v2::OpTag::Boolean(BoolOp::Subtract)
    ));
    assert!(evo.generated.is_empty() && evo.deleted.is_empty());
    for f in out_faces {
        let pid = a.face_pid(f).expect("output face pid");
        assert!(
            evo.modified.iter().any(|&(src, dst, k)| dst == pid
                && k == kernel_v2::EvoKind::Same
                && operand_pids.contains(&src)),
            "output face {f:?} has no `Same` lineage to an operand face"
        );
    }
}

fn census(
    a: &BrepArena,
    x: SolidId,
    y: SolidId,
) -> (Vec<yang_rs::ShellContact>, Vec<yang_rs::ShellContact>) {
    let (ya, yb) = (
        to_yang_brep(a, x).expect("A"),
        to_yang_brep(a, y).expect("B"),
    );
    let all = |b: &yang_rs::BRep| vec![(0..b.faces().len() as u32).collect::<Vec<_>>()];
    yang_rs::shell_contact_census(&ya, &yb, &all(&ya), &all(&yb)).expect("census")
}

/// A box face 1e-4 INSIDE a radius-3 sphere: the exact surfaces cross on a
/// cap of radius ≈ 0.0245 while the sphere's mesh, a chord surface sagging
/// far more than 1e-4 below it, never reaches the box. The census must call
/// it a contact (the §4.3.1 2dε filter), so no set-algebra answer is given —
/// whatever the pipeline then says, it is not "two disjoint shells".
#[test]
fn a_sub_sagitta_graze_is_a_contact() {
    let mut a = BrepArena::new();
    let ball = sphere(&mut a, 3.0);
    let block = tilted_box(&mut a, 3.0 - 1e-4);
    let (va, vb) = census(&a, ball, block);
    assert_eq!(va, vec![yang_rs::ShellContact::Contact]);
    assert_eq!(vb, vec![yang_rs::ShellContact::Contact]);
    if let Ok(out) = boolean_op(&mut a, ball, block, BoolOp::Union) {
        let shells = validate_solid(&a, out).expect("valid").shells;
        assert_eq!(
            shells, 1,
            "a graze that crosses is ONE body, never two shells"
        );
    }
}

/// The mirror case: the box face 1e-4 OUTSIDE the sphere — a gap the meshes
/// cannot resolve either (a chord surface's deviation bound is far larger),
/// which the refined census still certifies.
#[test]
fn a_sub_sagitta_gap_is_certified_clear() {
    let mut a = BrepArena::new();
    let ball = sphere(&mut a, 3.0);
    let block = tilted_box(&mut a, 3.0 + 1e-4);
    let (va, vb) = census(&a, ball, block);
    let clear = yang_rs::ShellContact::Clear {
        inside_other: false,
    };
    assert_eq!((va, vb), (vec![clear], vec![clear]));
    let out = boolean_op(&mut a, ball, block, BoolOp::Union).expect("disjoint union");
    assert_eq!(validate_solid(&a, out).expect("valid").shells, 2);
}

/// Small ball centred at `(cx, 0, 0)`.
fn ball_at(a: &mut BrepArena, cx: f64, r: f64) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(cx, 0.0),
        r,
    )
    .expect("ball profile");
    revolve(
        a,
        &p,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("closed ball")
    .solid
}

/// Multi-shell operands: a hollow ball (sphere r=3 with a 2×2×2 box cavity)
/// and a small ball floating INSIDE the cavity, clear of every wall. The
/// cavity is outside the hollow ball's material — the winding number there is
/// 1 (outer sphere) − 1 (complemented cavity) = 0 — so the census must read
/// the small ball as OUTSIDE: a union keeps all three shells, a subtract
/// leaves the hollow ball as it was, an intersect is empty.
#[test]
fn a_ball_floating_in_a_cavity_is_outside_the_hollow_ball() {
    let build = |a: &mut BrepArena| {
        let outer = sphere(a, 3.0);
        let core = box_solid(a, (-1.0, 1.0), (-1.0, 1.0), (-1.0, 1.0));
        let hollow = boolean_op(a, outer, core, BoolOp::Subtract).expect("hollow ball");
        assert_eq!(validate_solid(a, hollow).expect("valid").shells, 2);
        let floating = ball_at(a, 0.0, 0.5);
        (hollow, floating)
    };
    for (op, want) in [
        (BoolOp::Union, solid(3, 1.0, 1.0)),
        (BoolOp::Subtract, solid(2, 1.0, 0.0)),
        (BoolOp::Intersect, Want::Empty),
    ] {
        let mut a = BrepArena::new();
        let (x, y) = build(&mut a);
        let vx = volume(&tessellate(&a, x).expect("tessellate A"));
        let vy = volume(&tessellate(&a, y).expect("tessellate B"));
        match (want, boolean_op(&mut a, x, y, op)) {
            (Want::Empty, Err(KernelV2Error::EmptyBooleanResult)) => {}
            (Want::Solid { shells, ka, kb }, Ok(s)) => {
                assert_eq!(
                    validate_solid(&a, s).expect("valid").shells,
                    shells,
                    "{op:?}"
                );
                let v = volume(&tessellate(&a, s).expect("tessellate"));
                let expect = ka * vx + kb * vy;
                assert!(
                    (v - expect).abs() <= 1e-9 * expect,
                    "{op:?}: {v} vs {expect}"
                );
            }
            (w, g) => panic!("{op:?}: want {w:?}, got {g:?}"),
        }
    }
}

/// inc-2 (spec §5): a box with a spherical cavity, cut on its OUTER skin by
/// a second box. The cavity sphere is a clear closed shell beside a touched
/// one, so the whole-operand rule declines and the pipeline still hands
/// Stage 6 the boundaryless cavity face (`s6-curved-empty-cycles`).
#[test]
#[ignore = "untouched_shell_passthrough inc-2: a clear closed shell beside a touched one"]
fn a_cut_on_the_outer_skin_keeps_the_spherical_cavity() {
    let mut a = BrepArena::new();
    let body = box_solid(&mut a, (-4.0, 4.0), (-4.0, 4.0), (-4.0, 4.0));
    let core = sphere(&mut a, 1.0);
    let hollow = boolean_op(&mut a, body, core, BoolOp::Subtract).expect("box with a cavity");
    let notch = box_solid(&mut a, (3.0, 5.0), (-1.0, 1.0), (-1.0, 1.0));
    let vh = volume(&tessellate(&a, hollow).expect("tessellate"));
    let out = boolean_op(&mut a, hollow, notch, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("notching a box with a spherical cavity: {e:?}"));
    assert_eq!(validate_solid(&a, out).expect("valid").shells, 2);
    let v = volume(&tessellate(&a, out).expect("tessellate"));
    assert!((v - (vh - 4.0)).abs() < 1e-9 * vh, "{v} vs {}", vh - 4.0);
}
