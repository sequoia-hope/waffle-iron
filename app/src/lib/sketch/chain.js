/**
 * Chain connectivity over sketch entities: find the connected run of
 * lines/arcs/splines through shared (or coincident) endpoints, and order it
 * end-to-end for offsetting. Pure module — no store imports.
 * See /specs/sketch_chain_offset.md.
 */

/**
 * Endpoint positions closer than this (sketch units, meters) are welded into
 * one chain node even when their point ids differ. Projected face boundaries
 * mix bound corner points and static polyline points, so id-only
 * connectivity would break at every straight↔curved seam.
 */
export const CHAIN_WELD_TOL = 1e-6;

/**
 * Connector endpoint point-ids for an entity, or null if the entity does not
 * participate in chains (Circle, Point, Gear, …). Arc centers are not
 * connectors.
 * @param {object} entity
 * @returns {[number, number] | null}
 */
export function entityEndpointIds(entity) {
	if (!entity) return null;
	if (entity.type === 'Line' || entity.type === 'Arc') {
		if (entity.start_id == null || entity.end_id == null) return null;
		return [entity.start_id, entity.end_id];
	}
	if (entity.type === 'Spline' && entity.point_ids?.length >= 2) {
		return [entity.point_ids[0], entity.point_ids[entity.point_ids.length - 1]];
	}
	return null;
}

/**
 * Weld endpoint point-ids into node ids: same point id → same node, and
 * distinct points within CHAIN_WELD_TOL of each other → same node.
 * Only endpoints of chainable entities are considered.
 * @param {Array<object>} entities
 * @param {Map<number, {x:number,y:number}>} positions
 * @returns {Map<number, number>} point id → node id
 */
function buildWeldNodes(entities, positions) {
	/** @type {number[]} */
	const pointIds = [];
	const seen = new Set();
	for (const e of entities) {
		const ends = entityEndpointIds(e);
		if (!ends) continue;
		for (const pid of ends) {
			if (!seen.has(pid)) {
				seen.add(pid);
				pointIds.push(pid);
			}
		}
	}

	// Union-find over the endpoint points.
	/** @type {Map<number, number>} */
	const parent = new Map(pointIds.map((p) => [p, p]));
	const find = (a) => {
		let r = a;
		while (parent.get(r) !== r) r = parent.get(r);
		let c = a;
		while (parent.get(c) !== c) {
			const n = parent.get(c);
			parent.set(c, r);
			c = n;
		}
		return r;
	};
	const union = (a, b) => parent.set(find(a), find(b));

	// Spatial hash so welding is O(n) instead of O(n²) — projected board
	// outlines can carry hundreds of polyline points.
	const cell = CHAIN_WELD_TOL * 4;
	/** @type {Map<string, number[]>} */
	const grid = new Map();
	for (const pid of pointIds) {
		const p = positions.get(pid);
		if (!p) continue;
		const cx = Math.floor(p.x / cell);
		const cy = Math.floor(p.y / cell);
		for (let dx = -1; dx <= 1; dx++) {
			for (let dy = -1; dy <= 1; dy++) {
				const key = `${cx + dx},${cy + dy}`;
				for (const other of grid.get(key) ?? []) {
					const q = positions.get(other);
					if (Math.abs(q.x - p.x) <= CHAIN_WELD_TOL && Math.abs(q.y - p.y) <= CHAIN_WELD_TOL) {
						union(pid, other);
					}
				}
			}
		}
		const key = `${cx},${cy}`;
		if (!grid.has(key)) grid.set(key, []);
		grid.get(key).push(pid);
	}

	/** @type {Map<number, number>} */
	const nodes = new Map();
	for (const pid of pointIds) nodes.set(pid, find(pid));
	return nodes;
}

/**
 * All entity ids connected to `startId` through shared/coincident endpoints,
 * including `startId` itself. Non-chainable entities are singleton chains.
 * @param {number} startId
 * @param {Array<object>} entities
 * @param {Map<number, {x:number,y:number}>} positions
 * @returns {number[]}
 */
export function findConnectedChain(startId, entities, positions) {
	const start = entities.find((e) => e.id === startId);
	if (!start || !entityEndpointIds(start)) return start ? [startId] : [];

	const nodes = buildWeldNodes(entities, positions);
	/** @type {Map<number, object[]>} node id → entities touching it */
	const byNode = new Map();
	for (const e of entities) {
		const ends = entityEndpointIds(e);
		if (!ends) continue;
		for (const pid of ends) {
			const node = nodes.get(pid);
			if (!byNode.has(node)) byNode.set(node, []);
			byNode.get(node).push(e);
		}
	}

	const visited = new Set([startId]);
	const queue = [start];
	while (queue.length) {
		const e = queue.pop();
		for (const pid of entityEndpointIds(e)) {
			for (const other of byNode.get(nodes.get(pid)) ?? []) {
				if (!visited.has(other.id)) {
					visited.add(other.id);
					queue.push(other);
				}
			}
		}
	}
	return [...visited];
}

/**
 * Order a set of chainable entities end-to-end.
 *
 * @param {number[]} entityIds
 * @param {Array<object>} entities
 * @param {Map<number, {x:number,y:number}>} positions
 * @returns {{ items: Array<{id:number, reversed:boolean}>, closed: boolean } | { error: 'branching'|'disconnected'|'empty'|'unsupported' }}
 *   `reversed: true` means traversal runs end→start relative to the entity's
 *   own start/end fields.
 */
export function orderChain(entityIds, entities, positions) {
	const byId = new Map(entities.map((e) => [e.id, e]));
	const members = entityIds.map((id) => byId.get(id)).filter(Boolean);
	if (members.length === 0) return { error: 'empty' };
	if (members.some((e) => !entityEndpointIds(e))) return { error: 'unsupported' };

	const nodes = buildWeldNodes(members, positions);
	/** @type {Map<number, Array<{entity:object, endIdx:number}>>} */
	const byNode = new Map();
	for (const e of members) {
		const ends = entityEndpointIds(e);
		ends.forEach((pid, endIdx) => {
			const node = nodes.get(pid);
			if (!byNode.has(node)) byNode.set(node, []);
			byNode.get(node).push({ entity: e, endIdx });
		});
	}

	let startNode = null;
	for (const [node, touching] of byNode) {
		if (touching.length > 2) return { error: 'branching' };
		if (touching.length === 1 && startNode == null) startNode = node;
	}
	const closed = startNode == null;
	if (closed) startNode = nodes.get(entityEndpointIds(members[0])[0]);

	const used = new Set();
	const items = [];
	let node = startNode;
	while (items.length < members.length) {
		const next = (byNode.get(node) ?? []).find((t) => !used.has(t.entity.id));
		if (!next) return { error: 'disconnected' };
		used.add(next.entity.id);
		// Departing from `node` via endpoint endIdx: traversal is forward when
		// we leave through the START (index 0) endpoint.
		const reversed = next.endIdx !== 0;
		items.push({ id: next.entity.id, reversed });
		const ends = entityEndpointIds(next.entity);
		node = nodes.get(ends[reversed ? 0 : 1]);
	}
	if (used.size !== members.length) return { error: 'disconnected' };
	return { items, closed };
}
