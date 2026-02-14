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
	getExtractedProfiles,
	setSelectedProfileIndex,
	setHoveredProfileIndex,
	showDimensionPopup,
	hideDimensionPopup
} from '$lib/engine/store.svelte.js';
import { log } from '$lib/engine/logger.js';
import { detectSnaps } from './snap.js';
import { profileToPolygon, pointInPolygon } from './profiles.js';

// -- Module state (reactive via $state in .svelte.js, but we use plain JS here) --

/** @type {{ type: string, data: any } | null} */
let currentPreview = null;

/** @type {string} */
let toolState = 'idle';

// -- Click-and-drag state --
let isDragging = false;
/** @type {{ x: number, y: number } | null} */
let pointerDownPos = null;
const DRAG_THRESHOLD_PX = 5;

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

/** @type {{ id: number, type: string } | null} */
let dimFirstEntity = null;

/** @type {import('./snap.js').SnapIndicator | null} */
let currentSnapIndicator = null;

// -- Event instrumentation (ring buffer for test diagnostics) --
/** @type {Array<{tool: string, event: string, x: number, y: number, toolState: string, isDragging: boolean, timestamp: number}>} */
const toolEventLog = [];
const MAX_EVENT_LOG = 50;

/**
 * Get the current preview geometry for the renderer.
 * @returns {{ type: string, data: any } | null}
 */
export function getPreview() {
	return currentPreview;
}

/**
 * Get the current snap indicator for the renderer.
 * @returns {import('./snap.js').SnapIndicator | null}
 */
export function getSnapIndicator() {
	return currentSnapIndicator;
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
	dimFirstEntity = null;
	currentPreview = null;
	currentSnapIndicator = null;
	isDragging = false;
	pointerDownPos = null;
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
		case 'dimension':
			handleDimensionTool(eventType, sketchX, sketchY, screenPixelSize);
			break;
	}
}

// ---- Line Tool ----

function handleLineTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, startPointId, screenPixelSize);
	currentSnapIndicator = snap.indicator;

	if (eventType === 'pointermove') {
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
			currentPreview = {
				type: 'line',
				data: { x1: startPos.x, y1: startPos.y, x2: snap.x, y2: snap.y }
			};
		}
		return;
	}

	if (eventType === 'pointerdown') {
		if (toolState === 'idle') {
			beginSketchAction();
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			startPointId = pt.id;
			startPos = { x: pt.x, y: pt.y };
			pointerDownPos = { x: snap.x, y: snap.y };
			isDragging = false;
			toolState = 'firstPointPlaced';
			currentPreview = null;
		} else if (toolState === 'firstPointPlaced' && !isDragging) {
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

	// Auto-apply constraints from snap (H/V/Tangent/Perpendicular)
	for (const c of snap.constraints) {
		if (c.type === 'Horizontal') {
			addLocalConstraint({ type: 'Horizontal', entity: lineId });
		} else if (c.type === 'Vertical') {
			addLocalConstraint({ type: 'Vertical', entity: lineId });
		} else if (c.type === 'Tangent' && c.entity_b != null) {
			addLocalConstraint({ type: 'Tangent', line: lineId, curve: c.entity_b });
		} else if (c.type === 'Perpendicular' && c.entity_b != null) {
			addLocalConstraint({ type: 'Perpendicular', line_a: lineId, line_b: c.entity_b });
		}
	}

	endSketchAction();

	// Continuous chaining — end becomes next start (only for click-click, not drag)
	if (!isDragging) {
		beginSketchAction();
		startPointId = endPt.id;
		startPos = { x: endPt.x, y: endPt.y };
		currentPreview = null;
	} else {
		// After drag, reset to idle
		toolState = 'idle';
		startPointId = null;
		startPos = null;
		currentPreview = null;
		currentSnapIndicator = null;
	}
}

// ---- Rectangle Tool ----

function handleRectangleTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, null, screenPixelSize);
	currentSnapIndicator = snap.indicator;

	if (eventType === 'pointermove') {
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
			currentPreview = {
				type: 'rectangle',
				data: { x1: startPos.x, y1: startPos.y, x2: snap.x, y2: snap.y }
			};
		}
		return;
	}

	if (eventType === 'pointerdown') {
		if (toolState === 'idle') {
			beginSketchAction();
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			startPointId = pt.id;
			startPos = { x: pt.x, y: pt.y };
			pointerDownPos = { x: snap.x, y: snap.y };
			isDragging = false;
			toolState = 'firstCornerPlaced';
		} else if (toolState === 'firstCornerPlaced' && !isDragging) {
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
	currentPreview = null;
	currentSnapIndicator = null;
}

// ---- Circle Tool ----

function handleCircleTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, centerPointId, screenPixelSize);
	currentSnapIndicator = snap.indicator;

	if (eventType === 'pointermove') {
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
			currentPreview = {
				type: 'circle',
				data: { cx: centerPos.x, cy: centerPos.y, radius }
			};
		}
		return;
	}

	if (eventType === 'pointerdown') {
		if (toolState === 'idle') {
			beginSketchAction();
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			centerPointId = pt.id;
			centerPos = { x: pt.x, y: pt.y };
			pointerDownPos = { x: snap.x, y: snap.y };
			isDragging = false;
			toolState = 'centerPlaced';
		} else if (toolState === 'centerPlaced' && !isDragging) {
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
	currentPreview = null;
	currentSnapIndicator = null;
}

// ---- Arc Tool ----

function handleArcTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, arcStartPointId ?? centerPointId, screenPixelSize);
	currentSnapIndicator = snap.indicator;

	if (eventType === 'pointermove') {
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
			currentPreview = {
				type: 'arc-preview-radius',
				data: { cx: centerPos.x, cy: centerPos.y, ex: snap.x, ey: snap.y }
			};
		} else if (toolState === 'arcStartPlaced' && centerPos && arcStartPos) {
			const startAngle = Math.atan2(arcStartPos.y - centerPos.y, arcStartPos.x - centerPos.x);
			const endAngle = Math.atan2(snap.y - centerPos.y, snap.x - centerPos.x);
			const dx = arcStartPos.x - centerPos.x;
			const dy = arcStartPos.y - centerPos.y;
			const radius = Math.sqrt(dx * dx + dy * dy);
			currentPreview = {
				type: 'arc',
				data: { cx: centerPos.x, cy: centerPos.y, radius, startAngle, endAngle }
			};
		}
		return;
	}

	if (eventType === 'pointerdown') {
		if (toolState === 'idle') {
			beginSketchAction();
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			centerPointId = pt.id;
			centerPos = { x: pt.x, y: pt.y };
			pointerDownPos = { x: snap.x, y: snap.y };
			isDragging = false;
			toolState = 'centerPlaced';
		} else if (toolState === 'centerPlaced' && !isDragging) {
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
			currentPreview = null;
			currentSnapIndicator = null;
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

// ---- Select Tool ----

function handleSelectTool(eventType, x, y, screenPixelSize, shiftKey) {
	currentPreview = null;

	if (eventType === 'pointermove') {
		// Show snap indicators on hover even in select mode
		const snap = detectSnaps(x, y, null, screenPixelSize);
		currentSnapIndicator = snap.indicator;

		// Hit-test for hover
		const hitId = hitTest(x, y, screenPixelSize);
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
		const hitId = hitTest(x, y, screenPixelSize);
		const selection = getSketchSelection();

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

		if (shiftKey) {
			const next = new Set(selection);
			if (next.has(hitId)) {
				next.delete(hitId);
			} else {
				next.add(hitId);
			}
			setSketchSelection(next);
		} else {
			setSketchSelection(new Set([hitId]));
		}
	}
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
function handleDimensionTool(eventType, x, y, screenPixelSize) {
	currentSnapIndicator = null;
	currentPreview = null;

	if (eventType === 'pointermove') {
		const hitId = hitTest(x, y, screenPixelSize);
		setSketchHover(hitId);
		return;
	}

	if (eventType !== 'pointerdown') return;

	const entities = getSketchEntities();
	const positions = getSketchPositions();
	const hitId = hitTest(x, y, screenPixelSize);
	if (hitId == null) return;

	const entity = entities.find(e => e.id === hitId);
	if (!entity) return;

	if (toolState === 'idle') {
		// Single-click dimension: line → distance, circle/arc → radius
		if (entity.type === 'Line') {
			const p1 = positions.get(entity.start_id);
			const p2 = positions.get(entity.end_id);
			if (p1 && p2) {
				const dx = p2.x - p1.x, dy = p2.y - p1.y;
				const len = Math.sqrt(dx * dx + dy * dy);
				const mx = (p1.x + p2.x) / 2;
				const my = (p1.y + p2.y) / 2;
				showDimensionPopup({
					entityA: entity.id,
					entityB: null,
					sketchX: mx,
					sketchY: my,
					dimType: 'distance',
					defaultValue: parseFloat(len.toFixed(4))
				});
			}
			return;
		}

		if (entity.type === 'Circle' || entity.type === 'Arc') {
			const center = positions.get(entity.center_id);
			let radius = entity.radius;
			if (entity.type === 'Arc') {
				const startPt = positions.get(entity.start_id);
				if (startPt && center) {
					const dx = startPt.x - center.x, dy = startPt.y - center.y;
					radius = Math.sqrt(dx * dx + dy * dy);
				}
			}
			if (center) {
				showDimensionPopup({
					entityA: entity.id,
					entityB: null,
					sketchX: center.x + (radius || 1) * 0.7,
					sketchY: center.y + (radius || 1) * 0.7,
					dimType: 'radius',
					defaultValue: parseFloat((radius || 1).toFixed(4))
				});
			}
			return;
		}

		if (entity.type === 'Point') {
			dimFirstEntity = { id: entity.id, type: 'Point' };
			toolState = 'firstEntityPicked';
			return;
		}
	} else if (toolState === 'firstEntityPicked' && dimFirstEntity) {
		// Second click: point-to-point or point-to-line distance
		if (entity.type === 'Point' && entity.id !== dimFirstEntity.id) {
			const pA = positions.get(dimFirstEntity.id);
			const pB = positions.get(entity.id);
			if (pA && pB) {
				const dx = pB.x - pA.x, dy = pB.y - pA.y;
				const dist = Math.sqrt(dx * dx + dy * dy);
				const mx = (pA.x + pB.x) / 2;
				const my = (pA.y + pB.y) / 2;
				showDimensionPopup({
					entityA: dimFirstEntity.id,
					entityB: entity.id,
					sketchX: mx,
					sketchY: my,
					dimType: 'distance',
					defaultValue: parseFloat(dist.toFixed(4))
				});
			}
			toolState = 'idle';
			dimFirstEntity = null;
			return;
		}

		if (entity.type === 'Line') {
			const pos = positions.get(dimFirstEntity.id);
			const p1 = positions.get(entity.start_id);
			const p2 = positions.get(entity.end_id);
			if (pos && p1 && p2) {
				const mx = (pos.x + (p1.x + p2.x) / 2) / 2;
				const my = (pos.y + (p1.y + p2.y) / 2) / 2;
				showDimensionPopup({
					entityA: dimFirstEntity.id,
					entityB: entity.id,
					sketchX: mx,
					sketchY: my,
					dimType: 'distance',
					defaultValue: 1.0
				});
			}
			toolState = 'idle';
			dimFirstEntity = null;
			return;
		}

		// Clicked something invalid — reset
		toolState = 'idle';
		dimFirstEntity = null;
	}
}
