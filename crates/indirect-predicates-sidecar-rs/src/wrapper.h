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

#ifdef __cplusplus
}
#endif

#endif /* INDIRECT_PREDICATES_SIDECAR_RS_WRAPPER_H */
