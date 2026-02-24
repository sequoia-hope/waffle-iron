# Selection Enhancement Plan

## Context

The extrude dialog's "Add Region" workflow uses a dropdown + "Add" button pattern that is clunky and disconnected from the 3D viewport. The user wants to click directly on faces and closed regions in the model to build up extrude profiles. Additionally, there is no way to reference existing 3D model geometry (edges, face boundaries) from within a sketch — a fundamental gap for real parametric modeling workflows.

This plan redesigns the extrude region selector into an interactive pick-from-viewport system, extends the ghost preview to multiple regions, and adds edge/face projection onto sketch planes.

---

## Phase 1: Interactive Region Picker (Extrude Dialog Redesign)

Replace the dropdown + "Add" button with a clickable "Regions" box. When the box is active, viewport face clicks add regions to the extrude.

### 1.1 Add region-pick-mode state

**File:** `app/src/lib/engine/store.svelte.js`

Add near line 101 (alongside other extrude dialog state):

```javascript
let extrudeRegionPickMode = $state(false);
export function getExtrudeRegionPickMode() { return extrudeRegionPickMode; }
export function setExtrudeRegionPickMode(active) { extrudeRegionPickMode = active; }
```

Expose `getExtrudeRegionPickMode` and `setExtrudeRegionPickMode` on the `storeAPI` return object (around line 360) and on `window.__waffle` (around line 330).

### 1.2 Intercept face clicks during region pick mode

**File:** `app/src/lib/engine/store.svelte.js`

At the top of `selectRef()` (around line 585), before existing logic:

```javascript
if (extrudeRegionPickMode && ref?.kind?.type === 'Face') {
    addExtrudeRegionFromRef(ref);
    return;
}
```

### 1.3 Implement `addExtrudeRegionFromRef()`

**File:** `app/src/lib/engine/store.svelte.js`

New function near the other extrude functions (~line 1435):

```javascript
export function addExtrudeRegionFromRef(geomRef) {
    if (!extrudeDialogState) return;

    // Try to resolve the face to a sketch profile
    // Look for the sketch that created this face's feature
    const featureId = geomRef?.anchor?.feature_id;
    const feature = featureId
        ? featureTree?.features?.find(f => f.id === featureId)
        : null;
    const sourceSketch = feature?.operation?.sketch;

    // If we can trace this face back to a sketch profile EndCap, use the sketch profile
    // Otherwise, store as a face-based region

    const region = {
        type: 'face',
        geomRef: JSON.parse(JSON.stringify(geomRef)),
        label: describeFaceRef(geomRef),
    };

    // Deduplicate
    const isDupe = extrudeDialogState.regions.some(r =>
        r.type === 'face' && geomRefEquals(r.geomRef, geomRef)
    );
    if (isDupe) return;

    extrudeDialogState = {
        ...extrudeDialogState,
        regions: [...extrudeDialogState.regions, region],
    };
}
```

Add a helper:

```javascript
function describeFaceRef(ref) {
    const role = ref?.selector?.role?.type;
    const featureId = ref?.anchor?.feature_id;
    const feature = featureId
        ? featureTree?.features?.find(f => f.id === featureId)
        : null;
    const name = feature?.name || 'Body';
    if (role) return `${name} / ${role}`;
    return name;
}
```

### 1.4 Migrate region type to discriminated union

**File:** `app/src/lib/engine/store.svelte.js`

The existing `regions` array contains `{ sketchId, sketchName, profileIndex }`. Change to a discriminated union:

```
{ type: 'sketchProfile', sketchId, sketchName, profileIndex }
{ type: 'face', geomRef, label }
```

Update `showExtrudeDialog()` (~line 1331) to populate the initial region with `type: 'sketchProfile'`. Update `addExtrudeRegion()` (~line 1404) similarly. Update all callers that read `region.sketchId` / `region.profileIndex` to check `region.type` first.

### 1.5 Redesign ExtrudeDialog regions section

**File:** `app/src/lib/ui/ExtrudeDialog.svelte`

Replace lines 137-163 (the region-list + add-profile sections) with:

```svelte
<div
    class="region-box"
    class:active={regionPickActive}
    role="button"
    tabindex="0"
    onclick={toggleRegionPick}
    data-testid="extrude-region-box"
>
    <div class="region-box-header">
        <span class="region-header">Regions ({regions.length})</span>
        <span class="pick-hint">
            {regionPickActive ? 'Click faces to add...' : 'Click to pick'}
        </span>
    </div>
    {#each regions as region, i}
        <div class="region-item" data-testid="extrude-region-{i}">
            <span class="region-label">{regionLabel(region)}</span>
            <button
                class="region-remove"
                onclick|stopPropagation={() => handleRemoveRegion(i)}
            >&times;</button>
        </div>
    {/each}
    {#if regions.length === 0}
        <div class="region-empty">No regions — click to pick faces</div>
    {/if}
</div>
```

**Remove entirely:** The "Add Profile" section (lines 151-163), `addProfileIndex` state, `handleAddProfile()`.

Add local state and functions:

```javascript
let regionPickActive = $state(false);

function toggleRegionPick() {
    regionPickActive = !regionPickActive;
    setExtrudeRegionPickMode(regionPickActive);
}

function regionLabel(region) {
    if (region.type === 'sketchProfile') {
        return `${region.sketchName} / Profile ${region.profileIndex + 1}`;
    }
    if (region.type === 'face') return region.label || 'Face';
    // Legacy fallback
    return `${region.sketchName || '?'} / Profile ${(region.profileIndex ?? 0) + 1}`;
}

// Deactivate pick mode when dialog closes
$effect(() => {
    if (!dialogState) {
        regionPickActive = false;
        setExtrudeRegionPickMode(false);
    }
});
```

Add CSS:

```css
.region-box {
    border: 2px solid var(--border-color, #444);
    border-radius: 4px;
    padding: 8px;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    display: flex;
    flex-direction: column;
    gap: 4px;
}
.region-box:hover { border-color: var(--accent, #0078d4); }
.region-box.active {
    border-color: var(--accent, #0078d4);
    background: rgba(0, 120, 212, 0.1);
}
.region-box-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
}
.pick-hint {
    font-size: 10px;
    color: var(--text-muted, #888);
    font-style: italic;
}
```

### 1.6 Hover color for pick mode

**File:** `app/src/lib/viewport/CadModel.svelte`

Import `getExtrudeRegionPickMode` from store. In `buildMaterials()` (~line 127), when `getExtrudeRegionPickMode()` is true, use distinct colors:

- Hovered face: green tint `0x55cc88` (instead of normal `0xaabbdd`)
- Faces already in regions list: bright green `0x44ff88`

To check if a face is already in the regions list, add a helper that checks `geomRefEquals(ref, region.geomRef)` against all face-type regions from `getExtrudeRegions()`.

### 1.7 Apply with graceful fallback

**File:** `app/src/lib/ui/ExtrudeDialog.svelte`

Update `handleApply()`:

```javascript
function handleApply() {
    const firstRegion = regions[0];
    if (!firstRegion) return;

    if (regions.length > 1) {
        log('warning', `Multi-region extrude: using first region (${regions.length} selected)`);
    }

    if (firstRegion.type === 'face') {
        showToast('warning', 'Face-based extrude not yet supported by engine');
        return;
    }

    const opts = { depthMode, secondDir, secondDepth, flipDirection };
    applyExtrude(depth, firstRegion.profileIndex ?? 0, cut, opts)
        .catch(err => log('error', `Extrude apply failed: ${err}`));
}
```

---

## Phase 2: Multi-Region Ghost Preview

Show ghost previews for all selected regions, not just regions[0].

### 2.1 Change preview params to an array

**File:** `app/src/lib/engine/store.svelte.js`

`extrudePreviewParams` becomes `Array | null` instead of `Object | null`. The setter and getter stay the same. The `$effect` in ExtrudeDialog changes.

### 2.2 Update ExtrudeDialog preview effect

**File:** `app/src/lib/ui/ExtrudeDialog.svelte`

Replace the preview `$effect` (lines 41-59) with:

```javascript
$effect(() => {
    if (!dialogState || depthMode !== 'Blind') {
        setExtrudePreviewParams(null);
        return;
    }
    const params = regions
        .filter(r => r.type === 'sketchProfile' || r.sketchId)
        .map(r => ({
            sketchId: r.sketchId ?? r.sketchId,
            profileIndex: r.profileIndex ?? 0,
            depth,
            flipDirection,
            symmetric: secondDir === 'Symmetric',
            cut,
        }));
    setExtrudePreviewParams(params.length > 0 ? params : null);
});
```

### 2.3 Update GhostPreview for arrays

**File:** `app/src/lib/viewport/GhostPreview.svelte`

Change `currentPreview` to `currentPreviews`:

```javascript
let currentPreviews = $derived.by(() => {
    const raw = getExtrudePreviewParams();
    if (!raw) return [];
    const arr = Array.isArray(raw) ? raw : [raw];
    return arr.map(p => buildPreview(p)).filter(Boolean);
});
```

Update the template:

```svelte
{#each currentPreviews as preview}
    <T.Mesh geometry={preview.geometry} material={preview.material}
        position={preview.position}
        rotation={[preview.rotation.x, preview.rotation.y, preview.rotation.z]}
        renderOrder={999} />
    <T.LineSegments geometry={preview.edgeGeometry} material={edgeMaterial}
        position={preview.position}
        rotation={[preview.rotation.x, preview.rotation.y, preview.rotation.z]}
        renderOrder={999} />
{/each}
```

The revolve preview (`currentRevolvePreview`) is unchanged.

---

## Phase 3: Face Boundary Extraction and Preview

Allow face-based regions to show ghost previews by extracting face boundaries from mesh data.

### 3.1 Face boundary extraction utility

**New file:** `app/src/lib/viewport/faceGeometry.js`

```javascript
/**
 * Extract the boundary polygon of a mesh face (identified by faceRange)
 * by finding triangle edges that appear only once (boundary edges).
 *
 * @param {object} mesh - { vertices: Float32Array, indices: Uint32Array, faceRanges }
 * @param {{ start_index: number, end_index: number }} range
 * @returns {Array<[number, number, number]>} Ordered boundary vertices (world coords)
 */
export function extractFaceBoundary(mesh, range) {
    // 1. Collect all edges (vertex index pairs) from triangles in [start_index, end_index)
    // 2. Count occurrences: edges appearing once are boundary, twice are interior
    // 3. Chain boundary edges into ordered loop(s)
    // 4. Return 3D vertex positions
}

/**
 * Find the mesh and faceRange matching a geomRef.
 */
export function findFaceRangeByRef(meshes, geomRef) {
    for (const mesh of meshes) {
        if (!mesh.faceRanges) continue;
        for (const range of mesh.faceRanges) {
            if (geomRefEquals(range.geom_ref, geomRef)) {
                return { mesh, range };
            }
        }
    }
    return null;
}
```

Import `geomRefEquals` from store or duplicate the comparison locally.

### 3.2 Face-based ghost preview

**File:** `app/src/lib/viewport/GhostPreview.svelte`

Add `buildFacePreview(params)` alongside existing `buildPreview()`:

```javascript
function buildFacePreview(params) {
    const meshes = getMeshes();
    const faceData = findFaceRangeByRef(meshes, params.geomRef);
    if (!faceData) return null;

    const boundary = extractFaceBoundary(faceData.mesh, faceData.range);
    if (boundary.length < 3) return null;

    // Compute face plane from boundary
    // (cross product of first two edge vectors, centroid as origin)
    const origin = centroid(boundary);
    const normal = computeNormal(boundary);
    const sp = buildSketchPlane(
        [origin.x, origin.y, origin.z],
        [normal.x, normal.y, normal.z]
    );

    // Project boundary to 2D
    const points2d = boundary.map(([x, y, z]) => {
        const rel = new THREE.Vector3(x, y, z).sub(sp.origin);
        return new THREE.Vector2(rel.dot(sp.xAxis), rel.dot(sp.yAxis));
    });

    const shape = new THREE.Shape(points2d);
    const effectiveDepth = Math.max(params.depth, 0.01);
    const extrudeDepth = params.symmetric ? effectiveDepth * 2 : effectiveDepth;

    const geometry = new THREE.ExtrudeGeometry(shape, {
        depth: extrudeDepth,
        bevelEnabled: false,
    });
    const edgeGeometry = new THREE.EdgesGeometry(geometry);

    const flipVisual = params.flipDirection !== params.cut;
    const basis = new THREE.Matrix4().makeBasis(sp.xAxis, sp.yAxis, sp.normal);
    const quaternion = new THREE.Quaternion().setFromRotationMatrix(basis);

    const position = sp.origin.clone();
    if (params.symmetric) position.addScaledVector(sp.normal, -effectiveDepth);
    else if (flipVisual) position.addScaledVector(sp.normal, -extrudeDepth);

    return {
        geometry, edgeGeometry,
        material: params.cut ? cutMaterial : bossMaterial,
        position: [position.x, position.y, position.z],
        rotation: new THREE.Euler().setFromQuaternion(quaternion),
    };
}
```

Update `currentPreviews` derivation to dispatch by region type:

```javascript
let currentPreviews = $derived.by(() => {
    const raw = getExtrudePreviewParams();
    if (!raw) return [];
    const arr = Array.isArray(raw) ? raw : [raw];
    return arr.map(p => {
        if (p.type === 'face') return buildFacePreview(p);
        return buildPreview(p);
    }).filter(Boolean);
});
```

### 3.3 Pass face region params to preview

**File:** `app/src/lib/ui/ExtrudeDialog.svelte`

Update the preview `$effect` to include face regions:

```javascript
const params = regions.map(r => {
    if (r.type === 'sketchProfile' || (!r.type && r.sketchId)) {
        return { type: 'sketchProfile', sketchId: r.sketchId, profileIndex: r.profileIndex, depth, flipDirection, symmetric: secondDir === 'Symmetric', cut };
    }
    if (r.type === 'face') {
        return { type: 'face', geomRef: r.geomRef, depth, flipDirection, symmetric: secondDir === 'Symmetric', cut };
    }
    return null;
}).filter(Boolean);
```

---

## Phase 4: Edge/Face Projection onto Sketch Planes

While in sketch mode, select 3D model edges or face boundaries and project them as construction geometry.

### 4.1 Projection math utility

**New file:** `app/src/lib/sketch/projectGeometry.js`

```javascript
import * as THREE from 'three';

/**
 * Project 3D edge vertices onto a sketch plane, returning 2D sketch coordinates.
 *
 * @param {Float32Array} vertices - All edge vertices for the mesh
 * @param {{ start_index: number, end_index: number }} range
 * @param {{ origin: THREE.Vector3, normal: THREE.Vector3, xAxis: THREE.Vector3, yAxis: THREE.Vector3 }} plane
 * @returns {Array<{ x: number, y: number }>}
 */
export function projectEdgeToSketch(vertices, range, plane) {
    const points = [];
    for (let i = range.start_index * 3; i < range.end_index * 3; i += 3) {
        const world = new THREE.Vector3(vertices[i], vertices[i+1], vertices[i+2]);
        const rel = world.clone().sub(plane.origin);
        const along = rel.dot(plane.normal);
        const projected = world.clone().addScaledVector(plane.normal, -along);
        const pRel = projected.clone().sub(plane.origin);
        points.push({ x: pRel.dot(plane.xAxis), y: pRel.dot(plane.yAxis) });
    }
    return points;
}

/**
 * Project a closed boundary (array of [x,y,z]) to sketch 2D.
 */
export function projectBoundaryToSketch(boundary, plane) {
    return boundary.map(([x, y, z]) => {
        const world = new THREE.Vector3(x, y, z);
        const rel = world.clone().sub(plane.origin);
        const along = rel.dot(plane.normal);
        const projected = world.clone().addScaledVector(plane.normal, -along);
        const pRel = projected.clone().sub(plane.origin);
        return { x: pRel.dot(plane.xAxis), y: pRel.dot(plane.yAxis) };
    });
}

/**
 * Simplify a polyline by collapsing points closer than `tolerance`.
 */
export function simplifyPolyline(points, tolerance = 0.01) {
    if (points.length < 2) return points;
    const result = [points[0]];
    for (let i = 1; i < points.length; i++) {
        const prev = result[result.length - 1];
        const dx = points[i].x - prev.x;
        const dy = points[i].y - prev.y;
        if (Math.sqrt(dx*dx + dy*dy) > tolerance) {
            result.push(points[i]);
        }
    }
    return result;
}
```

### 4.2 Add "Project" tool

**File:** `app/src/lib/sketch/tools.js`

Add a new tool handler in the switch statement (~line 173):

```javascript
case 'project': handleProjectTool(event, sketchPlane, snapped, rawSketchPos); break;
```

Implement `handleProjectTool`:

```javascript
function handleProjectTool(event, sketchPlane, snapped, rawSketchPos) {
    if (event.type !== 'click') return;

    // The event comes from CadModel or EdgeOverlay (re-enabled during project tool)
    // Check if we have a hovered edge or face ref
    const hovered = getHoveredRef();
    if (!hovered) return;

    const meshes = getMeshes();

    if (hovered.kind?.type === 'Edge') {
        // Find the edge in mesh data
        for (const mesh of meshes) {
            if (!mesh.edges?.ranges) continue;
            for (const range of mesh.edges.ranges) {
                if (!geomRefEquals(range.geom_ref, hovered)) continue;
                const projected = projectEdgeToSketch(
                    mesh.edges.vertices, range, sketchPlane
                );
                const simplified = simplifyPolyline(projected);
                createConstructionLinesFromPoints(simplified, false);
                return;
            }
        }
    }

    if (hovered.kind?.type === 'Face') {
        const faceData = findFaceRangeByRef(meshes, hovered);
        if (!faceData) return;
        const boundary = extractFaceBoundary(faceData.mesh, faceData.range);
        const projected = projectBoundaryToSketch(boundary, sketchPlane);
        const simplified = simplifyPolyline(projected);
        createConstructionLinesFromPoints(simplified, true); // closed=true
    }
}
```

Add `createConstructionLinesFromPoints()`:

```javascript
function createConstructionLinesFromPoints(points, closed) {
    if (points.length < 2) return;
    const pointIds = [];
    for (const pt of points) {
        const id = nextEntityId();
        addLocalEntity({ type: 'Point', id, x: pt.x, y: pt.y, construction: true });
        pointIds.push(id);
    }
    const n = closed ? points.length : points.length - 1;
    for (let i = 0; i < n; i++) {
        const j = (i + 1) % points.length;
        addLocalEntity({
            type: 'Line',
            id: nextEntityId(),
            start_id: pointIds[i],
            end_id: pointIds[j],
            construction: true,
        });
    }
}
```

### 4.3 Re-enable 3D picking during project tool in sketch mode

**File:** `app/src/lib/viewport/CadModel.svelte`

The model currently becomes non-interactive (opacity 0.2, raycasting disabled) during sketch mode. When the active tool is `project`, re-enable raycasting and increase opacity slightly:

In `buildMaterials()` (~line 127), check `isProjectToolActive()`:

```javascript
const projectActive = isProjectToolActive();
const transparent = inSketchMode && !projectActive;
const opacity = transparent ? 0.2 : (projectActive ? 0.5 : 1.0);
```

For raycasting, the mesh `raycast` prop should be conditional:

```svelte
{#if inSketchMode && !projectActive}
    <!-- Disable raycasting -->
{:else}
    <!-- Normal interactive mesh -->
{/if}
```

**File:** `app/src/lib/viewport/EdgeOverlay.svelte`

Similarly, the edge picking early-return for sketch mode (~line 186) needs an exception:

```javascript
if (getSketchMode()?.active && !isProjectToolActive()) return;
```

### 4.4 Add project tool to toolbar

**File:** `app/src/lib/ui/Toolbar.svelte`

Add a "Project" button in the sketch tools section, visible only when `sketchMode.active`:

```svelte
{#if inSketchMode}
    <button class:active={activeTool === 'project'} onclick={() => setActiveTool('project')}>
        Project
    </button>
{/if}
```

### 4.5 Store helpers

**File:** `app/src/lib/engine/store.svelte.js`

Add:

```javascript
export function isProjectToolActive() {
    return sketchMode?.active && activeTool === 'project';
}
```

Expose on `storeAPI`.

---

## Phase Dependencies

```
Phase 1 (Region picker UI) ──> Phase 2 (Multi-region ghost preview)
                             ╲
                              ──> Phase 3 (Face boundary + face preview)

Phase 4 (Edge/face projection to sketch) is independent
```

Recommended order: **1 → 2 → 3 → 4**. Phases 1+2 are one coherent PR. Phase 3 extends them. Phase 4 is standalone.

---

## Known Limitations (Implement UI, Let Fail Gracefully)

| Limitation | Behavior | Fix Later |
|---|---|---|
| **Multi-region extrude** | Toast: "Using first region only" | Engine needs multi-profile support in `rebuild.rs` |
| **Face-based extrude** | Apply disabled, toast: "Not yet supported" | Engine needs face→profile conversion |
| **Contiguous region detection** | All multi-regions treated as non-contiguous | Needs mesh adjacency analysis |
| **Multi-body union** | If attempted, engine error propagated | Boolean reliability work |
| **Non-planar face projection** | Polygon approximation of curved boundary | Acceptable for construction geometry |
| **Ghost preview for face regions** | May show slightly different shape than kernel would produce | Mesh boundary ≠ exact BREP boundary |

---

## Testing

### Phase 1+2 tests (`app/tests/gui/extrude-region-picker.spec.js`)
- Open extrude dialog after creating a sketch with 2+ profiles
- Click the region box → verify `.active` CSS class appears
- Click a face in viewport → verify region item appears in the box
- Click X on region item → verify it's removed
- Add 2 sketch profile regions → verify 2 ghost preview meshes render
- Close dialog → verify pick mode deactivates
- Escape key → verify dialog closes and pick mode clears

### Phase 3 tests (`app/tests/gui/extrude-face-region.spec.js`)
- Create box extrude
- Open new extrude dialog, activate region picker
- Click top face of box → verify face region appears with label like "Extrude 1 / EndCapPositive"
- Verify ghost preview renders on the face
- Click Apply → verify graceful toast "not yet supported"

### Phase 4 tests (`app/tests/gui/sketch-project-edge.spec.js`)
- Create box extrude
- Enter sketch mode on top face
- Select "Project" tool
- Hover edge of box → verify edge highlights
- Click edge → verify construction line entities appear in sketch
- Verify entities have `construction: true`
- Click a face → verify closed construction polyline appears

---

## Files Modified

| File | Phase | Changes |
|---|---|---|
| `app/src/lib/engine/store.svelte.js` | 1, 2, 4 | Region pick mode state, `selectRef` intercept, `addExtrudeRegionFromRef`, region type migration, `isProjectToolActive`, preview params as array |
| `app/src/lib/ui/ExtrudeDialog.svelte` | 1, 2, 3 | Replace dropdown+Add with clickable region box, multi-region preview effect, face region handling |
| `app/src/lib/viewport/CadModel.svelte` | 1, 4 | Pick-mode highlight colors, re-enable raycasting for project tool |
| `app/src/lib/viewport/GhostPreview.svelte` | 2, 3 | Array of previews, face-boundary-based preview |
| `app/src/lib/viewport/EdgeOverlay.svelte` | 4 | Allow edge picking during project tool in sketch mode |
| `app/src/lib/sketch/tools.js` | 4 | Project tool handler, `createConstructionLinesFromPoints` |
| `app/src/lib/ui/Toolbar.svelte` | 4 | Project tool button |

## New Files

| File | Phase | Purpose |
|---|---|---|
| `app/src/lib/viewport/faceGeometry.js` | 3 | `extractFaceBoundary`, `findFaceRangeByRef` |
| `app/src/lib/sketch/projectGeometry.js` | 4 | `projectEdgeToSketch`, `projectBoundaryToSketch`, `simplifyPolyline` |
| `app/tests/gui/extrude-region-picker.spec.js` | 1, 2 | Region picker interaction tests |
| `app/tests/gui/extrude-face-region.spec.js` | 3 | Face-based region tests |
| `app/tests/gui/sketch-project-edge.spec.js` | 4 | Edge/face projection tests |
