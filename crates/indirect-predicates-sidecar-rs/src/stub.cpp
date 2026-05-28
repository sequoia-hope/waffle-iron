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
