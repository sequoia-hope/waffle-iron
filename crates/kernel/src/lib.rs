pub(crate) mod boolean;
pub mod diagnostics;
pub mod geometry;
pub mod mock_kernel;
// SSI solvers for all 15 quadric pairs exist but only plane-cylinder and
// cylinder-cylinder are wired into the boolean pipeline so far.  The remaining
// solvers (cone, sphere, torus pairs) will be activated as Tier-1 surface
// support expands.  Blanket allow(dead_code) is intentional — see A15.4.
#[allow(dead_code)]
pub(crate) mod ssi;
pub mod tessellation;
pub mod topology;
pub mod traits;
pub mod types;
pub mod units;
pub(crate) mod vecmath;
pub mod waffle_kernel;

pub use mock_kernel::MockKernel;
pub use traits::*;
pub use types::*;
pub use waffle_kernel::WaffleKernel;
