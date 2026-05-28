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

extern "C" void ip_init_fpu(void) {
    // No-op on 64-bit Linux without USE_SIMD_INSTRUCTIONS (the
    // default for PR-CR-IP1). Kept callable so the API surface is
    // portable to platforms where this matters (PR-CR-IP8 SIMD).
    initFPU();
}

// Convenience: construct an `interval_number` from a flat
// `(inf, sup)` double pair. The C++ 2-arg constructor takes
// `(min_low, sup)` where `min_low = -inf` (internal sign-inverted
// representation for SIMD).
static inline interval_number ip_from_pair(double inf, double sup) {
    return interval_number(-inf, sup);
}

extern "C" void ip_lambda3d_lpi_interval(
    const double* p, const double* q, const double* r,
    const double* s, const double* t,
    double* lambda_out, bool* reliable
) {
    // Each point is 6 doubles: x_inf, x_sup, y_inf, y_sup, z_inf, z_sup.
    interval_number px = ip_from_pair(p[0], p[1]);
    interval_number py = ip_from_pair(p[2], p[3]);
    interval_number pz = ip_from_pair(p[4], p[5]);
    interval_number qx = ip_from_pair(q[0], q[1]);
    interval_number qy = ip_from_pair(q[2], q[3]);
    interval_number qz = ip_from_pair(q[4], q[5]);
    interval_number rx = ip_from_pair(r[0], r[1]);
    interval_number ry = ip_from_pair(r[2], r[3]);
    interval_number rz = ip_from_pair(r[4], r[5]);
    interval_number sx = ip_from_pair(s[0], s[1]);
    interval_number sy = ip_from_pair(s[2], s[3]);
    interval_number sz = ip_from_pair(s[4], s[5]);
    interval_number tx = ip_from_pair(t[0], t[1]);
    interval_number ty = ip_from_pair(t[2], t[3]);
    interval_number tz = ip_from_pair(t[4], t[5]);

    interval_number lx, ly, lz, ld;
    // Upstream lambda3d_LPI_interval sets FPU UPWARD on entry,
    // restores TONEAREST on exit (indirect_predicates.hpp:7180/7222).
    bool ok = lambda3d_LPI_interval(
        px, py, pz, qx, qy, qz, rx, ry, rz, sx, sy, sz, tx, ty, tz,
        lx, ly, lz, ld);

    lambda_out[0] = lx.inf();
    lambda_out[1] = lx.sup();
    lambda_out[2] = ly.inf();
    lambda_out[3] = ly.sup();
    lambda_out[4] = lz.inf();
    lambda_out[5] = lz.sup();
    lambda_out[6] = ld.inf();
    lambda_out[7] = ld.sup();
    *reliable = ok;
}
