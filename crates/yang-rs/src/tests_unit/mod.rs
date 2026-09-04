//! yang-rs in-crate unit tests (moved verbatim from lib.rs `mod tests`
//! — spec `specs/yang_rs_lib_decomposition.md`, increment 10). Shared
//! fixtures live here; group files glob-import this module.

#[allow(unused_imports)]
use crate::*;
#[allow(unused_imports)]
use std::error::Error;

mod adversary;
mod attribution;
mod boolean_functional;
mod construction_stage1;
mod i13_junction_overrun;
mod i5_seam_density;
mod m4_substitute;
mod m5_case_iii;
mod m5_case_iv;
mod m5_k11_pair_chain;
mod m8_rim_refine;
mod matching;
mod membrane;
mod n137_torus_plane_corner;
mod n178_subres_coplanar;
pub(crate) mod n2_junction;
mod n47_moved_weld;
mod n50_f32_render_twin;
mod n55_s44b_coincidence;
mod p3a_edge_overrides;
mod p3a_insertion_conformality;
mod p3a_junction_pierce;
mod p3a_junction_wiring;
mod p3a_wedge_dedup;
mod p3b_beyond_corner_trim;
mod p3b_cylinder_pierce;
mod p3b_fan_retriangulation;
mod p3b_rim_insertion;
mod p3b_rim_pierce;
mod p3b_tube_insertion;
mod s188_envelope;
mod s194_edge_collapse;
mod s195_rim_plane_graze;
mod s434_output_restore;
mod s451_crease_domain;
mod s4_boundary_curve;
mod stage0_rim_projection;
mod stage1_cdt_flap;
mod topology;

#[allow(unused_imports)]
pub(crate) use adversary::*;
#[allow(unused_imports)]
pub(crate) use attribution::*;
#[allow(unused_imports)]
pub(crate) use boolean_functional::*;
#[allow(unused_imports)]
pub(crate) use construction_stage1::*;
#[allow(unused_imports)]
pub(crate) use m4_substitute::*;
#[allow(unused_imports)]
pub(crate) use m5_case_iii::*;
#[allow(unused_imports)]
pub(crate) use m5_case_iv::*;
#[allow(unused_imports)]
pub(crate) use matching::*;
#[allow(unused_imports)]
pub(crate) use membrane::*;
#[allow(unused_imports)]
pub(crate) use n2_junction::*;
#[allow(unused_imports)]
pub(crate) use topology::*;
