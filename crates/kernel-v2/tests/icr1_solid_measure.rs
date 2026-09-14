//! ICR-1 of `specs/waffle_mcp_server.md`: exact solid measurement through the
//! kernel trait. `KernelIntrospect::solid_volume` / `solid_surface_area` expose
//! kernel-v2's exact-rational B-Rep integrals (`geom::signed_volume`,
//! `introspect::surface_area`) to consumers that may not reach into kernel
//! internals; a kernel without them answers a typed `NotSupported`, never a
//! guessed number.

use std::collections::HashMap;

use kernel_v2::KernelV2Adapter;
use waffle_types::kernel::{Kernel, KernelIntrospect, KernelSolidHandle};
use waffle_types::{CircleProfile, ClosedProfile};

fn rectangle(kernel: &mut KernelV2Adapter, a: f64, b: f64, height: f64) -> KernelSolidHandle {
    let positions: HashMap<u32, (f64, f64)> =
        [(1, (0.0, 0.0)), (2, (a, 0.0)), (3, (a, b)), (4, (0.0, b))]
            .into_iter()
            .collect();
    let faces = kernel
        .make_faces_from_profiles(
            &[ClosedProfile {
                entity_ids: vec![1, 2, 3, 4],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("rectangle stages");
    kernel
        .extrude_face(faces[0], [0.0, 0.0, 1.0], height)
        .expect("box")
}

fn cylinder(kernel: &mut KernelV2Adapter, radius: f64, height: f64) -> KernelSolidHandle {
    let faces = kernel
        .make_faces_from_profiles(
            &[ClosedProfile {
                entity_ids: vec![7],
                is_outer: true,
                vertex_ids: vec![],
                circle: Some(CircleProfile {
                    center_u: 0.0,
                    center_v: 0.0,
                    radius,
                }),
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &HashMap::new(),
        )
        .expect("circle stages");
    kernel
        .extrude_face(faces[0], [0.0, 0.0, 1.0], height)
        .expect("cylinder")
}

#[test]
fn box_volume_and_area_are_exact() {
    // Spec oracle O1: 20 mm × 10 mm × 5 mm.
    let mut kernel = KernelV2Adapter::new();
    let solid = rectangle(&mut kernel, 0.02, 0.01, 0.005);

    let volume = kernel.solid_volume(&solid).expect("planar solid measures");
    assert!((volume - 1.0e-6).abs() <= 1e-15, "volume {volume:e}");

    let area = kernel
        .solid_surface_area(&solid)
        .expect("planar solid measures");
    let expected = 2.0 * (0.02 * 0.01 + 0.01 * 0.005 + 0.02 * 0.005);
    assert!(
        (area - expected).abs() <= 1e-15,
        "area {area:e} vs {expected:e}"
    );
}

#[test]
fn cylinder_volume_and_area_are_exact_not_chordal() {
    // Spec oracle O2: r = 5 mm, h = 10 mm. A tessellation would come in low;
    // the B-Rep integral does not.
    let (r, h) = (0.005, 0.01);
    let mut kernel = KernelV2Adapter::new();
    let solid = cylinder(&mut kernel, r, h);

    let volume = kernel.solid_volume(&solid).expect("cylinder measures");
    let exact_v = std::f64::consts::PI * r * r * h;
    assert!(
        (volume - exact_v).abs() <= 1e-15,
        "{volume:e} vs {exact_v:e}"
    );

    let area = kernel
        .solid_surface_area(&solid)
        .expect("cylinder measures");
    let exact_a = 2.0 * std::f64::consts::PI * r * (h + r);
    assert!((area - exact_a).abs() <= 1e-15, "{area:e} vs {exact_a:e}");
}

#[test]
fn a_dead_handle_is_an_error_not_zero() {
    let kernel = KernelV2Adapter::new();
    let bogus = KernelSolidHandle::from_raw(987_654);
    assert!(kernel.solid_volume(&bogus).is_err());
    assert!(kernel.solid_surface_area(&bogus).is_err());
}
