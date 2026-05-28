# Third-Party License Boundary

## This crate's own code

The Rust source code in this crate (`src/`, `tests/`, `build.rs`,
`Cargo.toml`, `CLAUDE.md`, `LICENSE-THIRD-PARTY.md`, and this
documentation) is licensed under **MIT** (the Waffle Iron workspace
default).

## Dynamically linked library: `Indirect_Predicates`

This crate dynamically links **Marco Attene's `Indirect_Predicates`**
C++ library (IMATI-GE / CNR), which is licensed under
**LGPL-2.1-or-later**.

- Upstream: <https://github.com/MarcoAttene/Indirect_Predicates>
- License: LGPL-2.1
- Headers used: `indirect_predicates.h`, `implicit_point.h`,
  `numerics.h`, plus their `.hpp` template / inline-implementation
  counterparts.

The library is **header-only**; this crate compiles a thin C++ shim
(`src/wrapper.cpp`) that includes the library headers, instantiates
the inline functions called by `extern "C"` shim entry points, and
exposes them to Rust via `bindgen`-generated bindings.

## License boundary semantics

LGPL-2.1 permits **dynamic linking** with code under any license
(including proprietary). Distributors of binaries that statically
embed `Indirect_Predicates` into a non-LGPL binary inherit LGPL
distribution obligations (provide source for the LGPL portion and
allow relinking).

**That obligation is the consumer's concern**, not this crate's.
The Waffle Iron workspace ships this crate as source; downstream
distributors who package it must audit their linkage and comply
accordingly.

## Eventual replacement

A clean-room Rust reimplementation of indirect predicates (no LGPL,
written from the Cherchi 2020 paper) is on the long-term roadmap.
When it lands, this sidecar becomes a reference oracle for
validation rather than a runtime dependency. See
`crates/indirect-predicates-sidecar-rs/CLAUDE.md` and
`memory/cherchi_rs_pr_cr_ip1.md` for the trajectory.
