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

    let removed = remove_doubled_membranes(&mut mesh);
    assert_eq!(removed, 2, "exactly the two fin triangles are removed");
    assert_eq!(mesh.tris.len(), 4, "only the tetra faces remain");

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
    let removed = remove_doubled_membranes(&mut mesh);
    assert_eq!(removed, 0, "a clean shell has no membrane to remove");
    assert_eq!(mesh.tris, before, "clean shell must be byte-identical");
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
    let removed = remove_doubled_membranes(&mut mesh);
    assert_eq!(
        removed, 0,
        "same-winding duplicate is not a cancelling fin — leave it loud"
    );
    assert_eq!(mesh.tris.len(), 6, "no triangles removed");
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
