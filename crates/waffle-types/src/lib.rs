pub mod bspline;
pub mod gear;
pub mod gear_planetary;
pub mod geom_ref;
pub mod kernel;
pub mod profiles;
pub mod regions;
pub mod roles;
pub mod sketch;
pub mod topo;

pub use gear::{
    generate_gear_preview_polyline, generate_gear_profile, GearParams, GearProfileResult,
};
pub use gear_planetary::{
    carrier_radius, generate_planetary, generate_planetary_preview, mesh_external, ring_teeth,
    PlanetaryError, PlanetaryParams, PlanetaryResult,
};
pub use geom_ref::*;
pub use profiles::extract_profiles;
pub use regions::{compute_regions, Region, RegionEdge};
pub use roles::*;
pub use sketch::*;
pub use topo::*;
