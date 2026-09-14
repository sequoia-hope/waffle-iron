/**
 * The profile payload of a `FinishSketch` message, built from a solved sketch.
 *
 * Shared by the app's Finish Sketch (`store.finishSketch`) and the agent link's
 * `sketch_create`, so a sketch committed by either path carries identical
 * `solved_profiles` and `solved_positions` (specs/waffle_mcp_server.md I1).
 *
 * The profile extraction yields entity ids; the kernel expects ordered point-id
 * loops (looked up in `solved_positions`). Lines contribute their start point,
 * arcs are sampled into synthetic points with an `arc_segments` record, splines
 * contribute every control point, and a standalone circle becomes a tagged
 * `circle` profile (true cylinder extrusion).
 */

/**
 * @param {Iterable<{entityIds: Iterable<number>, isOuter: boolean}>} extractedProfiles - `extractProfiles` output
 * @param {Array<any>} entities - the sketch entities (solved radii already applied)
 * @param {Map<number, {x: number, y: number}>} positions - solved point positions
 * @returns {{ profiles: Array<object>, solvedPositions: Record<number, [number, number]> }}
 *   `solvedPositions` holds every solved point plus the synthetic arc samples.
 */
export function buildFinishProfiles(extractedProfiles, entities, positions) {
	// Serialize positions map to plain object with string keys
	const posObj = {};
	for (const [id, pos] of positions) {
		posObj[id] = [pos.x, pos.y];
	}

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

	const profiles = [...extractedProfiles].map((p) => {
		const pointIds = [];
		const arcSegments = [];
		const edgeEntities = [...p.entityIds].map(id => entities.find(e => e.id === id)).filter(Boolean);

		// Standalone circles: pass as tagged circle profile for true NURBS cylinder extrusion
		if (edgeEntities.length === 1 && edgeEntities[0].type === 'Circle') {
			const circle = edgeEntities[0];
			const center = positions.get(circle.center_id);
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
				const center = positions.get(entity.center_id);
				const sPos = positions.get(sId);
				const ePos = positions.get(eId);
				if (center && sPos && ePos) {
					const radius = Math.hypot(sPos.x - center.x, sPos.y - center.y);
					let startAngle = Math.atan2(sPos.y - center.y, sPos.x - center.x);
					let endAngle = Math.atan2(ePos.y - center.y, ePos.x - center.x);
					// Arc entities are CCW start→end. A forward traversal samples
					// CCW; a REVERSED traversal walks the same physical arc
					// clockwise, so angles must DECREASE — forcing CCW here used
					// to sample the complement arc (wrong side of the circle).
					if (forward) {
						if (endAngle <= startAngle) endAngle += Math.PI * 2;
					} else if (endAngle >= startAngle) {
						endAngle -= Math.PI * 2;
					}

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
					// The arc's true end vertex is the NEXT entity's start point —
					// pointIds.length is the index it will occupy (wrapped to 0
					// after the loop when this arc closes the profile). Pointing
					// at the last interior sample instead cut every arc to
					// (N-1)/N of its sweep and left a chord-sliver line: an
					// extruded 90° fillet was really an 84.4° arc + sliver.
					arcSegments.push({
						start_vertex_index: arcStartIdx,
						end_vertex_index: pointIds.length,
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

		// First entity: derive its traversal direction from connectivity with
		// the second entity — the profile walk can enter it either way. Always
		// assuming forward pushed the shared vertex twice (the kernel rejects
		// the loop with ProfileRepeatedVertex) and misdirected the whole chain.
		// A 2-entity loop (bigon) connects at both ends; keep forward for it.
		let firstForward = true;
		if (edgeEntities.length > 2) {
			const [s2, e2] = entityEndpoints(edgeEntities[1]);
			const endConnects = firstEnd === s2 || firstEnd === e2;
			const startConnects = firstStart === s2 || firstStart === e2;
			if (!endConnects && startConnects) firstForward = false;
		}
		addEntityPoints(edgeEntities[0], firstForward);
		let prevEnd = firstForward ? firstEnd : firstStart;

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

		// An arc that closes the profile ends on vertex 0 (the kernel's
		// reconstruction treats index runs cyclically).
		for (const seg of arcSegments) {
			if (seg.end_vertex_index >= pointIds.length) seg.end_vertex_index = 0;
		}

		const result = { entity_ids: [...p.entityIds], is_outer: p.isOuter, vertex_ids: pointIds };
		if (arcSegments.length > 0) {
			result.arc_segments = arcSegments;
		}
		return result;
	});

	return { profiles, solvedPositions: posObj };
}
