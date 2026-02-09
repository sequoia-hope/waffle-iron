// Re-export all shared types from waffle-types
pub use waffle_types::*;

/// Internal classification of entity types for constraint dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    Point,
    Line,
    Circle,
    Arc,
}
