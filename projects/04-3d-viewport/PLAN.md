# 04 — 3D Viewport: Plan

## Completed

- [x] **Studio PBR Renderer & Real Material Appearance** (`specs/studio_pbr_renderer.md`):
  - End-to-end projection of `Material` / `Appearance` from `FeatureTree` through `wasm_bridge::render_view::body_metadata`, web worker, `/view` host decoder, and `CadModel.svelte` (Part, Assembly, and host viewer modes).
  - Curated engineering material library (`app/src/lib/viewport/materialPresets.js`) + `FeatureTree.svelte` Materials panel (`upsertMaterial`, `renameMaterial`, `deleteMaterial`, `setBodyMaterial`).
  - Opt-in Studio PBR lighting pipeline (`app/src/lib/viewport/studio/`): procedural 3-point softbox HDR PMREM environments + 5 bundled CC0 1K Radiance `.hdr` environments (Outdoor Sunny Park, Outdoor Overcast Plaza, Indoor Workbench, Machine Shop, Indoor Office) + session `.hdr`/`.exr` upload, Z-up right-handed equirectangular mapping & azimuthal rotation (`studioEnvRotation`), softly defocused CubeUV environment backdrop (`studioBackdrop`) across Orthographic & Perspective cameras, linear-depth view-space G-buffer (Orthographic & Perspective + `logarithmicDepthBuffer` + section clipping), Jimenez 2016 horizon-based GTAO + bilateral depth/normal denoiser, multi-bounce diffuse AO + Ground-Truth Specular Occlusion (GTSO), view-space Screen-Space Reflections (SSR), Z-up contact-shadow ground catcher, Khronos PBR Neutral / AgX tone mapping, and 8-sample Halton progressive idle super-sampling.

## Open

No open planned work. Viewport changes are driven by bug reports and by the
feature work that needs them.
