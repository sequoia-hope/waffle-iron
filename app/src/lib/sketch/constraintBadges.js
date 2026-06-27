/**
 * Shared computation of geometric constraint badge placement.
 *
 * A "badge" is the little glyph (H, V, M, ‖, ⟂, =, T, •, ↔, ×, 📌) drawn near a
 * geometric constraint. Both the renderer (SketchRenderer) and the hit-test
 * (select tool) consume this so badge visuals and click targets stay in sync.
 *
 * Badges are offset off their anchor geometry by a constant *screen* gap
 * (BADGE_GAP_PX × screenPixelSize) so they always clear the entity hit zone
 * regardless of zoom — letting entity selection take priority without the badge
 * ever sitting on top of the line/point it annotates.
 *
 * Returns sketch-space positions (the caller maps to world via sketchToWorld).
 * Dimensional constraints (Distance/Radius/Angle/…) are intentionally excluded
 * — those render as interactive HTML labels in DimensionLabels.svelte.
 */

/** Constant screen-pixel gap between a badge and its anchor geometry. */
export const BADGE_GAP_PX = 18;

/** Stable, presentation-order-independent key for a constraint (for offsets). */
export function constraintKey(c) {
	const refs = [
		c.entity, c.entity_a, c.entity_b, c.entity_c,
		c.line, c.curve, c.line_a, c.line_b,
		c.point, c.point_a, c.point_b,
	].filter((v) => v != null);
	refs.sort((a, b) => a - b);
	return `${c.type}|${refs.join(',')}`;
}

function lineMid(entity, positions) {
	const p1 = positions.get(entity.start_id);
	const p2 = positions.get(entity.end_id);
	if (p1 && p2) return { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 };
	return null;
}

function entityMidpoint(entity, positions) {
	if (!entity) return null;
	if (entity.type === 'Line') return lineMid(entity, positions);
	if (entity.type === 'Circle') {
		const center = positions.get(entity.center_id);
		if (center) return { x: center.x + (entity.radius || 1), y: center.y };
	} else if (entity.type === 'Arc') {
		const center = positions.get(entity.center_id);
		if (center) return { x: center.x, y: center.y };
	} else if (entity.type === 'Spline' && entity.point_ids?.length > 0) {
		const midPt = positions.get(entity.point_ids[Math.floor(entity.point_ids.length / 2)]);
		if (midPt) return { x: midPt.x, y: midPt.y };
	}
	return null;
}

/**
 * @param {object[]} constraints
 * @param {object[]} entities
 * @param {Map<number,{x:number,y:number}>} positions
 * @param {Set<number>} failedIndices
 * @param {Map<string,{dx:number,dy:number}>} [offsets] - per-key drag offsets
 * @param {number} [screenPixelSize] - sketch units per screen pixel (for the gap)
 * @returns {Array<{index:number,key:string,glyph:string,sx:number,sy:number,failed:boolean}>}
 */
export function computeConstraintBadges(constraints, entities, positions, failedIndices, offsets, screenPixelSize = 0.00001) {
	const byId = new Map(entities.map((e) => [e.id, e]));
	const gap = BADGE_GAP_PX * screenPixelSize; // diagonal up-right, off the geometry
	const out = [];
	// (ax, ay) is the anchor on the geometry; the badge is drawn `gap` up-right of
	// it (plus any user drag offset) so it never overlaps the entity hit zone.
	const push = (index, key, glyph, ax, ay) => {
		const off = offsets?.get(key);
		out.push({
			index, key, glyph,
			// Raw anchor on the annotated geometry (where the leader line ends).
			ax, ay,
			sx: ax + gap + (off?.dx ?? 0),
			sy: ay + gap + (off?.dy ?? 0),
			failed: failedIndices?.has(index) ?? false,
		});
	};

	for (let ci = 0; ci < constraints.length; ci++) {
		const c = constraints[ci];
		if (c._isDrag) continue;
		const key = constraintKey(c);

		if (c.type === 'Horizontal' || c.type === 'Vertical') {
			const e = byId.get(c.entity);
			const m = e && e.type === 'Line' ? lineMid(e, positions) : null;
			if (m) push(ci, key, c.type === 'Horizontal' ? 'H' : 'V', m.x, m.y);
		} else if (c.type === 'HorizontalPoints' || c.type === 'VerticalPoints') {
			const a = positions.get(c.point_a);
			const b = positions.get(c.point_b);
			if (a && b) push(ci, key, c.type === 'HorizontalPoints' ? 'H' : 'V', (a.x + b.x) / 2, (a.y + b.y) / 2);
		} else if (c.type === 'Parallel' || c.type === 'Perpendicular') {
			const l0 = byId.get(c.line_a);
			const l1 = byId.get(c.line_b);
			if (l0 && l1) {
				const p0s = positions.get(l0.start_id), p0e = positions.get(l0.end_id);
				const p1s = positions.get(l1.start_id), p1e = positions.get(l1.end_id);
				if (p0s && p0e && p1s && p1e) {
					const mx = (p0s.x + p0e.x + p1s.x + p1e.x) / 4;
					const my = (p0s.y + p0e.y + p1s.y + p1e.y) / 4;
					push(ci, key, c.type === 'Parallel' ? '‖' : '⟂', mx, my);
				}
			}
		} else if (c.type === 'Equal' || c.type === 'EqualRadius') {
			for (const ref of [c.entity_a, c.entity_b]) {
				const m = entityMidpoint(byId.get(ref), positions);
				if (m) push(ci, key, '=', m.x, m.y);
			}
		} else if (c.type === 'Tangent') {
			const line = byId.get(c.line);
			const m = line ? lineMid(line, positions) : null;
			if (m) push(ci, key, 'T', m.x, m.y);
		} else if (c.type === 'Coincident') {
			const p = positions.get(c.point_a);
			if (p) push(ci, key, '•', p.x, p.y);
		} else if (c.type === 'Midpoint') {
			const p = positions.get(c.point);
			if (p) push(ci, key, 'M', p.x, p.y);
		} else if (c.type === 'WhereDragged') {
			const p = positions.get(c.point);
			if (p) push(ci, key, '📌', p.x, p.y);
		} else if (c.type === 'Symmetric' || c.type === 'SymmetricH' || c.type === 'SymmetricV') {
			const a = positions.get(c.point_a ?? c.entity_a);
			const b = positions.get(c.point_b ?? c.entity_b);
			if (a && b) push(ci, key, '↔', (a.x + b.x) / 2, (a.y + b.y) / 2);
		} else if (c.type === 'OnEntity') {
			const p = positions.get(c.point);
			if (p) push(ci, key, '×', p.x, p.y);
		}
	}
	return out;
}
