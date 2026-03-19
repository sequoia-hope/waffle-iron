# Document Model Spec

**Status**: Draft
**Author**: Human + Claude
**Date**: 2026-03-19
**Research Basis**: Onshape document model (multi-tab Part Studios + Assemblies), Fusion 360 hub/project/document hierarchy, SolidWorks part/assembly/drawing paradigm.

---

## 1. Goal

Give Waffle Iron a document model that supports multiple modeling contexts (tabs) within a single saveable file, a home page for browsing documents, and interactive 3D thumbnail previews. This replaces the current single-feature-tree-per-session model.

---

## 2. Document Hierarchy

```
Home (per-user, browser-local)
 └─ Document (.waffle file)
     ├─ DocumentMetadata { name, created, modified, display_unit }
     ├─ Tab[] (ordered, named, typed)
     │   ├─ PartTab { feature_tree, preview_mesh? }
     │   └─ AssemblyTab { references, mates }  ← deferred (sub-project 10)
     └─ active_tab_id: Uuid
```

### Definitions

| Term | Meaning |
|------|---------|
| **Document** | The shareable/saveable unit. One `.waffle` file = one document. |
| **Tab** | An isolated modeling context within a document. Has its own feature tree, bodies, and undo stack. |
| **PartTab** | A tab containing a parametric feature tree that produces N bodies (like Onshape's Part Studio). |
| **AssemblyTab** | A tab that references bodies from PartTabs and adds positional constraints. Deferred with sub-project 10. |
| **Home** | A document browser UI. Not a modeling environment. |

### Key constraint

Tabs are independent. A PartTab cannot reference geometry from another PartTab. Cross-tab references only exist in AssemblyTabs (deferred). This keeps the engine model simple: one active tab = one rebuild context.

---

## 3. File Format v3

### Schema

```rust
pub const FORMAT_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaffleFile {
    pub format: String,                // "waffle-iron"
    pub version: u32,                  // 3
    pub document: DocumentMetadata,
    pub tabs: Vec<Tab>,
    pub active_tab: Uuid,              // which tab is shown on open
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub name: String,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub display_unit: Option<String>,  // "mm", "cm", "m", "in", "ft"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    pub id: Uuid,
    pub name: String,
    pub kind: TabKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TabKind {
    Part {
        features: FeatureTree,
        preview_mesh: Option<PreviewMesh>,
    },
    // Assembly — deferred, variant added when sub-project 10 begins
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewMesh {
    pub vertices: Vec<f32>,    // positions, flat [x,y,z,...]
    pub normals: Vec<f32>,     // normals, flat [nx,ny,nz,...]
    pub indices: Vec<u32>,     // triangle indices
}
```

### Migration v2 → v3

```rust
fn migrate_v2_to_v3(v2: WaffleFileV2) -> WaffleFile {
    let tab_id = Uuid::new_v4();
    WaffleFile {
        format: "waffle-iron".into(),
        version: 3,
        document: DocumentMetadata {
            name: v2.project.name,
            created: v2.project.created,
            modified: v2.project.modified,
            display_unit: v2.project.display_unit,
        },
        tabs: vec![Tab {
            id: tab_id,
            name: "Part 1".into(),
            kind: TabKind::Part {
                features: v2.features,
                preview_mesh: None,
            },
        }],
        active_tab: tab_id,
    }
}
```

Existing v1 files migrate v1 → v2 → v3 (chain through existing mm→m migration first).

### Invariants

- `tabs` must contain at least one tab. Empty documents are not valid.
- `active_tab` must reference an `id` that exists in `tabs`.
- Tab `id` values must be unique within the document.
- Tab `name` values need not be unique (users may have "Part 1" and "Part 1" — UI appends disambiguation only in references).

---

## 4. Preview Mesh

### Purpose

Enable 3D thumbnail previews of documents in the home page without rebuilding the full model. The preview mesh is a snapshot of the last-known tessellation output.

### Generation

- **When**: After every successful rebuild that produces a non-empty mesh, update the active tab's `preview_mesh`.
- **What**: The full `RenderMesh` from the engine, decimated to a triangle budget.
- **Budget**: Max 2000 triangles. If the full mesh exceeds this, decimate using vertex-clustering (quantize positions to a grid, merge coincident vertices, remove degenerate triangles). No normals interpolation needed — flat shading is acceptable for thumbnails.
- **Storage**: Inline in the `.waffle` JSON. At 2000 tris × 3 verts × 3 floats × ~8 chars/float ≈ ~150KB of JSON. Acceptable for local files.

### Decimation algorithm

Vertex-clustering (simplest approach that preserves shape):

1. Compute AABB of mesh.
2. Choose grid cell size = AABB diagonal / 32 (≈32³ = 32K cells, more than enough).
3. For each vertex, compute grid cell `(ix, iy, iz)`.
4. For each cell with vertices, output one vertex at the centroid of its members, with the average normal.
5. Remap triangle indices to the merged vertices.
6. Remove degenerate triangles (two or more indices the same).
7. If still over budget, increase cell size and repeat.

### Rendering in thumbnails

The preview mesh renders in a minimal Threlte scene: no edge overlay, no sketch plane, no selection highlights. One ambient light + one directional light. Camera positioned at the mesh centroid + 2× bounding sphere radius along the isometric diagonal `[1,1,1]`.

The thumbnail viewport is interactive: orbit on drag, scroll to zoom. This is a natural consequence of using a real Threlte `<Canvas>` — interactivity comes free from `<OrbitControls>`.

---

## 5. Home Page

### Route

```
/           → Home (document browser)
/edit       → Document editor (current app, with tab bar added)
```

The current single-page app at `+page.svelte` moves to `/edit`. The new `/` route is the home page.

### Layout

```
┌─────────────────────────────────────────────────┐
│  Waffle Iron                    [New Document]  │
├─────────────────────────────────────────────────┤
│                                                 │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐      │
│  │ ╔══════╗ │  │ ╔══════╗ │  │ ╔══════╗ │      │
│  │ ║3D    ║ │  │ ║3D    ║ │  │ ║3D    ║ │      │
│  │ ║thumb ║ │  │ ║thumb ║ │  │ ║thumb ║ │      │
│  │ ╚══════╝ │  │ ╚══════╝ │  │ ╚══════╝ │      │
│  │ Bracket  │  │ Gearbox  │  │ Untitled │      │
│  │ 2h ago   │  │ yesterday│  │ Mar 15   │      │
│  └──────────┘  └──────────┘  └──────────┘      │
│                                                 │
└─────────────────────────────────────────────────┘
```

### Document cards

Each card shows:
- **3D preview**: A `<Canvas>` rendering the document's preview mesh (200×150px). Interactive orbit/zoom on hover.
- **Name**: Document name, editable on double-click.
- **Modified**: Relative timestamp ("2h ago", "yesterday", "Mar 15").
- **Context menu** (right-click or kebab icon): Rename, Duplicate, Export (.step), Delete.

### Actions

| Action | Behavior |
|--------|----------|
| Click card | Navigate to `/edit` with that document loaded |
| "New Document" button | Create document with one empty PartTab, navigate to `/edit` |
| Import button | File picker for `.waffle` files, add to home, open |
| Delete | Confirm dialog, remove from storage |
| Duplicate | Deep-copy document, append "(copy)" to name |

### Empty state

When no documents exist, show a centered message:

> No documents yet.
> [Create your first document]

---

## 6. Tab Bar

### Layout

The tab bar sits between the Toolbar and the viewport, replacing the current top edge of the modeling area.

```
┌─────────────────────────────────────────┐
│              Toolbar                    │
├──┬──────────┬──────────┬──┬─────────────┤
│  │ Part 1 ✕ │ Part 2 ✕ │ +│             │
├──┴──────────┴──────────┴──┴─────────────┤
│  │          Viewport          │         │
│  │                            │         │
```

### Tab behavior

| Action | Behavior |
|--------|----------|
| Click tab | Switch active tab. Engine unloads current tab's state, loads new tab's feature tree, triggers full rebuild. |
| Double-click tab name | Inline rename. Enter to confirm, Escape to cancel. |
| Click `✕` | Close tab. If it's the last tab, show confirm dialog ("This will delete the tab and its contents. Continue?"). If unsaved changes, prompt save first. |
| Click `+` | Add new empty PartTab named "Part N" (N = next unused integer). |
| Drag tab | Reorder tabs. Updates `tabs` array order in document. |
| Right-click tab | Context menu: Rename, Duplicate, Delete. |

### Tab switching — engine integration

Tab switching is the most complex interaction. The sequence:

1. **Save current tab state**: Capture the active feature tree (already in engine state) and current preview mesh into `tabs[active_tab]`.
2. **Send `SwitchTab` message** to engine with the new tab's `FeatureTree`.
3. **Engine resets**: Clears all solids, loads new feature tree, performs full rebuild.
4. **Engine responds** with `ModelUpdated` containing new meshes + feature tree.
5. **UI updates**: Feature tree panel, viewport, property editor all reflect new tab.

This is functionally equivalent to a `LoadProject` but without file I/O. The engine message:

```rust
UiToEngine::SwitchTab {
    features: FeatureTree,  // new tab's feature tree to load
}
// Response: EngineToUi::ModelUpdated { ... } (same as LoadProject)
```

### Undo isolation

Each tab has its own undo stack. Switching tabs does not carry undo history. This matches Onshape behavior and avoids cross-tab undo confusion.

---

## 7. Storage (Phase 1: Browser-Local)

Initial implementation uses browser storage only. No cloud, no OAuth.

### IndexedDB

Documents are stored in IndexedDB (not localStorage — too small for multiple documents with preview meshes). Schema:

```
Database: "waffle-iron"
  ObjectStore: "documents"
    key: Uuid (document id)
    value: {
      id: Uuid,
      json: string,        // full WaffleFile JSON
      name: string,         // denormalized for listing
      modified: number,     // epoch ms, denormalized for sorting
      thumbnail: string?,   // data URL of a 2D snapshot (fallback if no preview mesh)
    }
```

### Autosave migration

The current localStorage autosave (`waffle-autosave` key) migrates to IndexedDB on first load:
1. Check for `waffle-autosave` in localStorage.
2. If found, parse as v2 WaffleFile, migrate to v3, store in IndexedDB as a new document.
3. Clear localStorage keys.
4. Show the home page with the migrated document.

### Storage budget

IndexedDB has no hard limit in most browsers (uses available disk). A reasonable soft limit: warn the user when total stored data exceeds 100MB. Each document is typically 10KB–500KB (features) + 0–150KB (preview mesh).

---

## 8. Branch Table

| Branch | Condition | Behavior |
|--------|-----------|----------|
| Open app, no documents | IndexedDB empty, no localStorage autosave | Home page with empty state |
| Open app, has documents | IndexedDB has entries | Home page with document cards |
| Open app, has localStorage autosave | Legacy autosave key exists | Migrate to IndexedDB, show home |
| Click document card | — | Navigate to `/edit`, load document |
| New document | — | Create 1-tab document, navigate to `/edit` |
| Switch tab | Tabs exist | Save current tab state, load new tab, full rebuild |
| Close last tab | Only 1 tab remains | Confirm dialog; if confirmed, delete tab and navigate to home |
| Add tab | — | Append new PartTab, switch to it |
| Save (Ctrl+S) | — | Serialize full document (all tabs) to IndexedDB + trigger download |
| Load (.waffle file) | File picker | Parse, migrate if needed, store in IndexedDB, open |
| File version = 2 | v2 file loaded | Migrate v2→v3 (wrap in single tab) |
| File version = 1 | v1 file loaded | Migrate v1→v2→v3 (chain) |

---

## 9. Failure Modes

| Failure | Response |
|---------|----------|
| IndexedDB unavailable (private browsing) | Fall back to in-memory only. Warn user that documents will not persist. Save-to-file still works. |
| Preview mesh generation fails | Store `preview_mesh: None`. Home page shows placeholder icon instead of 3D preview. |
| Tab rebuild fails after switch | Show error in status bar. Feature tree shows error markers. Viewport shows last successful mesh (or empty). Do not lose the tab's feature tree. |
| Document JSON parse fails on load | Show error toast. Do not navigate away from home. Do not corrupt other documents. |
| Tab name is empty string | Reject. Default to "Part N". |
| v3 file with unknown TabKind variant | `serde(other)` → `TabKind::Unknown`. Display tab with warning "Unsupported tab type". Do not discard — preserve for round-trip. |

---

## 10. Oracles and Invariants

| Invariant | Oracle |
|-----------|--------|
| Document always has ≥1 tab | Assert on save, load, and tab delete |
| `active_tab` references valid tab id | Assert on save, load, and tab switch |
| Tab ids are unique within document | Assert on tab create and load |
| Preview mesh triangle count ≤ 2000 | Assert after decimation |
| v2 files load correctly as v3 | Round-trip test: save v2, load, assert single tab with correct features |
| Tab switch preserves inactive tab state | Test: create 2 tabs with features, switch between them, assert both feature trees intact |
| Autosave migrates to IndexedDB | Test: populate localStorage keys, init app, assert IndexedDB entry exists and localStorage cleared |

---

## 11. Non-Goals (Explicit Exclusions)

- **Cloud storage / OAuth**: Deferred to a future spec. The document model is storage-agnostic — `WaffleFile` serializes to JSON, which can go to IndexedDB, a file, or an API.
- **Assemblies**: Deferred with sub-project 10. The `TabKind::Assembly` variant is not implemented.
- **Multi-user / collaboration**: Not in scope. One user, one browser, one session.
- **Version history / undo across saves**: Not in scope. Each save is a full snapshot.
- **Document linking** (one document referencing another): Not in scope.
- **Fillet/chamfer/shell in tabs**: Still deferred per CLAUDE.md.

---

## 12. Implementation Sequence

1. **File format v3** (`crates/file-format/`): New types, v2→v3 migration, round-trip tests.
2. **Engine tab support** (`crates/wasm-bridge/`): `SwitchTab` message, tab-aware save/load.
3. **Tab bar UI** (`app/src/lib/ui/`): Tab bar component, tab switching, add/remove/rename.
4. **Routing** (`app/src/routes/`): Split into `/` (home) and `/edit` (editor).
5. **IndexedDB storage** (`app/src/lib/storage/`): Document persistence layer.
6. **Home page** (`app/src/routes/+page.svelte`): Document cards, create/delete/open.
7. **Preview mesh** (`crates/kernel/` or `crates/feature-engine/`): Decimation, storage in tab.
8. **3D thumbnail viewports** (`app/src/lib/ui/`): Minimal Threlte scene for document cards.

Steps 1–3 can be developed and tested independently. Steps 4–6 form the home page. Steps 7–8 add the preview thumbnails.
