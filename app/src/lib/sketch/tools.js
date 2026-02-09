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
	findPointNear,
	getSketchPositions,
	getSketchEntities,
	getSketchSelection,
	setSketchSelection,
	setSketchHover,
	findLineNear,
	findCircleNear
} from '$lib/engine/store.svelte.js';
import { detectSnaps } from './snap.js';

// -- Module state (reactive via $state in .svelte.js, but we use plain JS here) --

/** @type {{ type: string, data: any } | null} */
let currentPreview = null;

/** @type {string} */
let toolState = 'idle';

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

/** @type {import('./snap.js').SnapIndicator | null} */
let currentSnapIndicator = null;

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

/**
 * Reset the current tool state to idle.
 */
export function resetTool() {
	toolState = 'idle';
	startPointId = null;
	startPos = null;
	centerPos = null;
	centerPointId = null;
	arcStartPos = null;
	arcStartPointId = null;
	currentPreview = null;
	currentSnapIndicator = null;
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
	}
}

// ---- Line Tool ----

function handleLineTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, startPointId, screenPixelSize);
	currentSnapIndicator = snap.indicator;

	if (eventType === 'pointermove') {
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
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			startPointId = pt.id;
			startPos = { x: pt.x, y: pt.y };
			toolState = 'firstPointPlaced';
			currentPreview = null;
		} else if (toolState === 'firstPointPlaced') {
			const endPt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);

			// Don't create zero-length lines
			if (endPt.id === startPointId) return;

			const lineId = allocEntityId();
			addLocalEntity({
				type: 'Line', id: lineId,
				start_id: startPointId, end_id: endPt.id,
				construction: false
			});

			// Auto-apply H/V constraints from snap
			for (const c of snap.constraints) {
				if (c.type === 'Horizontal') {
					addLocalConstraint({ type: 'Horizontal', entity: lineId });
				} else if (c.type === 'Vertical') {
					addLocalConstraint({ type: 'Vertical', entity: lineId });
				}
			}

			// Continuous chaining — end becomes next start
			startPointId = endPt.id;
			startPos = { x: endPt.x, y: endPt.y };
			currentPreview = null;
		}
	}
}

// ---- Rectangle Tool ----

function handleRectangleTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, null, screenPixelSize);
	currentSnapIndicator = snap.indicator;

	if (eventType === 'pointermove') {
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
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			startPointId = pt.id;
			startPos = { x: pt.x, y: pt.y };
			toolState = 'firstCornerPlaced';
		} else if (toolState === 'firstCornerPlaced') {
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

			// Auto-apply H/V constraints
			addLocalConstraint({ type: 'Horizontal', entity: l1Id });
			addLocalConstraint({ type: 'Horizontal', entity: l3Id });
			addLocalConstraint({ type: 'Vertical', entity: l2Id });
			addLocalConstraint({ type: 'Vertical', entity: l4Id });

			toolState = 'idle';
			startPointId = null;
			startPos = null;
			currentPreview = null;
			currentSnapIndicator = null;
		}
	}
}

// ---- Circle Tool ----

function handleCircleTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, centerPointId, screenPixelSize);
	currentSnapIndicator = snap.indicator;

	if (eventType === 'pointermove') {
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
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			centerPointId = pt.id;
			centerPos = { x: pt.x, y: pt.y };
			toolState = 'centerPlaced';
		} else if (toolState === 'centerPlaced') {
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
			}

			toolState = 'idle';
			centerPointId = null;
			centerPos = null;
			currentPreview = null;
			currentSnapIndicator = null;
		}
	}
}

// ---- Arc Tool ----

function handleArcTool(eventType, x, y, screenPixelSize) {
	const snap = detectSnaps(x, y, arcStartPointId ?? centerPointId, screenPixelSize);
	currentSnapIndicator = snap.indicator;

	if (eventType === 'pointermove') {
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
			const pt = findOrCreatePoint(snap.x, snap.y, screenPixelSize, snap.snapPointId);
			centerPointId = pt.id;
			centerPos = { x: pt.x, y: pt.y };
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

			toolState = 'idle';
			centerPointId = null;
			centerPos = null;
			arcStartPointId = null;
			arcStartPos = null;
			currentPreview = null;
			currentSnapIndicator = null;
		}
	}
}

// ---- Select Tool ----

function handleSelectTool(eventType, x, y, screenPixelSize, shiftKey) {
	currentSnapIndicator = null;
	currentPreview = null;

	if (eventType === 'pointermove') {
		// Hit-test for hover
		const hitId = hitTest(x, y, screenPixelSize);
		setSketchHover(hitId);
		return;
	}

	if (eventType === 'pointerdown') {
		const hitId = hitTest(x, y, screenPixelSize);
		const selection = getSketchSelection();

		if (hitId == null) {
			if (!shiftKey) setSketchSelection(new Set());
			return;
		}

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
