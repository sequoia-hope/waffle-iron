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

extern "C" void ip_lambda3d_lpi_exact(
    const double* p, const double* q, const double* r,
    const double* s, const double* t,
    double** lambda_x_out, int* lambda_x_len,
    double** lambda_y_out, int* lambda_y_len,
    double** lambda_z_out, int* lambda_z_len,
    double** lambda_d_out, int* lambda_d_len
) {
    // Pass initial (null, 0) for each expansion buffer. Inside
    // lambda3d_LPI_exact, every Gen_*_With_PreAlloc check
    // `if (hlen < newlen) *h = AllocDoubles(newlen)` triggers
    // (since hlen starts at 0), so the function allocates the
    // entire result from expansionObject::mempool (thread_local).
    double* lx = nullptr; int lxn = 0;
    double* ly = nullptr; int lyn = 0;
    double* lz = nullptr; int lzn = 0;
    double* ld = nullptr; int ldn = 0;
    lambda3d_LPI_exact(
        p[0], p[1], p[2], q[0], q[1], q[2],
        r[0], r[1], r[2], s[0], s[1], s[2], t[0], t[1], t[2],
        &lx, lxn, &ly, lyn, &lz, lzn, &ld, ldn);
    *lambda_x_out = lx; *lambda_x_len = lxn;
    *lambda_y_out = ly; *lambda_y_len = lyn;
    *lambda_z_out = lz; *lambda_z_len = lzn;
    *lambda_d_out = ld; *lambda_d_len = ldn;
}

extern "C" void ip_free_doubles(double* p) {
    if (p) FreeDoubles(p);   // == expansionObject::mempool.release(p)
}

extern "C" void ip_lambda3d_tpi_interval(
    const double* v, const double* w, const double* u,
    double* lambda_out, bool* reliable
) {
    // Unpack 18 doubles per triangle into 9 interval_number values.
    // Layout: [v0_x_inf, v0_x_sup, v0_y_inf, v0_y_sup, v0_z_inf, v0_z_sup,
    //          v1_x_inf, ..., v2_z_sup]. Same for w, u.
    interval_number v1x = ip_from_pair(v[0], v[1]);
    interval_number v1y = ip_from_pair(v[2], v[3]);
    interval_number v1z = ip_from_pair(v[4], v[5]);
    interval_number v2x = ip_from_pair(v[6], v[7]);
    interval_number v2y = ip_from_pair(v[8], v[9]);
    interval_number v2z = ip_from_pair(v[10], v[11]);
    interval_number v3x = ip_from_pair(v[12], v[13]);
    interval_number v3y = ip_from_pair(v[14], v[15]);
    interval_number v3z = ip_from_pair(v[16], v[17]);
    interval_number w1x = ip_from_pair(w[0], w[1]);
    interval_number w1y = ip_from_pair(w[2], w[3]);
    interval_number w1z = ip_from_pair(w[4], w[5]);
    interval_number w2x = ip_from_pair(w[6], w[7]);
    interval_number w2y = ip_from_pair(w[8], w[9]);
    interval_number w2z = ip_from_pair(w[10], w[11]);
    interval_number w3x = ip_from_pair(w[12], w[13]);
    interval_number w3y = ip_from_pair(w[14], w[15]);
    interval_number w3z = ip_from_pair(w[16], w[17]);
    interval_number u1x = ip_from_pair(u[0], u[1]);
    interval_number u1y = ip_from_pair(u[2], u[3]);
    interval_number u1z = ip_from_pair(u[4], u[5]);
    interval_number u2x = ip_from_pair(u[6], u[7]);
    interval_number u2y = ip_from_pair(u[8], u[9]);
    interval_number u2z = ip_from_pair(u[10], u[11]);
    interval_number u3x = ip_from_pair(u[12], u[13]);
    interval_number u3y = ip_from_pair(u[14], u[15]);
    interval_number u3z = ip_from_pair(u[16], u[17]);

    interval_number lx, ly, lz, ld;
    // Upstream sets FPU UPWARD on entry, restores TONEAREST on exit
    // (indirect_predicates.hpp:7366/7474).
    bool ok = lambda3d_TPI_interval(
        v1x, v1y, v1z, v2x, v2y, v2z, v3x, v3y, v3z,
        w1x, w1y, w1z, w2x, w2y, w2z, w3x, w3y, w3z,
        u1x, u1y, u1z, u2x, u2y, u2z, u3x, u3y, u3z,
        lx, ly, lz, ld);

    lambda_out[0] = lx.inf(); lambda_out[1] = lx.sup();
    lambda_out[2] = ly.inf(); lambda_out[3] = ly.sup();
    lambda_out[4] = lz.inf(); lambda_out[5] = lz.sup();
    lambda_out[6] = ld.inf(); lambda_out[7] = ld.sup();
    *reliable = ok;
}

// ----- PR-CR-IP5: ExplicitPoint3D opaque handle.
// Wraps upstream `explicitPoint3D` (implicit_point.h:336-355).
// Caller owns the heap allocation; Rust `Drop` calls
// `ip_explicit_point3d_drop` to release.
//
// The returned `void*` is also a valid `const genericPoint*` at
// the C++ level (subclass-to-base implicit conversion). Future
// PR-CR-IP6 predicate shims will accept the same pointer and
// reinterpret as `const genericPoint*`.
extern "C" void* ip_explicit_point3d_new(double x, double y, double z) {
    return new explicitPoint3D(x, y, z);
}
extern "C" void ip_explicit_point3d_drop(void* p) {
    delete (explicitPoint3D*)p;
}
extern "C" double ip_explicit_point3d_x(const void* p) {
    return ((const explicitPoint3D*)p)->X();
}
extern "C" double ip_explicit_point3d_y(const void* p) {
    return ((const explicitPoint3D*)p)->Y();
}
extern "C" double ip_explicit_point3d_z(const void* p) {
    return ((const explicitPoint3D*)p)->Z();
}

extern "C" void ip_lambda3d_tpi_exact(
    const double* v, const double* w, const double* u,
    double** lambda_x_out, int* lambda_x_len,
    double** lambda_y_out, int* lambda_y_len,
    double** lambda_z_out, int* lambda_z_len,
    double** lambda_d_out, int* lambda_d_len
) {
    // Same pool-allocation pattern as ip_lambda3d_lpi_exact: initial
    // (null, 0) → C++ allocates from expansionObject::mempool.
    // Each input triangle is 9 doubles [v0_x, v0_y, v0_z, v1..., v2...].
    double* lx = nullptr; int lxn = 0;
    double* ly = nullptr; int lyn = 0;
    double* lz = nullptr; int lzn = 0;
    double* ld = nullptr; int ldn = 0;
    lambda3d_TPI_exact(
        v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8],
        w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7], w[8],
        u[0], u[1], u[2], u[3], u[4], u[5], u[6], u[7], u[8],
        &lx, lxn, &ly, lyn, &lz, lzn, &ld, ldn);
    *lambda_x_out = lx; *lambda_x_len = lxn;
    *lambda_y_out = ly; *lambda_y_len = lyn;
    *lambda_z_out = lz; *lambda_z_len = lzn;
    *lambda_d_out = ld; *lambda_d_len = ldn;
}
