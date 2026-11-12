# 14 — Agent Link: Plan

Spec: `specs/waffle_mcp_server.md`.

## Open tasks

- [ ] Design parameters as MCP resources.
- [ ] A name on creation: `feature_add` and `sketch_create` cannot name the
      feature in the same undo step (`AddFeature` / `FinishSketch` carry no
      `name`).
- [ ] A kernel stop reaches an agent as a generic kernel failure. A distinct
      error kind needs kernel-v2 to map its typed stops to their own
      `KernelError` variant.
- [ ] `body_measure.closed` is false for boolean outputs. It is computed as
      "every edge has exactly two faces" (`wasm-bridge/src/dispatch.rs`);
      find out whether those bodies carry such edges or the census is wrong
      for them before fixing.
- [ ] The tool manifest is about 1.1 MB, all of it sent in `tools/list`.
- [ ] Pairing has not been tested by hand in headed Chrome, Edge or Safari
      against the hosted page (does the local-network prompt appear for a
      socket opened from the Allow click?).

## Blockers

None.
