// indirect-predicates-sidecar-rs — C++ shim for PR-CR-IP1.
//
// Bridges the pure-C extern "C" interface declared in wrapper.h
// against Marco Attene's LGPL-2.1 Indirect_Predicates library. This
// file is compiled by build.rs ONLY when the library source was
// discovered (env var INDIRECT_PREDICATES_SRC or the default
// Cherchi2022 vendored path). When the source is unavailable,
// stub.cpp is compiled instead and provides sentinel return values.
//
// PR-CR-IP1 exposes a single shim, `ip_link_probe`, that calls
// dotProductSign2D — a free function declared at
// indirect_predicates.h:35 with a 6-double-in / int-out signature.
// The input (0,0)(1,0)(0,1) is well-conditioned and short-circuits
// on the library's `_filtered` fast path, so no FPU init / SIMD /
// bigfloat machinery is exercised. Real predicate wrappers (LPI/TPI
// lambdas, orient3d_indirect_IIII, etc.) ship in PR-CR-IP2+.

// Include order is load-bearing: `implicit_point.h` must come first.
// Including `indirect_predicates.h` directly triggers parse errors in
// `implicit_point.hpp` because that file's inline method bodies refer
// to `lambda*_exact` / `lambda*_bigfloat` functions whose forward
// declarations only appear later in `indirect_predicates.h`.
// Including `implicit_point.h` first defers those parse decisions
// until `indirect_predicates.h` later supplies the declarations.
// Upstream's own test.cpp uses this order. Do NOT reorder.
#include "implicit_point.h"
#include "indirect_predicates.h"

#include "wrapper.h"

extern "C" int ip_link_probe(void) {
    // Reference inputs: P=(0,0), R=(1,0), Q=(0,1). dotProductSign2D
    // returns the sign of (P-R) · (Q-R). Well-conditioned; resolves
    // on the `_filtered` fast path without invoking the cascade.
    return dotProductSign2D(0.0, 0.0, 1.0, 0.0, 0.0, 1.0);
}
