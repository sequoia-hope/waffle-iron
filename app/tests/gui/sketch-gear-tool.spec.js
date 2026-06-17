/**
 * Gear tool — create gear via __waffle API, finish sketch, verify no errors.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickFinishSketch, clickExtrude, clickSelect } from './helpers/toolbar.js';
import { getConstraints, getSketchSelection } from './helpers/constraint.js';
import { clickAt } from './helpers/canvas.js';
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
				module: 0.001,
				pressureAngle: 20,
				backlash: 0,
				centerX: 0,
				centerY: 0,
				rotationOffset: 0
			});
		});

		expect(gearId).toBeDefined();

		// The gear is stored as a single compact Gear entity (not expanded primitives).
		const entities = await getEntities(waffle.page);
		const gearEntities = entities.filter(e => e.type === 'Gear');
		expect(gearEntities.length).toBe(1);
		expect(gearEntities[0].params.toothCount).toBe(8);

		// Its display expansion has valid (non-null, numeric) primitive ids.
		const display = await waffle.page.evaluate((gid) => window.__waffle.getGearDisplay()[gid], gearId);
		const splines = display.entities.filter(e => e.type === 'Spline');
		expect(splines.length).toBeGreaterThan(0);
		for (const spline of splines) {
			expect(Array.isArray(spline.point_ids)).toBe(true);
			expect(spline.point_ids.length).toBeGreaterThan(0);
			for (const pid of spline.point_ids) {
				expect(typeof pid).toBe('number');
			}
		}
		const arcs = display.entities.filter(e => e.type === 'Arc');
		for (const arc of arcs) {
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

		const gearId = await waffle.page.evaluate(() => {
			return window.__waffle.createGear({
				toothCount: 6,
				module: 0.001,
				pressureAngle: 20,
				backlash: 0,
				centerX: 0,
				centerY: 0,
				rotationOffset: 0
			});
		});

		// Build an adjacency map from the gear's display-primitive endpoints and
		// verify the profile forms a single closed loop.
		const result = await waffle.page.evaluate((gid) => {
			const entities = window.__waffle.getGearDisplay()[gid].entities;
			const splines = entities.filter(e => e.type === 'Spline');
			const arcs = entities.filter(e => e.type === 'Arc' && !e.construction);
			const lines = entities.filter(e => e.type === 'Line');

			// Collect edge endpoints: each edge entity connects two point IDs
			const edges = [];
			for (const s of splines) {
				edges.push([s.point_ids[0], s.point_ids[s.point_ids.length - 1]]);
			}
			for (const a of arcs) {
				edges.push([a.start_id, a.end_id]);
			}
			for (const l of lines) {
				edges.push([l.start_id, l.end_id]);
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
		}, gearId);

		// 6 teeth × 6 entities per tooth (2 splines + 2 lines + tip arc + root arc) = 36 edges
		expect(result.edgeCount).toBe(36);
		// Each edge contributes 2 unique connection vertices, but they're shared
		expect(result.allDegreeTwo).toBe(true);
		// Walking the graph should visit all vertices in one loop
		expect(result.visitedInLoop).toBe(result.vertexCount);
		expect(result.loopClosed).toBe(true);
	});

	test('gear entities have correct types and counts', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const gearId = await waffle.page.evaluate(() => {
			return window.__waffle.createGear({
				toothCount: 6,
				module: 0.002,
				pressureAngle: 20,
				backlash: 0,
				centerX: 0,
				centerY: 0,
				rotationOffset: 0
			});
		});

		// Canonical sketch holds exactly one compact Gear entity.
		expect(await getEntityCountByType(waffle.page, 'Gear')).toBe(1);

		// The display expansion carries the primitive type counts.
		const display = await waffle.page.evaluate((gid) => window.__waffle.getGearDisplay()[gid], gearId);
		const counts = display.counts;
		// 6 teeth: each tooth → 2 splines, 2 lines, 2 arcs; plus 1 construction pitch circle
		expect(counts.Spline).toBe(12);
		expect(counts.Line).toBe(12);
		expect(counts.Arc).toBe(12);
		expect(counts.Circle).toBe(1);
		expect(counts.Point).toBeGreaterThan(30);

		// All arcs share a single center point.
		const arcs = display.entities.filter(e => e.type === 'Arc');
		const centerIds = new Set(arcs.map(a => a.center_id));
		expect(centerIds.size).toBe(1);
	});

	test('gear registry tracks created gear', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const gearId = await waffle.page.evaluate(() => {
			return window.__waffle.createGear({
				toothCount: 8,
				module: 0.001,
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
		expect(registry[gearId].module).toBe(0.001);
	});

	test('internal gear: stored compact + pitch circle centered + no loose geometry', async ({ waffle }) => {
		// Regression for the internal-gear bugs, now under the compact-entity model:
		//  (b) the construction pitch circle must be centered on the gear center
		//      (it used to anchor on the first boundary vertex → an offset ring).
		//  (a) the gear is a single rigid Gear entity with NO loose primitives and
		//      NO anchor constraint flooding the solver — it is rigid by params.
		const crashes = collectCrashErrors(waffle.page);
		await clickSketch(waffle.page);

		const cx = 0.012;
		const cy = -0.007;
		const gearId = await waffle.page.evaluate(({ cx, cy }) => {
			return window.__waffle.createGear({
				toothCount: 14,
				module: 0.001,
				pressureAngleDeg: 20,
				backlash: 0,
				centerX: cx,
				centerY: cy,
				rotationOffset: 0,
				internal: true
			});
		}, { cx, cy });

		// (a) canonical sketch holds exactly one Gear entity and zero loose primitives.
		const entities = await getEntities(waffle.page);
		expect(entities.length).toBe(1);
		expect(entities[0].type).toBe('Gear');
		expect(entities[0].params.internal).toBe(true);

		// No anchor constraint is needed — the Gear entity is inherently rigid.
		const constraints = await getConstraints(waffle.page);
		expect(constraints.length).toBe(0);

		// (b) in the display expansion, the construction pitch circle is centered
		// on the gear center, not on a tooth-boundary vertex.
		const result = await waffle.page.evaluate((gid) => {
			const ents = window.__waffle.getGearDisplay()[gid].entities;
			const circle = ents.find(e => e.type === 'Circle' && e.construction);
			const center = ents.find(e => e.type === 'Point' && e.id === circle?.center_id);
			return { circleCenter: center ? { x: center.x, y: center.y } : null };
		}, gearId);
		expect(result.circleCenter).not.toBeNull();
		expect(result.circleCenter.x).toBeCloseTo(cx, 9);
		expect(result.circleCenter.y).toBeCloseTo(cy, 9);

		expectNoAnyCrash(crashes);
	});

	test('clicking a gear body selects the whole gear (one Gear entity)', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		const gearId = await page.evaluate(() => window.__waffle.createGear({
			toothCount: 10, module: 0.003, pressureAngleDeg: 20, backlash: 0,
			centerX: 0, centerY: 0, rotationOffset: 0
		}));
		const gearEntityId = await page.evaluate((gid) =>
			window.__waffle.getGearRegistry().get(gid).entityId, gearId);

		// Click the gear body (canvas center = sketch origin = gear center).
		await clickSelect(page);
		await clickAt(page, 0, 0);

		const selection = await getSketchSelection(page);
		expect(selection).toEqual([gearEntityId]);
	});

	test('double-clicking a gear opens the edit dialog with its params', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		await page.evaluate(() => window.__waffle.createGear({
			toothCount: 9, module: 0.003, pressureAngleDeg: 20, backlash: 0,
			centerX: 0, centerY: 0, rotationOffset: 0
		}));

		// Two rapid clicks inside the gear → double-click → edit dialog.
		await clickSelect(page);
		await clickAt(page, 0, 0);
		await clickAt(page, 0, 0);

		const dialog = page.locator('[data-testid="gear-dialog"]');
		await dialog.waitFor({ state: 'visible', timeout: 5000 });
		await expect(page.locator('[data-testid="gear-teeth-input"]')).toHaveValue('9');
	});

	test('deleting a gear entity clears its registry + display expansion', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);

		const gearId = await page.evaluate(() => window.__waffle.createGear({
			toothCount: 8, module: 0.002, pressureAngleDeg: 20, backlash: 0,
			centerX: 0, centerY: 0, rotationOffset: 0
		}));
		const gearEntityId = await page.evaluate((gid) =>
			window.__waffle.getGearRegistry().get(gid).entityId, gearId);

		await page.evaluate((eid) => window.__waffle.removeSketchEntities([eid]), gearEntityId);

		const state = await page.evaluate(() => ({
			gearEntities: window.__waffle.getEntities().filter(e => e.type === 'Gear').length,
			display: Object.keys(window.__waffle.getGearDisplay()).length,
			registry: window.__waffle.getGearRegistry().size
		}));
		expect(state.gearEntities).toBe(0);
		expect(state.display).toBe(0);
		expect(state.registry).toBe(0);
	});

	test('a gear survives save/reload: regrouped + editable in the reopened sketch', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await page.evaluate(() => window.__waffle.createGear({
			toothCount: 11, module: 0.003, pressureAngleDeg: 20, backlash: 0,
			centerX: 0, centerY: 0, rotationOffset: 0
		}));
		await clickFinishSketch(page);
		await waitForFeatureCount(page, 1, 10000);

		// Save and reload the document (grouping/display are session-only and must
		// be rebuilt from the persisted compact Gear entity).
		const json = await page.evaluate(() => window.__waffle.saveProject());
		await page.evaluate((j) => window.__waffle.loadProject(j), json);

		// Reopen the sketch for editing → rebuildGearsFromEntities runs.
		const sketchId = await page.evaluate(() =>
			window.__waffle.getFeatureTree().features.find(f => f.operation?.type === 'Sketch').id);
		await page.evaluate((id) => window.__waffle.enterSketchEditMode(id), sketchId);
		await page.waitForFunction(
			() => window.__waffle?.getState()?.sketchMode?.active === true, { timeout: 5000 });

		// One compact Gear entity persisted; its grouping + display were rebuilt.
		const state = await page.evaluate(() => ({
			gearEntities: window.__waffle.getEntities().filter(e => e.type === 'Gear').length,
			display: Object.keys(window.__waffle.getGearDisplay()).length,
			registry: window.__waffle.getGearRegistry().size,
			toothCount: window.__waffle.getEntities().find(e => e.type === 'Gear')?.params.toothCount,
			splines: Object.values(window.__waffle.getGearDisplay())[0]?.counts.Spline
		}));
		expect(state.gearEntities).toBe(1);
		expect(state.display).toBe(1);
		expect(state.registry).toBe(1);
		expect(state.toothCount).toBe(11);
		expect(state.splines).toBe(22); // 11 teeth × 2 involute flanks → the gear renders
	});

	test('gear sketch can be extruded', async ({ waffle }) => {
		const crashes = collectCrashErrors(waffle.page);

		// Create gear sketch
		await clickSketch(waffle.page);

		await waffle.page.evaluate(() => {
			return window.__waffle.createGear({
				toothCount: 8,
				module: 0.001,
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
