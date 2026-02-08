pub mod geometry;
pub mod topology;
pub mod boolean;
pub mod operations;
pub mod validation;
pub mod traits;

// Re-export key traits at crate root for convenience.
pub use boolean::BooleanEngine;
pub use boolean::DefaultBooleanEngine;
pub use geometry::{CurveEval, SurfaceEval};
pub use traits::{CurveValidation, SurfaceValidation, SolidVerification, DefaultSolidVerifier};

/// Global tolerance configuration for geometric comparisons.
#[derive(Debug, Clone, Copy)]
pub struct Tolerance {
    /// Points closer than this are considered coincident (meters).
    pub coincidence: f64,
    /// Angles smaller than this (radians) are considered zero.
    pub angular: f64,
    /// Parameter-space tolerance for curve/surface evaluations.
    pub parametric: f64,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            coincidence: 1e-7,
            angular: 1e-10,
            parametric: 1e-9,
        }
    }
}

impl Tolerance {
    pub fn points_coincident(&self, a: &geometry::point::Point3d, b: &geometry::point::Point3d) -> bool {
        a.distance_to(b) < self.coincidence
    }

    pub fn is_zero_length(&self, length: f64) -> bool {
        length.abs() < self.coincidence
    }

    pub fn is_zero_angle(&self, angle: f64) -> bool {
        angle.abs() < self.angular
    }
}

/// Thread-local default tolerance.
pub fn default_tolerance() -> Tolerance {
    Tolerance::default()
}
