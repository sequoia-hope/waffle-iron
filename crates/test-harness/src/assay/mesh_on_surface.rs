//! Harness render-mesh-on-surface oracle (remediation plan O3, review
//! 2026-10-28 §2.1).
//!
//! `assay::on_surface` (O1) reads the exact B-Rep's boundary against its
//! surfaces. A face's INTERIOR reaches the user only through its render
//! tessellation, and a triangle that spans the wrong region of its face (a
//! fan across a concavity, a parametric triangulation folded through a
//! seam) keeps every vertex on the surface while cutting through the solid.
//! This module reads the kernel's raw per-face distances of the render
//! mesh (`KernelIntrospect::mesh_conformance`): every vertex, and every
//! triangle centroid, against its face's surface, for every live solid.
//!
//! Two readings, because a plane's render mesh is exact and a curved face's
//! is a chord approximation:
//!
//! - **Planar faces** — vertex and centroid distance over the case scale,
//!   against O1's band (`on_surface::ON_SURFACE_BAND_REL`, Yang's d_p).
//! - **Curved faces** — vertex and centroid distance over the face's own
//!   render sagitta band `d_ε = rel · r` (`kernel_v2::RENDER_CHORD_TOLERANCE_REL`,
//!   `r` the surface's [`chord_radius`]), against [`CHORD_RATIO_BAND`].
//!
//! The bands came from a census, never a guess (record-only full corpus
//! run, 2026-10-30, 360C / 0W / 9E / 5EE / 0T held, 359 cases measured):
//! planar readings all ≤ 1.2e-12 of scale; curved ratios 210 cases in
//! [0.1, 1), 14 in [1, 2.92]. Every reading above 1 was anchored
//! (`TMP_O3_PROBE`, since removed) to one of two render-fidelity classes,
//! neither a geometry defect (O1 holds the exact B-Rep on its surfaces to
//! ≤ 3.5e-10 on the same cases): doubly-curved face interiors whose
//! centroids exceed d_ε (sphere ≤ 2.55 in C0067 / C0104, torus 1.05 in
//! R0028), and boundary vertices on a shared chord of a larger-radius edge
//! curve (small cylinders and cones, ≤ 2.92 in R0009). The synthetic
//! spanning triangle of kernel-v2's
//! `a_triangle_spanning_its_face_is_measured_at_its_centroid` reads ~10³.
//! The band, 10, sits 3.4× above the worst clean reading and ~100× below
//! that defect.
//!
//! [`chord_radius`]: waffle_types::kernel::FaceMeshConformance::chord_radius

use crate::workflow::ModelBuilder;

/// The kernel's documented render sagitta band, relative to a face's chord
/// radius (`d_ε = rel · r`, `kernel_v2::RENDER_CHORD_TOLERANCE_REL`). A
/// render vertex placed on a shared boundary chord (so two faces' meshes
/// conform) and every triangle centroid of a curved face legitimately sit
/// off the surface by up to this sagitta; nothing in a correct mesh sits
/// farther.
pub const RENDER_SAGITTA_REL: f64 = kernel_v2::RENDER_CHORD_TOLERANCE_REL;

/// The verdict band on [`MeshOnSurfaceReading::chord_ratio`] (see the
/// module docs for its census).
pub const CHORD_RATIO_BAND: f64 = 10.0;

/// The worst render-mesh-to-surface readings over every live solid.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshOnSurfaceReading {
    /// Planar faces: the largest vertex or centroid distance over the case
    /// scale (a plane's render mesh is exact).
    pub plane_rel: f64,
    /// Curved faces: the largest vertex or centroid distance over the face's
    /// own sagitta band `RENDER_SAGITTA_REL · chord_radius`.
    pub chord_ratio: f64,
    /// Where the worst `chord_ratio` was measured.
    pub chord_at: [f64; 3],
    /// Live solids measured.
    pub measured: usize,
    /// Live solids the kernel could not measure (mesh-backed imports).
    pub unmeasured: usize,
}

impl MeshOnSurfaceReading {
    /// The record-only summary appended to a case's detail. Stable format:
    /// `mesh_on_surface: plane rel <r>, chord ratio <q> at (x, y, z), solids <n>[ (+<k> unmeasured)]`.
    pub fn summary(&self) -> String {
        let extra = if self.unmeasured > 0 {
            format!(" (+{} unmeasured)", self.unmeasured)
        } else {
            String::new()
        };
        format!(
            "mesh_on_surface: plane rel {:.3e}, chord ratio {:.3e} at ({:.6e}, {:.6e}, {:.6e}), solids {}{extra}",
            self.plane_rel,
            self.chord_ratio,
            self.chord_at[0],
            self.chord_at[1],
            self.chord_at[2],
            self.measured
        )
    }
}

impl MeshOnSurfaceReading {
    /// Whether the reading is a geometric defect: a planar reading outside
    /// O1's band, a curved reading outside [`CHORD_RATIO_BAND`], or either
    /// not a finite number.
    pub fn is_defect(&self) -> bool {
        self.plane_rel.is_nan()
            || self.chord_ratio.is_nan()
            || self.plane_rel > crate::assay::on_surface::ON_SURFACE_BAND_REL
            || self.chord_ratio > CHORD_RATIO_BAND
    }
}

/// Measure every live solid of `builder` (see the module docs). `scale` is
/// the case's characteristic size (`AssayMeta::scale`).
pub fn measure(builder: &ModelBuilder, scale: f64) -> MeshOnSurfaceReading {
    let kernel = builder.kernel_ref().as_introspect();
    let mut r = MeshOnSurfaceReading {
        plane_rel: 0.0,
        chord_ratio: 0.0,
        chord_at: [f64::NAN; 3],
        measured: 0,
        unmeasured: 0,
    };
    // NaN never compares greater: a non-finite reading must still surface.
    let worse = |d: f64, max: f64| d.is_nan() || d > max;
    let fold = |d: f64| if d.is_nan() { f64::INFINITY } else { d };
    for h in builder.live_solid_handles() {
        match kernel.mesh_conformance(&h) {
            Ok(c) => {
                r.measured += 1;
                for f in &c.faces {
                    let (d, at) = if worse(f.max_vertex_distance, f.max_centroid_distance) {
                        (fold(f.max_vertex_distance), f.vertex_at)
                    } else {
                        (fold(f.max_centroid_distance), f.centroid_at)
                    };
                    if f.chord_radius > 0.0 {
                        let q = d / (RENDER_SAGITTA_REL * f.chord_radius);
                        if worse(q, r.chord_ratio) || r.chord_at[0].is_nan() {
                            r.chord_ratio = fold(q);
                            r.chord_at = at;
                        }
                    } else {
                        let q = if scale > 0.0 { d / scale } else { d };
                        if worse(q, r.plane_rel) {
                            r.plane_rel = fold(q);
                        }
                    }
                }
            }
            Err(_) => r.unmeasured += 1,
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(plane_rel: f64, chord_ratio: f64) -> MeshOnSurfaceReading {
        MeshOnSurfaceReading {
            plane_rel,
            chord_ratio,
            chord_at: [0.0; 3],
            measured: 1,
            unmeasured: 0,
        }
    }

    #[test]
    fn the_bands_separate_the_census_populations() {
        // Worst clean readings of the 2026-10-30 census.
        assert!(!reading(1.16e-12, 2.921).is_defect());
        assert!(!reading(0.0, 0.0).is_defect());
        // The synthetic spanning triangle reads ~10³ of its sagitta band.
        assert!(reading(0.0, 1.0e3).is_defect());
        assert!(
            reading(1e-6, 0.0).is_defect(),
            "a planar render vertex off its plane"
        );
        assert!(
            reading(f64::NAN, 0.0).is_defect(),
            "an unmeasurable distance is not a pass"
        );
        assert!(reading(0.0, f64::INFINITY).is_defect());
    }
}
