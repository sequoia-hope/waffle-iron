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

/*
 * Line-plane intersection in exact (Shewchuk-expansion) arithmetic.
 * Wraps upstream `lambda3d_LPI_exact` (indirect_predicates.h:69).
 *
 * Inputs:
 *   p, q, r, s, t: each a 3-double array [x, y, z] of input
 *   coordinates (NOT intervals — exact arithmetic uses plain
 *   doubles).
 *
 * Outputs (variable-length expansion arrays):
 *   lambda_x_out, lambda_y_out, lambda_z_out, lambda_d_out:
 *     each receives a pointer to a thread-local pool-allocated
 *     buffer of doubles. The buffer encodes a Shewchuk expansion
 *     (an "expansion of doubles" — the geometric value is the sum
 *     of the buffer entries).
 *   lambda_x_len, lambda_y_len, lambda_z_len, lambda_d_len:
 *     each receives the actual length of its expansion.
 *
 * Caller MUST release each output pointer by calling
 * `ip_free_doubles` on the same thread that allocated it
 * (`expansionObject::mempool` is `thread_local`).
 *
 * Stub build: writes null pointer + length 0 for all four outputs.
 *
 * PR-CR-IP3.
 */
void ip_lambda3d_lpi_exact(
    const double* p, const double* q, const double* r,
    const double* s, const double* t,
    double** lambda_x_out, int* lambda_x_len,
    double** lambda_y_out, int* lambda_y_len,
    double** lambda_z_out, int* lambda_z_len,
    double** lambda_d_out, int* lambda_d_len);

/*
 * Release a buffer previously returned by an `ip_*_exact` shim.
 * Must be called on the SAME thread that produced the buffer
 * (`expansionObject::mempool` is `thread_local`). Null pointer is
 * accepted as a no-op.
 *
 * Stub build: no-op (output pointers are always null in stub mode).
 *
 * PR-CR-IP3.
 */
void ip_free_doubles(double* p);

#ifdef __cplusplus
}
#endif

#endif /* INDIRECT_PREDICATES_SIDECAR_RS_WRAPPER_H */
