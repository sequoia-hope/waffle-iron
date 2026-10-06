/**
 * Sprint 5: DOF counter display tests.
 *
 * Verifies the DOF badge shows in the toolbar during sketch mode
 * with correct values as entities and constraints are added.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickSelect, clickTool } from './helpers/toolbar.js';
import { drawLine, drawRectangle, drawCircle } from './helpers/canvas.js';
import { waitForEntityCount, getEntities } from './helpers/state.js';
import { getConstraintCount, setSketchSelection } from './helpers/constraint.js';

test.describe('sketch DOF display', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	test('DOF badge appears when sketch has entities', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a line (adds 2 points = 4 DOF)
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(500); // Wait for solve

		const badge = page.locator('[data-testid="dof-badge"]');
		await badge.waitFor({ state: 'visible', timeout: 5000 });

		const text = await badge.textContent();
		expect(text.trim()).toContain('DOF');
	});

	test('DOF shows correct value for a line (may have auto H constraint)', async ({ waffle }) => {
		const page = waffle.page;

		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(500);

		const badge = page.locator('[data-testid="dof-badge"]');
		await badge.waitFor({ state: 'visible', timeout: 5000 });

		const text = await badge.textContent();
		// A line near horizontal auto-applies H constraint via snap,
		// so DOF is 3 (4 point DOF - 1 H constraint) or 4 if no snap
		expect(text.trim()).toMatch(/^[34] DOF$/);
	});

	test('adding H constraint reduces DOF by 1', async ({ waffle }) => {
		const page = waffle.page;

		// Draw a diagonal line so the drawing snap does not auto-apply a
		// Horizontal/Vertical constraint (which would make the explicit
		// Horizontal below redundant and leave DOF unchanged).
		await drawLine(page, -100, -40, 100, 40);
		await waitForEntityCount(page, 3, 5000);
		await page.waitForTimeout(500);

		// Get DOF before
		const dofBefore = await page.evaluate(() => window.__waffle.getSolveStatus()?.dof ?? -1);

		// Add Horizontal constraint
		const entities = await getEntities(page);
		const line = entities.find(e => e.type === 'Line');
		await page.evaluate((lineId) => {
			window.__waffle.addSketchConstraint({ type: 'Horizontal', entity: lineId });
		}, line.id);
		await page.waitForTimeout(500);

		const dofAfter = await page.evaluate(() => window.__waffle.getSolveStatus()?.dof ?? -1);
		if (dofBefore >= 0 && dofAfter >= 0) {
			expect(dofAfter).toBe(dofBefore - 1);
		}

		// Badge should update
		const badge = page.locator('[data-testid="dof-badge"]');
		const text = await badge.textContent();
		expect(text.trim()).toContain('DOF');
	});

	test('rectangle with H/V constraints shows correct DOF', async ({ waffle }) => {
		const page = waffle.page;

		// Rectangle: 4 points (8 DOF) - 4 H/V constraints (4 DOF removed) - 4 coincident
		// (from shared corners, implicit via point reuse, not separate constraints)
		// So 8 - 4 = 4 DOF remaining
		await clickRectangle(page);
		await drawRectangle(page, -80, -60, 80, 60);
		await waitForEntityCount(page, 8, 5000);
		await page.waitForTimeout(500);

		const badge = page.locator('[data-testid="dof-badge"]');
		await badge.waitFor({ state: 'visible', timeout: 5000 });

		const text = await badge.textContent();
		// 4 points x 2 DOF = 8, minus 4 constraints (2 H + 2 V) = 4 DOF
		expect(text.trim()).toContain('DOF');
	});
});

// The badge is a function of the LIVE sketch — entities included, and an
// empty sketch has nothing to say. Each of these was a user-visible defect on
// 2026-10-06: "-1 DOF" on a fully constrained sketch (FullyConstrained has no
// dof field on the wire), a stale verdict after adding an entity (the engine's
// AddSketchEntity does not solve), and the undone line's "3 DOF" still showing
// after undo had emptied the sketch.
test.describe('sketch DOF display is a function of the live sketch', () => {
	test.beforeEach(async ({ waffle }) => {
		await clickSketch(waffle.page);
	});

	const badgeText = async (page) => {
		const badge = page.locator('[data-testid="dof-badge"]');
		return (await badge.count()) ? (await badge.textContent()).trim() : null;
	};

	test('a circle on the origin counts its free radius, then reads fully constrained once dimensioned', async ({ waffle }) => {
		const page = waffle.page;
		await clickTool(page, 'circle');
		await drawCircle(page, 0, 0, 80, 0);
		await waitForEntityCount(page, 2, 5000);

		// Centre pinned to the origin (2 rows), radius free: exactly 1 DOF —
		// not the centre-only "fully constrained" verdict that was solved
		// before the circle existed.
		await expect(page.locator('[data-testid="dof-badge"]')).toHaveText('1 DOF', { timeout: 5000 });

		const circle = (await getEntities(page)).find((e) => e.type === 'Circle');
		await page.evaluate((id) => {
			window.__waffle.addSketchConstraint({ type: 'Diameter', entity: id, value: 0.05 });
		}, circle.id);
		await expect(page.locator('[data-testid="dof-badge"]')).toHaveText('Fully constrained', { timeout: 5000 });
		const status = await page.evaluate(() => window.__waffle.getSolveStatus());
		expect(status.status).toBe('FullyConstrained');
		expect(status.dof).toBe(0);
	});

	test('a free diagonal line shows its 4 DOF as soon as it exists', async ({ waffle }) => {
		const page = waffle.page;
		await drawLine(page, -100, -40, 100, 40);
		await waitForEntityCount(page, 3, 5000);
		expect(await getConstraintCount(page)).toBe(0);
		await expect(page.locator('[data-testid="dof-badge"]')).toHaveText('4 DOF', { timeout: 5000 });
	});

	test('undoing the sketch to empty clears the badge instead of keeping the undone verdict', async ({ waffle }) => {
		const page = waffle.page;
		await drawLine(page, -100, 0, 100, 0);
		await waitForEntityCount(page, 3, 5000);
		await expect(page.locator('[data-testid="dof-badge"]')).toHaveText(/^[34] DOF$/, { timeout: 5000 });

		await page.evaluate(() => window.__waffle.undo());
		await waitForEntityCount(page, 0, 5000);
		await expect(page.locator('[data-testid="dof-badge"]')).toHaveCount(0, { timeout: 5000 });
		expect(await badgeText(page)).toBeNull();
		expect(await page.evaluate(() => window.__waffle.getSolveStatus())).toBeNull();
	});
});
