/**
 * Engine state store using Svelte 5 runes.
 *
 * Manages reactive state for the WASM engine, including
 * feature tree, mesh data, and engine status.
 */

import { EngineBridge } from './bridge.js';
import { log, getLogs, exportLogs, clearLogs } from './logger.js';
import { showToast, initLoggerToasts } from '$lib/ui/toast.svelte.js';
import { extractProfiles } from '$lib/sketch/profiles.js';
import { getPreview, getSnapIndicator, resetTool } from '$lib/sketch/tools.js';

/** @type {{ features: Array<any>, active_index: number | null }} */
let featureTree = $state({ features: [], active_index: null });

/** @type {Array<{ featureId: string, vertices: Float32Array, normals: Float32Array, indices: Uint32Array, triangleCount: number, faceRanges?: Array<{geom_ref: any, start_index: number, end_index: number}> }>} */
let meshes = $state([]);

let engineReady = $state(false);

/** @type {string | null} */
let lastError = $state(null);

let rebuildTime = $state(0);

let statusMessage = $state('Initializing...');

/** @type {any | null} */
let hoveredRef = $state(null);

/** @type {Array<any>} */
let selectedRefs = $state([]);

/** @type {{ active: boolean, origin: [number, number, number], normal: [number, number, number] }} */
let sketchMode = $state({ active: false, origin: [0, 0, 0], normal: [0, 0, 1] });

/** @type {string | null} */
let selectedFeatureId = $state(null);

/** @type {string} */
let activeTool = $state('select');

// -- Sketch drawing state --

/** @type {Array<object>} */
let sketchEntities = $state([]);

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

/** @type {{ sketchId: string, sketchName: string, profileCount: number } | null} */
let extrudeDialogState = $state(null);

/** @type {{ sketchId: string, sketchName: string, profileCount: number } | null} */
let revolveDialogState = $state(null);

/** @type {{ entityA: number, entityB: number | null, sketchX: number, sketchY: number, dimType: 'distance'|'radius'|'angle', defaultValue: number } | null} */
let dimensionPopup = $state(null);

// -- Sketch plane dialog state --

let sketchPlaneDialogVisible = $state(false);
/** @type {{ origin: [number,number,number], normal: [number,number,number], label: string } | null} */
let sketchPlaneDialogSelection = $state(null);

/** Configurable snap thresholds */
let snapSettings = $state({
	coincidentPx: 8,
	onEntityPx: 5,
	hvAngleDeg: 3
});

// -- Camera state refs (set by CameraControls) --

/** @type {import('three').PerspectiveCamera | null} */
let cameraObject = null;

/** @type {any | null} OrbitControls ref */
let controlsObject = null;

// -- Box selection state --

/** @type {{ active: boolean, startX: number, startY: number, endX: number, endY: number, mode: 'window'|'crossing' }} */
let boxSelectState = $state({ active: false, startX: 0, startY: 0, endX: 0, endY: 0, mode: 'window' });

// -- Select Other cycle state --

/** @type {{ intersections: Array<any>, cycleIndex: number, lastScreenX: number, lastScreenY: number }} */
let selectOtherState = $state({ intersections: [], cycleIndex: 0, lastScreenX: -1, lastScreenY: -1 });

// -- Mobile layout state --

let isMobileLayout = $state(false);
/** @type {'left' | 'right' | null} */
let mobileActivePanel = $state(null);

/** @type {EngineBridge | null} */
let bridge = null;

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
		log('engine', 'Model updated', { meshCount: meshes.length, featureCount: featureTree?.features?.length ?? 0 });
	});

	bridge.on('sketchSolved', (msg) => {
		if (msg.positions && msg.status !== 'not_ready' && msg.status !== 'solver_not_ready') {
			const newPositions = new Map();
			for (const [id, pos] of Object.entries(msg.positions)) {
				newPositions.set(Number(id), pos);
			}
			sketchPositions = newPositions;
			reExtractProfiles();
		}
		sketchSolveStatus = {
			status: msg.status,
			dof: msg.dof ?? -1,
			failed: msg.failed || [],
			solveTime: msg.solveTime
		};
		log('engine', 'Sketch solved', { status: msg.status, dof: msg.dof });
	});

	bridge.on('error', (msg) => {
		lastError = msg.message;
		statusMessage = `Error: ${msg.message}`;
	});

	log('system', 'Engine init started');
	try {
		statusMessage = 'Loading WASM engine...';
		await bridge.init('/pkg/wasm_bridge.js');
		engineReady = true;
		lastError = null;
		statusMessage = 'Engine ready';
		log('system', 'Engine ready (WASM loaded)');
		initLoggerToasts();
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
				})),
			})),
			computeFacePlane: (geomRef) => computeFacePlane(geomRef),
			applyExtrude: (depth, profileIndex, cut) => applyExtrude(depth, profileIndex, cut),
			showExtrudeDialog: () => showExtrudeDialog(),
			saveProject: () => saveProject(),
			loadProject: (jsonData) => loadProject(jsonData),
			exportStl: () => exportStl(),
			getCameraState: () => getCameraState(),
			getConstraints: () => [...sketchConstraints],
			getProfiles: () => [...extractedProfilesState],
			getExtrudeDialogState: () => extrudeDialogState,
			getRevolveDialogState: () => revolveDialogState,
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
			getPreview: () => getPreview(),
			getSolveStatus: () => sketchSolveStatus ? { ...sketchSolveStatus } : null,
			getOverConstrained: () => [...overConstrainedEntities],
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
			getSketchSelection: () => [...sketchSelection],
			setSketchSelection: (ids) => { sketchSelection = new Set(ids); },
			addSketchEntity: (entity) => addLocalEntity(entity),
			addSketchConstraint: (constraint) => addLocalConstraint(constraint),
			undo: () => undo(),
			redo: () => redo(),
			getLogs: (filter) => getLogs(filter),
			exportLogs: (filter) => exportLogs(filter),
			clearLogs: () => clearLogs(),
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
 * Get reactive engine state.
 */
export function getFeatureTree() {
	return featureTree;
}

export function getMeshes() {
	return meshes;
}

export function isEngineReady() {
	return engineReady;
}

export function getLastError() {
	return lastError;
}

export function getRebuildTime() {
	return rebuildTime;
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
	if (!ref) {
		selectedRefs = [];
		return;
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

/**
 * Check if a geom ref uses the JS-only DatumPlane anchor (not known to Rust engine).
 * @param {any} ref
 * @returns {boolean}
 */
function isDatumPlaneRef(ref) {
	return ref?.anchor?.type === 'DatumPlane';
}

/**
 * Clear all selections.
 */
export function clearSelection() {
	selectedRefs = [];
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
		a.selector?.type === b.selector?.type &&
		JSON.stringify(a.selector) === JSON.stringify(b.selector)
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
 * Enter sketch mode on a plane.
 * @param {[number, number, number]} origin - plane origin
 * @param {[number, number, number]} normal - plane normal
 */
export async function enterSketchMode(origin = [0, 0, 0], normal = [0, 0, 1]) {
	log('action', 'Enter sketch mode', { origin, normal });
	resetSketchState();

	// Notify the engine about the new sketch session
	if (bridge && engineReady) {
		// crypto.randomUUID() requires secure context (HTTPS/localhost).
		// Fall back to crypto.getRandomValues() which works everywhere.
		const datumId = typeof crypto.randomUUID === 'function'
			? crypto.randomUUID()
			: ([1e7]+-1e3+-4e3+-8e3+-1e11).replace(/[018]/g, c =>
				(c ^ crypto.getRandomValues(new Uint8Array(1))[0] & 15 >> c / 4).toString(16));
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

	// Dispatch event so CameraControls aligns to the sketch plane
	if (typeof window !== 'undefined') {
		window.dispatchEvent(new CustomEvent('waffle-align-to-plane', { detail: { origin, normal } }));
	}
}

/**
 * Exit sketch mode.
 */
export function exitSketchMode() {
	log('action', 'Exit sketch mode');
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

	reExtractProfiles();
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
		bridge.send({ type: 'AddConstraint', constraint: cloned })
			.catch(err => log('error', `AddConstraint failed: ${err}`));
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
 * Detect over-constrained entities by checking constraint count vs DOF.
 * Simple heuristic: count constraints per entity; if an entity has more
 * same-type constraints than allowed, flag it.
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

	// A line with >1 of the same H/V constraints is over-constrained
	// Also, H+V on same line = fully constrained direction (OK)
	// Simple: flag lines with >2 geometric constraints
	for (const entity of sketchEntities) {
		const count = constraintCount.get(entity.id) || 0;
		if (entity.type === 'Line' && count > 2) {
			overconstrained.add(entity.id);
		}
		// Points with >2 constraints (each constraint removes 1 DOF, point has 2 DOF)
		if (entity.type === 'Point' && count > 2) {
			overconstrained.add(entity.id);
		}
	}

	overConstrainedEntities = overconstrained;
}

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

export function getOverConstrainedEntities() { return overConstrainedEntities; }

export function getSketchCursorPos() { return sketchCursorPos; }
/** @param {{ x: number, y: number } | null} pos */
export function setSketchCursorPos(pos) { sketchCursorPos = pos; }

// -- Extrude dialog --

export function getExtrudeDialogState() { return extrudeDialogState; }

/**
 * Show the extrude dialog. Auto-selects the last sketch in the feature tree.
 */
export function showExtrudeDialog() {
	const tree = featureTree;
	if (!tree || !tree.features) return;

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
	log('ui', 'Show extrude dialog', { sketchId: lastSketch.id, profileCount });
	extrudeDialogState = {
		sketchId: lastSketch.id,
		sketchName: lastSketch.name,
		profileCount
	};
}

export function hideExtrudeDialog() {
	extrudeDialogState = null;
}

/**
 * Apply an extrude operation from the dialog.
 * @param {number} depth
 * @param {number} profileIndex
 * @param {boolean} [cut=false] - If true, perform a cut (subtract) operation
 */
export async function applyExtrude(depth, profileIndex, cut = false) {
	if (!extrudeDialogState || !bridge || !engineReady) return;

	log('action', 'Apply extrude', { depth, profileIndex, cut: !!cut });
	try {
		await bridge.send({
			type: 'AddFeature',
			operation: {
				type: 'Extrude',
				params: {
					sketch_id: extrudeDialogState.sketchId,
					profile_index: profileIndex,
					depth,
					direction: null,
					symmetric: false,
					cut: !!cut,
					target_body: null
				}
			}
		});

		extrudeDialogState = null;
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
	log('ui', 'Show revolve dialog', { sketchId: lastSketch.id, profileCount });
	revolveDialogState = {
		sketchId: lastSketch.id,
		sketchName: lastSketch.name,
		profileCount
	};
}

export function hideRevolveDialog() {
	revolveDialogState = null;
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

	log('action', 'Apply revolve', { angle: angleDeg, profileIndex });
	const angleRad = angleDeg * Math.PI / 180;

	try {
		await bridge.send({
			type: 'AddFeature',
			operation: {
				type: 'Revolve',
				params: {
					sketch_id: revolveDialogState.sketchId,
					profile_index: profileIndex,
					axis_origin: axisOrigin,
					axis_direction: axisDir,
					angle: angleRad
				}
			}
		});

		revolveDialogState = null;
	} catch (err) {
		log('error', `Revolve failed: ${err.message}`);
		showToast('error', `Revolve failed: ${err.message}`);
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

	// Handle datum planes directly
	if (geomRef.anchor?.type === 'DatumPlane') {
		const planes = {
			XY: { origin: /** @type {[number,number,number]} */ ([0, 0, 0]), normal: /** @type {[number,number,number]} */ ([0, 0, 1]) },
			XZ: { origin: /** @type {[number,number,number]} */ ([0, 0, 0]), normal: /** @type {[number,number,number]} */ ([0, 1, 0]) },
			YZ: { origin: /** @type {[number,number,number]} */ ([0, 0, 0]), normal: /** @type {[number,number,number]} */ ([1, 0, 0]) },
		};
		return planes[geomRef.anchor.plane] ?? null;
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
	// The profile extraction stores line/arc entity IDs, but the kernel expects
	// point IDs (looked up in solved_positions). Convert by chaining line endpoints.
	const profiles = extractedProfilesState.map((p) => {
		const pointIds = [];
		const lineEntities = [...p.entityIds].map(id => sketchEntities.find(e => e.id === id)).filter(Boolean);

		// Standalone circles: approximate as polygon with N points on circumference
		if (lineEntities.length === 1 && lineEntities[0].type === 'Circle') {
			const circle = lineEntities[0];
			const center = sketchPositions.get(circle.center_id);
			if (center) {
				const N = 32; // polygon approximation segments
				const circlePointIds = [];
				for (let ci = 0; ci < N; ci++) {
					const angle = (2 * Math.PI * ci) / N;
					const px = center.x + circle.radius * Math.cos(angle);
					const py = center.y + circle.radius * Math.sin(angle);
					const ptId = 90000 + circle.id * 100 + ci; // synthetic point IDs
					posObj[ptId] = [px, py];
					circlePointIds.push(ptId);
				}
				return { entity_ids: circlePointIds, is_outer: p.isOuter };
			}
			return { entity_ids: [...p.entityIds], is_outer: p.isOuter };
		}

		if (lineEntities.length === 0) return { entity_ids: [...p.entityIds], is_outer: p.isOuter };

		// Chain lines: find start point of each line following end→start connections
		let current = lineEntities[0];
		pointIds.push(current.start_id);
		let prevEnd = current.end_id;

		for (let i = 1; i < lineEntities.length; i++) {
			const next = lineEntities[i];
			// Determine direction: if next.start_id === prevEnd, use start_id; otherwise use end_id
			if (next.start_id === prevEnd) {
				pointIds.push(next.start_id);
				prevEnd = next.end_id;
			} else if (next.end_id === prevEnd) {
				pointIds.push(next.end_id);
				prevEnd = next.start_id;
			} else {
				// Not connected — just add start_id
				pointIds.push(next.start_id);
				prevEnd = next.end_id;
			}
		}

		return { entity_ids: pointIds, is_outer: p.isOuter };
	});

	const profileCount = profiles.length;

	// Capture plane geometry before exiting sketch mode (spread to unwrap proxies)
	const planeOrigin = [...sketchMode.origin];
	const planeNormal = [...sketchMode.normal];

	log('action', 'Finish sketch', { entityCount: sketchEntities.length, profileCount });

	// Send to engine FIRST, exit sketch mode only on success
	try {
		await bridge.send({
			type: 'FinishSketch',
			solved_positions: posObj,
			solved_profiles: profiles,
			plane_origin: planeOrigin,
			plane_normal: planeNormal
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
 * @param {import('three').PerspectiveCamera} camera
 * @param {any} controls - OrbitControls instance
 */
export function setCameraRefs(camera, controls) {
	cameraObject = camera;
	controlsObject = controls;
}

/**
 * Get camera state for tests and external access.
 * @returns {{ position: number[], target: number[], fov: number, up: number[] } | null}
 */
export function getCameraState() {
	if (!cameraObject) return null;
	const pos = cameraObject.position;
	const up = cameraObject.up;
	const target = controlsObject?.target;
	return {
		position: [pos.x, pos.y, pos.z],
		target: target ? [target.x, target.y, target.z] : [0, 0, 0],
		fov: cameraObject.fov,
		up: [up.x, up.y, up.z],
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
	} else if (p.dimType === 'radius') {
		addLocalConstraint({ type: 'Radius', entity: p.entityA, value });
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
 * @param {Partial<{ coincidentPx: number, onEntityPx: number, hvAngleDeg: number }>} updates
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
	sketchPlaneDialogVisible = true;
}

export function hideSketchPlaneDialog() {
	sketchPlaneDialogVisible = false;
	sketchPlaneDialogSelection = null;
}

export async function confirmSketchPlaneDialog() {
	if (!sketchPlaneDialogSelection) return;
	const sel = sketchPlaneDialogSelection;
	sketchPlaneDialogVisible = false;
	sketchPlaneDialogSelection = null;
	await enterSketchMode(sel.origin, sel.normal);
	setActiveTool('line');
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

	// Deep-clone reactive state to avoid DataCloneError from Svelte 5 proxies
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

// -- Engine commands --

/**
 * Delete a feature by ID.
 * @param {string} featureId
 */
export async function deleteFeature(featureId) {
	if (!bridge || !engineReady) return;
	log('action', 'Delete feature', { featureId });
	await bridge.send({ type: 'DeleteFeature', feature_id: featureId });
}

/**
 * Suppress or unsuppress a feature.
 * @param {string} featureId
 * @param {boolean} suppressed
 */
export async function suppressFeature(featureId, suppressed) {
	if (!bridge || !engineReady) return;
	log('action', 'Suppress feature', { featureId, suppressed });
	await bridge.send({ type: 'SuppressFeature', feature_id: featureId, suppressed });
}

/**
 * Set the rollback index.
 * @param {number | null} index
 */
export async function setRollbackIndex(index) {
	if (!bridge || !engineReady) return;
	await bridge.send({ type: 'SetRollbackIndex', index });
}

/**
 * Edit a feature's operation.
 * @param {string} featureId
 * @param {object} operation
 */
export async function editFeature(featureId, operation) {
	if (!bridge || !engineReady) return;
	await bridge.send({ type: 'EditFeature', feature_id: featureId, operation });
}

/**
 * Reorder a feature to a new position in the tree.
 * @param {string} featureId
 * @param {number} newPosition
 */
export async function reorderFeature(featureId, newPosition) {
	if (!bridge || !engineReady) return;
	log('action', 'Reorder feature', { featureId, newPosition });
	await bridge.send({ type: 'ReorderFeature', feature_id: featureId, new_position: newPosition });
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
		await bridge.send({ type: 'Undo' });
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
		await bridge.send({ type: 'Redo' });
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
		a.download = 'project.waffle';
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
export async function exportStl() {
	if (!bridge || !engineReady) return false;
	log('action', 'Export STL');
	const response = await bridge.send({ type: 'ExportStl' });
	if (response.type !== 'StlExportReady' || !response.stl_data) return false;
	showToast('success', 'STL exported');

	// Decode base64 to binary
	if (typeof document !== 'undefined') {
		const binary = atob(response.stl_data);
		const bytes = new Uint8Array(binary.length);
		for (let i = 0; i < binary.length; i++) {
			bytes[i] = binary.charCodeAt(i);
		}
		const blob = new Blob([bytes], { type: 'application/octet-stream' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = 'model.stl';
		document.body.appendChild(a);
		a.click();
		document.body.removeChild(a);
		URL.revokeObjectURL(url);
	}

	return true;
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
		await bridge.send({ type: 'LoadProject', data: jsonData });
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
			const text = await file.text();
			await bridge.send({ type: 'LoadProject', data: text });
			resolve(true);
		};
		input.click();
	});
}
