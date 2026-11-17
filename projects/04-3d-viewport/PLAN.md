# 04 — 3D Viewport: Plan

## Completed

- [x] **Studio PBR Renderer & Real Material Appearance** (`specs/studio_pbr_renderer.md`):
  - End-to-end projection of `Material` / `Appearance` from `FeatureTree` through `wasm_bridge::render_view::body_metadata`, web worker, `/view` host decoder, and `CadModel.svelte` (Part, Assembly, and host viewer modes).
  - Curated engineering & 3D-printed material library (`app/src/lib/viewport/materialPresets.js`, including shader-based FDM `3D Printed PLA / Silk PLA / PETG / CF-Nylon` exterior layer lines + global `studioPrintLayers` override) + `FeatureTree.svelte` Materials panel (`upsertMaterial`, `renameMaterial`, `deleteMaterial`, `setBodyMaterial`).
  - Opt-in Studio PBR lighting pipeline (`app/src/lib/viewport/studio/`): procedural 3-point softbox HDR PMREM environments + 6 bundled CC0 real-world scenes (`1K` `.hdr` PMREM + crisp `4096×2048` 4K `.webp` backdrops + optical DOF blur control + session `.hdr`/`.exr` upload) + grounded 3D beveled PBR tabletop stages (`StudioStageSurface.svelte`: Wooden Workbench, Walnut Office Desk, Polished Granite Countertop, Green Cutting Mat, or Floating), Z-up right-handed equirectangular mapping & azimuthal rotation (`studioEnvRotation`), linear-depth view-space G-buffer (Orthographic & Perspective + `logarithmicDepthBuffer` + section clipping), Jimenez 2016 horizon-based GTAO + bilateral depth/normal denoiser, multi-bounce diffuse AO + Ground-Truth Specular Occlusion (GTSO), view-space Screen-Space Reflections (SSR), Z-up contact-shadow ground catcher, Khronos PBR Neutral / AgX tone mapping, and 8-sample Halton progressive idle super-sampling.

## Open

No open planned work. Viewport changes are driven by bug reports and by the
feature work that needs them.
