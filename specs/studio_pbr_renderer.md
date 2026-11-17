# Studio PBR Renderer & Real Material Appearance

## 1. Goal

Render Waffle Iron parts and assemblies with physically based materials (`Material` / `Appearance`) and an opt-in, high-speed **Studio** lighting pipeline inspired by Ben Houston's *Building the AAA Lighting Pipeline in Three.js*, while keeping the default **Shaded** CAD viewport pixel-identical for unassigned parts and 60 fps interactive on WebGL2.

Specifically:

1. **Real Material Appearance (`CadModel` + `wasm-bridge` + `FeatureTree`):**
   - Wire `FeatureTree.materials` and `FeatureTree.body_materials` (`crates/feature-engine/src/types.rs`) end-to-end into `render_view::body_metadata`, the web worker, the `/view` host decoder, and `CadModel.svelte` so both Part tabs and multi-part Assembly tabs display each body's assigned material appearance (`color`, `metalness`, `roughness`, plus Studio finish traits such as `clearcoat`, `anisotropy`, and translucency).
   - Provide an in-editor **Materials** library and editor in `FeatureTree.svelte` backed by the authoritative Rust `UiToEngine::EditMaterials` command (`upsert`, `rename`, `delete`, `assign_body`), shipping curated engineering presets with SI densities (`kg/m³`) and physically calibrated PBR reflectance.

2. **AAA Studio Lighting Pipeline (`app/src/lib/viewport/studio/`):**
   - **Procedural HDR Studio Softbox IBL (`PMREMGenerator`):** Zero-network procedural studio environments (`studio-softbox`, `daylight-workshop`, `darkroom-rim`) prefiltered into PMREM mip chains for realistic metallic and dielectric reflections across rough and polished surfaces.
   - **Exact Linear-Depth View-Space G-Buffer + Horizon-Based GTAO:** Renders opaque CAD bodies (`waffleType === 'model'`) into a dedicated view-space `(normal.xyz, linearDepth)` target (supporting both `OrthographicCamera` and `PerspectiveCamera`, `logarithmicDepthBuffer: true`, and `SectionCap` clipping planes without interfering with 1 px edge/sketch overlays), computes Jimenez et al. 2016 Ground Truth Ambient Occlusion, and denoises with a depth-and-normal-weighted bilateral filter.
   - **Multi-Bounce Diffuse AO + Ground-Truth Specular Occlusion (GTSO):** Injects screen-space AO directly into `MeshPhysicalMaterial` (`onBeforeCompile` with a shared `customProgramCacheKey`), applying multi-bounce albedo-tinted indirect diffuse occlusion and Lagarde/Jimenez specular occlusion (`computeSpecularOcclusion(dotNV, ao, roughness)`) so bright studio softboxes do not glow inside occluded holes, pockets, or gear roots.
   - **View-Space Screen-Space Reflections (SSR) → PMREM Fallback:** Ray-marches reflected view rays against the linear-depth G-buffer and previous lit HDR frame so polished shafts, flanges, and neighboring assembly parts reflect each other and fall back smoothly to the studio PMREM map.
   - **Z-Up Soft Contact-Shadow Ground Catcher:** Places a cached vertical-projection soft shadow catcher plane just below the model's world minimum `Z` (`raycast` disabled; hidden automatically in sketch mode, section view, empty scenes, or when viewing from below `minZ`).
   - **Khronos PBR Neutral / AgX Tone Mapping + Premultiplied Background Preservation:** Compresses HDR studio highlights cleanly without shifting CSS `--viewport-bg` theme colors across dark and light themes.
   - **Demand-Driven Progressive Idle Accumulation (0% Idle Studio Cost):** Renders half-resolution AO/SSR while orbiting for responsive interaction, then accumulates `8` sub-pixel Halton `(2, 3)` jittered frames once stationary and halts further studio passes until the camera, hover/selection, or model changes.

---

## 2. Parameters

### 2.1 Persisted UI Settings (`app/src/lib/ui/settings.svelte.js`)

| Key | Type | Default | Valid Values | Description |
|-----|------|---------|--------------|-------------|
| `viewportShading` | `string` | `'shaded'` | `'shaded' \| 'studio'` | Active 3D viewport shading pipeline |
| `studioEnvironment` | `string` | `'studio-softbox'` | `'studio-softbox' \| 'daylight-workshop' \| 'darkroom-rim'` | Procedural HDR PMREM rig in Studio mode |
| `studioAO` | `boolean` | `true` | `boolean` | Enable GTAO + multi-bounce diffuse AO + GTSO specular occlusion |
| `studioSSR` | `boolean` | `true` | `boolean` | Enable screen-space reflections blended into PBR IBL radiance |
| `studioGroundShadow` | `boolean` | `true` | `boolean` | Enable Z-up ground contact-shadow catcher below model `minZ` |
| `studioToneMapping` | `string` | `'neutral'` | `'neutral' \| 'agx'` | HDR tone mapper applied in Studio composite pass |

### 2.2 Engine Material & Appearance (`feature_engine::types::Material`)

- `name`: non-empty unique `String`
- `density_kg_m3`: finite `f64 > 0.0` (SI `kg/m³`)
- `appearance`: `Option<Appearance>`:
  - `color`: `[f64; 3]`, each channel in `0.0..=1.0` (sRGB)
  - `metalness`: `Option<f64>` in `0.0..=1.0` (defaults to `0.3` in Shaded, `0.15` in Studio when omitted)
  - `roughness`: `Option<f64>` in `0.0..=1.0` (defaults to `0.6` in Shaded, `0.42` in Studio when omitted)

### 2.3 Render-View Body Metadata (`wasm_bridge::render_view::body_metadata_for`)

- When a body has an entry in its owning `FeatureTree.body_materials`:
  - `material`: `String` (material name)
  - `appearance`: `{ color: [f64; 3], metalness?: f64, roughness?: f64 }` (present when the referenced `Material` has `appearance: Some(...)`)
- When a body has **no** assigned material: both `material` and `appearance` keys are omitted from the metadata JSON object so unassigned documents remain byte-for-byte identical to existing `render_view_parity` fixtures.

---

## 3. Branch Table

| `viewportShading` | Body Material Assigned? | Sketch / Ghost / Highlight State | `studioAO` | `studioSSR` | `studioGroundShadow` | Camera / Section State | Expected Behavior |
|---|---|---|---|---|---|---|---|
| `'shaded'` | No | Normal | *ignored* | *ignored* | *ignored* | Any | Stock Threlte `autoRender`, `scene.environment = null`, `MeshStandardMaterial` with `--model-color`, `metalness: 0.3`, `roughness: 0.6` (pixel-identical to existing baselines) |
| `'shaded'` | Yes (`appearance`) | Normal | *ignored* | *ignored* | *ignored* | Any | `MeshStandardMaterial` using `appearance.color`, `roughness`, and `min(metalness, 0.45)` so metallic parts remain legible without IBL |
| `'studio'` | No | Normal | `true` | `true` | `true` | Stationary | Procedural PMREM IBL + `MeshPhysicalMaterial` (`--model-color`), GTAO + GTSO + SSR + ground shadow + progressive 8-sample Halton settle |
| `'studio'` | Yes (e.g. Chrome / Brass / Anodized / Clear PC) | Normal | `true` | `true` | `true` | Orbiting → Stationary | True `metalness` (up to `1.0`), `roughness`, preset `clearcoat`/`anisotropy`/translucency; half-res AO/SSR while moving, 8-sample super-sampled settle when still |
| `'studio'` | Any | Hover / Selected / Body-Selected / Feature-Face | Any | Any | Any | Any | Highlight material colors (`HOVER_COLOR`, `SELECTED_COLOR`, `BODY_SELECTED_COLOR`, `FEATURE_FACE_COLOR`) override affected face/body groups just as in Shaded mode |
| `'studio'` | Any | `sketchMode.active` or `context` ghost | Any | Any | Any | Any | Transparent bodies (`opacity: 0.2` / `0.4`) excluded from opaque G-buffer and ground shadow hidden during sketch mode |
| `'studio'` | Any | Normal | `false` | `false` | `false` | Any | Studio PMREM + `MeshPhysicalMaterial` + tone mapping run; G-buffer/AO/SSR skipped (`ao = 1.0`, `ssr = 0`), ground shadow mesh hidden |
| `'studio'` | Any | Normal | `true` | `true` | `true` | `section.active` or `camZ < minZ` | G-buffer clips by active section plane; `SectionCap` stencil passes work via stencil-enabled `frameTarget`; ground shadow hides during section view or upward bottom view |

---

## 4. Invariants

- **I1 (Default Shaded Identity):** When `viewportShading === 'shaded'` and no body has an assigned `Material`, the rendered output, material parameters, lighting rig, and closed UI chrome are identical to pre-feature behavior.
- **I2 (Engine-Authoritative Material State):** No duplicate material table exists in UI state; presets insert real `Material` records via `UiToEngine::EditMaterials`, and body appearance resolves from the engine's `FeatureTree` in Part, Assembly, and `/view` host modes.
- **I3 (Multi-Material Draw-Call & Reference Sharing):** `CadModel.svelte` caches plain material arrays in a bounded `Map` keyed by `(shadingMode, color, metalness, roughness, finishKey, transparent, opacity)` so unhighlighted bodies sharing a material share the exact `[Material]` array instance and shader program (`customProgramCacheKey`).
- **I4 (Projection & Logarithmic-Depth Independence):** The Studio G-buffer writes exact linear view-space depth `-vViewPosition.z` and view-space unit normals directly into a color attachment, reconstructing 3D view positions analytically for both `OrthographicCamera` and `PerspectiveCamera` regardless of `logarithmicDepthBuffer`.
- **I5 (Physical Ambient & Specular Occlusion):** In Studio mode with `studioAO: true`, ambient occlusion modulates indirect diffuse irradiance (with albedo multi-bounce) and indirect specular radiance via GTSO (`computeSpecularOcclusion`), darkening concave pockets/crevices relative to convex/exposed faces without uniformly blackening direct highlights.
- **I6 (Background Color Preservation):** Empty background pixels (`alpha == 0`) bypass tone-mapping offsets so CSS `--viewport-bg` displays without color shift across all 8 dark and light themes.
- **I7 (Progressive Idle Convergence):** After camera and scene inputs remain unchanged for `TARGET_SAMPLES` (`8`) frames in Studio mode, progressive accumulation converges (`converged === true`), projection view-offset jitter is cleared after every frame so picking raycasts are unjittered, and no further G-buffer/GTAO/SSR passes execute while idle.
- **I8 (Non-Interfering Ground Catcher):** The Studio ground catcher sits at `z = bbox.min.z - margin`, uses `raycast = () => {}`, `depthWrite = false`, and hides when `meshes.length === 0`, `sketchMode.active`, `section.active`, or camera looks upward from below `bbox.min.z`.

---

## 5. Oracles

1. **Rust Render-View Unit Oracle (`crates/wasm-bridge`):**
   - Unassigned Part and Assembly bodies omit `material` and `appearance` in `render_view::body_metadata` (preserving `render_view_parity`).
   - Assigned Part body and Assembly leaf instance emit exact `material` name and `appearance` (`color`, `metalness`, `roughness`) from the leaf's `FeatureTree`.
2. **Playwright Studio Pipeline & Material Oracles (`app/tests/gui/studio-pbr-renderer.spec.js`):**
   - **Material Appearance Oracle:** Assigning `Anodized Red` vs `Anodized Blue` vs `Gold (24k)` from the preset library updates `FeatureTree.materials`, computes positive SI mass via `MeasureMass`, and shifts rendered part pixel RGB channels in the expected physical direction (`R > B` for Red, `B > R` for Blue) in both Shaded and Studio modes.
   - **Metallic PMREM IBL Oracle:** A `Polished Chrome` (`metalness: 1.0, roughness: 0.06`) part in Studio mode reflects the procedural studio softbox HDRI with bright non-zero luminance, whereas turning Studio on vs Shaded produces measurably higher specular dynamic range.
   - **GTAO + GTSO Cavity Oracle:** On a part with an extruded pocket / Concave L-corner or counterbore, enabling `studioAO: true` vs `studioAO: false` measurably darkens interior cavity pixels more than exposed outer face pixels, in both Orthographic and Perspective cameras.
   - **SSR Reflection Oracle:** On a two-body scene (a bright colored body beside a reflective chrome plate), enabling `studioSSR: true` vs `studioSSR: false` measurably increases the reflected color channel on the chrome face viewing the neighbor.
   - **Ground Shadow Oracle:** Enabling `studioGroundShadow: true` vs `studioGroundShadow: false` darkens pixels directly beneath `bbox.min.z` while leaving background sky pixels above `bbox.max.z` unchanged, and clicking through the shadow region does not select a mesh (`selectedRefs.length === 0`).
   - **Progressive Idle Settle Oracle:** `window.__waffle.getStudioStats()` advances `sampleCount` up to `targetSamples` (`8`), reports `settled: true`, stops incrementing `passCount` while idle, and resets `sampleCount` on camera orbit.
   - **Chrome & Regression Oracles:** `body-list.spec.js`, `layout-overflow.spec.js`, `icon-gallery.spec.js`, `settings-modal.spec.js`, and `baselines/screenshot-suite.spec.js` pass without regression.

---

## 6. Failure Modes

- **Software WebGL without float color renderbuffer (`EXT_color_buffer_float`):** Detected via `renderer.extensions.has(...)`; falls back cleanly to `UnsignedByteType` (`RGBA8`) targets with packed depth/normals so headless software drivers never produce incomplete framebuffers.
- **Zero-dimension canvas (`width === 0 || height === 0`):** Studio pipeline skips render-target allocation and rendering cleanly without WebGL errors.
- **Invalid material input (empty name, non-positive density, out-of-range RGB):** Refused loudly by `Material::check` / `Appearance::check` in `feature-engine` with an error toast, leaving the document unchanged.

---

## 7. Research Basis

- **Ben Houston (2025/2026), *Building the AAA Lighting Pipeline in Three.js* (`ben3d.ca`):** Staged physically based lighting architecture separating direct lighting, prefiltered studio IBL (`PMREM`), horizon ambient occlusion (`GTAO`), ground-truth specular occlusion (`GTSO`), screen-space reflection (`SSR`) with PMREM fallback, tone mapping, and temporal accumulation. Adapted to dynamic B-Rep CAD by omitting static UV lightmaps and static 3D irradiance probe volumes in favor of procedural studio PMREM rigs and scale-adaptive view-space passes.
- **Jimenez et al. (2016), *Practical Real-Time Strategies for Accurate Indirect Occlusion* (SIGGRAPH):** Horizon-based Ground Truth Ambient Occlusion (GTAO), cubic polynomial multi-bounce diffuse AO approximation, and specular occlusion formulation.
- **Lagarde & de Rousiers (2014), *Moving Frostbite to Physically Based Rendering 3.0* (SIGGRAPH):** Specular occlusion from ambient visibility, view angle, and roughness.
- **Khronos Group (2024), *PBR Neutral Tone Mapper*:** Hue-preserving highlight compression (`THREE.NeutralToneMapping`) designed for accurate material swatch reproduction under studio lighting.

### 7a. Analytical vs. Approximate Method Justification

This specification governs **viewport shading and material presentation only** (`app/src/lib/viewport/`, `app/src/lib/ui/`, and metadata projection in `crates/wasm-bridge/src/render_view.rs`). It performs **zero** B-Rep modeling or surface-surface intersection operations; all CAD geometry and mass properties remain exact per `ARCHITECTURAL_INVARIANTS.md` A15.
