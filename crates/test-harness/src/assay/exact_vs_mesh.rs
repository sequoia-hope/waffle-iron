//! Exact-versus-mesh volume (remediation plan O2, review 2026-10-28 §2.5
//! test 3).
//!
//! The kernel integrates each solid's volume exactly from its B-Rep
//! (`KernelIntrospect::solid_volume`); the assay's volume checks read the
//! render mesh. The two must agree to within the tessellation's chord error.
//! A patch whose B-Rep and mesh disagree — an inverted patch too small for
//! the lattice's 0.5 % band, a face the tessellator dropped or doubled — shows
//! up as a gap between them that no mesh-only oracle can see.
//!
//! Phase one is RECORD-ONLY (appended to the detail; no verdict), so the band
//! comes from a census, exactly as for `assay::on_surface`.

use crate::workflow::ModelBuilder;

/// Summed exact and mesh volumes over every live solid.
#[derive(Debug, Clone, PartialEq)]
pub struct ExactVsMesh {
    /// Σ exact B-Rep volume, m³ (over measurable solids).
    pub exact: f64,
    /// Σ render-mesh signed volume of the same solids, m³.
    pub mesh: f64,
    /// `(mesh − exact) / |exact|`; NaN when nothing was measurable.
    pub rel: f64,
    /// Live solids the kernel could not integrate (mesh-backed imports).
    pub unmeasured: usize,
    /// Σ exact surface area of the measured solids, m².
    pub area: f64,
    /// Tessellation tolerance the mesh was made at, m.
    pub tol: f64,
}

impl ExactVsMesh {
    /// `|mesh − exact| / (tol · area)`: the volume gap measured against what
    /// the tessellation's own chord tolerance can explain (every mesh point
    /// within `tol` of the surface moves the enclosed volume by at most
    /// `tol · area` to first order). ≤ 1 is explained; above 1 is not.
    pub fn bound_ratio(&self) -> f64 {
        (self.mesh - self.exact).abs() / (self.tol * self.area)
    }
}

impl ExactVsMesh {
    /// The record-only summary. Stable format:
    /// `exact_vs_mesh: exact <v> mesh <v> rel <r>[ (+<k> unmeasured)]`.
    pub fn summary(&self) -> String {
        let extra = if self.unmeasured > 0 {
            format!(" (+{} unmeasured)", self.unmeasured)
        } else {
            String::new()
        };
        format!(
            "exact_vs_mesh: exact {:.9e} mesh {:.9e} rel {:+.3e} bound_ratio {:.3e}{extra}",
            self.exact,
            self.mesh,
            self.rel,
            self.bound_ratio()
        )
    }
}

/// Compare per live solid at tessellation tolerance `tol`.
pub fn measure(builder: &mut ModelBuilder, tol: f64) -> ExactVsMesh {
    let mut r = ExactVsMesh {
        exact: 0.0,
        mesh: 0.0,
        rel: f64::NAN,
        unmeasured: 0,
        area: 0.0,
        tol,
    };
    for h in builder.live_solid_handles() {
        let exact = builder.kernel_ref().as_introspect().solid_volume(&h);
        let area = builder.kernel_ref().as_introspect().solid_surface_area(&h);
        let mesh = builder.kernel_mut().tessellate(&h, tol);
        match (exact, area, mesh) {
            (Ok(v), Ok(a), Ok(m)) => {
                r.exact += v;
                r.area += a;
                r.mesh += crate::helpers::mesh_signed_volume(&m);
            }
            _ => r.unmeasured += 1,
        }
    }
    if r.exact != 0.0 {
        r.rel = (r.mesh - r.exact) / r.exact.abs();
    }
    r
}
