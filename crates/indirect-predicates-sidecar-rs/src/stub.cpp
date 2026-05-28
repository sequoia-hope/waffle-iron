// indirect-predicates-sidecar-rs — Stub implementation.
//
// Compiled by build.rs when the Indirect_Predicates source is
// unavailable at build time (env var INDIRECT_PREDICATES_SRC unset
// AND default vendored path missing). build.rs also emits
// `cargo:rustc-cfg=ip_unavailable` so the Rust side knows it should
// self-skip dependent tests.

#include "wrapper.h"
#include <stdlib.h>   // malloc, free

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

extern "C" void ip_lambda3d_lpi_exact(
    const double* /*p*/, const double* /*q*/, const double* /*r*/,
    const double* /*s*/, const double* /*t*/,
    double** lambda_x_out, int* lambda_x_len,
    double** lambda_y_out, int* lambda_y_len,
    double** lambda_z_out, int* lambda_z_len,
    double** lambda_d_out, int* lambda_d_len
) {
    // Stub: null pointers + zero lengths. Rust side maps this to
    // 4 empty Vec<f64>.
    *lambda_x_out = nullptr; *lambda_x_len = 0;
    *lambda_y_out = nullptr; *lambda_y_len = 0;
    *lambda_z_out = nullptr; *lambda_z_len = 0;
    *lambda_d_out = nullptr; *lambda_d_len = 0;
}

extern "C" void ip_free_doubles(double* /*p*/) {
    // Stub: no allocation happened, nothing to free.
}

extern "C" void ip_lambda3d_tpi_interval(
    const double* /*v*/, const double* /*w*/, const double* /*u*/,
    double* lambda_out, bool* reliable
) {
    for (int i = 0; i < 8; ++i) lambda_out[i] = 0.0;
    *reliable = false;
}

extern "C" void ip_lambda3d_tpi_exact(
    const double* /*v*/, const double* /*w*/, const double* /*u*/,
    double** lambda_x_out, int* lambda_x_len,
    double** lambda_y_out, int* lambda_y_len,
    double** lambda_z_out, int* lambda_z_len,
    double** lambda_d_out, int* lambda_d_len
) {
    *lambda_x_out = nullptr; *lambda_x_len = 0;
    *lambda_y_out = nullptr; *lambda_y_len = 0;
    *lambda_z_out = nullptr; *lambda_z_len = 0;
    *lambda_d_out = nullptr; *lambda_d_len = 0;
}

// ----- PR-CR-IP5: ExplicitPoint3D round-trip-correct stub.
// Backing buffer is a 3-double malloc'd array; accessors read by
// offset. From Rust's perspective, behaves identically to the real
// shim (round-trip x/y/z through new + accessor).
extern "C" void* ip_explicit_point3d_new(double x, double y, double z) {
    double* buf = (double*)malloc(3 * sizeof(double));
    buf[0] = x; buf[1] = y; buf[2] = z;
    return buf;
}
extern "C" void ip_explicit_point3d_drop(void* p) {
    free(p);
}
extern "C" double ip_explicit_point3d_x(const void* p) {
    return ((const double*)p)[0];
}
extern "C" double ip_explicit_point3d_y(const void* p) {
    return ((const double*)p)[1];
}
extern "C" double ip_explicit_point3d_z(const void* p) {
    return ((const double*)p)[2];
}
