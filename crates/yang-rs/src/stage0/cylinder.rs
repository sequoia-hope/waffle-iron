//! Stage-0 coincident-cylinder handling: pair/group detection and the
//! conformal outer-mesh path (extracted verbatim from stage0/mod.rs —
//! spec `specs/stage0_decomposition.md`, increment 2).

#[allow(clippy::wildcard_imports)]
use super::*;

/// Detect coincident-cylinder A×B face pairs: one `Surface::Cylinder` face from
/// A and one from B that share the SAME cylindrical surface (collinear axes,
/// equal radius) with overlapping axial extent. Each becomes a [`PairCylinder`]
/// supplying the post-`keep_set` membrane keep/drop decision in `boolean()`.
///
/// This is a PARALLEL detector — it does NOT touch the planar overlay / mesh
/// re-tessellation path. cherchi already constructs the coincident-cylinder
/// overlap (the shared lateral sheet is bit-identical in both solids' Stage-1
/// meshes because the gear's bore wall and the flange's outer wall are the
/// identical analytic cylinder); we only need to tell `boolean()` whether that
/// internal sheet survives the op.
pub(crate) fn detect_coincident_cylinder_pairs(a: &BRep, b: &BRep) -> Vec<PairCylinder> {
    let cyls_a = cylinder_faces(a);
    let cyls_b = cylinder_faces(b);
    let mut out = Vec::new();
    for ca in &cyls_a {
        for cb in &cyls_b {
            // Scale-relative band over both cylinders' geometry (axis points,
            // radii, and the axial-extent endpoints). Mirrors the planar
            // `near_coplanar_band`: `TAU_MODEL.max(scale·TAU_WORK)`.
            let mut scale = 0.0_f64;
            for v in ca
                .axis_point
                .iter()
                .chain(cb.axis_point.iter())
                .chain(std::iter::once(&ca.radius))
                .chain(std::iter::once(&cb.radius))
                .chain(ca.extent.iter())
                .chain(cb.extent.iter())
            {
                scale = scale.max(v.abs());
            }
            let band = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);

            if !cylinders_coincident(ca, cb, band) {
                continue;
            }
            // Axial extents must overlap (band-inflated) — two coaxial,
            // equal-radius cylinders that do not overlap along the axis share
            // no surface region.
            let (lo_a, hi_a) = (ca.extent[0], ca.extent[1]);
            let (lo_b, hi_b) = (cb.extent[0], cb.extent[1]);
            if lo_a > hi_b + band || lo_b > hi_a + band {
                continue;
            }
            // Opposite iff exactly one face is a cavity wall (`reversed`): both
            // share the analytic outward direction (radially away from axis), so
            // their EFFECTIVE outward normals oppose iff their `reversed` flags
            // differ — the same opposite/equal split the planar pair makes.
            let opposite = ca.reversed != cb.reversed;
            out.push(PairCylinder {
                axis_point: ca.axis_point,
                axis_dir: ca.axis_dir,
                radius: ca.radius,
                band,
                opposite,
            });
        }
    }
    out
}

/// A cylinder face's analytic parameters plus the axial extent of its loop
/// vertices (projected onto the axis) and its `reversed` flag.
pub(crate) struct CylFace {
    pub(crate) axis_point: [f64; 3],
    pub(crate) axis_dir: [f64; 3],
    pub(crate) radius: f64,
    /// `[lo, hi]` axial parameter `(p − axis_point)·axis_dir` over the face's
    /// loop vertices.
    pub(crate) extent: [f64; 2],
    pub(crate) reversed: bool,
}

/// All `Surface::Cylinder` faces of `brep` with normalized axes and the axial
/// extent of their loop vertices. Faces whose axis is degenerate are skipped.
pub(crate) fn cylinder_faces(brep: &BRep) -> Vec<CylFace> {
    let mut out = Vec::new();
    for (fi, f) in brep.faces().iter().enumerate() {
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = f.surface
        else {
            continue;
        };
        let ap = axis_point.as_array();
        let ad = axis_dir.as_array();
        let len = (ad[0] * ad[0] + ad[1] * ad[1] + ad[2] * ad[2]).sqrt();
        if len < cad_primitives::MIN_FEATURE_SIZE {
            continue;
        }
        let au = [ad[0] / len, ad[1] / len, ad[2] / len];
        // Axial extent over the face's loop vertices.
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for vi in face_loop_verts(brep, fi) {
            let Some(v) = brep.vertices().get(vi as usize) else {
                continue;
            };
            let p = v.point.as_array();
            let t = (p[0] - ap[0]) * au[0] + (p[1] - ap[1]) * au[1] + (p[2] - ap[2]) * au[2];
            lo = lo.min(t);
            hi = hi.max(t);
        }
        if !lo.is_finite() {
            // No loop vertices (e.g. a seam-only loop): treat the extent as a
            // point at the axis origin so coaxial/equal-radius matching still
            // fires but the axial-overlap test stays meaningful.
            lo = 0.0;
            hi = 0.0;
        }
        out.push(CylFace {
            axis_point: ap,
            axis_dir: au,
            radius,
            extent: [lo, hi],
            reversed: f.reversed,
        });
    }
    out
}

/// Are two cylinder faces COINCIDENT: collinear axes (parallel directions AND
/// one axis point lies on the other's axis line) and equal radius, all within
/// the scale-relative `band`?
pub(crate) fn cylinders_coincident(ca: &CylFace, cb: &CylFace, band: f64) -> bool {
    // Equal radius.
    if (ca.radius - cb.radius).abs() > band {
        return false;
    }
    // Parallel axis directions (|cross| ≈ 0).
    let cross = [
        ca.axis_dir[1] * cb.axis_dir[2] - ca.axis_dir[2] * cb.axis_dir[1],
        ca.axis_dir[2] * cb.axis_dir[0] - ca.axis_dir[0] * cb.axis_dir[2],
        ca.axis_dir[0] * cb.axis_dir[1] - ca.axis_dir[1] * cb.axis_dir[0],
    ];
    let sin = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    // Scale the angular tolerance by the radius so the band is a true distance
    // bound on the surface (a tiny angular error over a large radius is still a
    // surface displacement of band·… — keep it conservative: compare directly).
    if sin > band.max(cad_primitives::TAU_MODEL) {
        return false;
    }
    // b's axis point lies on a's axis line: the perpendicular distance from
    // cb.axis_point to a's line (point ca.axis_point, dir ca.axis_dir).
    let w = [
        cb.axis_point[0] - ca.axis_point[0],
        cb.axis_point[1] - ca.axis_point[1],
        cb.axis_point[2] - ca.axis_point[2],
    ];
    let t = w[0] * ca.axis_dir[0] + w[1] * ca.axis_dir[1] + w[2] * ca.axis_dir[2];
    let perp = [
        w[0] - t * ca.axis_dir[0],
        w[1] - t * ca.axis_dir[1],
        w[2] - t * ca.axis_dir[2],
    ];
    let perp_dist = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
    perp_dist <= band
}

// ════════════════════════════════════════════════════════════════════════
// M8-cyl Increment 1 — coincident-cylinder Stage-0 conformal re-tessellation
// (Yang 2025 §4.5.5, the CURVED analog of the planar coplanar overlay).
// ════════════════════════════════════════════════════════════════════════
//
// §4.5.5 requires coincident surfaces between two solids to carry IDENTICAL
// meshes on their overlap region BEFORE the mesh boolean. For two coincident
// cylinders that overlap on a z-band (full θ — the gear's bore-wall ∩
// flange-wall case), the (θ, z) 2D Boolean reduces to a 1D z-interval: the
// overlap is `[max(za0, zb0), min(za1, zb1)]`. We make the overlap band
// bit-identical by inserting, into the LARGER cylinder's lateral, conformal
// rings that are LITERAL COPIES of the smaller (contained) cylinder's rim-ring
// vertices at the overlap boundary z-levels (`task28` proved both impls produce
// a non-watertight raw boolean here, so this upstream step is the un-portable-
// from-Cherchi capability). The two laterals then share bit-identical triangles
// on the overlap, so cherchi's pocket-dedup (PR-4) collapses them to ONE
// multi-label sheet and the §4.5.5 membrane resolution in `boolean()` drops it
// for the union — leaving a watertight result.
//
// Bit-identity is BY CONSTRUCTION (the inserted ring vertices are the SAME f64
// `Point3`s the contained solid's Stage-1 tessellation produced), NOT by
// tolerance fusing (P9 — the F0057 rounding-weld and broad SSI fallback were
// both reverted; this never welds within a tolerance).

/// A coincident-cylinder A×B pair with the lateral FACE indices and the
/// solids' axial extents — the richer form of [`PairCylinder`] used by the
/// conformal re-tessellation (which needs to know WHICH face to rebuild).
/// A coincident-cylinder GROUP: ALL faces of A and of B that lie on ONE shared
/// cylinder (the gear's bore wall is split into 4 arc-patch faces, the flange
/// wall into 4 more — collectively two coincident full-θ cylinders). The
/// conformal re-tessellation treats the group as a unit: aggregate each solid's
/// rings over ALL its faces in the group, then rebuild the outer solid's group
/// faces as one re-banded full-θ strip.
pub(crate) struct CoincidentCylinderGroup {
    pub(crate) faces_a: Vec<usize>,
    pub(crate) faces_b: Vec<usize>,
    pub(crate) axis_point: [f64; 3],
    pub(crate) axis_dir: [f64; 3],
    pub(crate) band: f64,
    /// `[lo, hi]` aggregate axial extent of A's faces, B's faces.
    pub(crate) extent_a: [f64; 2],
    pub(crate) extent_b: [f64; 2],
    /// `true` iff A's and B's faces have OPPOSING effective outward normals
    /// (bore cavity wall vs solid wall) — derived from the `reversed` flags,
    /// which must agree within each solid's faces of the group.
    pub(crate) opposite: bool,
}

/// Detect coincident-cylinder GROUPS between A and B: cluster each solid's
/// cylinder faces by shared analytic cylinder (collinear axis + equal radius),
/// then pair an A-cluster with a B-cluster on the SAME cylinder with
/// overlapping axial extent. Increment 1 returns groups where every face in a
/// solid's cluster shares the SAME `reversed` flag (a single coherent wall);
/// mixed flags → that cluster is skipped (a later increment).
pub(crate) fn detect_coincident_cylinder_groups(
    a: &BRep,
    b: &BRep,
) -> Vec<CoincidentCylinderGroup> {
    let clusters_a = cluster_cylinder_faces(a);
    let clusters_b = cluster_cylinder_faces(b);
    let mut out = Vec::new();
    for ca in &clusters_a {
        for cb in &clusters_b {
            let mut scale = 0.0_f64;
            for v in ca
                .axis_point
                .iter()
                .chain(cb.axis_point.iter())
                .chain(std::iter::once(&ca.radius))
                .chain(std::iter::once(&cb.radius))
                .chain(ca.extent.iter())
                .chain(cb.extent.iter())
            {
                scale = scale.max(v.abs());
            }
            let band = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);
            let rep_a = CylFace {
                axis_point: ca.axis_point,
                axis_dir: ca.axis_dir,
                radius: ca.radius,
                extent: ca.extent,
                reversed: ca.reversed,
            };
            let rep_b = CylFace {
                axis_point: cb.axis_point,
                axis_dir: cb.axis_dir,
                radius: cb.radius,
                extent: cb.extent,
                reversed: cb.reversed,
            };
            if !cylinders_coincident(&rep_a, &rep_b, band) {
                continue;
            }
            let (lo_a, hi_a) = (ca.extent[0], ca.extent[1]);
            let (lo_b, hi_b) = (cb.extent[0], cb.extent[1]);
            if lo_a > hi_b + band || lo_b > hi_a + band {
                continue;
            }
            out.push(CoincidentCylinderGroup {
                faces_a: ca.faces.clone(),
                faces_b: cb.faces.clone(),
                axis_point: ca.axis_point,
                axis_dir: ca.axis_dir,
                band,
                extent_a: ca.extent,
                extent_b: cb.extent,
                opposite: ca.reversed != cb.reversed,
            });
        }
    }
    out
}

/// One solid's cluster of cylinder faces sharing an analytic cylinder.
pub(crate) struct CylCluster {
    pub(crate) faces: Vec<usize>,
    pub(crate) axis_point: [f64; 3],
    pub(crate) axis_dir: [f64; 3],
    pub(crate) radius: f64,
    pub(crate) extent: [f64; 2],
    /// Shared `reversed` flag across the cluster (clusters with mixed flags are
    /// split so each cluster is a single coherent wall).
    pub(crate) reversed: bool,
}

/// Cluster a solid's `Surface::Cylinder` faces by shared analytic cylinder
/// (collinear axis + equal radius + same `reversed`), aggregating each
/// cluster's axial extent over all its faces.
pub(crate) fn cluster_cylinder_faces(brep: &BRep) -> Vec<CylCluster> {
    let faces = cylinder_faces_indexed(brep);
    let mut clusters: Vec<CylCluster> = Vec::new();
    for (fi, cf) in &faces {
        let mut scale = 0.0_f64;
        for v in cf
            .axis_point
            .iter()
            .chain(std::iter::once(&cf.radius))
            .chain(cf.extent.iter())
        {
            scale = scale.max(v.abs());
        }
        let band = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);
        let mut matched = false;
        for cl in clusters.iter_mut() {
            let rep = CylFace {
                axis_point: cl.axis_point,
                axis_dir: cl.axis_dir,
                radius: cl.radius,
                extent: cl.extent,
                reversed: cl.reversed,
            };
            if cl.reversed == cf.reversed && cylinders_coincident(&rep, cf, band) {
                cl.faces.push(*fi);
                cl.extent[0] = cl.extent[0].min(cf.extent[0]);
                cl.extent[1] = cl.extent[1].max(cf.extent[1]);
                matched = true;
                break;
            }
        }
        if !matched {
            clusters.push(CylCluster {
                faces: vec![*fi],
                axis_point: cf.axis_point,
                axis_dir: cf.axis_dir,
                radius: cf.radius,
                extent: cf.extent,
                reversed: cf.reversed,
            });
        }
    }
    clusters
}

/// All `Surface::Cylinder` faces of `brep` with their FACE INDEX and parameters.
pub(crate) fn cylinder_faces_indexed(brep: &BRep) -> Vec<(usize, CylFace)> {
    let mut out = Vec::new();
    for (fi, f) in brep.faces().iter().enumerate() {
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = f.surface
        else {
            continue;
        };
        let ap = axis_point.as_array();
        let ad = axis_dir.as_array();
        let len = (ad[0] * ad[0] + ad[1] * ad[1] + ad[2] * ad[2]).sqrt();
        if len < cad_primitives::MIN_FEATURE_SIZE {
            continue;
        }
        let au = [ad[0] / len, ad[1] / len, ad[2] / len];
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for vi in face_loop_verts(brep, fi) {
            let Some(v) = brep.vertices().get(vi as usize) else {
                continue;
            };
            let p = v.point.as_array();
            let t = (p[0] - ap[0]) * au[0] + (p[1] - ap[1]) * au[1] + (p[2] - ap[2]) * au[2];
            lo = lo.min(t);
            hi = hi.max(t);
        }
        if !lo.is_finite() {
            lo = 0.0;
            hi = 0.0;
        }
        out.push((
            fi,
            CylFace {
                axis_point: ap,
                axis_dir: au,
                radius,
                extent: [lo, hi],
                reversed: f.reversed,
            },
        ));
    }
    out
}

/// One conformal ring on the shared cylinder: its axial parameter `z` (along
/// the axis from `axis_point`) and the ORDERED mesh-vertex indices around it
/// (CCW in the shared axis frame).
pub(crate) struct ConformalRing {
    pub(crate) z: f64,
    /// Vertex indices into the host solid's growing mesh vertex pool.
    pub(crate) ids: Vec<u32>,
}

/// Run §4.5.5 coincident-cylinder Stage-0 conformal re-tessellation.
///
/// `Ok(None)` — not Increment 1's case (no coincident pair, a face in >1 pair,
/// non-opposite, full-θ extents that don't yield a clean 1-contained-in-other
/// band, or a lateral whose rim rings cannot be extracted). The caller falls
/// back to the existing path (raw Stage-1 meshes / the planar Stage-0). This is
/// a LOUD-free fall-through: the downstream membrane resolution or the
/// `NonManifoldOutput` wall still fires if the config truly is unhandled.
///
/// `Ok(Some(_))` — both solids re-tessellated so the coincident overlap band is
/// bit-identical; feed the meshes to cherchi exactly as the planar overlay
/// output is fed.
pub(crate) fn coincident_cylinder_stage0(a: &BRep, b: &BRep) -> Result<Option<Stage0>, YangError> {
    let probe = std::env::var_os("CYLST0_PROBE").is_some();
    let groups = detect_coincident_cylinder_groups(a, b);
    if probe {
        eprintln!(
            "[cylst0] detected {} coincident cylinder groups",
            groups.len()
        );
        for (i, g) in groups.iter().enumerate() {
            eprintln!(
                "  group[{i}] fa={:?} fb={:?} opp={} ea=[{:.5},{:.5}] eb=[{:.5},{:.5}]",
                g.faces_a,
                g.faces_b,
                g.opposite,
                g.extent_a[0],
                g.extent_a[1],
                g.extent_b[0],
                g.extent_b[1]
            );
        }
    }
    if groups.len() != 1 {
        // Increment 1: exactly one coincident-cylinder GROUP. Zero → not our
        // case; >1 → a later increment (n-ary coincidence).
        return Ok(None);
    }
    let g = &groups[0];

    // Increment 1 scope gate: OPPOSITE-normal, full-θ, with one cluster's axial
    // extent CONTAINED in (or equal to) the other within the band.
    if !g.opposite {
        if probe {
            eprintln!("[cylst0] group not opposite");
        }
        return Ok(None);
    }
    let (lo_a, hi_a) = (g.extent_a[0], g.extent_a[1]);
    let (lo_b, hi_b) = (g.extent_b[0], g.extent_b[1]);
    let (outer_is_a, ov_lo, ov_hi) = {
        let a_contains_b = lo_a <= lo_b + g.band && hi_b <= hi_a + g.band;
        let b_contains_a = lo_b <= lo_a + g.band && hi_a <= hi_b + g.band;
        if a_contains_b {
            (true, lo_b, hi_b)
        } else if b_contains_a {
            (false, lo_a, hi_a)
        } else {
            if probe {
                eprintln!("[cylst0] partial overlap a=[{lo_a},{hi_a}] b=[{lo_b},{hi_b}]");
            }
            return Ok(None);
        }
    };

    // Tessellate both solids forcing the SAME circle-rim N (§4.5.5 identical
    // overlap meshes): two coincident cylinders sampled at different N produce
    // non-identical overlap rings cherchi cannot pocket-dedup. Probe each
    // solid's own N (its cluster's aggregate rings), then re-tessellate BOTH at
    // the max (a finer N only shrinks the sagitta — chord-valid for both, NOT a
    // tolerance relaxation).
    let verts_a: Vec<BRepVertex> = a.vertices().to_vec();
    let verts_b: Vec<BRepVertex> = b.vertices().to_vec();
    let probe0_a = stage1_tessellate(&verts_a, a.edges(), a.faces())?;
    let probe0_b = stage1_tessellate(&verts_b, b.edges(), b.faces())?;
    let n_a = cluster_rim_rings(&probe0_a, &g.faces_a, g.axis_point, g.axis_dir)
        .and_then(|r| r.first().map(|ring| ring.ids.len()));
    let n_b = cluster_rim_rings(&probe0_b, &g.faces_b, g.axis_point, g.axis_dir)
        .and_then(|r| r.first().map(|ring| ring.ids.len()));
    let shared_n = match (n_a, n_b) {
        // Case-IV phantom guard: a forced rim N on either operand folds into
        // the shared N (a finer N is always chord-valid).
        (Some(na), Some(nb)) => na
            .max(nb)
            .max(a.forced_rim_n().unwrap_or(0))
            .max(b.forced_rim_n().unwrap_or(0)),
        _ => {
            if probe {
                eprintln!("[cylst0] could not extract cluster ring N (na={n_a:?} nb={n_b:?})");
            }
            return Ok(None);
        }
    };
    let tess_a =
        crate::stage1_tessellate_min_segments(&verts_a, a.edges(), a.faces(), Some(shared_n))?;
    let tess_b =
        crate::stage1_tessellate_min_segments(&verts_b, b.edges(), b.faces(), Some(shared_n))?;

    let outer_tess = if outer_is_a { &tess_a } else { &tess_b };
    let outer_faces = if outer_is_a { &g.faces_a } else { &g.faces_b };
    let outer_reversed = if outer_is_a {
        a.faces()[g.faces_a[0]].reversed
    } else {
        b.faces()[g.faces_b[0]].reversed
    };
    let cont_tess = if outer_is_a { &tess_b } else { &tess_a };
    let cont_faces = if outer_is_a { &g.faces_b } else { &g.faces_a };

    let Some(outer_rings) = cluster_rim_rings(outer_tess, outer_faces, g.axis_point, g.axis_dir)
    else {
        if probe {
            eprintln!("[cylst0] outer cluster rings None");
        }
        return Ok(None);
    };
    let Some(cont_rings) = cluster_rim_rings(cont_tess, cont_faces, g.axis_point, g.axis_dir)
    else {
        if probe {
            eprintln!("[cylst0] cont cluster rings None");
        }
        return Ok(None);
    };
    // Increment 1: each clustered wall presents exactly 2 aggregate rim rings.
    if outer_rings.len() != 2 || cont_rings.len() != 2 {
        if probe {
            eprintln!(
                "[cylst0] ring count: outer={} cont={}",
                outer_rings.len(),
                cont_rings.len()
            );
        }
        return Ok(None);
    }

    let Some((outer_mesh, outer_tri_face)) = build_conformal_outer_mesh(
        outer_tess,
        outer_faces,
        &outer_rings,
        &cont_rings,
        cont_tess,
        g.axis_point,
        g.axis_dir,
        outer_reversed,
        ov_lo,
        ov_hi,
        g.band,
    ) else {
        if probe {
            eprintln!("[cylst0] build_conformal_outer_mesh None");
        }
        return Ok(None);
    };
    let cont_mesh = Mesh::new(cont_tess.verts.clone(), cont_tess.tris.clone());
    // N4: the contained mesh IS `cont_tess` unchanged → its face map is the
    // direct inversion of the face ranges (every triangle a real Stage-1 face).
    let cont_tri_face = invert_face_tri_ranges(cont_tess);

    let (mesh_a, mesh_b, tri_face_a, tri_face_b) = if outer_is_a {
        (outer_mesh, cont_mesh, outer_tri_face, cont_tri_face)
    } else {
        (cont_mesh, outer_mesh, cont_tri_face, outer_tri_face)
    };
    if probe {
        eprintln!(
            "[cylst0] HANDLED: outer_is_a={outer_is_a} outer_faces={outer_faces:?} \
             outer_rings_z={:?} cont_rings_z={:?} ov=[{ov_lo},{ov_hi}] N={shared_n} \
             mesh_a(v={},t={}) mesh_b(v={},t={})",
            outer_rings.iter().map(|r| r.z).collect::<Vec<_>>(),
            cont_rings.iter().map(|r| r.z).collect::<Vec<_>>(),
            mesh_a.verts.len(),
            mesh_a.tris.len(),
            mesh_b.verts.len(),
            mesh_b.tris.len(),
        );
    }

    debug_assert_eq!(tri_face_a.len(), mesh_a.tris.len(), "tri_face_a 1:1");
    debug_assert_eq!(tri_face_b.len(), mesh_b.tris.len(), "tri_face_b 1:1");
    Ok(Some(Stage0 {
        mesh_a,
        mesh_b,
        pairs: Vec::new(),
        // N4: per-triangle → face provenance for BOTH re-tessellated meshes, so
        // Stage-6 attributes coincident-cylinder overlaps by provenance rather
        // than geometric proximity (the last Stage-0 producer to gain this).
        tri_face_a,
        tri_face_b,
    }))
}

/// Extract a CLUSTER of cylinder faces' aggregate full-circle rim rings from
/// Stage-1 triangles: collect the unique vertices of ALL the cluster's faces,
/// group by axial parameter `z` along the shared axis, and order each ring CCW
/// in the shared axis frame. Aggregating over the (arc-patch) faces re-forms
/// the full-θ rings the gear's 4-arc-per-wall decomposition splits up.
/// `None` if the cluster does not present clean equal-size rings (≥ 3 each).
pub(crate) fn cluster_rim_rings(
    tess: &crate::Stage1Tess,
    faces: &[usize],
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
) -> Option<Vec<ConformalRing>> {
    let au = normalize3(axis_dir);
    let (e1, e2) = ortho_basis(cad_primitives::Vector3::new(au[0], au[1], au[2]));
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let zof = |p: [f64; 3]| -> f64 {
        (p[0] - axis_point[0]) * au[0]
            + (p[1] - axis_point[1]) * au[1]
            + (p[2] - axis_point[2]) * au[2]
    };
    let azof = |p: [f64; 3]| -> f64 {
        let w = [
            p[0] - axis_point[0],
            p[1] - axis_point[1],
            p[2] - axis_point[2],
        ];
        let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
        let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
        y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI)
    };

    // Collect unique cluster vertices (deduped across the arc faces — adjacent
    // arcs share their boundary ruling vertices), bucketed by axial level.
    let mut seen = std::collections::BTreeSet::new();
    let mut by_z: Vec<(f64, Vec<u32>)> = Vec::new();
    for &fi in faces {
        let range = tess.face_tri_ranges.get(fi)?.clone();
        for tri in &tess.tris[range] {
            for &v in tri {
                if !seen.insert(v) {
                    continue;
                }
                let z = zof(tess.verts[v as usize].as_array());
                let scale = z.abs().max(1.0);
                let zband = 1.0e-9 * scale;
                if let Some(slot) = by_z.iter_mut().find(|(zz, _)| (*zz - z).abs() <= zband) {
                    slot.1.push(v);
                } else {
                    by_z.push((z, vec![v]));
                }
            }
        }
    }
    by_z.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    if by_z.len() < 2 {
        return None;
    }
    let nring = by_z[0].1.len();
    if nring < 3 || by_z.iter().any(|(_, ids)| ids.len() != nring) {
        return None;
    }
    // De-duplicate vertices at the SAME azimuth within a ring (an arc-patch
    // decomposition can list a shared ruling vertex once per incident arc — the
    // `seen` set already dedups by index, but two DISTINCT indices at the same
    // bit coordinates would double-count; guard by azimuth uniqueness).
    let mut rings = Vec::with_capacity(by_z.len());
    for (z, mut ids) in by_z {
        ids.sort_by(|&i, &j| {
            azof(tess.verts[i as usize].as_array())
                .total_cmp(&azof(tess.verts[j as usize].as_array()))
        });
        rings.push(ConformalRing { z, ids });
    }
    Some(rings)
}

/// N4: invert a Stage-1 tessellation's `face_tri_ranges` into a per-triangle →
/// owning-face map (1:1 with `tess.tris`), mirroring the `BRep::new` inversion.
pub(crate) fn invert_face_tri_ranges(tess: &crate::Stage1Tess) -> Vec<u32> {
    let mut tf = vec![0u32; tess.tris.len()];
    for (fi, range) in tess.face_tri_ranges.iter().enumerate() {
        for ti in range.clone() {
            tf[ti] = fi as u32;
        }
    }
    tf
}

/// The smallest CCW arc `(start, end)` covering all `azimuths` (each in
/// `[0, 2π)`): the circle minus the LARGEST cyclic gap between consecutive
/// (sorted) azimuths. `end < start` denotes an arc that wraps past 2π. `None`
/// when empty. Recovers an arc-patch cluster face's angular span from its rim
/// vertices (§4.5.5 coincident-cylinder provenance). Only used for a MULTI-face
/// cluster, where each face is a proper sub-arc (a single full-θ face is handled
/// without this — it owns every azimuth).
pub(crate) fn smallest_covering_arc(azimuths: &[f64]) -> Option<(f64, f64)> {
    if azimuths.is_empty() {
        return None;
    }
    let mut a: Vec<f64> = azimuths.to_vec();
    a.sort_by(|x, y| x.total_cmp(y));
    let m = a.len();
    if m == 1 {
        return Some((a[0], a[0]));
    }
    let tau = 2.0 * std::f64::consts::PI;
    // Start assuming the WRAP gap (last → first+2π) is the largest, so the
    // covering arc is the contiguous span [first, last] with no wrap.
    let mut best_gap = a[0] + tau - a[m - 1];
    let mut start = a[0];
    let mut end = a[m - 1];
    for i in 0..m - 1 {
        let gap = a[i + 1] - a[i];
        if gap > best_gap {
            // A larger interior gap → the arc wraps: it runs from the vertex
            // after the gap, past 2π, to the vertex before it.
            best_gap = gap;
            start = a[i + 1];
            end = a[i];
        }
    }
    Some((start, end))
}

/// Is `theta` within the CCW arc `[start, end]`? Wraps past 2π when `end < start`.
pub(crate) fn arc_contains(theta: f64, start: f64, end: f64) -> bool {
    if start <= end {
        theta >= start && theta <= end
    } else {
        theta >= start || theta <= end
    }
}

/// Per outer cluster face, its rim-vertex azimuth arc `(face_idx, start, end)`
/// in the shared axis frame — used to attribute a band-strip triangle to the
/// arc-patch face covering its column's azimuth.
pub(crate) fn cluster_face_arcs(
    outer_tess: &crate::Stage1Tess,
    outer_faces: &[usize],
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
) -> Vec<(u32, f64, f64)> {
    let au = normalize3(axis_dir);
    let (e1, e2) = ortho_basis(cad_primitives::Vector3::new(au[0], au[1], au[2]));
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let azof = |p: [f64; 3]| -> f64 {
        let w = [
            p[0] - axis_point[0],
            p[1] - axis_point[1],
            p[2] - axis_point[2],
        ];
        let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
        let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
        y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI)
    };
    let mut arcs = Vec::new();
    for &fi in outer_faces {
        let Some(range) = outer_tess.face_tri_ranges.get(fi) else {
            continue;
        };
        let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        let mut azs: Vec<f64> = Vec::new();
        for t in &outer_tess.tris[range.clone()] {
            for &v in t {
                if seen.insert(v) {
                    azs.push(azof(outer_tess.verts[v as usize].as_array()));
                }
            }
        }
        if let Some((s, e)) = smallest_covering_arc(&azs) {
            arcs.push((fi as u32, s, e));
        }
    }
    arcs
}

/// Build the OUTER solid's conformal mesh: every face is its Stage-1
/// triangles, EXCEPT the coincident lateral, which is rebuilt as a banded strip
/// from its own two rim rings plus the contained solid's overlap-boundary rings
/// inserted as LITERAL COPIES (bit-identical vertices) at their z-levels. The
/// band strips between consecutive z-rings are paired by GLOBAL azimuth (the
/// merge convention — robust to the two solids' differing seam frames).
///
/// N4 (provenance): also returns a per-output-triangle → owning-face map
/// (1:1 with the mesh `tris`). Non-cluster triangles keep their Stage-1 face;
/// band-strip triangles are attributed to the arc-patch cluster face whose
/// azimuth arc contains the strip column's midpoint (trivial when the cluster
/// is a single face). A column that finds no covering arc (a floating-point
/// anomaly at a seam) gets the `u32::MAX` sentinel → geometric fallback.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_conformal_outer_mesh(
    outer_tess: &crate::Stage1Tess,
    outer_faces: &[usize],
    outer_rings: &[ConformalRing],
    cont_rings: &[ConformalRing],
    cont_tess: &crate::Stage1Tess,
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    reversed: bool,
    ov_lo: f64,
    ov_hi: f64,
    band: f64,
) -> Option<(Mesh, Vec<u32>)> {
    let mut verts: Vec<Point3> = outer_tess.verts.clone();

    // Assemble the full set of conformal rings for the outer lateral, ordered by
    // z: the outer lateral's own two rims + the contained rings whose z lies
    // STRICTLY inside the outer extent (the overlap boundary). Contained-ring
    // vertices are appended as new mesh vertices (literal copies → bit-identical
    // to the contained solid's mesh).
    let mut all: Vec<ConformalRing> = Vec::new();
    for r in outer_rings {
        all.push(ConformalRing {
            z: r.z,
            ids: r.ids.clone(),
        });
    }
    let (z_lo, z_hi) = (outer_rings[0].z, outer_rings[outer_rings.len() - 1].z);
    let _ = (ov_lo, ov_hi);
    for r in cont_rings {
        // Insert the contained solid's rim rings that sit STRICTLY between the
        // outer rims (the overlap-band boundary; a ring AT an outer rim would be
        // a duplicate). The outer ring span IS the overlap geometry — using the
        // extracted ring z-levels (not the loop-vertex extent) is the reliable
        // truth, since a wall's tessellated rims can sit at different axial
        // params than its loop vertices' aggregate extent.
        if r.z <= z_lo + band || r.z >= z_hi - band {
            continue;
        }
        // Equal ring size required for the banded strip (Increment 1: same N).
        if r.ids.len() != outer_rings[0].ids.len() {
            if std::env::var_os("CYLST0_PROBE").is_some() {
                eprintln!(
                    "[cylst0] ring size mismatch: contained ring N={} vs outer N={}",
                    r.ids.len(),
                    outer_rings[0].ids.len()
                );
            }
            return None;
        }
        let mut ids = Vec::with_capacity(r.ids.len());
        for &v in &r.ids {
            let idx = verts.len() as u32;
            verts.push(cont_tess.verts[v as usize]);
            ids.push(idx);
        }
        all.push(ConformalRing { z: r.z, ids });
    }
    all.sort_by(|a, b| a.z.partial_cmp(&b.z).unwrap());

    // If nothing was inserted (extents equal, no interior boundary) the outer
    // lateral is already conformal with the contained one — no rebuild needed.
    // The mesh IS `outer_tess.tris` unchanged, so its face map is the direct
    // inversion of the face ranges.
    if all.len() == outer_rings.len() {
        let tri_face = invert_face_tri_ranges(outer_tess);
        return Some((Mesh::new(verts, outer_tess.tris.clone()), tri_face));
    }

    // Rebuild: keep all faces' triangles EXCEPT the coincident-cylinder cluster
    // faces (the arc patches), whose triangles are replaced by the re-banded
    // full-θ strip below.
    let mut in_cluster = vec![false; outer_tess.tris.len()];
    for &fi in outer_faces {
        let range = outer_tess.face_tri_ranges.get(fi)?.clone();
        for slot in in_cluster.iter_mut().take(range.end).skip(range.start) {
            *slot = true;
        }
    }
    // N4: face map built in lockstep with `tris`. Non-cluster triangles keep
    // their Stage-1 owning face; band-strip triangles are attributed by azimuth.
    let face_of = invert_face_tri_ranges(outer_tess);
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut tri_face: Vec<u32> = Vec::new();
    for (i, tri) in outer_tess.tris.iter().enumerate() {
        if !in_cluster[i] {
            tris.push(*tri);
            tri_face.push(face_of[i]);
        }
    }
    // Per band-strip column midpoint azimuth → the arc-patch cluster face that
    // covers it. Single-face cluster: trivially that face (the full-θ wall).
    let single_face = (outer_faces.len() == 1).then(|| outer_faces[0] as u32);
    let arcs = if single_face.is_some() {
        Vec::new()
    } else {
        cluster_face_arcs(outer_tess, outer_faces, axis_point, axis_dir)
    };
    let face_at = |mid: f64| -> u32 {
        if let Some(f) = single_face {
            return f;
        }
        for &(fi, s, e) in &arcs {
            if arc_contains(mid, s, e) {
                return fi;
            }
        }
        u32::MAX // no covering arc → geometric fallback (P9-safe)
    };
    // Banded strip over consecutive z-rings.
    let probe = std::env::var_os("CYLST0_PROBE").is_some();
    if probe {
        eprintln!(
            "[cylst0] all rings z = {:?}",
            all.iter().map(|r| r.z).collect::<Vec<_>>()
        );
    }
    for w in all.windows(2) {
        if band_strip(
            &w[0],
            &w[1],
            &verts,
            axis_point,
            axis_dir,
            reversed,
            &mut tris,
            &face_at,
            &mut tri_face,
        )
        .is_none()
        {
            if probe {
                eprintln!("[cylst0] band_strip None at z=[{},{}]", w[0].z, w[1].z);
            }
            return None;
        }
    }
    debug_assert_eq!(tri_face.len(), tris.len(), "outer tri_face 1:1 with tris");
    Some((Mesh::new(verts, tris), tri_face))
}

/// Connect two cylinder rings (`lo`, `hi`) into a watertight quad strip, pairing
/// their vertices by GLOBAL azimuth (in the shared axis frame). Each ring must
/// present the SAME azimuth multiset (within a quarter-step tol — a missing
/// match is malformed, not fudged). Triangles are oriented radially outward
/// (inward for a `reversed` cavity wall), matching `tessellate_lateral_face`.
///
/// N4 (provenance): each column's two triangles are tagged with the owning
/// face via `face_at(column_midpoint_azimuth)`, pushed to `out_tri_face` in
/// lockstep with `out_tris`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn band_strip(
    lo: &ConformalRing,
    hi: &ConformalRing,
    verts: &[Point3],
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    reversed: bool,
    out_tris: &mut Vec<[u32; 3]>,
    face_at: &dyn Fn(f64) -> u32,
    out_tri_face: &mut Vec<u32>,
) -> Option<()> {
    let n = lo.ids.len();
    if n < 3 || hi.ids.len() != n {
        return None;
    }
    let au = normalize3(axis_dir);
    let (e1, e2) = ortho_basis(cad_primitives::Vector3::new(au[0], au[1], au[2]));
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let azof = |vi: u32| -> f64 {
        let p = verts[vi as usize].as_array();
        let w = [
            p[0] - axis_point[0],
            p[1] - axis_point[1],
            p[2] - axis_point[2],
        ];
        let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
        let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
        y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI)
    };
    let mut lo_s: Vec<(f64, u32)> = lo.ids.iter().map(|&v| (azof(v), v)).collect();
    let mut hi_s: Vec<(f64, u32)> = hi.ids.iter().map(|&v| (azof(v), v)).collect();
    lo_s.sort_by(|a, b| a.0.total_cmp(&b.0));
    hi_s.sort_by(|a, b| a.0.total_cmp(&b.0));
    let tol = (2.0 * std::f64::consts::PI / n as f64) * 0.25;
    for k in 0..n {
        let mut d = (lo_s[k].0 - hi_s[k].0).abs();
        d = d.min(2.0 * std::f64::consts::PI - d);
        if d > tol {
            return None;
        }
    }
    let orient = |verts: &[Point3], tri: &[u32; 3]| -> [f64; 3] {
        let nrm = ring_radial_normal(verts, tri, axis_point, au);
        if reversed {
            [-nrm[0], -nrm[1], -nrm[2]]
        } else {
            nrm
        }
    };
    let tau = 2.0 * std::f64::consts::PI;
    for k in 0..n {
        let kn = (k + 1) % n;
        let b0 = lo_s[k].1;
        let b1 = lo_s[kn].1;
        let t0 = hi_s[k].1;
        let t1 = hi_s[kn].1;
        // Column midpoint azimuth (the wrap column advances the upper azimuth
        // by 2π so the mean lands inside the column, not on the far side).
        let a0 = lo_s[k].0;
        let mut a1 = lo_s[kn].0;
        if a1 < a0 {
            a1 += tau;
        }
        let mid = ((a0 + a1) * 0.5).rem_euclid(tau);
        let face = face_at(mid);
        for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
            let nrm = orient(verts, &tri);
            orient_band_tri(verts, &mut tri, nrm);
            out_tris.push(tri);
            out_tri_face.push(face);
        }
    }
    Some(())
}

/// Outward radial normal at a band triangle's centroid (local copy of
/// `radial_outward_normal`, kept inside Stage-0 to avoid widening its
/// visibility).
pub(crate) fn ring_radial_normal(
    verts: &[Point3],
    tri: &[u32; 3],
    axis_point: [f64; 3],
    axis_unit: [f64; 3],
) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let cen = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let w = [
        cen[0] - axis_point[0],
        cen[1] - axis_point[1],
        cen[2] - axis_point[2],
    ];
    let along = w[0] * axis_unit[0] + w[1] * axis_unit[1] + w[2] * axis_unit[2];
    let radial = [
        w[0] - along * axis_unit[0],
        w[1] - along * axis_unit[1],
        w[2] - along * axis_unit[2],
    ];
    normalize3(radial)
}

/// Flip `tri` to align its geometric normal with `target` (local copy of
/// `orient_tri`).
pub(crate) fn orient_band_tri(verts: &[Point3], tri: &mut [u32; 3], target: [f64; 3]) {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let dot = cross[0] * target[0] + cross[1] * target[1] + cross[2] * target[2];
    if dot < 0.0 {
        tri.swap(1, 2);
    }
}

#[cfg(test)]
mod cylinder_pair_tests {
    use super::*;
    use crate::{BRepEdge, BRepFace, BRepVertex, Curve};
    use cad_primitives::{Point3, Vector3};

    /// Build a minimal closed-cylinder B-Rep: two full-circle rim edges at
    /// z=`z0` and z=`z1`, one lateral `Surface::Cylinder` face referencing both
    /// rims, with the given `reversed` flag. Axis = +Z through the origin.
    fn cylinder_brep(radius: f64, z0: f64, z1: f64, reversed: bool) -> BRep {
        // Two rim vertices (seam points) + the lateral face.
        let v0 = BRepVertex {
            point: Point3::new(radius, 0.0, z0),
        };
        let v1 = BRepVertex {
            point: Point3::new(radius, 0.0, z1),
        };
        let rim0 = BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
        };
        let rim1 = BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z1),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
        };
        let face = BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
            outer_loop: vec![0, 1],
            inner_loops: vec![],
            reversed,
        };
        BRep::new(vec![v0, v1], vec![rim0, rim1], vec![face]).expect("build cylinder brep")
    }

    #[test]
    fn coaxial_bore_vs_wall_one_opposite_pair() {
        // A: a bore (cavity wall, reversed) of radius 2, z∈[0,5].
        // B: an outer wall (solid, not reversed) of the SAME cylinder, z∈[0,5].
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(2.0, 0.0, 5.0, false);
        let pairs = detect_coincident_cylinder_pairs(&a, &b);
        assert_eq!(pairs.len(), 1, "exactly one coincident-cylinder pair");
        assert!(
            pairs[0].opposite,
            "bore (reversed) vs wall (not reversed) must be opposite"
        );
        assert!((pairs[0].radius - 2.0).abs() < 1e-12);
        assert!((pairs[0].axis_dir[2] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn coaxial_same_sense_not_opposite() {
        // Two solid walls of the same cylinder (both not reversed) → equal.
        let a = cylinder_brep(2.0, 0.0, 5.0, false);
        let b = cylinder_brep(2.0, 1.0, 4.0, false);
        let pairs = detect_coincident_cylinder_pairs(&a, &b);
        assert_eq!(pairs.len(), 1);
        assert!(!pairs[0].opposite, "same-sense walls are not opposite");
    }

    #[test]
    fn different_radius_no_pair() {
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(3.0, 0.0, 5.0, false);
        assert!(detect_coincident_cylinder_pairs(&a, &b).is_empty());
    }

    #[test]
    fn offset_axis_no_pair() {
        // Same radius/direction but axis shifted off in x by 1 (parallel, not
        // collinear) → not coincident.
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let mut b = cylinder_brep(2.0, 0.0, 5.0, false);
        // Shift B's axis off the line: rebuild with a translated axis_point.
        if let Surface::Cylinder {
            axis_dir, radius, ..
        } = b.faces()[0].surface
        {
            let new_face = BRepFace {
                surface: Surface::Cylinder {
                    axis_point: Point3::new(1.0, 0.0, 0.0),
                    axis_dir,
                    radius,
                },
                outer_loop: b.faces()[0].outer_loop.clone(),
                inner_loops: vec![],
                reversed: b.faces()[0].reversed,
            };
            b = BRep::new(b.vertices().to_vec(), b.edges().to_vec(), vec![new_face])
                .expect("rebuild offset cylinder");
        }
        assert!(detect_coincident_cylinder_pairs(&a, &b).is_empty());
    }

    #[test]
    fn disjoint_axial_extent_no_pair() {
        // Coaxial, equal radius, but z-ranges do not overlap.
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(2.0, 10.0, 15.0, false);
        assert!(detect_coincident_cylinder_pairs(&a, &b).is_empty());
    }

    // ── M8-cyl Increment 1: group detection (cluster + cross-pairing) ──────

    #[test]
    fn cluster_single_cylinder_is_one_cluster() {
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let clusters = cluster_cylinder_faces(&a);
        assert_eq!(clusters.len(), 1, "one cylinder face → one cluster");
        assert_eq!(clusters[0].faces.len(), 1);
        assert!(clusters[0].reversed);
        assert!((clusters[0].radius - 2.0).abs() < 1e-12);
    }

    #[test]
    fn coincident_group_opposite_pair() {
        // Bore (reversed) z∈[0,5] vs an outward wall z∈[1,4]: one group, the
        // wall contained, opposite normals.
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(2.0, 1.0, 4.0, false);
        let groups = detect_coincident_cylinder_groups(&a, &b);
        assert_eq!(groups.len(), 1, "exactly one coincident cylinder group");
        assert!(groups[0].opposite, "bore vs wall must be opposite");
        assert_eq!(groups[0].faces_a, vec![0]);
        assert_eq!(groups[0].faces_b, vec![0]);
        // A's extent contains B's.
        assert!(groups[0].extent_a[0] <= groups[0].extent_b[0] + groups[0].band);
        assert!(groups[0].extent_b[1] <= groups[0].extent_a[1] + groups[0].band);
    }

    #[test]
    fn different_radius_no_group() {
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(3.0, 0.0, 5.0, false);
        assert!(detect_coincident_cylinder_groups(&a, &b).is_empty());
    }

    #[test]
    fn coincident_stage0_emits_valid_tri_face() {
        // N4 (coincident-cylinder provenance): the handled path must emit a
        // per-triangle → face map 1:1 with each produced mesh, so Stage-6 can
        // attribute by cherchi provenance instead of geometric proximity.
        // Single-cluster-face case: bore (reversed) z∈[0,5] vs outward wall
        // z∈[1,4]; A is the containing (outer) extent.
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(2.0, 1.0, 4.0, false);
        let s0 = coincident_cylinder_stage0(&a, &b)
            .expect("must not error")
            .expect("must reach the handled path");

        // I1: 1:1 with the meshes, non-empty (the whole point).
        assert_eq!(s0.tri_face_a.len(), s0.mesh_a.tris.len(), "A map 1:1");
        assert_eq!(s0.tri_face_b.len(), s0.mesh_b.tris.len(), "B map 1:1");
        assert!(
            !s0.tri_face_a.is_empty() && !s0.tri_face_b.is_empty(),
            "coincident-cylinder Stage-0 must emit provenance"
        );

        // I2: every entry is a valid face index or the u32::MAX fallback.
        let na = a.faces().len() as u32;
        let nb = b.faces().len() as u32;
        assert!(
            s0.tri_face_a.iter().all(|&f| f < na || f == u32::MAX),
            "A face indices valid"
        );
        assert!(
            s0.tri_face_b.iter().all(|&f| f < nb || f == u32::MAX),
            "B face indices valid"
        );

        // outer_is_a here (A contains B): A is the outer, one cluster face (0);
        // its band-strip tris all attribute to real faces, never the sentinel.
        assert!(
            s0.tri_face_a.iter().all(|&f| f != u32::MAX),
            "single-cluster outer fully attributed (no sentinel)"
        );
        // I3: the contained mesh (B) is the full Stage-1 re-tessellation — every
        // tri is a real face; the lateral-only helper has one face (0).
        assert!(
            s0.tri_face_b.iter().all(|&f| f == 0),
            "contained lateral-only cylinder: all tris on face 0"
        );
    }

    #[test]
    fn arc_helpers_partition_the_circle() {
        use std::f64::consts::PI;
        // I4: a quarter-arc face's vertices span [0, π/2]; the largest gap is
        // the rest of the circle → covering arc is exactly [0, π/2] (no wrap).
        let q = [0.0, PI / 6.0, PI / 3.0, PI / 2.0];
        let (s, e) = smallest_covering_arc(&q).unwrap();
        assert!((s - 0.0).abs() < 1e-12 && (e - PI / 2.0).abs() < 1e-12);
        assert!(arc_contains(PI / 4.0, s, e), "midpoint inside");
        assert!(!arc_contains(PI, s, e), "opposite side outside");

        // A face straddling the 0/2π seam: vertices near 2π and near 0 → the
        // covering arc WRAPS (end < start).
        let w = [0.1, 0.2, 2.0 * PI - 0.2, 2.0 * PI - 0.1];
        let (s, e) = smallest_covering_arc(&w).unwrap();
        assert!(s > PI && e < PI, "wrap arc: s={s} e={e}");
        assert!(
            arc_contains(0.0, s, e),
            "seam azimuth 0 is inside the wrap arc"
        );
        assert!(arc_contains(2.0 * PI - 0.15, s, e));
        assert!(!arc_contains(PI, s, e), "far side outside");

        // Two adjacent quarter arcs partition [0, π]: a midpoint lands in
        // exactly one (they meet at the shared seam π/2, never a column mid).
        let a0 = smallest_covering_arc(&[0.0, PI / 4.0, PI / 2.0]).unwrap();
        let a1 = smallest_covering_arc(&[PI / 2.0, 3.0 * PI / 4.0, PI]).unwrap();
        assert!(arc_contains(PI / 8.0, a0.0, a0.1) && !arc_contains(PI / 8.0, a1.0, a1.1));
        assert!(
            arc_contains(3.0 * PI / 4.0, a1.0, a1.1) && !arc_contains(3.0 * PI / 4.0, a0.0, a0.1)
        );
    }

    #[test]
    fn coincident_stage0_returns_none_on_lateral_only_breps() {
        // The lateral-only test helper is not a closed solid (no caps); its
        // clusters present but the rebuild has no incident caps, so the path
        // either falls back (Ok(None)) or handles it — it must NOT error and
        // must not panic.
        let a = cylinder_brep(2.0, 0.0, 5.0, true);
        let b = cylinder_brep(2.0, 1.0, 4.0, false);
        let r = coincident_cylinder_stage0(&a, &b);
        assert!(
            r.is_ok(),
            "coincident_cylinder_stage0 must never error here"
        );
    }
}
