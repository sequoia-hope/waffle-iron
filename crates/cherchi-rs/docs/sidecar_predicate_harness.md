# TODO: C++ Sidecar Predicate Harness

## What this is

A planned addition to `cherchi-rs`'s test infrastructure: a small C++
program that exposes individual Cherchi 2020/2022 predicates as a JSON
stdin/stdout protocol, callable from Rust integration tests for byte-
exact differential testing.

## Why we don't have it yet

The PR-CR1 spike (porting `points_are_collinear_3d`) deliberately
deferred this work. Rationale:

1. **Sidecar build cost**: The Cherchi C++ sidecar at
   `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/` takes
   ~22 minutes to build from scratch (see
   `docs/sidecar/cherchi2022_build_guide.md`). The session that ran
   PR-CR1 didn't have the sidecar built.
2. **Harness authoring cost**: The existing sidecar wrapper invokes
   `mesh_booleans` (full pipeline) on OBJ files. For per-predicate
   testing, we need a different C++ binary — a small program that
   reads predicate inputs from stdin, calls the specific predicate,
   writes results to stdout.
3. **Adequate oracle exists for PR-CR1**: `points_are_collinear_3d`
   is a 3-projection composition of Shewchuk's exact `orient2d`. The
   Rust `geometry-predicates` crate is itself a Shewchuk port and
   serves as our reference for the orient2d primitives. Mathematical
   truth values (canonical collinear / non-collinear) cover the rest.

## When to build it

Trigger one of these conditions:

- A function being ported has NO independent Rust reference (e.g.,
  Cherchi's indirect predicates `orient3D_LPI`, `orient3D_TPI` have
  no `geometry-predicates` equivalent — they're Cherchi's unique contribution)
- We hit a behavior divergence between our port and the C++ that we
  can't pin down by reading the paper or the C++ source alone
- We commit to porting a stage (mesh arrangement, intersection
  classification) where per-predicate verification matters

The first port likely to trigger build is PR-CR2 or later — an
indirect predicate.

## Design sketch

### C++ binary: `cherchi_predicate_harness`

Lives in a new directory (NOT in this Rust repo — keep C++ outside,
matching the existing sidecar pattern):

```
/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/predicate_harness/
  main.cpp
  CMakeLists.txt
```

Protocol:

```
$ cherchi_predicate_harness points_are_colinear_3d
{"a": [0,0,0], "b": [1,0,0], "c": [2,0,0]}
{"collinear": true}
^D
```

- Reads JSON predicate-name from argv[1]
- Reads JSON inputs from stdin (one line per invocation)
- Calls the corresponding cinolib/Cherchi function
- Writes JSON result to stdout (one line per invocation)
- Use existing serde-json + nlohmann::json for parsing

### Rust integration test

```rust
#[test]
fn cpp_sidecar_parity_for_points_are_collinear_3d() {
    let mut child = Command::new(sidecar_bin("cherchi_predicate_harness"))
        .arg("points_are_colinear_3d")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let cases = vec![
        ((0.0,0.0,0.0), (1.0,0.0,0.0), (2.0,0.0,0.0)),
        ((0.0,0.0,0.0), (1.0,2.0,3.0), (2.0,4.0,6.0)),
        // ... corpus
    ];

    for (a, b, c) in &cases {
        let input = format!(r#"{{"a":[{},{},{}], "b":[{},{},{}], "c":[{},{},{}]}}\n"#,
            a.0, a.1, a.2, b.0, b.1, b.2, c.0, c.1, c.2);
        child.stdin.as_mut().unwrap().write_all(input.as_bytes())?;
        let output = read_line(&mut child.stdout)?;
        let cpp_result: bool = parse_json_bool(&output, "collinear");
        let rust_result = points_are_collinear_3d(
            Point3::new(a.0, a.1, a.2),
            Point3::new(b.0, b.1, b.2),
            Point3::new(c.0, c.1, c.2),
        );
        assert_eq!(rust_result, cpp_result,
            "divergence on ({:?}, {:?}, {:?})", a, b, c);
    }
}
```

Test should be gated on `CHERCHI_SIDECAR_BIN` env var pointing at the
binary (matching `cherchi2022_build_guide.md` convention). If env var
absent, test is skipped — CI runs differential when env is set, local
dev runs without it.

### Env-var contract

```
CHERCHI_SIDECAR_BIN=/path/to/cherchi_predicate_harness cargo test
```

When env var unset, differential tests are `#[ignore]`'d via a runtime
check (similar to how existing `cherchi2022_reference_parity.rs` handles
missing sidecar).

## What predicates the harness should expose (initial list)

Anything ported into `cherchi-rs/src/predicates/` that has a corresponding
cinolib or Cherchi 2020 function. Priority order:

1. `points_are_colinear_3d` (PR-CR1)
2. `orient3D_LPI_filtered` / `orient3D_LPI_exact` (cinolib `implicit_point.hpp`)
3. `orient3D_TPI_filtered` / `orient3D_TPI_exact`
4. `maxComponentInTriangleNormal_filtered` / `_exact`
5. `triangles_intersect_exact` (Cherchi 2022 §3 cinolib `predicates.cpp:1128-1252`)

## Estimated effort

- Sidecar build (one-time): ~22 min
- C++ harness binary: ~4 hours (CMakeLists, JSON parsing, dispatch table for predicates)
- Rust integration test infrastructure: ~2 hours (test helpers, env-var handling, common runner)
- First predicate's tests: ~1 hour
- Per-additional-predicate: ~30 min (just adding to dispatch table + adding tests)

## Banked decisions

When this work begins:
- Use `nlohmann::json` for C++ (already cinolib dependency? — verify)
- Use `serde_json` for Rust JSON
- Single binary handles all predicates (dispatch on argv[1]) vs binary-per-predicate (cleaner but more build setup) → single binary
- Run harness as a long-lived subprocess (stdin → stdout per case) vs spawn-per-case (slower but simpler) → long-lived (significant perf win for property-based tests)
