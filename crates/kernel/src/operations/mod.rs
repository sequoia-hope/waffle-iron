pub mod chamfer;
pub mod extrude;
pub mod feature;
pub mod fillet;
pub mod revolve;

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
        }
    }
}

impl std::error::Error for OperationError {}
