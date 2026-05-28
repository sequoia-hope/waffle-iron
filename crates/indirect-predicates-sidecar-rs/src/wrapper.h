/*
 * indirect-predicates-sidecar-rs — FFI boundary header (pure C).
 *
 * Bindgen consumes this file with `-x c`. The C++ implementation
 * (which #include's the LGPL Indirect_Predicates headers) lives in
 * src/wrapper.cpp. When the upstream source is unavailable, the
 * symbols below are satisfied by src/stub.cpp instead.
 *
 * All entry points are prefixed `ip_` so bindgen's allowlist can
 * filter precisely (`--allowlist-function=ip_.*`).
 */

#ifndef INDIRECT_PREDICATES_SIDECAR_RS_WRAPPER_H
#define INDIRECT_PREDICATES_SIDECAR_RS_WRAPPER_H

/* Pulls in `bool` for C99+. C++ has its own built-in bool — both work. */
#ifndef __cplusplus
#include <stdbool.h>
#endif

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Calls dotProductSign2D on a well-conditioned input and returns
 * the result. Used by PR-CR-IP1 to prove the entire build chain
 * (cc + bindgen + linker against the LGPL library) works.
 *
 * Available build: returns +1 (sign of dot(P=(1,0), Q=(0,1)) at R=(0,0)).
 * Stub build:      returns -2 (sentinel).
 */
int ip_link_probe(void);

/*
 * Idempotent one-time-per-thread FPU initialization. Wraps upstream
 * `initFPU()`. No-op on 64-bit Linux without USE_SIMD_INSTRUCTIONS.
 * Always safe to call. PR-CR-IP2.
 */
void ip_init_fpu(void);

/*
 * Line-plane intersection in interval arithmetic. Wraps upstream
 * `lambda3d_LPI_interval` (indirect_predicates.h:68).
 *
 * Inputs (each 6 doubles = 3 IntervalNumbers as [inf,sup] pairs):
 *   p, q: two points defining the line
 *   r, s, t: three points defining the plane
 *
 * Outputs (lambda_out is 8 doubles):
 *   [lx_inf, lx_sup, ly_inf, ly_sup, lz_inf, lz_sup, ld_inf, ld_sup]
 *
 * `*reliable` is set to true iff the denominator interval `ld` does
 * not straddle zero. When false, fall back to lambda3d_LPI_exact
 * (PR-CR-IP3). Internally sets FPU to UPWARD then restores TONEAREST.
 *
 * Stub build: writes 8 zeros to lambda_out and sets *reliable = false.
 *
 * PR-CR-IP2.
 */
void ip_lambda3d_lpi_interval(
    const double* p, const double* q, const double* r,
    const double* s, const double* t,
    double* lambda_out, bool* reliable);

#ifdef __cplusplus
}
#endif

#endif /* INDIRECT_PREDICATES_SIDECAR_RS_WRAPPER_H */
