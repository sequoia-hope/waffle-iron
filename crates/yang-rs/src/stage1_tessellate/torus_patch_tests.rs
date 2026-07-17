use super::*;
use std::collections::BTreeMap;

#[allow(clippy::too_many_arguments)]
fn eval(
    center: Point3,
    ax: [f64; 3],
    e1a: [f64; 3],
    e2a: [f64; 3],
    major: f64,
    minor: f64,
    u: f64,
    v: f64,
) -> Point3 {
    let c = center.as_array();
    let (cu, su) = (u.cos(), u.sin());
    let (cv, sv) = (v.cos(), v.sin());
    let rad = major + minor * cu;
    Point3::new(
        c[0] + rad * (cv * e1a[0] + sv * e2a[0]) + minor * su * ax[0],
        c[1] + rad * (cv * e1a[1] + sv * e2a[1]) + minor * su * ax[1],
        c[2] + rad * (cv * e1a[2] + sv * e2a[2]) + minor * su * ax[2],
    )
}

#[test]
fn torus_patch_roundtrip_on_surface_watertight() {
    // Torus: center origin, axis +Z, R=3, r=1.
    let center = Point3::new(0.0, 0.0, 0.0);
    let axis = Vector3::new(0.0, 0.0, 1.0);
    let (major, minor) = (3.0_f64, 1.0_f64);
    let ax = normalize3(axis.as_array());
    let (e1, e2) = ortho_basis(axis);
    let (e1a, e2a) = (e1.as_array(), e2.as_array());

    // A sub-(u,v)-rectangle patch boundary, finely sampled along its 4 edges.
    let (u0, u1, v0, v1) = (0.2_f64, 1.2_f64, 0.5_f64, 1.8_f64);
    let ns = 8;
    let mut boundary: Vec<Point3> = Vec::new();
    let mut push = |u: f64, v: f64| boundary.push(eval(center, ax, e1a, e2a, major, minor, u, v));
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        push(u0 + (u1 - u0) * t, v0);
    }
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        push(u1, v0 + (v1 - v0) * t);
    }
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        push(u1 - (u1 - u0) * t, v1);
    }
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        push(u0, v1 - (v1 - v0) * t);
    }

    let n = boundary.len();
    let (verts, tris) = tessellate_torus_patch(center, axis, major, minor, &boundary, &[], 0.05)
        .expect("patch tessellation");

    // Interior Steiner points were added (refinement actually fired).
    assert!(verts.len() > n, "no Steiner points: {} verts", verts.len());

    // Boundary verts preserved bit-for-bit (conformal).
    for i in 0..n {
        assert_eq!(verts[i], boundary[i], "boundary vert {i} moved");
    }

    // Every vert lies on the torus surface.
    let surf = Surface::Torus {
        center,
        axis_dir: axis,
        major_radius: major,
        minor_radius: minor,
    };
    for (i, &p) in verts.iter().enumerate() {
        let d = signed_distance_to_surface(surf, p).expect("torus distance");
        assert!(d.abs() < 1e-9, "vert {i} off torus: d={d}");
    }

    // Manifold/watertight: every edge in 1 (boundary) or 2 (interior) tris.
    let mut edges: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in &tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    assert!(
        edges.values().all(|&c| c == 1 || c == 2),
        "non-manifold edge present"
    );

    // The closed boundary loop is exactly the count-1 edges (no slits).
    let boundary_edges = edges.values().filter(|&&c| c == 1).count();
    assert_eq!(
        boundary_edges, n,
        "boundary edge count {boundary_edges} != {n}"
    );

    // The chorded 3D area matches the analytic patch area (a faithful, hole-
    // free, non-folded tessellation). Analytic area of the (u,v) rectangle:
    //   ∫∫ (R + r·cos u)·r du dv = r·(v1−v0)·[R·(u1−u0) + r·(sin u1 − sin u0)].
    let analytic = minor * (v1 - v0) * (major * (u1 - u0) + minor * (u1.sin() - u0.sin()));
    let mut area3d = 0.0;
    for t in &tris {
        let a = verts[t[0] as usize].as_array();
        let b = verts[t[1] as usize].as_array();
        let c = verts[t[2] as usize].as_array();
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cr = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        area3d += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
    }
    // Inscribed chords slightly under-shoot the smooth area; refinement keeps
    // it within ~1%. It must never exceed the smooth area (that would signal
    // folded/overlapping triangles).
    assert!(
        area3d <= analytic * (1.0 + 1e-6) && area3d >= analytic * 0.985,
        "area3d {area3d} vs analytic {analytic} (folds or holes?)"
    );
}

#[test]
fn torus_patch_rejects_degenerate() {
    let center = Point3::new(0.0, 0.0, 0.0);
    let axis = Vector3::new(0.0, 0.0, 1.0);
    let too_few = [Point3::new(4.0, 0.0, 0.0), Point3::new(3.0, 0.0, 1.0)];
    assert!(tessellate_torus_patch(center, axis, 3.0, 1.0, &too_few, &[], 0.05).is_none());
}

/// Seam-wrapping (cylindrical) BAND render: a longitude slice v ∈ [v0, v1] of
/// the tube wraps the full meridian (u ∈ [0, 2π)). Bounded by two meridian
/// circles (opposite winding), it is triangulated via the universal-cover
/// seam bridge into a watertight, on-tube mesh.
#[test]
fn torus_band_seam_wrapping_render() {
    let center = Point3::new(0.0, 0.0, 0.0);
    let axis = Vector3::new(0.0, 0.0, 1.0);
    let (major, minor) = (3.0_f64, 1.0_f64);
    let ax = normalize3(axis.as_array());
    let (e1, e2) = ortho_basis(axis);
    let (e1a, e2a) = (e1.as_array(), e2.as_array());
    // A FULL-quarter longitude slice (Δv = π/2): a large band whose seam
    // bridges must be subdivided, or the edge regions stay coarse and the
    // chorded area undershoots (the KV6d band-render regression).
    let (v0, v1) = (0.0_f64, std::f64::consts::FRAC_PI_2);
    let nu = 24;
    // Two meridian circles: v0 wound +u (wrap +1), v1 wound −u (wrap −1).
    let mut c0: Vec<Point3> = Vec::new();
    let mut c1: Vec<Point3> = Vec::new();
    for k in 0..nu {
        let u = std::f64::consts::TAU * (k as f64) / (nu as f64);
        c0.push(eval(center, ax, e1a, e2a, major, minor, u, v0));
        c1.push(eval(center, ax, e1a, e2a, major, minor, -u, v1));
    }
    let (verts, tris) = tessellate_torus_patch(center, axis, major, minor, &c0, &[c1], 0.05)
        .expect("band tessellation");
    assert!(!tris.is_empty(), "non-empty band mesh");

    // Every vertex on the tube.
    let surf = torus(major, minor);
    for (i, &p) in verts.iter().enumerate() {
        let d = signed_distance_to_surface(surf, p).unwrap();
        assert!(d.abs() < 1e-9, "band vert {i} off tube: {d:e}");
    }
    // Manifold + watertight across the SEAM: group edges by 3D POSITION (the
    // periodic seam's duplicated vertices coincide in 3D). Every edge is
    // shared by 2 tris (interior + the seam, where the universal-cover bridge
    // duplicates coincide) EXCEPT the band's two real meridian-circle
    // boundaries at v0 / v1 (shared by 1). No edge is shared by >2.
    let key = |p: Point3| {
        let a = p.as_array();
        [
            (a[0] * 1e7).round() as i64,
            (a[1] * 1e7).round() as i64,
            (a[2] * 1e7).round() as i64,
        ]
    };
    let mut edges: BTreeMap<([i64; 3], [i64; 3]), u32> = BTreeMap::new();
    for t in &tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(verts[a as usize]), key(verts[b as usize]));
            let e = if ka < kb { (ka, kb) } else { (kb, ka) };
            *edges.entry(e).or_insert(0) += 1;
        }
    }
    assert!(
        edges.values().all(|&c| c == 1 || c == 2),
        "non-manifold edge (some positional edge in >2 tris) — seam not watertight"
    );
    // Exactly the two meridian-circle boundaries (nu edges each) are count-1;
    // the seam bridges coincide in 3D and are interior (count-2).
    let boundary_edges = edges.values().filter(|&&c| c == 1).count();
    assert_eq!(
        boundary_edges,
        2 * nu,
        "expected the two v-circle boundaries ({} edges), got {boundary_edges}",
        2 * nu
    );

    // The chorded area approaches the analytic band area
    // ∫∫ (R + r cos u)·r du dv = r·(v1−v0)·2π·R  (∫cos u over a full turn = 0).
    let analytic = minor * (v1 - v0) * std::f64::consts::TAU * major;
    let mut area = 0.0;
    for t in &tris {
        let a = verts[t[0] as usize].as_array();
        let b = verts[t[1] as usize].as_array();
        let c = verts[t[2] as usize].as_array();
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cr = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        area += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
    }
    assert!(
        area <= analytic * (1.0 + 1e-6) && area >= analytic * 0.97,
        "band area {area} vs analytic {analytic}"
    );
}

/// KV14 Slice F-2: a seam-wrapping torus BAND with a WINDOW HOLE in the tube
/// wall — the two meridian-circle band edges wrap the full meridian, and a
/// small non-wrapping (u,v) window is excluded. The band's universal-cover
/// seam bridge must still triangulate the outer ring while the window is
/// carved as a CDT hole (placed into the band's unrolled u-period).
#[test]
fn torus_band_with_window_hole_render() {
    let center = Point3::new(0.0, 0.0, 0.0);
    let axis = Vector3::new(0.0, 0.0, 1.0);
    let (major, minor) = (3.0_f64, 1.0_f64);
    let ax = normalize3(axis.as_array());
    let (e1, e2) = ortho_basis(axis);
    let (e1a, e2a) = (e1.as_array(), e2.as_array());
    // Band longitude slice v ∈ [0, π/2]; meridian wraps fully.
    let (v0, v1) = (0.0_f64, std::f64::consts::FRAC_PI_2);
    let nu = 24;
    let mut c0: Vec<Point3> = Vec::new();
    let mut c1: Vec<Point3> = Vec::new();
    for k in 0..nu {
        let u = std::f64::consts::TAU * (k as f64) / (nu as f64);
        c0.push(eval(center, ax, e1a, e2a, major, minor, u, v0));
        c1.push(eval(center, ax, e1a, e2a, major, minor, -u, v1));
    }
    // A small non-wrapping window inside the band: u ∈ [1.0, 2.0],
    // v ∈ [0.4, 1.0] (both well inside the band's ranges, non-wrapping).
    let (wu0, wu1, wv0, wv1) = (1.0_f64, 2.0_f64, 0.4_f64, 1.0_f64);
    let nw = 6;
    let mut win: Vec<Point3> = Vec::new();
    let mut wpush = |u: f64, v: f64| win.push(eval(center, ax, e1a, e2a, major, minor, u, v));
    for k in 0..nw {
        wpush(wu0 + (wu1 - wu0) * (k as f64 / nw as f64), wv0);
    }
    for k in 0..nw {
        wpush(wu1, wv0 + (wv1 - wv0) * (k as f64 / nw as f64));
    }
    for k in 0..nw {
        wpush(wu1 - (wu1 - wu0) * (k as f64 / nw as f64), wv1);
    }
    for k in 0..nw {
        wpush(wu0, wv1 - (wv1 - wv0) * (k as f64 / nw as f64));
    }
    let win_edges = win.len();

    let (verts, tris) = tessellate_torus_patch(center, axis, major, minor, &c0, &[c1, win], 0.05)
        .expect("holed band tessellation");
    assert!(!tris.is_empty(), "non-empty holed band mesh");

    // Every vertex on the tube.
    let surf = torus(major, minor);
    for (i, &p) in verts.iter().enumerate() {
        let d = signed_distance_to_surface(surf, p).unwrap();
        assert!(d.abs() < 1e-9, "holed band vert {i} off tube: {d:e}");
    }
    // Manifold + watertight by 3D position; the three boundaries (2 meridian
    // circles + 1 window) are count-1, everything else count-2, none > 2.
    let key = |p: Point3| {
        let a = p.as_array();
        [
            (a[0] * 1e7).round() as i64,
            (a[1] * 1e7).round() as i64,
            (a[2] * 1e7).round() as i64,
        ]
    };
    let mut edges: BTreeMap<([i64; 3], [i64; 3]), u32> = BTreeMap::new();
    for t in &tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(verts[a as usize]), key(verts[b as usize]));
            let e = if ka < kb { (ka, kb) } else { (kb, ka) };
            *edges.entry(e).or_insert(0) += 1;
        }
    }
    assert!(
        edges.values().all(|&c| c == 1 || c == 2),
        "non-manifold edge (some positional edge in >2 tris)"
    );
    let boundary_edges = edges.values().filter(|&&c| c == 1).count();
    assert_eq!(
        boundary_edges,
        2 * nu + win_edges,
        "expected 2 meridian circles ({}) + 1 window ({win_edges}), got {boundary_edges}",
        2 * nu
    );

    // Chorded area ≈ analytic band area MINUS the excluded window area.
    //   band:   r·(v1−v0)·2π·R
    //   window: r·(wv1−wv0)·[R·(wu1−wu0) + r·(sin wu1 − sin wu0)]
    let band_area = minor * (v1 - v0) * std::f64::consts::TAU * major;
    let win_area = minor * (wv1 - wv0) * (major * (wu1 - wu0) + minor * (wu1.sin() - wu0.sin()));
    let analytic = band_area - win_area;
    let mut area = 0.0;
    for t in &tris {
        let a = verts[t[0] as usize].as_array();
        let b = verts[t[1] as usize].as_array();
        let c = verts[t[2] as usize].as_array();
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cr = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        area += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
    }
    assert!(
        area <= analytic * (1.0 + 1e-6) && area >= analytic * 0.97,
        "holed band area {area} vs analytic {analytic} (band {band_area} − window {win_area})"
    );
}

/// KV14 Slice F-2 seam-avoidance branch: the window straddles the DEFAULT
/// seam (meridian u=0, where both band edges are anchored). `band_seam_bridge`
/// must skip that anchor and cut the seam elsewhere, so the window projects as
/// a simple interior hole (not split across the seam → CDT self-intersection).
/// This is R0028's complement-band wall in miniature.
#[test]
fn torus_band_window_on_seam_render() {
    let center = Point3::new(0.0, 0.0, 0.0);
    let axis = Vector3::new(0.0, 0.0, 1.0);
    let (major, minor) = (3.0_f64, 1.0_f64);
    let ax = normalize3(axis.as_array());
    let (e1, e2) = ortho_basis(axis);
    let (e1a, e2a) = (e1.as_array(), e2.as_array());
    let (v0, v1) = (0.0_f64, std::f64::consts::FRAC_PI_2);
    let nu = 24;
    // Band edges anchored at meridian u=0 (k=0), same as the default seam.
    let mut c0: Vec<Point3> = Vec::new();
    let mut c1: Vec<Point3> = Vec::new();
    for k in 0..nu {
        let u = std::f64::consts::TAU * (k as f64) / (nu as f64);
        c0.push(eval(center, ax, e1a, e2a, major, minor, u, v0));
        c1.push(eval(center, ax, e1a, e2a, major, minor, -u, v1));
    }
    // Window centred ON the u=0 seam: u ∈ [−0.3, 0.3], v ∈ [0.4, 1.0].
    let (wu0, wu1, wv0, wv1) = (-0.3_f64, 0.3_f64, 0.4_f64, 1.0_f64);
    let nw = 6;
    let mut win: Vec<Point3> = Vec::new();
    let mut wpush = |u: f64, v: f64| win.push(eval(center, ax, e1a, e2a, major, minor, u, v));
    for k in 0..nw {
        wpush(wu0 + (wu1 - wu0) * (k as f64 / nw as f64), wv0);
    }
    for k in 0..nw {
        wpush(wu1, wv0 + (wv1 - wv0) * (k as f64 / nw as f64));
    }
    for k in 0..nw {
        wpush(wu1 - (wu1 - wu0) * (k as f64 / nw as f64), wv1);
    }
    for k in 0..nw {
        wpush(wu0, wv1 - (wv1 - wv0) * (k as f64 / nw as f64));
    }
    let win_edges = win.len();

    let (verts, tris) = tessellate_torus_patch(center, axis, major, minor, &c0, &[c1, win], 0.05)
        .expect("seam-straddling window band tessellates (seam avoided)");

    // Every vertex on the tube.
    let surf = torus(major, minor);
    for (i, &p) in verts.iter().enumerate() {
        let d = signed_distance_to_surface(surf, p).unwrap();
        assert!(d.abs() < 1e-9, "vert {i} off tube: {d:e}");
    }
    // Watertight/manifold by 3D position; the window survives as a boundary.
    let key = |p: Point3| {
        let a = p.as_array();
        [
            (a[0] * 1e7).round() as i64,
            (a[1] * 1e7).round() as i64,
            (a[2] * 1e7).round() as i64,
        ]
    };
    let mut edges: BTreeMap<([i64; 3], [i64; 3]), u32> = BTreeMap::new();
    for t in &tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(verts[a as usize]), key(verts[b as usize]));
            let e = if ka < kb { (ka, kb) } else { (kb, ka) };
            *edges.entry(e).or_insert(0) += 1;
        }
    }
    assert!(
        edges.values().all(|&c| c == 1 || c == 2),
        "non-manifold edge (seam or window split)"
    );
    let boundary_edges = edges.values().filter(|&&c| c == 1).count();
    assert_eq!(
        boundary_edges,
        2 * nu + win_edges,
        "expected 2 meridian circles + 1 window, got {boundary_edges}"
    );
    // Area = band − window (a split window would leak area or fold).
    let band_area = minor * (v1 - v0) * std::f64::consts::TAU * major;
    let win_area = minor * (wv1 - wv0) * (major * (wu1 - wu0) + minor * (wu1.sin() - wu0.sin()));
    let analytic = band_area - win_area;
    let mut area = 0.0;
    for t in &tris {
        let a = verts[t[0] as usize].as_array();
        let b = verts[t[1] as usize].as_array();
        let c = verts[t[2] as usize].as_array();
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cr = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        area += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
    }
    assert!(
        area <= analytic * (1.0 + 1e-6) && area >= analytic * 0.97,
        "seam-window band area {area} vs analytic {analytic}"
    );
}

fn torus(major: f64, minor: f64) -> Surface {
    Surface::Torus {
        center: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        major_radius: major,
        minor_radius: minor,
    }
}

#[test]
fn newton_relocates_onto_torus_plane_intersection() {
    // Torus R=3 r=1 axis +z; oblique-ish plane x = 3.4 (a spiric section,
    // NOT a conic). A chord point near the curve must land on BOTH surfaces.
    let t = torus(3.0, 1.0);
    let plane = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -3.4,
    };
    // Seed: a torus surface point near x≈3.4, nudged off both surfaces.
    let (u, v) = (0.7_f64, 0.15_f64);
    let rad = 3.0 + 1.0 * u.cos();
    let seed = Point3::new(
        rad * v.cos() + 0.03,
        rad * v.sin() - 0.02,
        1.0 * u.sin() + 0.04,
    );
    let relocated = relocate_onto_implicit_pair(seed, t, plane).expect("converges");
    let ft = signed_distance_to_surface(t, relocated).unwrap();
    let fp = signed_distance_to_surface(plane, relocated).unwrap();
    assert!(ft.abs() <= cad_primitives::TAU_MODEL, "off torus: {ft:e}");
    assert!(fp.abs() <= cad_primitives::TAU_MODEL, "off plane: {fp:e}");
}

#[test]
fn newton_relocates_onto_torus_cylinder_intersection() {
    let t = torus(3.0, 1.0);
    // Cylinder coaxial-offset: axis ∥ +y through (3,0,0), radius 0.6 — cuts
    // the tube near θ=0 in a degree-4 curve.
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(3.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 1.0, 0.0),
        radius: 0.6,
    };
    let seed = Point3::new(3.5, 0.1, 0.45);
    let r = relocate_onto_implicit_pair(seed, t, cyl).expect("converges");
    assert!(signed_distance_to_surface(t, r).unwrap().abs() <= cad_primitives::TAU_MODEL);
    assert!(signed_distance_to_surface(cyl, r).unwrap().abs() <= cad_primitives::TAU_MODEL);
}

#[test]
fn newton_relocates_onto_torus_torus_intersection() {
    // M5 #172 (R0096 class): TWO tori in general position — degree-8 curve,
    // no closed form; the implicit-pair Newton is the paper's procedural
    // relocation. Torus A: axis +z, center origin. Torus B: axis +x, center
    // (0.5, 0, 0) (offset breaks the tangential symmetry of the coaxial-
    // perpendicular pair). Both R=3 r=1; they intersect near the +y side.
    let ta = torus(3.0, 1.0);
    let tb = Surface::Torus {
        center: Point3::new(0.5, 0.0, 0.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 3.0,
        minor_radius: 1.0,
    };
    // Seed: an A-surface point near B's zero set (u≈0.65, v=π/2), nudged off.
    let seed = Point3::new(0.02, 3.82, 0.57);
    let r = relocate_onto_implicit_pair(seed, ta, tb).expect("converges");
    let fa = signed_distance_to_surface(ta, r).unwrap();
    let fb = signed_distance_to_surface(tb, r).unwrap();
    assert!(fa.abs() <= cad_primitives::TAU_MODEL, "off torus A: {fa:e}");
    assert!(fb.abs() <= cad_primitives::TAU_MODEL, "off torus B: {fb:e}");
    // The move is a chord-scale correction, not a jump to a far branch.
    let d = {
        let (s, q) = (seed.as_array(), r.as_array());
        ((q[0] - s[0]).powi(2) + (q[1] - s[1]).powi(2) + (q[2] - s[2]).powi(2)).sqrt()
    };
    assert!(d < 0.5, "relocation jumped {d:e} — wrong branch");
}

#[test]
fn newton_relocates_onto_torus_torus_plane_junction() {
    // M5 #172 (R0096 v7/v18 class): a torus×torus lateral curve meeting a
    // cutting plane — the 3-surface junction resolved by the triple Newton.
    let ta = torus(3.0, 1.0);
    let tb = Surface::Torus {
        center: Point3::new(0.5, 0.0, 0.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0),
        major_radius: 3.0,
        minor_radius: 1.0,
    };
    let plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -0.5,
    };
    let seed = Point3::new(0.02, 3.83, 0.52);
    let j = relocate_onto_implicit_triple(seed, ta, tb, plane).expect("converges");
    for (name, s) in [("torus A", ta), ("torus B", tb), ("plane", plane)] {
        let f = signed_distance_to_surface(s, j).unwrap();
        assert!(f.abs() <= cad_primitives::TAU_MODEL, "off {name}: {f:e}");
    }
}

#[test]
fn newton_stops_on_coincident_tori() {
    // Identical tori: normals parallel everywhere ⇒ rank-deficient pair ⇒
    // the tangential gate must REFUSE (there is no 1D curve to land on).
    let t = torus(3.0, 1.0);
    let seed = Point3::new(4.0, 0.0, 0.01);
    assert!(
        relocate_onto_implicit_pair(seed, t, t).is_none(),
        "coincident tori ⇒ STOP, not a fabricated curve"
    );
}

#[test]
fn newton_stops_when_there_is_no_intersection() {
    // Plane x = 10 lies entirely outside the torus (max x = R+r = 4): no
    // common zero, so the relocation must REFUSE (no curve to land on)
    // rather than wander to a wrong point.
    let t = torus(3.0, 1.0);
    let far = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -10.0,
    };
    let seed = Point3::new(3.5, 0.0, 0.2);
    assert!(
        relocate_onto_implicit_pair(seed, t, far).is_none(),
        "no intersection ⇒ STOP, not a guessed relocation"
    );
}
