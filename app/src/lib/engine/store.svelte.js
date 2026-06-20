/**
 * Engine state store using Svelte 5 runes.
 *
 * Manages reactive state for the WASM engine, including
 * feature tree, mesh data, and engine status.
 */

import { base } from '$app/paths';
import { EngineBridge } from './bridge.js';
import { log, getLogs, exportLogs, clearLogs } from './logger.js';
import { showToast, getToasts, dismissToast, initLoggerToasts } from '$lib/ui/toast.svelte.js';
import { extractProfiles } from '$lib/sketch/profiles.js';
import { sampleBSpline } from '$lib/sketch/bspline.js';
import { getPreview, getSnapIndicator, getSnapCandidates as _getSnapCandidates } from '$lib/sketch/sketchToolState.svelte.js';
import { resetTool, getToolState as _getToolState, getIsDragging as _getIsDragging, getPointerDownPos as _getPointerDownPos, getStartPos as _getStartPos, getStartPointId as _getStartPointId, getToolEventLog as _getToolEventLog, clearToolEventLog as _clearToolEventLog } from '$lib/sketch/tools.js';
import { buildSketchPlane, sketchToScreen } from '$lib/sketch/sketchCoords.js';
import { isDatumPlaneRef, getPlaneIdFromRef, getPlaneById, resolvePlane, BUILTIN_PLANES } from './planes.js';
import { fetchTestCases, fetchTestCase, createTestCase as apiCreateTestCase, deleteTestCase as apiDeleteTestCase } from './testCaseApi.js';

/**
 * Generate a UUID, with fallback for non-secure contexts (e.g. HTTP without localhost).
 * crypto.randomUUID() requires HTTPS or localhost; crypto.getRandomValues() works everywhere.
 * @returns {string}
 */
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

let rebuildTime = $state(0);

let rebuilding = $state(false);

let statusMessage = $state('Initializing...');

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
let sketchMode = $state({ active: false, origin: [0, 0, 0], normal: [0, 0, 1] });

/** @type {string | null} */
let selectedFeatureId = $state(null);

/** @type {string} */
let activeTool = $state('select');

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

/** @type {Array<{ entityIds: number[], isOuter: boolean }>} */
let extractedProfilesState = $state([]);

/** @type {number | null} */
let selectedProfileIndex = $state(null);

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

/** @type {boolean} */
let axisPickMode = $state(false);

/** @type {{ sketchId: string, sketchName: string, profileCount: number, selectedProfile?: any, selectedAxis?: any } | null} */
let revolveDialogState = $state(null);

/** @type {{ sketchId: string, profileIndex: number, angle: number, axisOrigin: [number,number,number], axisDir: [number,number,number] } | null} */
let revolvePreviewParams = $state(null);

/** @type {{ edges: Array<any>, edgeCount: number } | null} */
let chamferDialogState = $state(null);

/** @type {{ edges: Array<any>, edgeCount: number } | null} */
let filletDialogState = $state(null);

/** @type {{ faces: Array<any>, faceCount: number } | null} */
let shellDialogState = $state(null);

/** @type {{ bodies: Array<{ featureId: string, name: string }>, operation: string } | null} */
let booleanDialogState = $state(null);

// -- Test case browser state --

/** @type {{ visible: boolean, cases: Array<object>, loading: boolean, error: string | null }} */
let testCaseBrowserState = $state({ visible: false, cases: [], loading: false, error: null });

/** @type {{ name: string, description: string, expectedOutcome: string, tags: string } | null} */
let saveTestCaseDialogState = $state(null);

/** @type {{ visible: boolean, cases: Array<object>, activeCase: string | null, activeMeta: object | null, loading: boolean, error: string | null, results: Object<string, { status: string, category: string, detail: string }> }} */
let assayBrowserState = $state({ visible: false, cases: [], activeCase: null, activeMeta: null, loading: false, error: null, results: {} });

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

// -- Camera state refs (set by CameraControls) --

/** @type {import('three').PerspectiveCamera | import('three').OrthographicCamera | null} */
let cameraObject = null;

/** @type {any | null} OrbitControls ref */
let controlsObject = null;

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

/** @type {boolean} */
let planetaryDialogOpen = $state(false);

/** @type {number} */
let nextGearId = $state(1);

/** @type {number | null} */
let autoSaveTimer = null;

/** @type {{ available: boolean, timestamp: number } | null} */
let autoRestoreState = $state(null);

/** @type {EngineBridge | null} */
let bridge = null;

/** Get the engine bridge instance (or null if not initialized). */
export function getBridge() { return bridge; }

/**
 * Initialize the engine bridge and WASM worker.
 */
export async function initEngine() {
	if (bridge) return;

	bridge = new EngineBridge();

	bridge.on('modelUpdated', (msg) => {
		if (msg.feature_tree) {
			featureTree = msg.feature_tree;
		}
		if (msg.meshes) {
			meshes = msg.meshes;
		}
		lastError = null;
		statusMessage = `Model updated (${meshes.length} ${meshes.length === 1 ? 'body' : 'bodies'})`;

		// Generate preview mesh from the last mesh for thumbnail/save.
		// Prefer Rust-side preview_mesh if available, otherwise build from JS meshes.
		if (activeTabId) {
			const tab = documentTabs.find(t => t.id === activeTabId);
			if (tab) {
				if (msg.preview_mesh) {
					tab.kind.preview_mesh = msg.preview_mesh;
				} else if (meshes.length > 0) {
					// Build preview from the last JS-side mesh (typed arrays)
					const last = meshes[meshes.length - 1];
					if (last.vertices?.length > 0 && last.indices?.length > 0) {
						tab.kind.preview_mesh = {
							vertices: Array.from(last.vertices),
							normals: Array.from(last.normals || []),
							indices: Array.from(last.indices)
						};
					}
				} else {
					tab.kind.preview_mesh = null;
				}
			}
		}

		scheduleAutoSave();
		log('engine', 'Model updated', { meshCount: meshes.length, featureCount: featureTree?.features?.length ?? 0 });

		// Track feature errors for tree display
		const newErrors = new Map();
		if (msg.errors && msg.errors.length > 0) {
			for (const [featureId, errorMsg] of msg.errors) {
				newErrors.set(featureId, errorMsg);
				log('error', `Feature ${featureId} failed: ${errorMsg}`);
				showToast('error', `Feature failed: ${errorMsg}`);
			}
		}
		featureErrors = newErrors;

		// Surface non-fatal warnings (e.g. auto-union fallback)
		if (msg.warnings && msg.warnings.length > 0) {
			for (const warning of msg.warnings) {
				log('warning', warning);
				showToast('warning', warning);
			}
		}
	});

	bridge.on('sketchSolved', (msg) => {
		if (msg.positions && msg.status !== 'not_ready' && msg.status !== 'solver_not_ready') {
			const newPositions = new Map();
			for (const [id, pos] of Object.entries(msg.positions)) {
				newPositions.set(Number(id), pos);
			}
			sketchPositions = newPositions;

			// Apply solved radii to circle entities
			if (msg.solvedRadii) {
				let changed = false;
				for (const [id, radius] of Object.entries(msg.solvedRadii)) {
					const numId = Number(id);
					const idx = sketchEntities.findIndex(e => e.id === numId);
					if (idx >= 0 && sketchEntities[idx].type === 'Circle') {
						sketchEntities[idx] = { ...sketchEntities[idx], radius };
						changed = true;
					}
				}
				if (changed) {
					sketchEntities = [...sketchEntities];
				}
			}

			reExtractProfiles();
		}

		// Apply reference dimension value updates
		if (msg.refUpdates && msg.refUpdates.length > 0) {
			let constraintsChanged = false;
			for (const upd of msg.refUpdates) {
				if (upd.index >= 0 && upd.index < sketchConstraints.length) {
					const c = sketchConstraints[upd.index];
					if (c.reference && 'value' in c) {
						sketchConstraints[upd.index] = { ...c, value: upd.value };
						constraintsChanged = true;
					}
				}
			}
			if (constraintsChanged) {
				sketchConstraints = [...sketchConstraints];
			}
		}

		sketchSolveStatus = {
			status: msg.status,
			dof: msg.dof ?? -1,
			failed: msg.failed || [],
			solveTime: msg.solveTime
		};
		recomputeOverConstrained();
		log('engine', 'Sketch solved', { status: msg.status, dof: msg.dof });
	});

	bridge.on('error', (msg) => {
		lastError = msg.message;
		statusMessage = `Error: ${msg.message}`;
		if (msg.needsRestart) {
			engineReady = false;
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

		// Check for auto-save data (legacy localStorage)
		if (typeof localStorage !== 'undefined') {
			const saved = localStorage.getItem(AUTOSAVE_KEY);
			const savedTime = localStorage.getItem(AUTOSAVE_TIME_KEY);
			if (saved && savedTime) {
				autoRestoreState = { available: true, timestamp: parseInt(savedTime, 10) };
			}
		}

		// If no localStorage restore found, check IndexedDB for most recently modified doc
		if (!autoRestoreState) {
			try {
				const { getStore } = await import('$lib/storage/index.js');
				const local = getStore();
				const docs = await local.list();
				if (docs.length > 0) {
					// docs are sorted by modified desc — first is most recent
					const newest = docs[0];
					autoRestoreState = { available: true, timestamp: newest.modified, source: 'indexeddb', docId: newest.id };
				}
			} catch {
				// IndexedDB not available or empty — no restore
			}
		}

		// Ensure activeDocId is set so saveToProvider() works on direct `/` navigation
		if (!activeDocId) {
			const { generateDocId } = await import('$lib/storage/types.js');
			activeDocId = generateDocId();
		}

		// Initialize default tab state if no document was loaded.
		// Use a real UUID (not a literal like 'default') so the document
		// round-trips through the Rust loader, which historically required
		// tab ids to be parseable — see load.rs / metadata.rs Tab.id.
		if (documentTabs.length === 0) {
			const tabId = generateUUID();
			documentTabs = [{ id: tabId, name: 'Part 1', kind: { type: 'Part', features: { features: [], active_index: null } } }];
			activeTabId = tabId;
		}
	} catch (err) {
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
				entityCount: sketchEntities.length,
				lastError,
				statusMessage,
			}),
			getEntities: () => [...sketchEntities],
			getPositions: () => new Map(sketchPositions),
			enterSketch: (origin, normal) => enterSketchMode(origin, normal),
			exitSketch: () => exitSketchMode(),
			setTool: (tool) => setActiveTool(tool),
			finishSketch: () => finishSketch(),
			getFeatureTree: () => featureTree,
			getSelectedFeatureId: () => selectedFeatureId,
			getMeshes: () => meshes.map(m => ({
				featureId: m.featureId,
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
				})),
			})),
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
			exportStl: () => exportStl(),
			exportBodyStl: (bodyId, name) => exportBodyStl(bodyId, name),
			exportStep: () => exportStep(),
			getCameraState: () => getCameraState(),
			getCameraProjection: () => getCameraProjection(),
			setCameraProjection: (proj) => setCameraProjection(proj),
			getConstraints: () => [...sketchConstraints],
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
			addExtrudeRegion: (sketchId, sketchName, profileIndex) => addExtrudeRegion(sketchId, sketchName, profileIndex),
			removeExtrudeRegion: (index) => removeExtrudeRegion(index),
			changeExtrudeSketch: (sketchId) => changeExtrudeSketch(sketchId),
			getRevolveDialogState: () => revolveDialogState,
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
			getSelectedRefs: () => [...selectedRefs],
			getHoveredRef: () => hoveredRef,
			selectRef: (ref, additive) => selectRef(ref, additive),
			clearSelection: () => clearSelection(),
			setHoveredRef: (ref) => setHoveredRef(ref),
			getBoxSelectState: () => ({ ...boxSelectState }),
			getSelectOtherState: () => ({ ...selectOtherState }),
			getRebuildTime: () => rebuildTime,
			getDimensionPopup: () => dimensionPopup ? { ...dimensionPopup } : null,
			showDimensionPopup: (popup) => showDimensionPopup(popup),
			hideDimensionPopup: () => hideDimensionPopup(),
			applyDimensionFromPopup: (value) => applyDimensionFromPopup(value),
			getSnapIndicator: () => getSnapIndicator(),
			getSnapCandidates: () => _getSnapCandidates(),
			getSnapSettings: () => getSnapSettings(),
			updateSnapSettings: (updates) => updateSnapSettings(updates),
			sketchToScreenOffset: (sx, sy) => {
				const sm = sketchMode;
				if (!sm?.active) return null;
				const cam = cameraObject;
				const canvas = document.querySelector('canvas');
				if (!cam || !canvas) return null;
				const plane = buildSketchPlane(sm.origin, sm.normal);
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
				const results = [];
				for (const mesh of meshes) {
					if (!mesh.faceRanges) continue;
					for (const range of mesh.faceRanges) {
						if (!range.geom_ref) continue;
						const i0 = mesh.indices[range.start_index];
						const i1 = mesh.indices[range.start_index + 1];
						const i2 = mesh.indices[range.start_index + 2];
						const cx = (mesh.vertices[i0*3] + mesh.vertices[i1*3] + mesh.vertices[i2*3]) / 3;
						const cy = (mesh.vertices[i0*3+1] + mesh.vertices[i1*3+1] + mesh.vertices[i2*3+1]) / 3;
						const cz = (mesh.vertices[i0*3+2] + mesh.vertices[i1*3+2] + mesh.vertices[i2*3+2]) / 3;
						const v = cam.position.clone().set(cx, cy, cz).project(cam);
						const screenX = (v.x * 0.5 + 0.5) * rect.width + rect.left;
						const screenY = (-v.y * 0.5 + 0.5) * rect.height + rect.top;
						results.push({ geomRef: range.geom_ref, screenX, screenY, behindCamera: v.z > 1 });
					}
				}
				return results.filter(r => !r.behindCamera);
			},
			isProjectToolActive: () => isProjectToolActive(),
			getProjectName: () => getProjectName(),
			setProjectName: (name) => setProjectName(name),
			getAutoRestoreState: () => getAutoRestoreState(),
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
			addSketchEntity: (entity) => addLocalEntity(entity),
			addSketchConstraint: (constraint) => addLocalConstraint(constraint),
			removeSketchEntities: (ids) => removeSketchEntities(new Set(ids)),
			createGear: (params) => createGear(params),
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
			finalizeDrag: () => finalizeDrag(),
			undo: () => undo(),
			redo: () => redo(),
			getLogs: (filter) => getLogs(filter),
			exportLogs: (filter) => exportLogs(filter),
			clearLogs: () => clearLogs(),
			showToast: (level, message, durationMs) => showToast(level, message, durationMs),
			getToasts: () => getToasts(),
			dismissAllToasts: () => { for (const t of getToasts()) dismissToast(t.id); },
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
				activeDocId,
				activeTabId,
				documentTabs: documentTabs.map(t => ({ id: t.id, name: t.name, kind: t.kind?.type })),
				documentName,
			}),
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
	return meshes.map((m) => {
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

export function getRebuildTime() {
	return rebuildTime;
}

export function isRebuilding() {
	return rebuilding;
}

export function getStatusMessage() {
	return statusMessage;
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
 * Select a geometry reference. Supports multi-select with additive flag.
 * @param {any | null} ref
 * @param {boolean} additive - If true, toggle selection; if false, replace selection
 */
export function selectRef(ref, additive = false) {
	// Intercept face clicks when in profile pick mode (extrude)
	if (profilePickMode?.target === 'extrude' && ref?.kind?.type === 'Face') {
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
		canonicalJson(a.selector) === canonicalJson(b.selector)
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
	const plane = buildSketchPlane(origin, normal);

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
		const srcPlane = buildSketchPlane(sOrigin, sNormal);

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
		const datumId = generateUUID();
		try {
			await bridge.send({
				type: 'BeginSketch',
				plane: {
					kind: { type: 'Face' },
					anchor: { type: 'Datum', datum_id: datumId },
					selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
					policy: { type: 'BestEffort' },
				}
			});
		} catch (err) {
			log('error', `BeginSketch failed: ${err}`);
			statusMessage = 'Failed to start sketch';
			return;
		}
	}

	sketchMode = { active: true, origin, normal };

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
		window.dispatchEvent(new CustomEvent('waffle-align-to-plane', { detail: { origin, normal } }));
	}
}

/**
 * Exit sketch mode.
 */
export function exitSketchMode() {
	log('action', 'Exit sketch mode');
	editingSketchFeatureId = null;
	resetSketchState();
	sketchMode = { active: false, origin: [0, 0, 0], normal: [0, 0, 1] };
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
	activeTool = tool;
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
 * Begin recording a sketch action (for undo grouping).
 * Call before a tool creates entities/constraints.
 */
export function beginSketchAction() {
	pendingSketchAction = { entities: [], constraints: [] };
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
		sketchUndoStack = [...sketchUndoStack, { entities: [cloned], constraints: [] }];
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

/**
 * Map a JS sketch constraint to the Rust bridge format.
 * Some constraint type names differ between JS (libslvs) and Rust (waffle-types).
 * @param {object} c - Constraint in JS format
 * @returns {object | null} Constraint in Rust bridge format, or null to skip
 */
function mapConstraintForBridge(c) {
	if (c.type === 'WhereDragged') {
		return { type: 'Dragged', point: c.point };
	}
	return c;
}

/**
 * Add a constraint locally and send to engine.
 * @param {object} constraint - SketchConstraint object
 */
export function addLocalConstraint(constraint) {
	log('sketch', `Constraint added: ${constraint.type}`, { type: constraint.type });
	sketchConstraints = [...sketchConstraints, constraint];
	recomputeOverConstrained();

	// Record for sketch undo
	const cloned = JSON.parse(JSON.stringify(constraint));
	if (pendingSketchAction) {
		pendingSketchAction.constraints.push(cloned);
	} else if (sketchMode.active) {
		sketchUndoStack = [...sketchUndoStack, { entities: [], constraints: [cloned] }];
		sketchRedoStack = [];
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
			_isDeletion: true
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
		_isDeletion: true
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

async function expandGearForDisplay(gearId, gearParams) {
	// Deep-clone: gearParams may be a Svelte reactive proxy (e.g. from a loaded
	// Gear entity), which postMessage cannot structured-clone to the worker.
	const params = JSON.parse(JSON.stringify(gearParams));
	const response = await bridge.send({ type: 'GenerateGearProfile', params });
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

/** @returns {Map<string, object>} */
export function getInactiveGearDisplay() { return inactiveGearDisplay; }

/**
 * Ensure every gear in the given inactive sketches is expanded into
 * `inactiveGearDisplay`, and drop entries no longer present. Idempotent.
 * @param {Array<{ key: string, entityId: number, params: object }>} specs
 */
export async function ensureInactiveGearsExpanded(specs) {
	const wanted = new Set(specs.map(s => s.key));
	let changed = false;
	const next = new Map(inactiveGearDisplay);
	for (const k of [...next.keys()]) {
		if (!wanted.has(k)) { next.delete(k); changed = true; }
	}
	for (const { key, entityId, params } of specs) {
		if (next.has(key)) continue;
		const p = JSON.parse(JSON.stringify(params));
		const response = await bridge.send({ type: 'GenerateGearProfile', params: p });
		// Per-gear id range, distinct from the active `gearDisplay` range.
		next.set(key, remapGearResponse(response, 50_000_000 + entityId * 100_000));
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

/** @returns {boolean} */
export function getPlanetaryDialogOpen() { return planetaryDialogOpen; }

/** Show the planetary gear dialog. */
export function showPlanetaryDialog() { planetaryDialogOpen = true; }

/** Hide the planetary gear dialog. */
export function hidePlanetaryDialog() { planetaryDialogOpen = false; }

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
	const gearId = nextGearId++;

	// Build the (non-persisted) display expansion from the gear params.
	await expandGearForDisplay(gearId, gearParams);

	// Store the single compact Gear entity — this is the persisted representation.
	const gearEntityId = allocEntityId();
	addLocalEntity({
		type: 'Gear',
		id: gearEntityId,
		params: { ...gearParams },
		construction: false
	});

	// Register gear: one entity id per gear (not a list of expanded primitives).
	const newRegistry = new Map(gearRegistry);
	newRegistry.set(gearId, { ...gearParams, entityId: gearEntityId });
	gearRegistry = newRegistry;

	const newEntityMap = new Map(entityToGearMap);
	newEntityMap.set(gearEntityId, gearId);
	entityToGearMap = newEntityMap;

	log('sketch', `Gear created: ${gearParams.toothCount} teeth, module ${gearParams.module}`, { gearId });
	return gearId;
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

	// Add all N+2 gears as a SINGLE undo step.
	beginSketchAction();
	const gearIds = [];
	for (const g of result.gears) {
		// `g` is a GearParams (camelCase serde) — the shape createGear expects.
		gearIds.push(await addGearFromParams({ ...g }));
	}
	endSketchAction();

	log('sketch', `Planetary stage created: ${result.gears.length} gears (ring ${result.ringTeeth}t)`);
	return { gearIds, result };
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
	delete mergedParams.entityId;
	const newGearId = await createGear(mergedParams);

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
 * Begin dragging a sketch point. Adds a temporary WhereDragged constraint
 * and updates the point position, triggering a solve.
 * @param {number} pointId
 * @param {number} newX
 * @param {number} newY
 */
export function dragSketchPoint(pointId, newX, newY) {
	if (!dragState) {
		// First drag move — record original position
		const pos = sketchPositions.get(pointId);
		if (!pos) return;
		dragState = { pointId, originalX: pos.x, originalY: pos.y };
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

	const { pointId } = dragState;

	// Remove temporary drag constraint
	sketchConstraints = sketchConstraints.filter(c => !(c.type === 'WhereDragged' && c.point === pointId && c._isDrag));

	// Trigger final solve without the drag constraint
	triggerSolve();
	reExtractProfiles();

	dragState = null;
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
	const gearEntities = sketchEntities.filter(e => e.type === 'Gear');
	for (const ge of gearEntities) {
		const gearId = nextGearId++;
		await expandGearForDisplay(gearId, ge.params);
		const nextReg = new Map(gearRegistry);
		nextReg.set(gearId, { ...ge.params, entityId: ge.id });
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

/**
 * Minimal sketch faces (regions) per sketch feature id, computed in Rust via
 * the ComputeRegions query. Each region is annotated with its `_index`.
 * @type {Map<string, Array<object>>}
 */
let sketchRegions = $state(new Map());

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
			if (e.type === 'Gear') gearSpecs.push({ key: `${f.id}:${e.id}`, entityId: e.id, params: e.params });
		}
	}
	if (gearSpecs.length) await ensureInactiveGearsExpanded(gearSpecs);
	const gears = getInactiveGearDisplay();

	const next = new Map();
	for (const feature of tree.features) {
		if (feature.operation?.type !== 'Sketch') continue;
		const sketch = feature.operation.sketch;
		const entities = [];
		const solved_positions = {};
		for (const e of (sketch.entities || [])) {
			if (e.type === 'Gear') {
				// Substitute the gear's cached primitive expansion (teeth + points).
				const exp = gears.get(`${feature.id}:${e.id}`);
				if (exp) {
					for (const ge of exp.entities) {
						entities.push(ge);
						if (ge.type === 'Point' && ge.id != null) solved_positions[ge.id] = [ge.x, ge.y];
					}
				}
			} else {
				entities.push(e);
				if (e.type === 'Point' && e.id != null) solved_positions[e.id] = [e.x, e.y];
			}
		}
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
	} else {
		// Default: auto-add profile 0
		regions = [{ type: 'sketchProfile', sketchId: lastSketch.id, sketchName: lastSketch.name, profileIndex: 0 }];
	}

	log('ui', 'Show extrude dialog', { sketchId: lastSketch.id, profileCount, regionCount: regions.length });
	extrudeDialogState = {
		sketchId: lastSketch.id,
		sketchName: lastSketch.name,
		profileCount,
		availableSketches: allSketches,
		regions
	};
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

/** Stable-ish identity for a sub-region (first outer vertex + area). */
function regionKey(region) {
	const p = region.outer?.[0] ?? [0, 0];
	return `${p[0].toFixed(6)},${p[1].toFixed(6)}:${(region.area ?? 0).toFixed(6)}`;
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
	const subRegion =
		region?.region && region.region.profile_entity_ids == null
			? JSON.parse(JSON.stringify(region.region))
			: null;

	const { depthMode = 'Blind', secondDir = 'None', secondDepth = 10, flipDirection = false } = opts;

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
			if (cut) {
				direction = [normal[0], normal[1], normal[2]];
			} else {
				direction = [-normal[0], -normal[1], -normal[2]];
			}
		} else {
			direction = cut ? [0, 0, 1] : [0, 0, -1];
		}
	}

	const operation = {
		type: 'Extrude',
		params: {
			sketch_id: effectiveSketchId,
			profile_index: effectiveProfileIndex,
			depth,
			direction,
			symmetric: secondDir === 'Symmetric',
			cut: !!cut,
			target_body: null,
			depth_mode,
			second_direction,
			region: subRegion
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
}

/**
 * Apply a revolve operation from the dialog.
 * @param {number} angleDeg - angle in degrees
 * @param {[number,number,number]} axisOrigin
 * @param {[number,number,number]} axisDir
 * @param {number} profileIndex
 */
export async function applyRevolve(angleDeg, axisOrigin, axisDir, profileIndex) {
	if (!revolveDialogState || !bridge || !engineReady) return;

	const operation = {
		type: 'Revolve',
		params: {
			sketch_id: revolveDialogState.sketchId,
			profile_index: profileIndex,
			axis_origin: axisOrigin,
			axis_direction: axisDir,
			angle: angleDeg
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

		revolveDialogState = null;
		revolvePreviewParams = null;
	} catch (err) {
		log('error', `Revolve failed: ${err.message}`);
		showToast('error', `Revolve failed: ${err.message}`);
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

	// Find features that produce solid bodies
	const bodies = tree.features
		.filter(f => ['Extrude', 'Revolve', 'BooleanCombine', 'Chamfer', 'Fillet', 'Shell'].includes(f.operation?.type))
		.map(f => ({ featureId: f.id, name: f.name }));

	log('ui', 'Show boolean dialog', { bodyCount: bodies.length });
	booleanDialogState = { bodies };
}

export function hideBooleanDialog() {
	booleanDialogState = null;
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

	// Serialize positions map to plain object with string keys
	const posObj = {};
	for (const [id, pos] of sketchPositions) {
		posObj[id] = [pos.x, pos.y];
	}

	// Convert extractedProfiles to the ClosedProfile format.
	// The profile extraction stores line/arc/spline entity IDs, but the kernel expects
	// point IDs (looked up in solved_positions). Convert by chaining entity endpoints.

	// Helper: get the two connection-point IDs (start, end) for any edge entity.
	function entityEndpoints(entity) {
		if (entity.type === 'Line' || entity.type === 'Arc') {
			return [entity.start_id, entity.end_id];
		}
		if (entity.type === 'Spline' && entity.point_ids?.length >= 2) {
			return [entity.point_ids[0], entity.point_ids[entity.point_ids.length - 1]];
		}
		return [undefined, undefined];
	}

	// Helper: get the start point ID for an entity (1 point per entity).
	// Each entity contributes exactly 1 point to the polygon; the end point
	// is the next entity's start. Spline curve geometry is communicated
	// via spline_segments (not by dumping all interior control points).
	function entityStartPoint(entity, forward) {
		if (entity.type === 'Line' || entity.type === 'Arc') {
			return forward ? entity.start_id : entity.end_id;
		}
		if (entity.type === 'Spline' && entity.point_ids?.length >= 2) {
			return forward ? entity.point_ids[0] : entity.point_ids[entity.point_ids.length - 1];
		}
		return undefined;
	}

// Synthetic point ID counter for arc samples (high value to avoid collision with real IDs)
	let nextSynthId = 900000;

	const profiles = extractedProfilesState.map((p) => {
		const pointIds = [];
		const arcSegments = [];
		const edgeEntities = [...p.entityIds].map(id => sketchEntities.find(e => e.id === id)).filter(Boolean);

		// Standalone circles: pass as tagged circle profile for true NURBS cylinder extrusion
		if (edgeEntities.length === 1 && edgeEntities[0].type === 'Circle') {
			const circle = edgeEntities[0];
			const center = sketchPositions.get(circle.center_id);
			if (center) {
				return {
					entity_ids: [circle.id],
					is_outer: p.isOuter,
					circle: { center_u: center.x, center_v: center.y, radius: circle.radius }
				};
			}
			return { entity_ids: [...p.entityIds], is_outer: p.isOuter };
		}

		if (edgeEntities.length === 0) return { entity_ids: [...p.entityIds], is_outer: p.isOuter };

		// Chain entities into a dense polygon. Splines contribute ALL their sample
		// points; arcs are sampled into intermediate points. This preserves involute
		// curve geometry for gear profiles (and any other curved profiles).
		const [firstStart, firstEnd] = entityEndpoints(edgeEntities[0]);
		if (firstStart == null) return { entity_ids: [...p.entityIds], is_outer: p.isOuter };

		// Helper: add all points for an entity (dense sampling) in the given direction.
		// Adds all points EXCEPT the last one (next entity's start handles it).
		function addEntityPoints(entity, forward) {
			if (entity.type === 'Spline' && entity.point_ids?.length >= 2) {
				// Spline: add ALL sample points (involute curves have 12+ points)
				const pts = forward ? entity.point_ids : [...entity.point_ids].reverse();
				for (const pid of pts.slice(0, -1)) {
					pointIds.push(pid);
				}
			} else if (entity.type === 'Arc') {
				// Arc: sample the curve into intermediate points
				const sId = forward ? entity.start_id : entity.end_id;
				const eId = forward ? entity.end_id : entity.start_id;
				const center = sketchPositions.get(entity.center_id);
				const sPos = sketchPositions.get(sId);
				const ePos = sketchPositions.get(eId);
				if (center && sPos && ePos) {
					const radius = Math.hypot(sPos.x - center.x, sPos.y - center.y);
					let startAngle = Math.atan2(sPos.y - center.y, sPos.x - center.x);
					let endAngle = Math.atan2(ePos.y - center.y, ePos.x - center.x);
					if (endAngle <= startAngle) endAngle += Math.PI * 2;

					const arcStartIdx = pointIds.length;
					pointIds.push(sId); // start point
					const ARC_SAMPLES = 16;
					for (let s = 1; s < ARC_SAMPLES; s++) {
						const t = s / ARC_SAMPLES;
						const angle = startAngle + t * (endAngle - startAngle);
						const synthId = nextSynthId++;
						posObj[synthId] = [
							center.x + Math.cos(angle) * radius,
							center.y + Math.sin(angle) * radius
						];
						pointIds.push(synthId);
					}
					const arcEndIdx = pointIds.length - 1;
					// Record arc metadata for cylindrical face assignment
					arcSegments.push({
						start_vertex_index: arcStartIdx,
						end_vertex_index: arcEndIdx,
						center_u: center.x,
						center_v: center.y,
						radius: radius,
					});
					// Don't push end point — next entity's start handles it
				} else {
					// Fallback: just add start point
					pointIds.push(sId);
				}
			} else {
				// Line or unknown: single start point
				const pt = entityStartPoint(entity, forward);
				if (pt != null) pointIds.push(pt);
			}
		}

		// First entity
		addEntityPoints(edgeEntities[0], true);
		let prevEnd = firstEnd;

		for (let i = 1; i < edgeEntities.length; i++) {
			const entity = edgeEntities[i];
			const [nextStart, nextEnd] = entityEndpoints(entity);
			if (nextStart == null) continue;

			const forward = nextStart === prevEnd;
			const connected = forward || nextEnd === prevEnd;
			const dir = connected ? forward : true;

			addEntityPoints(entity, dir);
			prevEnd = connected ? (forward ? nextEnd : nextStart) : nextEnd;
		}

		const result = { entity_ids: [...p.entityIds], is_outer: p.isOuter, vertex_ids: pointIds };
		if (arcSegments.length > 0) {
			result.arc_segments = arcSegments;
		}
		return result;
	});

	const profileCount = profiles.length;

	// Capture plane geometry before exiting sketch mode (spread to unwrap proxies)
	const planeOrigin = [...sketchMode.origin];
	const planeNormal = [...sketchMode.normal];

	log('action', 'Finish sketch', { entityCount: sketchEntities.length, profileCount, editing: !!editingSketchFeatureId });

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
					entities: sketchEntities,
					constraints: sketchConstraints.filter(c => c.type !== 'WhereDragged'),
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
			entities: JSON.parse(JSON.stringify(sketchEntities)),
			constraints: JSON.parse(JSON.stringify(
				sketchConstraints.filter(c => c.type !== 'WhereDragged')
			)),
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
export function setCameraRefs(camera, controls) {
	cameraObject = camera;
	controlsObject = controls;
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

// -- Dimension popup --

export function getDimensionPopup() { return dimensionPopup; }

/**
 * Show the dimension input popup.
 * @param {{ entityA: number, entityB: number | null, sketchX: number, sketchY: number, dimType: 'distance'|'radius'|'angle', defaultValue: number }} popup
 */
export function showDimensionPopup(popup) { dimensionPopup = popup; }

export function hideDimensionPopup() { dimensionPopup = null; }

/**
 * Apply the dimension value from the popup as a constraint.
 * @param {number} value
 */
export function applyDimensionFromPopup(value) {
	if (!dimensionPopup) return;
	const p = dimensionPopup;

	// Custom callback takes priority over built-in dimType handling
	if (p.customApply) {
		dimensionPopup = null;
		p.customApply(value);
		return;
	}

	if (p.dimType === 'distance') {
		if (p.entityB != null) {
			addLocalConstraint({ type: 'Distance', entity_a: p.entityA, entity_b: p.entityB, value });
		} else {
			// Single line — distance between endpoints
			const entity = sketchEntities.find(e => e.id === p.entityA);
			if (entity && entity.type === 'Line') {
				addLocalConstraint({ type: 'Distance', entity_a: entity.start_id, entity_b: entity.end_id, value });
			}
		}
	} else if (p.dimType === 'pointLineDistance') {
		addLocalConstraint({ type: 'PointLineDistance', point: p.entityA, entity: p.entityB, value });
	} else if (p.dimType === 'radius') {
		// libslvs uses Diameter constraint; convert radius to diameter
		addLocalConstraint({ type: 'Diameter', entity: p.entityA, value: value * 2 });
	} else if (p.dimType === 'angle') {
		if (p.entityB != null) {
			addLocalConstraint({ type: 'Angle', line_a: p.entityA, line_b: p.entityB, value_degrees: value });
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
	resetSketchState();

	// Repopulate sketch state from saved data
	sketchEntities = JSON.parse(JSON.stringify(sketch.entities || []));
	sketchConstraints = JSON.parse(JSON.stringify(sketch.constraints || []));

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
	sketchMode = { active: true, origin, normal };

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
		window.dispatchEvent(new CustomEvent('waffle-align-to-plane', { detail: { origin, normal } }));
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

// -- Project name --

export function getProjectName() { return projectName; }
/** @param {string} name */
export function setProjectName(name) { projectName = name; documentName = name; }

// -- Document display unit --

export function getDocumentDisplayUnit() { return documentDisplayUnit; }
/** @param {string} unit */
export function setDocumentDisplayUnit(unit) {
	documentDisplayUnit = unit;
	// Notify engine so it persists in save
	if (bridge && engineReady) {
		bridge.send({ type: 'SetDisplayUnit', unit }).catch(() => {});
	}
}

// -- Document model --

export function getActiveDocId() { return activeDocId; }
export function getActiveTabId() { return activeTabId; }
export function getDocumentTabs() { return documentTabs; }
export function getDocumentName() { return documentName; }
export function setDocumentName(name) { documentName = name; projectName = name; }

/**
 * Check sessionStorage for a pending document (set by /doc/[id] route) and load it.
 * Called from +page.svelte onMount — separate from initEngine because SvelteKit
 * client-side navigation means the layout onMount (which calls initEngine) doesn't re-fire.
 */
export async function loadPendingDocument() {
	if (typeof sessionStorage === 'undefined') return;
	const pendingDocId = sessionStorage.getItem('waffle-active-doc');
	const pendingJson = sessionStorage.getItem('waffle-active-json');
	if (!pendingDocId || !pendingJson) return;

	sessionStorage.removeItem('waffle-active-doc');
	sessionStorage.removeItem('waffle-active-json');

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
		const parsed = JSON.parse(pendingJson);
		initDocumentState(pendingDocId, parsed);
		// Load the active tab's features into the engine
		const activeTab = documentTabs.find(t => t.id === activeTabId);
		const tabFeatures = activeTab?.kind?.features;
		if (tabFeatures?.features?.length > 0) {
			await loadProject(pendingJson);
		} else if (bridge && engineReady) {
			// Empty document — clear the engine's stale model
			await sendRebuild({
				type: 'SwitchTab',
				features: { features: [], active_index: null }
			});
		}
		log('system', `Loaded document ${pendingDocId}`);
	} catch (err) {
		log('error', `Failed to load pending document: ${err}`);
	}
}

/**
 * Switch to a different tab. Saves current tab's feature tree, then loads the target tab.
 * @param {string} tabId
 */
export async function switchTab(tabId) {
	if (tabId === activeTabId) return;
	if (!documentTabs.find(t => t.id === tabId)) return;

	// Cancel pending autosave to prevent stale state capture
	if (autoSaveTimer) {
		clearTimeout(autoSaveTimer);
		autoSaveTimer = null;
	}

	// Save current tab's features before switching
	if (activeTabId) {
		const currentTab = documentTabs.find(t => t.id === activeTabId);
		if (currentTab) {
			currentTab.kind.features = JSON.parse(JSON.stringify(featureTree));
		}
	}

	// Load target tab's features
	const targetTab = documentTabs.find(t => t.id === tabId);
	activeTabId = tabId;

	if (bridge && engineReady && targetTab?.kind?.features) {
		// Deep-clone to unwrap Svelte 5 proxies (they can't be postMessage'd)
		const features = JSON.parse(JSON.stringify(targetTab.kind.features));
		await sendRebuild({
			type: 'SwitchTab',
			features
		});
	}

	scheduleAutoSave();
}

/**
 * Add a new tab to the document.
 * @returns {string} The new tab's ID
 */
export function addTab() {
	const id = generateUUID();
	const name = `Part ${documentTabs.length + 1}`;
	documentTabs = [...documentTabs, {
		id,
		name,
		kind: { type: 'Part', features: { features: [], active_index: null } }
	}];
	return id;
}

/**
 * Close a tab by ID. If the active tab is closed, switch to an adjacent tab.
 * @param {string} tabId
 */
export async function closeTab(tabId) {
	if (documentTabs.length <= 1) return; // Don't close the last tab

	const idx = documentTabs.findIndex(t => t.id === tabId);
	if (idx === -1) return;

	documentTabs = documentTabs.filter(t => t.id !== tabId);

	if (activeTabId === tabId) {
		// Switch to adjacent tab (prefer left neighbor, else right)
		const newIdx = Math.min(idx, documentTabs.length - 1);
		await switchTab(documentTabs[newIdx].id);
	}

	scheduleAutoSave();
}

/**
 * Rename a tab.
 * @param {string} tabId
 * @param {string} name
 */
export function renameTab(tabId, name) {
	documentTabs = documentTabs.map(t =>
		t.id === tabId ? { ...t, name } : t
	);
	scheduleAutoSave();
}

/**
 * Initialize document state from a v3 document JSON.
 * Called when loading a document from IndexedDB or creating a new one.
 * @param {string} docId
 * @param {object} parsed - Parsed v3 JSON
 */
export function initDocumentState(docId, parsed) {
	activeDocId = docId;
	documentName = parsed.document?.name || 'Untitled';
	projectName = documentName;

	if (parsed.tabs && parsed.tabs.length > 0) {
		documentTabs = parsed.tabs.map(t => ({
			id: t.id,
			name: t.name,
			kind: t.kind || { type: 'Part', features: { features: [], active_index: null } }
		}));
		activeTabId = parsed.active_tab || parsed.tabs[0].id;
	} else {
		// Legacy v1/v2 — single implicit tab
		const tabId = generateUUID();
		documentTabs = [{ id: tabId, name: 'Part 1', kind: { type: 'Part', features: { features: [], active_index: null } } }];
		activeTabId = tabId;
	}
}

/**
 * Build full v3 JSON from current document state for saving.
 * Includes all tabs (inactive ones from documentTabs, active one from live engine state).
 * @returns {Promise<string | null>}
 */
export async function buildDocumentJson() {
	// Get current active tab's features from the engine via SaveProject.
	// The engine returns v3 JSON with features inside tabs[0].kind.features.
	const liveJson = await saveProjectToString();
	let liveFeatures = null;
	if (liveJson) {
		try {
			const parsed = JSON.parse(liveJson);
			// v3 format: features are in tabs[0].kind.features
			if (parsed.tabs?.[0]?.kind?.features) {
				liveFeatures = parsed.tabs[0].kind.features;
			}
			// v2 fallback: features at project.features
			else if (parsed.project?.features) {
				liveFeatures = parsed.project.features;
			}
			// v1 fallback: feature_tree at top level
			else if (parsed.feature_tree) {
				liveFeatures = { features: parsed.feature_tree.features || [], active_index: parsed.feature_tree.active_index ?? null };
			}
		} catch { /* ignore */ }
	}

	const now = new Date().toISOString();
	// Deep-clone documentTabs to unwrap Svelte 5 proxies
	const tabSnapshot = JSON.parse(JSON.stringify(documentTabs));
	const tabs = tabSnapshot.map(t => {
		const features = (t.id === activeTabId && liveFeatures)
			? liveFeatures
			: (t.kind?.features || { features: [], active_index: null });
		return {
			id: t.id,
			name: t.name,
			kind: { type: t.kind?.type || 'Part', features, preview_mesh: t.kind?.preview_mesh || null }
		};
	});

	const doc = {
		format: 'waffle-iron',
		version: 3,
		document: {
			name: documentName,
			created: now,
			modified: now,
			display_unit: documentDisplayUnit
		},
		tabs,
		active_tab: activeTabId
	};

	return JSON.stringify(doc);
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

export async function restoreAutoSave() {
	// Restore from IndexedDB if that was the source
	if (autoRestoreState?.source === 'indexeddb' && autoRestoreState?.docId) {
		try {
			const { getStore } = await import('$lib/storage/index.js');
			const local = getStore();
			const doc = await local.get(autoRestoreState.docId);
			if (doc?.json) {
				activeDocId = autoRestoreState.docId;
				await loadProject(doc.json);
				autoRestoreState = null;
				return true;
			}
		} catch {
			// fall through
		}
		autoRestoreState = null;
		return false;
	}
	// Legacy localStorage restore
	if (typeof localStorage === 'undefined') return false;
	const saved = localStorage.getItem(AUTOSAVE_KEY);
	if (!saved) return false;
	const savedName = localStorage.getItem(AUTOSAVE_NAME_KEY);
	if (savedName) projectName = savedName;
	await loadProject(saved);
	autoRestoreState = null;
	return true;
}

export async function discardAutoSave() {
	// Clear IndexedDB restore doc if that was the source
	if (autoRestoreState?.source === 'indexeddb' && autoRestoreState?.docId) {
		try {
			const { getStore } = await import('$lib/storage/index.js');
			const local = getStore();
			await local.delete(autoRestoreState.docId);
		} catch {
			// ignore cleanup errors
		}
	}
	if (typeof localStorage !== 'undefined') {
		localStorage.removeItem(AUTOSAVE_KEY);
		localStorage.removeItem(AUTOSAVE_TIME_KEY);
		localStorage.removeItem(AUTOSAVE_NAME_KEY);
	}
	autoRestoreState = null;
}

/**
 * Trigger a constraint solve via the libslvs solver in the worker.
 * Sends current sketch state to the worker for solving.
 */
export function triggerSolve() {
	if (!bridge || !engineReady) return;
	if (!sketchMode.active) return;
	if (sketchEntities.length === 0) return;

	// Serialize positions map to plain object for postMessage (clone values to unwrap proxies)
	const posObj = {};
	for (const [id, pos] of sketchPositions) {
		posObj[id] = { x: pos.x, y: pos.y };
	}

	// Deep-clone reactive state to avoid DataCloneError from Svelte 5 proxies.
	// Gears are stored as a single compact `Gear` entity which the solver skips
	// natively (it is a rigid, fully-parametric block), so no gear filtering is
	// needed here.
	const entities = JSON.parse(JSON.stringify(sketchEntities));
	const constraints = JSON.parse(JSON.stringify(sketchConstraints));

	bridge
		.send({
			type: 'SolveSketchLocal',
			entities,
			constraints,
			positions: posObj
		})
		.catch(err => log('error', `SolveSketchLocal failed: ${err}`));
}

const AUTOSAVE_KEY = 'waffle-autosave';
const AUTOSAVE_TIME_KEY = 'waffle-autosave-time';
const AUTOSAVE_NAME_KEY = 'waffle-autosave-name';
const AUTOSAVE_DELAY_MS = 3000;

function scheduleAutoSave() {
	if (autoSaveTimer) clearTimeout(autoSaveTimer);
	autoSaveTimer = setTimeout(async () => {
		autoSaveTimer = null;
		try {
			await saveToProvider();
		} catch (err) {
			// If remote provider fails, fall back to local IndexedDB
			console.warn('Auto-save to provider failed, falling back to local:', err.message || err);
			try {
				const { getStore } = await import('$lib/storage/index.js');
				const local = getStore();
				if (activeDocId) {
					const jsonData = await buildDocumentJson();
					if (jsonData) {
						const existing = await local.get(activeDocId);
						await local.put({
							id: activeDocId,
							json: jsonData,
							created: existing?.created || Date.now(),
							modified: Date.now()
						});
					}
				}
			} catch {
				console.warn('Local fallback auto-save also failed');
			}
		}
	}, AUTOSAVE_DELAY_MS);
}

/**
 * Save current document state to the active storage provider.
 * Builds full v3 JSON including all tabs.
 */
async function saveToProvider() {
	if (!activeDocId) return;
	const jsonData = await buildDocumentJson();
	if (!jsonData) return;

	const { getActiveProvider } = await import('$lib/storage/index.js');
	const store = getActiveProvider();
	const existing = await store.get(activeDocId);
	await store.put({
		id: activeDocId,
		json: jsonData,
		created: existing?.created || Date.now(),
		modified: Date.now()
	});
}

/**
 * Save immediately to the active storage provider (for Ctrl+S).
 * @returns {Promise<boolean>}
 */
export async function saveToStorage() {
	if (!activeDocId) {
		// No active doc — fall back to file download
		return !!(await saveProject());
	}
	try {
		await saveToProvider();
		showToast('success', 'Saved');
		log('action', 'Document saved');
		return true;
	} catch (err) {
		showToast('error', `Save failed: ${err.message || err}`);
		return false;
	}
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
		assayBrowserState.visible = true;
		refreshAssayCases();
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
	const nextPos = new Map(sketchPositions);
	for (const e of action.entities) {
		if (e.type === 'Point') nextPos.delete(e.id);
	}
	sketchPositions = nextPos;

	// Remove action constraints + cascaded constraints
	const allRemovedJsons = new Set([
		...action.constraints.map(c => JSON.stringify(c)),
		...cascadedConstraints.map(c => JSON.stringify(c))
	]);
	sketchConstraints = sketchConstraints.filter(c => !allRemovedJsons.has(JSON.stringify(c)));

	// Push to redo stack with cascaded info for restore
	sketchRedoStack = [...sketchRedoStack, {
		entities: action.entities,
		constraints: action.constraints,
		cascadedConstraints
	}];

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

	// Push to undo stack (merge cascaded into constraints so undo removes them all)
	sketchUndoStack = [...sketchUndoStack, {
		entities: action.entities,
		constraints: allConstraints
	}];

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
	const response = await bridge.send({ type: 'SaveProject' });
	if (response.type !== 'SaveReady' || !response.json_data) return null;

	const jsonData = response.json_data;
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
function triggerStlDownload(stlBase64, filename) {
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
 * Sends ExportStep to engine, receives ExportReady { step_data },
 * and triggers download as 'model.step'.
 * @returns {Promise<boolean>} True if export succeeded
 */
export async function exportStep() {
	if (!bridge || !engineReady) return false;
	log('action', 'Export STEP');
	const response = await bridge.send({ type: 'ExportStep' });
	if (response.type !== 'ExportReady' || !response.step_data) return false;
	showToast('success', 'STEP exported');

	if (typeof document !== 'undefined') {
		const blob = new Blob([response.step_data], { type: 'application/step' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = `${projectName}.step`;
		document.body.appendChild(a);
		a.click();
		document.body.removeChild(a);
		URL.revokeObjectURL(url);
	}

	return true;
}

/**
 * Extract display_unit from .waffle JSON and set it on the store.
 * Falls back to 'mm' if not present (legacy v1 files).
 * @param {string} jsonData
 */
function extractDisplayUnit(jsonData) {
	try {
		const parsed = JSON.parse(jsonData);
		const unit = parsed?.project?.display_unit;
		if (unit && typeof unit === 'string') {
			documentDisplayUnit = unit;
		} else {
			documentDisplayUnit = 'mm';
		}
	} catch {
		documentDisplayUnit = 'mm';
	}
}

/**
 * Load a project from a .waffle/.json file (browser file picker).
 * Opens a hidden file input, reads the file, sends LoadProject { data } to engine.
 * The engine responds with ModelUpdated, which is handled by the existing callback.
 * @param {string} [jsonData] - Optional JSON string to load directly (for programmatic use)
 * @returns {Promise<boolean>} True if load was initiated
 */
export async function loadProject(jsonData) {
	if (!bridge || !engineReady) return false;

	log('action', 'Load project');
	if (jsonData) {
		extractDisplayUnit(jsonData);
		await sendRebuild({ type: 'LoadProject', data: jsonData });
		showToast('info', 'Project loaded');
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
			// Set project name from filename (remove extension)
			const nameWithoutExt = file.name.replace(/\.(waffle|json)$/i, '');
			if (nameWithoutExt) setProjectName(nameWithoutExt);
			const text = await file.text();
			extractDisplayUnit(text);
			try {
				await sendRebuild({ type: 'LoadProject', data: text });
				showToast('info', 'Project loaded');
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
