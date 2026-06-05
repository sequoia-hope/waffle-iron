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

/*
 * Triangle-plane intersection in interval arithmetic. Wraps
 * upstream `lambda3d_TPI_interval` (indirect_predicates.h:71).
 *
 * Inputs (each 18 doubles = 3 vertices × 3 coordinates × 2 bounds):
 *   v, w, u: three triangles defining the three planes whose
 *            intersection is being computed.
 *   Layout per triangle: [vert0_x_inf, vert0_x_sup, vert0_y_inf,
 *   vert0_y_sup, vert0_z_inf, vert0_z_sup, vert1..., vert2...].
 *
 * Outputs (lambda_out is 8 doubles, same layout as LPI interval):
 *   [lx_inf, lx_sup, ly_inf, ly_sup, lz_inf, lz_sup, ld_inf, ld_sup]
 *
 * `*reliable` is set to true iff the denominator interval `ld`
 * does not straddle zero. Internally sets FPU to UPWARD then
 * restores TONEAREST.
 *
 * Stub build: writes 8 zeros to lambda_out, sets *reliable = false.
 *
 * PR-CR-IP4.
 */
void ip_lambda3d_tpi_interval(
    const double* v, const double* w, const double* u,
    double* lambda_out, bool* reliable);

/*
 * Triangle-plane intersection in Shewchuk-expansion arithmetic.
 * Wraps upstream `lambda3d_TPI_exact` (indirect_predicates.h:72).
 *
 * Inputs (each 9 doubles = 3 vertices × 3 coordinates):
 *   v, w, u: three triangles defining the three planes.
 *   Layout per triangle: [vert0_x, vert0_y, vert0_z, vert1...,
 *                         vert2...].
 *
 * Outputs (variable-length pool-allocated expansions, same memory
 * model as ip_lambda3d_lpi_exact):
 *   lambda_*_out: pointers to thread-local pool buffers (caller
 *                 must call ip_free_doubles on each, same thread).
 *   lambda_*_len: actual expansion lengths.
 *
 * Stub build: writes 4 null pointers + 4 lengths of 0.
 *
 * PR-CR-IP4.
 */
void ip_lambda3d_tpi_exact(
    const double* v, const double* w, const double* u,
    double** lambda_x_out, int* lambda_x_len,
    double** lambda_y_out, int* lambda_y_len,
    double** lambda_z_out, int* lambda_z_len,
    double** lambda_d_out, int* lambda_d_len);

/*
 * ExplicitPoint3D opaque handle (PR-CR-IP5).
 *
 * Wraps upstream `explicitPoint3D` — a subclass of the polymorphic
 * `genericPoint` (implicit_point.h:336-355). Heap-allocated by
 * `ip_explicit_point3d_new`; freed via `ip_explicit_point3d_drop`.
 *
 * The pointer returned by `ip_explicit_point3d_new` is also a
 * valid `const genericPoint*` at the C++ level (subclass-to-base
 * conversion). Future predicate shims (PR-CR-IP6) accept the same
 * `void*` and reinterpret as `const genericPoint*`.
 *
 * Stub build: backing buffer is `malloc`'d `double[3]` storing
 * the input coordinates; accessors read by offset. Round-trip
 * correct from Rust's perspective.
 */
void* ip_explicit_point3d_new(double x, double y, double z);
void ip_explicit_point3d_drop(void* p);
double ip_explicit_point3d_x(const void* p);
double ip_explicit_point3d_y(const void* p);
double ip_explicit_point3d_z(const void* p);

/*
 * ImplicitPoint3DLpi opaque handle (PR-CR-IP5b).
 *
 * Wraps upstream `implicitPoint3D_LPI` (implicit_point.h:358-380).
 * Stores const references to 5 `explicitPoint3D` instances:
 *   p, q  — define the line
 *   r, s, t — define the plane
 *
 * The Rust side must enforce (via lifetime parameter) that the
 * implicit point doesn't outlive any of its 5 input points — the
 * upstream class holds raw references and dereferencing them after
 * the underlying explicit points are destroyed is UB.
 *
 * The returned `void*` is also a valid `const genericPoint*` at
 * the C++ level (subclass-to-base implicit conversion). Future
 * PR-CR-IP6 predicate shims will use the same pointer.
 *
 * Stub build: returns a 1-byte sentinel from malloc; drop frees.
 * No observable behavior beyond round-trip safety.
 */
void* ip_implicit_point3d_lpi_new(
    const void* p, const void* q,
    const void* r, const void* s, const void* t);
void ip_implicit_point3d_lpi_drop(void* p);

/*
 * ImplicitPoint3DTpi opaque handle (PR-CR-IP5b).
 *
 * Wraps upstream `implicitPoint3D_TPI` (implicit_point.h:384-412).
 * Stores const references to 9 `explicitPoint3D` instances:
 *   v1, v2, v3 — vertices of triangle 1 (plane 1)
 *   w1, w2, w3 — vertices of triangle 2 (plane 2)
 *   u1, u2, u3 — vertices of triangle 3 (plane 3)
 *
 * Same lifetime + ABI conventions as ip_implicit_point3d_lpi_*.
 */
void* ip_implicit_point3d_tpi_new(
    const void* v1, const void* v2, const void* v3,
    const void* w1, const void* w2, const void* w3,
    const void* u1, const void* u2, const void* u3);
void ip_implicit_point3d_tpi_drop(void* p);

/*
 * Cherchi 2022 §6.4 boolean-labeling trigger set (PR-CR-IP6).
 *
 * Each `const void*` parameter is a pointer to one of our handle
 * types' underlying C++ object — explicitPoint3D, implicitPoint3D_LPI,
 * or implicitPoint3D_TPI. The shim reinterprets as `const genericPoint*`
 * (valid via subclass-to-base single-inheritance address equality)
 * and binds to the C++ reference parameter.
 *
 * Returns an `int` matching upstream's `IP_Sign` convention:
 *   -1 = Negative, 0 = Zero, +1 = Positive, 2 = Undefined (NaN /
 *   catastrophic cancellation).
 *
 * Stub build: all four functions return 2 (Undefined sentinel).
 */
int ip_orient3d_indirect_iiii(
    const void* p1, const void* p2, const void* p3, const void* p4);
int ip_less_than_on_x_ii(const void* p1, const void* p2);
int ip_less_than_on_y_ii(const void* p1, const void* p2);
int ip_less_than_on_z_ii(const void* p1, const void* p2);

/*
 * PR-CR-AR2a Cycle 1 (CR-IP6b): 2D orientation + point-in-triangle.
 *
 * Each `const void*` is a pointer to one of our handle types'
 * underlying C++ object — explicitPoint3D, implicitPoint3D_LPI,
 * or implicitPoint3D_TPI. The shim reinterprets as
 * `const genericPoint*` (valid via subclass-to-base single-
 * inheritance address equality) and binds to the C++ reference
 * parameter.
 *
 * ip_orient2d_{xy,yz,zx} wrap `genericPoint::orient2D{xy,yz,zx}`
 * (implicit_point.h:138-140), the CCW/left-turn test for a triple
 * projected onto the named coordinate pair. They return an `int`
 * matching upstream's `IP_Sign` convention:
 *   -1 = Negative (CW), 0 = Zero (collinear), +1 = Positive (CCW),
 *    2 = Undefined (NaN / catastrophic cancellation).
 *
 * ip_point_in_triangle wraps `genericPoint::pointInTriangle`
 * (implicit_point.h:212), boundary-inclusive: returns 1 when P is
 * inside OR on the boundary of triangle ABC, 0 otherwise.
 *
 * Stub build: ip_orient2d_* return 2 (Undefined sentinel);
 * ip_point_in_triangle returns 0.
 */
int ip_orient2d_xy(const void* a, const void* b, const void* c);
int ip_orient2d_yz(const void* a, const void* b, const void* c);
int ip_orient2d_zx(const void* a, const void* b, const void* c);
int ip_point_in_triangle(const void* p, const void* a, const void* b, const void* c);

#ifdef __cplusplus
}
#endif

#endif /* INDIRECT_PREDICATES_SIDECAR_RS_WRAPPER_H */
