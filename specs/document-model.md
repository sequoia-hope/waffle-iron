# Document Model Spec

**Status**: Draft
**Author**: Human + Claude
**Date**: 2026-03-19
**Research Basis**: Onshape document model (multi-tab Part Studios + Assemblies), Fusion 360 hub/project/document hierarchy, SolidWorks part/assembly/drawing paradigm. remoteStorage open protocol. AT Protocol (Bluesky) personal data stores.

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

## 5. Routing and URLs

### Routes

```
/                              → Home (document browser)
/doc/:id                       → Open document (redirects to active tab)
/doc/:id/:slug                 → Same, slug is cosmetic (ignored by router)
/doc/:id/:slug/tab/:tabIndex   → Deep link to specific tab
```

Document IDs are 8-character base62 strings derived from the document's UUID (e.g., `k7Tm4xQ2`). The optional `:slug` is a URL-safe version of the document name, included for readability but not used for resolution (GitHub-style). The router strips the slug and resolves by ID alone.

The current single-page app at `+page.svelte` becomes the `/doc/:id` route. The new `/` route is the home page.

### Shareable URLs

Document URLs are designed to be shareable even before cloud storage exists. The sharing contract:

```
https://waffle.app/doc/k7Tm4xQ2/gearbox-housing?src=<encoded-url-to-waffle-json>
```

| Parameter | Purpose |
|-----------|---------|
| `/doc/:id` | Local document ID (used if document exists in local IndexedDB) |
| `/:slug` | Human-readable name (cosmetic, ignored by router) |
| `?src=` | URL to the raw `.waffle` JSON (Google Drive direct link, GitHub raw URL, AT Protocol record URL, etc.) |

**Resolution order** when opening a shared URL:
1. Check IndexedDB for a document with matching ID. If found, open it.
2. If not found but `?src=` is present, fetch the JSON, parse as WaffleFile, store locally, open.
3. If neither, show "Document not found" with option to browse home.

This means sharing works across storage providers. A GitHub user can share a link with someone who uses Google Drive — the recipient's app fetches the JSON from the `src` URL regardless of provider.

### Future: provider-native resolution

When a user has connected a storage provider, the app can resolve document IDs through that provider without the `?src=` parameter. The provider's `DocumentStore.load(id)` handles resolution. This enables clean URLs like `https://waffle.app/doc/k7Tm4xQ2/gearbox-housing` that resolve through the user's connected GitHub repo or AT Protocol PDS.

### Home Page Layout

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
| Click card | Navigate to `/doc/:id/:slug` |
| "New Document" button | Create document with one empty PartTab, navigate to `/doc/:id/untitled` |
| Import button | File picker for `.waffle` files, add to storage, open |
| Delete | Confirm dialog, remove from storage |
| Duplicate | Deep-copy document with new ID, append "(copy)" to name |
| Share | Copy shareable URL to clipboard (provider-dependent, see §7) |

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

## 7. Storage

### Design principle

Storage is a pluggable adapter. The document model is storage-agnostic — `WaffleFile` serializes to JSON, which can go to IndexedDB, a file download, a GitHub repo, Google Drive, or an AT Protocol PDS. Users can use multiple providers simultaneously (e.g., local + GitHub). The home page aggregates documents from all connected providers.

### Storage adapter interface

```typescript
interface DocumentStore {
  readonly id: string              // "local", "github", "gdrive", "atproto"
  readonly label: string           // "This Browser", "GitHub", etc.
  readonly canShare: boolean       // whether getShareUrl is meaningful

  list(): Promise<DocumentSummary[]>
  load(docId: string): Promise<WaffleFile>
  save(docId: string, doc: WaffleFile): Promise<void>
  delete(docId: string): Promise<void>
  getShareUrl(docId: string): Promise<string | null>
}

interface DocumentSummary {
  id: string                       // 8-char base62
  name: string
  modified: number                 // epoch ms
  provider: string                 // which store this came from
  previewMesh: PreviewMesh | null
}
```

### Provider roadmap

| Phase | Provider | Auth | Sharing | Notes |
|-------|----------|------|---------|-------|
| **0** | **IndexedDB** (local) | None | Export `.waffle` file | Zero setup. User opens URL, starts modeling. Default for all users. |
| **0** | **File export/import** | None | Send the file | Download `.waffle`, drag-and-drop import. Always available. |
| **1** | **GitHub** | OAuth | Public repo URL | Documents as files in a user repo. Commits = version history. Natural fit for open-source CAD community. |
| **2** | **Google Drive** | OAuth | Drive sharing link | Broadest user base. Familiar sharing model. |
| **3** | **AT Protocol** | OAuth (DID) | AT URI → public record | Long-term bet. Documents as signed records in user's PDS. Public-by-default, forkable, decentralized identity. See §7.5. |

### 7.1 IndexedDB (Phase 0)

Default provider. Zero-setup, always available, offline-first.

```
Database: "waffle-iron"
  ObjectStore: "documents"
    key: string (8-char base62 document id)
    value: {
      id: string,
      json: string,        // full WaffleFile JSON
      name: string,         // denormalized for listing
      modified: number,     // epoch ms, denormalized for sorting
    }
```

Limitations: tied to one browser on one device. Clearing browser data deletes everything. This is the motivation for connecting a cloud provider — the app should gently surface this ("Your documents are only saved in this browser. Connect GitHub to back them up.").

**Autosave migration**: The current localStorage autosave (`waffle-autosave` key) migrates to IndexedDB on first load. Check for key, parse as v2, migrate to v3, store in IndexedDB, clear localStorage.

### 7.2 GitHub (Phase 1)

Documents stored as `.waffle` files in a user's GitHub repo.

**Setup**: "Connect GitHub" → OAuth popup → authorize Waffle Iron → app creates or selects a repo (default: `waffle-iron-documents`).

**Storage mapping**:
```
github.com/username/waffle-iron-documents/
  ├── gearbox-housing.waffle      (slug derived from doc name)
  ├── bracket-v2.waffle
  └── .waffle-index.json          (id→filename mapping, metadata cache)
```

**Save**: Commit the `.waffle` JSON to the repo via GitHub API. Each save = one commit. Users get version history, diff, rollback for free.

**Share**: For public repos, the share URL is:
```
https://waffle.app/doc/k7Tm4xQ2/gearbox-housing?src=https://raw.githubusercontent.com/user/repo/main/gearbox-housing.waffle
```

Anyone can open this link — the app fetches the raw JSON from GitHub.

**Why GitHub first**: The Waffle Iron audience (open-source CAD, makers, engineers who code) overlaps heavily with GitHub users. Version history via git commits is a genuine feature, not just a storage hack.

### 7.3 Google Drive (Phase 2)

Documents stored in a `Waffle Iron/` folder in the user's Google Drive.

**Setup**: "Connect Google Drive" → standard Google OAuth consent screen → app gets `drive.file` scope (can only access files it created).

**Storage mapping**: Each document is a file in the `Waffle Iron/` folder. File metadata includes the document ID in `appProperties`.

**Share**: Google Drive's native sharing. The share URL uses the Drive direct-download link as `?src=`.

### 7.4 AT Protocol (Phase 3 — future)

Documents as records in the user's Personal Data Store (PDS).

**Why AT Protocol**: It's the most philosophically aligned option for an open-source, user-first project. The user owns their data (it lives in their PDS, not on Waffle Iron's servers). Documents are public by default, discoverable, and forkable — like code on GitHub but for physical objects. The user's identity is decentralized (a DID, like a Bluesky handle).

**Lexicon** (AT Protocol schema for Waffle Iron documents):
```json
{
  "lexicon": 1,
  "id": "app.waffle.document",
  "defs": {
    "main": {
      "type": "record",
      "key": "tid",
      "record": {
        "type": "object",
        "required": ["name", "version", "data"],
        "properties": {
          "name": { "type": "string", "maxLength": 256 },
          "version": { "type": "integer" },
          "data": { "type": "blob", "accept": ["application/json"], "maxSize": 10485760 },
          "createdAt": { "type": "string", "format": "datetime" }
        }
      }
    }
  }
}
```

**Setup**: "Sign in with Bluesky" (or any AT Protocol identity provider) → OAuth with granular permissions (read/write `app.waffle.document` records only).

**Share**: AT URIs are natively resolvable across the network:
```
at://did:plc:abc123/app.waffle.document/3k7tm4xq2
→ https://waffle.app/doc/k7Tm4xQ2/gearbox-housing?src=at://did:plc:abc123/app.waffle.document/3k7tm4xq2
```

**Discovery**: Because AT Protocol records are public and indexable, a future "Explore" page could show public documents from across the network — a social feed of CAD models. Users could fork (copy) any public document into their own PDS.

### 7.5 Provider UX in the home page

The home page shows a provider selector in the sidebar or header:

```
┌─────────────────────────────────────────────────┐
│  Waffle Iron    [This Browser ▾]  [New Document]│
├─────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐      │
│  │          │  │          │  │          │      │
│  ...                                            │
```

The dropdown shows connected providers:
- **This Browser** (always present)
- **GitHub** (if connected) — shows repo name
- **Google Drive** (if connected)
- **All** — merged view across providers

"Connect a provider" link at the bottom of the dropdown opens a settings/connections panel.

### 7.6 Conflict resolution

When the same document exists locally and in a cloud provider (e.g., user connected GitHub after working locally):

- **First sync**: User chooses "Upload local documents to GitHub" or "Keep separate".
- **Ongoing**: Cloud provider is the source of truth. Local IndexedDB is a cache. Saves go to both. If offline, save locally and sync when online.
- **Conflicts** (same doc edited on two devices): Last-write-wins for Phase 1. Merge is a non-goal (CAD parametric trees don't merge well).

---

## 8. Branch Table

| Branch | Condition | Behavior |
|--------|-----------|----------|
| Open `/` , no documents | IndexedDB empty, no localStorage autosave | Home page with empty state |
| Open `/`, has documents | IndexedDB has entries | Home page with document cards |
| Open `/`, has localStorage autosave | Legacy autosave key exists | Migrate to IndexedDB, show home |
| Open `/doc/:id` , doc exists locally | ID found in IndexedDB | Load document, show editor |
| Open `/doc/:id?src=...`, doc not local | ID not in IndexedDB, `src` present | Fetch from `src` URL, store locally, show editor |
| Open `/doc/:id`, doc not found, no `src` | — | "Document not found" page with link to home |
| Click document card | — | Navigate to `/doc/:id/:slug` |
| New document | — | Create 1-tab document, navigate to `/doc/:id/untitled` |
| Switch tab | Tabs exist | Save current tab state, load new tab, full rebuild |
| Close last tab | Only 1 tab remains | Confirm dialog; if confirmed, delete tab and navigate to home |
| Add tab | — | Append new PartTab, switch to it |
| Save (Ctrl+S) | — | Serialize full document to active provider(s) |
| Load (.waffle file) | File picker | Parse, migrate if needed, store in IndexedDB, open |
| File version = 2 | v2 file loaded | Migrate v2→v3 (wrap in single tab) |
| File version = 1 | v1 file loaded | Migrate v1→v2→v3 (chain) |
| Share button | Provider supports sharing | Copy shareable URL to clipboard |
| Share button | Provider doesn't support sharing (local-only) | Prompt to export `.waffle` file or connect a provider |
| Connect GitHub | OAuth flow | Create/select repo, sync documents |
| Disconnect provider | Settings | Remove OAuth tokens, keep local copies |

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
| Document IDs are 8-char base62 | Assert on document create and URL generation |
| Preview mesh triangle count ≤ 2000 | Assert after decimation |
| v2 files load correctly as v3 | Round-trip test: save v2, load, assert single tab with correct features |
| Tab switch preserves inactive tab state | Test: create 2 tabs with features, switch between them, assert both feature trees intact |
| Autosave migrates to IndexedDB | Test: populate localStorage keys, init app, assert IndexedDB entry exists and localStorage cleared |
| `/doc/:id` resolves from local store | Test: save document, navigate to URL, assert document loads |
| `/doc/:id?src=url` fetches and stores | Test: serve JSON at URL, navigate with `?src=`, assert document appears in local store |
| Slug in URL is cosmetic | Test: `/doc/:id/wrong-slug` loads same document as `/doc/:id/correct-slug` |
| All providers implement DocumentStore | Type-check: each provider satisfies the interface |
| Save round-trips through every provider | Test per provider: save → list → load → assert equality |

---

## 11. Non-Goals (Explicit Exclusions)

- **Assemblies**: Deferred with sub-project 10. The `TabKind::Assembly` variant is not implemented.
- **Multi-user / real-time collaboration**: Not in scope. One user, one session. Sharing is read-only (recipient gets a copy).
- **Merge / conflict resolution beyond last-write-wins**: CAD parametric trees don't merge well. Not in scope.
- **Version history UI**: GitHub provides this implicitly via commits. No in-app version browser.
- **Document linking** (one document referencing another): Not in scope.
- **Fillet/chamfer/shell in tabs**: Still deferred per CLAUDE.md.
- **AT Protocol social features** (feed, likes, comments on documents): Deferred. The lexicon is designed for it, but the UX is not specified here.

---

## 12. Implementation Sequence

### Phase 0: Document model + local storage

1. **File format v3** (`crates/file-format/`): New types, v2→v3 migration, round-trip tests.
2. **Storage adapter** (`app/src/lib/storage/`): `DocumentStore` interface + IndexedDB implementation.
3. **Engine tab support** (`crates/wasm-bridge/`): `SwitchTab` message, tab-aware save/load.
4. **Routing** (`app/src/routes/`): Split into `/` (home) and `/doc/:id` (editor). URL scheme with ID + slug.
5. **Tab bar UI** (`app/src/lib/ui/`): Tab bar component, tab switching, add/remove/rename/reorder.
6. **Home page** (`app/src/routes/+page.svelte`): Document cards, create/delete/open, provider dropdown (local only initially).
7. **Preview mesh** (`crates/kernel/` or `crates/feature-engine/`): Decimation, storage in tab.
8. **3D thumbnail viewports** (`app/src/lib/ui/`): Minimal Threlte scene for document cards.
9. **Shareable URL resolution**: `?src=` parameter fetching, "document not found" page.

Steps 1–3 can be developed in parallel. Steps 4–6 form the home page. Steps 7–8 add thumbnails. Step 9 enables sharing via any direct URL.

### Phase 1: GitHub storage

10. **GitHub OAuth** (`app/src/lib/storage/github.ts`): OAuth flow, token management.
11. **GitHub DocumentStore** implementation: List/load/save/delete via GitHub API. Commit-per-save.
12. **Share via GitHub**: Generate `?src=` URLs pointing to raw GitHub content.
13. **Provider UI**: Connect/disconnect GitHub in settings. Provider dropdown in home page.

### Phase 2: Google Drive storage

14. **Google Drive OAuth + DocumentStore**: Same adapter pattern as GitHub.

### Phase 3: AT Protocol

15. **AT Protocol OAuth** (DID-based auth).
16. **Lexicon registration**: `app.waffle.document` record type.
17. **AT Protocol DocumentStore**: CRUD via PDS API.
18. **AT URI resolution**: Resolve `at://` URIs in `?src=` parameter.
