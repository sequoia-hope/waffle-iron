<script>
	import { T } from '@threlte/core';
	import { HTML } from '@threlte/extras';
	import * as THREE from 'three';
	import {
		getSketchMode,
		getSketchConstraints,
		getSketchEntities,
		getSketchPositions,
		updateConstraintValue,
		toggleConstraintReference
	} from '$lib/engine/store.svelte.js';
	import { buildSketchPlane, sketchToWorld } from './sketchCoords.js';

	let sm = $derived(getSketchMode());
	let constraints = $derived(getSketchConstraints());
	let entities = $derived(getSketchEntities());
	let positions = $derived(getSketchPositions());
	let plane = $derived(sm?.active ? buildSketchPlane(sm.origin, sm.normal) : null);

	/** @type {number | null} */
	let editingIndex = $state(null);
	let editValue = $state('');

	/**
	 * Compute label data for dimensional constraints.
	 */
	let dimensionLabels = $derived.by(() => {
		if (!plane) return [];
		const labels = [];

		constraints.forEach((c, index) => {
			if (c.type === 'Distance') {
				const labelPos = computeDistanceLabelPos(c);
				if (labelPos) {
					labels.push({
						index,
						type: 'Distance',
						value: c.value,
						reference: !!c.reference,
						world: sketchToWorld(labelPos.x, labelPos.y, plane),
						leaderStart: sketchToWorld(labelPos.fromX, labelPos.fromY, plane)
					});
				}
			} else if (c.type === 'Radius') {
				const labelPos = computeRadiusLabelPos(c);
				if (labelPos) {
					labels.push({
						index,
						type: 'Radius',
						value: c.value,
						reference: !!c.reference,
						world: sketchToWorld(labelPos.x, labelPos.y, plane),
						leaderStart: sketchToWorld(labelPos.fromX, labelPos.fromY, plane)
					});
				}
			} else if (c.type === 'Diameter') {
				const labelPos = computeRadiusLabelPos(c);
				if (labelPos) {
					labels.push({
						index,
						type: 'Diameter',
						value: c.value,
						reference: !!c.reference,
						world: sketchToWorld(labelPos.x, labelPos.y, plane),
						leaderStart: sketchToWorld(labelPos.fromX, labelPos.fromY, plane)
					});
				}
			} else if (c.type === 'Angle') {
				const labelPos = computeAngleLabelPos(c);
				if (labelPos) {
					labels.push({
						index,
						type: 'Angle',
						value: c.value_degrees,
						reference: !!c.reference,
						world: sketchToWorld(labelPos.x, labelPos.y, plane),
						leaderStart: sketchToWorld(labelPos.fromX, labelPos.fromY, plane)
					});
				}
			} else if (c.type === 'HDistance') {
				const labelPos = computeHVDistanceLabelPos(c, 'H');
				if (labelPos) {
					labels.push({
						index,
						type: 'HDistance',
						value: c.value,
						reference: !!c.reference,
						world: sketchToWorld(labelPos.x, labelPos.y, plane),
						leaderStart: sketchToWorld(labelPos.fromX, labelPos.fromY, plane)
					});
				}
			} else if (c.type === 'VDistance') {
				const labelPos = computeHVDistanceLabelPos(c, 'V');
				if (labelPos) {
					labels.push({
						index,
						type: 'VDistance',
						value: c.value,
						reference: !!c.reference,
						world: sketchToWorld(labelPos.x, labelPos.y, plane),
						leaderStart: sketchToWorld(labelPos.fromX, labelPos.fromY, plane)
					});
				}
			} else if (c.type === 'PointLineDistance') {
				const labelPos = computePointLineDistanceLabelPos(c);
				if (labelPos) {
					labels.push({
						index,
						type: 'PointLineDistance',
						value: c.value,
						reference: !!c.reference,
						world: sketchToWorld(labelPos.x, labelPos.y, plane),
						leaderStart: sketchToWorld(labelPos.fromX, labelPos.fromY, plane)
					});
				}
			}
		});

		return labels;
	});

	function computeDistanceLabelPos(c) {
		// Find entities a and b
		const eA = entities.find(e => e.id === c.entity_a);
		const eB = entities.find(e => e.id === c.entity_b);
		if (!eA || !eB) return null;

		let ax, ay, bx, by;

		// Both points
		if (eA.type === 'Point' && eB.type === 'Point') {
			const pA = positions.get(eA.id);
			const pB = positions.get(eB.id);
			if (!pA || !pB) return null;
			ax = pA.x; ay = pA.y; bx = pB.x; by = pB.y;
		} else {
			return null;
		}

		const mx = (ax + bx) / 2;
		const my = (ay + by) / 2;
		// Perpendicular offset
		const dx = bx - ax, dy = by - ay;
		const len = Math.sqrt(dx * dx + dy * dy);
		if (len < 0.001) return null;
		const offsetX = -dy / len * 0.3;
		const offsetY = dx / len * 0.3;

		return { x: mx + offsetX, y: my + offsetY, fromX: mx, fromY: my };
	}

	function computeRadiusLabelPos(c) {
		const entity = entities.find(e => e.id === c.entity);
		if (!entity) return null;
		const center = positions.get(entity.center_id);
		if (!center) return null;
		const radius = entity.radius || c.value;
		// Label at 45 degrees
		const angle = Math.PI / 4;
		const edgeX = center.x + Math.cos(angle) * radius;
		const edgeY = center.y + Math.sin(angle) * radius;
		return {
			x: edgeX + 0.2, y: edgeY + 0.2,
			fromX: edgeX, fromY: edgeY
		};
	}

	function computeHVDistanceLabelPos(c, direction) {
		const pA = positions.get(c.point_a);
		const pB = positions.get(c.point_b);
		if (!pA || !pB) return null;

		if (direction === 'H') {
			// Horizontal: label at midpoint X, offset below
			const mx = (pA.x + pB.x) / 2;
			const my = Math.min(pA.y, pB.y) - 0.4;
			return { x: mx, y: my, fromX: mx, fromY: (pA.y + pB.y) / 2 };
		} else {
			// Vertical: label at midpoint Y, offset right
			const mx = Math.max(pA.x, pB.x) + 0.4;
			const my = (pA.y + pB.y) / 2;
			return { x: mx, y: my, fromX: (pA.x + pB.x) / 2, fromY: my };
		}
	}

	function computePointLineDistanceLabelPos(c) {
		const pos = positions.get(c.point);
		const line = entities.find(e => e.id === c.entity);
		if (!pos || !line) return null;
		const p1 = positions.get(line.start_id);
		const p2 = positions.get(line.end_id);
		if (!p1 || !p2) return null;
		// Label at midpoint between point and nearest point on line
		const lx = p2.x - p1.x, ly = p2.y - p1.y;
		const lLen2 = lx * lx + ly * ly;
		let t = 0.5;
		if (lLen2 > 1e-10) {
			t = Math.max(0, Math.min(1, ((pos.x - p1.x) * lx + (pos.y - p1.y) * ly) / lLen2));
		}
		const nearX = p1.x + t * lx;
		const nearY = p1.y + t * ly;
		const mx = (pos.x + nearX) / 2;
		const my = (pos.y + nearY) / 2;
		return { x: mx + 0.15, y: my + 0.15, fromX: mx, fromY: my };
	}

	function computeAngleLabelPos(c) {
		const lineA = entities.find(e => e.id === c.line_a);
		if (!lineA || lineA.type !== 'Line') return null;
		const p1 = positions.get(lineA.start_id);
		const p2 = positions.get(lineA.end_id);
		if (!p1 || !p2) return null;
		const mx = (p1.x + p2.x) / 2;
		const my = (p1.y + p2.y) / 2;
		return { x: mx + 0.3, y: my + 0.3, fromX: mx, fromY: my };
	}

	function startEditing(index, currentValue, labelType) {
		editingIndex = index;
		editingType = labelType;
		// Show radius value to user for Diameter constraints
		editValue = labelType === 'Diameter' ? String(currentValue / 2) : String(currentValue);
	}

	let editingType = null;

	function finishEditing() {
		if (editingIndex != null) {
			const val = parseFloat(editValue);
			if (!isNaN(val) && val > 0) {
				// Convert radius back to diameter for storage
				const storeVal = editingType === 'Diameter' ? val * 2 : val;
				updateConstraintValue(editingIndex, storeVal);
			}
			editingIndex = null;
			editingType = null;
			editValue = '';
		}
	}

	function handleKeyDown(e) {
		if (e.key === 'Enter') {
			finishEditing();
		} else if (e.key === 'Escape') {
			editingIndex = null;
			editValue = '';
		}
	}

	/** Format display value */
	function formatValue(label) {
		const suffix = label.reference ? ' (REF)' : '';
		if (label.type === 'Angle') return `${label.value.toFixed(1)}\u00B0${suffix}`;
		if (label.type === 'Diameter') return `R ${(label.value / 2).toFixed(2)}${suffix}`;
		if (label.type === 'Radius') return `R ${label.value.toFixed(2)}${suffix}`;
		if (label.type === 'HDistance') return `H: ${label.value.toFixed(2)}${suffix}`;
		if (label.type === 'VDistance') return `V: ${label.value.toFixed(2)}${suffix}`;
		return `${label.value.toFixed(2)}${suffix}`;
	}
</script>

{#if sm?.active && plane}
	{#each dimensionLabels as label (label.index)}
		<!-- Leader line -->
		{@const leaderGeo = new THREE.BufferGeometry().setFromPoints([label.leaderStart, label.world])}
		<T.Line geometry={leaderGeo} renderOrder={12}>
			<T.LineBasicMaterial color={0x888888} depthTest={false} transparent opacity={0.5} />
		</T.Line>

		<!-- HTML label -->
		<HTML position={[label.world.x, label.world.y, label.world.z]} center pointerEvents="auto" wrapperClass="dim-html-wrapper">
			{#if editingIndex === label.index}
				<input
					type="number"
					class="dim-input"
					value={editValue}
					oninput={(e) => { editValue = e.target.value; }}
					onblur={finishEditing}
					onkeydown={handleKeyDown}
					autofocus
				/>
			{:else}
				<button
					class="dim-label"
					class:dim-reference={label.reference}
					onclick={() => { if (!label.reference) startEditing(label.index, label.value, label.type); }}
					oncontextmenu={(e) => { e.preventDefault(); toggleConstraintReference(label.index); }}
					title={label.reference ? 'Reference dimension (right-click to make driving)' : 'Right-click to toggle reference'}
				>
					{formatValue(label)}
				</button>
			{/if}
		</HTML>
	{/each}
{/if}

<style>
	/* Threlte <HTML> ignores pointerEvents prop in non-transform mode;
	   force pointer-events: none on the wrapper so clicks pass through to canvas,
	   but keep the interactive children (buttons/inputs) clickable */
	:global(.dim-html-wrapper) {
		pointer-events: none !important;
		overflow: visible !important;
	}
	:global(.dim-html-wrapper *) {
		pointer-events: none !important;
	}

	:global(.dim-label) {
		background: rgba(30, 30, 50, 0.85);
		color: #aaccff;
		border: 1px solid #4488ff;
		border-radius: 3px;
		padding: 2px 6px;
		font-size: 11px;
		font-family: monospace;
		cursor: pointer;
		white-space: nowrap;
		min-width: fit-content;
		pointer-events: auto !important;
	}

	:global(.dim-label:hover) {
		background: rgba(40, 40, 80, 0.95);
		color: #ccddff;
	}

	:global(.dim-label.dim-reference) {
		color: #8899bb;
		border-color: #667799;
		font-style: italic;
		cursor: default;
	}

	:global(.dim-input) {
		background: rgba(30, 30, 50, 0.95);
		color: #ffffff;
		border: 1px solid #44cc44;
		border-radius: 3px;
		padding: 2px 6px;
		font-size: 11px;
		font-family: monospace;
		width: 80px;
		outline: none;
		pointer-events: auto !important;
	}
</style>
