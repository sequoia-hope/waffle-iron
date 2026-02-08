# 10 — Assemblies: Interfaces

## Status: DEFERRED

Placeholder types for future design. These will be refined when assembly work begins.

## Planned Types

```rust
/// An assembly containing parts and sub-assemblies.
pub struct Assembly {
    pub id: uuid::Uuid,
    pub name: String,
    pub nodes: Vec<AssemblyNode>,
    pub mates: Vec<Mate>,
}

/// A node in the assembly tree.
pub enum AssemblyNode {
    Part {
        id: uuid::Uuid,
        /// Path to the .waffle file or embedded feature tree.
        source: PartSource,
        /// Transform from part-local to assembly coordinates.
        transform: [f64; 16],
    },
    SubAssembly {
        id: uuid::Uuid,
        assembly: Box<Assembly>,
        transform: [f64; 16],
    },
}

/// Where a part's definition comes from.
pub enum PartSource {
    /// Reference to an external .waffle file.
    FileReference { path: String },
    /// Embedded feature tree (for standalone assembly files).
    Embedded { tree: FeatureTree },
}

/// A mate constraint between two mate connectors.
pub struct Mate {
    pub id: uuid::Uuid,
    pub name: String,
    pub connector_a: MateConnector,
    pub connector_b: MateConnector,
    pub mate_type: MateType,
}

/// A coordinate frame attached to part geometry.
pub struct MateConnector {
    pub part_id: uuid::Uuid,
    pub geom_ref: GeomRef,
    pub origin: [f64; 3],
    pub z_axis: [f64; 3],
    pub x_axis: [f64; 3],
}

/// Type of mate constraint.
pub enum MateType {
    Fastened,
    Revolute,
    Slider,
    Cylindrical,
    Ball,
    Planar,
}
```

## Notes

- These types are preliminary. They will be refined based on implementation experience.
- truck-assembly provides positional grouping only — we build our own mate system.
