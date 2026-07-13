use super::tessellate;
use crate::arena::UnitVector3;
use crate::cone_fixtures::build_frustum;
use cad_primitives::Point3;
use std::f64::consts::FRAC_PI_4;

#[test]
fn frustum_lateral_tessellates_with_tilted_outward_normals() {
    // 45° frustum, apex at the origin, axis +z, rims at radii 1 and 2.
    let plus_z = UnitVector3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };
    let (arena, solid, lat) = build_frustum(
        Point3::new(0.0, 0.0, 0.0),
        plus_z,
        1.0,
        2.0,
        FRAC_PI_4,
        FRAC_PI_4,
    );
    let mesh = tessellate(&arena, solid).expect("frustum tessellates");

    let nv = mesh.num_vertices();
    assert!(
        mesh.indices.iter().all(|&i| (i as usize) < nv),
        "all triangle indices in range"
    );

    // Isolate the cone lateral's triangles.
    let fr = mesh
        .face_ranges
        .iter()
        .find(|r| r.face == lat)
        .expect("lateral face range present");
    assert!(fr.count > 0 && fr.count % 3 == 0, "whole triangles");

    let want_z = -(FRAC_PI_4.sin()); // tilt toward the apex: n·axis = −sin α
    let want_xy = FRAC_PI_4.cos(); // radial magnitude = cos α
    let s = fr.start as usize;
    let e = s + fr.count as usize;
    for &idx in &mesh.indices[s..e] {
        let i = idx as usize;
        let n = [
            mesh.normals[3 * i],
            mesh.normals[3 * i + 1],
            mesh.normals[3 * i + 2],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-9, "unit normal, got {len}");
        assert!((n[2] - want_z).abs() < 1e-9, "n.z={} want {want_z}", n[2]);
        let xy = (n[0] * n[0] + n[1] * n[1]).sqrt();
        assert!((xy - want_xy).abs() < 1e-9, "radial magnitude cos(α)");
        // Outward: the radial component agrees with the position's radial
        // (apex at origin, axis +z ⇒ position radial = (x, y)).
        let p = [mesh.positions[3 * i], mesh.positions[3 * i + 1]];
        assert!(n[0] * p[0] + n[1] * p[1] > 0.0, "outward radial");
    }
}
