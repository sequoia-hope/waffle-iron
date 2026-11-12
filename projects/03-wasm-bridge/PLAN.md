# 03 — WASM Bridge: Plan

## Open tasks

- [ ] Latency baseline: measure command round-trip time and mesh transfer
      time for 10K, 100K and 1M triangle meshes, and record the numbers.
- [ ] `feature_engine::preview_mesh::decimate_mesh` orders its output by
      `HashMap` iteration, so a native process's `preview_mesh` varies from
      run to run.
- [ ] `render_view_parity.mjs` compares each bundle's census to `golden.json`
      byte for byte, but the serialized response contains `DocumentInfo.id`, a
      UUID minted per `EngineState`, so the response hashes differ every run.
      Normalize or exclude `document.id` in the census. Until then a moved
      response hash with an unchanged length is noise.
- [ ] `cargo clippy -p wasm-bridge --target wasm32-unknown-unknown` flags the
      `thread_local!` initializer in `wasm_api.rs`. CI does not lint wasm32.
- [ ] The store writes `activeTabId` before it sends `SwitchTab` /
      `OpenPartInContext`, ahead of the session it is meant to mirror.

## Blockers

None.
