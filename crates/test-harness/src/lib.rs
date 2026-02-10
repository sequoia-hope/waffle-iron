//! Test harness for agent-driven CAD development.
//!
//! Provides programmatic tools for scripting multi-step CAD workflows,
//! verifying correctness at every step, and generating diagnostic output.
//!
//! # Key Components
//!
//! - [`ModelBuilder`] — Fluent API for building and verifying CAD models
//! - [`oracle`] — Verification functions returning pass/fail verdicts
//! - [`report`] — Structured text model descriptions
//! - [`stl`] — STL export from RenderMesh
//! - [`helpers`] — GeomRef constructors, profile builders, mesh math
//! - [`assertions`] — Rich assertion helpers with diagnostics

pub mod assertions;
pub mod helpers;
pub mod oracle;
pub mod report;
pub mod stl;
pub mod workflow;

pub use helpers::HarnessError;
pub use oracle::OracleVerdict;
pub use report::ModelReport;
pub use workflow::ModelBuilder;
