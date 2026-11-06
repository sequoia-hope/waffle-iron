//! Doubled-membrane removal (spec `yang_doubled_membrane_removal.md`, task
//! #146 χ=3 sub-layer). Pins `remove_doubled_membranes`: it heals the odd-χ
//! shell that a zero-thickness fin produces, is a strict no-op on clean
//! meshes, and leaves a same-winding coincident pair (a distinct defect) for
//! the loud gate.

#[allow(unused_imports)]
use super::*;

/// A closed tetrahedron (V=4, E=6, F=4, χ=2) — the minimal valid shell.
fn tetra_tris() -> Vec<[u32; 3]> {
    // Outward-oriented faces of the tetra on verts 0,1,2,3.
    vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]
}

fn tetra_verts() -> Vec<Point3> {
    vec![
        p(0.0, 0.0, 0.0),
        p(1.0, 0.0, 0.0),
        p(0.0, 1.0, 0.0),
        p(0.0, 0.0, 1.0),
    ]
}

/// CANONICAL (I3, P9 gate): a tetrahedron with a doubled-membrane fin
/// {1,2,4} (apex 4 a spur off edge (1,2), present with both windings) reads
/// the impossible χ=3 and the shell gate stops loud. `remove_doubled_membranes`
/// drops both fin triangles → the shell heals to χ=2 and the gate passes. The
/// spur apex 4 is left dangling (compacted by the caller in production).
#[test]
pub(crate) fn doubled_membrane_heals_odd_chi_shell() {
    let mut verts = tetra_verts();
    verts.push(p(0.4, 0.4, 0.02)); // 4 = spur apex just off edge (1,2)
    let mut tris = tetra_tris();
    // Doubled membrane on edge (1,2): the two coincident opposite-winding copies.
    tris.push([1, 2, 4]);
    tris.push([1, 4, 2]);
    let mut mesh = Mesh::new(verts, tris);

    // Before: the odd-χ shell is rejected (edge (1,2) is a double cover).
    assert!(
        check_watertight_2manifold(&mesh).is_err(),
        "doubled membrane must trip the shell gate before removal"
    );

    let mut attr: Vec<Option<TriangleAttribution>> = vec![None; mesh.tris.len()];
    let mut side_labels: Vec<TriangleSideLabel> =
        vec![TriangleSideLabel::default(); mesh.tris.len()];
    let removed = remove_doubled_membranes(&mut mesh, &mut attr, &mut side_labels);
    assert_eq!(removed, 2, "exactly the two fin triangles are removed");
    assert_eq!(mesh.tris.len(), 4, "only the tetra faces remain");
    assert_eq!(attr.len(), 4, "attribution stays 1:1 with the triangles");
    assert_eq!(
        side_labels.len(),
        4,
        "side labels stay 1:1 with the triangles"
    );

    // After: the healed shell is a clean χ=2 manifold.
    assert!(
        check_watertight_2manifold(&mesh).is_ok(),
        "membrane removal must heal the shell to a valid 2-manifold"
    );
}

/// I5 (no-op on manifold outputs): a clean tetrahedron passes through
/// byte-identical — the entire green corpus relies on this.
#[test]
pub(crate) fn clean_shell_is_byte_identical() {
    let mut mesh = Mesh::new(tetra_verts(), tetra_tris());
    let before = mesh.tris.clone();
    let mut attr: Vec<Option<TriangleAttribution>> = vec![None; mesh.tris.len()];
    let mut side_labels: Vec<TriangleSideLabel> =
        vec![TriangleSideLabel::default(); mesh.tris.len()];
    let removed = remove_doubled_membranes(&mut mesh, &mut attr, &mut side_labels);
    assert_eq!(removed, 0, "a clean shell has no membrane to remove");
    assert_eq!(mesh.tris, before, "clean shell must be byte-identical");
    assert_eq!(attr.len(), 4, "attribution untouched on the no-op path");
    assert_eq!(
        side_labels.len(),
        4,
        "side labels untouched on the no-op path"
    );
}

/// P9 GUARD (adversary): two coincident triangles with the SAME winding are a
/// DIFFERENT defect (not a cancelling fin) and must be LEFT for the loud gate.
/// This pins the opposite-winding requirement — a mutant that removes any
/// same-key pair (ignoring sign) would wrongly delete these.
#[test]
pub(crate) fn same_winding_duplicate_is_left_for_the_gate() {
    let mut verts = tetra_verts();
    verts.push(p(0.4, 0.4, 0.02)); // 4
    let mut tris = tetra_tris();
    tris.push([1, 2, 4]);
    tris.push([1, 2, 4]); // SAME winding, not a membrane
    let mut mesh = Mesh::new(verts, tris);
    let mut attr: Vec<Option<TriangleAttribution>> = vec![None; mesh.tris.len()];
    let mut side_labels: Vec<TriangleSideLabel> =
        vec![TriangleSideLabel::default(); mesh.tris.len()];
    let removed = remove_doubled_membranes(&mut mesh, &mut attr, &mut side_labels);
    assert_eq!(
        removed, 0,
        "same-winding duplicate is not a cancelling fin — leave it loud"
    );
    assert_eq!(mesh.tris.len(), 6, "no triangles removed");
    assert_eq!(attr.len(), 6, "attribution untouched");
    assert_eq!(side_labels.len(), 6, "side labels untouched");
}

/// I8 (attribution lockstep) — the R0051 op-3 mechanism (2026-09-07). The
/// fin sits BEFORE a triangle carrying a DIFFERENT attribution in index
/// order; removing the fin from the mesh alone would shift that triangle
/// into the fin's attribution slot (B's inner-cylinder triangle read the
/// annulus's face and Stage 6 STOPped on a vertex a third of the model off
/// the plane). Every survivor must keep its own `(input, face)`.
#[test]
pub(crate) fn membrane_removal_keeps_attribution_in_lockstep() {
    use crate::brep::InputId;
    let at = |input: InputId, face: u32| Some(TriangleAttribution { input, face });
    let lbl = |patch: u32, surf_a: bool| TriangleSideLabel {
        cherchi_patch: patch,
        inside: [false, false],
        surface: [surf_a, !surf_a],
        sheet_opposite: None,
    };
    let mut verts = tetra_verts();
    verts.push(p(0.4, 0.4, 0.02)); // 4 = spur apex
                                   // Fin FIRST, then the tetra faces with distinct attributions, so the
                                   // survivors' indices all shift by two.
    let tris = vec![
        [1, 2, 4],
        [1, 4, 2],
        [0, 2, 1],
        [0, 1, 3],
        [0, 3, 2],
        [1, 2, 3],
    ];
    let mut attr = vec![
        at(InputId::B, 3),
        at(InputId::B, 3),
        at(InputId::A, 0),
        at(InputId::A, 1),
        at(InputId::B, 4),
        at(InputId::B, 5),
    ];
    let mut side_labels = vec![
        lbl(30, false),
        lbl(30, false),
        lbl(10, true),
        lbl(11, true),
        lbl(24, false),
        lbl(25, false),
    ];
    let mut mesh = Mesh::new(verts, tris);
    let removed = remove_doubled_membranes(&mut mesh, &mut attr, &mut side_labels);
    assert_eq!(removed, 2);
    assert_eq!(
        mesh.tris,
        tetra_tris(),
        "the four tetra faces survive in order"
    );
    assert_eq!(
        attr,
        vec![
            at(InputId::A, 0),
            at(InputId::A, 1),
            at(InputId::B, 4),
            at(InputId::B, 5),
        ],
        "each survivor keeps its OWN attribution — a mesh-only filter would \
         have handed [0,2,1] the fin's B:3 and [0,3,2] A:1"
    );
    assert_eq!(
        side_labels,
        vec![lbl(10, true), lbl(11, true), lbl(24, false), lbl(25, false)],
        "each survivor keeps its OWN side label in lockstep"
    );
    assert!(check_watertight_2manifold(&mesh).is_ok());
}

/// Orientation-sign helper: opposite cyclic rotations of a triple sort to
/// opposite parities; identical rotations sort to the same parity.
#[test]
pub(crate) fn membrane_orientation_sign_is_parity() {
    let key = [5u32, 7, 9];
    // Even (cyclic) rotations.
    assert_eq!(membrane_orientation_sign([5, 7, 9], key), 1);
    assert_eq!(membrane_orientation_sign([7, 9, 5], key), 1);
    assert_eq!(membrane_orientation_sign([9, 5, 7], key), 1);
    // Odd (reversed) rotations.
    assert_eq!(membrane_orientation_sign([5, 9, 7], key), -1);
    assert_eq!(membrane_orientation_sign([9, 7, 5], key), -1);
    assert_eq!(membrane_orientation_sign([7, 5, 9], key), -1);
}

/// A 3×3 grid torus (V=9, E=27, F=18, χ=0): a valid genus-1 closed shell.
/// Vertex `(i, j)` is `3j + i`, indices mod 3.
fn grid_torus() -> Mesh {
    let v = |i: u32, j: u32| 3 * (j % 3) + (i % 3);
    let mut verts = Vec::new();
    for j in 0..3 {
        for i in 0..3 {
            let (u, w) = (
                f64::from(i) * std::f64::consts::TAU / 3.0,
                f64::from(j) * std::f64::consts::TAU / 3.0,
            );
            let r = 2.0 + w.cos();
            verts.push(p(r * u.cos(), r * u.sin(), w.sin()));
        }
    }
    let mut tris = Vec::new();
    for j in 0..3 {
        for i in 0..3 {
            tris.push([v(i, j), v(i + 1, j), v(i + 1, j + 1)]);
            tris.push([v(i, j), v(i + 1, j + 1), v(i, j + 1)]);
        }
    }
    Mesh::new(verts, tris)
}

/// N41 retirement (2026-11-06): the shell gate names a doubled membrane
/// itself (`s4-doubled-membrane`) instead of relying on the odd χ a single
/// fin happens to produce.
#[test]
pub(crate) fn the_gate_names_a_doubled_membrane() {
    let mut verts = tetra_verts();
    verts.push(p(0.4, 0.4, 0.02));
    let mut tris = tetra_tris();
    tris.push([1, 2, 4]);
    tris.push([4, 2, 1]);
    let mesh = Mesh::new(verts, tris);
    assert_eq!(doubled_membranes(&mesh), vec![(4, 5)]);
    assert!(check_watertight_2manifold(&mesh).is_err());
    // A clean shell has none, and a same-winding duplicate is not one.
    assert!(doubled_membranes(&Mesh::new(tetra_verts(), tetra_tris())).is_empty());
    let mut same = tetra_tris();
    same.push([1, 2, 3]);
    assert!(doubled_membranes(&Mesh::new(tetra_verts(), same)).is_empty());
}

/// The silent hazard N41's own entry recorded: a membrane on three EXISTING
/// edges adds two faces and no edge or vertex, so on a genus-1 shell it reads
/// χ = 0 + 2 = 2 with every half-edge paired — exactly a sphere's numbers.
/// Before the membrane check the gate passed this zero-volume fin; now it
/// stops loud, and T4 names the fin's faces.
#[test]
pub(crate) fn a_membrane_that_evens_chi_still_stops_the_gate() {
    let torus = grid_torus();
    assert!(
        check_watertight_2manifold(&torus).is_ok(),
        "the grid torus is a valid genus-1 shell"
    );
    let mut tris = torus.tris.clone();
    // (0,0) (1,0) (2,0): a cycle of three existing horizontal edges, not a face.
    tris.push([0, 1, 2]);
    tris.push([0, 2, 1]);
    let n = tris.len();
    let mesh = Mesh::new(torus.verts.clone(), tris);
    assert_eq!(doubled_membranes(&mesh), vec![(n - 2, n - 1)]);
    assert!(
        check_watertight_2manifold(&mesh).is_err(),
        "an even-χ membrane must not pass the shell gate"
    );
    let mut attr = TriangleAttributionMap::empty();
    attr.attributions = vec![None; n];
    attr.attributions[n - 2] = Some(TriangleAttribution {
        input: crate::brep::InputId::A,
        face: 7,
    });
    attr.attributions[n - 1] = Some(TriangleAttribution {
        input: crate::brep::InputId::B,
        face: 2,
    });
    let sites = crate::boolean::failures::membrane_sites(&mesh, &attr);
    assert_eq!(sites.len(), 1);
    assert_eq!(
        sites[0].faces,
        vec![(crate::brep::InputId::A, 7), (crate::brep::InputId::B, 2)]
    );
}
