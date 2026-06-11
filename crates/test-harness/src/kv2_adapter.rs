//! Re-export shim: `KernelV2Adapter` moved to `kernel_v2::adapter` at the
//! Phase 6 migration (2026-06-11). Kept so existing `test_harness::kv2_adapter`
//! paths (assay_kv2, workflow) keep working.
pub use kernel_v2::adapter::*;
