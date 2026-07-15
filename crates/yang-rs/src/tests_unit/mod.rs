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
mod m4_substitute;
mod m5_case_iv;
mod matching;
mod membrane;
pub(crate) mod n2_junction;
mod n47_moved_weld;
mod n50_f32_render_twin;
mod stage0_rim_projection;
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
pub(crate) use m5_case_iv::*;
#[allow(unused_imports)]
pub(crate) use matching::*;
#[allow(unused_imports)]
pub(crate) use membrane::*;
#[allow(unused_imports)]
pub(crate) use n2_junction::*;
#[allow(unused_imports)]
pub(crate) use topology::*;
