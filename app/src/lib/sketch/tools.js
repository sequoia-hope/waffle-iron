/**
 * Sketch drawing tool state machines.
 *
 * Each tool manages its own state transitions and emits sketch entities
 * via the store's addLocalEntity/addLocalConstraint functions.
 * A reactive `preview` variable drives SketchRenderer's rubberband display.
 */

import {
	allocEntityId,
	addLocalEntity,
	addLocalConstraint,
	beginSketchAction,
	endSketchAction,
	findPointNear,
	getSketchPositions,
	getSketchEntities,
	getSketchSelection,
	setSketchSelection,
	setSketchHover,
	findLineNear,
	findCircleNear,
	findSplineNear,
	getGearIdForEntity,
	getGearRegistry,
	getGearDisplay,
	showGearDialog,
	showPlanetaryGearDialog,
	getExtractedProfiles,
	setSelectedProfileIndex,
	setHoveredProfileIndex,
	showDimensionPopup,
	hideDimensionPopup,
	getSnapSettings,
	dragSketchPoint,
	dragSketchLine,
	finalizeDrag,
	getDragState,
	getHoveredRef,
	getMeshes,
	geomRefEquals,
	getSketchMode,
	removeSketchEntities,
	getBridge,
	getSketchConstraints,
	getFailedConstraintIndices,
	getConstraintBadgeOffsets,
	setConstraintBadgeOffset,
	setSelectedConstraintIndex,
	constraintModalPick,
	getConstraintModal
} from '$lib/engine/store.svelte.js';
import {
	findLineLineIntersection,
	findLineCircleIntersections,
	findArcLineIntersections,
	distanceToLineSegment,
	angleBisector,
	perpendicularFoot,
	parameterOnSegment
} from './geometry-utils.js';
import { log } from '$lib/engine/logger.js';
import { detectSnaps, collectSnapCandidates } from './snap.js';
import { computeConstraintBadges } from './constraintBadges.js';
import { profileToPolygon, pointInPolygon } from './profiles.js';
import { setPreview, setSnapIndicator, setSnapCandidates, getPreview as _getPreview, getSnapIndicator as _getSnapIndicator, getSnapCandidates as _getSnapCandidates } from './sketchToolState.svelte.js';
import { buildSketchPlane } from './sketchCoords.js';
import { projectEdgeToSketch, simplifyPolyline } from './projectGeometry.js';
import { classifyDimension, isDimensionComplete, linearPreviewPolyline } from './dimensionHeuristic.js';
import { DRAG_THRESHOLD_PX, GEAR_PREVIEW_MODULE_M, DEFAULT_GEAR_TOOTH_COUNT, DEFAULT_GEAR_PRESSURE_ANGLE } from '$lib/config.js';

// -- Module state --

/** @type {string} */
let toolState = 'idle';

// -- Click-and-drag state --
let isDragging = false;
/** @type {{ x: number, y: number } | null} */
let pointerDownPos = null;

/** @type {number | null} */
let startPointId = null;
/** @type {{ x: number, y: number } | null} */
let startPos = null;

/** @type {{ x: number, y: number } | null} */
let centerPos = null;
/** @type {number | null} */
let centerPointId = null;

/** @type {{ x: number, y: number } | null} */
let arcStartPos = null;
/** @type {number | null} */
let arcStartPointId = null;

/**
 * Dimension tool: targets picked so far (points/lines) and whether we are in
 * the leader-placement phase. See /specs/dimension_tool.md.
 * @type {Array<{ id: number, type: string }>}
 */
let dimTargets = [];
let dimPlacing = false;

// -- Slot tool state --
/** @type {number | null} */
let slotFirstCenterId = null;
/** @type {{ x: number, y: number } | null} */
let slotFirstCenterPos = null;
/** @type {number | null} */
let slotSecondCenterId = null;
/** @type {{ x: number, y: number } | null} */
let slotSecondCenterPos = null;

// -- Gear tool state --
/** @type {number | null} */
let lastSelectClickTime = null;
/** @type {number | null} */
let lastSelectClickEntity = null;

// -- Trim tool state --
/** @type {{ entityId: number, segStart: {x:number,y:number}, segEnd: {x:number,y:number}, splitPoints: Array<{x:number,y:number}> } | null} */
let trimHighlight = null;

// -- Sketch Fillet tool state --
/** @type {{ pointId: number, lines: Array<any> } | null} */
let filletCorner = null;

// -- Drag-to-reposition state --
/** @type {number | null} Point ID being dragged */
let dragPointId = null;
/** @type {number | null} Line ID being dragged (whole-line translate) */
let dragLineId = null;
/** @type {{ x: number, y: number } | null} Sketch position where a line drag started */
let dragLineStart = null;
/** @type {object | null} Most recent snap during a point drag (applied on release) */
let lastDragSnap = null;
/** @type {object | null} Constraint badge grabbed at pointerdown (pending drag/select) */
let pendingBadge = null;
/** @type {string | null} Constraint badge key currently being dragged */
let dragBadgeKey = null;
/** @type {{ x:number, y:number, dx:number, dy:number } | null} Drag-start anchor + original offset */
let dragBadgeOrig = null;

/**
 * Hit-test geometric constraint badges at the given sketch coords.
 * @returns {{ index:number, key:string, sx:number, sy:number } | null}
 */
function hitTestConstraintBadge(x, y, screenPixelSize) {
	const threshold = 10 * screenPixelSize;
	const badges = computeConstraintBadges(
		getSketchConstraints(), getSketchEntities(), getSketchPositions(),
		getFailedConstraintIndices(), getConstraintBadgeOffsets(), screenPixelSize
	);
	let best = null;
	let bestDist = threshold;
	for (const b of badges) {
		const d = Math.hypot(x - b.sx, y - b.sy);
		if (d < bestDist) { bestDist = d; best = b; }
	}
	return best;
}
/** @type {{ x: number, y: number } | null} Screen position at drag start */
let selectPointerDownPos = null;

// -- Event instrumentation (ring buffer for test diagnostics) --
/** @type {Array<{tool: string, event: string, x: number, y: number, toolState: string, isDragging: boolean, timestamp: number}>} */
const toolEventLog = [];
const MAX_EVENT_LOG = 50;

/**
 * Get the current preview geometry for the renderer.
 * @returns {{ type: string, data: any } | null}
 */
export function getPreview() {
	return _getPreview();
}

/**
 * Get the current snap indicator for the renderer.
 * @returns {import('./snap.js').SnapIndicator | null}
 */
export function getSnapIndicator() {
	return _getSnapIndicator();
}

/**
 * Get the current snap candidate preview markers.
 * @returns {Array<{ type: string, x: number, y: number, entityId?: number }>}
 */
export function getSnapCandidates() {
	return _getSnapCandidates();
}

// -- Tool state getters (for test instrumentation via __waffle) --

/** @returns {string} */
export function getToolState() { return toolState; }

/** @returns {boolean} */
export function getIsDragging() { return isDragging; }

/** @returns {{ x: number, y: number } | null} */
export function getPointerDownPos() { return pointerDownPos ? { ...pointerDownPos } : null; }

/** @returns {{ x: number, y: number } | null} */
export function getStartPos() { return startPos ? { ...startPos } : null; }

/** @returns {number | null} */
export function getStartPointId() { return startPointId; }

/** @returns {Array<{tool: string, event: string, x: number, y: number, toolState: string, isDragging: boolean, timestamp: number}>} */
export function getToolEventLog() { return [...toolEventLog]; }

export function clearToolEventLog() { toolEventLog.length = 0; }

/**
 * Reset the current tool state to idle.
 */
export function resetTool() {
	log('sketch', 'Tool reset');
	endSketchAction();
	toolState = 'idle';
	startPointId = null;
	startPos = null;
	centerPos = null;
	centerPointId = null;
	arcStartPos = null;
	arcStartPointId = null;
	dimTargets = [];
	dimPlacing = false;
	polyFirstPointId = null;
	slotFirstCenterId = null;
	slotFirstCenterPos = null;
	slotSecondCenterId = null;
	slotSecondCenterPos = null;
	trimHighlight = null;
	filletCorner = null;
	lastSelectClickTime = null;
	lastSelectClickEntity = null;
	setPreview(null);
	setSnapIndicator(null);
	setSnapCandidates([]);
	isDragging = false;
	pointerDownPos = null;
	dragPointId = null;
	dragLineId = null;
	dragLineStart = null;
	lastDragSnap = null;
	pendingBadge = null;
	dragBadgeKey = null;
	dragBadgeOrig = null;
	selectPointerDownPos = null;
	if (getDragState()) finalizeDrag();
	hideDimensionPopup();
}

/**
 * Find or create a point at the given coordinates.
 * If a point already exists within threshold, reuse it.
 *
 * @param {number} x
 * @param {number} y
 * @param {number} screenPixelSize
 * @param {number | null} [snapPointId] - Pre-detected snap point ID
 * @returns {{ id: number, x: number, y: number }}
 */
/**
 * Apply point-level snap constraints (e.g. WhereDragged for origin snap).
 * Call after creating/finding a point via snap.
 * @param {number} pointId - The point entity ID
 * @param {import('./snap.js').SnapResult} snap - Snap result with constraints
 */
function applyPointSnapConstraints(pointId, snap) {
	for (const c of snap.constraints) {
		if (c.type === 'WhereDragged') {
			addLocalConstraint({ type: 'WhereDragged', point: pointId, x: c.x, y: c.y });
		} else if (c.type === 'Midpoint') {
			// Pin this point to the midpoint of the snapped line.
			addLocalConstraint({ type: 'Midpoint', point: pointId, line: c.line });
		}
	}
}

function findOrCreatePoint(x, y, screenPixelSize, snapPointId) {
	if (snapPointId != null) {
		const positions = getSketchPositions();
		const pos = positions.get(snapPointId);
		if (pos) return { id: snapPointId, x: pos.x, y: pos.y };
	}

	const threshold = 8 * screenPixelSize;
	const existing = findPointNear(x, y, threshold);
	if (existing) return existing;

	const id = allocEntityId();
	addLocalEntity({ type: 'Point', id, x, y, construction: false });
	return { id, x, y };
}

/**
 * Handle a tool event (pointer down/move/up, or key).
 *
 * @param {string} activeTool - Current tool name from store
 * @param {string} eventType - 'pointerdown' | 'pointermove' | 'pointerup' | 'contextmenu'
 * @param {number} sketchX - Sketch-local X coordinate
 * @param {number} sketchY - Sketch-local Y coordinate
 * @param {number} screenPixelSize - Sketch units per screen pixel
 * @param {boolean} shiftKey - Whether shift is held
 */
export function handleToolEvent(activeTool, eventType, sketchX, sketchY, screenPixelSize, shiftKey) {
	// Event instrumentation for test diagnostics
	toolEventLog.push({
		tool: activeTool, event: eventType,
		x: +sketchX.toFixed(2), y: +sketchY.toFixed(2),
		toolState, isDragging,
		timestamp: Date.now()
	});
	if (toolEventLog.length > MAX_EVENT_LOG) toolEventLog.shift();

	if (eventType === 'pointerdown') {
		log('sketch', `Tool ${activeTool} pointerdown`, { tool: activeTool, x: +sketchX.toFixed(2), y: +sketchY.toFixed(2) });
	}
	switch (activeTool) {
		case 'point':
			handlePointTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'line':
			handleLineTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'rectangle':
			handleRectangleTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'circle':
			handleCircleTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'arc':
			handleArcTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'select':
			handleSelectTool(eventType, sketchX, sketchY, screenPixelSize, shiftKey);
			break;
		case 'constraint':
			handleConstraintTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'dimension':
			handleDimensionTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'polyline':
			handlePolylineTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'project':
			handleProjectTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'slot':
			handleSlotTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'trim':
			handleTrimTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'sketch-fillet':
			handleSketchFilletTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'gear':
			handleGearTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
		case 'planetary':
			handlePlanetaryTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
	}
}

/**
 * Update snap candidate preview markers, filtering out the active snap point.
 * @param {import('./snap.js').SnapResult} snap
 * @param {number} screenPixelSize
 */
function updateSnapCandidates(snap, screenPixelSize) {
	const settings = getSnapSettings();
	const previewRadius = (settings.previewPx ?? 30) * screenPixelSize;
	const raw = collectSnapCandidates(snap.x, snap.y, previewRadius);

	// Filter out the active snap point to avoid double-rendering
	if (snap.indicator) {
		const sx = snap.x;
		const sy = snap.y;
		setSnapCandidates(raw.filter(c => {
			const dist = Math.sqrt((c.x - sx) ** 2 + (c.y - sy) ** 2);
			return dist > 0.001;
		}));
	} else {
		setSnapCandidates(raw);
	}
}

// ---- Point Tool ----

/**
 * Standalone point tool: each click drops a sketch point at the (optionally
 * snapped) location. Snapping to the origin / a reference point pins it, and
 * snapping onto a line midpoint adds a Midpoint constraint.
 */
function handlePointTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, null, screenPixelSize);
	setSnapIndicator(snap.indicator);

	if (eventType === 'pointermove') {
		updateSnapCandidates(snap, screenPixelSize);
		return;
	}

	if (eventType === 'pointerdown') {
		isDragging = false;
		beginSketchAction();
		// snapPointId reuse would just re-select an existing point; for the point
		// tool we always want to drop a new point unless coincident-snapping.
		const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
		applyPointSnapConstraints(pt.id, snap);
		endSketchAction();
		setSnapIndicator(null);
		log('sketch', 'Point created', { id: pt.id });
	}
}

// ---- Line Tool ----

function handleLineTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, startPointId, screenPixelSize);
	setSnapIndicator(snap.indicator);

	if (eventType === 'pointermove') {
		updateSnapCandidates(snap, screenPixelSize);
		// Detect drag threshold
		if (pointerDownPos && toolState === 'firstPointPlaced') {
			const dragThreshold = DRAG_THRESHOLD_PX * screenPixelSize;
			const dx = snap.x - pointerDownPos.x;
			const dy = snap.y - pointerDownPos.y;
			if (Math.sqrt(dx * dx + dy * dy) > dragThreshold) {
				isDragging = true;
			}
		}
		if (toolState === 'firstPointPlaced' && startPos) {
			setPreview({
				type: 'line',
				data: { x1: startPos.x, y1: startPos.y, x2: snap.x, y2: snap.y }
			});
		}
		return;
	}

	if (eventType === 'pointerdown') {
		isDragging = false;
		if (toolState === 'idle') {
			beginSketchAction();
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			applyPointSnapConstraints(pt.id, snap);
			startPointId = pt.id;
			startPos = { x: pt.x, y: pt.y };
			pointerDownPos = { x: snap.x, y: snap.y };
			toolState = 'firstPointPlaced';
			setPreview(null);
		} else if (toolState === 'firstPointPlaced') {
			// Click-click mode: second click places end point
			finalizeLine(snap, screenPixelSize);
		}
	}

	if (eventType === 'pointerup') {
		if (isDragging && toolState === 'firstPointPlaced') {
			// Drag release: finalize the line
			finalizeLine(snap, screenPixelSize);
			isDragging = false;
			pointerDownPos = null;
		} else {
			pointerDownPos = null;
		}
	}
}

/** Finalize a line from startPos to snap position, then chain. */
function finalizeLine(snap, screenPixelSize) {
	const endPt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);

	// Don't create zero-length lines
	if (endPt.id === startPointId) return;

	const lineId = allocEntityId();
	addLocalEntity({
		type: 'Line', id: lineId,
		start_id: startPointId, end_id: endPt.id,
		construction: false
	});
	log('sketch', 'Line created', { lineId, startId: startPointId, endId: endPt.id });

	// Auto-apply constraints from snap (H/V/Tangent/Perpendicular/WhereDragged)
	for (const c of snap.constraints) {
		if (c.type === 'Horizontal') {
			addLocalConstraint({ type: 'Horizontal', entity: lineId });
		} else if (c.type === 'Vertical') {
			addLocalConstraint({ type: 'Vertical', entity: lineId });
		} else if (c.type === 'Tangent' && c.entity_b != null) {
			addLocalConstraint({ type: 'Tangent', line: lineId, curve: c.entity_b });
		} else if (c.type === 'Perpendicular' && c.entity_b != null) {
			addLocalConstraint({ type: 'Perpendicular', line_a: lineId, line_b: c.entity_b });
		} else if (c.type === 'WhereDragged') {
			addLocalConstraint({ type: 'WhereDragged', point: endPt.id, x: c.x, y: c.y });
		} else if (c.type === 'Midpoint') {
			addLocalConstraint({ type: 'Midpoint', point: endPt.id, line: c.line });
		}
	}

	endSketchAction();

	// Continuous chaining — end becomes next start (only for click-click, not drag)
	if (!isDragging) {
		beginSketchAction();
		startPointId = endPt.id;
		startPos = { x: endPt.x, y: endPt.y };
		setPreview(null);
	} else {
		// After drag, reset to idle
		toolState = 'idle';
		startPointId = null;
		startPos = null;
		setPreview(null);
		setSnapIndicator(null);
	}
}

// ---- Rectangle Tool ----

function handleRectangleTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, null, screenPixelSize);
	setSnapIndicator(snap.indicator);

	if (eventType === 'pointermove') {
		updateSnapCandidates(snap, screenPixelSize);
		// Detect drag threshold
		if (pointerDownPos && toolState === 'firstCornerPlaced') {
			const dragThreshold = DRAG_THRESHOLD_PX * screenPixelSize;
			const dx = snap.x - pointerDownPos.x;
			const dy = snap.y - pointerDownPos.y;
			if (Math.sqrt(dx * dx + dy * dy) > dragThreshold) {
				isDragging = true;
			}
		}
		if (toolState === 'firstCornerPlaced' && startPos) {
			setPreview({
				type: 'rectangle',
				data: { x1: startPos.x, y1: startPos.y, x2: snap.x, y2: snap.y }
			});
		}
		return;
	}

	if (eventType === 'pointerdown') {
		isDragging = false;
		if (toolState === 'idle') {
			beginSketchAction();
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			applyPointSnapConstraints(pt.id, snap);
			startPointId = pt.id;
			startPos = { x: pt.x, y: pt.y };
			pointerDownPos = { x: snap.x, y: snap.y };
			toolState = 'firstCornerPlaced';
		} else if (toolState === 'firstCornerPlaced') {
			// Click-click mode: second click places opposite corner
			finalizeRectangle(snap, screenPixelSize);
		}
	}

	if (eventType === 'pointerup') {
		if (isDragging && toolState === 'firstCornerPlaced') {
			finalizeRectangle(snap, screenPixelSize);
			isDragging = false;
			pointerDownPos = null;
		} else {
			pointerDownPos = null;
		}
	}
}

/** Finalize a rectangle from startPos to snap position. */
function finalizeRectangle(snap, screenPixelSize) {
	const x1 = startPos.x, y1 = startPos.y;
	const x2 = snap.x, y2 = snap.y;

	// Create 4 corner points (reuse startPoint for p1)
	const p1 = { id: startPointId, x: x1, y: y1 };
	const p2 = findOrCreatePoint(x2, y1, screenPixelSize);
	const p3 = findOrCreatePoint(x2, y2, screenPixelSize);
	const p4 = findOrCreatePoint(x1, y2, screenPixelSize);

	// Create 4 lines connecting corners
	const l1Id = allocEntityId();
	addLocalEntity({ type: 'Line', id: l1Id, start_id: p1.id, end_id: p2.id, construction: false });
	const l2Id = allocEntityId();
	addLocalEntity({ type: 'Line', id: l2Id, start_id: p2.id, end_id: p3.id, construction: false });
	const l3Id = allocEntityId();
	addLocalEntity({ type: 'Line', id: l3Id, start_id: p3.id, end_id: p4.id, construction: false });
	const l4Id = allocEntityId();
	addLocalEntity({ type: 'Line', id: l4Id, start_id: p4.id, end_id: p1.id, construction: false });

	log('sketch', 'Rectangle created', { lineIds: [l1Id, l2Id, l3Id, l4Id] });

	// Auto-apply H/V constraints
	addLocalConstraint({ type: 'Horizontal', entity: l1Id });
	addLocalConstraint({ type: 'Horizontal', entity: l3Id });
	addLocalConstraint({ type: 'Vertical', entity: l2Id });
	addLocalConstraint({ type: 'Vertical', entity: l4Id });

	endSketchAction();
	toolState = 'idle';
	startPointId = null;
	startPos = null;
	setPreview(null);
	setSnapIndicator(null);
}

// ---- Circle Tool ----

function handleCircleTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, centerPointId, screenPixelSize);
	setSnapIndicator(snap.indicator);

	if (eventType === 'pointermove') {
		updateSnapCandidates(snap, screenPixelSize);
		// Detect drag threshold
		if (pointerDownPos && toolState === 'centerPlaced') {
			const dragThreshold = DRAG_THRESHOLD_PX * screenPixelSize;
			const dx = snap.x - pointerDownPos.x;
			const dy = snap.y - pointerDownPos.y;
			if (Math.sqrt(dx * dx + dy * dy) > dragThreshold) {
				isDragging = true;
			}
		}
		if (toolState === 'centerPlaced' && centerPos) {
			const dx = snap.x - centerPos.x;
			const dy = snap.y - centerPos.y;
			const radius = Math.sqrt(dx * dx + dy * dy);
			setPreview({
				type: 'circle',
				data: { cx: centerPos.x, cy: centerPos.y, radius }
			});
		}
		return;
	}

	if (eventType === 'pointerdown') {
		isDragging = false;
		if (toolState === 'idle') {
			beginSketchAction();
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			applyPointSnapConstraints(pt.id, snap);
			centerPointId = pt.id;
			centerPos = { x: pt.x, y: pt.y };
			pointerDownPos = { x: snap.x, y: snap.y };
			toolState = 'centerPlaced';
		} else if (toolState === 'centerPlaced') {
			// Click-click mode: second click sets radius
			finalizeCircle(snap);
		}
	}

	if (eventType === 'pointerup') {
		if (isDragging && toolState === 'centerPlaced') {
			finalizeCircle(snap);
			isDragging = false;
			pointerDownPos = null;
		} else {
			pointerDownPos = null;
		}
	}
}

/** Finalize a circle from centerPos with radius to snap position. */
function finalizeCircle(snap) {
	const dx = snap.x - centerPos.x;
	const dy = snap.y - centerPos.y;
	const radius = Math.sqrt(dx * dx + dy * dy);

	if (radius > 0.001) {
		const circleId = allocEntityId();
		addLocalEntity({
			type: 'Circle', id: circleId,
			center_id: centerPointId, radius,
			construction: false
		});
		log('sketch', 'Circle created', { circleId, radius: +radius.toFixed(2) });
	}

	endSketchAction();
	toolState = 'idle';
	centerPointId = null;
	centerPos = null;
	setPreview(null);
	setSnapIndicator(null);
}

// ---- Arc Tool ----

function handleArcTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, arcStartPointId ?? centerPointId, screenPixelSize);
	setSnapIndicator(snap.indicator);

	if (eventType === 'pointermove') {
		updateSnapCandidates(snap, screenPixelSize);
		// Detect drag threshold (center → start drag)
		if (pointerDownPos && toolState === 'centerPlaced') {
			const dragThreshold = DRAG_THRESHOLD_PX * screenPixelSize;
			const dx = snap.x - pointerDownPos.x;
			const dy = snap.y - pointerDownPos.y;
			if (Math.sqrt(dx * dx + dy * dy) > dragThreshold) {
				isDragging = true;
			}
		}
		if (toolState === 'centerPlaced' && centerPos) {
			setPreview({
				type: 'arc-preview-radius',
				data: { cx: centerPos.x, cy: centerPos.y, ex: snap.x, ey: snap.y }
			});
		} else if (toolState === 'arcStartPlaced' && centerPos && arcStartPos) {
			const startAngle = Math.atan2(arcStartPos.y - centerPos.y, arcStartPos.x - centerPos.x);
			const endAngle = Math.atan2(snap.y - centerPos.y, snap.x - centerPos.x);
			const dx = arcStartPos.x - centerPos.x;
			const dy = arcStartPos.y - centerPos.y;
			const radius = Math.sqrt(dx * dx + dy * dy);
			setPreview({
				type: 'arc',
				data: { cx: centerPos.x, cy: centerPos.y, radius, startAngle, endAngle }
			});
		}
		return;
	}

	if (eventType === 'pointerdown') {
		isDragging = false;
		if (toolState === 'idle') {
			beginSketchAction();
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			centerPointId = pt.id;
			centerPos = { x: pt.x, y: pt.y };
			pointerDownPos = { x: snap.x, y: snap.y };
			toolState = 'centerPlaced';
		} else if (toolState === 'centerPlaced') {
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			arcStartPointId = pt.id;
			arcStartPos = { x: pt.x, y: pt.y };
			toolState = 'arcStartPlaced';
		} else if (toolState === 'arcStartPlaced') {
			const endPt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			const arcId = allocEntityId();
			addLocalEntity({
				type: 'Arc', id: arcId,
				center_id: centerPointId,
				start_id: arcStartPointId,
				end_id: endPt.id,
				construction: false
			});
			log('sketch', 'Arc created', { arcId });

			endSketchAction();
			toolState = 'idle';
			centerPointId = null;
			centerPos = null;
			arcStartPointId = null;
			arcStartPos = null;
			setPreview(null);
			setSnapIndicator(null);
		}
	}

	if (eventType === 'pointerup') {
		if (isDragging && toolState === 'centerPlaced') {
			// Drag release from center sets the start point of the arc
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			arcStartPointId = pt.id;
			arcStartPos = { x: pt.x, y: pt.y };
			toolState = 'arcStartPlaced';
			isDragging = false;
			pointerDownPos = null;
		} else {
			pointerDownPos = null;
		}
	}
}

// ---- Polyline Tool ----

/** @type {number | null} First point ID of the polyline (for close-to-start detection) */
let polyFirstPointId = null;

function handlePolylineTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, startPointId, screenPixelSize);
	setSnapIndicator(snap.indicator);

	if (eventType === 'pointermove') {
		updateSnapCandidates(snap, screenPixelSize);
		if (toolState === 'polyDrawing' && startPos) {
			setPreview({
				type: 'line',
				data: { x1: startPos.x, y1: startPos.y, x2: snap.x, y2: snap.y }
			});
		}
		return;
	}

	if (eventType === 'pointerdown') {
		isDragging = false;

		if (toolState === 'idle') {
			// First point
			beginSketchAction();
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			applyPointSnapConstraints(pt.id, snap);
			startPointId = pt.id;
			polyFirstPointId = pt.id;
			startPos = { x: pt.x, y: pt.y };
			toolState = 'polyDrawing';
			setPreview(null);
			return;
		}

		if (toolState === 'polyDrawing') {
			// Check if closing to first point
			if (snap.snapPointId === polyFirstPointId && polyFirstPointId != null) {
				// Close the polyline
				const lineId = allocEntityId();
				addLocalEntity({
					type: 'Line', id: lineId,
					start_id: startPointId, end_id: polyFirstPointId,
					construction: false
				});
				// Auto-apply constraints from snap
				for (const c of snap.constraints) {
					if (c.type === 'Horizontal') addLocalConstraint({ type: 'Horizontal', entity: lineId });
					else if (c.type === 'Vertical') addLocalConstraint({ type: 'Vertical', entity: lineId });
				}
				endSketchAction();
				toolState = 'idle';
				startPointId = null;
				startPos = null;
				polyFirstPointId = null;
				setPreview(null);
				setSnapIndicator(null);
				log('sketch', 'Polyline closed');
				return;
			}

			// Add a segment
			const endPt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			if (endPt.id === startPointId) return;
			applyPointSnapConstraints(endPt.id, snap);

			const lineId = allocEntityId();
			addLocalEntity({
				type: 'Line', id: lineId,
				start_id: startPointId, end_id: endPt.id,
				construction: false
			});
			// Auto-apply constraints from snap
			for (const c of snap.constraints) {
				if (c.type === 'Horizontal') addLocalConstraint({ type: 'Horizontal', entity: lineId });
				else if (c.type === 'Vertical') addLocalConstraint({ type: 'Vertical', entity: lineId });
			}
			log('sketch', 'Polyline segment added', { lineId });

			// Chain: end becomes next start
			startPointId = endPt.id;
			startPos = { x: endPt.x, y: endPt.y };
		}
	}
}

/**
 * Handle keyboard events for the polyline tool.
 * @param {string} key
 */
export function handlePolylineKey(key) {
	if (key === 'Escape' && toolState === 'polyDrawing') {
		// Finish open polyline
		endSketchAction();
		toolState = 'idle';
		startPointId = null;
		startPos = null;
		polyFirstPointId = null;
		setPreview(null);
		setSnapIndicator(null);
		log('sketch', 'Polyline finished (open)');
	}
}

// ---- Select Tool ----

function handleSelectTool(eventType, x, y, screenPixelSize, shiftKey) {
	setPreview(null);

	if (eventType === 'pointermove') {
		// Handle active drag-to-reposition
		if (dragPointId != null) {
			const snap = detectSnaps(x, y, dragPointId, screenPixelSize);
			setSnapIndicator(snap.indicator);
			lastDragSnap = snap;
			dragSketchPoint(dragPointId, snap.x, snap.y);
			return;
		}

		// Handle active whole-line drag (translate both endpoints)
		if (dragLineId != null && dragLineStart) {
			dragSketchLine(dragLineId, x - dragLineStart.x, y - dragLineStart.y);
			return;
		}

		// Handle active constraint-badge drag (reposition its display offset)
		if (dragBadgeKey != null && dragBadgeOrig) {
			setConstraintBadgeOffset(dragBadgeKey,
				dragBadgeOrig.dx + (x - dragBadgeOrig.x),
				dragBadgeOrig.dy + (y - dragBadgeOrig.y));
			return;
		}

		// Detect drag threshold for select drag
		if (selectPointerDownPos) {
			const dx = x - selectPointerDownPos.x;
			const dy = y - selectPointerDownPos.y;
			const dragThreshold = DRAG_THRESHOLD_PX * screenPixelSize;
			if (Math.sqrt(dx * dx + dy * dy) > dragThreshold) {
				// A constraint badge was grabbed → start dragging it.
				if (pendingBadge) {
					const off = getConstraintBadgeOffsets().get(pendingBadge.key);
					dragBadgeKey = pendingBadge.key;
					dragBadgeOrig = {
						x: selectPointerDownPos.x, y: selectPointerDownPos.y,
						dx: off?.dx ?? 0, dy: off?.dy ?? 0,
					};
					setConstraintBadgeOffset(dragBadgeKey,
						dragBadgeOrig.dx + (x - dragBadgeOrig.x),
						dragBadgeOrig.dy + (y - dragBadgeOrig.y));
					return;
				}
				// Check what we clicked on at drag start.
				const hitId = hitTest(selectPointerDownPos.x, selectPointerDownPos.y, screenPixelSize);
				if (hitId != null) {
					const entities = getSketchEntities();
					const entity = entities.find(e => e.id === hitId);
					if (entity && entity.type === 'Point') {
						dragPointId = hitId;
						const snap = detectSnaps(x, y, dragPointId, screenPixelSize);
						setSnapIndicator(snap.indicator);
						lastDragSnap = snap;
						dragSketchPoint(dragPointId, snap.x, snap.y);
						return;
					}
					if (entity && entity.type === 'Line') {
						// Drag the whole line by translating both endpoints.
						dragLineId = hitId;
						dragLineStart = { x: selectPointerDownPos.x, y: selectPointerDownPos.y };
						dragSketchLine(dragLineId, x - dragLineStart.x, y - dragLineStart.y);
						return;
					}
				}
				// Not dragging a draggable entity — clear down pos to stop checking
				selectPointerDownPos = null;
			}
		}

		// Show snap indicators on hover even in select mode
		const snap = detectSnaps(x, y, null, screenPixelSize);
		setSnapIndicator(snap.indicator);
		updateSnapCandidates(snap, screenPixelSize);

		// Hit-test for hover (concrete geometry, then gear body)
		let hitId = hitTest(x, y, screenPixelSize);
		if (hitId == null) hitId = hitTestGear(x, y);
		setSketchHover(hitId);

		// Profile hover detection (only when no entity is hovered)
		if (hitId == null) {
			const profileIdx = hitTestProfile(x, y);
			setHoveredProfileIndex(profileIdx);
		} else {
			setHoveredProfileIndex(null);
		}
		return;
	}

	if (eventType === 'pointerdown') {
		selectPointerDownPos = { x, y };
		dragPointId = null;
		pendingBadge = null;

		// Entities win — a badge is drawn a constant pixel gap off its geometry
		// (see constraintBadges.js), so it never overlaps a line/point hit zone.
		// Only when nothing geometric is under the cursor do we test badges.
		let hitId = hitTest(x, y, screenPixelSize);
		if (hitId == null) hitId = hitTestGear(x, y);
		const selection = getSketchSelection();

		if (hitId == null) {
			const badge = hitTestConstraintBadge(x, y, screenPixelSize);
			if (badge) {
				setSelectedConstraintIndex(badge.index);
				pendingBadge = badge;
				return;
			}
		}
		setSelectedConstraintIndex(null);

		if (hitId == null) {
			// Check if clicking inside a profile region
			const profileIdx = hitTestProfile(x, y);
			if (profileIdx != null) {
				setSelectedProfileIndex(profileIdx);
				setSketchSelection(new Set());
				return;
			}

			if (!shiftKey) {
				setSketchSelection(new Set());
				setSelectedProfileIndex(null);
			}
			return;
		}

		// Clicking an entity clears profile selection
		setSelectedProfileIndex(null);

		// Double-click to edit gear
		const now = Date.now();
		const gearId = getGearIdForEntity(hitId);
		if (gearId != null && lastSelectClickEntity === hitId && lastSelectClickTime && (now - lastSelectClickTime) < 400) {
			// Double-click on gear entity → open edit dialog
			const gearData = getGearRegistry().get(gearId);
			if (gearData) {
				showGearDialog({
					editGearId: gearId,
					params: gearData,
					centerX: gearData.centerX ?? 0,
					centerY: gearData.centerY ?? 0,
					rotationOffset: gearData.rotationOffset ?? 0
				});
			}
			lastSelectClickTime = null;
			lastSelectClickEntity = null;
			return;
		}
		lastSelectClickTime = now;
		lastSelectClickEntity = hitId;

		// A gear is a single compact entity, so selection is uniform: `hitId` is
		// either a concrete entity or the gear's `Gear` entity id.
		if (shiftKey) {
			const next = new Set(selection);
			if (next.has(hitId)) next.delete(hitId);
			else next.add(hitId);
			setSketchSelection(next);
		} else {
			setSketchSelection(new Set([hitId]));
		}
	}

	if (eventType === 'pointerup') {
		if (dragPointId != null) {
			finalizeDrag();
			// If the drag ended on a snap (origin, another point, a midpoint),
			// commit the corresponding permanent constraint so the snap "sticks".
			applyDragEndConstraints(dragPointId, lastDragSnap);
			dragPointId = null;
		}
		if (dragLineId != null) {
			finalizeDrag();
			dragLineId = null;
			dragLineStart = null;
		}
		dragBadgeKey = null;
		dragBadgeOrig = null;
		pendingBadge = null;
		lastDragSnap = null;
		selectPointerDownPos = null;
	}
}

/**
 * Constraint-modal tool: each click feeds the picked entity into the active
 * constraint modal, which applies/chains the constraint. Hover highlights the
 * pickable entity. The modal panel + Escape handle closing. Empty-space clicks
 * are inert (passed as null so the engine can flash an instruction).
 * See /specs/constraint_modal.md.
 */
function handleConstraintTool(eventType, x, y, screenPixelSize) {
	if (!getConstraintModal()) return;

	if (eventType === 'pointermove') {
		let hitId = hitTest(x, y, screenPixelSize);
		if (hitId == null) hitId = hitTestGear(x, y);
		setSketchHover(hitId);
		return;
	}

	if (eventType === 'pointerdown') {
		selectPointerDownPos = { x, y };
		return;
	}

	if (eventType === 'pointerup') {
		// Treat as a click only if the pointer didn't travel (no drag/pan).
		if (selectPointerDownPos) {
			const moved = Math.hypot(x - selectPointerDownPos.x, y - selectPointerDownPos.y);
			selectPointerDownPos = null;
			if (moved > DRAG_THRESHOLD_PX * screenPixelSize) return;
		}
		let hitId = hitTest(x, y, screenPixelSize);
		if (hitId == null) hitId = hitTestGear(x, y);
		constraintModalPick(hitId);
	}
}

/**
 * After a point drag, commit a permanent constraint matching the snap the drag
 * released on. Origin/reference snaps pin via WhereDragged; landing on another
 * point makes them Coincident; landing on a line midpoint pins via Midpoint.
 * A free release (no snap) adds nothing.
 * @param {number} pointId
 * @param {object | null} snap
 */
function applyDragEndConstraints(pointId, snap) {
	if (!snap) return;
	if (snap.snapPointId != null && snap.snapPointId !== pointId) {
		addLocalConstraint({ type: 'Coincident', point_a: pointId, point_b: snap.snapPointId });
		return;
	}
	applyPointSnapConstraints(pointId, snap);
}

/**
 * Hit-test extracted profiles at the given sketch coordinates.
 * Returns the index of the profile containing the point, or null.
 *
 * @param {number} x
 * @param {number} y
 * @returns {number | null}
 */
function hitTestProfile(x, y) {
	const profiles = getExtractedProfiles();
	const entities = getSketchEntities();
	const positions = getSketchPositions();

	for (let i = 0; i < profiles.length; i++) {
		const poly = profileToPolygon(profiles[i], entities, positions);
		if (poly.length < 3) continue;
		if (pointInPolygon(x, y, poly)) return i;
	}
	return null;
}

/**
 * Hit-test sketch entities at the given sketch coordinates.
 * Returns the ID of the nearest entity, or null.
 *
 * @param {number} x
 * @param {number} y
 * @param {number} screenPixelSize
 * @returns {number | null}
 */
function hitTest(x, y, screenPixelSize) {
	const pointThreshold = 8 * screenPixelSize;
	const lineThreshold = 5 * screenPixelSize;

	// Points first (highest priority)
	const nearPoint = findPointNear(x, y, pointThreshold);
	if (nearPoint) return nearPoint.id;

	// Lines
	const nearLine = findLineNear(x, y, lineThreshold);
	if (nearLine) return nearLine.id;

	// Circles
	const nearCircle = findCircleNear(x, y, lineThreshold);
	if (nearCircle) return nearCircle.id;

	// Splines
	const nearSpline = findSplineNear(x, y, lineThreshold);
	if (nearSpline) return nearSpline.id;

	return null;
}

/**
 * Hit-test gears: a gear is one compact `Gear` entity whose drawable geometry
 * lives in `gearDisplay`. A click inside a gear's boundary outline selects the
 * whole gear. Returns the gear's `Gear` entity id, or null.
 *
 * @param {number} x
 * @param {number} y
 * @returns {number | null}
 */
function hitTestGear(x, y) {
	const display = getGearDisplay();
	if (display.size === 0) return null;
	const registry = getGearRegistry();
	for (const [gearId, disp] of display) {
		if (disp.outline && disp.outline.length >= 3 && pointInPolygon(x, y, disp.outline)) {
			const entityId = registry.get(gearId)?.entityId;
			if (entityId != null) return entityId;
		}
	}
	return null;
}

// ---- Dimension Tool ----

/**
 * Smart Dimension tool state machine.
 *
 * idle → click line → show distance popup (line length)
 * idle → click circle/arc → show radius popup
 * idle → click point → firstEntityPicked → click second point → distance popup
 * idle → click point → firstEntityPicked → click line → distance popup
 */
/**
 * Pick-then-place dimension tool. The user clicks the object(s) to dimension;
 * once the pick set is complete a leader follows the cursor, and clicking in
 * free space places it and opens the value popup. The leader position chooses
 * the orientation (horizontal/vertical/aligned) for point/line measurements via
 * the heuristic in dimensionHeuristic.js. Circles/arcs dimension immediately.
 * See /specs/dimension_tool.md.
 */
function handleDimensionTool(eventType, x, y, screenPixelSize) {
	setSnapIndicator(null);

	if (eventType === 'pointermove') {
		if (dimPlacing) {
			updateDimensionLeaderPreview({ x, y });
			setSketchHover(null);
		} else {
			setPreview(null);
			setSketchHover(hitTest(x, y, screenPixelSize));
		}
		return;
	}

	if (eventType !== 'pointerdown') return;

	// Placement phase: a click places the leader. Exception — while placing a
	// single-target dimension, clicking a second compatible entity extends the
	// pick into a pair (e.g. line→line, point→line) instead of placing.
	if (dimPlacing) {
		const hitId = hitTest(x, y, screenPixelSize);
		if (hitId != null && dimTargets.length === 1 && tryExtendDimensionTargets(hitId)) {
			updateDimensionLeaderPreview({ x, y });
			return;
		}
		finalizeDimensionPlacement({ x, y });
		return;
	}

	// Collecting phase.
	const hitId = hitTest(x, y, screenPixelSize);
	if (hitId == null) return;
	const entity = getSketchEntities().find(e => e.id === hitId);
	if (!entity) return;

	// Circles/arcs dimension immediately (radius) — no placement step.
	if (entity.type === 'Circle' || entity.type === 'Arc') {
		showRadiusPopupFor(entity);
		resetDimensionTool();
		return;
	}

	if (entity.type !== 'Point' && entity.type !== 'Line') return;
	if (dimTargets.some(t => t.id === entity.id)) return; // ignore re-pick

	dimTargets = [...dimTargets, { id: entity.id, type: entity.type }];
	setSketchSelection(new Set(dimTargets.map(t => t.id)));

	if (isDimensionComplete(dimTargets)) {
		dimPlacing = true;
		updateDimensionLeaderPreview({ x, y });
	}
}

/** Try to add a second compatible target during placement. */
function tryExtendDimensionTargets(hitId) {
	if (dimTargets.some(t => t.id === hitId)) return false;
	const entity = getSketchEntities().find(e => e.id === hitId);
	if (!entity || (entity.type !== 'Point' && entity.type !== 'Line')) return false;
	const candidate = [...dimTargets, { id: entity.id, type: entity.type }];
	if (!isDimensionComplete(candidate)) return false;
	dimTargets = candidate;
	setSketchSelection(new Set(dimTargets.map(t => t.id)));
	return true;
}

/** Refresh the leader/witness preview for the current cursor while placing. */
function updateDimensionLeaderPreview(leader) {
	const res = classifyDimension({
		targets: dimTargets,
		leader,
		positions: getSketchPositions(),
		entities: getSketchEntities(),
	});
	if (!res) { setPreview(null); return; }
	const points = dimensionPreviewPoints(res, leader);
	setPreview(points ? { type: 'dimension', data: { points } } : null);
}

/** Build the preview polyline (sketch coords) for a classified dimension. */
function dimensionPreviewPoints(res, leader) {
	const positions = getSketchPositions();
	const entities = getSketchEntities();
	const ent = (id) => entities.find(e => e.id === id);

	if (res.dimKind === 'linear') {
		const c = res.constraint;
		const idA = c.point_a ?? c.entity_a;
		const idB = c.point_b ?? c.entity_b;
		// For a single-line linear dim the ids are the line endpoints.
		const a = positions.get(idA);
		const b = positions.get(idB);
		if (!a || !b) return null;
		return linearPreviewPolyline(res.orientation, a, b, leader);
	}
	if (res.dimKind === 'perp') {
		const p = positions.get(res.constraint.point);
		return p ? [[p.x, p.y], [leader.x, leader.y]] : null;
	}
	if (res.dimKind === 'lineDistance') {
		const line2start = res.constraint.point;
		const p = positions.get(line2start);
		return p ? [[p.x, p.y], [leader.x, leader.y]] : null;
	}
	if (res.dimKind === 'angle') {
		const line1 = ent(res.constraint.line_a);
		const a = line1 && positions.get(line1.start_id);
		const b = line1 && positions.get(line1.end_id);
		if (!a || !b) return null;
		const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
		return [[mid.x, mid.y], [leader.x, leader.y]];
	}
	return null;
}

/** Place the leader: classify, open the value popup, and reset. */
function finalizeDimensionPlacement(leader) {
	const res = classifyDimension({
		targets: dimTargets,
		leader,
		positions: getSketchPositions(),
		entities: getSketchEntities(),
	});
	if (res) {
		const { constraint, valueField } = res;
		showDimensionPopup({
			entityA: null,
			entityB: null,
			sketchX: leader.x,
			sketchY: leader.y,
			dimType: 'custom',
			defaultValue: res.value,
			// Reuse the popup's customApply hook: clone the measured constraint
			// and override its value with the user-entered number.
			customApply: (v) => {
				const c = { ...constraint };
				c[valueField] = v;
				addLocalConstraint(c);
			},
		});
	}
	resetDimensionTool();
}

function resetDimensionTool() {
	dimTargets = [];
	dimPlacing = false;
	setPreview(null);
	setSketchSelection(new Set());
}

/** Immediate radius dimension popup for a circle/arc (unchanged behavior). */
function showRadiusPopupFor(entity) {
	const positions = getSketchPositions();
	const center = positions.get(entity.center_id);
	let radius = entity.radius;
	if (entity.type === 'Arc') {
		const startPt = positions.get(entity.start_id);
		if (startPt && center) {
			radius = Math.hypot(startPt.x - center.x, startPt.y - center.y);
		}
	}
	if (!center) return;
	showDimensionPopup({
		entityA: entity.id,
		entityB: null,
		sketchX: center.x + (radius || 1) * 0.7,
		sketchY: center.y + (radius || 1) * 0.7,
		dimType: 'radius',
		defaultValue: parseFloat((radius || 1).toFixed(4)),
	});
}

// ---- Project Tool ----

function handleProjectTool(eventType, x, y, screenPixelSize) {
	setPreview(null);
	setSnapIndicator(null);

	if (eventType !== 'pointerdown') return;

	const hovered = getHoveredRef();
	if (!hovered) return;

	const meshData = getMeshes();
	if (!meshData) return;

	// Build sketch plane from current sketch mode
	const sm = getSketchMode();
	if (!sm?.active) return;
	const sketchPlane = buildSketchPlane(sm.origin, sm.normal);

	if (hovered.kind?.type === 'Edge') {
		for (const mesh of meshData) {
			if (!mesh.edges || !mesh.edges.ranges) continue;
			for (const range of mesh.edges.ranges) {
				if (!geomRefEquals(range.geom_ref, hovered)) continue;

				const projected = projectEdgeToSketch(
					mesh.edges.vertices, range, sketchPlane
				);
				const simplified = simplifyPolyline(projected);
				if (simplified.length >= 2) {
					beginSketchAction();
					createConstructionLinesFromPoints(simplified, false);
					endSketchAction();
					log('sketch', 'Projected edge as construction lines', { pointCount: simplified.length });
				}
				return;
			}
		}
	}

	if (hovered.kind?.type === 'Face') {
		log('sketch', 'Face projection not yet implemented');
	}
}

/**
 * Create construction points and lines from projected 2D points.
 * @param {Array<{ x: number, y: number }>} points
 * @param {boolean} closed - If true, connect last point to first
 */
function createConstructionLinesFromPoints(points, closed) {
	if (points.length < 2) return;
	const pointIds = [];
	for (const pt of points) {
		const id = allocEntityId();
		addLocalEntity({ type: 'Point', id, x: pt.x, y: pt.y, construction: true });
		pointIds.push(id);
	}
	const n = closed ? points.length : points.length - 1;
	for (let i = 0; i < n; i++) {
		const j = (i + 1) % points.length;
		addLocalEntity({
			type: 'Line',
			id: allocEntityId(),
			start_id: pointIds[i],
			end_id: pointIds[j],
			construction: true,
		});
	}
}

// ---- Slot Tool ----

function handleSlotTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, slotFirstCenterId, screenPixelSize);
	setSnapIndicator(snap.indicator);

	if (eventType === 'pointermove') {
		updateSnapCandidates(snap, screenPixelSize);
		if (toolState === 'slotFirstCenter' && slotFirstCenterPos) {
			setPreview({
				type: 'slot',
				data: {
					cx1: slotFirstCenterPos.x, cy1: slotFirstCenterPos.y,
					cx2: snap.x, cy2: snap.y,
					width: 2 * screenPixelSize * 20 // default preview width
				}
			});
		}
		return;
	}

	if (eventType === 'pointerdown') {
		isDragging = false;
		if (toolState === 'idle') {
			beginSketchAction();
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			slotFirstCenterId = pt.id;
			slotFirstCenterPos = { x: pt.x, y: pt.y };
			toolState = 'slotFirstCenter';
			setPreview(null);
		} else if (toolState === 'slotFirstCenter') {
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			if (pt.id === slotFirstCenterId) return; // same point

			slotSecondCenterId = pt.id;
			slotSecondCenterPos = { x: pt.x, y: pt.y };

			// Compute default width from distance between centers / 3
			const dx = slotSecondCenterPos.x - slotFirstCenterPos.x;
			const dy = slotSecondCenterPos.y - slotFirstCenterPos.y;
			const dist = Math.sqrt(dx * dx + dy * dy);
			const defaultWidth = Math.max(dist / 3, 0.5);

			// Show dimension popup for width
			const mx = (slotFirstCenterPos.x + slotSecondCenterPos.x) / 2;
			const my = (slotFirstCenterPos.y + slotSecondCenterPos.y) / 2;
			showDimensionPopup({
				entityA: slotFirstCenterId,
				entityB: slotSecondCenterId,
				sketchX: mx,
				sketchY: my,
				dimType: 'distance',
				defaultValue: parseFloat(defaultWidth.toFixed(4)),
				customApply: (width) => finalizeSlot(width)
			});
			toolState = 'slotWidthPending';
		}
	}
}

/** Finalize a slot with two center points and a width. */
function finalizeSlot(width) {
	if (!slotFirstCenterPos || !slotSecondCenterPos) return;

	const cx1 = slotFirstCenterPos.x, cy1 = slotFirstCenterPos.y;
	const cx2 = slotSecondCenterPos.x, cy2 = slotSecondCenterPos.y;
	const dx = cx2 - cx1, dy = cy2 - cy1;
	const len = Math.sqrt(dx * dx + dy * dy);
	if (len < 0.001) { resetSlotState(); return; }

	const radius = width / 2;
	// Perpendicular direction (normalized)
	const nx = -dy / len, ny = dx / len;

	// 4 connection points where lines meet arcs
	const cp1 = { x: cx1 + nx * radius, y: cy1 + ny * radius }; // top-left
	const cp2 = { x: cx2 + nx * radius, y: cy2 + ny * radius }; // top-right
	const cp3 = { x: cx2 - nx * radius, y: cy2 - ny * radius }; // bottom-right
	const cp4 = { x: cx1 - nx * radius, y: cy1 - ny * radius }; // bottom-left

	// Create 4 connection point entities
	const p1 = { id: allocEntityId(), ...cp1 };
	addLocalEntity({ type: 'Point', id: p1.id, x: p1.x, y: p1.y, construction: false });
	const p2 = { id: allocEntityId(), ...cp2 };
	addLocalEntity({ type: 'Point', id: p2.id, x: p2.x, y: p2.y, construction: false });
	const p3 = { id: allocEntityId(), ...cp3 };
	addLocalEntity({ type: 'Point', id: p3.id, x: p3.x, y: p3.y, construction: false });
	const p4 = { id: allocEntityId(), ...cp4 };
	addLocalEntity({ type: 'Point', id: p4.id, x: p4.x, y: p4.y, construction: false });

	// Create 2 lines (parallel sides)
	const line1Id = allocEntityId();
	addLocalEntity({ type: 'Line', id: line1Id, start_id: p1.id, end_id: p2.id, construction: false });
	const line2Id = allocEntityId();
	addLocalEntity({ type: 'Line', id: line2Id, start_id: p3.id, end_id: p4.id, construction: false });

	// Create 2 arcs (semicircles at each end)
	// Arc at center2: from p2 (top-right) to p3 (bottom-right), center = center2
	const arc1Id = allocEntityId();
	addLocalEntity({
		type: 'Arc', id: arc1Id,
		center_id: slotSecondCenterId,
		start_id: p2.id, end_id: p3.id,
		construction: false
	});

	// Arc at center1: from p4 (bottom-left) to p1 (top-left), center = center1
	const arc2Id = allocEntityId();
	addLocalEntity({
		type: 'Arc', id: arc2Id,
		center_id: slotFirstCenterId,
		start_id: p4.id, end_id: p1.id,
		construction: false
	});

	// Constraints: tangent (line to arc)
	addLocalConstraint({ type: 'Tangent', line: line1Id, curve: arc1Id });
	addLocalConstraint({ type: 'Tangent', line: line1Id, curve: arc2Id });
	addLocalConstraint({ type: 'Tangent', line: line2Id, curve: arc1Id });
	addLocalConstraint({ type: 'Tangent', line: line2Id, curve: arc2Id });

	// Equal radius
	addLocalConstraint({ type: 'EqualRadius', entity_a: arc1Id, entity_b: arc2Id });

	log('sketch', 'Slot created', { width, line1Id, line2Id, arc1Id, arc2Id });

	endSketchAction();
	resetSlotState();
}

function resetSlotState() {
	toolState = 'idle';
	slotFirstCenterId = null;
	slotFirstCenterPos = null;
	slotSecondCenterId = null;
	slotSecondCenterPos = null;
	setPreview(null);
	setSnapIndicator(null);
}

// ---- Gear Tool ----

function handleGearTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, null, screenPixelSize);
	setSnapIndicator(snap.indicator);

	if (eventType === 'pointermove') {
		updateSnapCandidates(snap, screenPixelSize);
		// Show gear preview at cursor position via WASM
		if (toolState === 'idle') {
			const b = getBridge();
			if (b) {
				b.send({
					type: 'GenerateGearPreview',
					params: {
						toothCount: DEFAULT_GEAR_TOOTH_COUNT,
						module: GEAR_PREVIEW_MODULE_M,
						pressureAngleDeg: DEFAULT_GEAR_PRESSURE_ANGLE,
						centerX: snap.x,
						centerY: snap.y
					}
				}).then(response => {
					setPreview({ type: 'gear-preview', data: { polyline: response.polyline } });
				}).catch(() => {});
			}
		}
		return;
	}

	if (eventType === 'pointerdown') {
		if (toolState === 'idle') {
			// Check for pre-selected circle
			const selection = getSketchSelection();
			const entities = getSketchEntities();
			const positions = getSketchPositions();

			let centerX = snap.x;
			let centerY = snap.y;
			let pitchDiameter = null;
			let diameterLocked = false;

			// Check if clicking on a circle
			const nearCircle = findCircleNear(snap.x, snap.y, 8 * screenPixelSize);
			if (nearCircle) {
				const circleEntity = entities.find(e => e.id === nearCircle.id);
				if (circleEntity && circleEntity.type === 'Circle') {
					const center = positions.get(circleEntity.center_id);
					if (center) {
						centerX = center.x;
						centerY = center.y;
						pitchDiameter = circleEntity.radius * 2;
						diameterLocked = true;
					}
				}
			} else {
				// Check if clicking on a point
				const nearPoint = findPointNear(snap.x, snap.y, 8 * screenPixelSize);
				if (nearPoint) {
					centerX = nearPoint.x;
					centerY = nearPoint.y;
				}
			}

			// Open dialog
			showGearDialog({
				centerX,
				centerY,
				pitchDiameter,
				diameterLocked,
				rotationOffset: 0
			});

			setPreview(null);
			toolState = 'gearDialogOpen';
		}
	}
}

// ---- Planetary Gear Tool ----
//
// Placement tool mirroring the single-gear tool: a hover preview at the cursor
// and a click that captures the center (snap point, or a clicked existing
// point) then opens the planetary dialog seeded with it. The dialog drives the
// live param-/center-aware preview while open.

function handlePlanetaryTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, null, screenPixelSize);
	setSnapIndicator(snap.indicator);

	if (eventType === 'pointermove') {
		updateSnapCandidates(snap, screenPixelSize);
		// Lightweight stage preview at the cursor (default params) via WASM.
		if (toolState === 'idle') {
			const b = getBridge();
			if (b) {
				b.send({
					type: 'GeneratePlanetaryPreview',
					params: {
						module: GEAR_PREVIEW_MODULE_M,
						pressureAngleDeg: DEFAULT_GEAR_PRESSURE_ANGLE,
						sunTeeth: 24,
						planetTeeth: 16,
						planetCount: 4,
						backlash: 0,
						centerX: snap.x,
						centerY: snap.y,
						autoAdjust: false
					}
				}).then(response => {
					setPreview({ type: 'planetary-preview', data: { polylines: response.polylines } });
				}).catch(() => {});
			}
		}
		return;
	}

	if (eventType === 'pointerdown') {
		if (toolState === 'idle') {
			let centerX = snap.x;
			let centerY = snap.y;

			// Reuse a clicked existing point as the center, like the gear tool.
			const nearPoint = findPointNear(snap.x, snap.y, 8 * screenPixelSize);
			if (nearPoint) {
				centerX = nearPoint.x;
				centerY = nearPoint.y;
			}

			showPlanetaryGearDialog({ centerX, centerY });

			setPreview(null);
			toolState = 'planetaryDialogOpen';
		}
	}
}

// ---- Trim Tool ----

function handleTrimTool(eventType, x, y, screenPixelSize) {
	setSnapIndicator(null);

	if (eventType === 'pointermove') {
		const hitId = hitTest(x, y, screenPixelSize);
		setSketchHover(hitId);

		if (hitId == null) {
			trimHighlight = null;
			setPreview(null);
			return;
		}

		const entities = getSketchEntities();
		const positions = getSketchPositions();
		const entity = entities.find(e => e.id === hitId);
		if (!entity || entity.type === 'Point') {
			trimHighlight = null;
			setPreview(null);
			return;
		}

		// Compute intersections between this entity and all others
		const intersections = findEntityIntersections(entity, entities, positions);

		if (entity.type === 'Line') {
			const p1 = positions.get(entity.start_id);
			const p2 = positions.get(entity.end_id);
			if (!p1 || !p2) return;

			// Project intersection points onto the line parameter [0, 1]
			const params = intersections.map(pt => parameterOnSegment(pt, p1, p2));
			// Add endpoints at t=0 and t=1
			params.push(0, 1);
			params.sort((a, b) => a - b);

			// Find the cursor's parameter
			const cursorT = parameterOnSegment({ x, y }, p1, p2);

			// Find bracketing parameters
			let segStartT = 0, segEndT = 1;
			for (let i = 0; i < params.length - 1; i++) {
				if (params[i] <= cursorT + 1e-8 && params[i + 1] >= cursorT - 1e-8) {
					segStartT = params[i];
					segEndT = params[i + 1];
					break;
				}
			}

			const segStart = { x: p1.x + segStartT * (p2.x - p1.x), y: p1.y + segStartT * (p2.y - p1.y) };
			const segEnd = { x: p1.x + segEndT * (p2.x - p1.x), y: p1.y + segEndT * (p2.y - p1.y) };

			// Only highlight if there are actual intersection points to split at
			if (intersections.length > 0) {
				trimHighlight = {
					entityId: hitId,
					segStart, segEnd,
					segStartT, segEndT,
					splitPoints: intersections
				};
				setPreview({
					type: 'trim-highlight',
					data: { points: [segStart, segEnd] }
				});
			} else {
				// No intersections — highlight entire entity for deletion
				trimHighlight = {
					entityId: hitId,
					segStart: p1, segEnd: p2,
					segStartT: 0, segEndT: 1,
					splitPoints: []
				};
				setPreview({
					type: 'trim-highlight',
					data: { points: [{ x: p1.x, y: p1.y }, { x: p2.x, y: p2.y }] }
				});
			}
		} else {
			// For circles/arcs, just highlight for deletion when no intersections
			trimHighlight = { entityId: hitId, segStart: null, segEnd: null, splitPoints: intersections };
			setPreview(null);
		}
		return;
	}

	if (eventType === 'pointerdown') {
		if (!trimHighlight) return;

		const entities = getSketchEntities();
		const positions = getSketchPositions();
		const entity = entities.find(e => e.id === trimHighlight.entityId);
		if (!entity) { trimHighlight = null; return; }

		beginSketchAction();

		if (entity.type === 'Line' && trimHighlight.splitPoints.length > 0) {
			executeTrimLine(entity, trimHighlight, screenPixelSize);
		} else {
			// No intersections or non-line: delete the whole entity
			removeSketchEntities(new Set([trimHighlight.entityId]));
		}

		endSketchAction();
		trimHighlight = null;
		setPreview(null);
	}
}

/**
 * Execute trim on a line entity: split at intersection points, remove middle segment.
 */
function executeTrimLine(entity, highlight, screenPixelSize) {
	const positions = getSketchPositions();
	const p1 = positions.get(entity.start_id);
	const p2 = positions.get(entity.end_id);
	if (!p1 || !p2) return;

	const { segStartT, segEndT } = highlight;

	// Determine which segments survive (those outside the trimmed range)
	// If trimming from start to an interior point, we keep [segEndT, 1]
	// If trimming from interior to end, we keep [0, segStartT]
	// If trimming interior segment, we keep [0, segStartT] and [segEndT, 1]

	const survivors = [];
	if (segStartT > 0.001) {
		survivors.push({ t0: 0, t1: segStartT });
	}
	if (segEndT < 0.999) {
		survivors.push({ t0: segEndT, t1: 1 });
	}

	if (survivors.length === 0) {
		// Remove entire entity
		removeSketchEntities(new Set([entity.id]));
		return;
	}

	// Remove the original entity (and its orphaned points will be handled)
	removeSketchEntities(new Set([entity.id]));

	// Create replacement segments
	for (const seg of survivors) {
		const sx = p1.x + seg.t0 * (p2.x - p1.x);
		const sy = p1.y + seg.t0 * (p2.y - p1.y);
		const ex = p1.x + seg.t1 * (p2.x - p1.x);
		const ey = p1.y + seg.t1 * (p2.y - p1.y);

		const startPt = findOrCreatePoint(sx, sy, screenPixelSize);
		const endPt = findOrCreatePoint(ex, ey, screenPixelSize);
		if (startPt.id === endPt.id) continue;

		const lineId = allocEntityId();
		addLocalEntity({
			type: 'Line', id: lineId,
			start_id: startPt.id, end_id: endPt.id,
			construction: false
		});
	}
}

/**
 * Find all intersection points between one entity and all other entities.
 * @param {object} entity - The entity to find intersections for
 * @param {Array} allEntities - All sketch entities
 * @param {Map} positions - Position map
 * @returns {Array<{x:number,y:number}>}
 */
function findEntityIntersections(entity, allEntities, positions) {
	const results = [];

	if (entity.type === 'Line') {
		const p1 = positions.get(entity.start_id);
		const p2 = positions.get(entity.end_id);
		if (!p1 || !p2) return results;

		for (const other of allEntities) {
			if (other.id === entity.id) continue;

			if (other.type === 'Line') {
				const p3 = positions.get(other.start_id);
				const p4 = positions.get(other.end_id);
				if (!p3 || !p4) continue;

				const pt = findLineLineIntersection(p1, p2, p3, p4);
				if (pt) {
					// Check if intersection is on BOTH segments
					const t1 = parameterOnSegment(pt, p1, p2);
					const t2 = parameterOnSegment(pt, p3, p4);
					if (t1 > 0.001 && t1 < 0.999 && t2 > -0.001 && t2 < 1.001) {
						results.push(pt);
					}
				}
			} else if (other.type === 'Circle') {
				const center = positions.get(other.center_id);
				if (!center) continue;
				const pts = findLineCircleIntersections(p1, p2, center, other.radius);
				for (const pt of pts) {
					const t = parameterOnSegment(pt, p1, p2);
					if (t > 0.001 && t < 0.999) results.push(pt);
				}
			} else if (other.type === 'Arc') {
				const center = positions.get(other.center_id);
				const startPt = positions.get(other.start_id);
				const endPt = positions.get(other.end_id);
				if (!center || !startPt || !endPt) continue;
				const radius = Math.sqrt((startPt.x - center.x) ** 2 + (startPt.y - center.y) ** 2);
				const startAngle = Math.atan2(startPt.y - center.y, startPt.x - center.x);
				let endAngle = Math.atan2(endPt.y - center.y, endPt.x - center.x);
				if (endAngle <= startAngle) endAngle += Math.PI * 2;

				const pts = findArcLineIntersections(center, radius, startAngle, endAngle, p1, p2);
				for (const pt of pts) {
					const t = parameterOnSegment(pt, p1, p2);
					if (t > 0.001 && t < 0.999) results.push(pt);
				}
			}
		}
	}

	return results;
}

// ---- Sketch Fillet Tool ----

function handleSketchFilletTool(eventType, x, y, screenPixelSize) {
	setSnapIndicator(null);

	if (eventType === 'pointermove') {
		const hitId = hitTest(x, y, screenPixelSize);
		setSketchHover(hitId);

		if (hitId == null) {
			filletCorner = null;
			setPreview(null);
			return;
		}

		// Check if the hovered entity is a point at a corner (shared by exactly 2 lines)
		const entities = getSketchEntities();
		const entity = entities.find(e => e.id === hitId);
		if (!entity || entity.type !== 'Point') {
			filletCorner = null;
			setPreview(null);
			return;
		}

		const corner = findCornerAtPoint(hitId);
		if (!corner) {
			filletCorner = null;
			setPreview(null);
			return;
		}

		filletCorner = corner;

		// Compute preview arc
		const positions = getSketchPositions();
		const pos = positions.get(hitId);
		if (!pos) return;

		const previewData = computeFilletPreview(corner, positions, null);
		if (previewData) {
			setPreview({
				type: 'fillet-preview',
				data: previewData
			});
		}
		return;
	}

	if (eventType === 'pointerdown') {
		if (!filletCorner) return;

		const positions = getSketchPositions();
		const pos = positions.get(filletCorner.pointId);
		if (!pos) return;

		// Compute default radius from shorter line / 3
		const line1 = filletCorner.lines[0];
		const line2 = filletCorner.lines[1];
		const len1 = lineLength(line1, positions);
		const len2 = lineLength(line2, positions);
		const defaultRadius = Math.min(len1, len2) / 3;

		showDimensionPopup({
			entityA: filletCorner.pointId,
			entityB: null,
			sketchX: pos.x,
			sketchY: pos.y,
			dimType: 'radius',
			defaultValue: parseFloat(defaultRadius.toFixed(4)),
			customApply: (radius) => executeSketchFillet(filletCorner, radius)
		});
	}
}

/**
 * Find a corner at a point: the point must be shared by exactly 2 lines.
 * @param {number} pointId
 * @returns {{ pointId: number, lines: Array<any> } | null}
 */
function findCornerAtPoint(pointId) {
	const entities = getSketchEntities();
	const lines = entities.filter(e =>
		e.type === 'Line' && (e.start_id === pointId || e.end_id === pointId)
	);
	if (lines.length !== 2) return null;
	return { pointId, lines };
}

/**
 * Compute the length of a line entity.
 */
function lineLength(lineEntity, positions) {
	const p1 = positions.get(lineEntity.start_id);
	const p2 = positions.get(lineEntity.end_id);
	if (!p1 || !p2) return 0;
	return Math.sqrt((p2.x - p1.x) ** 2 + (p2.y - p1.y) ** 2);
}

/**
 * Compute fillet preview data (arc center, tangent points, radius).
 */
function computeFilletPreview(corner, positions, overrideRadius) {
	const pos = positions.get(corner.pointId);
	if (!pos) return null;

	const line1 = corner.lines[0];
	const line2 = corner.lines[1];

	// Get the "other" endpoint of each line (the one NOT at the corner)
	const other1Id = line1.start_id === corner.pointId ? line1.end_id : line1.start_id;
	const other2Id = line2.start_id === corner.pointId ? line2.end_id : line2.start_id;
	const other1 = positions.get(other1Id);
	const other2 = positions.get(other2Id);
	if (!other1 || !other2) return null;

	// Direction vectors from corner toward each line's other end
	const dir1 = { x: other1.x - pos.x, y: other1.y - pos.y };
	const dir2 = { x: other2.x - pos.x, y: other2.y - pos.y };
	const len1 = Math.sqrt(dir1.x ** 2 + dir1.y ** 2);
	const len2 = Math.sqrt(dir2.x ** 2 + dir2.y ** 2);
	if (len1 < 1e-10 || len2 < 1e-10) return null;

	// Normalize
	dir1.x /= len1; dir1.y /= len1;
	dir2.x /= len2; dir2.y /= len2;

	// Angle between lines
	const dot = dir1.x * dir2.x + dir1.y * dir2.y;
	if (Math.abs(dot) > 0.9999) return null; // lines are parallel

	const halfAngle = Math.acos(Math.min(1, Math.abs(dot))) / 2;
	// For the fillet, the half-angle between the bisector and a line direction
	// is (PI - angle_between) / 2
	const angleB = Math.acos(Math.min(1, Math.abs(dot)));
	const sinHalf = Math.sin(angleB / 2);
	if (sinHalf < 1e-10) return null;

	const radius = overrideRadius ?? Math.min(len1, len2) / 3;
	const distToCenter = radius / sinHalf;

	// Tangent points: project fillet center onto each line
	const tangentDist = radius / Math.tan(angleB / 2);
	if (tangentDist > Math.min(len1, len2) - 0.001) return null; // radius too large

	const bisector = angleBisector(dir1, dir2);
	const center = {
		x: pos.x + bisector.x * distToCenter,
		y: pos.y + bisector.y * distToCenter
	};

	// Tangent points on each line
	const tp1 = perpendicularFoot(center, pos, other1);
	const tp2 = perpendicularFoot(center, pos, other2);

	// Arc angles
	const startAngle = Math.atan2(tp1.y - center.y, tp1.x - center.x);
	const endAngle = Math.atan2(tp2.y - center.y, tp2.x - center.x);

	return { cx: center.x, cy: center.y, radius, startAngle, endAngle, tp1, tp2 };
}

/**
 * Execute sketch fillet: modify existing lines and create arc.
 */
function executeSketchFillet(corner, radius) {
	if (!corner) return;

	const positions = getSketchPositions();
	const preview = computeFilletPreview(corner, positions, radius);
	if (!preview) {
		log('sketch', 'Cannot apply fillet — radius too large or lines are parallel');
		return;
	}

	beginSketchAction();

	const { tp1, tp2 } = preview;

	// Create tangent point entities
	const tp1Pt = findOrCreatePoint(tp1.x, tp1.y, 0.001);
	const tp2Pt = findOrCreatePoint(tp2.x, tp2.y, 0.001);

	// Create arc center (reuse the corner point as it gets freed)
	// Actually, the arc center must be at preview.cx, preview.cy
	const arcCenterId = allocEntityId();
	addLocalEntity({ type: 'Point', id: arcCenterId, x: preview.cx, y: preview.cy, construction: false });

	// Create fillet arc
	const arcId = allocEntityId();
	addLocalEntity({
		type: 'Arc', id: arcId,
		center_id: arcCenterId,
		start_id: tp1Pt.id, end_id: tp2Pt.id,
		construction: false
	});

	// Modify existing line endpoints: move them to tangent points
	// Line 1: the endpoint at the corner should become tp1
	const line1 = corner.lines[0];
	const line2 = corner.lines[1];

	// Remove old lines and recreate with new endpoints
	const l1OtherId = line1.start_id === corner.pointId ? line1.end_id : line1.start_id;
	const l2OtherId = line2.start_id === corner.pointId ? line2.end_id : line2.start_id;

	removeSketchEntities(new Set([line1.id, line2.id]));

	// Recreate lines with tangent point endpoints
	const newLine1Id = allocEntityId();
	addLocalEntity({
		type: 'Line', id: newLine1Id,
		start_id: l1OtherId, end_id: tp1Pt.id,
		construction: false
	});

	const newLine2Id = allocEntityId();
	addLocalEntity({
		type: 'Line', id: newLine2Id,
		start_id: l2OtherId, end_id: tp2Pt.id,
		construction: false
	});

	// Add tangent constraints
	addLocalConstraint({ type: 'Tangent', line: newLine1Id, curve: arcId });
	addLocalConstraint({ type: 'Tangent', line: newLine2Id, curve: arcId });

	log('sketch', 'Sketch fillet applied', { radius, arcId });

	endSketchAction();
	filletCorner = null;
	setPreview(null);
}
