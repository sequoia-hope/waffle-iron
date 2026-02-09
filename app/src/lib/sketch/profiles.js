/**
 * Client-side closed-loop (profile) extraction from sketch entity graph.
 *
 * Ports the half-edge minimal face detection algorithm from
 * crates/sketch-solver/src/profiles.rs to JavaScript for use in WASM
 * builds where the native solver is unavailable.
 */

/**
 * @typedef {{ entityIds: number[], isOuter: boolean }} ClosedProfile
 */

/**
 * Extract closed profiles from sketch entities.
 *
 * @param {Array<object>} entities - Sketch entities (Point, Line, Circle, Arc)
 * @param {Map<number, {x: number, y: number}>} positions - Point positions
 * @returns {ClosedProfile[]}
 */
export function extractProfiles(entities, positions) {
	const profiles = [];

	// Standalone non-construction circles are automatic profiles
	for (const entity of entities) {
		if (entity.type === 'Circle' && !entity.construction) {
			profiles.push({ entityIds: [entity.id], isOuter: true });
		}
	}

	// Build directed half-edges for non-construction lines and arcs
	const edges = [];
	for (const entity of entities) {
		if (entity.type === 'Line' && !entity.construction) {
			edges.push({ from: entity.start_id, to: entity.end_id, entityId: entity.id });
			edges.push({ from: entity.end_id, to: entity.start_id, entityId: entity.id });
		} else if (entity.type === 'Arc' && !entity.construction) {
			edges.push({ from: entity.start_id, to: entity.end_id, entityId: entity.id });
			edges.push({ from: entity.end_id, to: entity.start_id, entityId: entity.id });
		}
	}

	if (edges.length === 0) {
		return profiles;
	}

	// Build adjacency: vertex → sorted outgoing edges
	/** @type {Map<number, Array<{from: number, to: number, entityId: number}>>} */
	const adjacency = new Map();
	for (const edge of edges) {
		if (!adjacency.has(edge.from)) adjacency.set(edge.from, []);
		adjacency.get(edge.from).push(edge);
	}

	// Sort each vertex's outgoing edges by departure angle
	for (const [vertexId, outEdges] of adjacency) {
		const fromPos = positions.get(vertexId);
		if (!fromPos) continue;
		outEdges.sort((a, b) => {
			const angleA = departureAngle(fromPos, positions, a);
			const angleB = departureAngle(fromPos, positions, b);
			return angleA - angleB;
		});
	}

	// Track used directed edges: key = "from-to-entityId"
	const used = new Map();
	for (const edge of edges) {
		used.set(edgeKey(edge), false);
	}

	// Walk minimal faces
	for (const edge of edges) {
		const key = edgeKey(edge);
		if (used.get(key)) continue;

		const faceEdges = [];
		const faceVertices = [];
		let current = { ...edge };

		while (true) {
			const ck = edgeKey(current);
			if (used.get(ck)) break;
			used.set(ck, true);

			// Record entity (deduplicate consecutive same-entity)
			if (faceEdges.length === 0 || faceEdges[faceEdges.length - 1] !== current.entityId) {
				faceEdges.push(current.entityId);
			}
			faceVertices.push(current.from);

			// Find next half-edge
			const next = nextHalfEdge(adjacency, current, positions);
			if (!next) break;
			if (next.from === edge.from && next.to === edge.to && next.entityId === edge.entityId) {
				break; // Completed the face
			}
			current = next;
		}

		if (faceEdges.length >= 2) {
			const winding = computeSignedArea(faceVertices, positions);
			profiles.push({ entityIds: faceEdges, isOuter: winding > 0 });
		}
	}

	// Remove unbounded outer face (largest absolute area, CW winding)
	if (profiles.length > 1) {
		let maxArea = 0;
		let maxIdx = null;

		for (let i = 0; i < profiles.length; i++) {
			const profile = profiles[i];
			// Skip standalone circles
			if (profile.entityIds.length === 1) {
				const isCircle = entities.some(e => e.type === 'Circle' && e.id === profile.entityIds[0]);
				if (isCircle) continue;
			}

			const area = Math.abs(computeProfileArea(profile, entities, positions));
			if (area > maxArea) {
				maxArea = area;
				maxIdx = i;
			}
		}

		if (maxIdx != null && !profiles[maxIdx].isOuter) {
			profiles.splice(maxIdx, 1);
		}
	}

	return profiles;
}

/**
 * Build polygon vertices from a profile's entity chain (for point-in-polygon tests).
 *
 * @param {ClosedProfile} profile
 * @param {Array<object>} entities
 * @param {Map<number, {x: number, y: number}>} positions
 * @returns {Array<{x: number, y: number}>}
 */
export function profileToPolygon(profile, entities, positions) {
	const points = [];
	for (const entityId of profile.entityIds) {
		const entity = entities.find(e => e.id === entityId);
		if (!entity) continue;

		if (entity.type === 'Line') {
			const p = positions.get(entity.start_id);
			if (p) points.push({ x: p.x, y: p.y });
		} else if (entity.type === 'Arc') {
			// Sample arc points for polygon approximation
			const center = positions.get(entity.center_id);
			const startPt = positions.get(entity.start_id);
			if (center && startPt) {
				const dx = startPt.x - center.x;
				const dy = startPt.y - center.y;
				const radius = Math.sqrt(dx * dx + dy * dy);
				const endPt = positions.get(entity.end_id);
				if (endPt) {
					let startAngle = Math.atan2(startPt.y - center.y, startPt.x - center.x);
					let endAngle = Math.atan2(endPt.y - center.y, endPt.x - center.x);
					if (endAngle <= startAngle) endAngle += Math.PI * 2;
					const segments = 16;
					for (let i = 0; i < segments; i++) {
						const t = i / segments;
						const angle = startAngle + t * (endAngle - startAngle);
						points.push({
							x: center.x + Math.cos(angle) * radius,
							y: center.y + Math.sin(angle) * radius
						});
					}
				}
			}
		} else if (entity.type === 'Circle') {
			// Sample circle points
			const center = positions.get(entity.center_id);
			if (center) {
				const segments = 32;
				for (let i = 0; i < segments; i++) {
					const angle = (i / segments) * Math.PI * 2;
					points.push({
						x: center.x + Math.cos(angle) * entity.radius,
						y: center.y + Math.sin(angle) * entity.radius
					});
				}
			}
		}
	}
	return points;
}

/**
 * Point-in-polygon test using ray casting algorithm.
 *
 * @param {number} px - Test point X
 * @param {number} py - Test point Y
 * @param {Array<{x: number, y: number}>} polygon - Polygon vertices
 * @returns {boolean}
 */
export function pointInPolygon(px, py, polygon) {
	let inside = false;
	const n = polygon.length;
	for (let i = 0, j = n - 1; i < n; j = i++) {
		const xi = polygon[i].x, yi = polygon[i].y;
		const xj = polygon[j].x, yj = polygon[j].y;
		if ((yi > py) !== (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
			inside = !inside;
		}
	}
	return inside;
}

// --- Internal helpers ---

function edgeKey(edge) {
	return `${edge.from}-${edge.to}-${edge.entityId}`;
}

function departureAngle(fromPos, positions, edge) {
	const toPos = positions.get(edge.to);
	if (!toPos) return 0;
	return Math.atan2(toPos.y - fromPos.y, toPos.x - fromPos.x);
}

function nextHalfEdge(adjacency, current, positions) {
	const outEdges = adjacency.get(current.to);
	if (!outEdges || outEdges.length === 0) return null;

	const vertexPos = positions.get(current.to);
	const fromPos = positions.get(current.from);
	if (!vertexPos || !fromPos) return null;

	// Angle of incoming direction reversed (arrival at current.to, pointing back to current.from)
	const incomingAngle = Math.atan2(fromPos.y - vertexPos.y, fromPos.x - vertexPos.x);

	let best = null;
	let bestDelta = Infinity;

	for (const edge of outEdges) {
		// Skip reverse of current edge
		if (edge.to === current.from && edge.entityId === current.entityId) continue;

		const edgeAngle = departureAngle(vertexPos, positions, edge);
		let delta = edgeAngle - incomingAngle;
		// Normalize to (0, 2π]
		while (delta <= 0) delta += Math.PI * 2;
		while (delta > Math.PI * 2) delta -= Math.PI * 2;

		if (delta < bestDelta) {
			bestDelta = delta;
			best = edge;
		}
	}

	return best ? { ...best } : null;
}

function computeSignedArea(vertices, positions) {
	if (vertices.length < 3) return 0;
	let area = 0;
	const n = vertices.length;
	for (let i = 0; i < n; i++) {
		const j = (i + 1) % n;
		const p1 = positions.get(vertices[i]);
		const p2 = positions.get(vertices[j]);
		if (!p1 || !p2) continue;
		area += p1.x * p2.y - p2.x * p1.y;
	}
	return area / 2;
}

function computeProfileArea(profile, entities, positions) {
	const vertices = [];
	for (const entityId of profile.entityIds) {
		const entity = entities.find(e => e.id === entityId);
		if (!entity) continue;
		if (entity.type === 'Line') {
			vertices.push(entity.start_id);
		} else if (entity.type === 'Arc') {
			vertices.push(entity.start_id);
		}
	}
	return computeSignedArea(vertices, positions);
}
