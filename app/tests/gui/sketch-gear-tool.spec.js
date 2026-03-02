/**
 * Gear tool — create gear via __waffle API, finish sketch, verify no errors.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickFinishSketch, clickExtrude } from './helpers/toolbar.js';
import {
	isSketchActive,
	getEntityCount,
	getEntities,
	waitForEntityCount,
	waitForFeatureCount,
	hasFeatureOfType,
	getFeatureCount,
	collectCrashErrors,
	expectNoAnyCrash,
	getEntityCountByType,
	hasMeshWithGeometry,
} from './helpers/state.js';

test.describe('gear tool', () => {
	test('create gear via API and finish sketch without errors', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);

		// Enter sketch mode
		await clickSketch(waffle.page);
		expect(await isSketchActive(waffle.page)).toBe(true);

		// Create a gear via the __waffle API
		const gearId = await waffle.page.evaluate(() => {
			return window.__waffle.createGear({
				toothCount: 8,
				module: 1.0,
				pressureAngle: 20,
				backlash: 0,
				centerX: 0,
				centerY: 0,
				rotationOffset: 0
			});
		});

		expect(gearId).toBeDefined();

		// Verify entities were created (points + splines + arcs + pitch circle)
		const entityCount = await getEntityCount(waffle.page);
		expect(entityCount).toBeGreaterThan(10);

		// Verify spline entities exist and have valid point_ids (no nulls)
		const entities = await getEntities(waffle.page);
		const splines = entities.filter(e => e.type === 'Spline');
		expect(splines.length).toBeGreaterThan(0);

		for (const spline of splines) {
			expect(spline.point_ids).toBeDefined();
			expect(Array.isArray(spline.point_ids)).toBe(true);
			expect(spline.point_ids.length).toBeGreaterThan(0);
			// Every point_id must be a number, not null/undefined
			for (const pid of spline.point_ids) {
				expect(pid).not.toBeNull();
				expect(pid).not.toBeUndefined();
				expect(typeof pid).toBe('number');
			}
		}

		// Verify arc entities have valid IDs (no nulls)
		const arcs = entities.filter(e => e.type === 'Arc');
		for (const arc of arcs) {
			expect(arc.center_id).not.toBeNull();
			expect(arc.start_id).not.toBeNull();
			expect(arc.end_id).not.toBeNull();
			expect(typeof arc.center_id).toBe('number');
			expect(typeof arc.start_id).toBe('number');
			expect(typeof arc.end_id).toBe('number');
		}

		// Collect console errors during finish
		const consoleErrors = [];
		waffle.page.on('console', msg => {
			if (msg.type() === 'error') consoleErrors.push(msg.text());
		});

		// Finish sketch — this is where the "invalid type: null, expected u32" error occurred
		await clickFinishSketch(waffle.page);

		// Verify sketch mode exited (not stuck due to error)
		const active = await isSketchActive(waffle.page);
		expect(active).toBe(false);

		// Verify a Sketch feature was created
		await waitForFeatureCount(waffle.page, 1, 10000);
		const hasSketch = await hasFeatureOfType(waffle.page, 'Sketch');
		expect(hasSketch).toBe(true);

		// Check no "invalid type: null" errors in console
		const nullErrors = consoleErrors.filter(e => /invalid type: null/.test(e));
		expect(nullErrors).toEqual([]);

		expectNoAnyCrash(crashes);
	});

	test('gear profile forms a single closed loop (shared point IDs)', async ({ waffle }) => {
		await clickSketch(waffle.page);

		await waffle.page.evaluate(() => {
			return window.__waffle.createGear({
				toothCount: 6,
				module: 1.0,
				pressureAngle: 20,
				backlash: 0,
				centerX: 0,
				centerY: 0,
				rotationOffset: 0
			});
		});

		// Build an adjacency map from entity endpoints and verify single closed loop
		const result = await waffle.page.evaluate(() => {
			const entities = window.__waffle.getEntities();
			const splines = entities.filter(e => e.type === 'Spline');
			const arcs = entities.filter(e => e.type === 'Arc' && !e.construction);

			// Collect edge endpoints: each edge entity connects two point IDs
			const edges = [];
			for (const s of splines) {
				edges.push([s.point_ids[0], s.point_ids[s.point_ids.length - 1]]);
			}
			for (const a of arcs) {
				edges.push([a.start_id, a.end_id]);
			}

			// Build adjacency and check each vertex has degree 2
			const degree = {};
			for (const [a, b] of edges) {
				degree[a] = (degree[a] || 0) + 1;
				degree[b] = (degree[b] || 0) + 1;
			}

			const vertices = Object.keys(degree);
			const allDegreeTwo = vertices.every(v => degree[v] === 2);

			// Walk the chain to verify a single closed loop
			const adj = {};
			for (const [a, b] of edges) {
				if (!adj[a]) adj[a] = [];
				if (!adj[b]) adj[b] = [];
				adj[a].push(b);
				adj[b].push(a);
			}

			let visited = 0;
			const seen = new Set();
			let current = parseInt(vertices[0]);
			let prev = -1;
			while (!seen.has(current)) {
				seen.add(current);
				visited++;
				const neighbors = adj[current];
				const next = neighbors.find(n => n !== prev) ?? neighbors[0];
				prev = current;
				current = next;
			}

			return {
				edgeCount: edges.length,
				vertexCount: vertices.length,
				allDegreeTwo,
				visitedInLoop: visited,
				loopClosed: current === parseInt(vertices[0])
			};
		});

		// 6 teeth × 4 entities per tooth (2 splines + tip arc + root arc) = 24 edges
		expect(result.edgeCount).toBe(24);
		// Each edge contributes 2 unique connection vertices, but they're shared
		expect(result.allDegreeTwo).toBe(true);
		// Walking the graph should visit all vertices in one loop
		expect(result.visitedInLoop).toBe(result.vertexCount);
		expect(result.loopClosed).toBe(true);
	});

	test('gear entities have correct types and counts', async ({ waffle }) => {
		await clickSketch(waffle.page);

		await waffle.page.evaluate(() => {
			return window.__waffle.createGear({
				toothCount: 6,
				module: 2.0,
				pressureAngle: 20,
				backlash: 0,
				centerX: 0,
				centerY: 0,
				rotationOffset: 0
			});
		});

		const pointCount = await getEntityCountByType(waffle.page, 'Point');
		const splineCount = await getEntityCountByType(waffle.page, 'Spline');
		const arcCount = await getEntityCountByType(waffle.page, 'Arc');
		const circleCount = await getEntityCountByType(waffle.page, 'Circle');

		// 6 teeth: each tooth produces 2 splines (right + left involute)
		expect(splineCount).toBe(12);
		// 6 teeth: each tooth produces 2 arcs (tip + root)
		expect(arcCount).toBe(12);
		// Pitch circle as construction
		expect(circleCount).toBe(1);
		// Points: 1 center + 6 tooth starts + 6×(11 right mid + 1 right end + 1 left start + 11 left mid + 1 left end) = 157
		expect(pointCount).toBeGreaterThan(30);

		// All arcs should share a single center point
		const arcs = (await getEntities(waffle.page)).filter(e => e.type === 'Arc');
		const centerIds = new Set(arcs.map(a => a.center_id));
		expect(centerIds.size).toBe(1);
	});

	test('gear registry tracks created gear', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const gearId = await waffle.page.evaluate(() => {
			return window.__waffle.createGear({
				toothCount: 8,
				module: 1.0,
				pressureAngle: 20,
				backlash: 0,
				centerX: 0,
				centerY: 0,
				rotationOffset: 0
			});
		});

		const registry = await waffle.page.evaluate(() => {
			const reg = window.__waffle.getGearRegistry();
			const result = {};
			for (const [k, v] of reg) {
				result[k] = v;
			}
			return result;
		});

		expect(registry[gearId]).toBeDefined();
		expect(registry[gearId].toothCount).toBe(8);
		expect(registry[gearId].module).toBe(1.0);
	});

	test('gear sketch can be extruded', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);

		// Create gear sketch
		await clickSketch(waffle.page);

		await waffle.page.evaluate(() => {
			return window.__waffle.createGear({
				toothCount: 8,
				module: 1.0,
				pressureAngle: 20,
				backlash: 0,
				centerX: 0,
				centerY: 0,
				rotationOffset: 0
			});
		});

		await clickFinishSketch(waffle.page);
		await waitForFeatureCount(waffle.page, 1, 10000);

		// Extrude the gear profile
		await clickExtrude(waffle.page);

		// Set depth and apply
		const depthInput = waffle.page.locator('[data-testid="extrude-depth"]');
		await depthInput.fill('5');
		await waffle.page.locator('[data-testid="extrude-apply"]').click();

		// Wait for the extrude feature
		await waitForFeatureCount(waffle.page, 2, 15000);

		// Verify extrude feature exists
		const hasExtrude = await hasFeatureOfType(waffle.page, 'Extrude');
		expect(hasExtrude).toBe(true);

		expectNoAnyCrash(crashes);
	});
});
