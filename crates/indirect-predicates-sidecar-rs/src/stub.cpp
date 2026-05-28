// indirect-predicates-sidecar-rs — Stub implementation.
//
// Compiled by build.rs when the Indirect_Predicates source is
// unavailable at build time (env var INDIRECT_PREDICATES_SRC unset
// AND default vendored path missing). build.rs also emits
// `cargo:rustc-cfg=ip_unavailable` so the Rust side knows it should
// self-skip dependent tests.

#include "wrapper.h"

extern "C" int ip_link_probe(void) {
    // Sentinel indicating the FFI shim is the no-op stub, not the
    // real Indirect_Predicates library.
    return -2;
}

extern "C" void ip_init_fpu(void) {
    // No-op in stub mode (matches the real `initFPU()` no-op behavior
    // on 64-bit Linux without USE_SIMD_INSTRUCTIONS).
}

extern "C" void ip_lambda3d_lpi_interval(
    const double* /*p*/, const double* /*q*/, const double* /*r*/,
    const double* /*s*/, const double* /*t*/,
    double* lambda_out, bool* reliable
) {
    // Stub: zero out the 8-double output array, mark unreliable.
    for (int i = 0; i < 8; ++i) lambda_out[i] = 0.0;
    *reliable = false;
}
