/**
 * Engine state store using Svelte 5 runes.
 *
 * Manages reactive state for the WASM engine, including
 * feature tree, mesh data, and engine status.
 */

import { base } from '$app/paths';
import * as THREE from 'three';
import { INFERENCE_SOURCES_MAX } from '$lib/config.js';
import { EngineBridge } from './bridge.js';
import { log, getLogs, exportLogs, clearLogs } from './logger.js';
import { showToast, getToasts, dismissToast, dismissAllToasts, initLoggerToasts } from '$lib/ui/toast.svelte.js';
import { extractProfiles } from '$lib/sketch/profiles.js';
import { buildFinishProfiles } from '$lib/sketch/finishProfiles.js';
import { sampleBSpline } from '$lib/sketch/bspline.js';
import { getPreview, getSnapIndicator, getSnapCandidates as _getSnapCandidates } from '$lib/sketch/sketchToolState.svelte.js';
import { resetTool, getToolState as _getToolState, getIsDragging as _getIsDragging, getPointerDownPos as _getPointerDownPos, getStartPos as _getStartPos, getStartPointId as _getStartPointId, getToolEventLog as _getToolEventLog, clearToolEventLog as _clearToolEventLog, getOffsetToolState as _getOffsetToolState } from '$lib/sketch/tools.js';
import { buildSketchPlane, sketchToScreen } from '$lib/sketch/sketchCoords.js';
import { computeConstraintBadges } from '$lib/sketch/constraintBadges.js';
import { stepConstraintModal, modalInstruction, isModalConstraint } from '$lib/sketch/constraintModalEngine.js';
import { classifyDimension } from '$lib/sketch/dimensionHeuristic.js';
import { getSetting, getSettings, updateSettings } from '$lib/ui/settings.svelte.js';
import { deleteDraft, getDraft, listDrafts, pruneDrafts, putDraft, rememberTabKey, tabKey } from '$lib/storage/drafts.js';
import { findConnectedChain, orderChain } from '$lib/sketch/chain.js';
import { resolveChainSegments, offsetChainSegments } from '$lib/sketch/offset.js';
import { isDatumPlaneRef, getPlaneIdFromRef, getPlaneById, resolvePlane, BUILTIN_PLANES } from './planes.js';
import { FORMAT_VERSION, MIN_READER_VERSION, fileTooNew } from './format.js';
import { fetchTestCases, fetchTestCase, createTestCase as apiCreateTestCase, deleteTestCase as apiDeleteTestCase } from './testCaseApi.js';

/**
 * Generate a UUID, with fallback for non-secure contexts (e.g. HTTP without localhost).
 * crypto.randomUUID() requires HTTPS or localhost; crypto.getRandomValues() works everywhere.
 * @returns {string}
 */
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
/** @param {unknown} s */
function isUuid(s) { return typeof s === 'string' && UUID_RE.test(s); }

/**
 * A point well inside a face's triangles `[start, end)` of an indexed mesh:
 * the area-weighted centroid when it lies on one of the face's triangles
 * (any convex face), else the centroid of the largest triangle. The first
 * triangle's centroid sits a third of the way in from an edge, close enough
 * to be an edge pick once the camera frames the part.
 * @param {ArrayLike<number>} vertices
 * @param {ArrayLike<number>} indices
 * @param {number} start
 * @param {number} end
 * @returns {[number, number, number]}
 */
function faceInteriorPoint(vertices, indices, start, end) {
	const p = (/** @type {number} */ i) => [vertices[i * 3], vertices[i * 3 + 1], vertices[i * 3 + 2]];
	const sub = (/** @type {number[]} */ a, /** @type {number[]} */ b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
	const cross = (/** @type {number[]} */ a, /** @type {number[]} */ b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
	const dot = (/** @type {number[]} */ a, /** @type {number[]} */ b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
	const tris = [];
	const sum = [0, 0, 0];
	let total = 0;
	for (let k = start; k + 3 <= end && k + 2 < indices.length; k += 3) {
		const a = p(indices[k]), b = p(indices[k + 1]), c = p(indices[k + 2]);
		const n = cross(sub(b, a), sub(c, a));
		const area = Math.sqrt(dot(n, n)) / 2;
		const centroid = [(a[0] + b[0] + c[0]) / 3, (a[1] + b[1] + c[1]) / 3, (a[2] + b[2] + c[2]) / 3];
		tris.push({ a, b, c, n, area, centroid });
		for (let j = 0; j < 3; j++) sum[j] += centroid[j] * area;
		total += area;
	}
	if (!tris.length) return [0, 0, 0];
	const largest = tris.reduce((m, t) => (t.area > m.area ? t : m));
	if (total <= 0) return /** @type {[number, number, number]} */ (largest.centroid);
	const g = [sum[0] / total, sum[1] / total, sum[2] / total];
	// On a triangle: close to its plane and on the inner side of all three edges.
	const onTriangle = (/** @type {any} */ t) => {
		const nn = dot(t.n, t.n);
		if (nn === 0) return false;
		const scale = Math.max(Math.sqrt(t.area), 1e-12);
		if (Math.abs(dot(sub(g, t.a), t.n)) / Math.sqrt(nn) > scale * 1e-3) return false;
		return [[t.a, t.b], [t.b, t.c], [t.c, t.a]].every(([u, w]) => dot(cross(sub(w, u), sub(g, u)), t.n) >= 0);
	};
	return /** @type {[number, number, number]} */ (tris.some(onTriangle) ? g : largest.centroid);
}

function generateUUID() {
	if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
		return crypto.randomUUID();
	}
	// Fallback: RFC 4122 v4 UUID via getRandomValues
	return ([1e7]+-1e3+-4e3+-8e3+-1e11).replace(/[018]/g, c =>
		(c ^ crypto.getRandomValues(new Uint8Array(1))[0] & 15 >> c / 4).toString(16));
}

/** @type {{ features: Array<any>, active_index: number | null }} */
let featureTree = $state({ features: [], active_index: null });

/** @type {Array<{ featureId: string, bodyId?: string, name?: string|null, outputKey?: any, outputIndex?: number, vertices: Float32Array, normals: Float32Array, indices: Uint32Array, triangleCount: number, faceRanges?: Array<{geom_ref: any, start_index: number, end_index: number}> }>} */
let meshes = $state([]);

let engineReady = $state(false);

/** @type {string | null} */
let lastError = $state(null);

/** @type {Map<string, string>} featureId -> error message */
let featureErrors = $state(new Map());
// Warnings carried by the previous modelUpdated — used to toast only warnings
// that are NEW on this rebuild (persisted diagnostics replay on every rebuild).
let lastRebuildWarnings = new Set();

let rebuildTime = $state(0);

let rebuilding = $state(false);

let statusMessage = $state('Initializing...');

/**
 * The latest rebuild progress frame of the command in flight
 * (`specs/b4_balanced_union.md` §2.3), cleared by the next model update.
 * @type {{ feature_id: string, feature_name: string, done: number, remaining: number, label: string } | null}
 */
let rebuildProgress = $state(null);

/** @type {Set<(msg: any) => void>} */
const progressListeners = new Set();

/**
 * Features whose bodies a later feature consumed (from `ModelUpdated`):
 * not live, so not a boolean operand.
 * @type {Set<string>}
 */
let consumedFeatures = $state(new Set());

/** @type {any | null} */
let hoveredRef = $state(null);

/** @type {Array<any>} */
let selectedRefs = $state([]);

/** Body id (`featureId/outputKeyTag`) currently selected via the Bodies list. */
/** @type {string | null} */
let selectedBodyId = $state(null);

/** Body id currently hovered in the Bodies list. */
/** @type {string | null} */
let hoveredBodyId = $state(null);

/** @type {{ active: boolean, origin: [number, number, number], normal: [number, number, number] }} */
// `xAxis` is the sketch's own +u direction when it carries one
// (`Sketch.plane_x_axis`); null ⇒ derived from the normal, which is every
// sketch the UI authors and every one written before 2026-09-24.
let sketchMode = $state({ active: false, origin: [0, 0, 0], normal: [0, 0, 1], xAxis: null });

/**
 * Projected-geometry bindings for the active sketch: each maps a local Point id
 * to the external model geometry it was projected from. Sent to the engine on
 * FinishSketch so rebuild can keep the point coincident with its source.
 * See specs/projected_sketch_geometry.md.
 * @type {Array<{ point_id: number, source: { geom_ref: object, kind: object } }>}
 */
let projectedBindings = $state([]);

/** @type {string | null} */
let selectedFeatureId = $state(null);

/** @type {string} */
let activeTool = $state('select');

/** @type {'rectangle' | 'rectangle-center'} Rectangle split-button selection. */
let rectMode = $state('rectangle');

// -- Sketch drawing state --

/** @type {Array<object>} */
let sketchEntities = $state([]);
let suppressProfileExtraction = false;

/** @type {Array<object>} */
let sketchConstraints = $state([]);

/** @type {Map<number, {x: number, y: number}>} */
let sketchPositions = $state(new Map());

/** @type {number} */
let nextEntityId = $state(1);

/** @type {object | null} */
let sketchSolveStatus = $state(null);

/** @type {Set<number>} */
let sketchSelection = $state(new Set());

/** @type {number | null} */
let sketchHover = $state(null);

/**
 * Constraint-modal state (constraint-FIRST application). Null when inactive.
 * `running` is the engine's running pick list (meaning depends on the
 * constraint's mode — see constraintModalEngine.js). `message` is the current
 * instruction or transient reject hint shown in the modal panel.
 * @type {{ constraintId: string, running: number[], message: string|null } | null}
 */
let constraintModal = $state(null);

/** @type {Array<{ entityIds: number[], isOuter: boolean }>} */
let extractedProfilesState = $state([]);

/** @type {number | null} */
let selectedProfileIndex = $state(null);

/** @type {number | null} Index (into sketchConstraints) of a selected constraint badge */
let selectedConstraintIndex = $state(null);

/** @type {Map<string, {dx:number, dy:number}>} Per-constraint badge display offsets (cosmetic, session-only) */
let constraintBadgeOffsets = $state(new Map());

/** @type {Map<string, {dx:number, dy:number}>} Per-dimension-label display offsets (cosmetic, session-only) */
let dimensionLabelOffsets = $state(new Map());

/** @type {number} Live sketch units per screen pixel (updated by SketchInteraction) */
let sketchPixelSize = $state(0.00001);

/** @type {number | null} */
let hoveredProfileIndex = $state(null);

/** @type {{ featureId: string, profileIndex: number } | null} */
let inactiveHoveredProfile = $state(null);

/** @type {Array<{ x: number, y: number, sourceId: string, worldPos?: [number, number, number] }>} */
let referenceSnapPoints = $state([]);

/** @type {{ x: number, y: number } | null} */
let sketchCursorPos = $state(null);

/** @type {Set<number>} Entity IDs that appear over-constrained */
let overConstrainedEntities = $state(new Set());

// -- Sketch undo/redo --

/** @type {Array<{ entities: object[], constraints: object[] }>} */
let sketchUndoStack = $state([]);
/** @type {Array<{ entities: object[], constraints: object[], cascadedConstraints?: object[] }>} */
let sketchRedoStack = $state([]);
/** @type {{ entities: object[], constraints: object[] } | null} */
let pendingSketchAction = null;

/** @type {{ sketchId: string, sketchName: string, profileCount: number, availableSketches?: Array<any>,
 *           regions: Array<{ type?: string, sketchId?: string, sketchName?: string, profileIndex?: number, geomRef?: any, label?: string }> } | null} */
let extrudeDialogState = $state(null);

/** @type {{ sketchId: string, profileIndex: number, depth: number, flipDirection: boolean, symmetric: boolean, cut: boolean } | null} */
let extrudePreviewParams = $state(null);

/** @type {{ target: 'extrude' | 'revolve' } | null} */
let profilePickMode = $state(null);

// Extrude dialog "Choose bodies" target selection, shared between the dialog and
// the viewport so a body can be picked by clicking it in 3D. `active` gates the
// viewport click branch; `ids` are the selected body ids (see getBodies()).
let extrudeTargetPickActive = $state(false);
let extrudeTargetIds = $state([]);

/** @type {boolean} */
let axisPickMode = $state(false);

/** @type {{ sketchId: string, sketchName: string, profileCount: number, selectedProfile?: any, selectedAxis?: any } | null} */
let revolveDialogState = $state(null);

/** @type {{ sketchId: string, profileIndex: number, angle: number, axisOrigin: [number,number,number], axisDir: [number,number,number] } | null} */
let revolvePreviewParams = $state(null);

/**
 * Pipe dialog (spec `specs/b2_pipe_sweep.md` checkpoint 3).
 * @type {{ sketchId: string, sketchName: string, entityIds: number[], editingFeatureId?: string, editParams?: any } | null}
 */
let pipeDialogState = $state(null);

/** Viewport pick mode for the pipe path: clicks on inactive-sketch lines/arcs toggle path entities. */
let pathPickMode = $state(false);

/** @type {{ edges: Array<any>, edgeCount: number } | null} */
let chamferDialogState = $state(null);

/** @type {{ edges: Array<any>, edgeCount: number } | null} */
let filletDialogState = $state(null);

/** @type {{ faces: Array<any>, faceCount: number } | null} */
let shellDialogState = $state(null);

/** @type {{ bodies: Array<{ featureId: string, name: string }>, operation: string } | null} */
let booleanDialogState = $state(null);
/** Part mate connector dialog (`specs/part_mate_connectors.md`); null when closed. */
let mateConnectorDialogState = $state(null);
/** The open part's named mate connectors as evaluated (`ModelUpdated.connectors`). */
let partConnectors = $state([]);

// -- Test case browser state --

/** @type {{ visible: boolean, cases: Array<object>, loading: boolean, error: string | null }} */
let testCaseBrowserState = $state({ visible: false, cases: [], loading: false, error: null });

/** @type {{ name: string, description: string, expectedOutcome: string, tags: string } | null} */
let saveTestCaseDialogState = $state(null);

/** @type {{ visible: boolean, cases: Array<object>, activeCase: string | null, activeMeta: object | null, loading: boolean, error: string | null, results: Object<string, { status: string, category: string, detail: string }> }} */
let assayBrowserState = $state({ visible: false, cases: [], activeCase: null, activeMeta: null, loading: false, error: null, results: {} });

/**
 * The Examples panel (`app/static/examples/`): the official example documents,
 * which one is open, and whether the development write endpoint is there.
 * @type {{ visible: boolean, examples: Array<object>, active: string | null, loading: boolean, error: string | null, writable: boolean, saving: boolean, opening: string | null }}
 */
let examplesBrowserState = $state({ visible: false, examples: [], active: null, loading: false, error: null, writable: false, saving: false, opening: null });
/** Set by a document open: fit the view to the first model update that carries geometry. */
let fitAllOnNextModel = false;
/** @type {ReturnType<typeof setTimeout> | null} */
let fitAllOnNextModelTimer = null;

/**
 * Frame the model once the document being opened arrives with geometry.
 * A document open replaces everything the camera was pointed at, so the
 * default camera (which frames the 200 mm datum planes) would otherwise
 * leave a 330 m tower — or a 3 mm screw — off-frame until the user hits F.
 * The flag survives however many rebuilds the open takes (an assembly
 * evaluates after the file lands) and expires so it can never fire against
 * a model the user built later.
 */
function fitAllOnDocumentOpen() {
	fitAllOnNextModel = true;
	if (fitAllOnNextModelTimer) clearTimeout(fitAllOnNextModelTimer);
	fitAllOnNextModelTimer = setTimeout(() => {
		fitAllOnNextModel = false;
		fitAllOnNextModelTimer = null;
	}, 120000);
}

/** @type {{ entityA: number, entityB: number | null, sketchX: number, sketchY: number, dimType: 'distance'|'radius'|'angle', defaultValue: number } | null} */
let dimensionPopup = $state(null);

// -- Sketch visibility and edit state --

/** @type {Map<string, boolean>} featureId -> visible (default true) */
let sketchVisibility = $state(new Map());

// -- Plane and axis visibility --

/** @type {Map<string, boolean>} planeId -> visible (default true) */
let planeVisibility = $state(new Map());

/** @type {Map<string, boolean>} axisId ('x'|'y'|'z') -> visible (default true) */
let axisVisibility = $state(new Map());

/** @type {Map<string, boolean>} bodyId -> visible (default true) */
let bodyVisibility = $state(new Map());

/**
 * Test-introspection counters: the number of edge bodies and topological
 * vertices the overlays are ACTUALLY rendering right now. The overlay
 * components publish their derived render-array lengths here so GUI tests can
 * assert visibility filtering against the real rendered output (not a
 * re-implemented filter). Not used by app logic.
 */
let renderedEdgeBodyCount = $state(0);
let renderedVertexCount = $state(0);
export function setRenderedEdgeBodyCount(n) { renderedEdgeBodyCount = n; }
export function setRenderedVertexCount(n) { renderedVertexCount = n; }

/**
 * Saved `active_index` captured when entering feature-edit mode so we can
 * restore it when the edit is applied or cancelled. `undefined` means we are
 * NOT currently in an edit-driven rollback (so restore is a no-op).
 * @type {number | null | undefined}
 */
let savedEditRollbackIndex = undefined;

/** @type {string | null} Feature ID of the sketch being edited (null = creating new) */
let editingSketchFeatureId = $state(null);

// -- Sketch plane dialog state --

let sketchPlaneDialogVisible = $state(false);
/** @type {{ origin: [number,number,number], normal: [number,number,number], label: string } | null} */
let sketchPlaneDialogSelection = $state(null);
/**
 * When true, the sketch-plane dialog opens straight into the datum-plane
 * (offset) creation flow rather than the plane-selection flow. Used by the
 * standalone "Datum Plane" toolbar entry so a datum plane can be created
 * without starting a sketch.
 */
let sketchPlaneDialogStartInOffset = $state(false);

// -- Inline sketch plane selection mode --

let sketchPlaneSelectionMode = $state(false);

/** Configurable snap thresholds */
let snapSettings = $state({
	coincidentPx: 8,
	onEntityPx: 5,
	hvAngleDeg: 3,
	previewPx: 30
});

// Point-alignment inference sources: sketch points recently hovered (armed) so
// their horizontal/vertical axes can be inferred while placing a new point.
// Most-recent first, LRU-capped at INFERENCE_SOURCES_MAX, deduped by point id.
// Cleared on sketch exit and tool switch. See specs/snap_inference_and_priority.md.
/** @type {Array<{ id: number, x: number, y: number }>} */
let inferenceSources = $state([]);

// -- Camera state refs (set by CameraControls) --

/** @type {import('three').PerspectiveCamera | import('three').OrthographicCamera | null} */
let cameraObject = null;

/** @type {any | null} OrbitControls ref */
let controlsObject = null;
/** The three.js scene and renderer, for `getRenderStats()`. */
let sceneObject = null;
let rendererObject = null;

// -- Camera projection state --

/** @type {'orthographic' | 'perspective'} */
let cameraProjection = $state('orthographic');

/** @type {string} CSS matrix3d() transform string synced from CameraControls each frame */
let viewCubeTransform = $state('');

// -- Box selection state --

/** @type {{ active: boolean, startX: number, startY: number, endX: number, endY: number, mode: 'window'|'crossing' }} */
let boxSelectState = $state({ active: false, startX: 0, startY: 0, endX: 0, endY: 0, mode: 'window' });

// -- Select Other cycle state --

/** @type {{ intersections: Array<any>, cycleIndex: number, lastScreenX: number, lastScreenY: number }} */
let selectOtherState = $state({ intersections: [], cycleIndex: 0, lastScreenX: -1, lastScreenY: -1 });

// -- Two-finger touch gesture state --
let twoFingerActive = $state(false);
/**
 * A rotate gesture is in progress (mouse drag, one-finger touch, or the View
 * Cube's drag-orbit). Published by `CameraControls` so the viewport can show
 * the point being turned about while — and only while — it is being used.
 */
let orbitActive = $state(false);

// -- Section view state --

/**
 * Capped section-view state. When `active`, the solid bodies are clipped at
 * `plane` and the cut is capped (stencil fill) so the model reads as a solid
 * section. `plane` is stored as plain origin/normal arrays (the captured
 * section plane). `flipped` keeps the opposite half; `offset` shifts the cut
 * along the normal (meters).
 * @type {{ active: boolean, plane: { origin: [number,number,number], normal: [number,number,number] } | null, flipped: boolean, offset: number }}
 */
let sectionState = $state({ active: false, plane: null, flipped: false, offset: 0 });

// -- Mobile layout state --

let isMobileLayout = $state(false);
/** @type {'left' | 'right' | null} */
let mobileActivePanel = $state(null);

/** @type {string} */
let projectName = $state('Untitled');

/** @type {string} Document display unit (mm, cm, m, in, ft) */
let documentDisplayUnit = $state('mm');

// -- Document model state --

/** @type {string | null} Active document ID (from IndexedDB) */
let activeDocId = $state(null);

/** @type {string | null} Active tab ID within the document */
let activeTabId = $state(null);

/**
 * Metadata for all tabs in the current document.
 * Each tab stores its own feature tree snapshot for save/restore on switch.
 * @type {Array<{ id: string, name: string, kind: { type: string, features: object } }>}
 */
let documentTabs = $state([]);

/** @type {string} Human-readable document name */
let documentName = $state('Untitled');
/**
 * Provenance of a document opened from a share link
 * (specs/waffle_v4_document_model.md §7.1, `$lib/storage/open-link.js`
 * `DocumentLink`): locator + resolved commit + content hash. Non-null ⇒ the
 * document is READ-ONLY here — autosave and Ctrl+S are refused; `forkLinkedDocument`
 * is the way to an editable copy. Null for every document of the user's own.
 * @type {import('$lib/storage/open-link.js').DocumentLink | null}
 */
let documentLink = $state(null);
/** v4 `document.id` (specs/waffle_v4_document_model.md §2.1): the document's
 *  own identity, independent of the storage record. Latched at open (minted
 *  once for legacy files) so every save in the session carries the same id. */
let documentId = $state(null);
/** Tab id used when a doc-less editor session (plain `/`) is saved: minted
 *  once so consecutive saves agree (v4: new tabs are UUIDs). */
let implicitTabId = null;

/**
 * The document's original `created` timestamp (ISO string), adopted from the
 * file/storage doc on open. `null` until a document is opened; the writer
 * falls back to "now" (an unsaved fresh session's first save IS its creation).
 * Without this the writer used to stamp `created: now` on every save,
 * destroying the creation time (docs/FILE_FORMAT.md §14.2).
 * @type {string | null}
 */
let documentCreated = null;

// -- Gear state --

/** @type {Map<number, object>} gearId -> GearParams */
let gearRegistry = $state(new Map());

/** @type {Map<number, number>} entityId -> gearId */
let entityToGearMap = $state(new Map());

/**
 * Ephemeral, NON-persisted expansion of each gear's compact `Gear` entity into
 * the primitives used purely for display (rendering). Keyed by gearId. The
 * canonical/persisted representation is the single `SketchEntity::Gear` in
 * `sketchEntities`; this map is derived from it (rebuilt on create and on sketch
 * load) and never saved. The primitive ids are remapped into a per-gear high
 * range (see `gearDisplayIdBase`) so they never collide with real sketch entity
 * ids or with another gear's primitives.
 * @type {Map<number, { entities: object[], positions: Map<number, {x:number,y:number}>, pitchRadius: number }>}
 */
let gearDisplay = $state(new Map());

/** Per-gear id offset for ephemeral display primitives (kept far above real entity ids). */
function gearDisplayIdBase(gearId) {
	return 10_000_000 + gearId * 100_000;
}

/** @type {object | null} */
let gearDialogState = $state(null);

/**
 * Sprocket dialog state — null when closed, else the object the placement
 * tool (or the double-click edit gesture) seeded it with:
 * `{ centerX, centerY, rotationOffset, editGearId?, params? }`. Mirrors
 * `gearDialogState`; the sprocket lives in the same registry as gears.
 * @type {object | null}
 */
let sprocketDialogState = $state(null);

/**
 * Import-STEP edit dialog state — null when closed, else `{ featureId }`.
 * The dialog reads the feature's current ImportedBody params from the tree.
 * @type {object | null}
 */
let importDialogState = $state(null);

/**
 * Planetary dialog state — null when closed, else an object seeded by the
 * placement tool: `{ centerX, centerY }` (internal sketch coords). Mirrors
 * `gearDialogState`.
 * @type {object | null}
 */
let planetaryDialogState = $state(null);

/** @type {number} */
let nextGearId = $state(1);

/** @type {number | null} */
let autoSaveTimer = null;

/**
 * What a reload can bring back (`findStartupRestore`), while it is offered or
 * being reopened.
 * @type {{ available: boolean, timestamp: number, source: 'legacy' | 'draft' | 'indexeddb', docId?: string, draftKey?: string, name?: string } | null}
 */
let autoRestoreState = $state(null);

/** Resolves once startup has reopened, offered-and-answered, or skipped a restore. */
let settleStartupRestore = () => {};
let startupRestorePending = true;
const startupRestoreSettled = new Promise((resolve) => {
	settleStartupRestore = () => {
		startupRestorePending = false;
		resolve(undefined);
	};
});

/** True while `openDocumentRecord` is loading a document into the engine. */
let documentLoadPending = false;

/**
 * Why the model cannot be read or edited yet, or null: `'restoring'` until this
 * tab's startup restore is settled, `'loading'` while a document is still
 * loading into the engine (a multi-boolean document rebuilds for minutes).
 * Meanwhile the store's feature tree is the blank bootstrap or the previous
 * document, so anything reading it would describe the wrong model (failure log F10).
 * @returns {'restoring' | 'loading' | null}
 */
export function getDocumentLoadBusyReason() {
	if (documentLoadPending) return 'loading';
	if (startupRestorePending) return 'restoring';
	return null;
}

/**
 * Resolves when this tab's startup restore is settled: reopened (policy
 * `auto`), answered in the dialog (`ask`), or nothing to restore. The agent
 * link resumes only after it, so a resumed agent never edits the blank
 * bootstrap document of a tab that is about to reopen its work.
 * @returns {Promise<void>}
 */
export function whenStartupRestoreSettled() {
	return startupRestoreSettled;
}

/** Autosave on hide/unload is installed once per page. */
let autosaveLifecycleInstalled = false;

/** @type {EngineBridge | null} */
let bridge = null;

/** Get the engine bridge instance (or null if not initialized). */
export function getBridge() { return bridge; }

// -- Engine lock (agent link: specs/waffle_mcp_server.md §2.7, I6) --
// The bridge pairs responses FIFO, which keeps each MESSAGE safe; the lock
// keeps whole agent CALLS safe. Every user-originated send (all but pointer
// hover/select) holds the lock from send to response through the bridge's
// send gate. The agent executor holds it for its whole call and posts
// ungated (`sendAgentMessage`). Waiters are served FIFO.

/** Tail of the FIFO lock queue: resolves when the last queued holder releases. */
let engineLockTail = Promise.resolve();
/** @type {'user' | 'agent' | null} */
let engineLockHolder = $state(null);

/** A lock acquisition that gave up waiting (spec G2). `holder` is who held it then. */
export class EngineLockTimeout extends Error {
	/** @param {'user' | 'agent' | null} holder */
	constructor(holder) {
		super(`the engine lock is held by ${holder ?? 'a queued action'}`);
		this.holder = holder;
	}
}

/** Who holds the engine lock right now. */
export function getEngineLockHolder() {
	return engineLockHolder;
}

/**
 * @param {'user' | 'agent'} origin
 * @param {number | undefined} timeoutMs - give up (reject EngineLockTimeout) after this long
 * @returns {Promise<() => void>} the release function
 */
function acquireEngineLock(origin, timeoutMs) {
	/** @type {() => void} */
	let release = () => {};
	const released = new Promise((r) => (release = r));
	const prev = engineLockTail;
	engineLockTail = prev.then(() => released);
	return new Promise((resolve, reject) => {
		let settled = false;
		const timer =
			timeoutMs != null
				? setTimeout(() => {
						if (settled) return;
						settled = true;
						reject(new EngineLockTimeout(engineLockHolder));
					}, timeoutMs)
				: null;
		prev.then(() => {
			if (settled) {
				release(); // abandoned slot: pass the lock straight on
				return;
			}
			settled = true;
			if (timer) clearTimeout(timer);
			engineLockHolder = origin;
			resolve(() => {
				engineLockHolder = null;
				release();
			});
		});
	});
}

/**
 * Run `fn` holding the engine lock.
 * @template T
 * @param {'user' | 'agent'} origin
 * @param {() => Promise<T>} fn
 * @param {{ timeoutMs?: number }} [opts]
 * @returns {Promise<T>}
 */
export async function withEngineLock(origin, fn, opts = {}) {
	const unlock = await acquireEngineLock(origin, opts.timeoutMs);
	try {
		return await fn();
	} finally {
		unlock();
	}
}

const UUID_PATTERN = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g;

/**
 * O3 parity oracle (specs/waffle_mcp_server.md §5): replay agent bridge
 * messages recorded in another page through the agent entry point, under the
 * agent lock. Feature ids the recording page's engine minted are mapped to the
 * ids this engine mints, in answer order (also inside body ids such as
 * `{feature_id}/Main`). A rejection is replayed as a rejection. Test-only,
 * exposed on `__waffle`.
 * @param {Array<{ message: object, response?: { type: string, feature_id: string | null } }>} entries
 */
async function replayEngineMessages(entries) {
	await withEngineLock('agent', async () => {
		/** @type {Map<string, string>} */
		const ids = new Map();
		const remap = (v) =>
			typeof v === 'string'
				? v.replace(UUID_PATTERN, (u) => ids.get(u) ?? u)
				: Array.isArray(v)
					? v.map(remap)
					: v && typeof v === 'object'
						? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, remap(x)]))
						: v;
		agentActivity = { tool: 'replay', agentName: 'replay', quietErrors: true };
		try {
			for (const entry of entries) {
				let response = null;
				try {
					response = await sendAgentMessage(remap(entry.message), { rebuild: true });
				} catch {
					// The recording page's engine rejected the same message.
				}
				// An authoring tool answers with a `ToolResult`, which keeps the
				// id of any feature it created in the MCP payload rather than at
				// the top level (S3 C4) — on BOTH sides of this mapping: the
				// recorded answer and the one this replay just got back. Miss
				// either and no id is learned, every later step replays against
				// a stale one, and the `catch` above swallows the failure.
				const recorded = entry.response?.feature_id;
				const replayed = response?.feature_id ?? response?.structuredContent?.feature_id;
				if (recorded && replayed) ids.set(recorded, replayed);
			}
		} finally {
			agentActivity = null;
		}
	});
}

/** Status-bar hint shown when the user tries a modeling command during an agent call (G8). */
export const AGENT_WORKING_HINT = 'Agent is working';

/**
 * The running agent-link call, or null. While set, modeling commands in the UI
 * are refused with AGENT_WORKING_HINT, and per-feature rebuild-error toasts are
 * left to the executor (one toast per agent step, spec A2/A3).
 * `quietErrors` is set by authoring calls, which toast their own step.
 * @type {{ tool: string, agentName: string, quietErrors?: boolean } | null}
 */
let agentActivity = $state(null);
export function getAgentActivity() {
	return agentActivity;
}
/** @param {{ tool: string, agentName: string, quietErrors?: boolean } | null} activity */
export function setAgentActivity(activity) {
	agentActivity = activity;
}

/**
 * Why a user interaction currently blocks agent EDITS (spec §2.7 busy states,
 * G3), or null. Agent queries stay allowed.
 * @returns {'sketch_mode' | 'feature_dialog' | 'edit_context' | null}
 */
export function getUserBusyReason() {
	if (sketchMode.active) return 'sketch_mode';
	if (editContext) return 'edit_context';
	if (
		extrudeDialogState ||
		revolveDialogState ||
		booleanDialogState ||
		mateConnectorDialogState ||
		chamferDialogState ||
		filletDialogState ||
		shellDialogState ||
		importDialogState ||
		sketchPlaneDialogVisible ||
		sketchPlaneSelectionMode
	) {
		return 'feature_dialog';
	}
	return null;
}

/**
 * The agent link's engine entry point (spec §2.7). Unlike the store's user
 * actions it swallows nothing: it resolves with the EngineToUi response (after
 * the store's own handlers updated tree, meshes, autosave) and rejects with the
 * bridge's typed Error (`kind`, `needsRestart`). The caller must hold the engine
 * lock as 'agent'.
 * @param {object} message - UiToEngine message (plain data)
 * @param {{ rebuild?: boolean }} [opts] - `rebuild` shows the rebuild spinner
 * @returns {Promise<any>}
 */
export async function sendAgentMessage(message, { rebuild = false } = {}) {
	if (!bridge) throw new Error('Engine not initialized');
	if (engineLockHolder !== 'agent') {
		throw new Error('sendAgentMessage needs the engine lock held by the agent');
	}
	if (!rebuild) return bridge.sendUngated(message);
	rebuilding = true;
	const t0 = performance.now();
	try {
		const result = await bridge.sendUngated(message);
		rebuildTime = performance.now() - t0;
		return result;
	} finally {
		rebuilding = false;
	}
}

/** The worker trapped and could not restart (`needsRestart`); only a reload recovers. */
let engineCrashed = $state(false);

/** True once the engine worker has crashed beyond auto-restart (agent link G6 `EngineCrashed`). */
export function isEngineCrashed() {
	return engineCrashed;
}

/** True on the `/view` route: the model is streamed from a host; no engine runs here. */
let viewerMode = $state(false);
export function isViewerMode() {
	return viewerMode;
}

/**
 * Draw a host's snapshot (`$lib/viewer/link.js`, specs/waffle_server_mode.md
 * §4): the same mirrors a `ModelUpdated` fills — the tree, the bodies, the
 * tab bar, the sources, the assembly status, the errors — and none of the
 * editor's side effects: no autosave (the document is the host's), no
 * thumbnail, no toasts (the errors are the tree's badges).
 * @param {any} snapshot - the host's `snapshot` frame
 * @param {any[]} viewerMeshes - the decoded bodies, in the worker's shape
 */
export function applyViewerSnapshot(snapshot, viewerMeshes) {
	viewerMode = true;
	if (snapshot.tree) featureTree = snapshot.tree;
	meshes = viewerMeshes;
	mirrorSessionDocument(snapshot.document);
	documentSources = snapshot.sources ?? [];
	entityMetaCache.clear();
	assemblyStatus = snapshot.assembly
		? { errors: [], warnings: [], parts: [], connectors: [], part_connectors: [], ...snapshot.assembly }
		: null;
	consumedFeatures = new Set(snapshot.consumed_features ?? []);
	lastError = null;
	rebuildProgress = null;
	statusMessage = `Viewing (${meshes.length} ${meshes.length === 1 ? 'body' : 'bodies'})`;
	const errors = new Map();
	for (const e of snapshot.errors ?? []) {
		if (e && typeof e === 'object') errors.set(e.feature_id, e.message);
	}
	featureErrors = errors;
	lastRebuildWarnings = new Set(snapshot.warnings ?? []);
}

/**
 * Initialize the engine bridge and WASM worker.
 */
export async function initEngine() {
	if (bridge) return;

	bridge = new EngineBridge();
	bridge.setSendGate((message, post) => withEngineLock('user', post));

	bridge.on('modelUpdated', (msg) => {
		if (msg.feature_tree) {
			featureTree = msg.feature_tree;
		}
		if (msg.meshes) {
			meshes = msg.meshes;
		}
		// A load that asked to be framed (any document open): the first model
		// with geometry is the one to fit, however many rebuilds the open
		// took (an assembly evaluates after the file lands).
		if (fitAllOnNextModel && meshes.some((m) => m.triangleCount > 0)) {
			fitAllOnNextModel = false;
			if (fitAllOnNextModelTimer) {
				clearTimeout(fitAllOnNextModelTimer);
				fitAllOnNextModelTimer = null;
			}
			setTimeout(() => window.dispatchEvent(new Event('waffle-fit-all')), 50);
		}
		// The tab bar, the active tab and the document's metadata are MIRRORS
		// of the session now (S2 C4, invariant A2.1): the engine is
		// authoritative and the store must not keep a copy that can diverge.
		// This replaces the id reconciliation C3a needed, which existed only
		// because the store minted tab ids of its own.
		mirrorSessionDocument(msg.document);
		// The document's `sources` table with availability (v4 §2.3) — the
		// Sources panel's data; absent on the wire when the table is empty.
		documentSources = msg.sources ?? [];
	entityMetaCache.clear();
		// Assembly evaluation (v4 Phase 3b): solved placements are derived
		// hints written back into the tab so they are saved with it.
		// Empty arrays are omitted on the wire; give the UI a stable shape.
		assemblyStatus = msg.assembly ? { errors: [], warnings: [], parts: [], connectors: [], part_connectors: [], ...msg.assembly } : null;
		partConnectors = msg.connectors ?? [];
		// In-context editing (v4 Phase 3d-4): present while a Part is open in
		// an assembly's context; the engine drops it on any tab switch.
		editContext = msg.context ? { instances: [], errors: [], warnings: [], ...msg.context } : null;
		// (The solved placements used to be written back into the store's tab
		// copy here so they would be saved with it. The engine records them
		// into its own tab now — S2 C3c — so this copy would only be a second
		// one that can disagree.)
		lastError = null;
		rebuildProgress = null;
		consumedFeatures = new Set(msg.consumed_features ?? []);
		statusMessage = `Model updated (${meshes.length} ${meshes.length === 1 ? 'body' : 'bodies'})`;

		// The thumbnail saved with the tab is the engine's decimated preview.
		// Never the full render mesh: copied into this $state tab, it was
		// serialized into every autosave (a 10 MB preview in a 44-feature document).
		if (activeTabId) {
			const tab = documentTabs.find(t => t.id === activeTabId);
			if (tab) tab.kind.preview_mesh = msg.preview_mesh ?? null;
		}

		scheduleAutoSave();
		log('engine', 'Model updated', { meshCount: meshes.length, featureCount: featureTree?.features?.length ?? 0 });

		// Track feature errors for tree display. Persisted feature diagnostics
		// replay on EVERY rebuild (each sketch edit triggers one), so only
		// errors that are new or changed since the previous rebuild are logged
		// and toasted — the tree badge carries the persistent state. The gate
		// must cover log('error') too: initLoggerToasts turns every error log
		// entry into a second toast.
		const prevErrors = featureErrors;
		const newErrors = new Map();
		if (msg.errors && msg.errors.length > 0) {
			for (const [featureId, errorMsg] of msg.errors) {
				newErrors.set(featureId, errorMsg);
				if (prevErrors.get(featureId) !== errorMsg) {
					if (agentActivity?.quietErrors) {
						// The agent executor toasts its step once (rolled back / kept, spec A2/A3).
						log('engine', `Feature ${featureId} failed during agent call: ${errorMsg}`);
					} else {
						log('error', `Feature ${featureId} failed: ${errorMsg}`);
						showToast('error', `Feature failed: ${errorMsg}`);
					}
				}
			}
		}
		featureErrors = newErrors;

		// Surface non-fatal warnings (e.g. auto-union fallback) — again only
		// when new relative to the previous rebuild, so a warning baked into a
		// persisted feature (e.g. "body created as standalone") toasts once
		// when it first appears, not on every rebuild thereafter.
		const warnings = new Set(msg.warnings ?? []);
		for (const warning of warnings) {
			if (!lastRebuildWarnings.has(warning)) {
				log('warning', warning);
				showToast('warning', agentActivity ? `${agentActivity.agentName}: ${warning}` : warning);
			}
		}
		lastRebuildWarnings = warnings;
	});

	bridge.on('sketchSolved', (msg) => {
		// sketch_create solves through the same bridge; that result belongs to the
		// agent call, not to the user's (inactive) sketch state.
		if (agentActivity) return;
		// The Rust engine sends { solved: { positions, profiles, status: SolveStatus } }
		// where SolveStatus is { type: 'FullyConstrained' } | { type: 'UnderConstrained', dof }
		// | { type: 'OverConstrained', conflicts } | { type: 'SolveFailed', reason }.
		// Normalize to the flat format the rest of the store expects.
		const solved = msg.solved || msg;
		const statusObj = solved.status || { type: msg.status || 'unknown' };
		const statusStr = statusObj.type || (typeof statusObj === 'string' ? statusObj : 'unknown');
		const dof = statusObj.dof ?? msg.dof ?? -1;
		const positions = solved.positions || msg.positions;
		// `conflicts` are indices into the DRIVING constraint list the solver
		// saw — triggerSolve excludes reference dimensions — so translate them
		// back to sketchConstraints indices with the same filter, or badge
		// highlighting shifts onto the wrong constraints whenever a reference
		// dim precedes a conflict.
		const failedDriving = statusObj.conflicts || msg.failed || [];
		const drivingToLocal = [];
		sketchConstraints.forEach((c, i) => {
			if (!c.reference) drivingToLocal.push(i);
		});
		const failed = failedDriving
			.map((j) => drivingToLocal[j])
			.filter((i) => i != null);

		// Apply-gate: a SolveFailed result is not a solution — the solver echoes
		// its input positions for that status (sketch_drag_stability.md B3/I4),
		// so applying would only clobber fresher local drag state with a stale
		// round-trip. Non-finite coordinates (any future divergence class) are
		// equally barred from becoming sketch state: they poison hit-testing
		// and drive the auto-fit camera to infinity.
		const applyPositions =
			positions &&
			statusStr !== 'not_ready' &&
			statusStr !== 'solver_not_ready' &&
			statusStr !== 'SolveFailed' &&
			Object.values(positions).every((pos) => {
				const x = pos.x !== undefined ? pos.x : pos[0];
				const y = pos.y !== undefined ? pos.y : pos[1];
				return Number.isFinite(x) && Number.isFinite(y);
			});
		if (positions && !applyPositions) {
			log('warning', `Solve result not applied (status=${statusStr})`);
		}
		if (applyPositions) {
			const newPositions = new Map();
			for (const [id, pos] of Object.entries(positions)) {
				const p = pos.x !== undefined ? pos : { x: pos[0], y: pos[1] };
				newPositions.set(Number(id), p);
			}
			sketchPositions = newPositions;

			// Apply solved RADII to circle entities. The radius is a standalone
			// solver param (not a point), so a Diameter/Radius constraint's result
			// arrives here in `radii`, not `positions` — without this the solved
			// circle radius is silently discarded (the resize never shows).
			const radii = solved.radii || msg.radii;
			if (radii && Object.keys(radii).length > 0) {
				let changed = false;
				const updated = sketchEntities.map((e) => {
					if (e.type === 'Circle' && e.id != null && radii[e.id] != null
						&& Math.abs((e.radius ?? 0) - radii[e.id]) > 1e-12) {
						changed = true;
						return { ...e, radius: radii[e.id] };
					}
					return e;
				});
				if (changed) sketchEntities = updated;
			}

			reExtractProfiles();
		}

		sketchSolveStatus = {
			status: statusStr,
			dof,
			failed,
			solveTime: msg.solveTime
		};
		recomputeOverConstrained();
		log('engine', 'Sketch solved', { status: statusStr, dof });
	});

	bridge.on('progress', (msg) => {
		rebuildProgress = msg;
		statusMessage = `${msg.feature_name}: ${msg.label}`;
		for (const cb of progressListeners) {
			try {
				cb(msg);
			} catch (err) {
				log('error', `Progress listener failed: ${err}`);
			}
		}
	});

	bridge.on('error', (msg) => {
		lastError = msg.message;
		statusMessage = `Error: ${msg.message}`;
		if (msg.needsRestart) {
			engineReady = false;
			engineCrashed = true;
			statusMessage = 'Engine crashed — restart failed. Reload the page.';
		}
	});

	log('system', 'Engine init started');
	try {
		statusMessage = 'Loading WASM engine...';
		await bridge.init(`${base}/pkg/wasm_bridge.js`);
		engineReady = true;
		lastError = null;
		statusMessage = 'Engine ready';
		log('system', 'Engine ready (WASM loaded)');
		initLoggerToasts();

		// A route handoff in flight (/doc/[id], /open) is an EXPLICIT open;
		// offering to restore some other document on top of it is wrong (and
		// the /open route's freshly written linked record would otherwise be
		// "the newest doc" — the dialog then sits over the read-only banner).
		const handoffPending =
			typeof sessionStorage !== 'undefined' && !!sessionStorage.getItem('waffle-active-doc');

		// What a reload may bring back, per the restoreOnReload setting: reopened
		// at the end of startup (`auto`) or offered in AutoRestoreDialog (`ask`).
		const restorePolicy = getSetting('restoreOnReload');
		if (!handoffPending && restorePolicy !== 'never') {
			autoRestoreState = await findStartupRestore();
		}
		pruneDrafts().catch(() => {});
		installAutosaveLifecycle();

		// Ensure activeDocId is set so saveToProvider() works on direct `/`
		// navigation. v4 P2-5: the storage record is keyed by the document's
		// own identity, so mint that identity now and use it as the key.
		if (!activeDocId) {
			if (!documentId) documentId = generateUUID();
			activeDocId = documentId;
		}

		// (The store used to mint a default tab here for the doc-less `/`
		// session. The engine's session already HAS that tab — one empty Part
		// tab — and the first `ModelUpdated` mirrors it (S2 C4). Minting one
		// as well put a tab the engine never had beside the mirrored one, and
		// the tab bar rendered two.)

		if (autoRestoreState && restorePolicy === 'auto') {
			if (await restoreAutoSave()) {
				showToast('info', `Reopened your last work: ${documentName}`);
			}
		} else if (!autoRestoreState) {
			settleStartupRestore();
		}
	} catch (err) {
		settleStartupRestore();
		lastError = /** @type {Error} */ (err).message;
		statusMessage = `Failed to load engine: ${lastError}`;
		log('error', `Engine init failed: ${lastError}`);
	}

	// Expose debug/test API for browser console and Playwright tests
	if (typeof window !== 'undefined') {
		window.__waffle = {
			getState: () => ({
				engineReady,
				sketchMode: { ...sketchMode },
				activeTool,
				rectMode,
				entityCount: sketchEntities.length,
				lastError,
				statusMessage,
				planetaryDialog: planetaryDialogState ? { ...planetaryDialogState } : null,
			}),
			getEntities: () => [...sketchEntities],
			getPositions: () => new Map(sketchPositions),
			enterSketch: (origin, normal) => enterSketchMode(origin, normal),
			exitSketch: () => exitSketchMode(),
			setTool: (tool) => setActiveTool(tool),
			finishSketch: () => finishSketch(),
			getFeatureTree: () => featureTree,
			getSelectedFeatureId: () => selectedFeatureId,
			getParameters: () => JSON.parse(JSON.stringify(getParameters())),
			setParameters: (params) => setParameters(params),
			evaluateExpression: (expr) => evaluateExpression(expr),
			getMeshes: () => meshes.map(m => ({
				featureId: m.featureId,
				bodyId: m.bodyId ?? null,
				instanceId: m.instanceId ?? null,
				instancePath: m.instancePath ? [...m.instancePath] : null,
				instanceName: m.instanceName ?? null,
				leafPartTabId: m.leafPartTabId ?? null,
				leafPartSourceId: m.leafPartSourceId ?? null,
				context: m.context === true,
				transform: m.transform ? JSON.parse(JSON.stringify(m.transform)) : null,
				vertexCount: m.vertices?.length / 3,
				triangleCount: m.triangleCount,
				hasNormals: m.normals?.length > 0,
				hasIndices: m.indices?.length > 0,
				faceRangeCount: m.faceRanges?.length ?? 0,
				faceRanges: (m.faceRanges || []).map(r => ({
					geom_ref: r.geom_ref,
					start_index: r.start_index,
					end_index: r.end_index,
					created_by_feature: r.created_by_feature ?? null,
					plane: r.plane ?? null,
				})),
				edgeRanges: (m.edges?.ranges || []).map(r => ({
					geom_ref: r.geom_ref,
					start_index: r.start_index,
					end_index: r.end_index,
					curve: r.curve ?? null,
				})),
			})),
			getRenderedOverlayCounts: () => ({
				edgeBodies: renderedEdgeBodyCount,
				vertices: renderedVertexCount,
			}),
			// Where the overlays actually draw: each edge LineSegments object's
			// world position per body, and the world bounds of the vertex
			// points. An assembly's overlays must follow the instance
			// placements the faces follow.
			getRenderedOverlayPlacements: () => {
				const scene = cameraObject?.parent;
				const edges = [];
				let vertexBounds = null;
				if (!scene) return { edges, vertexBounds };
				scene.updateMatrixWorld(true);
				const p = new THREE.Vector3();
				scene.traverse((obj) => {
					if (obj.isLineSegments && obj.userData?.waffleType === 'edges') {
						obj.getWorldPosition(p);
						edges.push({ bodyId: obj.userData.bodyId, position: p.toArray() });
					} else if (obj.isPoints && obj.geometry) {
						obj.geometry.computeBoundingBox();
						const bb = obj.geometry.boundingBox?.clone().applyMatrix4(obj.matrixWorld);
						if (bb) vertexBounds = { min: bb.min.toArray(), max: bb.max.toArray() };
					}
				});
				return { edges, vertexBounds };
			},
			getMeshBoundingBox: () => {
				const min = [Infinity, Infinity, Infinity];
				const max = [-Infinity, -Infinity, -Infinity];
				let hasVerts = false;
				for (const m of meshes) {
					if (!m.vertices || m.vertices.length < 3) continue;
					for (let i = 0; i < m.vertices.length; i += 3) {
						hasVerts = true;
						for (let a = 0; a < 3; a++) {
							if (m.vertices[i + a] < min[a]) min[a] = m.vertices[i + a];
							if (m.vertices[i + a] > max[a]) max[a] = m.vertices[i + a];
						}
					}
				}
				if (!hasVerts) return null;
				return {
					min, max,
					center: [(min[0]+max[0])/2, (min[1]+max[1])/2, (min[2]+max[2])/2],
					size: [max[0]-min[0], max[1]-min[1], max[2]-min[2]],
				};
			},
			computeFacePlane: (geomRef) => computeFacePlane(geomRef),
			applyExtrude: (depth, profileIndex, cut, opts) => applyExtrude(depth, profileIndex, cut, opts),
			showExtrudeDialog: () => showExtrudeDialog(),
			showDatumPlaneDialog: () => showDatumPlaneDialog(),
			/**
			 * Resolve a plane (by feature id, including offset-face datums) to
			 * its rendered origin + normal, using the live face resolver — the
			 * same path DatumVis renders with. Test verification helper.
			 */
			resolvePlaneById: (id) => {
				const features = featureTree?.features ?? [];
				const plane = getPlaneById(id, features);
				if (!plane) return null;
				try {
					return resolvePlane(plane.definition, features, computeFacePlane);
				} catch {
					return null;
				}
			},
			saveProject: () => saveProject(),
			// Test SETUP only (the pick-mode interaction has its own specs):
			// sets the revolve dialog's axis as a viewport pick would.
			setRevolveAxis: (origin, direction, label) => setRevolveAxis(origin, direction, label),
			loadProject: (jsonData) => loadProject(jsonData),
			// Agent-link load gate (agent-document-load-gate.spec.js, failure log F10).
			buildDocumentJson: () => buildDocumentJson(),
			openDocumentRecord: (docId, json) => openDocumentRecord(docId, json),
			getDocumentLoadBusyReason: () => getDocumentLoadBusyReason(),
			// Test SETUP: real file pickers can't be driven from Playwright.
			importStepFromText: (fileName, text) => importStepFromText(fileName, text),
			importStepFromLink: (url) => importStepFromLink(url),
			importKicadFromText: (fileName, text) => importKicadFromText(fileName, text),
			linkKicadFromLink: (url) => linkKicadFromLink(url),
			showImportLinkDialog: (kind) => showImportLinkDialog(kind),
			queryEntityMeta: (bodyId, instancePath) => queryEntityMeta(bodyId, instancePath),
			getEntityCard: () => (getEntityCard() ? JSON.parse(JSON.stringify(getEntityCard())) : null),
			getEntityDetail: () => (entityDetail ? JSON.parse(JSON.stringify(entityDetail)) : null),
			resolveDocumentSources: () => resolveDocumentSources(),
			listSources: async () => (await bridge.send({ type: 'ListSources' }))?.sources ?? [],
			getSources: () => JSON.parse(JSON.stringify(documentSources)),
			// Assemblies (v4 Phase 3)
			getAssembly: () => { const a = getAssembly(); return a ? JSON.parse(JSON.stringify(a)) : null; },
			getAssemblyStatus: () => assemblyStatus ? JSON.parse(JSON.stringify(assemblyStatus)) : null,
			addTab: (kind) => addTab(kind),
			moveTab: (tabId, index) => moveTab(tabId, index),
			switchTab: (id) => switchTab(id),
			refreshAssembly: () => refreshAssembly(),
			// In-context editing (v4 Phase 3d-4)
			openPartInContext: (path) => openPartInContext(path),
			updateEditContext: () => updateEditContext(),
			exitEditContext: () => exitEditContext(),
			getEditContext: () => editContext ? JSON.parse(JSON.stringify(editContext)) : null,
			// Test SETUP: start a sketch on a face by its ref (the plane the
			// toolbar's Sketch button would compute for the selected face).
			enterSketchOnFace: async (ref) => {
				const plane = computeFacePlane(ref);
				if (!plane) return false;
				await enterSketchMode(plane.origin, plane.normal, ref);
				return true;
			},
			addInstance: (opts) => addInstance(opts),
			updateInstance: (id, patch) => updateInstance(id, patch),
			removeInstance: (id) => removeInstance(id),
			addConnector: (opts) => addConnector(opts),
			getPartConnectors: () => JSON.parse(JSON.stringify(getPartConnectorFrames())),
			getAssemblyPartConnectors: () => JSON.parse(JSON.stringify(getAssemblyPartConnectors())),
			showMateConnectorDialog: (featureId) => showMateConnectorDialog(featureId ?? null),
			hideMateConnectorDialog: () => hideMateConnectorDialog(),
			getMateConnectorDialogState: () => (mateConnectorDialogState ? JSON.parse(JSON.stringify(mateConnectorDialogState)) : null),
			applyMateConnector: (choice) => applyMateConnector(choice),
			updateConnector: (id, patch) => updateConnector(id, patch),
			removeConnector: (id) => removeConnector(id),
			probeConnectorRef: (path, ref) => probeConnectorRef(path, ref),
			getAssemblyConnectorFrames: () => JSON.parse(JSON.stringify(getAssemblyConnectorFrames())),
			getConnectorRefusal: () => lastConnectorRefusal,
			addMate: (opts) => addMate(opts),
			updateMate: (id, patch) => updateMate(id, patch),
			removeMate: (id) => removeMate(id),
			getSelectedInstanceId: () => selectedInstancePath?.[0] ?? null,
			getSelectedInstancePath: () => selectedInstancePath ? [...selectedInstancePath] : null,
			getSourceTabs: () => JSON.parse(JSON.stringify(sourceTabs)),
			listSourceTabs: async (id) => (await bridge.send({ type: 'ListSourceTabs', source_id: id }))?.tabs ?? [],
			setSourcePack: (id, pack) => setSourcePack(id, pack),
			packAllSources: () => packAllSources(),
			pinSource: (id) => pinSource(id),
			updateSourceToTip: (id) => updateSourceToTip(id),
			fetchSource: (id) => fetchSource(id),
			getImportDialogState: () => importDialogState,
			showImportDialogForEdit: (featureId) => showImportDialogForEdit(featureId),
			showEditFeatureDialog: (featureId) => showEditFeatureDialog(featureId),
			hideImportDialog: () => hideImportDialog(),
			applyImportPlacement: (featureId, placement, opts) => applyImportPlacement(featureId, placement, opts),
			cancelImportPlacement: () => cancelImportPlacement(),
			getGhostFeatureId: () => getGhostFeatureId(),
			exportStl: () => exportStl(),
			exportBodyStl: (bodyId, name) => exportBodyStl(bodyId, name),
			exportStep: () => exportStep(),
			getCameraState: () => getCameraState(),
			/** The rotation-center marker's state, for tests. */
			getRotationCenter: () => {
				const { controls } = getCameraRefs();
				const at = controls?.orbitPivot ?? controls?.target ?? null;
				return {
					enabled: !!getSettings().showRotationCenter,
					orbiting: orbitActive,
					visible: !!getSettings().showRotationCenter && orbitActive,
					at: at ? [at.x, at.y, at.z] : null,
				};
			},
			getRenderStats: () => getRenderStats(),
			getCameraProjection: () => getCameraProjection(),
			setCameraProjection: (proj) => setCameraProjection(proj),
			getConstraints: () => [...sketchConstraints],
			getProjectedBindings: () => JSON.parse(JSON.stringify(projectedBindings)),
			getSketchSelection: () => [...sketchSelection],
			getSettings: () => JSON.parse(JSON.stringify(getSettings())),
			updateSettings: (patch) => updateSettings(patch),
			projectVertex: (geomRef) => projectVertex(geomRef),
			projectEdge: (anchor, p0, p1) => projectEdge(anchor, p0, p1),
			projectFace: (geomRef) => projectFace(geomRef),
			getConstraintModal: () => (constraintModal ? { ...constraintModal, running: [...constraintModal.running] } : null),
			// Pure decision-engine probe over the LIVE sketch entities/positions —
			// for deterministic branch-coverage tests of the modal step logic.
			constraintModalStep: (constraintId, running, pickId) =>
				stepConstraintModal({ constraintId, running, pickId, entities: sketchEntities, positions: sketchPositions }),
			openConstraintModal: (id) => openConstraintModal(id),
			constraintModalPick: (pickId) => constraintModalPick(pickId),
			closeConstraintModal: () => closeConstraintModal(),
			getProfiles: () => [...extractedProfilesState],
			getExtrudeDialogState: () => extrudeDialogState,
			getExtrudePreviewParams: () => extrudePreviewParams,
			setExtrudePreviewParams: (params) => setExtrudePreviewParams(params),
			getProfilePickMode: () => getProfilePickMode(),
			setProfilePickMode: (mode) => setProfilePickMode(mode),
			getSketchRegions: (featureId) => getSketchRegions(featureId),
			getInactiveHoveredProfile: () => getInactiveHoveredProfile(),
			getAxisPickMode: () => getAxisPickMode(),
			setAxisPickMode: (active) => setAxisPickMode(active),
			getExtrudeRegions: () => getExtrudeRegions(),
			addExtrudeRegion: (sketchId, sketchName, profileIndex, region = null) => addExtrudeRegion(sketchId, sketchName, profileIndex, region),
			removeExtrudeRegion: (index) => removeExtrudeRegion(index),
			changeExtrudeSketch: (sketchId) => changeExtrudeSketch(sketchId),
			getRevolveDialogState: () => revolveDialogState,
			getPipeDialogState: () => pipeDialogState,
			showPipeDialog: () => showPipeDialog(),
			// Custom feature scripts (A-M4). Test SETUP + agent-free driving of
			// the editor/dialog flows; the tools go through the executor.
			getScriptDialogState: () => (scriptDialogState ? JSON.parse(JSON.stringify(scriptDialogState)) : null),
			showScriptDialog: () => showScriptDialog(),
			showScriptDialogForEdit: (id) => showScriptDialogForEdit(id),
			setScriptDialogSource: (id) => setScriptDialogSource(id),
			hideScriptDialog: () => hideScriptDialog(),
			applyScript: (choice) => applyScript(choice),
			getScriptEditorState: () => (scriptEditorState ? JSON.parse(JSON.stringify(scriptEditorState)) : null),
			showScriptEditor: (id, opts) => showScriptEditor(id ?? null, opts ?? {}),
			hideScriptEditor: () => hideScriptEditor(),
			saveScriptEditor: (draft) => saveScriptEditor(draft),
			checkScript: (what) => checkScript(what),
			readSource: (id) => readSource(id),
			addScriptSource: (what) => addScriptSource(what),
			setScriptSource: (id, text) => setScriptSource(id, text),
			getScriptSources: () => JSON.parse(JSON.stringify(getScriptSources())),
			// Test SETUP only (the pick-mode interaction has its own coverage):
			// sets the pipe dialog's path as viewport picks would (a single id
			// expands to its connected chain, like a click).
			setPipePath: (sketchId, ids, opts = {}) => setPipePath(sketchId, ids, opts),
			getRevolvePreviewParams: () => revolvePreviewParams,
			setRevolvePreviewParams: (params) => setRevolvePreviewParams(params),
			getChamferDialogState: () => chamferDialogState,
			showChamferDialog: () => showChamferDialog(),
			hideChamferDialog: () => hideChamferDialog(),
			applyChamfer: (distance) => applyChamfer(distance),
			getFilletDialogState: () => filletDialogState,
			showFilletDialog: () => showFilletDialog(),
			hideFilletDialog: () => hideFilletDialog(),
			applyFillet: (radius) => applyFillet(radius),
			getShellDialogState: () => shellDialogState,
			showShellDialog: () => showShellDialog(),
			hideShellDialog: () => hideShellDialog(),
			applyShell: (thickness) => applyShell(thickness),
			getBooleanDialogState: () => booleanDialogState,
			showBooleanDialog: () => showBooleanDialog(),
			hideBooleanDialog: () => hideBooleanDialog(),
			applyBoolean: (op, target, tool) => applyBoolean(op, target, tool),
			applyUnionAll: () => applyUnionAll(),
			getRebuildProgress: () => rebuildProgress,
			getConsumedFeatures: () => [...consumedFeatures],
			getSelectedRefs: () => [...selectedRefs],
			getHoveredRef: () => hoveredRef,
			getSketchHover: () => getSketchHover(),
			selectRef: (ref, additive) => selectRef(ref, additive),
			clearSelection: () => clearSelection(),
			setHoveredRef: (ref) => setHoveredRef(ref),
			getBoxSelectState: () => ({ ...boxSelectState }),
			getSelectOtherState: () => ({ ...selectOtherState }),
			getRebuildTime: () => rebuildTime,
			getDimensionPopup: () => dimensionPopup ? { ...dimensionPopup } : null,
			// Pure dimension heuristic over the LIVE sketch — for branch-coverage
			// tests. targets: [{id,type}], leader: {x,y}. See /specs/dimension_tool.md.
			classifyDimension: (targets, leader) =>
				classifyDimension({ targets, leader, positions: sketchPositions, entities: sketchEntities }),
			showDimensionPopup: (popup) => showDimensionPopup(popup),
			hideDimensionPopup: () => hideDimensionPopup(),
			applyDimensionFromPopup: (value) => applyDimensionFromPopup(value),
			getSnapIndicator: () => getSnapIndicator(),
			getSnapCandidates: () => _getSnapCandidates(),
			getSketchPixelSize: () => getSketchPixelSize(),
			getSnapSettings: () => getSnapSettings(),
			updateSnapSettings: (updates) => updateSnapSettings(updates),
			getInferenceSources: () => getInferenceSources().map((s) => ({ ...s })),
			sketchToScreenOffset: (sx, sy) => {
				const sm = sketchMode;
				if (!sm?.active) return null;
				const cam = cameraObject;
				const canvas = document.querySelector('canvas');
				if (!cam || !canvas) return null;
				const plane = buildSketchPlane(sm.origin, sm.normal, sm.xAxis);
				const screen = sketchToScreen(sx, sy, plane, cam, canvas);
				const rect = canvas.getBoundingClientRect();
				return { x: screen.x - (rect.left + rect.width / 2), y: screen.y - (rect.top + rect.height / 2) };
			},
			getPreview: () => getPreview(),
			getToolState: () => _getToolState(),
			getIsDragging: () => _getIsDragging(),
			getPointerDownPos: () => _getPointerDownPos(),
			getDrawingState: () => ({
				toolState: _getToolState(),
				isDragging: _getIsDragging(),
				pointerDownPos: _getPointerDownPos(),
				startPos: _getStartPos(),
				startPointId: _getStartPointId(),
			}),
			getToolEventLog: () => _getToolEventLog(),
			clearToolEventLog: () => _clearToolEventLog(),
			getSolveStatus: () => sketchSolveStatus ? { ...sketchSolveStatus } : null,
			getOverConstrained: () => [...overConstrainedEntities],
			getUnderConstrained: () => [...getUnderConstrainedEntities()],
			getFailedConstraintIndices: () => [...failedConstraintIndices],
			getFeatureErrors: () => new Map(featureErrors),
			projectFaceCentroids: () => {
				const cam = cameraObject;
				const canvas = document.querySelector('canvas');
				if (!cam || !canvas) return [];
				const rect = canvas.getBoundingClientRect();
				// Visible = the point is the FIRST model surface along its camera
				// ray. Without this a hidden face's point was reported too, and a
				// click on it lands on whatever face happens to cover it.
				const modelObjects = [];
				let root = cam;
				while (root.parent) root = root.parent;
				root.traverse((obj) => {
					if (obj.visible && obj.userData?.waffleType === 'model') modelObjects.push(obj);
				});
				const raycaster = new THREE.Raycaster();
				const results = [];
				for (const mesh of meshes) {
					if (!mesh.faceRanges) continue;
					for (const range of mesh.faceRanges) {
						if (!range.geom_ref) continue;
						const point = new THREE.Vector3(...faceInteriorPoint(mesh.vertices, mesh.indices, range.start_index, range.end_index));
						const v = point.clone().project(cam);
						if (v.z > 1) continue; // behind the camera
						raycaster.setFromCamera(new THREE.Vector2(v.x, v.y), cam);
						const hit = raycaster.intersectObjects(modelObjects, true)[0];
						const reach = raycaster.ray.origin.distanceTo(point);
						if (modelObjects.length && (!hit || hit.distance < reach - Math.max(1e-9, reach * 1e-4))) continue;
						const screenX = (v.x * 0.5 + 0.5) * rect.width + rect.left;
						const screenY = (-v.y * 0.5 + 0.5) * rect.height + rect.top;
						results.push({ geomRef: range.geom_ref, screenX, screenY, behindCamera: false });
					}
				}
				return results;
			},
			isProjectToolActive: () => isProjectToolActive(),
			getProjectName: () => getProjectName(),
			setProjectName: (name) => setProjectName(name),
			getAutoRestoreState: () => getAutoRestoreState(),
			getDocumentInfo: () => JSON.parse(JSON.stringify(getDocumentInfo())),
			restoreAutoSave: () => restoreAutoSave(),
			discardAutoSave: () => discardAutoSave(),
			getSectionState: () => ({ ...sectionState, plane: sectionState.plane ? { origin: [...sectionState.plane.origin], normal: [...sectionState.plane.normal] } : null }),
			toggleSection: () => toggleSection(),
			flipSection: () => flipSection(),
			setSectionOffset: (o) => setSectionOffset(o),
			clearSection: () => clearSection(),
			// Count live MeshStandardMaterials in the scene that currently carry a
			// clipping plane — used by the section-view GUI test to confirm the
			// section clip actually reached materials in the render graph.
			countClippedMaterials: () => {
				const scene = cameraObject?.parent;
				if (!scene) return 0;
				const seen = new Set();
				let clipped = 0;
				scene.traverse((obj) => {
					if (obj.isMesh && obj.material) {
						const mats = Array.isArray(obj.material) ? obj.material : [obj.material];
						for (const m of mats) {
							if (m && Array.isArray(m.clippingPlanes) && m.clippingPlanes.length > 0 && !seen.has(m.uuid)) {
								seen.add(m.uuid);
								clipped++;
							}
						}
					}
				});
				return clipped;
			},
			getDatumPlanes: () => BUILTIN_PLANES,
			createDatumPlane: (definition, name) => createDatumPlane(definition, name),
			enterSketchEditMode: (featureId) => enterSketchEditMode(featureId),
			getEditingSketchFeatureId: () => editingSketchFeatureId,
			isSketchVisible: (featureId) => isSketchVisible(featureId),
			toggleSketchVisibility: (featureId) => toggleSketchVisibility(featureId),
			isPlaneVisible: (planeId) => isPlaneVisible(planeId),
			togglePlaneVisibility: (planeId) => togglePlaneVisibility(planeId),
			isAxisVisible: (axisId) => isAxisVisible(axisId),
			toggleAxisVisibility: (axisId) => toggleAxisVisibility(axisId),
			getSketchSelection: () => [...sketchSelection],
			setSketchSelection: (ids) => { sketchSelection = new Set(ids); },
			// Pure chain/offset queries for branch-coverage tests (the tool
			// flows themselves are driven with real pointer events).
			findConnectedChain: (id) => findConnectedChain(id, sketchEntities, sketchPositions),
			getOffsetToolState: () => _getOffsetToolState(),
			faceBoundaryPreview: (ref) => faceBoundaryPreview(ref),
			// Sketch-plane 2D → client-pixel position (the DimensionInput
			// mapping). Lets tests aim real pointer events at sketch geometry
			// whose screen position depends on the camera.
			sketchPointToScreen: (x, y) => {
				if (!sketchMode.active) return null;
				const camera = getCameraObject();
				const canvas = document.querySelector('[data-testid="viewport"] canvas') || document.querySelector('canvas');
				if (!camera || !canvas) return null;
				const plane = buildSketchPlane(sketchMode.origin, sketchMode.normal, sketchMode.xAxis);
				return sketchToScreen(x, y, plane, camera, canvas);
			},
			computeChainOffset: (ids, d) => {
				const ordered = orderChain(ids, sketchEntities, sketchPositions);
				if (ordered.error) return { error: ordered.error };
				const resolved = resolveChainSegments(ordered.items, sketchEntities, sketchPositions);
				if (resolved.error) return { error: resolved.error };
				return offsetChainSegments(resolved.segments, ordered.closed, d);
			},
			getSelectedConstraintIndex: () => getSelectedConstraintIndex(),
			setSelectedConstraintIndex: (idx) => setSelectedConstraintIndex(idx),
			deleteSelectedConstraint: () => deleteSelectedConstraint(),
			getConstraintBadgeOffsets: () => Object.fromEntries(constraintBadgeOffsets),
			getConstraintBadges: () => getConstraintBadges(),
			addSketchEntity: (entity) => addLocalEntity(entity),
			addSketchConstraint: (constraint) => addLocalConstraint(constraint),
			removeSketchEntities: (ids) => removeSketchEntities(new Set(ids)),
			createGear: (params) => createGear(params),
			createSprocket: (params) => createSprocket(params),
			createPlanetary: (params) => createPlanetary(params),
			updateGear: (gearId, params) => updateGear(gearId, params),
			deleteGear: (gearId) => deleteGear(gearId),
			getGearRegistry: () => new Map(gearRegistry),
			getGearIdForEntity: (entityId) => getGearIdForEntity(entityId),
			// Inactive (completed) sketch gear display cache, for tests/debug.
			getInactiveGearDisplay: () => {
				const out = {};
				for (const [key, disp] of inactiveGearDisplay) {
					out[key] = {
						entityCount: disp.entities.length,
						counts: disp.entities.reduce((acc, e) => {
							acc[e.type] = (acc[e.type] || 0) + 1;
							return acc;
						}, {})
					};
				}
				return out;
			},
			// Ephemeral per-gear display expansion (entities + counts), for tests/debug.
			getGearDisplay: () => {
				const out = {};
				for (const [gid, disp] of gearDisplay) {
					out[gid] = {
						entities: [...disp.entities],
						pitchRadius: disp.pitchRadius,
						counts: disp.entities.reduce((acc, e) => {
							acc[e.type] = (acc[e.type] || 0) + 1;
							return acc;
						}, {})
					};
				}
				return out;
			},
			removeSketchConstraint: (index) => removeSketchConstraint(index),
			toggleConstraintReference: (index) => toggleConstraintReference(index),
			dragSketchPoint: (pointId, x, y) => dragSketchPoint(pointId, x, y),
			dragSketchLine: (lineId, dx, dy) => dragSketchLine(lineId, dx, dy),
			finalizeDrag: () => finalizeDrag(),
			undo: () => undo(),
			redo: () => redo(),
			getLogs: (filter) => getLogs(filter),
			exportLogs: (filter) => exportLogs(filter),
			clearLogs: () => clearLogs(),
			showToast: (level, message, durationMs) => showToast(level, message, durationMs),
			getToasts: () => getToasts(),
			dismissAllToasts: () => dismissAllToasts(),
			diagnose: () => {
				const d = {
					engineReady,
					bridgeExists: !!bridge,
					sketchMode: { ...sketchMode },
					activeTool,
					entityCount: sketchEntities.length,
					constraintCount: sketchConstraints.length,
					featureCount: featureTree?.features?.length ?? 0,
					meshCount: meshes.length,
					lastError,
					statusMessage,
					userAgent: navigator.userAgent,
				};
				console.table(d);
				return d;
			},
			viewportDebug: () => {
				const cam = cameraObject;
				if (!cam) return { error: 'No camera' };
				const canvas = document.querySelector('canvas');
				const renderer = canvas?.__threlte_renderer ?? canvas?.getContext('webgl2');
				// Compute scene AABB from meshes
				const min = [Infinity, Infinity, Infinity];
				const max = [-Infinity, -Infinity, -Infinity];
				let hasVerts = false;
				for (const m of meshes) {
					if (!m.vertices || m.vertices.length < 3) continue;
					for (let i = 0; i < m.vertices.length; i += 3) {
						hasVerts = true;
						for (let a = 0; a < 3; a++) {
							if (m.vertices[i + a] < min[a]) min[a] = m.vertices[i + a];
							if (m.vertices[i + a] > max[a]) max[a] = m.vertices[i + a];
						}
					}
				}
				const sceneAABB = hasVerts ? { min, max } : null;
				let cameraDistanceToAABB = null;
				let isInsideAABB = false;
				if (sceneAABB) {
					const p = cam.position;
					isInsideAABB = p.x >= min[0] && p.x <= max[0] &&
						p.y >= min[1] && p.y <= max[1] &&
						p.z >= min[2] && p.z <= max[2];
					// Distance to AABB center
					const cx = (min[0] + max[0]) / 2;
					const cy = (min[1] + max[1]) / 2;
					const cz = (min[2] + max[2]) / 2;
					cameraDistanceToAABB = Math.sqrt(
						(p.x - cx) ** 2 + (p.y - cy) ** 2 + (p.z - cz) ** 2
					);
				}
				const isOrtho = /** @type {any} */ (cam).isOrthographicCamera;
				const result = {
					camera: {
						near: cam.near,
						far: cam.far,
						type: isOrtho ? 'orthographic' : 'perspective',
						fov: /** @type {any} */ (cam).fov ?? null,
						orthoFrustum: isOrtho ? {
							left: /** @type {any} */ (cam).left,
							right: /** @type {any} */ (cam).right,
							top: /** @type {any} */ (cam).top,
							bottom: /** @type {any} */ (cam).bottom,
						} : null,
						position: [cam.position.x, cam.position.y, cam.position.z],
						projectionMatrix: cam.projectionMatrix.elements.slice(),
					},
					sceneAABB,
					cameraDistanceToAABB,
					isInsideAABB,
					rendererInfo: {
						logDepthBuffer: true, // set in createRenderer
					},
				};
				console.table(result.camera);
				return result;
			},
			toggleWireframe: () => {
				const canvas = document.querySelector('canvas');
				if (!canvas) return;
				const scene = cameraObject?.parent;
				if (!scene) return;
				let toggled = 0;
				scene.traverse((obj) => {
					if (/** @type {any} */ (obj).isMesh && obj.visible) {
						const mats = Array.isArray(obj.material) ? obj.material : [obj.material];
						for (const mat of mats) {
							if (mat && 'wireframe' in mat) {
								mat.wireframe = !mat.wireframe;
								mat.needsUpdate = true;
								toggled++;
							}
						}
					}
				});
				console.log(`Toggled wireframe on ${toggled} materials`);
				return toggled;
			},
			shaderDebug: false,
			getDocumentState: () => ({
				documentId,
				activeDocId,
				activeTabId,
				documentTabs: documentTabs.map(t => ({ id: t.id, name: t.name, kind: t.kind?.type })),
				documentName,
				documentCreated,
				documentDisplayUnit,
				documentLink: documentLink ? JSON.parse(JSON.stringify(documentLink)) : null,
				readOnly: documentLink?.readOnly === true,
			}),
			// Test/debug: the exact document JSON the save paths write.
			buildDocumentJson: () => buildDocumentJson(),
			// Agent link oracles (spec §5): every bridge send with its origin (O7),
			// the lock holder, the busy reason (G3) and the running agent call.
			recordEngineSends: (on, opts) => bridge.recordSends(on, opts),
			replayEngineMessages: (entries) => replayEngineMessages(entries),
			getEngineSendLog: () => bridge.getSendLog(),
			getEngineLockHolder: () => engineLockHolder,
			getUserBusyReason: () => getUserBusyReason(),
			getAgentActivity: () => (agentActivity ? { ...agentActivity } : null),
			forkLinkedDocument: () => forkLinkedDocument(),
		};
	}
}

/**
 * Send a command to the engine.
 * @param {object} message - UiToEngine message
 * @returns {Promise<object>} EngineToUi response
 */
export async function send(message) {
	if (!bridge) {
		throw new Error('Engine not initialized');
	}
	return bridge.send(message);
}

/**
 * Send a rebuild-triggering command to the engine with spinner tracking.
 * Sets rebuilding=true before sending, false after response (or error).
 * Also records rebuildTime in ms.
 * @param {object} message - UiToEngine message
 * @returns {Promise<object>} EngineToUi response
 */
async function sendRebuild(message) {
	rebuilding = true;
	const t0 = performance.now();
	try {
		const result = await bridge.send(message);
		rebuildTime = performance.now() - t0;
		return result;
	} finally {
		rebuilding = false;
	}
}

/**
 * Get reactive engine state.
 */
export function getFeatureTree() {
	return featureTree;
}

export function getMeshes() {
	return meshes;
}

/**
 * List the solid bodies in the current model. A body is one mesh-bearing output
 * of a feature, so a multi-body feature (e.g. a boolean split) contributes more
 * than one entry. Each body carries its persistent `bodyId`, producing feature
 * id, output key, and resolved display name (the engine resolves the name: a
 * user override if set, else the derived feature name + ordinal).
 * Returns `[{ bodyId, featureId, outputKey, name }]` in render order.
 */
export function getBodies() {
	// Ghost bodies of an edit context belong to OTHER parts: never this part's
	// bodies (not renameable, not boolean targets).
	return meshes.filter((m) => !m.context).map((m) => {
		// Engine-resolved name (preferred); fall back to the feature name for the
		// legacy per-feature worker path, which doesn't resolve names.
		let name = m.name;
		if (!name) {
			const feature = featureTree.features.find((f) => f.id === m.featureId);
			name = feature?.name ?? 'Body';
		}
		return {
			bodyId: m.bodyId,
			featureId: m.featureId,
			outputKey: m.outputKey ?? null,
			instanceId: m.instanceId ?? null,
			name
		};
	});
}

/** Body id selected in the Bodies list, or null. */
export function getSelectedBodyId() {
	return selectedBodyId;
}

/** Body id hovered in the Bodies list, or null. */
export function getHoveredBodyId() {
	return hoveredBodyId;
}

/**
 * Select a whole body for highlighting (by its `bodyId`). Pass null to clear.
 * Selecting a body clears any face/edge-level selection so the whole-body
 * highlight reads cleanly.
 * @param {string | null} bodyId
 */
export function selectBody(bodyId) {
	selectedBodyId = bodyId;
	if (bodyId) {
		selectedRefs = [];
	}
}

/**
 * Set the hovered body (by `bodyId`), or null to clear.
 * @param {string | null} bodyId
 */
export function setHoveredBodyId(bodyId) {
	hoveredBodyId = bodyId;
}

/**
 * Rename a body — sets a display-name override independent of the producing
 * feature's name. An empty name clears the override (reverts to derived).
 * @param {string} bodyId
 * @param {string} newName
 */
export async function renameBody(bodyId, newName) {
	if (!bridge || !engineReady) return;
	log('action', 'Rename body', { bodyId, newName });
	await bridge.send({ type: 'RenameBody', body_id: bodyId, new_name: newName });
}

export function isEngineReady() {
	return engineReady;
}

export function getLastError() {
	return lastError;
}

export function getFeatureErrors() {
	return featureErrors;
}

/** Non-fatal warnings carried by the latest rebuild, verbatim, in engine order. */
export function getRebuildWarnings() {
	return [...lastRebuildWarnings];
}

export function getRebuildTime() {
	return rebuildTime;
}

export function isRebuilding() {
	return rebuilding;
}

export function getStatusMessage() {
	return statusMessage;
}

/** The rebuild progress frame in flight, or null. */
export function getRebuildProgress() {
	return rebuildProgress;
}

/**
 * Subscribe to rebuild progress frames (the agent link forwards them to the
 * relay for the call in flight). Returns the unsubscribe function.
 * @param {(msg: any) => void} cb
 */
export function subscribeRebuildProgress(cb) {
	progressListeners.add(cb);
	return () => progressListeners.delete(cb);
}

/** Feature ids whose bodies a later feature consumed (not live). */
export function getConsumedFeatures() {
	return consumedFeatures;
}

// Transient tool hint: shown in the status bar (over the engine status)
// while a sketch tool wants to say what the next click will do — e.g. the
// project/offset chain preview. Cleared on tool reset.
let toolHint = $state(null);
export function getToolHint() {
	return toolHint;
}
/** @param {string | null} text */
export function setToolHint(text) {
	toolHint = text;
}

export function getHoveredRef() {
	return hoveredRef;
}

export function getSelectedRefs() {
	return selectedRefs;
}

/**
 * Face→feature (KV13 F6b): the feature that *introduced* the selected face's
 * geometry, through chained booleans — NOT the last feature that owns the body.
 * Prefers the picked face's `created_by_feature` (resolved kernel-side from the
 * face's persistent-id lineage) when present, falling back to the GeomRef
 * anchor's feature (the Phase-D Tier-1 behavior — exact for single-feature
 * bodies). Null when nothing is selected, or the selection is a datum.
 * @returns {string | null}
 */
export function getSelectedRefFeatureId() {
	for (const ref of selectedRefs) {
		const anchor = ref?.anchor;
		if (anchor?.type === 'FeatureOutput' && anchor.feature_id) {
			return createdByFeatureForRef(ref) ?? anchor.feature_id;
		}
	}
	return null;
}

/**
 * The `created_by_feature` (introducing feature) of the face matching `ref`,
 * looked up from the rendered meshes' face ranges. Null if `ref` is not a face
 * or carries no resolved provenance. (KV13 F6b)
 * @param {any} ref
 * @returns {string | null}
 */
function createdByFeatureForRef(ref) {
	if (ref?.kind?.type !== 'Face') return null;
	for (const mesh of meshes) {
		if (!mesh.faceRanges) continue;
		for (const range of mesh.faceRanges) {
			if (range.geom_ref && geomRefEquals(range.geom_ref, ref)) {
				return range.created_by_feature ?? null;
			}
		}
	}
	return null;
}

/**
 * Set the hovered geometry reference.
 * @param {any | null} ref
 */
export function setHoveredRef(ref) {
	hoveredRef = ref;
	if (bridge && engineReady && !isDatumPlaneRef(ref)) {
		bridge.send({ type: 'HoverEntity', geom_ref: JSON.parse(JSON.stringify(ref)) });
	}
}

/**
 * Whether body geometry (faces/edges/vertices) is pickable right now.
 * Outside sketch mode: always (normal modeling picking). In sketch mode: only
 * under the Select or Project tool (Cycle-2 select-first flow); every drawing
 * tool keeps body picking fully gated off (invariant I5).
 * @returns {boolean}
 */
export function isBodyPickingEnabled() {
	if (!sketchMode?.active) return true;
	// Offset projects-then-offsets body geometry, so it picks bodies too.
	return activeTool === 'select' || activeTool === 'project' || activeTool === 'offset';
}

// Invariant I3: deterministic hover/selection priority Vertex ≻ Edge ≻ Face at
// a single pointer pixel, independent of which listener (mesh / edge overlay /
// vertex overlay) fires first. Each source PROPOSES its hit for the current
// pixel with a priority; the highest priority wins. Moving to a new pixel
// resets the arbitration so a fresh set of proposals competes cleanly.
const HOVER_PRIORITY = { Vertex: 3, Edge: 2, Face: 1 };
let _hoverArb = { x: null, y: null, priority: 0 };

/**
 * Propose a hovered ref for the pointer pixel (clientX, clientY). Applies the
 * highest-priority proposal for that pixel (Invariant I3). Pixel-keyed because
 * hover fires continuously — each distinct pixel is a fresh arbitration.
 * @param {any} ref
 * @param {number} clientX
 * @param {number} clientY
 */
export function proposeHoverRef(ref, clientX, clientY) {
	const priority = HOVER_PRIORITY[ref?.kind?.type] ?? 0;
	if (clientX !== _hoverArb.x || clientY !== _hoverArb.y) {
		_hoverArb = { x: clientX, y: clientY, priority: 0 };
	}
	if (priority < _hoverArb.priority) return;
	_hoverArb.priority = priority;
	setHoveredRef(ref);
}

// Last pointer pixel seen by the sketch interaction layer. Used to tell whether
// `hoveredRef` was arbitrated at the pixel a click is happening on, or is stale.
let _lastPointerClient = { x: null, y: null };
/** Screen-pixel tolerance for treating a hover as "at the current pointer". */
const HOVER_FRESH_TOL_PX = 4;

/** Record the current pointer pixel (called by the sketch interaction handler). */
export function setLastPointerClient(x, y) {
	_lastPointerClient = { x, y };
}

/**
 * `hoveredRef`, but ONLY when it was arbitrated at (approximately) the current
 * pointer pixel — otherwise null. This guards the click-selection paths against
 * a STALE Vertex/Edge hover: moving onto a face-interior pixel produces no
 * synchronous edge/vertex proposal, and the face hover is frame-deferred
 * (Threlte raycasts on rAF), so a click landing before that frame would
 * otherwise read a leftover Vertex/Edge from the previous pixel and mis-resolve.
 * Falls back to `hoveredRef` when no pixel info is available.
 * @returns {any | null}
 */
export function getFreshHoveredRef() {
	if (_hoverArb.x == null || _lastPointerClient.x == null) return hoveredRef;
	const dx = _hoverArb.x - _lastPointerClient.x;
	const dy = _hoverArb.y - _lastPointerClient.y;
	if (Math.abs(dx) <= HOVER_FRESH_TOL_PX && Math.abs(dy) <= HOVER_FRESH_TOL_PX) return hoveredRef;
	return null;
}

// Selection priority (Invariant I3) is driven off the ALREADY-ARBITRATED hover
// rather than a second race: a click selects whatever the hover arbitration
// chose for this pixel. The overlays (Edge/Vertex) select only when the hovered
// ref equals their own pick; the Face handler (CadModel) defers when a
// Vertex/Edge is hovered. This is timing-independent (no dependence on which
// click listener fires first, or on matching client coordinates across the
// Threlte and DOM events).

/**
 * Select a geometry reference. Supports multi-select with additive flag.
 * @param {any | null} ref
 * @param {boolean} additive - If true, toggle selection; if false, replace selection
 */
export function selectRef(ref, additive = false) {
	// Intercept face clicks when in profile pick mode (extrude). Prefer a sketch
	// region coincident with the face: when a region is under the cursor (set by
	// the inactive-sketch hover path), let that path add it and ignore the face,
	// so a sketch drawn on a body face extrudes its region rather than selecting
	// the (unsupported) underlying face.
	if (profilePickMode?.target === 'extrude' && ref?.kind?.type === 'Face') {
		if (inactiveHoveredProfile) return;
		addExtrudeRegionFromRef(ref);
		return;
	}

	// Intercept edge clicks when in axis pick mode (revolve)
	if (axisPickMode && ref?.kind?.type === 'Edge') {
		const axis = extractAxisFromEdgeRef(ref);
		if (axis) {
			setRevolveAxis(axis.origin, axis.direction, axis.label);
		}
		return;
	}

	// Any explicit face/edge selection supersedes a whole-body highlight.
	selectedBodyId = null;

	if (!ref) {
		selectedRefs = [];
		return;
	}

	// Intercept plane selection when in plane selection mode
	if (sketchPlaneSelectionMode && (isDatumPlaneRef(ref) || ref?.kind?.type === 'Face')) {
		const plane = computeFacePlane(ref);
		if (plane) {
			exitSketchPlaneSelection();
			enterSketchMode(plane.origin, plane.normal, ref);
			setActiveTool('line');
			return;
		}
	}

	log('ui', 'Select ref', { count: additive ? selectedRefs.length + 1 : 1 });

	if (additive) {
		const idx = selectedRefs.findIndex((r) => geomRefEquals(r, ref));
		if (idx >= 0) {
			selectedRefs = [...selectedRefs.slice(0, idx), ...selectedRefs.slice(idx + 1)];
		} else {
			selectedRefs = [...selectedRefs, ref];
		}
	} else {
		selectedRefs = [ref];
	}

	if (bridge && engineReady) {
		for (const r of selectedRefs) {
			if (!isDatumPlaneRef(r)) {
				bridge.send({ type: 'SelectEntity', geom_ref: JSON.parse(JSON.stringify(r)) });
			}
		}
	}
}

// isDatumPlaneRef is imported from planes.js

/**
 * Clear all selections.
 */
export function clearSelection() {
	selectedRefs = [];
	selectedBodyId = null;
}

/**
 * Check if two GeomRefs refer to the same entity.
 * @param {any} a
 * @param {any} b
 * @returns {boolean}
 */
export function geomRefEquals(a, b) {
	if (!a || !b) return false;
	return (
		a.kind?.type === b.kind?.type &&
		a.anchor?.type === b.anchor?.type &&
		a.anchor?.feature_id === b.anchor?.feature_id &&
		a.anchor?.plane === b.anchor?.plane &&
		a.anchor?.id === b.anchor?.id &&
		a.selector?.type === b.selector?.type &&
		canonicalJson(a.selector) === canonicalJson(b.selector) &&
		// In-context refs (v4 §2.8): the same face of two instances of one part
		// differs only by scope.
		canonicalJson(a.scope ?? null) === canonicalJson(b.scope ?? null)
	);
}

/**
 * Stable JSON of an object with object keys sorted recursively, so two
 * structurally-equal GeomRef selectors compare equal regardless of key
 * insertion order. (A selector that round-trips through the Rust engine comes
 * back with serde's key order, which differs from the JS-built order — a
 * plain JSON.stringify would then spuriously differ.)
 * @param {any} v
 * @returns {string}
 */
function canonicalJson(v) {
	return JSON.stringify(v, (_k, val) => {
		if (val && typeof val === 'object' && !Array.isArray(val)) {
			const sorted = {};
			for (const key of Object.keys(val).sort()) sorted[key] = val[key];
			return sorted;
		}
		return val;
	});
}

/**
 * Check if two GeomRefs have the same role type (ignoring role index).
 * Used for grouping SideFace facets from polygon-approximated curved surfaces.
 * @param {any} a
 * @param {any} b
 * @returns {boolean}
 */
export function geomRefSameRoleType(a, b) {
	if (!a || !b) return false;
	return (
		a.kind?.type === b.kind?.type &&
		a.anchor?.type === b.anchor?.type &&
		a.anchor?.feature_id === b.anchor?.feature_id &&
		a.anchor?.plane === b.anchor?.plane &&
		a.anchor?.id === b.anchor?.id &&
		a.selector?.type === b.selector?.type &&
		a.selector?.role?.type === b.selector?.role?.type
	);
}

/**
 * Check if a GeomRef is currently selected.
 * @param {any} ref
 * @returns {boolean}
 */
export function isSelected(ref) {
	return selectedRefs.some((r) => geomRefEquals(r, ref));
}

export function getSketchMode() {
	return sketchMode;
}

/**
 * Collect points from inactive sketches that lie on the same (or parallel) plane.
 * Projects them into the current sketch's 2D coordinate space.
 * @param {[number, number, number]} origin - Current sketch plane origin
 * @param {[number, number, number]} normal - Current sketch plane normal
 * @param {string | null} excludeFeatureId - Feature ID to exclude (the sketch being edited)
 * @returns {Array<{ x: number, y: number, sourceId: string, worldPos: [number, number, number] }>}
 */
function collectSamePlaneSketchPoints(origin, normal, excludeFeatureId) {
	const tree = featureTree;
	if (!tree?.features) return [];

	const pts = [];
	const nx = normal[0], ny = normal[1], nz = normal[2];
	const nLen = Math.sqrt(nx * nx + ny * ny + nz * nz);
	if (nLen < 1e-9) return [];
	const nnx = nx / nLen, nny = ny / nLen, nnz = nz / nLen;

	// Build current sketch plane basis for projection
	const plane = buildSketchPlane(origin, normal, sketchMode.xAxis);

	for (const feature of tree.features) {
		if (feature.operation?.type !== 'Sketch') continue;
		if (feature.suppressed) continue;
		if (feature.id === excludeFeatureId) continue;

		const sketch = feature.operation.sketch;
		if (!sketch?.solved_positions) continue;

		const sOrigin = sketch.plane_origin || [0, 0, 0];
		const sNormal = sketch.plane_normal || [0, 0, 1];
		const snLen = Math.sqrt(sNormal[0] ** 2 + sNormal[1] ** 2 + sNormal[2] ** 2);
		if (snLen < 1e-9) continue;
		const snx = sNormal[0] / snLen, sny = sNormal[1] / snLen, snz = sNormal[2] / snLen;

		// Check parallel normals (same or opposite direction)
		const dot = nnx * snx + nny * sny + nnz * snz;
		if (Math.abs(Math.abs(dot) - 1) > 0.001) continue;

		// Build the source sketch's plane to get 3D positions
		const srcPlane = buildSketchPlane(sOrigin, sNormal, sketch.plane_x_axis ?? null);

		for (const [id, coords] of Object.entries(sketch.solved_positions)) {
			if (!Array.isArray(coords) || coords.length < 2) continue;

			// Convert source sketch 2D -> 3D world
			const wx = sOrigin[0] + srcPlane.xAxis.x * coords[0] + srcPlane.yAxis.x * coords[1];
			const wy = sOrigin[1] + srcPlane.xAxis.y * coords[0] + srcPlane.yAxis.y * coords[1];
			const wz = sOrigin[2] + srcPlane.xAxis.z * coords[0] + srcPlane.yAxis.z * coords[1];

			// Project 3D world -> current sketch 2D
			const rx = wx - origin[0], ry = wy - origin[1], rz = wz - origin[2];
			const u = rx * plane.xAxis.x + ry * plane.xAxis.y + rz * plane.xAxis.z;
			const v = rx * plane.yAxis.x + ry * plane.yAxis.y + rz * plane.yAxis.z;

			pts.push({ x: u, y: v, sourceId: `${feature.id}:${id}`, worldPos: [wx, wy, wz] });
		}
	}
	return pts;
}

/**
 * The `BeginSketch` plane reference for a sketch started on `faceGeomRef`.
 * A face of ANOTHER instance (in-context editing, v4 §2.8) is recorded as the
 * sketch's plane reference so the engine re-derives the plane from that
 * instance on rebuild. A local face, a datum or no face keeps the historical
 * placeholder anchor (the sketch's origin/normal snapshot is authoritative).
 * Shared by Sketch mode and the agent link's `sketch_create`.
 * @param {any} [faceGeomRef]
 */
export function beginSketchPlaneRef(faceGeomRef = null) {
	if (faceGeomRef?.scope) return JSON.parse(JSON.stringify(faceGeomRef));
	return {
		kind: { type: 'Face' },
		anchor: { type: 'Datum', datum_id: generateUUID() },
		selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
		policy: { type: 'BestEffort' },
	};
}

/**
 * Enter sketch mode on a plane.
 * @param {[number, number, number]} origin - plane origin
 * @param {[number, number, number]} normal - plane normal
 * @param {any} [faceGeomRef] - optional face reference for zoom-to-face
 */
export async function enterSketchMode(origin = [0, 0, 0], normal = [0, 0, 1], faceGeomRef = null) {
	log('action', 'Enter sketch mode', { origin, normal });
	resetSketchState();

	// Notify the engine about the new sketch session
	if (bridge && engineReady) {
		const plane = beginSketchPlaneRef(faceGeomRef);
		try {
			await bridge.send({
				type: 'BeginSketch',
				plane
			});
		} catch (err) {
			log('error', `BeginSketch failed: ${err}`);
			statusMessage = 'Failed to start sketch';
			return;
		}
	}

	sketchMode = { active: true, origin, normal, xAxis: null };

	// Collect reference snap points from inactive sketches on the same/parallel plane
	referenceSnapPoints = collectSamePlaneSketchPoints(origin, normal, editingSketchFeatureId);

	// Save camera state before aligning, so it can be restored on sketch exit
	if (typeof window !== 'undefined') {
		window.dispatchEvent(new Event('waffle-save-camera'));
		if (faceGeomRef) {
			const bounds = computeFaceBounds(faceGeomRef);
			if (bounds) {
				window.dispatchEvent(new CustomEvent('waffle-zoom-to-face', {
					detail: { center: bounds.center, normal: bounds.normal, size: bounds.size }
				}));
				return;
			}
		}
		window.dispatchEvent(new CustomEvent('waffle-align-to-plane', { detail: { origin, normal, xAxis: sketchMode.xAxis } }));
	}
}

/**
 * Exit sketch mode.
 */
export function exitSketchMode() {
	log('action', 'Exit sketch mode');
	editingSketchFeatureId = null;
	resetSketchState();
	sketchMode = { active: false, origin: [0, 0, 0], normal: [0, 0, 1], xAxis: null };
	restoreEditRollback();
	// The draft still holds the session; a cancelled sketch must not come back on reload.
	scheduleAutoSave();
}

// -- Feature selection --

export function getSelectedFeatureId() {
	return selectedFeatureId;
}

/**
 * @param {string | null} id
 */
export function selectFeature(id) {
	selectedFeatureId = id;
}

/**
 * Get the currently selected feature object.
 */
export function getSelectedFeature() {
	if (!selectedFeatureId) return null;
	return featureTree.features.find((f) => f.id === selectedFeatureId) ?? null;
}

// -- Active tool --

export function getActiveTool() {
	return activeTool;
}

/**
 * @param {string} tool
 */
export function setActiveTool(tool) {
	log('ui', 'Set active tool', { tool });
	// Switching tools clears armed alignment-inference sources (branch table:
	// "tool switched → inference sources cleared").
	if (tool !== activeTool) clearInferenceSources();
	activeTool = tool;
	// Remember which rectangle variant the split button last selected, so the
	// button face + the `R` shortcut re-activate the same mode.
	if (tool === 'rectangle' || tool === 'rectangle-center') {
		rectMode = tool;
	}
}

/** Which rectangle construction mode the split button currently selects. */
export function getRectMode() {
	return rectMode;
}

/**
 * Check if the project tool is active in sketch mode.
 * @returns {boolean}
 */
export function isProjectToolActive() {
	return sketchMode?.active && activeTool === 'project';
}

// -- Sketch entity/constraint management --

/**
 * Allocate a new sketch entity ID.
 * @returns {number}
 */
export function allocEntityId() {
	return nextEntityId++;
}

/**
 * Deep-clone the current solved positions into a plain Map<id,{x,y}>, stripping
 * Svelte reactive proxies. Stored on undo entries as the pre-action geometry so
 * undo can revert solver-driven point movement, not just remove the entity/
 * constraint that triggered the re-solve.
 */
function snapshotPositions() {
	return new Map([...sketchPositions].map(([k, v]) => [k, { x: v.x, y: v.y }]));
}

/**
 * Dispatch a camera snapshot (getCameraState() shape) for CameraControls to
 * apply. Sketch undo/redo records carry one so reverting geometry also
 * reverts any auto-fit zoom the reverted action caused — without it, an
 * exploding solve that zoomed the view way out left the user stranded there
 * after undo (the growth-gated auto-fit never zooms back in).
 * @param {object | null} snap
 */
function restoreCameraState(snap) {
	if (!snap || typeof window === 'undefined') return;
	window.dispatchEvent(new CustomEvent('waffle-restore-camera', { detail: snap }));
}

/**
 * Begin recording a sketch action (for undo grouping).
 * Call before a tool creates entities/constraints.
 */
export function beginSketchAction() {
	pendingSketchAction = {
		entities: [],
		constraints: [],
		positionsBefore: snapshotPositions(),
		camera: getCameraState()
	};
}

/**
 * End recording a sketch action and push to undo stack.
 * Discards empty actions.
 */
export function endSketchAction() {
	if (pendingSketchAction &&
		(pendingSketchAction.entities.length || pendingSketchAction.constraints.length)) {
		sketchUndoStack = [...sketchUndoStack, pendingSketchAction];
		sketchRedoStack = [];
	}
	pendingSketchAction = null;
}

/**
 * Add a sketch entity locally and send to engine.
 * @param {object} entity - SketchEntity object
 */
export function addLocalEntity(entity) {
	log('sketch', `Entity added: ${entity.type}`, { id: entity.id, type: entity.type });
	sketchEntities = [...sketchEntities, entity];

	// Update positions map for Point entities
	if (entity.type === 'Point') {
		const next = new Map(sketchPositions);
		next.set(entity.id, { x: entity.x, y: entity.y });
		sketchPositions = next;
	}

	// Record for sketch undo
	const cloned = JSON.parse(JSON.stringify(entity));
	if (pendingSketchAction) {
		pendingSketchAction.entities.push(cloned);
	} else if (sketchMode.active) {
		// Snapshot WITHOUT the just-added point (undo restores this then drops the
		// added point) so geometry reverts to its pre-action state.
		const positionsBefore = snapshotPositions();
		if (cloned.type === 'Point') positionsBefore.delete(cloned.id);
		sketchUndoStack = [...sketchUndoStack, { entities: [cloned], constraints: [], positionsBefore, camera: getCameraState() }];
		sketchRedoStack = [];
	}

	// Send to engine (deep-clone to avoid Svelte 5 proxy DataCloneError)
	if (bridge && engineReady) {
		bridge.send({ type: 'AddSketchEntity', entity: cloned }).catch(err => console.error('AddSketchEntity failed:', err));
	}

	if (!suppressProfileExtraction) {
		reExtractProfiles();
	}
}

// -- Projected geometry (project external model geometry into the sketch) --
//
// Every projected entity is bound to model VERTICES via Position GeomRefs, so
// edges and faces reuse the proven vertex-reproject path: an edge projects its
// two endpoint vertices + a line; a face projects its in-plane boundary edges
// (shared corners deduped). The engine resolves each binding to the nearest
// vertex and reprojects on rebuild. See specs/projected_sketch_geometry.md.

export function getProjectedBindings() { return projectedBindings; }

/** Quantize a coordinate to the 1e-6 grid the engine resolves against. */
const quantize6 = (n) => Math.round(n * 1e6) / 1e6;

/** Map a 3D world point to the active sketch plane's 2D (u, v) coordinates. */
function worldToSketch2D(x, y, z) {
	const plane = buildSketchPlane(sketchMode.origin, sketchMode.normal, sketchMode.xAxis);
	const rx = x - plane.origin.x;
	const ry = y - plane.origin.y;
	const rz = z - plane.origin.z;
	return {
		u: rx * plane.xAxis.x + ry * plane.xAxis.y + rz * plane.xAxis.z,
		v: rx * plane.yAxis.x + ry * plane.yAxis.y + rz * plane.yAxis.z,
	};
}

/** Build a Vertex GeomRef with a Position selector at a 3D point. */
function vertexRefAt(anchor, x, y, z) {
	return {
		kind: { type: 'Vertex' },
		anchor: JSON.parse(JSON.stringify(anchor)),
		selector: { type: 'Position', x: quantize6(x), y: quantize6(y), z: quantize6(z) },
		policy: { type: 'BestEffort' },
	};
}

/**
 * Create a construction Point at the 2D projection of a 3D point and record a
 * binding to the given source. Returns the new Point id. Does NOT solve — the
 * caller batches and solves once.
 */
function addBoundProjectedPoint(x3, y3, z3, sourceBinding) {
	const { u, v } = worldToSketch2D(x3, y3, z3);
	const id = allocEntityId();
	addLocalEntity({ type: 'Point', id, x: u, y: v, construction: true });
	projectedBindings = [...projectedBindings, { point_id: id, source: sourceBinding }];
	return id;
}

/**
 * Project a picked model vertex into the active sketch: a construction Point at
 * the vertex's sketch-plane 2D position, bound so the engine keeps it coincident
 * with the source across rebuilds. The GeomRef carries a `Position` selector.
 * @param {object} geomRef - Vertex GeomRef with a Position selector.
 * @returns {number | null} the created Point id, or null if not applicable.
 */
export function projectVertex(geomRef) {
	if (!sketchMode.active) return null;
	const sel = geomRef?.selector;
	if (!sel || sel.type !== 'Position') return null;

	beginSketchAction();
	const id = addBoundProjectedPoint(sel.x, sel.y, sel.z, {
		geom_ref: JSON.parse(JSON.stringify(geomRef)),
		kind: { type: 'Vertex' },
	});
	endSketchAction();
	triggerSolve();
	return id;
}

/**
 * Project a straight model edge: its two endpoint vertices (bound) plus a
 * construction Line between them. `anchor` is the source body's GeomRef anchor
 * (taken from the picked edge); `p0`/`p1` are the endpoint world positions.
 * @returns {[number, number] | null} the two Point ids.
 */
export function projectEdge(anchor, p0, p1) {
	if (!sketchMode.active) return null;
	beginSketchAction();
	const id0 = addBoundProjectedPoint(p0[0], p0[1], p0[2], {
		geom_ref: vertexRefAt(anchor, p0[0], p0[1], p0[2]),
		kind: { type: 'Vertex' },
	});
	const id1 = addBoundProjectedPoint(p1[0], p1[1], p1[2], {
		geom_ref: vertexRefAt(anchor, p1[0], p1[1], p1[2]),
		kind: { type: 'Vertex' },
	});
	addLocalEntity({ type: 'Line', id: allocEntityId(), start_id: id0, end_id: id1, construction: true });
	endSketchAction();
	triggerSolve();
	return [id0, id1];
}

/**
 * Project a picked model face: every STRAIGHT body edge lying in the face's
 * plane becomes a bound construction line, with shared corner vertices deduped
 * so the boundary forms a connected loop. Curved in-plane edges are skipped for
 * now (their interior points are not vertices). Returns the count of lines made.
 * @param {object} faceGeomRef
 * @returns {number} number of construction lines created.
 */
export function projectFace(faceGeomRef) {
	if (!sketchMode.active) return 0;
	const anchor = faceGeomRef.anchor;
	const found = faceBoundaryRanges(faceGeomRef);
	if (!found) return 0;

	const corners = makeCornerAllocator(anchor);
	let lines = 0;
	beginSketchAction();
	for (const { mesh, index } of found) {
		lines += projectEdgeRange(mesh, mesh.edges.ranges[index], corners);
	}
	endSketchAction();
	if (lines > 0) triggerSolve();
	return lines;
}

/**
 * Project an explicit set of edge ranges from ONE mesh as a connected run:
 * shared corner vertices are deduped across the ranges so the projected
 * construction geometry chains (the Project/Offset tools' body-edge-chain
 * click). Returns the number of construction lines created.
 * @param {object} mesh - internal mesh object (with .edges)
 * @param {number[]} rangeIndices - indices into mesh.edges.ranges
 * @param {object} anchor - source body GeomRef anchor for vertex bindings
 * @returns {number}
 */
export function projectEdgeChain(mesh, rangeIndices, anchor) {
	if (!sketchMode.active || !mesh?.edges?.ranges) return 0;
	const corners = makeCornerAllocator(anchor);
	let lines = 0;
	beginSketchAction();
	for (const i of rangeIndices) {
		const range = mesh.edges.ranges[i];
		if (range) lines += projectEdgeRange(mesh, range, corners);
	}
	endSketchAction();
	if (lines > 0) triggerSolve();
	return lines;
}

/**
 * The face-boundary edge ranges projectFace would project: every edge (of
 * any mesh) whose points ALL lie in the face's plane. Null when the face
 * has no resolvable plane. Shared by projectFace and the hover preview.
 * @param {object} faceGeomRef
 * @returns {Array<{ mesh: object, index: number }> | null}
 */
function faceBoundaryRanges(faceGeomRef) {
	const plane3 = computeFacePlane(faceGeomRef);
	if (!plane3) return null;
	const meshData = getMeshes();
	if (!meshData) return null;

	const PLANE_TOL = 1e-5; // model units: edge must lie in the face plane
	const found = [];
	for (const mesh of meshData) {
		if (!mesh.edges || !mesh.edges.ranges || !mesh.edges.vertices) continue;
		const verts = mesh.edges.vertices;
		mesh.edges.ranges.forEach((range, index) => {
			const si = range.start_index;
			const ei = range.end_index;
			if (ei - si < 2) return;
			for (let k = si; k < ei; k++) {
				const dx = verts[k * 3] - plane3.origin[0];
				const dy = verts[k * 3 + 1] - plane3.origin[1];
				const dz = verts[k * 3 + 2] - plane3.origin[2];
				const d = dx * plane3.normal[0] + dy * plane3.normal[1] + dz * plane3.normal[2];
				if (Math.abs(d) > PLANE_TOL) return;
			}
			found.push({ mesh, index });
		});
	}
	return found;
}

/**
 * Sketch-2D polylines of the boundary projectFace would create for a face —
 * the tools' hover ghost, with no entities created. Empty when nothing
 * would project.
 * @param {object} faceGeomRef
 * @returns {Array<Array<[number, number]>>}
 */
export function faceBoundaryPreview(faceGeomRef) {
	if (!sketchMode.active) return [];
	const found = faceBoundaryRanges(faceGeomRef);
	if (!found) return [];
	const polylines = [];
	for (const { mesh, index } of found) {
		const verts = mesh.edges.vertices;
		const r = mesh.edges.ranges[index];
		const poly = [];
		for (let k = r.start_index; k < r.end_index; k++) {
			const p2 = worldToSketch2D(verts[k * 3], verts[k * 3 + 1], verts[k * 3 + 2]);
			poly.push([p2.u, p2.v]);
		}
		if (poly.length >= 2) polylines.push(poly);
	}
	return polylines;
}

/** Corner allocator: quantized world position → ONE bound projected Point. */
function makeCornerAllocator(anchor) {
	/** @type {Map<string, number>} */
	const corners = new Map();
	return (x, y, z) => {
		const key = `${quantize6(x)},${quantize6(y)},${quantize6(z)}`;
		let id = corners.get(key);
		if (id == null) {
			id = addBoundProjectedPoint(x, y, z, {
				geom_ref: vertexRefAt(anchor, x, y, z),
				kind: { type: 'Vertex' },
			});
			corners.set(key, id);
		}
		return id;
	};
}

/**
 * Project ONE edge range as construction geometry. Endpoints go through the
 * shared corner allocator (bound, deduped → adjacent ranges chain);
 * curved-edge interiors become a static polyline (invariant O4 + the
 * curved-edge snapshot contract in specs/projected_sketch_geometry.md).
 * Caller wraps in begin/endSketchAction. Returns lines created.
 */
function projectEdgeRange(mesh, range, getCorner) {
	const verts = mesh.edges.vertices;
	const si = range.start_index;
	const ei = range.end_index;
	if (ei - si < 2) return 0;

	// Analytic circular edges whose plane is PARALLEL to the sketch plane
	// project isometrically — mint a TRUE Arc/Circle (construction) instead
	// of a polyline, so offsets of rounded outlines keep real radii.
	const curve = range.curve;
	if (curve) {
		const sn = sketchMode.normal;
		const dn = curve.normal[0] * sn[0] + curve.normal[1] * sn[1] + curve.normal[2] * sn[2];
		if (Math.abs(Math.abs(dn) - 1) < 1e-6) {
			const c2 = worldToSketch2D(curve.center[0], curve.center[1], curve.center[2]);
			const centerId = allocEntityId();
			addLocalEntity({ type: 'Point', id: centerId, x: c2.u, y: c2.v, construction: true });
			if (curve.kind === 'circle') {
				addLocalEntity({
					type: 'Circle',
					id: allocEntityId(),
					center_id: centerId,
					radius: curve.radius,
					construction: true,
				});
				return 1;
			}
			// Arc: bound endpoints through the shared corner allocator so the
			// boundary chains. The 3D arc runs CCW around curve.normal; the
			// sketch Arc entity is CCW start→end, so an anti-parallel normal
			// swaps the endpoints.
			const pa = getCorner(verts[si * 3], verts[si * 3 + 1], verts[si * 3 + 2]);
			const pb = getCorner(verts[(ei - 1) * 3], verts[(ei - 1) * 3 + 1], verts[(ei - 1) * 3 + 2]);
			if (pa !== pb) {
				addLocalEntity({
					type: 'Arc',
					id: allocEntityId(),
					center_id: centerId,
					start_id: dn > 0 ? pa : pb,
					end_id: dn > 0 ? pb : pa,
					construction: true,
				});
				return 1;
			}
			return 0;
		}
	}

	const a = getCorner(verts[si * 3], verts[si * 3 + 1], verts[si * 3 + 2]);
	const b = getCorner(verts[(ei - 1) * 3], verts[(ei - 1) * 3 + 1], verts[(ei - 1) * 3 + 2]);
	let lines = 0;

	if (ei - si === 2) {
		// Straight edge: one bound construction line.
		if (a !== b) {
			addLocalEntity({ type: 'Line', id: allocEntityId(), start_id: a, end_id: b, construction: true });
			lines++;
		}
		return lines;
	}

	// Curved edge (e.g. a rounded board corner): endpoints are model
	// vertices — bound and deduped through the shared corners map so the
	// projected boundary stays ONE connected loop. Interior points are not
	// vertices, so they become a static construction polyline.
	const CURVE_SIMPLIFY_TOL = 1e-5;
	const end2 = worldToSketch2D(verts[(ei - 1) * 3], verts[(ei - 1) * 3 + 1], verts[(ei - 1) * 3 + 2]);
	let last2 = worldToSketch2D(verts[si * 3], verts[si * 3 + 1], verts[si * 3 + 2]);
	const kept = [];
	for (let k = si + 1; k < ei - 1; k++) {
		const p2 = worldToSketch2D(verts[k * 3], verts[k * 3 + 1], verts[k * 3 + 2]);
		if (Math.hypot(p2.u - last2.u, p2.v - last2.v) < CURVE_SIMPLIFY_TOL) continue;
		kept.push(p2);
		last2 = p2;
	}
	while (kept.length && Math.hypot(kept[kept.length - 1].u - end2.u, kept[kept.length - 1].v - end2.v) < CURVE_SIMPLIFY_TOL) {
		kept.pop();
	}
	let prevId = a;
	for (const p2 of kept) {
		const pid = allocEntityId();
		addLocalEntity({ type: 'Point', id: pid, x: p2.u, y: p2.v, construction: true });
		addLocalEntity({ type: 'Line', id: allocEntityId(), start_id: prevId, end_id: pid, construction: true });
		lines++;
		prevId = pid;
	}
	if (prevId !== b) {
		addLocalEntity({ type: 'Line', id: allocEntityId(), start_id: prevId, end_id: b, construction: true });
		lines++;
	}
	return lines;
}

/**
 * Map a JS sketch constraint to the Rust bridge format.
 * Some constraint type names differ between JS and Rust (waffle-types).
 * @param {object} c - Constraint in JS format
 * @returns {object | null} Constraint in Rust bridge format, or null to skip
 */
function mapConstraintForBridge(c) {
	if (c.type === 'WhereDragged') {
		// Live drag pins keep Dragged semantics (weight 1/20, target = the
		// point's current position, refreshed per pointermove). Persistent
		// pins (origin/reference snaps, the Fix modal) lower to Pinned so the
		// stored (x, y) target survives every solve at full constraint weight
		// — Dragged would re-anchor to the drifted position and let the lock
		// walk away. See specs/pinned_constraint.md.
		if (c._isDrag || c.x == null || c.y == null) {
			return { type: 'Dragged', point: c.point };
		}
		return { type: 'Pinned', point: c.point, x: c.x, y: c.y };
	}
	// `EqualRadius` and `LengthRatio` are UI-side names with no matching
	// waffle-types variant — the Rust enum spells them `Equal` and `Ratio`.
	// Sent unmapped, serde rejects the whole message ("unknown variant
	// `EqualRadius`"), and because SolveSketch resends the FULL constraint
	// list every subsequent solve in the sketch fails to parse too — one
	// Equal-Radius click silently kills constraint solving for the rest of
	// the session. `Equal` dispatches on entity kind (circle/circle,
	// arc/arc, circle/arc), which covers every selection that produces
	// these. Emitted by the toolbar's Eq button, the right-click menu's
	// Equal Radius / Length Ratio items, and the sketch fillet tool.
	if (c.type === 'EqualRadius') {
		return { type: 'Equal', entity_a: c.entity_a, entity_b: c.entity_b };
	}
	if (c.type === 'LengthRatio') {
		return { type: 'Ratio', entity_a: c.entity_a, entity_b: c.entity_b, value: c.value };
	}
	return c;
}

/**
 * Add a constraint locally and send to engine.
 * @param {object} constraint - SketchConstraint object
 */
/** Constraint types that carry a length (an Angle is not one). */
const LENGTH_DIMENSION_TYPES = new Set(['Distance', 'HDistance', 'VDistance', 'PointLineDistance', 'Diameter', 'Radius']);

/**
 * Measure the CURRENT value of a length-dimension constraint against the live
 * sketch geometry (meters). null when its entities cannot be resolved.
 * @param {any} c
 * @returns {number | null}
 */
function measureLengthDimension(c) {
	const pos = (id) => sketchPositions.get(id);
	const ent = (id) => sketchEntities.find((e) => e.id === id);
	const circleRadius = (e) => {
		if (!e) return null;
		if (e.type === 'Circle') return e.radius ?? null;
		if (e.type === 'Arc') {
			const ctr = pos(e.center_id), st = pos(e.start_id);
			return ctr && st ? Math.hypot(st.x - ctr.x, st.y - ctr.y) : null;
		}
		return null;
	};
	switch (c.type) {
		case 'Distance':
		case 'HDistance':
		case 'VDistance': {
			const a = pos(c.entity_a), b = pos(c.entity_b);
			if (!a || !b) return null;
			if (c.type === 'HDistance') return Math.abs(b.x - a.x);
			if (c.type === 'VDistance') return Math.abs(b.y - a.y);
			return Math.hypot(b.x - a.x, b.y - a.y);
		}
		case 'PointLineDistance': {
			const p = pos(c.point);
			const line = ent(c.entity);
			if (!p || !line || line.type !== 'Line') return null;
			const a = pos(line.start_id), b = pos(line.end_id);
			if (!a || !b) return null;
			const dx = b.x - a.x, dy = b.y - a.y;
			const len = Math.hypot(dx, dy);
			if (len < 1e-15) return null;
			return Math.abs((p.x - a.x) * dy - (p.y - a.y) * dx) / len;
		}
		case 'Diameter': {
			const r = circleRadius(ent(c.entity));
			return r == null ? null : r * 2;
		}
		case 'Radius':
			return circleRadius(ent(c.entity));
		default:
			return null;
	}
}

/**
 * If `constraint` is the FIRST driving length dimension on this sketch (and the
 * setting is on), scale every point and radius about the sketch origin so the
 * dimension is already satisfied and the sketch keeps its proportions. Returns
 * the radius changes for the undo record, or null when nothing was scaled.
 * Skipped for sketches with projected (externally driven) points or gears —
 * those carry geometry the scale must not touch.
 * @param {any} constraint
 * @returns {{ radiiBefore: Array<[number, number]>, radiiAfter: Array<[number, number]> } | null}
 */
function maybeScaleSketchToFirstDimension(constraint) {
	if (!sketchMode.active) return null;
	if (!getSetting('sketchScaleOnFirstDimension')) return null;
	if (!LENGTH_DIMENSION_TYPES.has(constraint?.type) || constraint.reference || constraint._isDrag) return null;
	if (!(constraint.value > 0)) return null;
	// "First": no other driving length dimension exists yet. The new one was
	// already appended by the caller as the LAST element (identity checks do
	// not work — $state wraps it in a proxy), so exclude it by position.
	const others = sketchConstraints.slice(0, -1).filter((c) => LENGTH_DIMENSION_TYPES.has(c.type) && !c.reference && !c._isDrag);
	if (others.length > 0) return null;
	if (projectedBindings.length > 0) return null;
	if (sketchEntities.some((e) => e.type === 'Gear' || e.type === 'Sprocket')) return null;

	const measured = measureLengthDimension(constraint);
	if (measured == null || !(measured > 1e-12)) return null;
	const s = constraint.value / measured;
	if (!Number.isFinite(s) || !(s > 0) || Math.abs(s - 1) < 1e-9) return null;

	const nextPos = new Map();
	for (const [id, p] of sketchPositions) nextPos.set(id, { x: p.x * s, y: p.y * s });
	sketchPositions = nextPos;

	/** @type {Array<[number, number]>} */
	const radiiBefore = [];
	/** @type {Array<[number, number]>} */
	const radiiAfter = [];
	sketchEntities = sketchEntities.map((e) => {
		if (e.type === 'Circle' && typeof e.radius === 'number') {
			radiiBefore.push([e.id, e.radius]);
			radiiAfter.push([e.id, e.radius * s]);
			return { ...e, radius: e.radius * s };
		}
		return e;
	});
	log('sketch', 'First dimension scaled the sketch', { type: constraint.type, measured: +measured.toPrecision(6), target: constraint.value, scale: +s.toPrecision(6) });
	reExtractProfiles();
	return { radiiBefore, radiiAfter };
}

/** Apply a list of [entityId, radius] pairs to circle entities. */
function applyRadii(pairs) {
	if (!pairs || !pairs.length) return;
	const m = new Map(pairs);
	sketchEntities = sketchEntities.map((e) => (m.has(e.id) ? { ...e, radius: m.get(e.id) } : e));
}

export function addLocalConstraint(constraint) {
	log('sketch', `Constraint added: ${constraint.type}`, { type: constraint.type });
	sketchConstraints = [...sketchConstraints, constraint];
	recomputeOverConstrained();

	// Record for sketch undo
	const cloned = JSON.parse(JSON.stringify(constraint));
	if (pendingSketchAction) {
		pendingSketchAction.constraints.push(cloned);
	} else if (sketchMode.active) {
		// Snapshot BEFORE the constraint's solve (triggerSolve runs below) so undo
		// can revert any point movement the new constraint induces.
		sketchUndoStack = [...sketchUndoStack, { entities: [], constraints: [cloned], positionsBefore: snapshotPositions(), camera: getCameraState() }];
		sketchRedoStack = [];
	}

	// First driving dimension on an undimensioned sketch: scale the whole
	// sketch proportionally to it (like Onshape), so the rest of the geometry
	// keeps the shape the user drew instead of the solver dragging one edge.
	// The undo record above already holds the pre-scale positions; the radii
	// it changed are attached to it so undo restores them too.
	const scaled = maybeScaleSketchToFirstDimension(constraint);
	if (scaled && !pendingSketchAction && sketchMode.active && sketchUndoStack.length) {
		const last = sketchUndoStack[sketchUndoStack.length - 1];
		sketchUndoStack = [...sketchUndoStack.slice(0, -1), { ...last, radiiBefore: scaled.radiiBefore, radiiAfter: scaled.radiiAfter, positionsAfter: snapshotPositions() }];
	}

	if (bridge && engineReady) {
		// Map JS constraint names to Rust bridge names
		const bridgeConstraint = mapConstraintForBridge(cloned);
		if (bridgeConstraint) {
			bridge.send({ type: 'AddConstraint', constraint: bridgeConstraint })
				.catch(err => log('error', `AddConstraint failed: ${err}`));
		}
	}

	triggerSolve();
}

// --- Constraint modal (constraint-first application) ---

export function getConstraintModal() { return constraintModal; }

/**
 * Open the constraint modal for a given geometric constraint id, switching the
 * sketch into the constraint pick-loop tool. No-op for non-modal constraints
 * (e.g. dimensional ones). Clears the current selection so picks start fresh.
 * @param {string} constraintId
 */
export function openConstraintModal(constraintId) {
	if (!sketchMode.active || !isModalConstraint(constraintId)) return;
	constraintModal = { constraintId, running: [], message: modalInstruction(constraintId) };
	setSketchSelection(new Set());
	setSelectedProfileIndex(null);
	setActiveTool('constraint');
}

/** Close the constraint modal and return to the select tool. */
export function closeConstraintModal() {
	if (!constraintModal) return;
	constraintModal = null;
	setSketchSelection(new Set());
	if (activeTool === 'constraint') setActiveTool('select');
}

/**
 * Feed a viewport pick (entity id, or null for empty space) into the active
 * constraint modal. Applies the constraint(s) the engine decides on, advances
 * the running selection, and highlights it. See /specs/constraint_modal.md.
 * @param {number | null} pickId
 */
export function constraintModalPick(pickId) {
	if (!constraintModal) return;
	const step = stepConstraintModal({
		constraintId: constraintModal.constraintId,
		running: constraintModal.running,
		pickId,
		entities: sketchEntities,
		positions: sketchPositions,
	});

	if (step.action === 'apply') {
		for (const c of step.constraints) addLocalConstraint(c);
	}

	constraintModal = {
		constraintId: constraintModal.constraintId,
		running: step.nextRunning,
		message: step.message ?? modalInstruction(constraintModal.constraintId),
	};
	// Highlight the running selection (e.g. the chain anchor) so the user sees
	// what the next pick will bind to.
	setSketchSelection(new Set(step.nextRunning));
	return step.action;
}

/**
 * Update a dimensional constraint's value locally.
 * @param {number} index - Index into sketchConstraints array
 * @param {number} newValue - New dimension value
 */
export function updateConstraintValue(index, newValue) {
	if (index < 0 || index >= sketchConstraints.length) return;
	const c = { ...sketchConstraints[index] };
	if ('value' in c) c.value = newValue;
	else if ('value_degrees' in c) c.value_degrees = newValue;
	// A plain numeric edit detaches any driving expression.
	delete c.expression;
	sketchConstraints = [
		...sketchConstraints.slice(0, index),
		c,
		...sketchConstraints.slice(index + 1)
	];

	triggerSolve();
}

/**
 * Drive a dimensional constraint with an expression. `internalValue` is the
 * engine-evaluated result converted to internal units (meters for lengths,
 * degrees for angles) so the live solver sees the number immediately; the
 * expression itself persists on the constraint and re-evaluates on rebuild.
 * @param {number} index - Index into sketchConstraints array
 * @param {string} expression
 * @param {number} internalValue
 */
export function updateConstraintExpression(index, expression, internalValue) {
	if (index < 0 || index >= sketchConstraints.length) return;
	const c = { ...sketchConstraints[index] };
	if ('value' in c) c.value = internalValue;
	else if ('value_degrees' in c) c.value_degrees = internalValue;
	c.expression = expression;
	sketchConstraints = [
		...sketchConstraints.slice(0, index),
		c,
		...sketchConstraints.slice(index + 1)
	];

	triggerSolve();
}

/**
 * Toggle a constraint between driving and reference mode.
 * Reference constraints are not sent to the solver — they display measured values.
 * @param {number} index - Index into sketchConstraints array
 */
export function toggleConstraintReference(index) {
	if (index < 0 || index >= sketchConstraints.length) return;
	const c = { ...sketchConstraints[index] };
	c.reference = !c.reference;
	sketchConstraints = [
		...sketchConstraints.slice(0, index),
		c,
		...sketchConstraints.slice(index + 1)
	];

	triggerSolve();
}

/**
 * Find a point near the given coordinates.
 * @param {number} x
 * @param {number} y
 * @param {number} threshold
 * @returns {{ id: number, x: number, y: number } | null}
 */
export function findPointNear(x, y, threshold) {
	let closest = null;
	let closestDist = threshold;
	for (const [id, pos] of sketchPositions) {
		const dx = pos.x - x;
		const dy = pos.y - y;
		const dist = Math.sqrt(dx * dx + dy * dy);
		if (dist < closestDist) {
			closestDist = dist;
			closest = { id, x: pos.x, y: pos.y };
		}
	}
	return closest;
}

/**
 * Find a line near the given coordinates (perpendicular distance).
 * @param {number} x
 * @param {number} y
 * @param {number} threshold
 * @returns {{ id: number, dist: number } | null}
 */
export function findLineNear(x, y, threshold) {
	let closest = null;
	let closestDist = threshold;
	for (const entity of sketchEntities) {
		if (entity.type !== 'Line') continue;
		const p1 = sketchPositions.get(entity.start_id);
		const p2 = sketchPositions.get(entity.end_id);
		if (!p1 || !p2) continue;

		const dist = pointToSegmentDist(x, y, p1.x, p1.y, p2.x, p2.y);
		if (dist < closestDist) {
			closestDist = dist;
			closest = { id: entity.id, dist };
		}
	}
	return closest;
}

/**
 * Find a circle/arc near the given coordinates (distance to circumference).
 * @param {number} x
 * @param {number} y
 * @param {number} threshold
 * @returns {{ id: number, dist: number } | null}
 */
export function findCircleNear(x, y, threshold) {
	let closest = null;
	let closestDist = threshold;
	for (const entity of sketchEntities) {
		if (entity.type !== 'Circle' && entity.type !== 'Arc') continue;
		const center = sketchPositions.get(entity.center_id);
		if (!center) continue;

		let radius;
		if (entity.type === 'Circle') {
			radius = entity.radius;
		} else {
			const startPt = sketchPositions.get(entity.start_id);
			if (!startPt) continue;
			const dx = startPt.x - center.x;
			const dy = startPt.y - center.y;
			radius = Math.sqrt(dx * dx + dy * dy);
		}

		const dx = x - center.x;
		const dy = y - center.y;
		const distToCenter = Math.sqrt(dx * dx + dy * dy);
		const dist = Math.abs(distToCenter - radius);
		if (dist < closestDist) {
			closestDist = dist;
			closest = { id: entity.id, dist };
		}
	}
	return closest;
}

/**
 * Perpendicular distance from point to line segment.
 */
function pointToSegmentDist(px, py, ax, ay, bx, by) {
	const abx = bx - ax, aby = by - ay;
	const len2 = abx * abx + aby * aby;
	if (len2 < 1e-12) {
		const dx = px - ax, dy = py - ay;
		return Math.sqrt(dx * dx + dy * dy);
	}
	let t = ((px - ax) * abx + (py - ay) * aby) / len2;
	t = Math.max(0, Math.min(1, t));
	const cx = ax + t * abx, cy = ay + t * aby;
	const dx = px - cx, dy = py - cy;
	return Math.sqrt(dx * dx + dy * dy);
}

/**
 * Find a spline near the given coordinates.
 * Samples the spline curve and checks min distance to each segment.
 * @param {number} x
 * @param {number} y
 * @param {number} threshold
 * @returns {{ id: number, dist: number } | null}
 */
export function findSplineNear(x, y, threshold) {
	let closest = null;
	let closestDist = threshold;
	for (const entity of sketchEntities) {
		if (entity.type !== 'Spline') continue;
		if (!entity.point_ids || entity.point_ids.length < 2) continue;

		const ctrlPts = entity.point_ids
			.map(pid => sketchPositions.get(pid))
			.filter(Boolean);
		if (ctrlPts.length < 2) continue;

		const samples = sampleBSpline(ctrlPts, 32);

		for (let i = 0; i < samples.length - 1; i++) {
			const p1 = samples[i];
			const p2 = samples[i + 1];
			const dist = pointToSegmentDist(x, y, p1.x, p1.y, p2.x, p2.y);
			if (dist < closestDist) {
				closestDist = dist;
				closest = { id: entity.id, dist };
			}
		}
	}
	return closest;
}

/**
 * Toggle an entity's construction flag.
 * @param {number} entityId
 */
export function toggleConstruction(entityId) {
	const idx = sketchEntities.findIndex(e => e.id === entityId);
	if (idx < 0) return;
	const entity = { ...sketchEntities[idx] };
	entity.construction = !entity.construction;
	sketchEntities = [
		...sketchEntities.slice(0, idx),
		entity,
		...sketchEntities.slice(idx + 1)
	];
	reExtractProfiles();
}

/**
 * Remove sketch entities by ID, cascade-delete referencing constraints,
 * and remove orphaned points. Pushes to undo stack for reversibility.
 * @param {Set<number>} entityIds - IDs of entities to remove
 */
export function removeSketchEntities(entityIds) {
	if (entityIds.size === 0) return;

	// Collect all entities to remove (including orphaned points from line/circle/arc deletion)
	const toRemove = new Set(entityIds);

	// Find entities being deleted and their referenced point IDs
	const deletedEntities = sketchEntities.filter(e => toRemove.has(e.id));
	const referencedPointIds = new Set();
	for (const e of deletedEntities) {
		if (e.type === 'Line') {
			referencedPointIds.add(e.start_id);
			referencedPointIds.add(e.end_id);
		} else if (e.type === 'Circle') {
			referencedPointIds.add(e.center_id);
		} else if (e.type === 'Arc') {
			referencedPointIds.add(e.center_id);
			referencedPointIds.add(e.start_id);
			referencedPointIds.add(e.end_id);
		}
	}

	// If deleting a point, find all entities that reference it and delete them too
	for (const e of sketchEntities) {
		if (toRemove.has(e.id)) continue;
		if (e.type === 'Line' && (toRemove.has(e.start_id) || toRemove.has(e.end_id))) {
			toRemove.add(e.id);
			// Also track points from cascaded lines
			referencedPointIds.add(e.start_id);
			referencedPointIds.add(e.end_id);
		}
		if (e.type === 'Circle' && toRemove.has(e.center_id)) {
			toRemove.add(e.id);
		}
		if (e.type === 'Arc' && (toRemove.has(e.center_id) || toRemove.has(e.start_id) || toRemove.has(e.end_id))) {
			toRemove.add(e.id);
			referencedPointIds.add(e.center_id);
			referencedPointIds.add(e.start_id);
			referencedPointIds.add(e.end_id);
		}
		if (e.type === 'Spline' && e.point_ids?.some(pid => toRemove.has(pid))) {
			toRemove.add(e.id);
			for (const pid of e.point_ids) referencedPointIds.add(pid);
		}
	}

	// Check which referenced points are orphaned (not used by any surviving entity)
	const survivingEntities = sketchEntities.filter(e => !toRemove.has(e.id));
	const usedPointIds = new Set();
	for (const e of survivingEntities) {
		if (e.type === 'Line') { usedPointIds.add(e.start_id); usedPointIds.add(e.end_id); }
		if (e.type === 'Circle') { usedPointIds.add(e.center_id); }
		if (e.type === 'Arc') { usedPointIds.add(e.center_id); usedPointIds.add(e.start_id); usedPointIds.add(e.end_id); }
		if (e.type === 'Spline' && e.point_ids) { for (const pid of e.point_ids) usedPointIds.add(pid); }
	}
	for (const ptId of referencedPointIds) {
		if (!usedPointIds.has(ptId) && !toRemove.has(ptId)) {
			toRemove.add(ptId);
		}
	}

	// Find constraints that reference any removed entity
	const removedConstraints = [];
	const survivingConstraints = [];
	for (const c of sketchConstraints) {
		const refs = [c.entity, c.entity_a, c.entity_b, c.entity_c,
			c.line, c.curve, c.line_a, c.line_b,
			c.point, c.point_a, c.point_b].filter(v => v != null);
		if (refs.some(id => toRemove.has(id))) {
			removedConstraints.push(JSON.parse(JSON.stringify(c)));
		} else {
			survivingConstraints.push(c);
		}
	}

	// Collect removed entities for undo
	const removedEntities = sketchEntities.filter(e => toRemove.has(e.id))
		.map(e => JSON.parse(JSON.stringify(e)));

	// Push to undo stack
	if (removedEntities.length > 0 || removedConstraints.length > 0) {
		sketchUndoStack = [...sketchUndoStack, {
			entities: removedEntities,
			constraints: removedConstraints,
			_isDeletion: true,
			camera: getCameraState()
		}];
		sketchRedoStack = [];
	}

	// Apply removals
	sketchEntities = survivingEntities.filter(e => !toRemove.has(e.id));
	sketchConstraints = survivingConstraints;

	// Update positions map
	const nextPos = new Map(sketchPositions);
	for (const id of toRemove) {
		nextPos.delete(id);
	}
	sketchPositions = nextPos;

	// Purge gear bookkeeping for any removed Gear entity (registry + display map).
	for (const id of toRemove) {
		const gid = entityToGearMap.get(id);
		if (gid == null) continue;
		const nextReg = new Map(gearRegistry); nextReg.delete(gid); gearRegistry = nextReg;
		const nextMap = new Map(entityToGearMap); nextMap.delete(id); entityToGearMap = nextMap;
		const nextDisp = new Map(gearDisplay); nextDisp.delete(gid); gearDisplay = nextDisp;
	}

	// Clear selection
	sketchSelection = new Set();

	recomputeOverConstrained();
	reExtractProfiles();
	triggerSolve();

	log('sketch', `Deleted ${removedEntities.length} entities, ${removedConstraints.length} constraints`);
}

/**
 * Remove a sketch constraint by index. Re-solves and re-extracts profiles.
 * @param {number} index - Index into sketchConstraints array
 */
export function removeSketchConstraint(index) {
	if (index < 0 || index >= sketchConstraints.length) return;

	const removed = JSON.parse(JSON.stringify(sketchConstraints[index]));
	sketchConstraints = [
		...sketchConstraints.slice(0, index),
		...sketchConstraints.slice(index + 1)
	];

	// Push to undo stack as a constraint-only action
	sketchUndoStack = [...sketchUndoStack, {
		entities: [],
		constraints: [removed],
		_isDeletion: true,
		camera: getCameraState()
	}];
	sketchRedoStack = [];

	recomputeOverConstrained();
	reExtractProfiles();
	triggerSolve();

	log('sketch', `Deleted constraint: ${removed.type}`);
}

// -- Gear CRUD --

/**
 * Get gear registry.
 * @returns {Map<number, object>}
 */
export function getGearRegistry() { return gearRegistry; }

/**
 * Get the ephemeral per-gear display expansion (primitives + profiles derived
 * from each compact `Gear` entity). Not persisted; for rendering / profile
 * preview only.
 * @returns {Map<number, object>}
 */
export function getGearDisplay() { return gearDisplay; }

/**
 * Build the display expansion for a gear from its params via WASM, and store it
 * in `gearDisplay` keyed by gearId. Shared by create and load paths.
 * @param {number} gearId
 * @param {object} gearParams - canonical GearParams (camelCase)
 * @returns {Promise<object>} the display expansion entry
 */
/**
 * Remap a raw gear-profile response into a display entry: primitive entities
 * (curves + construction pitch circle) and positions in a collision-free id
 * range (`base`), plus the boundary polygon for hit-testing.
 * @param {object} response - GenerateGearProfile response
 * @param {number} base - id offset for this gear's primitives
 * @returns {{ entities: object[], positions: Map<number,{x:number,y:number}>, pitchRadius: number, outline: Array<{x:number,y:number}> }}
 */
function remapGearResponse(response, base) {
	const remap = (id) => base + id;
	const positions = new Map();
	const entities = response.entities.map((e) => {
		const m = { ...e, id: remap(e.id) };
		if (m.start_id != null) m.start_id = remap(m.start_id);
		if (m.end_id != null) m.end_id = remap(m.end_id);
		if (m.center_id != null) m.center_id = remap(m.center_id);
		if (m.point_ids) m.point_ids = m.point_ids.map(remap);
		if (m.type === 'Point') positions.set(m.id, { x: m.x, y: m.y });
		return m;
	});
	// Pitch circle: a construction reference centered on the gear center (always
	// the first emitted point — external and internal both lead with the center).
	if (response.entities.length > 0) {
		const centerId = remap(response.entities[0].id);
		entities.push({
			type: 'Circle',
			id: base + 90000,
			center_id: centerId,
			radius: response.pitch_radius,
			construction: true
		});
	}
	// Boundary polygon (sketch coords) for click hit-testing — the profile's
	// ordered vertex loop, resolved from the un-remapped point coords.
	const localPos = new Map();
	for (const e of response.entities) {
		if (e.type === 'Point') localPos.set(e.id, { x: e.x, y: e.y });
	}
	const outline = (response.profiles?.[0]?.vertex_ids ?? [])
		.map(id => localPos.get(id))
		.filter(Boolean);
	return { entities, positions, pitchRadius: response.pitch_radius, outline };
}

/**
 * The stateless engine message that expands a compact generator entity of
 * `kind` (`'Gear'` | `'Sprocket'`) into its display primitives. Both answer
 * with the same shape (`entities`, `positions`, `profiles`, `pitch_radius`),
 * so one display path serves both.
 * @param {string} kind
 */
function generatorProfileMessage(kind) {
	return kind === 'Sprocket' ? 'GenerateSprocketProfile' : 'GenerateGearProfile';
}

async function expandGearForDisplay(gearId, gearParams, kind = 'Gear') {
	// Deep-clone: gearParams may be a Svelte reactive proxy (e.g. from a loaded
	// Gear entity), which postMessage cannot structured-clone to the worker.
	const params = JSON.parse(JSON.stringify(gearParams));
	const response = await bridge.send({ type: generatorProfileMessage(kind), params });
	const entry = remapGearResponse(response, gearDisplayIdBase(gearId));
	const next = new Map(gearDisplay);
	next.set(gearId, entry);
	gearDisplay = next;
	return entry;
}

// -- Inactive (completed) sketch gear display --
// Gears in finished sketches are rendered by InactiveSketchRenderer, which has
// no access to the per-edit `gearDisplay`. Expand them into this cache, keyed by
// `${featureId}:${entityId}`, so completed gear sketches render their teeth.
let inactiveGearDisplay = $state(new Map());

/** Per-gear id range of an inactive sketch's gear expansion, distinct from the active `gearDisplay` range. */
function inactiveGearIdBase(entityId) {
	return 50_000_000 + entityId * 100_000;
}

/** @returns {Map<string, object>} */
export function getInactiveGearDisplay() { return inactiveGearDisplay; }

/**
 * Ensure every gear in the given inactive sketches is expanded into
 * `inactiveGearDisplay`, and drop entries no longer present. Idempotent.
 * @param {Array<{ key: string, entityId: number, params: object, kind?: string }>} specs
 *   — `kind` is the compact entity's type (`'Gear'` default, or `'Sprocket'`)
 * @param {(message: object) => Promise<any>} [send] - the agent link passes its
 *   own sender, because it already holds the engine lock
 */
export async function ensureInactiveGearsExpanded(specs, send = (message) => bridge.send(message)) {
	const wanted = new Set(specs.map(s => s.key));
	let changed = false;
	const next = new Map(inactiveGearDisplay);
	for (const k of [...next.keys()]) {
		if (!wanted.has(k)) { next.delete(k); changed = true; }
	}
	for (const { key, entityId, params, kind } of specs) {
		if (next.has(key)) continue;
		const p = JSON.parse(JSON.stringify(params));
		const response = await send({ type: generatorProfileMessage(kind ?? 'Gear'), params: p });
		next.set(key, remapGearResponse(response, inactiveGearIdBase(entityId)));
		changed = true;
	}
	if (changed) inactiveGearDisplay = next;
}

/**
 * Get gear dialog state.
 * @returns {object | null}
 */
export function getGearDialogState() { return gearDialogState; }

/**
 * Get the gear ID that an entity belongs to.
 * @param {number} entityId
 * @returns {number | null}
 */
export function getGearIdForEntity(entityId) {
	return entityToGearMap.get(entityId) ?? null;
}

/**
 * Show the gear dialog with the given parameters.
 * @param {object} params
 */
export function showGearDialog(params) {
	gearDialogState = params;
}

/**
 * Hide the gear dialog.
 */
export function hideGearDialog() {
	gearDialogState = null;
}

/** @returns {object | null} */
export function getSprocketDialogState() { return sprocketDialogState; }

/**
 * Show the sprocket dialog. Mirrors `showGearDialog`: the placement tool
 * seeds `{ centerX, centerY, rotationOffset }`; the edit gesture adds
 * `{ editGearId, params }` (the registry entry, `kind: 'Sprocket'`).
 * @param {object} params
 */
export function showSprocketDialog(params) {
	sprocketDialogState = params;
}

/** Hide the sprocket dialog. */
export function hideSprocketDialog() {
	sprocketDialogState = null;
}

/** @returns {boolean} */
export function getPlanetaryDialogOpen() { return planetaryDialogState != null; }

/** @returns {object | null} The seeded dialog state ({ centerX, centerY }). */
export function getPlanetaryDialogState() { return planetaryDialogState; }

/**
 * Show the planetary gear dialog seeded with a placement center. Mirrors
 * `showGearDialog`. Called by the planetary placement tool on click.
 * @param {object} [params] - { centerX, centerY } in internal sketch coords.
 */
export function showPlanetaryGearDialog(params = {}) {
	planetaryDialogState = { centerX: params.centerX ?? 0, centerY: params.centerY ?? 0 };
}

/** Show the planetary gear dialog centered at the origin (legacy entry). */
export function showPlanetaryDialog() { showPlanetaryGearDialog({ centerX: 0, centerY: 0 }); }

/** Hide the planetary gear dialog. */
export function hidePlanetaryDialog() { planetaryDialogState = null; }

/**
 * Create a gear from parameters.
 *
 * The gear is stored as a single compact `Gear` sketch entity (the canonical,
 * persisted form — Rust expands it on rebuild/extrude and the solver skips it,
 * so a gear is inherently a rigid block). The primitive geometry used to draw
 * the gear and preview its profile is held separately in `gearDisplay` and is
 * never persisted. This is what lets gear grouping survive save/reload.
 * @param {object} gearParams - { toothCount, module, pressureAngleDeg, backlash, centerX, centerY, rotationOffset, internal }
 * @returns {Promise<number>} The gear ID
 */
export async function createGear(gearParams) {
	beginSketchAction();
	const gearId = await addGearFromParams(gearParams);
	endSketchAction();
	return gearId;
}

/**
 * Add a single Gear entity (display expansion + compact entity + registry)
 * WITHOUT managing the undo-action grouping. Callers wrap one or more of these
 * in a single `beginSketchAction()`/`endSketchAction()` so a multi-gear
 * operation (e.g. a planetary stage) is ONE undo step.
 * @param {object} gearParams
 * @returns {Promise<number>} The gear ID
 */
async function addGearFromParams(gearParams) {
	return addGeneratorFromParams('Gear', gearParams);
}

/**
 * Add one compact generator entity (`Gear` or `Sprocket`): display expansion +
 * the compact entity + registry bookkeeping. The gear registry/display maps
 * serve both kinds — the registry entry records `kind` so the edit gesture and
 * the update path know which generator they hold.
 * @param {'Gear' | 'Sprocket'} kind
 * @param {object} params - the entity's `params` (camelCase, as the engine takes them)
 * @returns {Promise<number>} The registry id
 */
async function addGeneratorFromParams(kind, params) {
	const gearId = nextGearId++;

	// Build the (non-persisted) display expansion from the params.
	await expandGearForDisplay(gearId, params, kind);

	// Store the single compact entity — this is the persisted representation.
	const gearEntityId = allocEntityId();
	addLocalEntity({
		type: kind,
		id: gearEntityId,
		params: { ...params },
		construction: false
	});

	// Register: one entity id per generator (not a list of expanded primitives).
	const newRegistry = new Map(gearRegistry);
	newRegistry.set(gearId, { ...params, entityId: gearEntityId, kind });
	gearRegistry = newRegistry;

	const newEntityMap = new Map(entityToGearMap);
	newEntityMap.set(gearEntityId, gearId);
	entityToGearMap = newEntityMap;

	if (kind === 'Sprocket') {
		log('sketch', `Sprocket created: ${params.toothCount} teeth, pitch ${params.pitch}`, { gearId });
	} else {
		log('sketch', `Gear created: ${params.toothCount} teeth, module ${params.module}`, { gearId });
	}
	return gearId;
}

/**
 * Create an ISO 606 roller-chain sprocket (spec
 * `specs/custom_features_and_modeling_roadmap.md` §B3) as one compact
 * `Sprocket` sketch entity, like `createGear`. Invalid parameters are the
 * engine's typed refusal (the promise rejects; nothing is added).
 * @param {object} params - { toothCount, pitch, rollerDiameter, centerX?, centerY?, rotationOffset?, seatingRadius?, flankRadius?, tipDiameter?, seatingAngleDeg? }
 * @returns {Promise<number>} The registry id
 */
export async function createSprocket(params) {
	beginSketchAction();
	try {
		return await addGeneratorFromParams('Sprocket', params);
	} finally {
		endSketchAction();
	}
}

/**
 * Generate a planetary gear stage (sun + N planets + ring) and add all N+2
 * gears to the ACTIVE sketch as ONE undo step.
 *
 * The Rust core (`generate_planetary`) validates the tooth-count / assembly /
 * non-interference constraints, computes the positioned `GearParams` with the
 * meshing phasing, and either blocks (hint mode) or auto-adjusts. We surface
 * its hints (and any blocking validation error) as toasts — never a silent bad
 * sketch. Requires an active sketch (like `createGear`).
 *
 * @param {object} params - { module, pressureAngleDeg, sunTeeth, planetTeeth, planetCount, backlash, autoAdjust }
 *   `module`/`backlash` are in INTERNAL units (meters); the dialog converts.
 * @returns {Promise<{ gearIds: number[], result: object } | null>} null if blocked/invalid
 */
export async function createPlanetary(params) {
	if (!sketchMode.active) {
		showToast('error', 'Start or open a sketch before creating a planetary stage');
		return null;
	}
	if (!bridge) {
		showToast('error', 'Engine not ready');
		return null;
	}

	let result;
	try {
		// Deep-clone to avoid Svelte 5 proxy DataCloneError across postMessage.
		const p = JSON.parse(JSON.stringify(params));
		const response = await bridge.send({ type: 'GeneratePlanetary', params: p });
		result = response.result;
	} catch (err) {
		// Blocking validation error (hint mode, no valid config) — show loudly.
		showToast('warning', `Planetary stage not created: ${err.message || err}`);
		return null;
	}

	// Surface any advisory hints (e.g. auto-adjusted planet count).
	for (const hint of result.hints ?? []) {
		showToast('info', hint);
	}

	// Add all N+2 gears AND a center Point at the sun + each planet as a SINGLE
	// undo step. `result.centers` is sun-first, then N planets (ring shares the
	// sun center, so it is not repeated) → N+1 points.
	beginSketchAction();
	const gearIds = [];
	for (const g of result.gears) {
		// `g` is a GearParams (camelCase serde) — the shape createGear expects.
		gearIds.push(await addGearFromParams({ ...g }));
	}
	const pointIds = [];
	for (const c of result.centers ?? []) {
		const pid = allocEntityId();
		// Regular (non-construction) sketch point — a real snap/constraint target.
		addLocalEntity({ type: 'Point', id: pid, x: c[0], y: c[1], construction: false });
		pointIds.push(pid);
	}
	endSketchAction();

	log('sketch', `Planetary stage created: ${result.gears.length} gears + ${pointIds.length} center points (ring ${result.ringTeeth}t)`);
	return { gearIds, pointIds, result };
}

/**
 * Update an existing gear with new parameters.
 * @param {number} gearId
 * @param {object} newParams
 */
export async function updateGear(gearId, newParams) {
	const existing = gearRegistry.get(gearId);
	if (!existing) return;

	// Remove the old compact Gear entity and its registry/display entries.
	deleteGear(gearId);

	// Recreate with merged params, then re-key the new gear back to gearId so
	// callers (and the entity→gear map) keep referring to the same gear.
	const mergedParams = { ...existing, ...newParams };
	const kind = mergedParams.kind ?? 'Gear';
	delete mergedParams.entityId;
	delete mergedParams.kind;
	beginSketchAction();
	let newGearId;
	try {
		newGearId = await addGeneratorFromParams(kind, mergedParams);
	} finally {
		endSketchAction();
	}

	const newGearData = gearRegistry.get(newGearId);
	const updatedRegistry = new Map(gearRegistry);
	updatedRegistry.delete(newGearId);
	updatedRegistry.set(gearId, newGearData);
	gearRegistry = updatedRegistry;

	const updatedEntityMap = new Map(entityToGearMap);
	updatedEntityMap.set(newGearData.entityId, gearId);
	entityToGearMap = updatedEntityMap;

	const updatedDisplay = new Map(gearDisplay);
	updatedDisplay.set(gearId, updatedDisplay.get(newGearId));
	updatedDisplay.delete(newGearId);
	gearDisplay = updatedDisplay;

	nextGearId--; // Reuse the ID we allocated
}

/**
 * Delete a gear and its compact entity.
 * @param {number} gearId
 */
export function deleteGear(gearId) {
	const existing = gearRegistry.get(gearId);
	if (!existing) return;

	if (existing.entityId != null) {
		removeSketchEntities(new Set([existing.entityId]));
	}

	const newRegistry = new Map(gearRegistry);
	newRegistry.delete(gearId);
	gearRegistry = newRegistry;

	const newEntityMap = new Map(entityToGearMap);
	if (existing.entityId != null) newEntityMap.delete(existing.entityId);
	entityToGearMap = newEntityMap;

	const newDisplay = new Map(gearDisplay);
	newDisplay.delete(gearId);
	gearDisplay = newDisplay;

	log('sketch', `Gear deleted`, { gearId });
}

// -- Drag state --
/** @type {{ pointId: number, originalX: number, originalY: number } | null} */
let dragState = null;

/**
 * Reactive flag: a sketch drag gesture is in progress. The sketch auto-fit
 * in CameraControls must NOT run while this is true — a mid-drag fit
 * rescales the pointer→sketch mapping, which teleports the drag target
 * outward, which grows the geometry, which re-triggers the fit: an
 * exponential feedback loop (observed blowing a 26mm sketch past 4 meters
 * within a single gesture; see sketch-drag-autofit-feedback.spec.js).
 */
let sketchDragActive = $state(false);
export function getSketchDragActive() { return sketchDragActive; }

/**
 * Begin dragging a sketch point. Adds a temporary WhereDragged constraint
 * and updates the point position, triggering a solve.
 * @param {number} pointId
 * @param {number} newX
 * @param {number} newY
 */
export function dragSketchPoint(pointId, newX, newY) {
	if (!dragState) {
		// First drag move — record original position, plus a full position +
		// camera snapshot so finalizeDrag can push a real undo record.
		const pos = sketchPositions.get(pointId);
		if (!pos) return;
		dragState = {
			pointId,
			originalX: pos.x,
			originalY: pos.y,
			positionsBefore: snapshotPositions(),
			camera: getCameraState()
		};
		sketchDragActive = true;
	}

	// Update position locally
	const nextPos = new Map(sketchPositions);
	nextPos.set(pointId, { x: newX, y: newY });
	sketchPositions = nextPos;

	// Remove any existing drag constraint for this point
	sketchConstraints = sketchConstraints.filter(c => !(c.type === 'WhereDragged' && c.point === pointId && c._isDrag));

	// Add temporary WhereDragged constraint
	sketchConstraints = [...sketchConstraints, {
		type: 'WhereDragged', point: pointId, x: newX, y: newY, _isDrag: true
	}];

	triggerSolve();
}

/**
 * Finalize a drag operation. Removes the temporary WhereDragged constraint
 * and pushes an undo action for the position change.
 */
export function finalizeDrag() {
	if (!dragState) return;

	// Remove all temporary drag constraints (single-point or multi-point line drag)
	sketchConstraints = sketchConstraints.filter(c => !c._isDrag);

	// Trigger final solve without the drag constraint
	triggerSolve();
	reExtractProfiles();

	// Push an undo record for the reposition (positions-only action: no
	// entities/constraints to remove, just geometry + camera to restore).
	if (dragState.positionsBefore) {
		const moved = [...dragState.positionsBefore].some(([id, p]) => {
			const cur = sketchPositions.get(id);
			return cur && (Math.abs(cur.x - p.x) > 1e-12 || Math.abs(cur.y - p.y) > 1e-12);
		});
		if (moved) {
			sketchUndoStack = [...sketchUndoStack, {
				entities: [],
				constraints: [],
				positionsBefore: dragState.positionsBefore,
				positionsAfter: snapshotPositions(),
				camera: dragState.camera
			}];
			sketchRedoStack = [];
		}
	}

	dragState = null;
	sketchDragActive = false;
}

/**
 * Drag a whole line by translating both endpoints by (dx, dy) from where the
 * drag started. Adds temporary WhereDragged constraints on both endpoints and
 * re-solves, so any geometry pinned to those endpoints (e.g. a square whose
 * side midpoints are Midpoint-constrained to this line) follows along.
 * @param {number} lineId
 * @param {number} dx - total X offset from drag start (sketch units)
 * @param {number} dy - total Y offset from drag start (sketch units)
 */
export function dragSketchLine(lineId, dx, dy) {
	const line = sketchEntities.find(e => e.id === lineId && e.type === 'Line');
	if (!line) return;

	if (!dragState || dragState.lineId !== lineId) {
		const sp = sketchPositions.get(line.start_id);
		const ep = sketchPositions.get(line.end_id);
		if (!sp || !ep) return;
		dragState = {
			lineId,
			points: [
				{ pointId: line.start_id, originalX: sp.x, originalY: sp.y },
				{ pointId: line.end_id, originalX: ep.x, originalY: ep.y },
			],
			positionsBefore: snapshotPositions(),
			camera: getCameraState(),
		};
		sketchDragActive = true;
	}

	const nextPos = new Map(sketchPositions);
	// Drop prior drag constraints, then re-pin every dragged endpoint.
	let nextConstraints = sketchConstraints.filter(c => !c._isDrag);
	for (const p of dragState.points) {
		const nx = p.originalX + dx;
		const ny = p.originalY + dy;
		nextPos.set(p.pointId, { x: nx, y: ny });
		nextConstraints = [...nextConstraints, {
			type: 'WhereDragged', point: p.pointId, x: nx, y: ny, _isDrag: true
		}];
	}
	sketchPositions = nextPos;
	sketchConstraints = nextConstraints;

	triggerSolve();
}

/** @returns {{ pointId: number, originalX: number, originalY: number } | null} */
export function getDragState() { return dragState; }

/** @type {Set<number>} Indices of failed/conflicting constraints from solver */
let failedConstraintIndices = $state(new Set());

/**
 * Detect over-constrained entities by checking constraint count vs DOF
 * and incorporating solver feedback (failed constraint indices).
 */
function recomputeOverConstrained() {
	// Count constraints applied to each entity
	/** @type {Map<number, number>} entity ID -> constraint count */
	const constraintCount = new Map();

	for (const c of sketchConstraints) {
		// entity-level constraints (H, V)
		if (c.entity != null) {
			constraintCount.set(c.entity, (constraintCount.get(c.entity) || 0) + 1);
		}
		// Point-pair constraints (coincident, distance, etc.)
		if (c.point_a != null) constraintCount.set(c.point_a, (constraintCount.get(c.point_a) || 0) + 1);
		if (c.point_b != null) constraintCount.set(c.point_b, (constraintCount.get(c.point_b) || 0) + 1);
		if (c.entity_a != null) constraintCount.set(c.entity_a, (constraintCount.get(c.entity_a) || 0) + 1);
		if (c.entity_b != null) constraintCount.set(c.entity_b, (constraintCount.get(c.entity_b) || 0) + 1);
	}

	const overconstrained = new Set();

	// Heuristic: flag entities with too many constraints
	for (const entity of sketchEntities) {
		const count = constraintCount.get(entity.id) || 0;
		if (entity.type === 'Line' && count > 2) {
			overconstrained.add(entity.id);
		}
		if (entity.type === 'Point' && count > 2) {
			overconstrained.add(entity.id);
		}
	}

	// Incorporate solver failed constraints — flag entities they reference
	if (sketchSolveStatus?.failed?.length > 0) {
		for (const failedIdx of sketchSolveStatus.failed) {
			if (failedIdx >= 0 && failedIdx < sketchConstraints.length) {
				const c = sketchConstraints[failedIdx];
				const refs = [c.entity, c.entity_a, c.entity_b, c.line, c.curve,
					c.line_a, c.line_b, c.point, c.point_a, c.point_b].filter(v => v != null);
				for (const id of refs) {
					overconstrained.add(id);
				}
			}
		}
		failedConstraintIndices = new Set(sketchSolveStatus.failed);
	} else {
		failedConstraintIndices = new Set();
	}

	overConstrainedEntities = overconstrained;
}

export function getFailedConstraintIndices() { return failedConstraintIndices; }

/**
 * Re-extract profiles from current sketch entities.
 */
function reExtractProfiles() {
	extractedProfilesState = extractProfiles(sketchEntities, sketchPositions);
	// Invalidate selections if profile list changed
	if (selectedProfileIndex != null && selectedProfileIndex >= extractedProfilesState.length) {
		selectedProfileIndex = null;
	}
	if (hoveredProfileIndex != null && hoveredProfileIndex >= extractedProfilesState.length) {
		hoveredProfileIndex = null;
	}
}

/**
 * Reset all sketch state. Called when entering/exiting sketch mode.
 */
export function resetSketchState() {
	sketchEntities = [];
	sketchConstraints = [];
	projectedBindings = [];
	sketchPositions = new Map();
	nextEntityId = 1;
	sketchSolveStatus = null;
	sketchSelection = new Set();
	sketchHover = null;
	extractedProfilesState = [];
	selectedProfileIndex = null;
	hoveredProfileIndex = null;
	sketchCursorPos = null;
	overConstrainedEntities = new Set();
	sketchUndoStack = [];
	sketchRedoStack = [];
	pendingSketchAction = null;
	referenceSnapPoints = [];
	inferenceSources = [];
	// Gears belong to a specific sketch; clear and rebuild per sketch (see
	// rebuildGearsFromEntities, called on sketch-edit load).
	gearRegistry = new Map();
	entityToGearMap = new Map();
	gearDisplay = new Map();
	nextGearId = 1;
}

/**
 * Rebuild the per-session gear bookkeeping (registry, entity→gear map, display
 * expansion) from the compact `Gear` entities in the current sketch. Called
 * after a sketch is loaded for editing, so gears persisted across save/reload
 * are rendered and re-editable as gears.
 */
async function rebuildGearsFromEntities() {
	const gearEntities = sketchEntities.filter(e => e.type === 'Gear' || e.type === 'Sprocket');
	for (const ge of gearEntities) {
		const gearId = nextGearId++;
		await expandGearForDisplay(gearId, ge.params, ge.type);
		const nextReg = new Map(gearRegistry);
		nextReg.set(gearId, { ...ge.params, entityId: ge.id, kind: ge.type });
		gearRegistry = nextReg;
		const nextMap = new Map(entityToGearMap);
		nextMap.set(ge.id, gearId);
		entityToGearMap = nextMap;
	}
}

// Sketch state getters/setters

export function getSketchEntities() { return sketchEntities; }
export function getSketchConstraints() { return sketchConstraints; }
export function getSketchPositions() { return sketchPositions; }
export function getSketchSolveStatus() { return sketchSolveStatus; }

export function getSketchSelection() { return sketchSelection; }
/** @param {Set<number>} sel */
export function setSketchSelection(sel) { sketchSelection = sel; }

export function getSketchHover() { return sketchHover; }
/** @param {number | null} id */
export function setSketchHover(id) { sketchHover = id; }

// -- Constraint badge selection + display offsets --

export function getSelectedConstraintIndex() { return selectedConstraintIndex; }
/** @param {number | null} idx */
export function setSelectedConstraintIndex(idx) {
	selectedConstraintIndex = idx;
	if (idx != null) {
		// Selecting a constraint clears entity/profile selection (mutually exclusive).
		sketchSelection = new Set();
		selectedProfileIndex = null;
	}
}

export function getSketchPixelSize() { return sketchPixelSize; }
/** @param {number} v */
export function setSketchPixelSize(v) { if (v > 0 && v !== sketchPixelSize) sketchPixelSize = v; }

/** Compute current geometric constraint badge positions (sketch coords). */
export function getConstraintBadges() {
	return computeConstraintBadges(
		sketchConstraints, sketchEntities, sketchPositions,
		failedConstraintIndices, constraintBadgeOffsets, sketchPixelSize
	);
}

/** @returns {Map<string, {dx:number, dy:number}>} */
export function getConstraintBadgeOffsets() { return constraintBadgeOffsets; }
/** @param {string} key @param {number} dx @param {number} dy */
export function setConstraintBadgeOffset(key, dx, dy) {
	const next = new Map(constraintBadgeOffsets);
	next.set(key, { dx, dy });
	constraintBadgeOffsets = next;
}

/** @returns {Map<string, {dx:number, dy:number}>} Per-dimension-label drag offsets. */
export function getDimensionLabelOffsets() { return dimensionLabelOffsets; }
/** @param {string} key @param {number} dx @param {number} dy */
export function setDimensionLabelOffset(key, dx, dy) {
	const next = new Map(dimensionLabelOffsets);
	next.set(key, { dx, dy });
	dimensionLabelOffsets = next;
}

/**
 * Delete the currently selected constraint badge, if any.
 * @returns {boolean} whether a constraint was deleted
 */
export function deleteSelectedConstraint() {
	if (selectedConstraintIndex == null) return false;
	const idx = selectedConstraintIndex;
	selectedConstraintIndex = null;
	if (idx >= 0 && idx < sketchConstraints.length) {
		removeSketchConstraint(idx);
		return true;
	}
	return false;
}

export function getExtractedProfiles() { return extractedProfilesState; }
export function getSelectedProfileIndex() { return selectedProfileIndex; }
/** @param {number | null} idx */
export function setSelectedProfileIndex(idx) { selectedProfileIndex = idx; }
export function getHoveredProfileIndex() { return hoveredProfileIndex; }
/** @param {number | null} idx */
export function setHoveredProfileIndex(idx) { hoveredProfileIndex = idx; }
export function getInactiveHoveredProfile() { return inactiveHoveredProfile; }
/** @param {{ featureId: string, profileIndex: number } | null} val */
export function setInactiveHoveredProfile(val) { inactiveHoveredProfile = val; }

export function getReferenceSnapPoints() { return referenceSnapPoints; }
/** @param {Array<{ x: number, y: number, sourceId: string, worldPos?: [number, number, number] }>} pts */
export function setReferenceSnapPoints(pts) { referenceSnapPoints = pts; }
export function clearReferenceSnapPoints() { referenceSnapPoints = []; }

export function getOverConstrainedEntities() { return overConstrainedEntities; }

/**
 * Get entity IDs of under-constrained points (points not referenced by any constraint).
 * @returns {Set<number>}
 */
export function getUnderConstrainedEntities() {
	const solveStatus = sketchSolveStatus;
	if (!solveStatus || solveStatus.dof === 0) return new Set();

	const constrainedIds = new Set();
	for (const c of sketchConstraints) {
		if (c._isDrag) continue;
		for (const key of ['point', 'point_a', 'point_b', 'entity_a', 'entity_b', 'entity']) {
			if (c[key] != null) {
				const ent = sketchEntities.find(e => e.id === c[key]);
				if (ent && ent.type === 'Point') constrainedIds.add(c[key]);
				if (ent && ent.type === 'Line') {
					constrainedIds.add(ent.start_id);
					constrainedIds.add(ent.end_id);
				}
				if (ent && (ent.type === 'Circle' || ent.type === 'Arc')) {
					constrainedIds.add(ent.center_id);
					if (ent.start_id) constrainedIds.add(ent.start_id);
					if (ent.end_id) constrainedIds.add(ent.end_id);
				}
			}
		}
	}

	const unconstrained = new Set();
	for (const e of sketchEntities) {
		if (e.type === 'Point' && !constrainedIds.has(e.id)) {
			unconstrained.add(e.id);
		}
	}
	return unconstrained;
}

export function getSketchCursorPos() { return sketchCursorPos; }
/** @param {{ x: number, y: number } | null} pos */
export function setSketchCursorPos(pos) { sketchCursorPos = pos; }

// -- Extrude dialog --

export function getExtrudeDialogState() { return extrudeDialogState; }

export function getExtrudePreviewParams() { return extrudePreviewParams; }
export function setExtrudePreviewParams(params) { extrudePreviewParams = params; }

export function getProfilePickMode() { return profilePickMode; }
export function setProfilePickMode(mode) {
	profilePickMode = mode;
	// Entering a profile/region pick: compute the minimal sketch regions in
	// Rust so the renderer can hit-test the smallest region under the click
	// (including sub-regions of overlapping shapes). Fire-and-forget; the
	// renderer falls back to whole-loop profiles until the regions arrive.
	if (mode) computeAllSketchRegions();
}

// -- Extrude target-body picking (shared dialog ↔ viewport) --
export function getExtrudeTargetPick() {
	return { active: extrudeTargetPickActive, ids: extrudeTargetIds };
}
export function setExtrudeTargetPickActive(v) {
	extrudeTargetPickActive = !!v;
}
export function setExtrudeTargetIds(ids) {
	extrudeTargetIds = Array.isArray(ids) ? [...ids] : [];
}
export function toggleExtrudeTargetId(bodyId) {
	if (!bodyId) return;
	extrudeTargetIds = extrudeTargetIds.includes(bodyId)
		? extrudeTargetIds.filter((id) => id !== bodyId)
		: [...extrudeTargetIds, bodyId];
}
export function clearExtrudeTargets() {
	extrudeTargetIds = [];
	extrudeTargetPickActive = false;
}

/**
 * Minimal sketch faces (regions) per sketch feature id, computed in Rust via
 * the ComputeRegions query. Each region is annotated with its `_index`.
 * @type {Map<string, Array<object>>}
 */
let sketchRegions = $state(new Map());

/**
 * ComputeRegions inputs for one completed sketch feature: gear entities replaced
 * by their primitive expansion (`gears`, keyed `${featureId}:${entityId}`) and
 * point positions from the solver. Shared by the region cache and the agent link.
 * @param {any} feature
 * @param {Map<string, any>} gears
 */
function regionInputs(feature, gears) {
	const sketch = feature.operation.sketch;
	// Solver output is the authoritative coordinate source (A2.1/A5.2): the
	// engine computes geometry truth, the UI must derive from it. Raw drawn
	// `e.x/e.y` is pre-solve scratch — feeding it to the arrangement produces
	// geometrically wrong regions (e.g. constraint-centered nested squares
	// appear off-center/wrong-size). Fall back to raw only when a point has
	// no solved entry yet (freshly drawn, pre-solve). Gear-expanded points
	// are deterministic from gear params and carry their own final coords.
	const solved = sketch.solved_positions || {};
	const entities = [];
	const solved_positions = {};
	for (const e of (sketch.entities || [])) {
		if (e.type === 'Gear' || e.type === 'Sprocket') {
			// Substitute the generator's cached primitive expansion (teeth + points).
			const exp = gears.get(`${feature.id}:${e.id}`);
			if (exp) {
				for (const ge of exp.entities) {
					entities.push(ge);
					if (ge.type === 'Point' && ge.id != null) solved_positions[ge.id] = [ge.x, ge.y];
				}
			}
		} else {
			entities.push(e);
			if (e.type === 'Point' && e.id != null) {
				const sp = solved[e.id];
				solved_positions[e.id] = sp ? [sp[0], sp[1]] : [e.x, e.y];
			}
		}
	}
	return { entities, solved_positions };
}

/** @param {string} featureId @returns {Array<object> | null} */
export function getSketchRegions(featureId) {
	return sketchRegions.get(featureId) ?? null;
}

/**
 * Compute regions for every completed sketch in the feature tree and cache
 * them in `sketchRegions`. Idempotent enough for repeated pick-mode entry.
 */
export async function computeAllSketchRegions() {
	if (!bridge || !engineReady) return;
	const tree = featureTree;
	if (!tree?.features) return;

	// Expand gears to their primitive entities first, so gear sketches (e.g. a
	// ring gear with a bore) get real minimal regions instead of falling back to
	// the whole-entity profile (which would mis-shade the hover).
	const gearSpecs = [];
	for (const f of tree.features) {
		if (f.operation?.type !== 'Sketch') continue;
		for (const e of (f.operation.sketch.entities || [])) {
			if (e.type === 'Gear' || e.type === 'Sprocket') {
				gearSpecs.push({ key: `${f.id}:${e.id}`, entityId: e.id, params: e.params, kind: e.type });
			}
		}
	}
	if (gearSpecs.length) await ensureInactiveGearsExpanded(gearSpecs);
	const gears = getInactiveGearDisplay();

	const next = new Map();
	for (const feature of tree.features) {
		if (feature.operation?.type !== 'Sketch') continue;
		const { entities, solved_positions } = regionInputs(feature, gears);
		try {
			const response = await bridge.send({
				type: 'ComputeRegions',
				entities: JSON.parse(JSON.stringify(entities)),
				solved_positions
			});
			const regions = (response?.regions ?? []).map((r, i) => ({ ...r, _index: i }));
			next.set(feature.id, regions);
		} catch (err) {
			console.error('ComputeRegions failed:', err);
		}
	}
	sketchRegions = next;
}

export function getAxisPickMode() { return axisPickMode; }
export function setAxisPickMode(active) { axisPickMode = active; }

export function getPathPickMode() { return pathPickMode; }
export function setPathPickMode(active) { pathPickMode = active; }

export function getRevolvePreviewParams() { return revolvePreviewParams; }
export function setRevolvePreviewParams(params) { revolvePreviewParams = params; }

/**
 * Show the extrude dialog. Auto-selects the last sketch in the feature tree.
 * Pre-populates regions from selectedProfileIndex or auto-selects single-profile sketches.
 */
export function showExtrudeDialog() {
	const tree = featureTree;
	if (!tree || !tree.features) return;

	// Collect ALL sketch features for the sketch selector
	const allSketches = tree.features
		.filter(f => f.operation?.type === 'Sketch')
		.map(f => ({
			id: f.id,
			name: f.name,
			profileCount: f.operation?.sketch?.solved_profiles?.length ?? 0
		}));

	// Find the last sketch feature
	let lastSketch = null;
	for (let i = tree.features.length - 1; i >= 0; i--) {
		const f = tree.features[i];
		if (f.operation?.type === 'Sketch') {
			lastSketch = f;
			break;
		}
	}

	if (!lastSketch) return;

	const profileCount = lastSketch.operation?.sketch?.solved_profiles?.length ?? 0;

	// Pre-populate regions
	/** @type {Array<{ sketchId: string, sketchName: string, profileIndex: number }>} */
	let regions = [];

	if (selectedProfileIndex != null && sketchMode.active) {
		// If a profile is already selected in sketch mode, use it
		regions = [{ type: 'sketchProfile', sketchId: lastSketch.id, sketchName: lastSketch.name, profileIndex: selectedProfileIndex }];
	}

	const autoSelect = regions.length === 0 && getSetting('extrudeAutoSelectRegion');
	log('ui', 'Show extrude dialog', { sketchId: lastSketch.id, profileCount, regionCount: regions.length, autoSelect });
	extrudeDialogState = {
		sketchId: lastSketch.id,
		sketchName: lastSketch.name,
		profileCount,
		availableSketches: allSketches,
		regions
	};
	if (regions.length === 0) {
		if (autoSelect) {
			// Auto-select resolves against the engine's region arrangement so what
			// the list shows is exactly what Apply extrudes (a bare "profile 0"
			// placeholder used to be shown, then rejected at Apply on any sketch
			// with several regions).
			autoSelectExtrudeRegion(lastSketch.id, lastSketch.name)
				.catch((err) => log('error', `Extrude auto-select failed: ${err}`));
		} else {
			// Nothing pre-selected: arm region picking so the next click on a
			// sketch region adds it.
			setProfilePickMode({ target: 'extrude' });
		}
	}
}

/**
 * Pick ONE region of `sketchId` for a freshly opened extrude dialog: the only
 * region when there is one; otherwise the region whose outer boundary is the
 * sketch's first whole-loop profile (a holed rectangle → the rectangle minus
 * its holes); otherwise the largest region. Falls back to the legacy whole
 * profile 0 when the arrangement yields nothing (e.g. an open sketch).
 * No-op if the dialog closed, switched sketch, or gained a region meanwhile.
 * @param {string} sketchId
 * @param {string} sketchName
 */
async function autoSelectExtrudeRegion(sketchId, sketchName) {
	let avail = getSketchRegions(sketchId);
	if (avail == null) {
		await computeAllSketchRegions();
		avail = getSketchRegions(sketchId);
	}
	if (!extrudeDialogState || extrudeDialogState.sketchId !== sketchId) return;
	if ((extrudeDialogState.regions?.length ?? 0) > 0) return;

	const sketch = featureTree?.features?.find((f) => f.id === sketchId)?.operation?.sketch;
	if (!avail || avail.length === 0) {
		if ((sketch?.solved_profiles?.length ?? 0) > 0) addExtrudeRegion(sketchId, sketchName, 0, null);
		return;
	}
	let pick = null;
	if (avail.length === 1) {
		pick = avail[0];
	} else {
		const profPoly = profileOuterPolygon(sketchId, 0);
		pick = profPoly ? avail.find((r) => outerPolygonMatches(r.outer, profPoly)) ?? null : null;
		if (!pick) pick = avail.reduce((a, b) => ((b.area ?? 0) > (a.area ?? 0) ? b : a));
	}
	// A whole-loop region rides the analytical profile_index path; map its
	// entity ids to the matching solved profile (0 for genuine sub-regions).
	let profileIndex = 0;
	const ids = pick.profile_entity_ids;
	if (ids && ids.length && Array.isArray(sketch?.solved_profiles)) {
		const want = [...ids].sort((a, b) => a - b).join(',');
		const i = sketch.solved_profiles.findIndex((p) => [...(p.entity_ids ?? [])].sort((a, b) => a - b).join(',') === want);
		if (i >= 0) profileIndex = i;
	}
	addExtrudeRegion(sketchId, sketchName, profileIndex, JSON.parse(JSON.stringify(pick)));
	log('ui', 'Extrude auto-selected region', { sketchId, profileIndex, subRegion: pick.profile_entity_ids == null, candidates: avail.length });
}

/**
 * Change the selected sketch in the extrude dialog.
 * @param {string} sketchId
 */
export function changeExtrudeSketch(sketchId) {
	if (!extrudeDialogState) return;
	const sketch = extrudeDialogState.availableSketches?.find(s => s.id === sketchId);
	if (!sketch) return;

	extrudeDialogState = {
		...extrudeDialogState,
		sketchId: sketch.id,
		sketchName: sketch.name,
		profileCount: sketch.profileCount,
		regions: [{ type: 'sketchProfile', sketchId: sketch.id, sketchName: sketch.name, profileIndex: 0 }]
	};
}

/**
 * Add a region to the extrude dialog's region list.
 * @param {string} sketchId
 * @param {string} sketchName
 * @param {number} profileIndex
 */
export function addExtrudeRegion(sketchId, sketchName, profileIndex, region = null) {
	if (!extrudeDialogState) return;
	// A genuine sub-region (annulus, lens, …) is identified by its geometry,
	// not a profile_index. Use a geometry key to dedup those; whole-loop
	// selections still dedup by (sketchId, profileIndex).
	const subRegion = region && region.profile_entity_ids == null;
	const key = subRegion ? regionKey(region) : null;
	const exists = extrudeDialogState.regions.some(r =>
		r.sketchId === sketchId &&
		(subRegion ? r.regionKey === key : (r.region == null && r.profileIndex === profileIndex))
	);
	if (exists) return;
	extrudeDialogState = {
		...extrudeDialogState,
		regions: [
			...extrudeDialogState.regions,
			{ type: 'sketchProfile', sketchId, sketchName, profileIndex, region, regionKey: key }
		]
	};
}

/**
 * Stable identity for a sub-region. Uses the area-weighted CENTROID (not the
 * first outer vertex): congruent sub-regions meeting at a shared point — e.g.
 * the four triangles of an X-in-square all touching the origin — can share a
 * first vertex AND area, so a first-vertex key collides and silently drops the
 * later click. The centroid is order-independent and distinct per region.
 */
function regionKey(region) {
	const outer = region.outer ?? [];
	const c = outer.length >= 3 ? polyCentroid(outer) : (outer[0] ?? [0, 0]);
	const holes = region.holes?.length ?? 0;
	return `${c[0].toFixed(6)},${c[1].toFixed(6)}:${(region.area ?? 0).toFixed(6)}:${outer.length}:${holes}`;
}

/**
 * Remove a region from the extrude dialog's region list by index.
 * @param {number} index
 */
export function removeExtrudeRegion(index) {
	if (!extrudeDialogState) return;
	const regions = [...extrudeDialogState.regions];
	regions.splice(index, 1);
	extrudeDialogState = { ...extrudeDialogState, regions };
}

export function clearExtrudeRegions() {
	if (!extrudeDialogState) return;
	extrudeDialogState = { ...extrudeDialogState, regions: [] };
}

/**
 * Get the current extrude regions list.
 * @returns {Array<{ sketchId: string, sketchName: string, profileIndex: number }>}
 */
export function getExtrudeRegions() {
	return extrudeDialogState?.regions ?? [];
}

/**
 * Add a face-based region to the extrude dialog from a viewport click.
 * @param {any} ref - GeomRef clicked in viewport
 */
export function addExtrudeRegionFromRef(ref) {
	if (!extrudeDialogState) return;

	const region = {
		type: 'face',
		geomRef: JSON.parse(JSON.stringify(ref)),
		label: describeFaceRef(ref),
	};

	// Deduplicate
	const isDupe = extrudeDialogState.regions.some(r =>
		r.type === 'face' && geomRefEquals(r.geomRef, ref)
	);
	if (isDupe) return;

	extrudeDialogState = {
		...extrudeDialogState,
		regions: [...extrudeDialogState.regions, region],
	};
}

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

export function hideExtrudeDialog() {
	extrudeDialogState = null;
	extrudePreviewParams = null;
	profilePickMode = null;
	restoreEditRollback();
}

/**
 * Show the extrude dialog pre-populated for editing an existing feature.
 * @param {string} featureId
 */
export function showExtrudeDialogForEdit(featureId) {
	const tree = featureTree;
	if (!tree || !tree.features) return;

	const feature = tree.features.find(f => f.id === featureId);
	if (!feature || feature.operation?.type !== 'Extrude') return;

	const params = feature.operation.params;
	const sketchId = params.sketch_id;
	const sketch = tree.features.find(f => f.id === sketchId);
	if (!sketch) return;

	const profileCount = sketch.operation?.sketch?.solved_profiles?.length ?? 0;
	const allSketches = tree.features
		.filter(f => f.operation?.type === 'Sketch')
		.map(f => ({ id: f.id, name: f.name, profileCount: f.operation?.sketch?.solved_profiles?.length ?? 0 }));

	const regions = [{ type: 'sketchProfile', sketchId, sketchName: sketch.name, profileIndex: params.profile_index ?? 0 }];

	log('ui', 'Show extrude dialog for edit', { featureId, sketchId });
	extrudeDialogState = {
		sketchId,
		sketchName: sketch.name,
		profileCount,
		availableSketches: allSketches,
		regions,
		editingFeatureId: featureId,
		editParams: params
	};
	beginEditRollback(featureId);
}

/** Absolute area of a closed polygon `[[x,y], …]` (shoelace). */
function polyAreaAbs(poly) {
	let a = 0;
	for (let i = 0; i < poly.length; i++) {
		const [x1, y1] = poly[i];
		const [x2, y2] = poly[(i + 1) % poly.length];
		a += x1 * y2 - x2 * y1;
	}
	return Math.abs(a / 2);
}

/** Area-weighted centroid of a closed polygon (vertex-average fallback). */
function polyCentroid(poly) {
	let a = 0, cx = 0, cy = 0;
	for (let i = 0; i < poly.length; i++) {
		const [x1, y1] = poly[i];
		const [x2, y2] = poly[(i + 1) % poly.length];
		const cross = x1 * y2 - x2 * y1;
		a += cross;
		cx += (x1 + x2) * cross;
		cy += (y1 + y2) * cross;
	}
	if (Math.abs(a) < 1e-14) {
		const n = poly.length || 1;
		return [poly.reduce((s, p) => s + p[0], 0) / n, poly.reduce((s, p) => s + p[1], 0) / n];
	}
	return [cx / (3 * a), cy / (3 * a)];
}

/**
 * The outer-boundary polygon of `solved_profiles[profileIndex]` for a sketch,
 * built from the authoritative `solved_positions`. Null if it can't be formed.
 * @param {string} sketchId @param {number} profileIndex
 */
function profileOuterPolygon(sketchId, profileIndex) {
	const sketch = featureTree?.features?.find((f) => f.id === sketchId)?.operation?.sketch;
	const prof = sketch?.solved_profiles?.[profileIndex];
	const pos = sketch?.solved_positions || {};
	if (!prof || !Array.isArray(prof.vertex_ids) || prof.vertex_ids.length < 3) return null;
	const poly = [];
	for (const vid of prof.vertex_ids) {
		const p = pos[vid];
		if (!p) return null;
		poly.push([p[0], p[1]]);
	}
	return poly;
}

/** Whether a region outer boundary equals a profile polygon (area + centroid). */
function outerPolygonMatches(outer, prof) {
	if (!Array.isArray(outer) || outer.length < 3) return false;
	const ao = polyAreaAbs(outer), ap = polyAreaAbs(prof);
	const m = Math.max(ao, ap, 1e-12);
	if (Math.abs(ao - ap) / m > 1e-2) return false;
	const co = polyCentroid(outer), cp = polyCentroid(prof);
	const scale = Math.sqrt(m);
	return Math.hypot(co[0] - cp[0], co[1] - cp[1]) < 1e-2 * Math.max(scale, 1e-9);
}

/**
 * Apply an extrude operation from the dialog.
 * @param {number} depth
 * @param {number} profileIndex - Legacy param, overridden by regions[0] if available
 * @param {boolean} [cut=false] - If true, perform a cut (subtract) operation
 */
export async function applyExtrude(depth, profileIndex, cut = false, opts = {}) {
	if (!extrudeDialogState || !bridge || !engineReady) return;

	// Use regions[0] if available, fall back to legacy profileIndex param
	const regions = extrudeDialogState.regions ?? [];
	const region = regions[0];
	const effectiveSketchId = region?.sketchId ?? extrudeDialogState.sketchId;
	const effectiveProfileIndex = region?.profileIndex ?? profileIndex;

	// A genuine sub-region (no whole-loop profile denotes it) is extruded from
	// its explicit boundary; whole-loop selections leave this null and use
	// profile_index (the analytical path). Send the whole region (outer/holes +
	// recovered arc edges) so the engine builds true curved walls. Deep-clone to
	// strip Svelte 5 $state proxies (postMessage can't clone them).
	let subRegion =
		region?.region && region.region.profile_entity_ids == null
			? JSON.parse(JSON.stringify(region.region))
			: null;

	// Default-selection guard: when no explicit region was clicked (the dialog's
	// bare `{ profileIndex }` default), resolve it against the engine's
	// authoritative arrangement (`compute_regions`) rather than the legacy
	// whole-profile path. That path cannot represent a self-intersecting sketch —
	// `extract_profiles` yields overlapping/pinched loops the kernel rejects
	// (ProfileRepeatedVertex / ProfileLoopsIntersect). A sketch with sub-regions
	// whose outer profile equals exactly one region (a plain or holed rectangle)
	// extrudes that region; one with NO single region equal to its outer profile
	// (an X-in-square) requires an explicit pick.
	if (!region?.region) {
		let avail = getSketchRegions(effectiveSketchId);
		if (avail == null) {
			await computeAllSketchRegions();
			avail = getSketchRegions(effectiveSketchId);
		}
		if (avail && avail.length > 1) {
			const profPoly = profileOuterPolygon(effectiveSketchId, effectiveProfileIndex);
			const match = profPoly ? avail.find((r) => outerPolygonMatches(r.outer, profPoly)) : null;
			if (!match) {
				showToast('error', 'This sketch has multiple regions — click a region to extrude.');
				return;
			}
			// A holed/genuine sub-region extrudes from its explicit boundary; a
			// clean whole-loop region keeps the analytical profile_index path.
			if ((match.holes?.length ?? 0) > 0 || match.profile_entity_ids == null) {
				subRegion = JSON.parse(JSON.stringify(match));
			}
		}
	}

	// Multi-region selection: every selected region (on the same sketch) that
	// carries an explicit boundary is sent so the engine unions their 2D
	// footprints into one body. ≥2 triggers the union path; adjacent regions
	// with shared/coplanar walls then merge cleanly (no 3D coplanar boolean).
	// Deep-clone to strip Svelte 5 $state proxies (postMessage can't clone them).
	const sameSketchRegions = regions.filter(
		(r) => (r.sketchId ?? extrudeDialogState.sketchId) === effectiveSketchId && r.region
	);
	const multiRegions =
		sameSketchRegions.length >= 2
			? JSON.parse(JSON.stringify(sameSketchRegions.map((r) => r.region)))
			: [];

	const {
		depthMode = 'Blind',
		secondDir = 'None',
		secondDepth = 10,
		flipDirection = false,
		// Optional driving expression for the depth (mm-space; see the
		// engine's design-parameter docs). null = plain numeric depth.
		depthExpr = null,
		// New-style optional-boolean combine (N-mb-*). `combine` is one of
		// 'NewBody' | 'Add' | 'Cut' | 'Intersect' (or null = legacy). `targets` is
		// an array of body GeomRefs, [] to force a new body, or null = Auto
		// (share-a-face). See specs/optional_booleans_multibody_extrude.md.
		combine = null,
		targets = null
	} = opts;

	const combineObj = combine ? { type: combine } : null;
	// When a combine mode is set the engine ignores cut/merge; keep the legacy
	// `cut` consistent (drives direction reversal + the ghost preview).
	const effectiveCut = combineObj ? combine === 'Cut' : !!cut;

	const depth_mode = { type: depthMode };

	let second_direction = null;
	if (secondDir === 'Symmetric') second_direction = { type: 'Symmetric' };
	else if (secondDir === 'Blind') second_direction = { type: 'Blind', depth: secondDepth };
	else if (secondDir === 'ThroughAll') second_direction = { type: 'ThroughAll' };

	// When flipDirection is true, send an explicit direction to override the engine default.
	// For a boss, flipping means opposite of normal: send -normal.
	// For a cut, the engine's default already reverses (cuts -normal into body),
	// so flipping a cut means the opposite of that: send +normal.
	let direction = null;
	if (flipDirection) {
		const tree = featureTree;
		const sketch = tree?.features?.find(f => f.id === effectiveSketchId);
		const normal = sketch?.operation?.sketch?.plane_normal;
		if (normal) {
			if (effectiveCut) {
				direction = [normal[0], normal[1], normal[2]];
			} else {
				direction = [-normal[0], -normal[1], -normal[2]];
			}
		} else {
			direction = effectiveCut ? [0, 0, 1] : [0, 0, -1];
		}
	}

	const operation = {
		type: 'Extrude',
		params: {
			sketch_id: effectiveSketchId,
			profile_index: effectiveProfileIndex,
			depth,
			depth_expr: depthExpr,
			direction,
			symmetric: secondDir === 'Symmetric',
			cut: effectiveCut,
			target_body: null,
			combine: combineObj,
			targets,
			depth_mode,
			second_direction,
			region: subRegion,
			regions: multiRegions
		}
	};

	const editingId = extrudeDialogState.editingFeatureId;
	log('action', editingId ? 'Edit extrude' : 'Apply extrude', { depth, profileIndex: effectiveProfileIndex, cut: !!cut, depthMode, secondDir, flipDirection });
	try {
		if (editingId) {
			await editFeature(editingId, operation);
		} else {
			await sendRebuild({ type: 'AddFeature', operation });
		}

		await restoreEditRollback();
		extrudeDialogState = null;
		extrudePreviewParams = null;
	} catch (err) {
		log('error', `Extrude failed: ${err.message}`);
		showToast('error', `Extrude failed: ${err.message}`);
	}
}

// -- Revolve dialog --

export function getRevolveDialogState() { return revolveDialogState; }

/**
 * Show the revolve dialog. Auto-selects the last sketch in the feature tree.
 */
export function showRevolveDialog() {
	const tree = featureTree;
	if (!tree || !tree.features) return;

	let lastSketch = null;
	for (let i = tree.features.length - 1; i >= 0; i--) {
		const f = tree.features[i];
		if (f.operation?.type === 'Sketch') {
			lastSketch = f;
			break;
		}
	}

	if (!lastSketch) return;

	const profileCount = lastSketch.operation?.sketch?.solved_profiles?.length ?? 0;
	const sketchData = lastSketch.operation?.sketch;

	log('ui', 'Show revolve dialog', { sketchId: lastSketch.id, profileCount });
	revolveDialogState = {
		sketchId: lastSketch.id,
		sketchName: lastSketch.name,
		profileCount,
		planeOrigin: sketchData?.plane_origin ?? [0, 0, 0],
		planeNormal: sketchData?.plane_normal ?? [0, 0, 1],
		selectedProfile: { sketchId: lastSketch.id, profileIndex: 0, label: `${lastSketch.name} / Profile 1` },
		selectedAxis: null
	};
}

export function hideRevolveDialog() {
	revolveDialogState = null;
	revolvePreviewParams = null;
	profilePickMode = null;
	axisPickMode = false;
	restoreEditRollback();
}

/**
 * Show the revolve dialog pre-populated for editing an existing feature.
 * @param {string} featureId
 */
export function showRevolveDialogForEdit(featureId) {
	const tree = featureTree;
	if (!tree || !tree.features) return;

	const feature = tree.features.find(f => f.id === featureId);
	if (!feature || feature.operation?.type !== 'Revolve') return;

	const params = feature.operation.params;
	const sketchId = params.sketch_id;
	const sketch = tree.features.find(f => f.id === sketchId);
	if (!sketch) return;

	const profileCount = sketch.operation?.sketch?.solved_profiles?.length ?? 0;
	const sketchData = sketch.operation?.sketch;

	log('ui', 'Show revolve dialog for edit', { featureId, sketchId });
	revolveDialogState = {
		sketchId,
		sketchName: sketch.name,
		profileCount,
		planeOrigin: sketchData?.plane_origin ?? [0, 0, 0],
		planeNormal: sketchData?.plane_normal ?? [0, 0, 1],
		selectedProfile: { sketchId, profileIndex: params.profile_index ?? 0, label: `${sketch.name} / Profile ${(params.profile_index ?? 0) + 1}` },
		selectedAxis: params.axis_origin && params.axis_direction
			? { origin: params.axis_origin, direction: params.axis_direction, label: 'Saved axis' }
			: null,
		editingFeatureId: featureId,
		editParams: params
	};
	beginEditRollback(featureId);
}

/**
 * Apply a revolve operation from the dialog.
 * @param {number} angleDeg - angle in degrees
 * @param {[number,number,number]} axisOrigin
 * @param {[number,number,number]} axisDir
 * @param {number} profileIndex
 */
export async function applyRevolve(angleDeg, axisOrigin, axisDir, profileIndex, opts = {}) {
	if (!revolveDialogState || !bridge || !engineReady) return;

	// Optional-boolean combine (mirrors extrude). `combine` is a mode string or
	// null (legacy); `targets` is an array of body GeomRefs, [] for a new body,
	// or null = Auto (share-a-face).
	const { combine = null, targets = null, angleExpr = null } = opts;
	const combineObj = combine ? { type: combine } : null;

	const operation = {
		type: 'Revolve',
		params: {
			sketch_id: revolveDialogState.sketchId,
			profile_index: profileIndex,
			axis_origin: axisOrigin,
			axis_direction: axisDir,
			angle: angleDeg,
			angle_expr: angleExpr,
			combine: combineObj,
			targets
		}
	};

	const editingId = revolveDialogState.editingFeatureId;
	log('action', editingId ? 'Edit revolve' : 'Apply revolve', { angle: angleDeg, profileIndex });

	try {
		if (editingId) {
			await editFeature(editingId, operation);
		} else {
			await sendRebuild({ type: 'AddFeature', operation });
		}

		await restoreEditRollback();
		revolveDialogState = null;
		revolvePreviewParams = null;
	} catch (err) {
		log('error', `Revolve failed: ${err.message}`);
		showToast('error', `Revolve failed: ${err.message}`);
	}
}

// -- Pipe dialog (spec `specs/b2_pipe_sweep.md` checkpoint 3) --

export function getPipeDialogState() { return pipeDialogState; }

/** Show the pipe dialog on the last sketch in the feature tree. */
export function showPipeDialog() {
	const tree = featureTree;
	if (!tree || !tree.features) return;
	let lastSketch = null;
	for (let i = tree.features.length - 1; i >= 0; i--) {
		const f = tree.features[i];
		if (f.operation?.type === 'Sketch') {
			lastSketch = f;
			break;
		}
	}
	if (!lastSketch) return;
	log('ui', 'Show pipe dialog', { sketchId: lastSketch.id });
	pipeDialogState = {
		sketchId: lastSketch.id,
		sketchName: lastSketch.name,
		entityIds: []
	};
}

export function hidePipeDialog() {
	pipeDialogState = null;
	pathPickMode = false;
	restoreEditRollback();
}

/**
 * Show the pipe dialog pre-populated for editing an existing feature.
 * @param {string} featureId
 */
export function showPipeDialogForEdit(featureId) {
	const tree = featureTree;
	if (!tree || !tree.features) return;
	const feature = tree.features.find(f => f.id === featureId);
	if (!feature || feature.operation?.type !== 'Pipe') return;
	const params = feature.operation.params;
	const sketch = tree.features.find(f => f.id === params.sketch_id);
	if (!sketch) return;
	log('ui', 'Show pipe dialog for edit', { featureId, sketchId: params.sketch_id });
	pipeDialogState = {
		sketchId: params.sketch_id,
		sketchName: sketch.name,
		entityIds: [...(params.entity_ids ?? [])],
		editingFeatureId: featureId,
		editParams: params
	};
	beginEditRollback(featureId);
}

/**
 * The connected chain of lines/arcs (by shared point ids) containing
 * `entityId` in the sketch feature `sketchId`. Construction entities count:
 * a sweep path is usually drawn as construction geometry.
 * @returns {number[]}
 */
function connectedPathChain(sketchId, entityId) {
	const feature = featureTree?.features?.find(f => f.id === sketchId);
	const entities = feature?.operation?.sketch?.entities ?? [];
	const segs = entities.filter(e => e.type === 'Line' || e.type === 'Arc');
	const byId = new Map(segs.map(e => [e.id, e]));
	if (!byId.has(entityId)) return [entityId];
	const incident = new Map();
	for (const e of segs) {
		for (const pid of [e.start_id, e.end_id]) {
			if (!incident.has(pid)) incident.set(pid, []);
			incident.get(pid).push(e.id);
		}
	}
	const seen = new Set([entityId]);
	const queue = [entityId];
	while (queue.length) {
		const id = queue.shift();
		const e = byId.get(id);
		for (const pid of [e.start_id, e.end_id]) {
			for (const other of incident.get(pid) ?? []) {
				if (!seen.has(other)) {
					seen.add(other);
					queue.push(other);
				}
			}
		}
	}
	return [...seen].sort((a, b) => a - b);
}

/**
 * Toggle a sketch entity in the pipe dialog's path. A click on an entity not
 * yet in the path adds its WHOLE connected chain (`expand`, default true);
 * a click on one already in the path removes just that entity.
 * @param {string} sketchId - sketch feature id
 * @param {number} entityId
 */
export function togglePipePathEntity(sketchId, entityId, opts = {}) {
	if (!pipeDialogState) return;
	const { expand = true } = opts;
	if (pipeDialogState.sketchId !== sketchId) {
		// The path lives on ONE sketch: switching sketches restarts the pick.
		const feature = featureTree?.features?.find(f => f.id === sketchId);
		pipeDialogState = {
			...pipeDialogState,
			sketchId,
			sketchName: feature?.name ?? pipeDialogState.sketchName,
			entityIds: []
		};
	}
	const current = pipeDialogState.entityIds;
	let next;
	if (current.includes(entityId)) {
		next = current.filter(id => id !== entityId);
	} else {
		const add = expand ? connectedPathChain(sketchId, entityId) : [entityId];
		next = [...current, ...add.filter(id => !current.includes(id))];
	}
	pipeDialogState = { ...pipeDialogState, entityIds: next };
}

/** Test/agent setup: replace the path with `ids` (each expanded like a click). */
export function setPipePath(sketchId, ids, opts = {}) {
	if (!pipeDialogState) return;
	const { expand = true } = opts;
	const feature = featureTree?.features?.find(f => f.id === sketchId);
	pipeDialogState = {
		...pipeDialogState,
		sketchId,
		sketchName: feature?.name ?? pipeDialogState.sketchName,
		entityIds: []
	};
	for (const id of ids) togglePipePathEntity(sketchId, id, { expand });
}

/**
 * Apply a pipe operation from the dialog.
 * @param {number} radius - tube radius, meters
 * @param {number|null} innerRadius - bore radius, meters (null = solid)
 */
export async function applyPipe(radius, innerRadius, opts = {}) {
	if (!pipeDialogState || !bridge || !engineReady) return;
	const { combine = null, targets = null, radiusExpr = null, innerRadiusExpr = null } = opts;
	const combineObj = combine ? { type: combine } : null;
	const operation = {
		type: 'Pipe',
		params: {
			sketch_id: pipeDialogState.sketchId,
			entity_ids: [...pipeDialogState.entityIds],
			radius,
			radius_expr: radiusExpr,
			inner_radius: innerRadius,
			inner_radius_expr: innerRadiusExpr,
			combine: combineObj,
			targets
		}
	};
	const editingId = pipeDialogState.editingFeatureId;
	log('action', editingId ? 'Edit pipe' : 'Apply pipe', { radius, innerRadius, entities: operation.params.entity_ids.length });
	try {
		if (editingId) {
			await editFeature(editingId, operation);
		} else {
			await sendRebuild({ type: 'AddFeature', operation });
		}
		await restoreEditRollback();
		pipeDialogState = null;
		pathPickMode = false;
	} catch (err) {
		log('error', `Pipe failed: ${err.message}`);
		showToast('error', `Pipe failed: ${err.message}`);
	}
}

// -- Custom feature scripts (specs/custom_features_and_modeling_roadmap.md
//    §A8, A-M4): the Script dialog (a node's arguments, generated from the
//    script's `@param` header) and the Script editor (the source text). The
//    engine owns the sources; the editor's Check / Save are engine messages
//    (`CheckScript`, `AddScriptSource`, `SetScriptSource`) so the page has
//    no second parser. --

/** The document's `Script` sources (from the mirrored `sources` table). */
export function getScriptSources() {
	return documentSources.filter((s) => s.kind === 'Script');
}

/** Built-in library scripts the engine ships (`AddScriptSource { library }`). */
export const SCRIPT_LIBRARY = [
	{ id: 'gear', label: 'Spur gear (gear.rhai)' },
	{ id: 'sprocket', label: 'Roller-chain sprocket (sprocket.rhai)' }
];

/** A starting point for a new script in the editor. */
export const SCRIPT_TEMPLATE = `// @feature name="My feature" version=1
// @param width: length = 0.02 min=0.001
// @param height: length = 0.01
// @param depth: length = 0.005
// @param plane: plane
// @output body: main

fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, p.width, p.height);
    let regions = sk.finish().regions();
    ctx.extrude(regions[0], #{ depth: p.depth, combine: "NewBody" })
}
`;

/**
 * Check a script (header + compile + entry; with `args`, a dry run).
 * @param {{ text?: string, sourceId?: string, entry?: string, args?: object }} what
 * @returns {Promise<{ ok: boolean, interface?: object, error?: {stage: string, reason: string}, dry_run?: object }>}
 */
export async function checkScript({ text = null, sourceId = null, entry = null, args = null } = {}) {
	if (!bridge || !engineReady) return { ok: false, error: { stage: 'engine', reason: 'Engine not ready' } };
	try {
		const resp = await bridge.send({
			type: 'CheckScript',
			source_id: sourceId,
			text,
			entry,
			args
		});
		if (resp?.type === 'ScriptChecked') return resp.check;
		return { ok: false, error: { stage: 'engine', reason: 'Unexpected engine response' } };
	} catch (err) {
		return { ok: false, error: { stage: 'engine', reason: err?.message || String(err) } };
	}
}

/**
 * The text of a source the engine holds.
 * @param {string} sourceId
 * @returns {Promise<{ source_id: string, name: string, kind: string, text: string } | null>}
 */
export async function readSource(sourceId) {
	if (!bridge || !engineReady) return null;
	const resp = await bridge.send({ type: 'ReadSource', source_id: sourceId });
	return resp?.type === 'SourceContent' ? resp : null;
}

/**
 * Add an embedded Script source (text, or a built-in library script). Any
 * text is accepted — the editor saves work in progress — and the answer
 * carries the check. Not an undo step (sources are assets).
 * @param {{ name?: string, text?: string, library?: string }} what
 * @returns {Promise<{ source_id: string, name: string, check: object } | null>}
 */
export async function addScriptSource({ name = null, text = null, library = null } = {}) {
	if (!bridge || !engineReady) return null;
	log('action', 'Add script source', { name, library, chars: text?.length ?? 0 });
	const resp = await bridge.send({ type: 'AddScriptSource', name, text, library });
	if (resp?.type !== 'ScriptSourceAdded') return null;
	// The `sources` table changed without a model update: mirror it now.
	documentSources = resp.sources ?? [];
	entityMetaCache.clear();
	scheduleAutoSave();
	return resp;
}

/**
 * Replace a Script source's text and rebuild every node naming it. Not an
 * undo step; a node the text breaks shows its typed error (never stale
 * geometry).
 * @param {string} sourceId
 * @param {string} text
 */
export async function setScriptSource(sourceId, text) {
	if (!bridge || !engineReady) return false;
	log('action', 'Set script source', { sourceId, chars: text.length });
	await sendRebuild({ type: 'SetScriptSource', source_id: sourceId, text });
	scheduleAutoSave();
	return true;
}

// The Script dialog: which source, and (editing) which node.
let scriptDialogState = $state(null);
export function getScriptDialogState() { return scriptDialogState; }

/** Open the Script dialog (toolbar "Script"). */
export function showScriptDialog() {
	if (sketchMode.active) return;
	log('ui', 'Show script dialog');
	scriptDialogState = { editingFeatureId: null, sourceId: getScriptSources()[0]?.id ?? null, editParams: null };
}

/** Open the Script dialog on an existing Script node (double-click). */
export function showScriptDialogForEdit(featureId) {
	const feature = featureTree?.features?.find((f) => f.id === featureId);
	if (!feature || feature.operation?.type !== 'Script') return;
	const params = feature.operation.params;
	log('ui', 'Show script dialog for edit', { featureId, sourceId: params.source_id });
	scriptDialogState = {
		editingFeatureId: featureId,
		sourceId: params.source_id,
		editParams: JSON.parse(JSON.stringify(params))
	};
	beginEditRollback(featureId);
}

/** Point the open dialog at another source (or a source just added). */
export function setScriptDialogSource(sourceId) {
	if (!scriptDialogState) return;
	scriptDialogState = { ...scriptDialogState, sourceId };
}

export function hideScriptDialog() {
	scriptDialogState = null;
	restoreEditRollback();
}

/**
 * Add (or, editing, replace) the Script node the dialog describes.
 * @param {{ sourceId: string, entry?: string, args: object, argExprs?: object }} choice
 * @returns {Promise<string | null>} the feature id, or null when refused
 */
export async function applyScript({ sourceId, entry = 'feature', args = {}, argExprs = {} } = {}) {
	if (!bridge || !engineReady || !sourceId) return null;
	const editing = scriptDialogState?.editingFeatureId ?? null;
	const params = { source_id: sourceId, entry, args: JSON.parse(JSON.stringify(args)) };
	if (argExprs && Object.keys(argExprs).length > 0) params.arg_exprs = { ...argExprs };
	const operation = { type: 'Script', params };
	log('action', editing ? 'Edit script feature' : 'Add script feature', { sourceId, args: Object.keys(args) });
	const before = new Set((featureTree?.features ?? []).map((f) => f.id));
	try {
		if (editing) {
			await editFeature(editing, operation);
		} else {
			await sendRebuild({ type: 'AddFeature', operation });
		}
	} catch (err) {
		const msg = err?.message || String(err);
		log('error', `Script feature failed: ${msg}`);
		showToast('error', `Script feature failed: ${msg}`);
		return null;
	}
	const id = editing ?? (featureTree?.features ?? []).find((f) => !before.has(f.id))?.id ?? null;
	const error = id ? featureErrors.get(id) : null;
	if (error) {
		// Loud in the tree and here; the dialog stays open on the node so the
		// arguments can be changed (like the mate connector dialog).
		showToast('error', `Script feature failed — ${error}`);
		if (scriptDialogState) scriptDialogState = { ...scriptDialogState, editingFeatureId: id };
		return id;
	}
	await restoreEditRollback();
	scriptDialogState = null;
	return id;
}

// The Script editor: a source's text (or a new script's).
let scriptEditorState = $state(null);
export function getScriptEditorState() { return scriptEditorState; }

/**
 * Open the editor on a Script source, or (no id) on a new script.
 * @param {string | null} sourceId
 * @param {{ text?: string, name?: string, forDialog?: boolean }} [opts] — `forDialog`:
 *   a save from a new script points the open Script dialog at it.
 */
export async function showScriptEditor(sourceId = null, opts = {}) {
	if (sketchMode.active) return;
	let text = opts.text ?? SCRIPT_TEMPLATE;
	let name = opts.name ?? '';
	if (sourceId) {
		const content = await readSource(sourceId);
		if (!content) {
			showToast('error', 'That script source is not loaded');
			return;
		}
		text = content.text;
		name = content.name;
	}
	log('ui', 'Show script editor', { sourceId });
	scriptEditorState = { sourceId, name, text, forDialog: !!opts.forDialog };
}

export function hideScriptEditor() {
	scriptEditorState = null;
}

/**
 * Save the editor's text: a new script becomes a Script source; an existing
 * one is replaced and every node using it regenerates.
 * @param {{ name: string, text: string }} draft
 * @returns {Promise<string | null>} the source id
 */
export async function saveScriptEditor({ name, text }) {
	if (!scriptEditorState) return null;
	const { sourceId, forDialog } = scriptEditorState;
	try {
		if (sourceId) {
			await setScriptSource(sourceId, text);
			scriptEditorState = { ...scriptEditorState, name, text };
			return sourceId;
		}
		const added = await addScriptSource({ name: name?.trim() || null, text });
		if (!added) {
			showToast('error', 'The script could not be added');
			return null;
		}
		scriptEditorState = { ...scriptEditorState, sourceId: added.source_id, name: added.name, text };
		if (forDialog && scriptDialogState) setScriptDialogSource(added.source_id);
		return added.source_id;
	} catch (err) {
		const msg = err?.message || String(err);
		log('error', `Script save failed: ${msg}`);
		showToast('error', `Script save failed: ${msg}`);
		return null;
	}
}

// -- Viewport pick mode helpers --

/**
 * Add a profile region from viewport click, dispatching to the appropriate dialog.
 * @param {string} featureId - sketch feature id
 * @param {number} profileIndex
 * @param {object|null} region - the picked minimal region (geometry + provenance),
 *   when the click resolved to a Rust-computed region. Sub-regions (annulus,
 *   lens) carry `profile_entity_ids == null` and are extruded from geometry.
 */
export function addProfileRegion(featureId, profileIndex, region = null) {
	if (!profilePickMode) return;

	// Find sketch name from feature tree
	const feature = featureTree?.features?.find(f => f.id === featureId);
	const sketchName = feature?.name || 'Sketch';

	if (profilePickMode.target === 'extrude') {
		addExtrudeRegion(featureId, sketchName, profileIndex, region);
	} else if (profilePickMode.target === 'revolve') {
		if (!revolveDialogState) return;
		revolveDialogState = {
			...revolveDialogState,
			selectedProfile: { sketchId: featureId, profileIndex, label: `${sketchName} / Profile ${profileIndex + 1}` }
		};
	}
}

/**
 * Set the revolve axis from a viewport pick.
 * @param {number[]} origin - [x, y, z]
 * @param {number[]} direction - [x, y, z]
 * @param {string} label
 */
export function setRevolveAxis(origin, direction, label) {
	if (!revolveDialogState) return;
	revolveDialogState = {
		...revolveDialogState,
		selectedAxis: { origin, direction, label }
	};
}

/**
 * Extract axis info from an edge GeomRef by finding matching edge vertices in mesh data.
 * @param {any} ref - GeomRef with kind.type === 'Edge'
 * @returns {{ origin: number[], direction: number[], label: string } | null}
 */
function extractAxisFromEdgeRef(ref) {
	for (const mesh of meshes) {
		if (!mesh.edges || !mesh.edges.ranges) continue;
		for (const range of mesh.edges.ranges) {
			if (!range.geom_ref || !geomRefEquals(range.geom_ref, ref)) continue;

			const verts = mesh.edges.vertices;
			const startIdx = range.start_index;
			const endIdx = range.end_index;
			if (startIdx >= endIdx || !verts || verts.length === 0) continue;

			const start = [verts[startIdx * 3], verts[startIdx * 3 + 1], verts[startIdx * 3 + 2]];
			const lastVert = endIdx - 1;
			const end = [verts[lastVert * 3], verts[lastVert * 3 + 1], verts[lastVert * 3 + 2]];

			// Check if edge is straight (only 2 unique positions means straight)
			const uniquePositions = new Set();
			for (let i = startIdx; i < endIdx; i++) {
				const key = `${verts[i*3].toFixed(8)},${verts[i*3+1].toFixed(8)},${verts[i*3+2].toFixed(8)}`;
				uniquePositions.add(key);
				if (uniquePositions.size > 2) {
					showToast('warning', 'Only straight edges can be used as revolve axis');
					return null;
				}
			}

			const { computeAxisFromEdgeVertices } = await_import_axisUtils();
			const axis = computeAxisFromEdgeVertices(start, end);
			if (!axis) return null;

			const featureId = ref?.anchor?.feature_id;
			const feature = featureId ? featureTree?.features?.find(f => f.id === featureId) : null;
			const label = feature ? `${feature.name} edge` : 'Model edge';

			return { origin: axis.origin, direction: axis.direction, label };
		}
	}
	return null;
}

// Inline import to avoid top-level dynamic import issues
function await_import_axisUtils() {
	// These are pure math functions, import them synchronously via the module system
	// We re-export them here since store.svelte.js can't use top-level await
	return {
		computeAxisFromEdgeVertices(startPos, endPos) {
			const dx = endPos[0] - startPos[0];
			const dy = endPos[1] - startPos[1];
			const dz = endPos[2] - startPos[2];
			const len = Math.sqrt(dx * dx + dy * dy + dz * dz);
			if (len < 1e-10) return null;
			return {
				origin: [...startPos],
				direction: [dx / len, dy / len, dz / len]
			};
		}
	};
}

// -- Chamfer dialog --

export function getChamferDialogState() { return chamferDialogState; }

/**
 * Show the chamfer dialog. Gathers selected Edge refs from the current selection.
 */
export function showChamferDialog() {
	const edges = selectedRefs.filter(r => r.kind?.type === 'Edge');
	log('ui', 'Show chamfer dialog', { edgeCount: edges.length });
	chamferDialogState = {
		edges: JSON.parse(JSON.stringify(edges)),
		edgeCount: edges.length
	};
}

export function hideChamferDialog() {
	chamferDialogState = null;
}

/**
 * Apply a chamfer operation from the dialog.
 * @param {number} distance
 */
export async function applyChamfer(distance) {
	if (!chamferDialogState || !bridge || !engineReady) return;

	log('action', 'Apply chamfer', { distance, edgeCount: chamferDialogState.edgeCount });
	try {
		await sendRebuild({
			type: 'AddFeature',
			operation: {
				type: 'Chamfer',
				params: {
					edges: chamferDialogState.edges,
					distance
				}
			}
		});

		chamferDialogState = null;
	} catch (err) {
		const msg = err.message || String(err);
		log('error', `Chamfer failed: ${msg}`);
		if (msg.includes('NotSupported') || msg.includes('not supported')) {
			showToast('error', 'Chamfer is not yet supported by the geometry kernel');
		} else {
			showToast('error', `Chamfer failed: ${msg}`);
		}
	}
}

// -- Fillet dialog --

export function getFilletDialogState() { return filletDialogState; }

/**
 * Show the fillet dialog. Gathers selected Edge refs from the current selection.
 */
export function showFilletDialog() {
	const edges = selectedRefs.filter(r => r.kind?.type === 'Edge');
	log('ui', 'Show fillet dialog', { edgeCount: edges.length });
	filletDialogState = {
		edges: JSON.parse(JSON.stringify(edges)),
		edgeCount: edges.length
	};
}

export function hideFilletDialog() {
	filletDialogState = null;
}

/**
 * Apply a fillet operation from the dialog.
 * @param {number} radius
 */
export async function applyFillet(radius) {
	if (!filletDialogState || !bridge || !engineReady) return;

	log('action', 'Apply fillet', { radius, edgeCount: filletDialogState.edgeCount });
	try {
		await sendRebuild({
			type: 'AddFeature',
			operation: {
				type: 'Fillet',
				params: {
					edges: filletDialogState.edges,
					radius
				}
			}
		});

		filletDialogState = null;
	} catch (err) {
		const msg = err.message || String(err);
		log('error', `Fillet failed: ${msg}`);
		if (msg.includes('NotSupported') || msg.includes('not supported')) {
			showToast('error', 'Fillet is not yet supported by the geometry kernel');
		} else {
			showToast('error', `Fillet failed: ${msg}`);
		}
	}
}

// -- Shell dialog --

export function getShellDialogState() { return shellDialogState; }

/**
 * Show the shell dialog. Gathers selected Face refs from the current selection.
 */
export function showShellDialog() {
	const faces = selectedRefs.filter(r => r.kind?.type === 'Face');
	log('ui', 'Show shell dialog', { faceCount: faces.length });
	shellDialogState = {
		faces: JSON.parse(JSON.stringify(faces)),
		faceCount: faces.length
	};
}

export function hideShellDialog() {
	shellDialogState = null;
}

/**
 * Apply a shell operation from the dialog.
 * @param {number} thickness
 */
export async function applyShell(thickness) {
	if (!shellDialogState || !bridge || !engineReady) return;

	log('action', 'Apply shell', { thickness, faceCount: shellDialogState.faceCount });
	try {
		await sendRebuild({
			type: 'AddFeature',
			operation: {
				type: 'Shell',
				params: {
					faces_to_remove: shellDialogState.faces,
					thickness
				}
			}
		});

		shellDialogState = null;
	} catch (err) {
		const msg = err.message || String(err);
		log('error', `Shell failed: ${msg}`);
		if (msg.includes('NotSupported') || msg.includes('not supported')) {
			showToast('error', 'Shell is not yet supported by the geometry kernel');
		} else if (msg.includes('planar') || msg.includes('non-planar')) {
			showToast('error', 'Shell only works on solids with planar faces');
		} else {
			showToast('error', `Shell failed: ${msg}`);
		}
	}
}

// -- Boolean dialog --

export function getBooleanDialogState() { return booleanDialogState; }

export function showBooleanDialog() {
	const tree = featureTree;
	if (!tree || !tree.features) return;

	// Find LIVE solid-producing features: a consumed or suppressed feature's
	// body is not an operand (the engine refuses a consumed one loudly,
	// `specs/b4_balanced_union.md` §2.4).
	const bodies = tree.features
		.filter(f => ['Extrude', 'Revolve', 'Pipe', 'BooleanCombine', 'UnionAll', 'Chamfer', 'Fillet', 'Shell', 'ImportedBody', 'PatternCircular', 'PatternLinear', 'PatternMirror', 'Script'].includes(f.operation?.type))
		.filter(f => !f.suppressed && !consumedFeatures.has(f.id))
		.map(f => ({ featureId: f.id, name: f.name }));

	log('ui', 'Show boolean dialog', { bodyCount: bodies.length });
	booleanDialogState = { bodies };
}

export function hideBooleanDialog() {
	booleanDialogState = null;
}

// -- Part mate connector dialog (specs/part_mate_connectors.md) --

export function getMateConnectorDialogState() { return mateConnectorDialogState; }

/**
 * Open the part mate connector dialog: to edit `featureId`'s connector, or
 * to add one on the selected face or edge (nothing selected ⇒ the part
 * origin, z up).
 * @param {string | null} [featureId]
 */
export function showMateConnectorDialog(featureId = null) {
	if (sketchMode.active) return;
	const feature = featureId ? featureTree?.features?.find(f => f.id === featureId) : null;
	if (featureId && feature?.operation?.type !== 'MateConnector') return;
	const p = feature?.operation?.params ?? {};
	const pick = feature ? null : getSelectedRefs().find(r => r?.kind?.type === 'Face' || r?.kind?.type === 'Edge');
	log('ui', 'Show mate connector dialog', { featureId });
	mateConnectorDialogState = {
		editingFeatureId: feature?.id ?? null,
		name: feature?.name ?? '',
		geomRef: JSON.parse(JSON.stringify((feature ? p.geom_ref : pick) ?? null)),
		frame: p.frame ? JSON.parse(JSON.stringify(p.frame)) : null,
		anchor: p.anchor ?? 'middle',
		flipZ: !!p.flip_z,
		rotationDeg: p.rotation_deg ?? 0,
		offsetMm: [0, 1, 2].map(k => Math.round((p.offset_m?.[k] ?? 0) * 1e6) / 1e3)
	};
}

export function hideMateConnectorDialog() {
	mateConnectorDialogState = null;
}

/**
 * A dialog choice as a `MateConnector` operation: lengths in meters, every
 * adjustment at its default omitted (the engine's own wire form).
 */
export function mateConnectorOperation({ name = '', geomRef = null, frame = null, anchor = 'middle', flipZ = false, rotationDeg = 0, offsetMm = [0, 0, 0] } = {}) {
	/** @type {Record<string, any>} */
	const params = {};
	if (String(name ?? '').trim()) params.name = String(name).trim();
	if (geomRef) params.geom_ref = JSON.parse(JSON.stringify(geomRef));
	params.frame = frame ? JSON.parse(JSON.stringify(frame)) : { origin: [0, 0, 0], z_axis: [0, 0, 1], x_axis: [0, 0, 0] };
	if (CONNECTOR_ANCHORS.includes(anchor) && anchor !== 'middle') params.anchor = anchor;
	if (flipZ) params.flip_z = true;
	const r = Number(rotationDeg) || 0;
	if (r) params.rotation_deg = r;
	const o = [0, 1, 2].map(k => (Number(offsetMm?.[k]) || 0) / 1000);
	if (o.some(v => v !== 0)) params.offset_m = o;
	return { type: 'MateConnector', params };
}

/**
 * Add (or, with the dialog editing one, replace) the part mate connector a
 * choice describes. A pick the engine cannot derive a frame from fails the
 * feature — loud, in the tree and a toast — and the dialog stays open on
 * that feature so the pick can be changed.
 * @returns {Promise<string | null>} the feature id
 */
export async function applyMateConnector(choice = {}) {
	if (!bridge || !engineReady) return null;
	const editing = mateConnectorDialogState?.editingFeatureId ?? null;
	const operation = mateConnectorOperation(choice);
	log('action', editing ? 'Edit mate connector' : 'Add mate connector', { name: operation.params.name });
	const before = new Set((featureTree?.features ?? []).map(f => f.id));
	try {
		if (editing) {
			await sendRebuild({ type: 'EditFeature', feature_id: editing, operation });
			const name = operation.params.name;
			const current = featureTree?.features?.find(f => f.id === editing);
			if (name && current && current.name !== name) await renameFeature(editing, name);
		} else {
			await sendRebuild({ type: 'AddFeature', operation });
		}
	} catch (err) {
		const msg = err?.message || String(err);
		log('error', `Mate connector failed: ${msg}`);
		showToast('error', `Mate connector failed: ${msg}`);
		return null;
	}
	const id = editing ?? (featureTree?.features ?? []).find(f => !before.has(f.id))?.id ?? null;
	const error = id ? featureErrors.get(id) : null;
	if (error) {
		showToast('error', `Cannot put a mate connector here — ${error}`);
		if (mateConnectorDialogState) mateConnectorDialogState = { ...mateConnectorDialogState, editingFeatureId: id };
		return id;
	}
	mateConnectorDialogState = null;
	return id;
}

/**
 * Apply a boolean combine operation from the dialog.
 * @param {string} operation - 'Union', 'Subtract', or 'Intersect'
 * @param {string} targetFeatureId
 * @param {string} toolFeatureId
 */
export async function applyBoolean(operation, targetFeatureId, toolFeatureId) {
	if (!booleanDialogState || !bridge || !engineReady) return;

	log('action', 'Apply boolean', { operation, targetFeatureId, toolFeatureId });

	const bodyA = {
		kind: { type: 'Face' },
		anchor: { type: 'FeatureOutput', feature_id: targetFeatureId, output_key: { type: 'Main' } },
		selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
		policy: { type: 'BestEffort' }
	};

	const bodyB = {
		kind: { type: 'Face' },
		anchor: { type: 'FeatureOutput', feature_id: toolFeatureId, output_key: { type: 'Main' } },
		selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
		policy: { type: 'BestEffort' }
	};

	try {
		await sendRebuild({
			type: 'AddFeature',
			operation: {
				type: 'BooleanCombine',
				params: {
					body_a: bodyA,
					body_b: bodyB,
					operation: { type: operation }
				}
			}
		});

		booleanDialogState = null;
	} catch (err) {
		const msg = err.message || String(err);
		log('error', `Boolean failed: ${msg}`);
		showToast('error', `Boolean operation failed: ${msg}`);
	}
}

/**
 * Union every live body of the part into connected solids with ONE feature
 * (`specs/b4_balanced_union.md`): a balanced tree of pairwise unions with a
 * bounding-box fast path, progress in the status bar.
 */
export async function applyUnionAll() {
	if (!bridge || !engineReady) return;
	log('action', 'Apply union all');
	try {
		await sendRebuild({
			type: 'AddFeature',
			operation: { type: 'UnionAll', params: { targets: { type: 'All' } } }
		});
		booleanDialogState = null;
	} catch (err) {
		const msg = err.message || String(err);
		log('error', `Union all failed: ${msg}`);
		showToast('error', `Union all failed: ${msg}`);
	}
}

// -- Sketch-on-face: compute face plane from mesh data --

/**
 * Compute the plane (origin + normal) for a face GeomRef from mesh triangle data.
 * @param {any} geomRef
 * @returns {{ origin: [number,number,number], normal: [number,number,number] } | null}
 */
export function computeFacePlane(geomRef) {
	if (!geomRef) return null;

	// Handle datum planes directly. User-created datums (including
	// offset-from-face) need the feature list and a face resolver — pass the
	// feature tree and computeFacePlane itself (recursively) so an
	// offset-face datum resolves through its base face.
	if (isDatumPlaneRef(geomRef)) {
		const planeId = getPlaneIdFromRef(geomRef);
		if (!planeId) return null;
		const features = featureTree?.features ?? [];
		const plane = getPlaneById(planeId, features);
		if (!plane) return null;
		try {
			return resolvePlane(plane.definition, features, computeFacePlane);
		} catch {
			return null;
		}
	}

	for (const mesh of meshes) {
		if (!mesh.faceRanges) continue;
		for (const range of mesh.faceRanges) {
			if (!range.geom_ref) continue;
			if (!geomRefEquals(range.geom_ref, geomRef)) continue;

			// A ghost face (in-context editing) carries the engine's plane —
			// face centroid + normal in this part's frame — which is exactly
			// what the engine re-derives on rebuild, so a sketch started here
			// does not slide when its plane is re-resolved.
			if (range.plane?.origin && range.plane?.normal) {
				return { origin: [...range.plane.origin], normal: [...range.plane.normal] };
			}

			// Get first triangle from this face range
			// start_index is already an index into the indices array
			const triStart = range.start_index;
			if (triStart + 2 >= mesh.indices.length) continue;

			const i0 = mesh.indices[triStart];
			const i1 = mesh.indices[triStart + 1];
			const i2 = mesh.indices[triStart + 2];

			const v0 = [mesh.vertices[i0 * 3], mesh.vertices[i0 * 3 + 1], mesh.vertices[i0 * 3 + 2]];
			const v1 = [mesh.vertices[i1 * 3], mesh.vertices[i1 * 3 + 1], mesh.vertices[i1 * 3 + 2]];
			const v2 = [mesh.vertices[i2 * 3], mesh.vertices[i2 * 3 + 1], mesh.vertices[i2 * 3 + 2]];

			// edge vectors
			const e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
			const e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

			// cross product
			const nx = e1[1] * e2[2] - e1[2] * e2[1];
			const ny = e1[2] * e2[0] - e1[0] * e2[2];
			const nz = e1[0] * e2[1] - e1[1] * e2[0];
			const len = Math.sqrt(nx * nx + ny * ny + nz * nz);
			if (len < 1e-12) continue;

			const normal = /** @type {[number,number,number]} */ ([nx / len, ny / len, nz / len]);
			const origin = /** @type {[number,number,number]} */ ([
				(v0[0] + v1[0] + v2[0]) / 3,
				(v0[1] + v1[1] + v2[1]) / 3,
				(v0[2] + v1[2] + v2[2]) / 3
			]);

			return { origin, normal };
		}
	}

	return null;
}

/**
 * Compute the bounding box of a face for zoom-to-face.
 * @param {any} geomRef
 * @returns {{ center: [number,number,number], normal: [number,number,number], size: number } | null}
 */
export function computeFaceBounds(geomRef) {
	const plane = computeFacePlane(geomRef);
	if (!plane) return null;

	let minX = Infinity, minY = Infinity, minZ = Infinity;
	let maxX = -Infinity, maxY = -Infinity, maxZ = -Infinity;
	let found = false;

	for (const mesh of meshes) {
		if (!mesh.faceRanges) continue;
		for (const range of mesh.faceRanges) {
			if (!range.geom_ref || !geomRefEquals(range.geom_ref, geomRef)) continue;
			for (let idx = range.start_index; idx < range.end_index; idx++) {
				const vi = mesh.indices[idx];
				const x = mesh.vertices[vi * 3];
				const y = mesh.vertices[vi * 3 + 1];
				const z = mesh.vertices[vi * 3 + 2];
				if (x < minX) minX = x;
				if (y < minY) minY = y;
				if (z < minZ) minZ = z;
				if (x > maxX) maxX = x;
				if (y > maxY) maxY = y;
				if (z > maxZ) maxZ = z;
				found = true;
			}
		}
	}

	if (!found) return null;

	const dx = maxX - minX, dy = maxY - minY, dz = maxZ - minZ;
	const size = Math.sqrt(dx * dx + dy * dy + dz * dz);
	return {
		center: /** @type {[number,number,number]} */ ([(minX + maxX) / 2, (minY + maxY) / 2, (minZ + maxZ) / 2]),
		normal: plane.normal,
		size: Math.max(size, 0.0001),
	};
}

/**
 * Finish the active sketch, sending solved positions and profiles to the engine.
 * Returns the sketch feature info (for optional extrude dialog follow-up).
 */
export async function finishSketch() {
	if (!bridge || !engineReady) return;

	const { profiles, solvedPositions: posObj } = buildFinishProfiles(
		extractedProfilesState,
		sketchEntities,
		sketchPositions
	);

	const profileCount = profiles.length;

	// Capture plane geometry before exiting sketch mode (spread to unwrap proxies)
	const planeOrigin = [...sketchMode.origin];
	const planeNormal = [...sketchMode.normal];

	log('action', 'Finish sketch', { entityCount: sketchEntities.length, profileCount, editing: !!editingSketchFeatureId });

	// Constraints for the persisted feature (Rust format). Persistent pins
	// (origin/reference snaps, Fix modal — non-_isDrag WhereDragged with a
	// stored target) lower to Pinned{point,x,y} so they SURVIVE re-edit;
	// dropping them here is how saved sketches silently lost their origin
	// locks (specs/pinned_constraint.md B6). Transient drag pins and
	// targetless legacy entries are still filtered out.
	const persistedConstraints = sketchConstraints
		.filter((c) => !(c.type === 'WhereDragged' && (c._isDrag || c.x == null || c.y == null)))
		.map((c) => mapConstraintForBridge(c))
		.filter(Boolean);

	// Edit path: update existing sketch feature via EditFeature
	if (editingSketchFeatureId) {
		const editId = editingSketchFeatureId;
		const feature = featureTree.features.find(f => f.id === editId);
		editingSketchFeatureId = null;

		try {
			// Deep-clone everything to strip Svelte 5 proxies before postMessage.
			// Re-use the original sketch's id, plane, and solve_status (Rust format)
			// since the JS sketchSolveStatus has a different shape than the Rust SolveStatus enum.
			const origSketch = feature?.operation?.sketch;
			const operation = JSON.parse(JSON.stringify({
				type: 'Sketch',
				sketch: {
					id: origSketch?.id || editId,
					plane: origSketch?.plane || null,
					plane_origin: planeOrigin,
					plane_normal: planeNormal,
					// A sketch that came with its own x axis keeps it: dropping
					// it here would silently re-base every point on edit.
					...(origSketch?.plane_x_axis ? { plane_x_axis: origSketch.plane_x_axis } : {}),
					entities: sketchEntities,
					constraints: persistedConstraints,
					solve_status: origSketch?.solve_status || { type: 'UnderConstrained', dof: 0 },
					solved_positions: posObj,
					solved_profiles: profiles,
				}
			}));
			await editFeature(editId, operation);
		} catch (err) {
			// Downstream feature rebuild errors (e.g. ProfileOutOfRange) should not
			// block saving the sketch itself. The sketch data is valid — dependent
			// features will show as failed in the feature tree.
			log('warn', `Sketch saved but downstream rebuild had errors: ${err.message}`);
			statusMessage = `Sketch saved. Downstream feature error: ${err.message}`;
			lastError = err.message;
		}
		exitSketchMode();
		setActiveTool('select');
		return { profileCount };
	}

	// Send to engine FIRST, exit sketch mode only on success
	try {
		await sendRebuild({
			type: 'FinishSketch',
			solved_positions: posObj,
			solved_profiles: profiles,
			plane_origin: planeOrigin,
			plane_normal: planeNormal,
			...(sketchMode.xAxis ? { plane_x_axis: JSON.parse(JSON.stringify(sketchMode.xAxis)) } : {}),
			entities: JSON.parse(JSON.stringify(sketchEntities)),
			constraints: JSON.parse(JSON.stringify(persistedConstraints)),
			projected: JSON.parse(JSON.stringify(projectedBindings)),
		});
		// Only clear sketch state after successful commit
		exitSketchMode();
		setActiveTool('select');
	} catch (err) {
		log('error', `Finish sketch failed: ${err.message}`);
		statusMessage = `Sketch save failed: ${err.message}`;
		lastError = err.message;
		// Sketch state is preserved — user can retry or fix issues
	}

	return { profileCount };
}

// -- Camera state accessors (used by CameraControls and __waffle) --

/**
 * Store camera and controls references. Called by CameraControls on mount.
 * @param {import('three').PerspectiveCamera | import('three').OrthographicCamera} camera
 * @param {any} controls - OrbitControls instance
 */
/**
 * Register the scene and renderer so `getRenderStats()` can report what a
 * frame actually costs. Measurement only — nothing reads these to draw.
 */
export function setSceneRefs(scene, renderer) {
	sceneObject = scene;
	rendererObject = renderer;
}

/**
 * What the last frame cost: the renderer's own counters plus a census of the
 * scene graph. Draw calls are the number that matters for a big model — three
 * thousand objects is three thousand state changes per frame however small
 * each one is.
 */
export function getRenderStats() {
	if (!rendererObject) return null;
	const info = rendererObject.info;
	const census = { meshes: 0, lineSegments: 0, lines: 0, points: 0, other: 0, culled: 0 };
	sceneObject?.traverse((o) => {
		if (!o.visible) return;
		if (o.isMesh) census.meshes++;
		else if (o.isLineSegments) census.lineSegments++;
		else if (o.isLine) census.lines++;
		else if (o.isPoints) census.points++;
		else census.other++;
		if (o.isObject3D && o.frustumCulled === false) census.culled++;
	});
	return {
		calls: info.render.calls,
		triangles: info.render.triangles,
		lines: info.render.lines,
		points: info.render.points,
		programs: info.programs?.length ?? 0,
		geometries: info.memory.geometries,
		textures: info.memory.textures,
		census
	};
}

export function setCameraRefs(camera, controls) {
	cameraObject = camera;
	controlsObject = controls;
}

/**
 * The live camera and controls objects. For viewport components that must
 * read them every frame (the rotation-center marker); everything else should
 * go through `getCameraState()`.
 * @returns {{ camera: any, controls: any }}
 */
export function getCameraRefs() {
	return { camera: cameraObject, controls: controlsObject };
}

/**
 * Get camera state for tests and external access.
 * @returns {{ position: number[], target: number[], fov: number, up: number[], zoom: number, projection: string } | null}
 */
export function getCameraState() {
	if (!cameraObject) return null;
	const pos = cameraObject.position;
	const up = cameraObject.up;
	const target = controlsObject?.target;
	return {
		position: [pos.x, pos.y, pos.z],
		target: target ? [target.x, target.y, target.z] : [0, 0, 0],
		fov: /** @type {any} */ (cameraObject).fov ?? 0,
		up: [up.x, up.y, up.z],
		zoom: cameraObject.zoom ?? 1,
		projection: cameraProjection,
		frustumTop: /** @type {any} */ (cameraObject).top ?? null,
		// The point an orbit turns about, which is NOT the look-at target: it
		// is re-anchored to the model under the cursor at the start of every
		// rotate (`CameraControls.svelte` `anchorOrbitPivot`). Null until the
		// first fit or orbit.
		orbitPivot: /** @type {any} */ (controlsObject)?.orbitPivot?.toArray() ?? null,
	};
}

/**
 * Get the camera object directly (for raycasting, zoom-to-cursor, etc.)
 * @returns {import('three').PerspectiveCamera | null}
 */
export function getCameraObject() {
	return cameraObject;
}

/**
 * Get the OrbitControls instance directly.
 * @returns {any | null}
 */
export function getControlsObject() {
	return controlsObject;
}

// -- Section view (capped clipping) --

/** Reactive section-view state. */
export function getSectionState() {
	return sectionState;
}

/** True when a capped section view is active. */
export function isSectionActive() {
	return sectionState.active;
}

/**
 * Toggle the section view. When turning on, captures the currently-selected
 * datum plane or planar face as the section plane. If nothing suitable is
 * selected, shows a hint toast and stays off. Toggling while active turns it
 * off (restoring the normal view exactly).
 * @returns {boolean} the resulting active state
 */
export function toggleSection() {
	if (sectionState.active) {
		clearSection();
		return false;
	}

	// Resolve a section plane from the current selection.
	let plane = null;
	for (const ref of selectedRefs) {
		if (isDatumPlaneRef(ref) || ref?.kind?.type === 'Face') {
			const p = computeFacePlane(ref);
			if (p) { plane = p; break; }
		}
	}

	if (!plane) {
		showToast('info', 'Select a plane or planar face');
		return false;
	}

	sectionState = {
		active: true,
		plane: { origin: [...plane.origin], normal: [...plane.normal] },
		flipped: false,
		offset: 0,
	};
	log('ui', 'Section view on', { origin: plane.origin, normal: plane.normal });
	return true;
}

/** Flip which half of the model the section keeps. */
export function flipSection() {
	if (!sectionState.active) return;
	sectionState = { ...sectionState, flipped: !sectionState.flipped };
}

/**
 * Set the section cut offset along the plane normal (meters).
 * @param {number} offset
 */
export function setSectionOffset(offset) {
	if (!sectionState.active) return;
	sectionState = { ...sectionState, offset: Number(offset) || 0 };
}

/** Clear/exit the section view, restoring the normal (un-clipped) render. */
export function clearSection() {
	if (!sectionState.active) return;
	sectionState = { active: false, plane: null, flipped: false, offset: 0 };
	log('ui', 'Section view off');
}

// -- Camera projection state accessors --

/** @returns {'orthographic' | 'perspective'} */
export function getCameraProjection() { return cameraProjection; }

/** @param {'orthographic' | 'perspective'} proj */
export function setCameraProjection(proj) {
	cameraProjection = proj;
	window.dispatchEvent(new CustomEvent('waffle-camera-projection-changed', { detail: { projection: proj } }));
}

export function toggleCameraProjection() {
	setCameraProjection(cameraProjection === 'orthographic' ? 'perspective' : 'orthographic');
}

/** @returns {string} */
export function getViewCubeTransform() { return viewCubeTransform; }

/** @param {string} css */
export function setViewCubeTransform(css) { viewCubeTransform = css; }

// -- Box selection state --

export function getBoxSelectState() { return boxSelectState; }
/**
 * @param {Partial<typeof boxSelectState>} updates
 */
export function setBoxSelectState(updates) {
	boxSelectState = { ...boxSelectState, ...updates };
}

// -- Select Other state --

export function getSelectOtherState() { return selectOtherState; }
/**
 * @param {Partial<typeof selectOtherState>} updates
 */
export function setSelectOtherState(updates) {
	selectOtherState = { ...selectOtherState, ...updates };
}

// -- Design parameters (variables) --

/**
 * The design-parameter table from the live feature tree.
 * Each row: { id, name, expression, value, error } — `value` is the engine's
 * evaluated mm-space number, `error` a user-facing message or absent.
 * @returns {Array<object>}
 */
export function getParameters() {
	return featureTree?.parameters ?? [];
}

/**
 * Replace the design-parameter table (send the COMPLETE list) and rebuild.
 * Evaluated values/errors come back on the updated feature tree.
 * @param {Array<{id?: string, name: string, expression: string}>} parameters
 */
export async function setParameters(parameters) {
	if (!bridge || !engineReady) return;
	log('action', 'Set parameters', { count: parameters.length });
	const payload = parameters.map((p) => ({
		id: p.id || crypto.randomUUID(),
		name: p.name,
		expression: p.expression,
		value: typeof p.value === 'number' ? p.value : 0
	}));
	try {
		await sendRebuild({
			type: 'SetParameters',
			parameters: JSON.parse(JSON.stringify(payload))
		});
	} catch (err) {
		log('error', `Set parameters failed: ${err.message}`);
		showToast('error', `Variables update failed: ${err.message}`);
	}
}

/**
 * Evaluate one expression against the current variables (stateless).
 * Resolves to { value, error }: `value` is an mm-space number (lengths in mm,
 * angles in degrees) or null; `error` a user-facing message or null.
 * @param {string} expression
 */
export async function evaluateExpression(expression) {
	if (!bridge || !engineReady) return { value: null, error: 'Engine not ready' };
	try {
		const resp = await bridge.send({ type: 'EvaluateExpression', expression });
		if (resp?.type === 'ExpressionEvaluated') {
			return { value: resp.value ?? null, error: resp.error ?? null };
		}
		return { value: null, error: 'Unexpected engine response' };
	} catch (err) {
		return { value: null, error: err.message };
	}
}

// -- Dimension popup --

export function getDimensionPopup() { return dimensionPopup; }

/**
 * Show the dimension input popup.
 * @param {{ entityA: number, entityB: number | null, sketchX: number, sketchY: number, dimType: 'distance'|'radius'|'angle', defaultValue: number }} popup
 */
export function showDimensionPopup(popup) { dimensionPopup = popup; }

export function hideDimensionPopup() { dimensionPopup = null; }

/**
 * Apply the dimension value from the popup as a constraint. When the user
 * typed an expression, `expression` carries it (already evaluated to `value`
 * by the engine); it is stored on the constraint so rebuilds re-drive it.
 * @param {number} value
 * @param {string | null} [expression]
 */
export function applyDimensionFromPopup(value, expression = null) {
	if (!dimensionPopup) return;
	const p = dimensionPopup;

	// Custom callback takes priority over built-in dimType handling
	if (p.customApply) {
		dimensionPopup = null;
		p.customApply(value);
		return;
	}

	const withExpr = (c, expr) => (expr ? { ...c, expression: expr } : c);

	if (p.dimType === 'distance') {
		if (p.entityB != null) {
			addLocalConstraint(withExpr({ type: 'Distance', entity_a: p.entityA, entity_b: p.entityB, value }, expression));
		} else {
			// Single line — distance between endpoints
			const entity = sketchEntities.find(e => e.id === p.entityA);
			if (entity && entity.type === 'Line') {
				addLocalConstraint(withExpr({ type: 'Distance', entity_a: entity.start_id, entity_b: entity.end_id, value }, expression));
			}
		}
	} else if (p.dimType === 'pointLineDistance') {
		addLocalConstraint(withExpr({ type: 'PointLineDistance', point: p.entityA, entity: p.entityB, value }, expression));
	} else if (p.dimType === 'diameter') {
		addLocalConstraint(withExpr({ type: 'Diameter', entity: p.entityA, value }, expression));
	} else if (p.dimType === 'radius') {
		// The popup edits the RADIUS but stores a Diameter constraint, so a
		// radius expression is wrapped as diameter = 2*(radius expression).
		addLocalConstraint(withExpr({ type: 'Diameter', entity: p.entityA, value: value * 2 }, expression ? `2*(${expression})` : null));
	} else if (p.dimType === 'angle') {
		if (p.entityB != null) {
			addLocalConstraint(withExpr({ type: 'Angle', line_a: p.entityA, line_b: p.entityB, value_degrees: value }, expression));
		}
	}

	dimensionPopup = null;
}

export function getSnapSettings() { return snapSettings; }
/**
 * Update snap threshold settings.
 * @param {Partial<{ coincidentPx: number, onEntityPx: number, hvAngleDeg: number, previewPx: number }>} updates
 */
export function updateSnapSettings(updates) {
	snapSettings = { ...snapSettings, ...updates };
}

/**
 * Arm a sketch point as an alignment-inference source (most-recent first, LRU
 * capped, deduped by id). Called when a coincident snap lands on a real point.
 * @param {number} id
 * @param {number} x
 * @param {number} y
 */
export function armInferenceSource(id, x, y) {
	if (id == null) return;
	const rest = inferenceSources.filter((s) => s.id !== id);
	inferenceSources = [{ id, x, y }, ...rest].slice(0, INFERENCE_SOURCES_MAX);
}

/**
 * Armed inference sources, most-recent first, with any whose point no longer
 * exists (undo/delete) dropped so no dangling-id constraint can be emitted.
 * @returns {Array<{ id: number, x: number, y: number }>}
 */
export function getInferenceSources() {
	return inferenceSources.filter((s) => sketchPositions.has(s.id));
}

/** Clear all armed inference sources (sketch exit / tool switch). */
export function clearInferenceSources() {
	if (inferenceSources.length) inferenceSources = [];
}

// -- Sketch plane dialog --

export function getSketchPlaneDialogVisible() { return sketchPlaneDialogVisible; }
export function getSketchPlaneDialogSelection() { return sketchPlaneDialogSelection; }

/** @param {{ origin: [number,number,number], normal: [number,number,number], label: string } | null} sel */
export function setSketchPlaneDialogSelection(sel) { sketchPlaneDialogSelection = sel; }

export function showSketchPlaneDialog() {
	log('ui', 'Show sketch plane dialog');
	sketchPlaneDialogSelection = null;
	sketchPlaneDialogStartInOffset = false;
	sketchPlaneDialogVisible = true;
}

/**
 * Open the dialog directly in the datum-plane (offset) creation flow, so a
 * DatumPlane feature can be created standalone (without starting a sketch).
 */
export function showDatumPlaneDialog() {
	log('ui', 'Show datum plane dialog (standalone)');
	sketchPlaneDialogSelection = null;
	sketchPlaneDialogStartInOffset = true;
	sketchPlaneDialogVisible = true;
}

export function getSketchPlaneDialogStartInOffset() { return sketchPlaneDialogStartInOffset; }

export function hideSketchPlaneDialog() {
	sketchPlaneDialogVisible = false;
	sketchPlaneDialogSelection = null;
	sketchPlaneDialogStartInOffset = false;
}

export async function confirmSketchPlaneDialog() {
	if (!sketchPlaneDialogSelection) return;
	const sel = sketchPlaneDialogSelection;
	sketchPlaneDialogVisible = false;
	sketchPlaneDialogSelection = null;
	await enterSketchMode(sel.origin, sel.normal);
	setActiveTool('line');
}

// -- Inline sketch plane selection mode --

export function getSketchPlaneSelectionMode() { return sketchPlaneSelectionMode; }

export function enterSketchPlaneSelection() {
	log('ui', 'Enter sketch plane selection mode');
	sketchPlaneSelectionMode = true;
}

export function exitSketchPlaneSelection() {
	log('ui', 'Exit sketch plane selection mode');
	sketchPlaneSelectionMode = false;
}


// -- Sketch visibility --

/**
 * Check if a sketch feature's wireframe is visible.
 * @param {string} featureId
 * @returns {boolean}
 */
export function isSketchVisible(featureId) {
	return sketchVisibility.get(featureId) ?? true;
}

/**
 * Toggle visibility of a sketch feature's wireframe.
 * @param {string} featureId
 */
export function toggleSketchVisibility(featureId) {
	const next = new Map(sketchVisibility);
	next.set(featureId, !(sketchVisibility.get(featureId) ?? true));
	sketchVisibility = next;
}

/**
 * Show all sketch features.
 * @param {Array<{id: string, operation?: {type: string}}>} features
 */
export function showAllSketches(features) {
	const next = new Map(sketchVisibility);
	for (const f of features) {
		if (f.operation?.type === 'Sketch') next.set(f.id, true);
	}
	sketchVisibility = next;
}

/**
 * Hide all sketch features.
 * @param {Array<{id: string, operation?: {type: string}}>} features
 */
export function hideAllSketches(features) {
	const next = new Map(sketchVisibility);
	for (const f of features) {
		if (f.operation?.type === 'Sketch') next.set(f.id, false);
	}
	sketchVisibility = next;
}

// -- Plane visibility --

/**
 * Check if a datum plane is visible.
 * @param {string} planeId
 * @returns {boolean}
 */
export function isPlaneVisible(planeId) {
	return planeVisibility.get(planeId) ?? true;
}

/**
 * Toggle visibility of a datum plane.
 * @param {string} planeId
 */
export function togglePlaneVisibility(planeId) {
	const next = new Map(planeVisibility);
	next.set(planeId, !(planeVisibility.get(planeId) ?? true));
	planeVisibility = next;
}

/**
 * Show all datum planes.
 * @param {Array<{id: string}>} planes
 */
export function showAllPlanes(planes) {
	const next = new Map(planeVisibility);
	for (const p of planes) next.set(p.id, true);
	planeVisibility = next;
}

/**
 * Hide all datum planes.
 * @param {Array<{id: string}>} planes
 */
export function hideAllPlanes(planes) {
	const next = new Map(planeVisibility);
	for (const p of planes) next.set(p.id, false);
	planeVisibility = next;
}

// -- Axis visibility --

/**
 * Check if an origin axis is visible.
 * @param {string} axisId - 'x', 'y', or 'z'
 * @returns {boolean}
 */
export function isAxisVisible(axisId) {
	return axisVisibility.get(axisId) ?? true;
}

/**
 * Toggle visibility of an origin axis.
 * @param {string} axisId - 'x', 'y', or 'z'
 */
export function toggleAxisVisibility(axisId) {
	const next = new Map(axisVisibility);
	next.set(axisId, !(axisVisibility.get(axisId) ?? true));
	axisVisibility = next;
}

/** Show all origin axes. */
export function showAllAxes() {
	const next = new Map(axisVisibility);
	next.set('x', true); next.set('y', true); next.set('z', true);
	axisVisibility = next;
}

/** Hide all origin axes. */
export function hideAllAxes() {
	const next = new Map(axisVisibility);
	next.set('x', false); next.set('y', false); next.set('z', false);
	axisVisibility = next;
}

// -- Body visibility --

/**
 * Check if a body (by `bodyId`) is visible. Visibility is a client-side display
 * toggle independent of the engine: hidden bodies are skipped at render time but
 * still exist in the model.
 * @param {string} bodyId
 * @returns {boolean}
 */
export function isBodyVisible(bodyId) {
	return bodyVisibility.get(bodyId) ?? true;
}

/**
 * Toggle visibility of a body.
 * @param {string} bodyId
 */
export function toggleBodyVisibility(bodyId) {
	const next = new Map(bodyVisibility);
	next.set(bodyId, !(bodyVisibility.get(bodyId) ?? true));
	bodyVisibility = next;
}

// -- Feature-edit rollback --
//
// Editing a feature rolls the timeline back so the feature's own body (and every
// body/sketch/plane created after it) is hidden while the user adjusts it — the
// live ghost preview stands in for the old result. The prior rollback point is
// restored when the edit is applied or cancelled.

/**
 * Roll the timeline back for editing the given feature. Captures the current
 * `active_index` so it can be restored later. For Extrude/Revolve the feature's
 * own output is excluded (rollback to the feature just before it) so its stale
 * body disappears; for a Sketch the feature is kept active (it has no body and
 * the live editor renders it) while everything downstream is hidden.
 * @param {string} featureId
 */
function beginEditRollback(featureId) {
	const idx = featureTree.features.findIndex((f) => f.id === featureId);
	if (idx < 0) return;
	const opType = featureTree.features[idx].operation?.type;
	const target = opType === 'Extrude' || opType === 'Revolve' ? idx - 1 : idx;
	if (target < 0) return; // nothing sensible to roll back to (no preceding feature)
	// Capture the pre-edit rollback point only once, so a re-entered edit doesn't
	// clobber the original value with an already-rolled-back index.
	if (savedEditRollbackIndex === undefined) {
		savedEditRollbackIndex = featureTree.active_index ?? null;
	}
	const index = target >= featureTree.features.length - 1 ? null : target;
	setRollbackIndex(index);
}

/**
 * Restore the rollback point saved by {@link beginEditRollback}. No-op when no
 * edit-driven rollback is active (e.g. creating a new feature).
 */
async function restoreEditRollback() {
	if (savedEditRollbackIndex === undefined) return;
	const restore = savedEditRollbackIndex;
	savedEditRollbackIndex = undefined;
	await setRollbackIndex(restore);
}

/**
 * Get the feature ID of the sketch currently being edited (null if creating new).
 * @returns {string | null}
 */
export function getEditingSketchFeatureId() {
	return editingSketchFeatureId;
}

/**
 * Enter sketch edit mode for an existing sketch feature.
 * Loads the sketch's saved entities/constraints/positions and re-enters sketch mode.
 * @param {string} featureId
 */
export async function enterSketchEditMode(featureId) {
	if (extrudeDialogState) return; // Don't edit sketch while extrude dialog is open
	const tree = featureTree;
	const feature = tree.features.find(f => f.id === featureId);
	if (!feature || feature.operation?.type !== 'Sketch') return;

	const sketch = feature.operation.sketch;
	if (!sketch) return;

	log('action', 'Enter sketch edit mode', { featureId, entityCount: sketch.entities?.length });

	editingSketchFeatureId = featureId;
	beginEditRollback(featureId);
	resetSketchState();

	// Repopulate sketch state from saved data. Stored Pinned constraints
	// upconvert to the in-session WhereDragged form so badges, snap logic and
	// deletion keep working on the single JS-side pin representation (the
	// bridge lowers them back to Pinned on every solve/save).
	sketchEntities = JSON.parse(JSON.stringify(sketch.entities || []));
	sketchConstraints = JSON.parse(JSON.stringify(sketch.constraints || [])).map((c) =>
		c.type === 'Pinned' ? { type: 'WhereDragged', point: c.point, x: c.x, y: c.y } : c
	);

	// Parse solved_positions: { "id": [x, y] } -> Map<Number, {x, y}>
	const savedPos = sketch.solved_positions || {};
	const posMap = new Map();
	for (const [id, coords] of Object.entries(savedPos)) {
		if (Array.isArray(coords) && coords.length >= 2) {
			posMap.set(Number(id), { x: coords[0], y: coords[1] });
		}
	}
	sketchPositions = posMap;

	// Set nextEntityId to avoid collisions
	let maxId = 0;
	for (const e of sketchEntities) {
		if (e.id > maxId) maxId = e.id;
	}
	nextEntityId = maxId + 1;

	// Rebuild gear grouping/display from the compact Gear entities so gears
	// persisted across reload render and stay editable as gears.
	await rebuildGearsFromEntities();

	reExtractProfiles();

	// Send BeginSketch to engine with the sketch's plane
	if (bridge && engineReady && sketch.plane) {
		try {
			await bridge.send({ type: 'BeginSketch', plane: JSON.parse(JSON.stringify(sketch.plane)) });
		} catch (err) {
			log('error', `BeginSketch (edit) failed: ${err}`);
		}
	}

	const origin = sketch.plane_origin || [0, 0, 0];
	const normal = sketch.plane_normal || [0, 0, 1];
	// Editing a sketch that carries its own x axis must draw it in ITS basis,
	// not in the derived one, or every point moves.
	sketchMode = { active: true, origin, normal, xAxis: sketch.plane_x_axis ?? null };

	// Re-send all entities/constraints to engine
	if (bridge && engineReady) {
		for (const entity of sketchEntities) {
			const cloned = JSON.parse(JSON.stringify(entity));
			bridge.send({ type: 'AddSketchEntity', entity: cloned }).catch(() => {});
		}
		for (const constraint of sketchConstraints) {
			const cloned = JSON.parse(JSON.stringify(constraint));
			const bridgeConstraint = mapConstraintForBridge(cloned);
			if (bridgeConstraint) {
				bridge.send({ type: 'AddConstraint', constraint: bridgeConstraint }).catch(() => {});
			}
		}
	}

	triggerSolve();

	// Save camera and align to sketch plane
	if (typeof window !== 'undefined') {
		window.dispatchEvent(new Event('waffle-save-camera'));
		window.dispatchEvent(new CustomEvent('waffle-align-to-plane', { detail: { origin, normal, xAxis: sketchMode.xAxis } }));
	}
}

// -- Mobile layout --

export function getMobileLayout() { return isMobileLayout; }

/** @param {boolean} val */
export function setMobileLayout(val) {
	isMobileLayout = val;
	if (!val) mobileActivePanel = null;
}

export function getMobileActivePanel() { return mobileActivePanel; }

/**
 * Toggle a mobile panel. Only one panel open at a time.
 * @param {'left' | 'right'} panel
 */
export function toggleMobilePanel(panel) {
	mobileActivePanel = mobileActivePanel === panel ? null : panel;
}

// -- Two-finger touch gesture --

export function isTwoFingerGestureActive() { return twoFingerActive; }
/** @param {boolean} v */
export function setTwoFingerActive(v) { twoFingerActive = v; }

// -- Orbit gesture (the "display rotation center" debug marker) --

export function isOrbitActive() { return orbitActive; }
/** @param {boolean} v */
export function setOrbitActive(v) {
	if (orbitActive !== v) orbitActive = v;
}

// -- Project name --

export function getProjectName() { return projectName; }
/** @param {string} name */
export function setProjectName(name) { setDocumentName(name); }

// -- Document display unit --

export function getDocumentDisplayUnit() { return documentDisplayUnit; }
/** @param {string} unit */
export function setDocumentDisplayUnit(unit) {
	documentDisplayUnit = unit;
	// Notify engine so it persists in save
	if (bridge && engineReady) {
		bridge.send({ type: 'SetDocumentMeta', display_unit: unit }).catch(() => {});
	}
}

// -- Document model --

export function getActiveDocId() { return activeDocId; }
export function getActiveTabId() { return activeTabId; }
export function getDocumentLink() { return documentLink; }
/** True when the open document came from a share link and must not be saved over. */
export function isDocumentReadOnly() { return documentLink?.readOnly === true; }

/** The open document's identity, tabs and read-only state (agent link `document_info`). */
export function getDocumentInfo() {
	return {
		documentId,
		storageId: activeDocId,
		name: documentName,
		tabs: documentTabs.map((t) => ({ id: t.id, name: t.name, kind: t.kind?.type ?? 'Part' })),
		activeTab: activeTabId,
		readOnly: documentLink?.readOnly === true
	};
}
export function getDocumentTabs() { return documentTabs; }
export function getDocumentName() { return documentName; }
export function setDocumentName(name) {
	documentName = name;
	projectName = name;
	// The name is a MIRROR of the session now (S2 C4): assigning it locally
	// would last exactly until the next `ModelUpdated` overwrote it from the
	// engine. Renames have to reach the session to stick — File→Open takes the
	// document's name from the FILENAME this way, and the inline rename in the
	// toolbar does too.
	if (bridge && engineReady) {
		bridge.send({ type: 'SetDocumentMeta', name }).catch((err) => {
			log('error', `SetDocumentMeta(name) failed: ${err?.message || err}`);
		});
	}
}

/**
 * Check sessionStorage for a pending document (set by /doc/[id] route) and load it.
 * Called from +page.svelte onMount — separate from initEngine because SvelteKit
 * client-side navigation means the layout onMount (which calls initEngine) doesn't re-fire.
 */
export async function loadPendingDocument() {
	if (typeof sessionStorage === 'undefined') return;
	const pendingDocId = sessionStorage.getItem('waffle-active-doc');
	const pendingJson = sessionStorage.getItem('waffle-active-json');
	const pendingLink = sessionStorage.getItem('waffle-active-link');
	if (!pendingDocId || !pendingJson) return;

	sessionStorage.removeItem('waffle-active-doc');
	sessionStorage.removeItem('waffle-active-json');
	sessionStorage.removeItem('waffle-active-link');
	let link = null;
	if (pendingLink) {
		try { link = JSON.parse(pendingLink); } catch { link = null; }
	}

	// Wait for engine if not ready yet
	if (!engineReady) {
		await new Promise((resolve) => {
			const check = setInterval(() => {
				if (engineReady) { clearInterval(check); resolve(); }
			}, 100);
			setTimeout(() => { clearInterval(check); resolve(); }, 10000);
		});
	}

	try {
		await openDocumentRecord(pendingDocId, pendingJson, link);
	} catch (err) {
		log('error', `Failed to load pending document: ${err}`);
	}
}

/**
 * Open a stored document record in this tab: adopt its metadata and tabs, load
 * it into the engine, resolve its sources and evaluate an active assembly.
 * Shared by the `/doc/[id]` handoff and the agent link's document_open /
 * document_new.
 * @param {string} docId - the storage record id
 * @param {string} json - the record's `.waffle` text
 * @param {any} [link] - share-link provenance (read-only linked copies)
 * @throws when the engine does not load the file
 */
export async function openDocumentRecord(docId, json, link = null) {
	// A pending autosave belongs to the PREVIOUS document; firing after the
	// engine swaps trees would capture mixed state.
	cancelPendingAutoSave();
	const parsed = JSON.parse(json);
	// Set before the first await: a call arriving during the load is refused,
	// not answered from the half-swapped store.
	documentLoadPending = true;
	fitAllOnDocumentOpen();
	try {
		initDocumentState(docId, parsed, link);
		// Load the document into the engine — ALWAYS, even when the active tab
		// is empty: the engine owns the document's `sources` table (v4 §2.3),
		// which an empty tab can still belong to, and the Rust loader is the
		// one place migrations run.
		if (!(await loadProject(json, { silent: true }))) {
			throw new Error('the engine did not load the document (not ready, or saved by a newer version)');
		}
		// (The store used to take the engine's tab ids here, because it had
		// minted its own while parsing the same file. It no longer parses and
		// no longer mints: the load's `ModelUpdated` mirrors the session — S2
		// C4.)
		// The load's own ModelUpdated scheduled an autosave of what was just read
		// from storage; nothing is unsaved yet (agent link S3 reads the timer).
		// Source resolution below may still schedule a real one.
		cancelPendingAutoSave();
		log('system', `Loaded document ${docId}`);
		await resolveDocumentSources();
		if (activeAssemblyTab()) await refreshAssembly();
	} finally {
		documentLoadPending = false;
		// The restore offer is dropped once the document is actually OPEN, not
		// before the first await. `AutoRestoreDialog` renders on
		// `autoRestoreState`, so clearing it early dismissed the dialog while
		// the load was still in flight — and since the tab list became a
		// mirror (S2 C4) it is empty until the load's `ModelUpdated` arrives,
		// so the user saw an empty tab bar behind the vanished dialog. The
		// bootstrap-offer race this used to guard cannot happen: an explicit
		// handoff (`/doc/[id]`, `/open`) stops the offer being set at all
		// (`handoffPending`).
		autoRestoreState = null;
		// Settled only once the document is in the engine: the agent link
		// resumes on it, and resuming earlier let calls read the blank
		// bootstrap tree for the whole rebuild (failure log F10).
		settleStartupRestore();
	}
}

/**
 * Switch to a different tab. Saves current tab's feature tree, then loads the target tab.
 * @param {string} tabId
 */
export async function switchTab(tabId) {
	if (tabId === activeTabId) return;
	if (!documentTabs.find(t => t.id === tabId)) return;
	hideEntityCard();
	hideEntityDetail();

	// Cancel pending autosave to prevent stale state capture
	if (autoSaveTimer) {
		clearTimeout(autoSaveTimer);
		autoSaveTimer = null;
	}

	// (The outgoing tab's tree used to be copied into the store's tab here.
	// The session stashes it into the tab it belongs to — S2 C3a — and the
	// save composes from the session — C3c — so nothing reads that copy.)
	const targetTab = documentTabs.find(t => t.id === tabId);
	activeTabId = tabId;
	// New document context: its rebuild warnings are "new" again.
	lastRebuildWarnings = new Set();

	if (bridge && engineReady && targetTab?.kind?.type === 'Assembly') {
		await refreshAssembly();
	} else if (bridge && engineReady) {
		// The tree is NOT on the wire any more (S2 C3): the session holds every
		// tab's, stashes the live one into the tab being left and loads the
		// incoming tab's — which also carries the undo history with it.
		await sendRebuild({ type: 'SwitchTab', tab_id: tabId });
	}

	scheduleAutoSave();
}

// -- Assemblies (v4 Phase 3) --

/**
 * Evaluation result of the open Assembly tab (`ModelUpdated.assembly`):
 * `{ placements, errors, warnings, parts }`, or null while a Part tab is open.
 */
let assemblyStatus = $state(null);
export function getAssemblyStatus() { return assemblyStatus; }

/**
 * Every mate connector's evaluated frame in WORLD coordinates —
 * `{ id, kind, origin, x_axis, y_axis, z_axis }` — as the engine derived it
 * from the parts' current geometry. What the viewport draws, so a connector's
 * position and its z direction are visible instead of inferred from a `flip`
 * checkbox. Empty unless an Assembly tab is open.
 */
export function getAssemblyConnectorFrames() {
	return assemblyStatus?.connectors ?? [];
}

/**
 * The open Part's named mate connectors (`MateConnector` features) as the
 * engine evaluated them: `{feature_id, name, kind?, origin, x_axis, y_axis,
 * z_axis}` in part coordinates. Empty while an assembly is open.
 */
export function getPartConnectorFrames() {
	return partConnectors;
}

/**
 * In an open assembly, every rendered part instance's named connectors (each
 * with its `instance_path`, in world coordinates) — what an assembly
 * connector can be made from (`addConnector({ partConnector })`).
 */
export function getAssemblyPartConnectors() {
	return assemblyStatus?.part_connectors ?? [];
}

/**
 * Can this pick carry a mate connector? Asks the engine (which answers from
 * the already-evaluated assembly, no rebuild) BEFORE one is created — see
 * `specs/assembly_connector_frame_resolver.md` §2.4. Returns
 * `{ ok, kind, reason }`.
 */
export async function probeConnectorRef(instancePath, geomRef) {
	if (!bridge || !engineReady) return { ok: false, reason: 'the engine is not ready' };
	try {
		const r = await bridge.send({
			type: 'ProbeConnectorRef',
			instance_path: [...instancePath],
			geom_ref: JSON.parse(JSON.stringify(geomRef))
		});
		if (r?.type === 'ConnectorRefProbed') return { ok: !!r.ok, kind: r.kind ?? null, reason: r.reason ?? null };
		return { ok: false, reason: r?.message ?? 'the engine could not judge this pick' };
	} catch (err) {
		return { ok: false, reason: err?.message || String(err) };
	}
}

/** The active tab when it is an Assembly, else null. */
function activeAssemblyTab() {
	const tab = documentTabs.find(t => t.id === activeTabId);
	return tab?.kind?.type === 'Assembly' ? tab : null;
}

/** The open assembly's tree (instances, connectors, mates, placements), or null. */
export function getAssembly() {
	const tab = activeAssemblyTab();
	return tab ? tab.kind.assembly : null;
}

/**
 * Re-evaluate the open Assembly tab: hand the engine the assembly and the
 * tab's name (`OpenAssembly`); the session supplies the assembly and every
 * part tree it references (S2 C3b), and the engine
 * builds each part once, derives connector frames, solves placements and
 * renders the instance bodies.
 */
export async function refreshAssembly() {
	const tab = activeAssemblyTab();
	if (!tab || !bridge || !engineReady) return false;
	try {
		// The session holds the tab's assembly and every tree it references
		// (S2 C3b) — the store used to re-send every Part tree here, on every
		// evaluation, which was the largest payload on this wire.
		await sendRebuild({ type: 'OpenAssembly', tab_id: tab.id });
		refreshSourceTabs();
		return true;
	} catch (err) {
		log('error', `Assembly evaluation failed: ${err?.message || err}`);
		showToast('error', `Assembly evaluation failed: ${err?.message || err}`);
		return false;
	}
}

// -- In-context editing (v4 Phase 3d-4) --

/**
 * The context the open Part is being edited in (`ModelUpdated.context`):
 * `{ assembly_tab_id, instance_path, instance_name, placement, instances,
 * errors, warnings }`, or null.
 */
let editContext = $state(null);
export function getEditContext() { return editContext; }

/**
 * The `OpenPartInContext` payload for editing `partTabId` as the instance at
 * `instancePath` of the assembly tab `assemblyTabId` — three names and nothing
 * else (S2 C3b). The session holds the part's tree, the assembly and every
 * tree it references; switching to `partTabId` is what makes the part's tree
 * the live one being edited.
 */
function contextPayload(assemblyTabId, instancePath, partTabId) {
	const asmTab = documentTabs.find((t) => t.id === assemblyTabId);
	if (asmTab?.kind?.type !== 'Assembly') return null;
	const partTab = documentTabs.find((t) => t.id === partTabId);
	if (partTab?.kind?.type !== 'Part') return null;
	// Nothing but names (S2 C3b): the session holds the part's tree, the
	// assembly, and every tree it references — and the switch to `partTabId`
	// is what makes the part's tree live, so it is the tree being edited.
	return {
		type: 'OpenPartInContext',
		tab_id: partTabId,
		assembly_tab_id: assemblyTabId,
		instance_path: [...instancePath]
	};
}

/**
 * Which same-document Part tab the instance at `path` of the open assembly is
 * of, or null (a linked part, a sub-assembly, an unknown path).
 */
function partTabOfInstance(path) {
	if (!path?.length) return null;
	// The rendered leaf knows its part (also for a sub-assembly member).
	const key = JSON.stringify(path);
	const leaf = meshes.find((m) => JSON.stringify(m.instancePath ?? null) === key);
	if (leaf?.leafPartTabId) {
		return leaf.leafPartSourceId ? null : leaf.leafPartTabId;
	}
	if (path.length === 1) {
		const inst = getAssembly()?.instances.find((i) => i.id === path[0]);
		if (inst?.source && !inst.source.source_id) {
			const tab = documentTabs.find((t) => t.id === inst.source.tab_id);
			return tab?.kind?.type === 'Part' ? tab.id : null;
		}
	}
	return null;
}

/**
 * Edit the part of instance `instancePath` of the open Assembly tab in that
 * assembly's context: switch to the Part tab and open it with the other
 * instances as ghosts (`OpenPartInContext`). Sketching on a ghost face records
 * a scoped plane reference. Returns true when the part opened in context.
 * @param {string[]} instancePath
 */
export async function openPartInContext(instancePath) {
	const asmTab = activeAssemblyTab();
	if (!asmTab || !bridge || !engineReady) return false;
	const partTabId = partTabOfInstance(instancePath);
	if (!partTabId) {
		showToast('error', 'Only a part of this document can be edited in context (linked parts are read-only)');
		return false;
	}
	if (autoSaveTimer) {
		clearTimeout(autoSaveTimer);
		autoSaveTimer = null;
	}
	const payload = contextPayload(asmTab.id, instancePath, partTabId);
	if (!payload) return false;
	activeTabId = partTabId;
	lastRebuildWarnings = new Set();
	try {
		await sendRebuild(payload);
		log('action', 'Open part in context', { assembly: asmTab.id, instancePath, partTabId });
		scheduleAutoSave();
		return true;
	} catch (err) {
		// Fall back to a plain open of the part so the user is not stranded.
		log('error', `Open in context failed: ${err?.message || err}`);
		showToast('error', `Could not open in context: ${err?.message || err}`);
		await sendRebuild({ type: 'SwitchTab', tab_id: partTabId }).catch(() => {});
		scheduleAutoSave();
		return false;
	}
}

/**
 * Re-take the context snapshot from the assembly's current state (its other
 * parts and placements may have changed): scoped sketch planes re-derive.
 */
export async function updateEditContext() {
	if (!editContext || !bridge || !engineReady) return false;
	const payload = contextPayload(editContext.assembly_tab_id, editContext.instance_path, activeTabId);
	if (!payload) return false;
	try {
		await sendRebuild(payload);
		return true;
	} catch (err) {
		showToast('error', `Could not update the context: ${err?.message || err}`);
		return false;
	}
}

/** Leave the context: the part stays open on its own (ghosts gone). */
export async function exitEditContext() {
	if (!editContext || !bridge || !engineReady) return false;
	// Switching to the tab that is already active: the session stashes the live
	// tree into it and loads it straight back, so the part stays exactly as it
	// is — only the ghosts go.
	await sendRebuild({ type: 'SwitchTab', tab_id: activeTabId });
	return true;
}

/**
 * Tabs of each available linked `.waffle` source (`ListSourceTabs`), for
 * the "add instance" chooser: `{ [source_id]: [{id, name, kind}] }`.
 */
let sourceTabs = $state({});
export function getSourceTabs() { return sourceTabs; }

async function refreshSourceTabs() {
	if (!bridge || !engineReady) return;
	const next = {};
	for (const s of documentSources) {
		if (s.kind !== 'Waffle' || !s.available) continue;
		try {
			const r = await bridge.send({ type: 'ListSourceTabs', source_id: s.id });
			if (r?.type === 'SourceTabsListed') next[s.id] = r.tabs;
		} catch (err) {
			log('warn', `ListSourceTabs ${s.name}: ${err?.message || err}`);
		}
	}
	sourceTabs = next;
}

/**
 * Why the last connector pick was refused (`null` once one succeeds) — shown
 * in the Assembly panel next to the pick button.
 */
let lastConnectorRefusal = $state(null);
export function getConnectorRefusal() { return lastConnectorRefusal; }

/**
 * Mutate the open assembly's tree, hand it to the session, then autosave.
 *
 * The session is the authority for an Assembly tab's content (S2 C3b), so the
 * edit has to REACH it: `OpenAssembly` no longer carries the assembly, and a
 * panel edit that stayed in the store's tab copy would be evaluated from the
 * session's stale one. `EditAssembly` re-evaluates the tab when it is the one
 * on screen, so this replaces the `refreshAssembly()` that used to follow.
 */
async function editAssembly(fn) {
	const tab = activeAssemblyTab();
	if (!tab) return null;
	const result = fn(tab.kind.assembly);
	if (bridge && engineReady) {
		// Placements are derived; the engine recomputes them.
		const assembly = JSON.parse(JSON.stringify(tab.kind.assembly));
		delete assembly.placements;
		try {
			await sendRebuild({ type: 'EditAssembly', tab_id: tab.id, assembly });
		} catch (err) {
			log('error', `EditAssembly failed: ${err?.message || err}`);
			// During an agent call the failure is the agent's to report (the
			// executor turns it into a typed result); a swallowed null would
			// read as "nothing happened" over the link.
			if (agentActivity) throw err;
			showToast('error', `The assembly edit failed: ${err?.message || err}`);
			return null;
		}
	}
	scheduleAutoSave();
	return result;
}

/**
 * Add an instance of a part: a Part tab of this document (`tabId`) or a
 * tab of a linked `.waffle` source (`sourceId` + `tabId`).
 * @param {{tabId: string, sourceId?: string|null, name?: string, transform?: object, fixed?: boolean}} opts
 * @returns {Promise<string|null>} the instance id
 */
export async function addInstance({ tabId, sourceId = null, name, transform, fixed = false }) {
	return editAssembly((asm) => {
		const id = generateUUID();
		const partTab = sourceId
			? (sourceTabs[sourceId] ?? []).find(t => t.id === tabId)
			: documentTabs.find(t => t.id === tabId);
		const count = asm.instances.filter(i => i.source.tab_id === tabId && (i.source.source_id ?? null) === sourceId).length;
		asm.instances.push({
			id,
			name: name || `${partTab?.name ?? 'Part'} ${count + 1}`,
			source: sourceId ? { source_id: sourceId, tab_id: tabId } : { tab_id: tabId },
			transform: transform || { translation_m: [0, 0, 0], rotation_quat: [0, 0, 0, 1] },
			...(fixed ? { fixed: true } : {})
		});
		return id;
	});
}

/** Patch an instance (`name`, `transform`, `fixed`, `suppressed`). */
export async function updateInstance(instanceId, patch) {
	return editAssembly((asm) => {
		const inst = asm.instances.find(i => i.id === instanceId);
		if (!inst) return false;
		for (const k of ['name', 'transform', 'fixed', 'suppressed', 'external_key']) {
			if (k in patch) inst[k] = JSON.parse(JSON.stringify(patch[k]));
		}
		return true;
	});
}

/** Remove an instance and everything that references it. */
export async function removeInstance(instanceId) {
	return editAssembly((asm) => {
		asm.instances = asm.instances.filter(i => i.id !== instanceId);
		const gone = new Set((asm.connectors ?? []).filter(c => c.instance_path?.[0] === instanceId).map(c => c.id));
		asm.connectors = (asm.connectors ?? []).filter(c => !gone.has(c.id));
		asm.mates = (asm.mates ?? []).filter(m => !m.connectors.some(c => gone.has(c)));
		if (asm.placements) delete asm.placements[instanceId];
		return true;
	});
}

/**
 * Add a mate connector on an instance: one of the part's named connectors
 * (`partConnector`, the id of its `MateConnector` feature — already judged
 * by the part's rebuild), a face or edge of the part (`geomRef`, in the
 * PART's feature space — the frame is derived from the geometry at
 * evaluation), or an explicit `frame`.
 * @returns {Promise<string|null>} the connector id
 */
/** Thrown to an agent call when the engine cannot derive a connector frame from a pick. */
export class ConnectorRefused extends Error {
	constructor(reason) {
		super(reason);
		this.name = 'ConnectorRefused';
		this.reason = reason;
	}
}

export async function addConnector({ instanceId = null, instancePath = null, geomRef = null, partConnector = null, frame = null, name }) {
	const path = instancePath?.length ? [...instancePath] : [instanceId];
	// Judge the pick BEFORE minting a connector: a reference the engine cannot
	// derive a frame from used to be accepted here and silently fall back to a
	// default frame at solve time, placing the part against geometry the user
	// never picked (`specs/assembly_connector_frame_resolver.md` §1).
	if (geomRef && !partConnector) {
		const probe = await probeConnectorRef(path, geomRef);
		if (!probe.ok) {
			const reason = probe.reason || 'this geometry cannot define a connector frame';
			lastConnectorRefusal = reason;
			log('warn', `Connector refused: ${reason}`);
			// Over the agent link the refusal is a typed result, not a toast.
			if (agentActivity) throw new ConnectorRefused(reason);
			showToast('error', `Cannot put a connector here — ${reason}`);
			return null;
		}
		lastConnectorRefusal = null;
	}
	return editAssembly((asm) => {
		const id = generateUUID();
		asm.connectors = asm.connectors ?? [];
		const inst = asm.instances.find(i => i.id === path[0]);
		const owner = `${inst?.name ?? 'Instance'}${path.length > 1 ? ' › member' : ''}`;
		const named = partConnector
			? getAssemblyPartConnectors().find(p => p.feature_id === partConnector && p.instance_path.join() === path.join())
			: null;
		asm.connectors.push({
			id,
			name: name || (named ? `${owner} › ${named.name}` : `${owner} connector ${asm.connectors.length + 1}`),
			instance_path: path,
			...(partConnector
				? { part_connector: partConnector }
				: geomRef ? { geom_ref: JSON.parse(JSON.stringify(geomRef)) } : {}),
			frame: frame ? JSON.parse(JSON.stringify(frame)) : { origin: [0, 0, 0], z_axis: [0, 0, 1], x_axis: [0, 0, 0] }
		});
		return id;
	});
}

/**
 * Where on a rotational face's axis a connector's frame sits: the middle of
 * the face's extent, or the end its z axis points toward / away from. The
 * ends are named by the connector's FINAL z (after `flipZ`), i.e. against
 * the triad the viewport draws.
 */
export const CONNECTOR_ANCHORS = ['middle', 'positive_end', 'negative_end'];

/**
 * Edit a mate connector after creation (`specs/assembly_connector_adjustments.md`):
 * `name`; `anchor` (one of `CONNECTOR_ANCHORS`, for a connector on a
 * cylindrical/conical/toroidal face); `flipZ` (reverse z — a 180° turn
 * about x); `rotationDeg` (turn about z, after the flip); `offsetMm`
 * (`[x, y, z]` along the connector's OWN axes after the turn). Each
 * adjustment at its default is removed from the stored connector, so an
 * unadjusted connector is written exactly as before these fields existed.
 */
export async function updateConnector(connectorId, patch) {
	return editAssembly((asm) => {
		const c = (asm.connectors ?? []).find(x => x.id === connectorId);
		if (!c) return false;
		if ('name' in patch) c.name = String(patch.name ?? '') || c.name;
		if ('anchor' in patch) {
			if (CONNECTOR_ANCHORS.includes(patch.anchor) && patch.anchor !== 'middle') c.anchor = patch.anchor;
			else delete c.anchor;
		}
		if ('flipZ' in patch) {
			if (patch.flipZ) c.flip_z = true;
			else delete c.flip_z;
		}
		if ('rotationDeg' in patch) {
			const r = Number(patch.rotationDeg) || 0;
			if (r) c.rotation_deg = r;
			else delete c.rotation_deg;
		}
		if ('offsetMm' in patch || 'offsetM' in patch) {
			// Stored in meters, like every length in the document. `offsetM`
			// (the agent link, which speaks meters) is taken as is — no unit
			// round trip.
			const o = 'offsetM' in patch
				? [0, 1, 2].map(k => Number(patch.offsetM?.[k]) || 0)
				: [0, 1, 2].map(k => (Number(patch.offsetMm?.[k]) || 0) / 1000);
			if (o.some(v => v !== 0)) c.offset_m = o;
			else delete c.offset_m;
		}
		return true;
	});
}

export async function removeConnector(connectorId) {
	return editAssembly((asm) => {
		asm.connectors = (asm.connectors ?? []).filter(c => c.id !== connectorId);
		asm.mates = (asm.mates ?? []).filter(m => !m.connectors.includes(connectorId));
		return true;
	});
}

/**
 * Fasten two connectors: `b`'s instance is placed so its frame coincides
 * with `a`'s (rotated by `rotationDeg` about z; `flip` opposes the z axes —
 * two outward face normals "stacked").
 * @returns {Promise<string|null>} the mate id
 */
/** Mate kinds this build solves; `Fastened` exactly, the others numerically. */
export const MATE_KINDS = ['Fastened', 'Revolute', 'Slider', 'Cylindrical', 'Planar', 'Ball'];

function mateKind(kind, flip, rotationDeg) {
	const k = { type: MATE_KINDS.includes(kind) ? kind : 'Fastened' };
	if (k.type !== 'Ball' && flip) k.flip = true;
	if (k.type === 'Fastened' && rotationDeg) k.rotation_deg = rotationDeg;
	return k;
}

export async function addMate({ a, b, kind = 'Fastened', flip = true, rotationDeg = 0, name }) {
	return editAssembly((asm) => {
		const id = generateUUID();
		asm.mates = asm.mates ?? [];
		const k = mateKind(kind, flip, rotationDeg);
		asm.mates.push({ id, name: name || `${k.type} ${asm.mates.length + 1}`, kind: k, connectors: [a, b] });
		return id;
	});
}

export async function updateMate(mateId, patch) {
	return editAssembly((asm) => {
		const m = (asm.mates ?? []).find(x => x.id === mateId);
		if (!m) return false;
		if ('name' in patch) m.name = patch.name;
		if ('suppressed' in patch) m.suppressed = !!patch.suppressed;
		if ('kind' in patch || 'flip' in patch || 'rotationDeg' in patch) {
			const type = 'kind' in patch ? patch.kind : m.kind?.type;
			const flip = 'flip' in patch ? !!patch.flip : !!m.kind?.flip;
			const rot = 'rotationDeg' in patch ? Number(patch.rotationDeg) || 0 : (m.kind?.rotation_deg ?? 0);
			m.kind = mateKind(type, flip, rot);
		}
		return true;
	});
}

export async function removeMate(mateId) {
	return editAssembly((asm) => {
		asm.mates = (asm.mates ?? []).filter(m => m.id !== mateId);
		return true;
	});
}

/**
 * The instance whose body was last clicked in the viewport (assembly mode):
 * its full path (`[instance, member, …]` through sub-assemblies).
 */
let selectedInstancePath = $state(null);
export function getSelectedInstanceId() { return selectedInstancePath?.[0] ?? null; }
export function getSelectedInstancePath() { return selectedInstancePath; }
export function setSelectedInstancePath(path) { selectedInstancePath = path?.length ? [...path] : null; }

/**
 * Add a new tab to the document. The ENGINE mints the id and the name (S2 C3):
 * the session owns the tab bar, so a tab the store invented on its own would be
 * a tab the engine cannot be asked to switch to.
 * @returns {Promise<string | null>} the new tab's id, or null when the engine
 *   refused (or is not running — a tab with no engine behind it is not one).
 */
export async function addTab(kind = 'Part') {
	if (!bridge || !engineReady) return null;
	let response;
	try {
		response = await bridge.send({ type: 'AddTab', kind });
	} catch (err) {
		log('error', `AddTab failed: ${err?.message || err}`);
		return null;
	}
	const tabs = response?.document?.tabs;
	if (response?.type !== 'ModelUpdated' || !tabs?.length) {
		log('error', `AddTab refused: ${response?.message || 'the engine added no tab'}`);
		return null;
	}
	// Appended last, by the message's contract. The tab itself is NOT added to
	// the store here: the answer's `ModelUpdated` mirrors the session's list
	// (S2 C4), and appending as well would put the same tab in twice — a
	// duplicate key in the tab bar's keyed each block.
	const added = tabs[tabs.length - 1];
	scheduleAutoSave();
	return added.id;
}

/**
 * Close a tab by ID. If the active tab is closed, switch to an adjacent tab.
 * @param {string} tabId
 */
export async function closeTab(tabId) {
	if (documentTabs.length <= 1) return; // Don't close the last tab

	const idx = documentTabs.findIndex(t => t.id === tabId);
	if (idx === -1) return;

	// The session closes the tab and, when it was the active one, makes its
	// successor active and rebuilds it — one message, so the store never holds
	// a tab list the engine has already moved past.
	let response = null;
	if (bridge && engineReady) {
		try {
			response = await bridge.send({ type: 'CloseTab', tab_id: tabId });
		} catch (err) {
			log('error', `CloseTab failed: ${err?.message || err}`);
			return;
		}
		if (response?.type !== 'ModelUpdated') {
			log('error', `CloseTab refused: ${response?.message || 'the engine kept the tab'}`);
			return;
		}
	}

	// The tab list and the active tab are the mirror's to write (S2 C4): the
	// answer above already carried the session's list without this tab, and
	// the successor it chose.
	if (activeTabId === tabId) lastRebuildWarnings = new Set();

	scheduleAutoSave();
}

/**
 * Rename a tab.
 * @param {string} tabId
 * @param {string} name
 */
export async function renameTab(tabId, name) {
	if (bridge && engineReady) {
		try {
			await bridge.send({ type: 'RenameTab', tab_id: tabId, name });
		} catch (err) {
			log('error', `RenameTab failed: ${err?.message || err}`);
			return;
		}
	}
	// The rename reaches the store through the answer's mirror (S2 C4).
	scheduleAutoSave();
}

/**
 * Move a tab to `index` in the tab order, clamped to the ends. The order is
 * the document's `tabs` array, so it is saved with the document.
 * @param {string} tabId
 * @param {number} index
 * @returns {boolean} whether the order changed
 */
export async function moveTab(tabId, index) {
	const from = documentTabs.findIndex(t => t.id === tabId);
	if (from === -1) return false;
	const to = Math.max(0, Math.min(documentTabs.length - 1, Math.trunc(index)));
	if (to === from) return false;
	if (bridge && engineReady) {
		try {
			await bridge.send({ type: 'MoveTab', tab_id: tabId, index: to });
		} catch (err) {
			log('error', `MoveTab failed: ${err?.message || err}`);
			return false;
		}
	}
	// The new order arrives with the answer (S2 C4); reordering here too would
	// apply the move twice, from a list the engine has already moved past.
	scheduleAutoSave();
	return true;
}

/**
 * The session's tab list as of the last `ModelUpdated` (S2 C4). `{id, name,
 * kind}` per tab; never a tree.
 * @type {any}
 */
let sessionDocument = null;

/**
 * Mirror the session's document into the store's `$state` (S2 C4, A2.1).
 *
 * The engine is authoritative for the tab bar, the active tab and the
 * document's name and display unit; the store renders them. Keeping a second
 * copy that the store also writes is what A2.1 forbids, and what made C3a's
 * tab ids diverge in the first place.
 *
 * Per-tab CONTENT is not on the wire and is not mirrored: `kind.assembly` is
 * still the store's until C4b, so each tab's existing content is carried over
 * by id rather than dropped. `id` and `created` stay the HOST's to latch
 * (the storage record is keyed by the identity, v4 P2-5) — they are adopted
 * from a load, not owned by the engine.
 * @param {any} info - `ModelUpdated.document`
 */
function mirrorSessionDocument(info) {
	if (!info?.tabs) return;
	sessionDocument = info;
	const byId = new Map(documentTabs.map((t) => [t.id, t]));
	documentTabs = info.tabs.map((t) => {
		const existing = byId.get(t.id);
		// The OPEN Assembly tab's tree comes with the message (S2 C4b) — it is
		// what the panel reads and what `editAssembly` mutates. Every other
		// tab's content is not display data and is not on the wire: the
		// session holds it, and supplies it to the messages that need it.
		const kind = t.id === info.active_tab && t.kind === 'Assembly' && info.assembly_tree
			? { type: 'Assembly', assembly: info.assembly_tree }
			: existing?.kind?.type === t.kind
				? existing.kind
				: t.kind === 'Assembly'
					? { type: 'Assembly', assembly: { instances: [], connectors: [], mates: [] } }
					: { type: t.kind, features: { features: [], active_index: null } };
		return { ...(existing ?? {}), id: t.id, name: t.name, kind };
	});
	activeTabId = info.active_tab;
	documentName = info.name;
	projectName = info.name;
	// Unconditional: the session reports no unit for a legacy file, and that
	// means mm — not "keep whatever the previous document used". (This used to
	// lean on a second `.waffle` parser in the load path to do the reset.)
	documentDisplayUnit = info.display_unit ?? 'mm';
	// `created` is NOT mirrored. It is the host's to latch (C3c) and the
	// engine only echoes it back — through a serializer that drops the
	// milliseconds, so mirroring it rewrites "…:05.000Z" as "…:05Z" and the
	// stored document's own timestamp changes shape on open. The store keeps
	// the value it read from the file.
}

/**
 * Adopt the storage identity of a document being opened.
 * Called when loading a document from IndexedDB or creating a new one.
 * @param {string} docId
 * @param {object} parsed - Parsed v3 JSON
 * @param {import('$lib/storage/open-link.js').DocumentLink | null} [link] -
 *   share-link provenance; non-null makes the document read-only here.
 */
export function initDocumentState(docId, parsed, link = null) {
	// What is left here is the state the ENGINE does not have: which storage
	// record this tab is filed under, and where the document was linked from.
	//
	// The name, the display unit, the tab list and the active tab used to be
	// parsed out of the `.waffle` right here — a second reader of the format,
	// beside the Rust loader, whose tab ids had to be rewritten "exactly as
	// the Rust v3→v4 migration does" to keep the two agreeing. They never
	// quite did (see C3a). `LoadProject` now tells the store all of it through
	// `ModelUpdated.document` (S2 C4, A2.1), so the parser is gone.
	activeDocId = docId;
	documentLink = link && link.readOnly ? link : null;
	// v4 identity stays the HOST's (v4 P2-5: the storage record is keyed by
	// it, and C3c pushes it down to the session): adopt the file's id, or mint
	// one for a legacy file that has none. `created` arrives with the load.
	documentId = isUuid(parsed.document?.id)
		? parsed.document.id
		: isUuid(docId) ? docId : generateUUID();
	// `created` is host state too, and it is read here rather than mirrored:
	// the engine echoes it through a serializer that drops the milliseconds,
	// so taking it back from `ModelUpdated` would rewrite the stored
	// document's own "…:05.000Z" as "…:05Z" on every open. Reading one field
	// is not the second format parser C4 deleted — that was the name, the
	// unit, the tab list and the active tab, all of which the engine now
	// reports.
	documentCreated = parsed.document?.created || parsed.project?.created || null;
	// New document context: its rebuild warnings are "new" again.
	lastRebuildWarnings = new Set();
}

/**
 * Build the full `.waffle` (v4) document for saving — through the ONE writer.
 * The UI owns the document metadata and the tab list (inactive tabs carry
 * their trees; the active tab's tree lives in the engine); the engine owns the
 * `sources` table and composes + verifies the file (`SaveDocument` →
 * `SaveReady`). No envelope is assembled in JavaScript
 * (specs/waffle_v4_document_model.md §4 invariant 7).
 * @returns {Promise<string | null>} the file text, or null when the engine
 *   refused (the last good stored copy is then left untouched).
 */
export async function buildDocumentJson({ via = 'user' } = {}) {
	if (!bridge || !engineReady) return null;
	// `via: 'agent'` composes through the agent's own entry point (the caller
	// holds the engine lock as 'agent'): the two sends are then recorded with
	// the agent's origin, so an agent step that stores itself leaves no
	// user-origin send in the engine log (parity oracle O3/O7).
	const sendMessage = via === 'agent' ? (m) => sendAgentMessage(m) : (m) => bridge.send(m);

	// Latch identity + creation time on first save so they stay stable. These
	// stay the STORE's to mint: the storage record is keyed by the document's
	// own identity (v4 P2-5), so the id has to be the one this tab already
	// filed under — the session takes them rather than inventing its own.
	if (!documentCreated) documentCreated = new Date().toISOString();
	if (!documentId) documentId = generateUUID();
	// One sync point, immediately before the save: the session composes the
	// file now (S2 C3c), so anything the store changed without telling it —
	// a rename, a freshly latched identity — would otherwise be written stale.
	//
	// Its answer is a ModelUpdated, and EVERY ModelUpdated schedules an
	// autosave — so syncing here would leave a timer armed and the document
	// looking permanently unsaved (`hasPendingAutoSave`), which the agent link
	// then refuses calls over. Saving must not itself dirty the document, so
	// the timer is put back exactly as it was: a real edit waiting for its
	// autosave still gets one, and a save on a clean document leaves it clean.
	const wasPending = hasPendingAutoSave();
	try {
		await sendMessage({
			type: 'SetDocumentMeta',
			id: documentId,
			name: documentName,
			created: documentCreated,
			display_unit: documentDisplayUnit
		});
	} catch (err) {
		log('error', `SetDocumentMeta failed: ${err?.message || err}`);
		return null;
	}
	if (!wasPending) cancelPendingAutoSave();

	let response;
	try {
		// No payload: metadata, every tab with its tree and thumbnail, and
		// which one is active all live in the session (v4 §4 inv. 7).
		response = await sendMessage({ type: 'SaveDocument' });
	} catch (err) {
		log('error', `SaveDocument failed: ${err?.message || err}`);
		return null;
	}
	if (response?.type !== 'SaveReady' || !response.json_data) {
		log('error', `SaveDocument refused: ${response?.message || 'engine returned no document'}`);
		return null;
	}
	return response.json_data;
}

// -- Visibility toggles (toolbar compat — delegates to per-item visibility) --

/**
 * Check if any datum plane is visible (for toolbar active state).
 * @returns {boolean}
 */
export function getShowDatumPlanes() {
	return BUILTIN_PLANES.some(p => isPlaneVisible(p.id));
}

/**
 * Check if any origin axis is visible (for toolbar active state).
 * @returns {boolean}
 */
export function getShowOriginTriad() {
	return isAxisVisible('x') || isAxisVisible('y') || isAxisVisible('z');
}

/**
 * Toggle all datum planes visibility (toolbar button).
 * If any visible → hide all; if none visible → show all.
 */
export function toggleDatumPlanes() {
	if (getShowDatumPlanes()) {
		hideAllPlanes(BUILTIN_PLANES);
	} else {
		showAllPlanes(BUILTIN_PLANES);
	}
}

/**
 * Toggle all origin axes visibility (toolbar button).
 * If any visible → hide all; if none visible → show all.
 */
export function toggleOriginTriad() {
	if (getShowOriginTriad()) {
		hideAllAxes();
	} else {
		showAllAxes();
	}
}

// -- Auto-restore --

export function getAutoRestoreState() { return autoRestoreState; }

/**
 * What a reload may bring back, best first: the legacy localStorage autosave;
 * this tab's own draft; else the newer of the newest draft (any tab) and the
 * newest stored local document.
 */
async function findStartupRestore() {
	if (typeof localStorage !== 'undefined') {
		const saved = localStorage.getItem(AUTOSAVE_KEY);
		const savedTime = localStorage.getItem(AUTOSAVE_TIME_KEY);
		if (saved && savedTime) {
			return { available: true, timestamp: parseInt(savedTime, 10), source: /** @type {const} */ ('legacy') };
		}
	}
	/** @param {import('$lib/storage/drafts.js').Draft} d */
	const draftOffer = (d) => ({
		available: true,
		timestamp: d.modified,
		source: /** @type {const} */ ('draft'),
		docId: d.docId,
		draftKey: d.tabKey,
		name: d.name
	});
	let best = null;
	try {
		const key = tabKey();
		const own = key ? await getDraft(key) : null;
		if (own) return draftOffer(own);
		const [newestDraft] = await listDrafts();
		if (newestDraft) best = draftOffer(newestDraft);
	} catch {
		// drafts unavailable
	}
	try {
		const { getStore } = await import('$lib/storage/index.js');
		const [newest] = await getStore().list();
		if (newest && (!best || newest.modified > best.timestamp)) {
			best = { available: true, timestamp: newest.modified, source: /** @type {const} */ ('indexeddb'), docId: newest.id, name: newest.name };
		}
	} catch {
		// IndexedDB not available
	}
	return best;
}

export async function restoreAutoSave() {
	const offer = autoRestoreState;
	try {
		if (offer?.source === 'draft' && offer.draftKey) {
			const draft = await getDraft(offer.draftKey);
			if (!draft?.json) return false;
			await openDocumentRecord(draft.docId, draft.json);
			await restoreSketchSession(draft.sketch);
			// A draft can be newer than the provider's copy (flushed on hide, or
			// the provider was unreachable): store it again.
			scheduleAutoSave();
			return true;
		}
		if (offer?.source === 'indexeddb' && offer.docId) {
			const { getStore } = await import('$lib/storage/index.js');
			const doc = await getStore().get(offer.docId);
			if (!doc?.json) return false;
			// The whole record — tabs, identity, name — not just the active
			// tab's tree: a bare loadProject left the bootstrap's one-tab
			// state and a fresh document.id in place, and the next autosave
			// wrote that over the stored document.
			await openDocumentRecord(doc.id, doc.json, doc.link ?? null);
			return true;
		}
		// Legacy localStorage restore
		if (typeof localStorage === 'undefined') return false;
		const saved = localStorage.getItem(AUTOSAVE_KEY);
		if (!saved) return false;
		const savedName = localStorage.getItem(AUTOSAVE_NAME_KEY);
		if (savedName) projectName = savedName;
		await loadProject(saved);
		return true;
	} catch (err) {
		log('error', `Restore failed: ${err?.message || err}`);
		return false;
	} finally {
		autoRestoreState = null;
		settleStartupRestore();
	}
}

export async function discardAutoSave() {
	const offer = autoRestoreState;
	// A document offer points at the stored DOCUMENT (autosave writes the
	// record itself), so Discard only dismisses it. A draft is a copy: this
	// tab's own is dropped; another live tab's is left alone.
	if (offer?.source === 'draft' && offer.draftKey && offer.draftKey === tabKey()) {
		try {
			await deleteDraft(offer.draftKey);
		} catch {
			// drafts unavailable
		}
	}
	if (typeof localStorage !== 'undefined') {
		localStorage.removeItem(AUTOSAVE_KEY);
		localStorage.removeItem(AUTOSAVE_TIME_KEY);
		localStorage.removeItem(AUTOSAVE_NAME_KEY);
	}
	autoRestoreState = null;
	settleStartupRestore();
}

/**
 * Trigger a constraint solve via the Rust solver (Levenberg-Marquardt) in the
 * WASM engine. Sends SolveSketch command through the bridge.
 */
export function triggerSolve() {
	if (!bridge || !engineReady) return;
	if (!sketchMode.active) return;
	if (sketchEntities.length === 0) return;

	// Re-sync the live sketch state before solving. The engine's per-item
	// AddSketchEntity / AddConstraint paths are append-only — they keep the
	// ORIGINAL drawn positions and cannot express a removal or a REFERENCE
	// (driven) dimension toggle. Replacing both lists makes the solver a pure
	// function of the UI's live geometry: point entities carry their current
	// (last-solved / dragged) position, and reference dims are excluded (they
	// display a measured value but must NOT constrain).
	// Deep-clone (JSON) to strip Svelte 5 reactive proxies — they cannot be
	// structured-cloned across the Worker boundary (DataCloneError), the same
	// reason AddSketchEntity/AddConstraint clone before sending.
	const entities = JSON.parse(
		JSON.stringify(
			sketchEntities.map((e) => {
				if (e.type === 'Point' && e.id != null) {
					const p = sketchPositions.get(e.id);
					if (p) return { ...e, x: p.x, y: p.y };
				}
				return e;
			})
		)
	);
	const driving = JSON.parse(
		JSON.stringify(
			sketchConstraints
				.filter((c) => !c.reference)
				.map((c) => mapConstraintForBridge(c))
				.filter(Boolean)
		)
	);
	// One atomic message: replace the engine's sketch state with the UI's live
	// geometry + driving constraints, then solve. Avoids the multi-round-trip
	// race/latency of separate sync messages.
	bridge.send({ type: 'SolveSketch', entities, constraints: driving })
		.catch((err) => log('error', `SolveSketch failed: ${err}`));
}

const AUTOSAVE_KEY = 'waffle-autosave';
const AUTOSAVE_TIME_KEY = 'waffle-autosave-time';
const AUTOSAVE_NAME_KEY = 'waffle-autosave-name';
const AUTOSAVE_DELAY_MS = 3000;

function scheduleAutoSave() {
	if (autoSaveTimer) clearTimeout(autoSaveTimer);
	// A linked document is read-only: never write it back to storage (the
	// linked record must keep the bytes fetched at `resolved.commit`).
	if (documentLink?.readOnly) return;
	autoSaveTimer = setTimeout(() => {
		autoSaveTimer = null;
		autosaveNow();
	}, AUTOSAVE_DELAY_MS);
}

/**
 * Run a pending autosave now instead of after its delay. Called when the tab
 * is hidden or unloaded: iOS may kill a backgrounded tab without another
 * event, so this is the last reliable moment to store the latest edit.
 */
export function flushAutoSave() {
	// An open sketch changes without scheduling an autosave (a drag, a
	// solve), so its session is stored on every hide.
	if (!autoSaveTimer && !sketchMode.active) return;
	if (autoSaveTimer) clearTimeout(autoSaveTimer);
	autoSaveTimer = null;
	autosaveNow();
}

/**
 * The open sketch session as plain data, for this tab's draft: what
 * `enterSketchEditMode` would load, plus the unfinished edits. Null when no
 * sketch is open.
 */
function sketchSessionSnapshot() {
	if (!sketchMode.active) return null;
	return JSON.parse(JSON.stringify({
		tabId: activeTabId,
		editingFeatureId: editingSketchFeatureId,
		origin: sketchMode.origin,
		normal: sketchMode.normal,
		entities: sketchEntities,
		// Transient drag pins belong to a pointer gesture, not the sketch.
		constraints: sketchConstraints.filter((c) => !c._isDrag),
		projectedBindings,
		positions: [...sketchPositions].map(([id, p]) => [id, p.x, p.y])
	}));
}

/**
 * Re-enter a sketch session saved by `sketchSessionSnapshot`, after its
 * document was reopened: the same entry path the user took (edit of an
 * existing sketch feature, else a new sketch on the plane), then the
 * unfinished geometry, re-synced to the engine by one solve.
 * @param {any} session
 */
async function restoreSketchSession(session) {
	if (!session || session.tabId !== activeTabId) return;
	const editing = session.editingFeatureId
		&& featureTree.features.some((f) => f.id === session.editingFeatureId);
	if (editing) await enterSketchEditMode(session.editingFeatureId);
	else await enterSketchMode(session.origin, session.normal);
	if (!sketchMode.active) return;

	sketchEntities = session.entities ?? [];
	sketchConstraints = session.constraints ?? [];
	projectedBindings = session.projectedBindings ?? [];
	sketchPositions = new Map((session.positions ?? []).map(([id, x, y]) => [id, { x, y }]));
	nextEntityId = sketchEntities.reduce((max, e) => Math.max(max, e.id), 0) + 1;
	await rebuildGearsFromEntities();
	reExtractProfiles();
	// SolveSketch replaces the engine's sketch state with these lists.
	triggerSolve();
}

function installAutosaveLifecycle() {
	if (autosaveLifecycleInstalled || typeof document === 'undefined') return;
	autosaveLifecycleInstalled = true;
	document.addEventListener('visibilitychange', () => {
		if (document.visibilityState === 'hidden') {
			// The OS may discard the tab from here without another event; a
			// reloaded tab that lost its sessionStorage finds its draft by this.
			rememberTabKey();
			flushAutoSave();
		}
	});
	window.addEventListener('pagehide', () => {
		rememberTabKey();
		flushAutoSave();
	});
}

/**
 * Store a pending autosave NOW and wait for it, so that whoever asked can
 * answer "stored" truthfully — the agent link does this after every mutating
 * tool, as the native host rewrites its record after every one
 * (specs/waffle_server_mode.md §3.5): a tab the OS discards a moment later
 * loses nothing that was already answered. Nothing pending: resolves true at
 * once. Resolves false when neither the draft nor a storage record took it.
 * `via: 'agent'` composes through the agent entry point (see
 * `buildDocumentJson`); the caller then holds the engine lock as 'agent'.
 * @param {{ via?: 'user' | 'agent' }} [opts]
 * @returns {Promise<boolean>}
 */
export async function commitPendingAutoSave({ via = 'user' } = {}) {
	if (!autoSaveTimer) return true;
	clearTimeout(autoSaveTimer);
	autoSaveTimer = null;
	return autosaveNow({ via });
}

/**
 * Compose the document once; store it as this tab's draft and in the active
 * provider. Resolves true when at least one of them took it.
 * @param {{ via?: 'user' | 'agent' }} [opts]
 * @returns {Promise<boolean>}
 */
async function autosaveNow({ via = 'user' } = {}) {
	if (!activeDocId) return false;
	const docId = activeDocId;
	const jsonData = await buildDocumentJson({ via });
	if (!jsonData) return false;
	let stored = await saveDraft(docId, jsonData);
	try {
		const { getActiveProvider } = await import('$lib/storage/index.js');
		await putRecord(getActiveProvider(), docId, jsonData);
		stored = true;
	} catch (err) {
		// If remote provider fails, fall back to local IndexedDB
		console.warn('Auto-save to provider failed, falling back to local:', err.message || err);
		try {
			const { getStore } = await import('$lib/storage/index.js');
			await putRecord(getStore(), docId, jsonData);
			stored = true;
		} catch {
			console.warn('Local fallback auto-save also failed');
		}
	}
	return stored;
}

/**
 * This tab's draft (`$lib/storage/drafts.js`); a failure never blocks the real save.
 * @returns {Promise<boolean>} whether the draft was written
 */
async function saveDraft(docId, jsonData) {
	try {
		await putDraft({ docId, name: documentName, json: jsonData, sketch: sketchSessionSnapshot() });
		return true;
	} catch (err) {
		console.warn('Draft save failed:', err?.message || err);
		return false;
	}
}

/**
 * Reopen a document in this tab by its storage id, from the newest draft that
 * holds it (any tab of this browser — the draft can be newer than the stored
 * record) or else from the active provider, then the local store. The agent
 * link uses it after a reload to land on the document it was working on,
 * whatever the tab's restore policy did. Resolves false when nothing holds it.
 * @param {string} docId
 * @returns {Promise<boolean>}
 */
export async function reopenDocumentById(docId) {
	try {
		const draft = (await listDrafts()).find((d) => d.docId === docId);
		if (draft?.json) {
			await openDocumentRecord(docId, draft.json);
			await restoreSketchSession(draft.sketch);
			scheduleAutoSave();
			return true;
		}
	} catch {
		// drafts unavailable
	}
	const { getActiveProvider, getStore } = await import('$lib/storage/index.js');
	for (const store of [getActiveProvider(), getStore()]) {
		let doc = null;
		try {
			doc = await store.get(docId);
		} catch {
			continue;
		}
		if (doc?.json) {
			await openDocumentRecord(doc.id, doc.json, doc.link ?? null);
			return true;
		}
	}
	return false;
}

/** Create or update storage record `docId`, keeping its `created` time. */
async function putRecord(store, docId, jsonData) {
	const existing = await store.get(docId);
	await store.put({
		id: docId,
		json: jsonData,
		created: existing?.created || Date.now(),
		modified: Date.now()
	});
}

/**
 * Save current document state to the active storage provider (and this tab's draft).
 * Builds full v3 JSON including all tabs.
 */
async function saveToProvider({ allowEmpty = true } = {}) {
	if (!activeDocId) return false;
	const docId = activeDocId;
	const jsonData = await buildDocumentJson();
	if (!jsonData) return false;
	if (!allowEmpty && documentTextIsEmpty(jsonData)) {
		throw Object.assign(new Error('the document has no features, instances or sources'), { emptyDocument: true });
	}
	await saveDraft(docId, jsonData);
	const { getActiveProvider } = await import('$lib/storage/index.js');
	await putRecord(getActiveProvider(), docId, jsonData);
	return true;
}

/** Whether an edit is waiting for its autosave (the last few seconds are not stored yet). */
export function hasPendingAutoSave() {
	return autoSaveTimer != null;
}

/** Drop a pending autosave (its changes are discarded or superseded). */
export function cancelPendingAutoSave() {
	if (autoSaveTimer) {
		clearTimeout(autoSaveTimer);
		autoSaveTimer = null;
	}
}

/**
 * Whether a composed `.waffle` is the blank startup document: one tab with
 * no feature or instance, and no sources. That is what a freshly booted tab
 * has, and an agent saving it after an unnoticed reload only litters the
 * storage list (the "Untitled" junk of 2026-09-23). A second tab is already
 * work, features or not.
 * @param {string} jsonData
 */
function documentTextIsEmpty(jsonData) {
	let parsed;
	try {
		parsed = JSON.parse(jsonData);
	} catch {
		return false;
	}
	if ((parsed?.sources ?? []).length > 0) return false;
	if ((parsed?.tabs ?? []).length > 1) return false;
	for (const tab of parsed?.tabs ?? []) {
		const kind = tab?.kind ?? {};
		if ((kind.features?.features ?? []).length > 0) return false;
		if ((kind.assembly?.instances ?? []).length > 0) return false;
	}
	return true;
}

/**
 * Save the open document to the active storage provider now, throwing instead
 * of toasting (Ctrl+S and the agent link's document_save). A pending autosave
 * is superseded. With `allowEmpty: false` an empty document (see
 * `documentTextIsEmpty`) is refused with `err.emptyDocument`.
 * @param {{ allowEmpty?: boolean }} [opts]
 * @returns {Promise<{ provider: string, id: string, saved_at: string }>}
 */
export async function saveDocumentOrThrow({ allowEmpty = true } = {}) {
	if (documentLink?.readOnly) {
		throw Object.assign(new Error('This document is linked read-only — fork it to edit'), { readOnly: true });
	}
	if (!activeDocId) throw new Error('no storage record is open in this tab');
	cancelPendingAutoSave();
	if (!(await saveToProvider({ allowEmpty }))) {
		throw new Error('the engine did not compose the document for saving');
	}
	const { getActiveProvider } = await import('$lib/storage/index.js');
	return { provider: getActiveProvider().id, id: activeDocId, saved_at: new Date().toISOString() };
}

/**
 * Save immediately to the active storage provider (for Ctrl+S).
 * @returns {Promise<boolean>}
 */
export async function saveToStorage() {
	if (documentLink?.readOnly) {
		showToast('warning', 'This document is linked read-only — fork it to edit');
		return false;
	}
	if (!activeDocId) {
		// No active doc — fall back to file download
		return !!(await saveProject());
	}
	try {
		await saveDocumentOrThrow();
		showToast('success', 'Saved');
		log('action', 'Document saved');
		return true;
	} catch (err) {
		showToast('error', `Save failed: ${err.message || err}`);
		return false;
	}
}

/**
 * Fork a linked (read-only) document into the user's active storage
 * (specs/waffle_v4_document_model.md §7.1): a copy with a NEW `document.id`
 * and storage record, its `Relative` sources rewritten by the engine to
 * absolute `Git` locators pinned at the commit the link was opened at
 * (`RebaseSources`), and the link dropped — the fork is an ordinary,
 * editable document from here on.
 * @returns {Promise<string | null>} the new storage document id
 */
export async function forkLinkedDocument() {
	if (!documentLink || !bridge || !engineReady) return null;
	// Plain data for postMessage — `documentLink` is a Svelte $state proxy.
	const link = JSON.parse(JSON.stringify(documentLink));
	if (link.locator?.type === 'Git' && link.resolved?.commit) {
		try {
			await bridge.send({ type: 'RebaseSources', base: link.locator, commit: link.resolved.commit });
		} catch (err) {
			showToast('error', `Fork failed: ${err?.message || err}`);
			return null;
		}
	}
	const { getActiveProvider } = await import('$lib/storage/index.js');
	documentId = generateUUID();
	documentCreated = new Date().toISOString();
	documentLink = null;
	// The fork's storage record is keyed by its new identity (P2-5).
	activeDocId = documentId;
	try {
		await saveToProvider();
	} catch (err) {
		showToast('error', `Fork failed: ${err?.message || err}`);
		return null;
	}
	showToast('success', `Forked to ${getActiveProvider().label}`);
	log('action', 'Forked linked document', { from: link.locator, docId: activeDocId });
	return activeDocId;
}

/**
 * Save project to JSON string without triggering browser download.
 * @returns {Promise<string | null>}
 */
async function saveProjectToString() {
	if (!bridge || !engineReady) return null;
	const response = await bridge.send({ type: 'SaveProject' });
	if (response.type !== 'SaveReady' || !response.json_data) return null;
	return response.json_data;
}

// -- Test case browser --

export function getTestCaseBrowserState() { return testCaseBrowserState; }

export function showTestCaseBrowser() {
	testCaseBrowserState.visible = true;
	refreshTestCases();
}

export function hideTestCaseBrowser() {
	testCaseBrowserState.visible = false;
}

export function toggleTestCaseBrowser() {
	if (testCaseBrowserState.visible) {
		hideTestCaseBrowser();
	} else {
		showTestCaseBrowser();
	}
}

export async function refreshTestCases() {
	testCaseBrowserState.loading = true;
	testCaseBrowserState.error = null;
	try {
		const manifest = await fetchTestCases();
		testCaseBrowserState.cases = manifest.cases;
	} catch (err) {
		testCaseBrowserState.error = err.message;
	} finally {
		testCaseBrowserState.loading = false;
	}
}

export function getSaveTestCaseDialogState() { return saveTestCaseDialogState; }

export function showSaveTestCaseDialog() {
	saveTestCaseDialogState = {
		name: projectName || 'Untitled',
		description: '',
		expectedOutcome: 'should_pass',
		tags: ''
	};
}

export function hideSaveTestCaseDialog() {
	saveTestCaseDialogState = null;
}

export async function saveAsTestCase(name, description, expectedOutcome, tags) {
	const waffleData = await saveProjectToString();
	if (!waffleData) {
		showToast('error', 'Failed to save test case: no project data');
		return;
	}
	const tagArray = tags ? tags.split(',').map(t => t.trim()).filter(Boolean) : [];
	try {
		await apiCreateTestCase({
			name,
			description,
			expectedOutcome,
			tags: tagArray,
			waffleData
		});
		showToast('info', `Test case "${name}" saved`);
		hideSaveTestCaseDialog();
		await refreshTestCases();
	} catch (err) {
		showToast('error', `Failed to save test case: ${err.message}`);
	}
}

export async function loadTestCase(id) {
	try {
		const waffleData = await fetchTestCase(id);
		await loadProject(waffleData);
		showToast('info', 'Test case loaded');
	} catch (err) {
		showToast('error', `Failed to load test case: ${err.message}`);
	}
}

export async function removeTestCase(id) {
	try {
		await apiDeleteTestCase(id);
		showToast('info', 'Test case deleted');
		await refreshTestCases();
	} catch (err) {
		showToast('error', `Failed to delete test case: ${err.message}`);
	}
}

// -- Assay browser --

export function getAssayBrowserState() { return assayBrowserState; }

export function toggleAssayBrowser() {
	if (assayBrowserState.visible) {
		assayBrowserState.visible = false;
	} else {
		// The two side panels share the right edge: one at a time.
		examplesBrowserState.visible = false;
		assayBrowserState.visible = true;
		refreshAssayCases();
	}
}

// -- Examples panel --

export function getExamplesBrowserState() { return examplesBrowserState; }

export function toggleExamplesBrowser() {
	if (examplesBrowserState.visible) {
		examplesBrowserState.visible = false;
	} else {
		assayBrowserState.visible = false;
		examplesBrowserState.visible = true;
		refreshExamples();
	}
}

export function hideExamplesBrowser() {
	examplesBrowserState.visible = false;
}

export async function refreshExamples() {
	examplesBrowserState.loading = true;
	examplesBrowserState.error = null;
	try {
		const { fetchExamplesManifest, examplesWritable } = await import('./examplesApi.js');
		const [manifest, writable] = await Promise.all([fetchExamplesManifest(), examplesWritable()]);
		examplesBrowserState.examples = manifest.examples || [];
		examplesBrowserState.writable = writable;
	} catch (err) {
		examplesBrowserState.error = err.message;
	} finally {
		examplesBrowserState.loading = false;
	}
}

/**
 * Open an official example as a NEW document: a copy under a fresh identity,
 * so edits autosave to a record of their own and the shipped file is never
 * the one being written. Goes through `openDocumentRecord`, the same path a
 * stored document takes (every tab adopted, sources resolved, an active
 * assembly evaluated).
 * @param {string} id - manifest entry id
 */
export async function loadExample(id) {
	const entry = examplesBrowserState.examples.find((e) => e.id === id);
	if (!entry) {
		showToast('error', `No example "${id}"`);
		return false;
	}
	examplesBrowserState.opening = id;
	try {
		const { fetchExampleDocument } = await import('./examplesApi.js');
		const text = await fetchExampleDocument(entry);
		const parsed = JSON.parse(text);
		if (fileTooNew(parsed)) {
			showToast('error', 'This example was saved by a newer version of Waffle Iron');
			return false;
		}
		const docId = generateUUID();
		const now = new Date().toISOString();
		parsed.document = { ...(parsed.document || {}), id: docId, name: entry.name, created: now, modified: now };
		await openDocumentRecord(docId, JSON.stringify(parsed));
		examplesBrowserState.active = id;
		showToast('info', `Example "${entry.name}" opened as a new document`);
		return true;
	} catch (err) {
		showToast('error', `Failed to open example: ${err.message || err}`);
		return false;
	} finally {
		examplesBrowserState.opening = null;
	}
}

/**
 * Save the open document as a new official example (development only: the
 * Vite plugin writes `static/examples/<id>.waffle` and the manifest entry).
 * @param {string} name
 * @param {string} description
 */
export async function saveAsExample(name, description) {
	if (!examplesBrowserState.writable) {
		showToast('error', 'Examples can only be saved from the development server');
		return false;
	}
	examplesBrowserState.saving = true;
	try {
		const waffleData = await buildDocumentJson();
		if (!waffleData) throw new Error('the engine did not compose the document');
		const { createExample } = await import('./examplesApi.js');
		const entry = await createExample({ name, description, waffleData });
		showToast('info', `Example "${entry.name}" saved to app/static/examples/${entry.filename}`);
		await refreshExamples();
		examplesBrowserState.active = entry.id;
		return true;
	} catch (err) {
		showToast('error', `Failed to save example: ${err.message || err}`);
		return false;
	} finally {
		examplesBrowserState.saving = false;
	}
}

export function hideAssayBrowser() {
	assayBrowserState.visible = false;
}

export async function refreshAssayCases() {
	assayBrowserState.loading = true;
	assayBrowserState.error = null;
	try {
		const { fetchAssayManifest, fetchAssayResults } = await import('./assayCaseApi.js');
		const [manifest, resultsData] = await Promise.all([
			fetchAssayManifest(),
			fetchAssayResults()
		]);
		assayBrowserState.cases = manifest.cases || [];
		// Build results lookup map { id -> { status, category, detail } }
		const resultsMap = {};
		if (resultsData && resultsData.results) {
			for (const r of resultsData.results) {
				resultsMap[r.id] = { status: r.status, category: r.category, detail: r.detail };
			}
		}
		assayBrowserState.results = resultsMap;
	} catch (err) {
		assayBrowserState.error = err.message;
	} finally {
		assayBrowserState.loading = false;
	}
}

export async function loadAssayCase(id) {
	try {
		const { fetchAssayCase, fetchAssayMeta } = await import('./assayCaseApi.js');
		const [waffleData, meta] = await Promise.all([
			fetchAssayCase(id),
			fetchAssayMeta(id)
		]);
		assayBrowserState.activeCase = id;
		assayBrowserState.activeMeta = meta;
		await loadProject(waffleData);
		setTimeout(() => window.dispatchEvent(new Event('waffle-fit-all')), 100);
		showToast('info', `Assay case ${id} loaded`);
	} catch (err) {
		showToast('error', `Failed to load assay case: ${err.message}`);
	}
}

/**
 * Create a user-defined datum (construction) plane.
 * @param {{ method: string, [key: string]: any }} definition - PlaneDefinition
 * @param {string} name - Display name for the plane
 */
export async function createDatumPlane(definition, name) {
	if (!bridge || !engineReady) return;
	log('action', 'Create datum plane', { name, method: definition.method });
	// Strip any reactive/proxy wrappers (e.g. a face GeomRef captured from
	// $state) to a plain structured-cloneable object before crossing the
	// Worker boundary — otherwise postMessage throws DataCloneError.
	const plainDefinition = JSON.parse(JSON.stringify(definition));
	try {
		await sendRebuild({
			type: 'AddFeature',
			operation: {
				type: 'DatumPlane',
				params: { name, definition: plainDefinition }
			}
		});
	} catch (err) {
		log('error', `Create datum plane failed: ${err.message}`);
		showToast('error', `Datum plane failed: ${err.message}`);
	}
}

// -- Engine commands --

/**
 * Delete a feature by ID.
 * @param {string} featureId
 */
export async function deleteFeature(featureId) {
	if (!bridge || !engineReady) return;
	log('action', 'Delete feature', { featureId });
	await sendRebuild({ type: 'DeleteFeature', feature_id: featureId });
}

/**
 * Suppress or unsuppress a feature.
 * @param {string} featureId
 * @param {boolean} suppressed
 */
export async function suppressFeature(featureId, suppressed) {
	if (!bridge || !engineReady) return;
	log('action', 'Suppress feature', { featureId, suppressed });
	await sendRebuild({ type: 'SuppressFeature', feature_id: featureId, suppressed });
}

/**
 * Set the rollback index.
 * @param {number | null} index
 */
export async function setRollbackIndex(index) {
	if (!bridge || !engineReady) return;
	await sendRebuild({ type: 'SetRollbackIndex', index });
}

/**
 * Edit a feature's operation.
 * @param {string} featureId
 * @param {object} operation
 */
export async function editFeature(featureId, operation) {
	if (!bridge || !engineReady) return;
	await sendRebuild({ type: 'EditFeature', feature_id: featureId, operation });
}

/**
 * Open the appropriate edit dialog for a feature (Extrude or Revolve).
 * @param {string} featureId
 */
export function showEditFeatureDialog(featureId) {
	const tree = featureTree;
	if (!tree) return;
	const feature = tree.features?.find(f => f.id === featureId);
	if (!feature) return;
	const opType = feature.operation?.type;
	if (opType === 'Extrude') showExtrudeDialogForEdit(featureId);
	else if (opType === 'Revolve') showRevolveDialogForEdit(featureId);
	else if (opType === 'Pipe') showPipeDialogForEdit(featureId);
	else if (opType === 'Script') showScriptDialogForEdit(featureId);
	else if (opType === 'ImportedBody') showImportDialogForEdit(featureId);
	else if (opType === 'MateConnector') showMateConnectorDialog(featureId);
}

/**
 * Reorder a feature to a new position in the tree.
 * @param {string} featureId
 * @param {number} newPosition
 */
export async function reorderFeature(featureId, newPosition) {
	if (!bridge || !engineReady) return;
	log('action', 'Reorder feature', { featureId, newPosition });
	await sendRebuild({ type: 'ReorderFeature', feature_id: featureId, new_position: newPosition });
}

/**
 * Rename a feature.
 * @param {string} featureId
 * @param {string} newName
 */
export async function renameFeature(featureId, newName) {
	if (!bridge || !engineReady) return;
	log('action', 'Rename feature', { featureId, newName });
	await bridge.send({ type: 'RenameFeature', feature_id: featureId, new_name: newName });
}

/**
 * Undo the last action. During sketch mode, undoes the last sketch drawing action.
 * Outside sketch mode, undoes the last feature-level action.
 */
export async function undo() {
	if (sketchMode.active) {
		log('action', 'Undo sketch');
		undoSketchAction();
		return;
	}
	log('action', 'Undo feature');
	if (!bridge || !engineReady) return;
	try {
		await sendRebuild({ type: 'Undo' });
	} catch { /* NothingToUndo — no-op */ }
}

/**
 * Redo the last undone action. During sketch mode, redoes the last sketch drawing action.
 * Outside sketch mode, redoes the last feature-level action.
 */
export async function redo() {
	if (sketchMode.active) {
		log('action', 'Redo sketch');
		redoSketchAction();
		return;
	}
	log('action', 'Redo feature');
	if (!bridge || !engineReady) return;
	try {
		await sendRebuild({ type: 'Redo' });
	} catch { /* NothingToRedo — no-op */ }
}

/**
 * Undo the last sketch drawing action. Removes entities/constraints and cascades.
 */
function undoSketchAction() {
	if (sketchUndoStack.length === 0) return;
	const action = sketchUndoStack[sketchUndoStack.length - 1];
	sketchUndoStack = sketchUndoStack.slice(0, -1);

	const idSet = new Set(action.entities.map(e => e.id));

	// Find cascaded constraints (reference removed entities but not part of this action)
	const actionConstraintJsons = new Set(action.constraints.map(c => JSON.stringify(c)));
	const cascadedConstraints = [];
	for (const c of sketchConstraints) {
		const cJson = JSON.stringify(c);
		if (actionConstraintJsons.has(cJson)) continue;
		const refs = [c.entity, c.entity_a, c.entity_b, c.line, c.curve,
			c.line_a, c.line_b, c.point].filter(v => v != null);
		if (refs.some(id => idSet.has(id))) {
			cascadedConstraints.push(JSON.parse(cJson));
		}
	}

	// Remove entities
	sketchEntities = sketchEntities.filter(e => !idSet.has(e.id));
	// Revert the projected-binding side-table for any projected points this action
	// removed (a projected point carries a binding created alongside it); keep the
	// removed bindings so redo can restore them.
	const removedBindings = projectedBindings.filter(b => idSet.has(b.point_id));
	if (removedBindings.length) {
		projectedBindings = projectedBindings.filter(b => !idSet.has(b.point_id));
	}
	// Restore the pre-action geometry as the base (so solver-driven movement from
	// the undone constraint/entity is reverted), then drop any points this action
	// added. Falls back to current positions for legacy entries without a snapshot.
	const base = action.positionsBefore
		? new Map([...action.positionsBefore].map(([k, v]) => [k, { x: v.x, y: v.y }]))
		: new Map(sketchPositions);
	for (const e of action.entities) {
		if (e.type === 'Point') base.delete(e.id);
	}
	sketchPositions = base;
	applyRadii(action.radiiBefore);

	// Remove action constraints + cascaded constraints
	const allRemovedJsons = new Set([
		...action.constraints.map(c => JSON.stringify(c)),
		...cascadedConstraints.map(c => JSON.stringify(c))
	]);
	sketchConstraints = sketchConstraints.filter(c => !allRemovedJsons.has(JSON.stringify(c)));

	// Push to redo stack with cascaded info for restore. Capture the camera
	// being left so redo can return to it; carry positionsAfter through for
	// positions-only (drag) actions.
	sketchRedoStack = [...sketchRedoStack, {
		entities: action.entities,
		constraints: action.constraints,
		cascadedConstraints,
		projectedBindings: removedBindings,
		positionsAfter: action.positionsAfter ?? null,
		radiiBefore: action.radiiBefore ?? null,
		radiiAfter: action.radiiAfter ?? null,
		camera: getCameraState()
	}];

	// Restore the viewport the action was performed in — undoing geometry
	// while leaving an auto-fit zoom-out in place strands the user far from
	// the restored sketch.
	restoreCameraState(action.camera);

	recomputeOverConstrained();
	reExtractProfiles();
	triggerSolve();
	resetTool();
}

/**
 * Redo the last undone sketch drawing action. Restores entities/constraints.
 */
function redoSketchAction() {
	if (sketchRedoStack.length === 0) return;
	const action = sketchRedoStack[sketchRedoStack.length - 1];
	sketchRedoStack = sketchRedoStack.slice(0, -1);

	// Pre-redo geometry + camera, so a later undo of this re-application
	// reverts movement and viewport alike.
	const positionsBefore = snapshotPositions();
	const cameraBefore = getCameraState();

	// Re-add entities
	for (const e of action.entities) {
		const clone = JSON.parse(JSON.stringify(e));
		sketchEntities = [...sketchEntities, clone];
		if (clone.type === 'Point') {
			const next = new Map(sketchPositions);
			next.set(clone.id, { x: clone.x, y: clone.y });
			sketchPositions = next;
		}
	}

	// Re-add constraints (action + cascaded)
	const allConstraints = [...action.constraints, ...(action.cascadedConstraints || [])];
	for (const c of allConstraints) {
		sketchConstraints = [...sketchConstraints, JSON.parse(JSON.stringify(c))];
	}

	// Restore any projected bindings that undo pruned with these entities.
	const restoredBindings = action.projectedBindings ?? [];
	if (restoredBindings.length) {
		projectedBindings = [...projectedBindings, ...restoredBindings];
	}

	// Positions-only actions (drag repositions) re-apply their end state.
	if (action.positionsAfter) {
		sketchPositions = new Map(
			[...action.positionsAfter].map(([k, v]) => [k, { x: v.x, y: v.y }])
		);
	}
	applyRadii(action.radiiAfter);

	// Push to undo stack (merge cascaded into constraints so undo removes them all)
	sketchUndoStack = [...sketchUndoStack, {
		entities: action.entities,
		constraints: allConstraints,
		positionsBefore,
		positionsAfter: action.positionsAfter ?? null,
		radiiBefore: action.radiiBefore ?? null,
		radiiAfter: action.radiiAfter ?? null,
		camera: cameraBefore
	}];

	// Return to the viewport the user was in before they undid this action.
	restoreCameraState(action.camera);

	recomputeOverConstrained();
	reExtractProfiles();
	triggerSolve();
}

/**
 * Save the current project to a .waffle file (browser download).
 * Sends SaveProject to engine, receives SaveReady { json_data }, triggers download.
 * @returns {Promise<string | null>} The JSON data string, or null on failure
 */
export async function saveProject() {
	if (!bridge || !engineReady) return null;
	log('action', 'Save project');
	// Full document (all tabs, preserved metadata) — the raw engine SaveProject
	// only carries the active tab's tree, so downloading it dropped every other
	// tab of a multi-tab document. docs/FILE_FORMAT.md §14.4.
	let jsonData = null;
	try {
		jsonData = await buildDocumentJson();
	} catch (err) {
		log('error', `Save project failed: ${err.message || err}`);
		showToast('error', `Save failed: ${err.message || err}`);
		return null;
	}
	if (!jsonData) return null;
	log('action', 'Project saved', { bytes: jsonData.length });
	showToast('success', 'Project saved');

	// Trigger browser file download
	if (typeof document !== 'undefined') {
		const blob = new Blob([jsonData], { type: 'application/json' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = `${projectName}.waffle`;
		document.body.appendChild(a);
		a.click();
		document.body.removeChild(a);
		URL.revokeObjectURL(url);
	}

	return jsonData;
}

/**
 * Export the current model as a binary STL file (browser download).
 * Sends ExportStl to engine, receives StlExportReady { stl_data } (base64),
 * decodes and triggers download as 'model.stl'.
 * @returns {Promise<boolean>} True if export succeeded
 */
/**
 * Decode base64 STL data and trigger a browser download as `${filename}.stl`.
 * @param {string} stlBase64
 * @param {string} filename - without extension
 */
export function triggerStlDownload(stlBase64, filename) {
	if (typeof document === 'undefined') return;
	const binary = atob(stlBase64);
	const bytes = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i++) {
		bytes[i] = binary.charCodeAt(i);
	}
	const blob = new Blob([bytes], { type: 'application/octet-stream' });
	const url = URL.createObjectURL(blob);
	const a = document.createElement('a');
	a.href = url;
	a.download = `${filename}.stl`;
	document.body.appendChild(a);
	a.click();
	document.body.removeChild(a);
	URL.revokeObjectURL(url);
}

export async function exportStl() {
	if (!bridge || !engineReady) return false;
	log('action', 'Export STL');
	const response = await bridge.send({ type: 'ExportStl' });
	if (response.type !== 'StlExportReady' || !response.stl_data) return false;
	showToast('success', 'STL exported');
	triggerStlDownload(response.stl_data, projectName);
	return true;
}

/**
 * Export a single body to STL (browser download as `${name}.stl`).
 * @param {string} bodyId - persistent body id (featureId/outputKeyTag)
 * @param {string} name - display name for the file
 * @returns {Promise<boolean>} True if export succeeded
 */
export async function exportBodyStl(bodyId, name) {
	if (!bridge || !engineReady) return false;
	log('action', 'Export body STL', { bodyId, name });
	const response = await bridge.send({ type: 'ExportBodyStl', body_id: bodyId });
	if (response.type !== 'StlExportReady' || !response.stl_data) {
		showToast('error', 'Body has no mesh to export');
		return false;
	}
	const safe = (name || 'body').replace(/[^\w.-]+/g, '_');
	showToast('success', `Exported ${safe}.stl`);
	triggerStlDownload(response.stl_data, safe);
	return true;
}

/**
 * Export the current model as a STEP AP203 file (browser download).
 * Sends ExportStep to engine, receives ExportReady { step_data, warnings },
 * and triggers download as '<project>.step'. The export is the whole model:
 * every live body of the part, or every placed instance of an open assembly.
 * @returns {Promise<boolean>} True if export succeeded
 */
export async function exportStep() {
	if (!bridge || !engineReady) return false;
	log('action', 'Export STEP');
	const response = await bridge.send({ type: 'ExportStep' });
	if (response.type !== 'ExportReady' || !response.step_data) return false;
	showToast('success', 'STEP exported');
	// The engine reports what it left out (a mesh-backed imported body has no
	// analytic geometry to write) — surface it rather than export silently.
	for (const w of response.warnings ?? []) {
		log('warn', `STEP export: ${w}`);
		showToast('warning', `STEP export: ${w}`);
	}
	triggerStepDownload(response.step_data, `${projectName}.step`);
	return true;
}

/**
 * Trigger a browser download of STEP text as `fileName`.
 * @param {string} stepData
 * @param {string} fileName - with extension
 */
export function triggerStepDownload(stepData, fileName) {
	if (typeof document === 'undefined') return;
	const blob = new Blob([stepData], { type: 'application/step' });
	const url = URL.createObjectURL(blob);
	const a = document.createElement('a');
	a.href = url;
	a.download = fileName;
	document.body.appendChild(a);
	a.click();
	document.body.removeChild(a);
	URL.revokeObjectURL(url);
}

/**
 * Import a STEP file as a new ImportedBody feature (task #138). Sends the raw
 * STEP text; the engine compresses and embeds it in the feature. Used by the
 * file picker AND directly by tests (real file pickers can't be driven).
 * @param {string} fileName - e.g. 'minihexa.step'
 * @param {string} text - raw STEP text
 * @returns {Promise<boolean>}
 */
export async function importStepFromText(fileName, text) {
	if (!bridge || !engineReady) return false;
	log('action', 'Import STEP', { fileName, bytes: text.length });
	try {
		await sendRebuild({ type: 'ImportStep', file_name: fileName, data: text });
		showToast('info', `Imported ${fileName}`);
		// Open the placement modal on the freshly-imported feature: the body
		// renders as a ghost preview until the user confirms (Apply) or
		// cancels (which removes the feature again).
		const features = featureTree?.features ?? [];
		const feature = [...features].reverse().find(f => f.operation?.type === 'ImportedBody');
		if (feature) showImportDialog(feature.id, { isNew: true });
		return true;
	} catch (err) {
		log('error', `STEP import failed: ${err.message || err}`);
		showToast('error', `STEP import failed: ${err.message || err}`);
		return false;
	}
}

/**
 * Import a STEP file from a link (a GitHub/GitLab/Gitea file URL, a raw URL,
 * an `/open` share link, or any https URL) as a LINKED source
 * (specs/waffle_v4_document_model.md §2.3, Phase 2 P2-3): fetched now at the
 * resolved commit, cached by hash, and recorded with its locator + commit so
 * later opens re-resolve it from its origin instead of embedding a copy.
 * @param {string} url
 * @returns {Promise<boolean>}
 */
export async function importStepFromLink(url) {
	if (!bridge || !engineReady) return false;
	const { locatorForImportLink, fetchGitAt } = await import('$lib/storage/sources.js');
	const { fetchUrlLocator } = await import('$lib/storage/git/hosts.js');
	const { cachePut } = await import('$lib/storage/git/cache.js');
	const { gitBlobSha1 } = await import('$lib/storage/git/hash.js');
	const locator = locatorForImportLink(url);
	if (!locator) {
		showToast('error', 'Not a usable link (https file URL or share link expected)');
		return false;
	}
	const fileName = (locator.type === 'Git' ? locator.path : locator.url).split('/').pop() || 'linked.step';
	log('action', 'Import STEP from link', { fileName, locator });
	try {
		let text;
		let resolvedCommit = null;
		if (locator.type === 'Git') {
			({ text, commit: resolvedCommit } = await fetchGitAt(locator, null));
		} else {
			({ text } = await fetchUrlLocator(locator.url));
		}
		await cachePut(await gitBlobSha1(text), text);
		await sendRebuild({
			type: 'ImportStepFromLocator',
			file_name: fileName,
			locator,
			data: text,
			resolved_commit: resolvedCommit
		});
		showToast('info', `Linked ${fileName}`);
		const features = featureTree?.features ?? [];
		const feature = [...features].reverse().find((f) => f.operation?.type === 'ImportedBody');
		if (feature) showImportDialog(feature.id, { isNew: true });
		return true;
	} catch (err) {
		log('error', `STEP link import failed: ${err?.message || err}`);
		showToast('error', `STEP link import failed: ${err?.message || err}`);
		return false;
	}
}

/**
 * Import a `.kicad_pcb` (specs/kicad_board_link.md C2/C4): the engine reads
 * the board, derives the Board Part (exact outline solid), one placeholder
 * Part per footprint shape and the board assembly (one instance per
 * footprint, a connector per mounting hole), and opens the Board tab. Used
 * by the file picker AND directly by tests.
 * @param {string} fileName
 * @param {string} text
 * @returns {Promise<boolean>}
 */
export async function importKicadFromText(fileName, text) {
	if (!bridge || !engineReady) return false;
	log('action', 'Import KiCad board', { fileName, bytes: text.length });
	entityMetaCache.clear();
	try {
		await sendRebuild({ type: 'ImportKicad', file_name: fileName, data: text });
		showToast('info', `Linked board ${fileName}`);
		return true;
	} catch (err) {
		log('error', `KiCad import failed: ${err?.message || err}`);
		showToast('error', `KiCad import failed: ${err?.message || err}`);
		return false;
	}
}

/**
 * Open a file picker for a `.kicad_pcb` and import it.
 * @returns {Promise<boolean>}
 */
export async function importKicad() {
	if (!bridge || !engineReady) return false;
	return new Promise((resolve) => {
		const input = document.createElement('input');
		input.type = 'file';
		input.accept = '.kicad_pcb';
		input.onchange = async () => {
			const file = input.files?.[0];
			if (!file) { resolve(false); return; }
			const text = await file.text();
			resolve(await importKicadFromText(file.name, text));
		};
		input.click();
	});
}

/**
 * Link a `.kicad_pcb` by URL (specs/kicad_board_link.md §1): fetched at the
 * resolved commit, cached by hash, recorded as a LINKED `KicadPcb` source,
 * then derived exactly as `importKicadFromText` does.
 * @param {string} url
 * @returns {Promise<boolean>}
 */
export async function linkKicadFromLink(url) {
	if (!bridge || !engineReady) return false;
	const { locatorForImportLink, fetchGitAt } = await import('$lib/storage/sources.js');
	const { fetchUrlLocator } = await import('$lib/storage/git/hosts.js');
	const { cachePut } = await import('$lib/storage/git/cache.js');
	const { gitBlobSha1 } = await import('$lib/storage/git/hash.js');
	const locator = locatorForImportLink(url);
	if (!locator) {
		showToast('error', 'Not a usable link (https file URL or share link expected)');
		return false;
	}
	const fileName = (locator.type === 'Git' ? locator.path : locator.url).split('/').pop() || '';
	if (!fileName.endsWith('.kicad_pcb')) {
		showToast('error', 'Expected a link to a .kicad_pcb file');
		return false;
	}
	log('action', 'Link KiCad board', { fileName, locator });
	entityMetaCache.clear();
	try {
		let text;
		let resolvedCommit = null;
		if (locator.type === 'Git') {
			({ text, commit: resolvedCommit } = await fetchGitAt(locator, null));
		} else {
			({ text } = await fetchUrlLocator(locator.url));
		}
		await cachePut(await gitBlobSha1(text), text);
		await sendRebuild({
			type: 'LinkKicadFromLocator',
			file_name: fileName,
			locator,
			data: text,
			resolved_commit: resolvedCommit
		});
		showToast('info', `Linked board ${fileName}`);
		return true;
	} catch (err) {
		log('error', `KiCad link failed: ${err?.message || err}`);
		showToast('error', `KiCad link failed: ${err?.message || err}`);
		return false;
	}
}

// ── Board data on hover and click (specs/kicad_board_link.md C4) ──────
//
// The viewport asks, per hovered body, what a linked KiCad board knows about
// it (`QueryEntityMeta`); the answer is cached per (tab, body) until the
// sources change, so hovering is one query per body, not one per pixel. A
// body that derives from no board answers null and shows nothing.

const entityMetaCache = new Map();
/** @type {{ x: number, y: number, meta: object } | null} */
let entityCard = $state(null);
/** @type {object | null} */
let entityDetail = $state(null);
let _entityCardKey = null;

/**
 * The card to show, or null. Visibility follows the arbitrated hover
 * (`hoveredRef`), not any one listener's leave event: the mesh, edge and
 * vertex listeners each propose for the same body at different times (DOM
 * listeners now, Threlte's raycast on the next frame), and whichever wins
 * the hover is the body the card is for.
 */
export function getEntityCard() {
	if (!entityCard || !hoveredRef) return null;
	const fid = hoveredRef.anchor?.feature_id;
	if (fid && entityCard.bodyId && !entityCard.bodyId.includes(fid)) return null;
	return entityCard;
}
export function getEntityDetail() { return entityDetail; }
export function hideEntityCard() { entityCard = null; _entityCardKey = null; }
export function hideEntityDetail() { entityDetail = null; }

/**
 * `{board, component, source}` for a body / instance, or null.
 * @param {string | null} bodyId
 * @param {string[] | null} instancePath
 */
export async function queryEntityMeta(bodyId, instancePath) {
	if (!bridge || !engineReady) return null;
	const key = `${activeTabId}|${bodyId ?? ''}|${(instancePath ?? []).join('/')}`;
	if (entityMetaCache.has(key)) return entityMetaCache.get(key);
	// Plain copies: the mesh records are reactive state, and a `$state`
	// proxy cannot be structured-cloned to the worker.
	const r = await bridge.send({
		type: 'QueryEntityMeta',
		body_id: bodyId == null ? null : String(bodyId),
		instance_path: instancePath?.length ? Array.from(instancePath, String) : null
	});
	const meta = r?.board || r?.component
		? { board: r.board ?? null, component: r.component ?? null, source: r.source ?? null }
		: null;
	entityMetaCache.set(key, meta);
	return meta;
}

/**
 * The viewport's hover hook: show (or move) the card for the body under the
 * pointer. Same body as last time ⇒ only the position moves.
 */
export async function proposeEntityCard(bodyId, instancePath, clientX, clientY) {
	const key = `${bodyId ?? ''}|${(instancePath ?? []).join('/')}`;
	if (key === _entityCardKey) {
		if (entityCard) entityCard = { ...entityCard, x: clientX, y: clientY };
		return;
	}
	_entityCardKey = key;
	let meta = null;
	try {
		meta = await queryEntityMeta(bodyId, instancePath);
	} catch (err) {
		// A hover must never surface as an exception; the card just stays off.
		log('warn', `entity meta query failed: ${err?.message || err}`);
	}
	if (_entityCardKey !== key) return; // the pointer moved on meanwhile
	entityCard = meta ? { x: clientX, y: clientY, meta, bodyId: bodyId ?? '' } : null;
}

/**
 * The viewport's click hook: open the detail panel for a body that derives
 * from a board; a click on anything else closes it.
 * @returns {Promise<boolean>} whether a panel opened
 */
export async function openEntityDetail(bodyId, instancePath) {
	const meta = await queryEntityMeta(bodyId, instancePath);
	entityDetail = meta;
	return meta != null;
}

/**
 * The document's `sources` table as the engine reports it on every model
 * update (`SourceStatus[]`: id, name, kind, locator, content_hash, resolved,
 * pack, available). Drives the Sources panel; actions below edit it through
 * the bridge (`UpdateSourceEntry`, `ProvideSource`).
 * @type {Array<object>}
 */
let documentSources = $state([]);
export function getSources() { return documentSources; }

/**
 * Writer policy for one source (v4 §2.3 `pack`): true embeds the content in
 * the file (self-contained), false keeps it linked. Refused by the engine for
 * an Embedded source (nothing to unpack to).
 * @param {string} sourceId
 * @param {boolean} pack
 */
export async function setSourcePack(sourceId, pack) {
	if (!bridge || !engineReady) return false;
	try {
		await sendRebuild({ type: 'UpdateSourceEntry', source_id: sourceId, pack });
		scheduleAutoSave();
		return true;
	} catch (err) {
		showToast('error', `Pack setting failed: ${err?.message || err}`);
		return false;
	}
}

/**
 * "Pack and go": embed every linked source that the engine holds content for.
 * @returns {Promise<number>} how many entries were packed
 */
export async function packAllSources() {
	let n = 0;
	for (const s of documentSources) {
		if (s.pack || !s.available || s.locator?.type === 'Embedded') continue;
		if (await setSourcePack(s.id, true)) n++;
	}
	if (n > 0) showToast('success', `Packed ${n} source${n === 1 ? '' : 's'} into the document`);
	return n;
}

/**
 * Pin a floating git source to the commit it currently resolves to
 * (v4 §2.4 "Pin": `ref ← Commit{resolved.commit}`; content kept).
 * @param {string} sourceId
 */
export async function pinSource(sourceId) {
	if (!bridge || !engineReady) return false;
	const s = documentSources.find((x) => x.id === sourceId);
	if (!s || s.locator?.type !== 'Git') return false;
	if (s.locator.ref?.type === 'Commit') return true;
	if (!s.resolved?.commit) {
		showToast('warning', `${s.name}: nothing resolved yet — fetch it first`);
		return false;
	}
	try {
		await sendRebuild({
			type: 'UpdateSourceEntry',
			source_id: sourceId,
			git_ref: { type: 'Commit', sha: s.resolved.commit }
		});
		showToast('success', `${s.name}: pinned to ${s.resolved.commit.slice(0, 7)}`);
		scheduleAutoSave();
		return true;
	} catch (err) {
		showToast('error', `Pin failed: ${err?.message || err}`);
		return false;
	}
}

/**
 * "Update to tip" (v4 §2.4): re-resolve a floating git source's ref, fetch
 * the content AT the new commit, and record both (`ProvideSource` with
 * `resolved_commit`). A pinned source is never changed. `Relative` sources
 * resolve against the document's location first.
 * @param {string} sourceId
 * @returns {Promise<'updated'|'unchanged'|false>}
 */
export async function updateSourceToTip(sourceId) {
	if (!bridge || !engineReady) return false;
	const s = documentSources.find((x) => x.id === sourceId);
	if (!s) return false;
	const { fetchGitAt } = await import('$lib/storage/sources.js');
	const { resolveRelative } = await import('$lib/storage/git/locator.js');
	const { cachePut } = await import('$lib/storage/git/cache.js');
	const { gitBlobSha1 } = await import('$lib/storage/git/hash.js');
	let loc = JSON.parse(JSON.stringify(s.locator));
	if (loc?.type === 'Relative') loc = resolveRelative(await documentLocation(), loc.path);
	if (loc?.type !== 'Git') {
		showToast('warning', `${s.name}: not a git source`);
		return false;
	}
	if (loc.ref?.type === 'Commit') {
		showToast('info', `${s.name} is pinned; unpin (retarget) before updating`);
		return 'unchanged';
	}
	try {
		const { text, commit } = await fetchGitAt(loc, null);
		if (s.resolved?.commit && commit === s.resolved.commit && s.available) {
			showToast('info', `${s.name} is already at the tip (${commit.slice(0, 7)})`);
			return 'unchanged';
		}
		await cachePut(await gitBlobSha1(text), text);
		await sendRebuild({ type: 'ProvideSource', source_id: sourceId, data: text, resolved_commit: commit });
		const from = s.resolved?.commit ? `${s.resolved.commit.slice(0, 7)} → ` : '';
		showToast('success', `${s.name}: updated ${from}${commit.slice(0, 7)}`);
		scheduleAutoSave();
		return 'updated';
	} catch (err) {
		showToast('error', `${s.name}: update failed: ${err?.message || err}`);
		return false;
	}
}

/**
 * Retry resolving one unavailable source (cache → locator), e.g. after
 * entering a host token.
 * @param {string} sourceId
 */
export async function fetchSource(sourceId) {
	if (!bridge || !engineReady) return false;
	const s = documentSources.find((x) => x.id === sourceId);
	if (!s) return false;
	const { resolveSourceContent } = await import('$lib/storage/sources.js');
	try {
		const { text, resolvedCommit } = await resolveSourceContent(JSON.parse(JSON.stringify(s)), await documentLocation());
		await sendRebuild({ type: 'ProvideSource', source_id: sourceId, data: text, resolved_commit: resolvedCommit });
		showToast('success', `${s.name}: fetched`);
		return true;
	} catch (err) {
		showToast('error', `${s.name}: ${err?.message || err}`);
		return false;
	}
}

/** @type {{ url: string, kind: 'step' | 'kicad' } | null} */
let importLinkDialogState = $state(null);
export function getImportLinkDialogState() { return importLinkDialogState; }
/** @param {'step' | 'kicad'} kind what the pasted link is expected to be */
export function showImportLinkDialog(kind = 'step') { importLinkDialogState = { url: '', kind }; }
export function hideImportLinkDialog() { importLinkDialogState = null; }

/**
 * Open a file picker for a .step/.stp file and import it.
 * @returns {Promise<boolean>} True if an import was initiated
 */
export async function importStep() {
	if (!bridge || !engineReady) return false;
	return new Promise((resolve) => {
		const input = document.createElement('input');
		input.type = 'file';
		input.accept = '.step,.stp,.STEP,.STP';
		input.onchange = async () => {
			const file = input.files?.[0];
			if (!file) { resolve(false); return; }
			const text = await file.text();
			resolve(await importStepFromText(file.name, text));
		};
		input.click();
	});
}

/** @returns {object | null} */
export function getImportDialogState() { return importDialogState; }

/**
 * While the import placement dialog is open, its feature's body renders as a
 * translucent ghost preview. @returns {string | null}
 */
export function getGhostFeatureId() {
	return importDialogState?.featureId ?? null;
}

/**
 * Open the placement modal for an ImportedBody feature. Snapshots the
 * current placement so Cancel can restore (edit) or remove (fresh import).
 * @param {string} featureId
 * @param {{isNew?: boolean}} [opts]
 */
export function showImportDialog(featureId, opts = {}) {
	const feature = featureTree?.features?.find(f => f.id === featureId);
	if (!feature || feature.operation?.type !== 'ImportedBody') return;
	const p = feature.operation.params;
	importDialogState = {
		featureId,
		isNew: opts.isNew === true,
		original: {
			translation_m: [...(p.translation_m ?? [0, 0, 0])],
			rotation_deg: [...(p.rotation_deg ?? [0, 0, 0])],
			scale: p.scale ?? 1.0,
		},
	};
}

/** Legacy name kept for the feature-tree edit path. */
export function showImportDialogForEdit(featureId) {
	showImportDialog(featureId, { isNew: false });
}

export function hideImportDialog() {
	importDialogState = null;
}

/**
 * Apply new placement values to an ImportedBody feature. The embedded STEP
 * payload is passed through UNTOUCHED — only the transform fields change.
 * Used by the dialog's live ghost preview (`close: false`) and by Apply.
 * @param {string} featureId
 * @param {{translation_m: number[], rotation_deg: number[], scale: number}} placement
 * @param {{close?: boolean}} [opts]
 */
export async function applyImportPlacement(featureId, placement, opts = {}) {
	const feature = featureTree?.features?.find(f => f.id === featureId);
	if (!feature || feature.operation?.type !== 'ImportedBody') return false;
	const params = {
		...feature.operation.params,
		translation_m: placement.translation_m,
		rotation_deg: placement.rotation_deg,
		scale: placement.scale ?? 1.0,
	};
	log('action', 'Edit STEP import placement', { featureId, live: opts.close === false });
	await editFeature(featureId, { type: 'ImportedBody', params });
	if (opts.close !== false) hideImportDialog();
	return true;
}

/**
 * Cancel the placement modal: a fresh import is REMOVED (the user declined
 * it), an edit reverts to the placement snapshotted at open.
 */
export async function cancelImportPlacement() {
	const state = importDialogState;
	importDialogState = null;
	if (!state) return;
	if (state.isNew) {
		log('action', 'Cancel STEP import (remove fresh feature)', { featureId: state.featureId });
		await sendRebuild({ type: 'DeleteFeature', feature_id: state.featureId });
		return;
	}
	const feature = featureTree?.features?.find(f => f.id === state.featureId);
	if (!feature || feature.operation?.type !== 'ImportedBody') return;
	const current = feature.operation.params;
	const o = state.original;
	const dirty =
		JSON.stringify([current.translation_m, current.rotation_deg, current.scale ?? 1.0]) !==
		JSON.stringify([o.translation_m, o.rotation_deg, o.scale]);
	if (dirty) {
		log('action', 'Cancel STEP placement edit (revert)', { featureId: state.featureId });
		await editFeature(state.featureId, {
			type: 'ImportedBody',
			params: { ...current, ...o },
		});
	}
}

/**
 * Load a project from a .waffle/.json file (browser file picker).
 * Opens a hidden file input, reads the file, sends LoadProject { data } to engine.
 * The engine responds with ModelUpdated, which is handled by the existing callback.
 * @param {string} [jsonData] - Optional JSON string to load directly (for programmatic use)
 * @returns {Promise<boolean>} True if load was initiated
 */
export async function loadProject(jsonData, { silent = false } = {}) {
	if (!bridge || !engineReady) return false;

	log('action', 'Load project');
	if (jsonData) {
		// Programmatic path: the caller owns the document state
		// (loadPendingDocument runs initDocumentState itself; test-case loads
		// deliberately keep the current document context).
		if (parseTooNew(jsonData)) return false;
		// The display unit (a legacy file's included — the loader converts
		// `project.display_unit`) arrives with the answer's `document` and is
		// mirrored there; nothing here parses the file for it.
		await sendRebuild({ type: 'LoadProject', data: jsonData });
		if (!silent) showToast('info', 'Project loaded');
		return true;
	}

	// Open file picker
	return new Promise((resolve) => {
		const input = document.createElement('input');
		input.type = 'file';
		input.accept = '.waffle,.json';
		input.onchange = async () => {
			const file = input.files?.[0];
			if (!file) { resolve(false); return; }
			const text = await file.text();
			let parsed = null;
			try { parsed = JSON.parse(text); } catch { /* engine load will report it */ }
			if (parsed && fileTooNew(parsed)) {
				showToast('error', 'This file was saved by a newer version of Waffle Iron');
				resolve(false);
				return;
			}
			// A pending autosave still belongs to the PREVIOUS document; firing
			// after the engine swaps trees would capture mixed state.
			if (autoSaveTimer) {
				clearTimeout(autoSaveTimer);
				autoSaveTimer = null;
			}
			try {
				await sendRebuild({ type: 'LoadProject', data: text });
				// Adopt the opened file's tab structure (the engine only holds
				// the active tab's tree) under the file's OWN identity as the
				// storage key (v4 P2-5): a different document can never
				// overwrite the previously open storage record
				// (docs/FILE_FORMAT.md §14.4), and re-opening an export of a
				// stored document re-homes to that same record. A legacy file
				// without an identity gets a fresh one.
				if (parsed) {
					const fileId = isUuid(parsed.document?.id) ? parsed.document.id : generateUUID();
					initDocumentState(fileId, parsed);
				}
				// Project name from filename (wins over the stored doc name).
				const nameWithoutExt = file.name.replace(/\.(waffle|json)$/i, '');
				if (nameWithoutExt) setProjectName(nameWithoutExt);
				showToast('info', 'Project loaded');
				await resolveDocumentSources();
				resolve(true);
			} catch (err) {
				log('error', `Load project failed: ${err.message || err}`);
				showToast('error', 'Failed to load project: invalid file format');
				resolve(false);
			}
		};
		input.click();
	});
}

/**
 * The document's own git location (specs/waffle_v4_document_model.md §2.4):
 * the link it was opened from, else the active provider's locator for it
 * (a document saved to the GitHub provider). Null for browser-local docs —
 * their `Relative` links are unresolvable until saved somewhere.
 * @returns {Promise<any|null>}
 */
async function documentLocation() {
	if (documentLink?.locator?.type === 'Git') return JSON.parse(JSON.stringify(documentLink.locator));
	if (!activeDocId) return null;
	try {
		const { getActiveProvider } = await import('$lib/storage/index.js');
		const provider = getActiveProvider();
		if (typeof provider?.getLocator === 'function') return await provider.getLocator(activeDocId);
	} catch {
		// no location
	}
	return null;
}

/**
 * Resolve every source the engine lacks content for (v4 §2.3 resolution
 * order, Phase 2 P2-3): `ListSources` → content cache by hash, else fetch
 * through the locator at the RECORDED commit → `ProvideSource` (rebuilds).
 * Failures are loud per source (toast) and leave the dependent feature's
 * `SourceUnavailable` error standing; the document itself stays open.
 * @returns {Promise<{resolved: number, failed: number}>}
 */
export async function resolveDocumentSources() {
	if (!bridge || !engineReady) return { resolved: 0, failed: 0 };
	let listed;
	try {
		listed = await bridge.send({ type: 'ListSources' });
	} catch (err) {
		log('error', `ListSources failed: ${err?.message || err}`);
		return { resolved: 0, failed: 0 };
	}
	const missing = (listed?.sources ?? []).filter((s) => !s.available);
	if (missing.length === 0) return { resolved: 0, failed: 0 };
	const { resolveSourceContent } = await import('$lib/storage/sources.js');
	const location = await documentLocation();
	let resolved = 0;
	let failed = 0;
	for (const s of missing) {
		try {
			const { text, resolvedCommit, from } = await resolveSourceContent(s, location);
			await sendRebuild({ type: 'ProvideSource', source_id: s.id, data: text, resolved_commit: resolvedCommit });
			log('system', `Resolved source ${s.name} (${from})`);
			resolved++;
		} catch (err) {
			failed++;
			log('error', `Source ${s.name} unavailable: ${err?.message || err}`);
			showToast('warning', `Source unavailable: ${err?.message || err}`);
		}
	}
	return { resolved, failed };
}

/**
 * Parse-and-check helper for the programmatic load path: true (with an error
 * toast) if the JSON declares a newer reader requirement than this build.
 * @param {string} jsonData
 */
function parseTooNew(jsonData) {
	try {
		if (fileTooNew(JSON.parse(jsonData))) {
			showToast('error', 'This file was saved by a newer version of Waffle Iron');
			return true;
		}
	} catch { /* not JSON — let the engine load path report it */ }
	return false;
}
