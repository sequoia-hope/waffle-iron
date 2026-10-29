//! Harness on-surface oracle (remediation plan O1, review 2026-10-28 §2.1).
//!
//! `categorize` grades the render mesh; nothing in it put an output vertex on
//! its surface, so corpus case R0099 carried a loop vertex 7.3e-3 off its
//! surface and was graded CORRECT. This module reads the kernel's raw
//! boundary-to-surface distances on the EXACT B-Rep
//! (`KernelIntrospect::surface_conformance`: every loop vertex and edge-curve
//! sample against each adjacent face's surface) for every live solid.
//!
//! The band came from a census, never a guess. The record-only full corpus
//! run of 2026-10-29 (360C / 0W / 9E / 5EE / 0T held) read, over the 360
//! CORRECT cases: 357 at or below 3.5e-10 of the case scale, and THREE at
//! 1.4e-3 – 5.3e-3 (R0026, R0015, R0050), every one a `LineSegment` chord
//! bounding a torus or cylinder face — the curved-rim-shipped-as-chord path
//! of ledger N100 item 3 (`kernel-v2/src/recover.rs` bail). The band is the
//! paper's point tolerance d_p = 1e-7 (Yang 2025 §5,
//! `refs/text/yang2025_hybrid_boolean.txt:744`) taken relative to the case
//! scale: ≥ 300× above every clean reading, ≥ 14,000× below every defect.

use crate::workflow::ModelBuilder;

/// The verdict band, relative to the case scale (see the module docs).
pub const ON_SURFACE_BAND_REL: f64 = 1e-7;

/// The worst boundary-to-surface distance over every live solid of a model.
#[derive(Debug, Clone, PartialEq)]
pub struct OnSurfaceReading {
    /// Largest distance, meters (0 when nothing was measurable).
    pub worst: f64,
    /// `worst / scale`, the case-size-relative reading.
    pub rel: f64,
    /// Where the worst distance was measured.
    pub at: [f64; 3],
    /// Live solids measured.
    pub measured: usize,
    /// Live solids the kernel could not measure (mesh-backed imports).
    pub unmeasured: usize,
}

impl OnSurfaceReading {
    /// The record-only summary appended to a case's detail. Stable format:
    /// `on_surface: max <m> m, rel <r>, solids <n>[ (+<k> unmeasured)]`.
    pub fn summary(&self) -> String {
        let extra = if self.unmeasured > 0 {
            format!(" (+{} unmeasured)", self.unmeasured)
        } else {
            String::new()
        };
        format!(
            "on_surface: max {:.3e} m, rel {:.3e}, solids {}{extra} at ({:.6e}, {:.6e}, {:.6e})",
            self.worst, self.rel, self.measured, self.at[0], self.at[1], self.at[2]
        )
    }
}

impl OnSurfaceReading {
    /// Whether the reading is a geometric defect: outside the band, or not a
    /// finite number at all.
    pub fn is_defect(&self) -> bool {
        self.rel.is_nan() || self.rel > ON_SURFACE_BAND_REL
    }
}

/// Measure every live solid of `builder` (see the module docs). `scale` is
/// the case's characteristic size (`AssayMeta::scale`).
pub fn measure(builder: &ModelBuilder, scale: f64) -> OnSurfaceReading {
    let kernel = builder.kernel_ref().as_introspect();
    let mut r = OnSurfaceReading {
        worst: 0.0,
        rel: 0.0,
        at: [f64::NAN; 3],
        measured: 0,
        unmeasured: 0,
    };
    for h in builder.live_solid_handles() {
        match kernel.surface_conformance(&h) {
            Ok(c) => {
                r.measured += 1;
                if let Some(f) = c.worst() {
                    if f.max_distance > r.worst || r.at[0].is_nan() {
                        r.worst = f.max_distance;
                        r.at = f.at;
                    }
                }
            }
            Err(_) => r.unmeasured += 1,
        }
    }
    r.rel = if scale > 0.0 {
        r.worst / scale
    } else {
        r.worst
    };
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(rel: f64) -> OnSurfaceReading {
        OnSurfaceReading {
            worst: rel,
            rel,
            at: [0.0; 3],
            measured: 1,
            unmeasured: 0,
        }
    }

    #[test]
    fn the_band_separates_the_census_populations() {
        // Worst clean reading and smallest defect of the 2026-10-29 census.
        assert!(!reading(3.49e-10).is_defect());
        assert!(reading(1.439e-3).is_defect());
        assert!(!reading(0.0).is_defect());
        assert!(
            reading(f64::NAN).is_defect(),
            "an unmeasurable distance is not a pass"
        );
        assert!(reading(f64::INFINITY).is_defect());
    }
}
