pub mod chamfer;
pub mod extrude;
pub mod feature;
pub mod fillet;
pub mod loft;
pub mod revolve;
pub mod sweep;

use std::fmt;

/// Structured error type for geometry operations.
#[derive(Debug, Clone)]
pub enum OperationError {
    /// Profile has too few points for the operation.
    InsufficientProfile {
        required: usize,
        provided: usize,
    },
    /// Distance / height / radius is zero or negative.
    InvalidDimension {
        parameter: &'static str,
        value: f64,
    },
    /// Direction vector has zero length.
    ZeroDirection,
    /// The specified edge was not found on the solid.
    EdgeNotFound,
    /// Too few segments for the operation.
    InsufficientSegments {
        required: usize,
        provided: usize,
    },
    /// Loft profiles have different vertex counts.
    ProfileMismatch {
        bottom_count: usize,
        top_count: usize,
    },
    /// Path has too few points for a sweep.
    InsufficientPath {
        required: usize,
        provided: usize,
    },
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientProfile { required, provided } => {
                write!(f, "profile has {provided} points, need at least {required}")
            }
            Self::InvalidDimension { parameter, value } => {
                write!(f, "{parameter} must be positive, got {value}")
            }
            Self::ZeroDirection => write!(f, "direction vector has zero length"),
            Self::EdgeNotFound => write!(f, "specified edge not found on solid"),
            Self::InsufficientSegments { required, provided } => {
                write!(f, "need at least {required} segments, got {provided}")
            }
            Self::ProfileMismatch { bottom_count, top_count } => {
                write!(f, "loft profiles have different vertex counts: bottom={bottom_count}, top={top_count}")
            }
            Self::InsufficientPath { required, provided } => {
                write!(f, "path has {provided} points, need at least {required}")
            }
        }
    }
}

impl std::error::Error for OperationError {}
