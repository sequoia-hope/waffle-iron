/**
 * Chain offset math: parallel-curve construction for an ordered chain of
 * line/arc segments (or a circle) at a signed distance. Pure module — plain
 * {x,y} data in, segment descriptions out; entity creation stays in tools.js.
 * See /specs/sketch_chain_offset.md.
 *
 * Sign convention: d > 0 offsets to the LEFT of the traversal direction.
 * The UI derives the sign from the cursor side, so users never see this.
 */

import { findLineLineIntersection, findLineCircleIntersections } from './geometry-utils.js';

/** Offset arcs whose radius would fall below this collapse → typed error. */
export const RADIUS_EPS = 1e-9;
/**
 * Offset endpoints closer than this weld into one joint (tangent chains).
 * The effective threshold scales with |d|: a tangency angle error ε at the
 * source joint opens a gap of ≈ |d|·ε between the offset endpoints, so
 * solver-converged tangents (fillets) must still weld while genuine corners
 * (turn ≫ 1e-3 rad) must not.
 */
export const JOINT_WELD_TOL = 1e-9;
/** Outside line-line corners turning less than this miter instead of arcing. */
export const MITER_MAX_RAD = Math.PI / 6;

const TWO_PI = Math.PI * 2;

const norm2pi = (a) => {
	let r = a % TWO_PI;
	if (r < 0) r += TWO_PI;
	return r;
};
const cross = (a, b) => a.x * b.y - a.y * b.x;
const dot = (a, b) => a.x * b.x + a.y * b.y;
const sub = (a, b) => ({ x: a.x - b.x, y: a.y - b.y });
const dist = (a, b) => Math.hypot(a.x - b.x, a.y - b.y);

/**
 * Resolve an ordered chain (from chain.js orderChain) into traversal
 * segments with concrete geometry.
 * @param {Array<{id:number, reversed:boolean}>} items
 * @param {Array<object>} entities
 * @param {Map<number, {x:number,y:number}>} positions
 * @returns {{ segments: Array<object> } | { error: string }}
 *   line seg: { type:'line', p0, p1 } in traversal order.
 *   arc seg:  { type:'arc', center, r, a0, a1, ccw } traversed a0 → a1 in the
 *   `ccw` sense (entities are CCW start→end; reversed traversal is CW).
 */
export function resolveChainSegments(items, entities, positions) {
	const byId = new Map(entities.map((e) => [e.id, e]));
	const segments = [];
	for (const { id, reversed } of items) {
		const e = byId.get(id);
		if (!e) return { error: 'missing-entity' };
		if (e.type === 'Line') {
			const s = positions.get(e.start_id);
			const t = positions.get(e.end_id);
			if (!s || !t) return { error: 'missing-point' };
			segments.push(reversed
				? { type: 'line', p0: { ...t }, p1: { ...s } }
				: { type: 'line', p0: { ...s }, p1: { ...t } });
		} else if (e.type === 'Arc') {
			const c = positions.get(e.center_id);
			const s = positions.get(e.start_id);
			const t = positions.get(e.end_id);
			if (!c || !s || !t) return { error: 'missing-point' };
			const r = Math.hypot(s.x - c.x, s.y - c.y);
			const aS = Math.atan2(s.y - c.y, s.x - c.x);
			const aE = Math.atan2(t.y - c.y, t.x - c.x);
			segments.push(reversed
				? { type: 'arc', center: { ...c }, r, a0: aE, a1: aS, ccw: false }
				: { type: 'arc', center: { ...c }, r, a0: aS, a1: aE, ccw: true });
		} else {
			// Splines are chainable for SELECT but not offsettable v1.
			return { error: 'unsupported-entity' };
		}
	}
	return { segments };
}

/** Point on an arc segment at angle a (uses the segment's own radius). */
const arcPoint = (seg, a) => ({
	x: seg.center.x + seg.r * Math.cos(a),
	y: seg.center.y + seg.r * Math.sin(a),
});

/** Traversal-direction unit tangent of a segment at its start or end. */
function segDirection(seg, atEnd) {
	if (seg.type === 'line') {
		const len = dist(seg.p0, seg.p1) || 1;
		return { x: (seg.p1.x - seg.p0.x) / len, y: (seg.p1.y - seg.p0.y) / len };
	}
	const a = atEnd ? seg.a1 : seg.a0;
	return seg.ccw
		? { x: -Math.sin(a), y: Math.cos(a) }
		: { x: Math.sin(a), y: -Math.cos(a) };
}

const segStart = (seg) => (seg.type === 'line' ? seg.p0 : arcPoint(seg, seg.a0));
const segEnd = (seg) => (seg.type === 'line' ? seg.p1 : arcPoint(seg, seg.a1));

/** Sweep of an arc segment in its traversal sense, in (0, 2π]. */
function arcSweep(seg) {
	const raw = seg.ccw ? norm2pi(seg.a1 - seg.a0) : norm2pi(seg.a0 - seg.a1);
	return raw < 1e-12 ? TWO_PI : raw;
}

/**
 * Signed area enclosed by the chain (arcs sampled). Positive = CCW.
 * Used to normalize typed-value offsets on closed chains to "positive = outward".
 */
export function chainSignedArea(segments) {
	const pts = [];
	for (const seg of segments) {
		if (seg.type === 'line') {
			pts.push(seg.p0);
		} else {
			const sweep = arcSweep(seg) * (seg.ccw ? 1 : -1);
			const n = 16;
			for (let i = 0; i < n; i++) {
				pts.push(arcPoint(seg, seg.a0 + (sweep * i) / n));
			}
		}
	}
	let area = 0;
	for (let i = 0; i < pts.length; i++) {
		const p = pts[i];
		const q = pts[(i + 1) % pts.length];
		area += p.x * q.y - q.x * p.y;
	}
	return area / 2;
}

/** Offset one segment by d (left of traversal). Null on radius collapse. */
function offsetSegment(seg, d) {
	if (seg.type === 'line') {
		const u = segDirection(seg, false);
		const n = { x: -u.y, y: u.x };
		return {
			type: 'line',
			p0: { x: seg.p0.x + d * n.x, y: seg.p0.y + d * n.y },
			p1: { x: seg.p1.x + d * n.x, y: seg.p1.y + d * n.y },
		};
	}
	// Left of a CCW arc points toward the center, so r shrinks by d; CW grows.
	const r = seg.ccw ? seg.r - d : seg.r + d;
	if (r <= RADIUS_EPS) return null;
	return { type: 'arc', center: { ...seg.center }, r, a0: seg.a0, a1: seg.a1, ccw: seg.ccw };
}

/** Assign a segment's traversal-start joint point. */
function setSegStart(seg, p) {
	if (seg.type === 'line') seg.p0 = { ...p };
	else seg.a0 = Math.atan2(p.y - seg.center.y, p.x - seg.center.x);
}

/** Assign a segment's traversal-end joint point. */
function setSegEnd(seg, p) {
	if (seg.type === 'line') seg.p1 = { ...p };
	else seg.a1 = Math.atan2(p.y - seg.center.y, p.x - seg.center.x);
}

/** Intersection candidates of the infinite carriers of two offset segments. */
function carrierIntersections(a, b) {
	if (a.type === 'line' && b.type === 'line') {
		const p = findLineLineIntersection(a.p0, a.p1, b.p0, b.p1);
		return p ? [p] : [];
	}
	if (a.type === 'line' || b.type === 'line') {
		const line = a.type === 'line' ? a : b;
		const arc = a.type === 'line' ? b : a;
		return findLineCircleIntersections(line.p0, line.p1, arc.center, arc.r);
	}
	// circle-circle
	const dx = b.center.x - a.center.x;
	const dy = b.center.y - a.center.y;
	const dd = Math.hypot(dx, dy);
	if (dd < 1e-12 || dd > a.r + b.r + 1e-12 || dd < Math.abs(a.r - b.r) - 1e-12) return [];
	const t = (a.r * a.r - b.r * b.r + dd * dd) / (2 * dd);
	const h2 = a.r * a.r - t * t;
	const h = h2 > 0 ? Math.sqrt(h2) : 0;
	const mx = a.center.x + (t * dx) / dd;
	const my = a.center.y + (t * dy) / dd;
	if (h < 1e-12) return [{ x: mx, y: my }];
	return [
		{ x: mx - (h * dy) / dd, y: my + (h * dx) / dd },
		{ x: mx + (h * dy) / dd, y: my - (h * dx) / dd },
	];
}

/**
 * Corner arc for an outside joint: centered at the source joint J with
 * radius |d| from E to S (both lie on that circle by construction). The
 * sweep direction is fixed by tangent continuity with the incoming segment.
 */
function cornerArc(J, d, E, S, dirOut) {
	const radial = sub(E, J);
	// CCW tangent at E is perp(radial); pick the sense that continues dirOut.
	const ccw = dot({ x: -radial.y, y: radial.x }, dirOut) >= 0;
	return {
		type: 'arc',
		center: { x: J.x, y: J.y },
		r: Math.abs(d),
		a0: Math.atan2(E.y - J.y, E.x - J.x),
		a1: Math.atan2(S.y - J.y, S.x - J.x),
		ccw,
	};
}

/**
 * Offset a resolved chain by signed distance d.
 * @param {Array<object>} segments - from resolveChainSegments
 * @param {boolean} closed
 * @param {number} d - positive = left of traversal
 * @returns {{ segments: Array<object>, closed: boolean } | { error: string }}
 */
export function offsetChainSegments(segments, closed, d) {
	if (!segments.length || Math.abs(d) < RADIUS_EPS) return { error: 'degenerate' };

	const out = [];
	for (const seg of segments) {
		const o = offsetSegment(seg, d);
		if (!o) return { error: 'radius-collapse' };
		out.push({ src: seg, off: o, cornerAfter: null });
	}

	const weldTol = Math.max(JOINT_WELD_TOL, 1e-3 * Math.abs(d));
	const jointCount = closed ? out.length : out.length - 1;
	for (let i = 0; i < jointCount; i++) {
		const a = out[i];
		const b = out[(i + 1) % out.length];
		const E = segEnd(a.off);
		const S = segStart(b.off);
		if (dist(E, S) < weldTol) {
			// Tangent joint (fillet/slot chains): snap to a shared point.
			const M = { x: (E.x + S.x) / 2, y: (E.y + S.y) / 2 };
			setSegEnd(a.off, M);
			setSegStart(b.off, M);
			continue;
		}

		const J = segEnd(a.src); // source joint position
		const dirOut = segDirection(a.src, true);
		const dirIn = segDirection(b.src, false);
		const turn = cross(dirOut, dirIn);
		const outside = turn * d < 0 || (Math.abs(turn) < 1e-12 && dot(dirOut, dirIn) < 0);

		if (outside) {
			const turnAngle = Math.atan2(Math.abs(turn), dot(dirOut, dirIn));
			if (a.off.type === 'line' && b.off.type === 'line' && turnAngle < MITER_MAX_RAD) {
				const P = findLineLineIntersection(a.off.p0, a.off.p1, b.off.p0, b.off.p1);
				if (P) {
					setSegEnd(a.off, P);
					setSegStart(b.off, P);
					continue;
				}
			}
			a.cornerAfter = cornerArc(J, d, E, S, dirOut);
			continue;
		}

		// Inside corner: trim/extend both to the carrier intersection nearest
		// the source joint. Degenerate misses weld at the midpoint.
		const candidates = carrierIntersections(a.off, b.off);
		let P = null;
		let best = Infinity;
		for (const c of candidates) {
			const dJ = dist(c, J);
			if (dJ < best) {
				best = dJ;
				P = c;
			}
		}
		if (!P) P = { x: (E.x + S.x) / 2, y: (E.y + S.y) / 2 };
		setSegEnd(a.off, P);
		setSegStart(b.off, P);
	}

	const result = [];
	for (const item of out) {
		const o = item.off;
		const degenerate = o.type === 'line'
			? dist(o.p0, o.p1) < 1e-12
			: arcSweep(o) < 1e-9 || arcSweep(o) >= TWO_PI - 1e-9;
		if (!degenerate) result.push(o);
		if (item.cornerAfter && arcSweep(item.cornerAfter) >= 1e-9 && arcSweep(item.cornerAfter) < TWO_PI - 1e-9) {
			result.push(item.cornerAfter);
		}
	}
	if (!result.length) return { error: 'degenerate' };
	return { segments: result, closed };
}

/**
 * Signed perpendicular distance from a point to the chain: magnitude = the
 * distance to the nearest segment, sign = +1 when the point is LEFT of that
 * segment's traversal. Drives the mouse-side offset preview.
 */
export function signedDistanceToChain(segments, pt) {
	let bestDist = Infinity;
	let bestSide = 1;
	for (const seg of segments) {
		let d;
		let side;
		if (seg.type === 'line') {
			const u = sub(seg.p1, seg.p0);
			const lenSq = dot(u, u);
			let t = lenSq > 0 ? dot(sub(pt, seg.p0), u) / lenSq : 0;
			t = Math.max(0, Math.min(1, t));
			const c = { x: seg.p0.x + t * u.x, y: seg.p0.y + t * u.y };
			d = dist(pt, c);
			side = cross(u, sub(pt, seg.p0)) >= 0 ? 1 : -1;
		} else {
			const v = sub(pt, seg.center);
			const rr = Math.hypot(v.x, v.y);
			const ang = Math.atan2(v.y, v.x);
			const rel = norm2pi(seg.ccw ? ang - seg.a0 : seg.a0 - ang);
			if (rel <= arcSweep(seg)) {
				d = Math.abs(rr - seg.r);
				// Left of CCW traversal is toward the center; of CW, away.
				side = seg.ccw === rr < seg.r ? 1 : -1;
			} else {
				const e0 = arcPoint(seg, seg.a0);
				const e1 = arcPoint(seg, seg.a1);
				const nearStart = dist(pt, e0) < dist(pt, e1);
				const ep = nearStart ? e0 : e1;
				d = dist(pt, ep);
				const tan = segDirection(seg, !nearStart);
				side = cross(tan, sub(pt, ep)) >= 0 ? 1 : -1;
			}
		}
		if (d < bestDist) {
			bestDist = d;
			bestSide = side;
		}
	}
	return bestDist === Infinity ? 0 : bestSide * bestDist;
}

/**
 * Sample offset segments into polylines for the line-segments preview.
 * @returns {Array<Array<[number, number]>>}
 */
export function segmentsToPolylines(segments, closed) {
	const pts = [];
	for (const seg of segments) {
		if (seg.type === 'line') {
			pts.push([seg.p0.x, seg.p0.y], [seg.p1.x, seg.p1.y]);
		} else if (seg.type === 'circle') {
			const poly = [];
			for (let i = 0; i <= 48; i++) {
				const a = (i / 48) * TWO_PI;
				poly.push([seg.center.x + seg.r * Math.cos(a), seg.center.y + seg.r * Math.sin(a)]);
			}
			return [poly];
		} else {
			const sweep = arcSweep(seg) * (seg.ccw ? 1 : -1);
			const n = Math.max(8, Math.ceil((Math.abs(sweep) / TWO_PI) * 48));
			for (let i = 0; i <= n; i++) {
				const p = arcPoint(seg, seg.a0 + (sweep * i) / n);
				pts.push([p.x, p.y]);
			}
		}
	}
	if (closed && pts.length) pts.push(pts[0]);
	return [pts];
}
