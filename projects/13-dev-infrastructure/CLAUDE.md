# 13 — Development Infrastructure: Agent Instructions

Read ARCHITECTURE.md in this directory and `docs/TESTING.md` first.

## Scripting a model

```rust
use test_harness::ModelBuilder;

let mut m = ModelBuilder::kernel_v2();
m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 0.01, 0.01).unwrap();
m.extrude("box", "sk", 0.01).unwrap();
m.assert_has_solid("box").unwrap();

let report = m.report().unwrap();
println!("{}", report.to_text());
```

Units are meters.

## Adding a scenario

1. Put it in a test binary under `crates/test-harness/tests/`. A new binary
   must be assigned to a tier (`docs/TESTING.md`, "New test-harness Binary").
2. Use `ModelBuilder::kernel_v2()` for real geometry and
   `ModelBuilder::mock()` for engine logic that does not depend on geometry.
3. Name every feature; failures are reported by name.
4. Call `assert_has_solid` after operations that should produce a solid, and
   `assert_no_errors` where the rebuild must be clean.
5. For a custom profile use `begin_sketch()` + `add_point()` + `add_line()` +
   `finish_sketch_manual()`.

## Checking a model

```rust
let verdicts = m.check_mesh("box").unwrap();      // mesh quality
let verdicts = m.check_topology("box").unwrap();  // V/E/F, Euler

for v in &m.report().unwrap().oracle_results {
    println!("[{}] {}: {}", if v.passed { "PASS" } else { "FAIL" }, v.oracle_name, v.detail);
}
```

## Running

```bash
cargo test -p test-harness --test <binary>                 # one binary
cargo test -p test-harness --test <binary> -- --nocapture  # with report output
./scripts/test.sh fast                                     # the fast tier
```

## Rules

- Never weaken a hook or a CI gate to get a change through. The overrides
  exist for the cases their messages name, and each use is logged.
- After changing Rust that feeds the WASM bundle, run `scripts/build-wasm.sh`
  and commit the bundle with the sources. `git add` new source files before
  building; the fingerprint hashes tracked files only.
- A new shipping crate goes into `rust-lint.yml`, `rust-test.yml` and
  `scripts/test.sh` in the commit that adds it.
