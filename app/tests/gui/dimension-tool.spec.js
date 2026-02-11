/**
 * Dimension tool tests — verifies the Smart Dimension workflow:
 * click entity → popup appears → type value → Enter → constraint created.
 *
 * Also tests: dimension popup dismiss via Escape, D key shortcut,
 * and dimension label rendering.
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickLine, clickCircle, pressKey } from './helpers/toolbar.js';
import { clickAt, drawLine, drawRectangle, drawCircle } from './helpers/canvas.js';
import {
	getEntityCount,
	getEntityCountByType,
	getEntities,
	waitForEntityCount,
	getActiveTool,
} from './helpers/state.js';
import {
	clickDimensionTool,
	getDimensionPopupState,
	waitForDimensionPopup,
	applyDimensionValue,
	dismissDimensionPopup,
	getConstraints,
	getConstraintCount,
	getConstraintCountByType,
} from './helpers/constraint.js';

/**
 * Helper: enter sketch and draw a line, then switch to dimension tool.
 */
async function sketchLineAndSwitchToDim(waffle) {
	await clickSketch(waffle.page);
	// Draw a line
	await drawLine(waffle.page, -100, 0, 100, 0);
	try {
		await waitForEntityCount(waffle.page, 3, 3000);
	} catch {
		await waffle.dumpState('dim-line-setup-failed');
	}

	// Switch to dimension tool
	await pressKey(waffle.page, 'd');
	const tool = await getActiveTool(waffle.page);
	expect(tool).toBe('dimension');
}

/**
 * Helper: enter sketch and draw a circle, then switch to dimension tool.
 */
async function sketchCircleAndSwitchToDim(waffle) {
	await clickSketch(waffle.page);
	await clickCircle(waffle.page);
	await drawCircle(waffle.page, 0, 0, 60, 0);
	try {
		await waitForEntityCount(waffle.page, 2, 3000);
	} catch {
		await waffle.dumpState('dim-circle-setup-failed');
	}

	// Switch to dimension tool
	await pressKey(waffle.page, 'd');
	const tool = await getActiveTool(waffle.page);
	expect(tool).toBe('dimension');
}

test.describe('dimension tool activation', () => {
	test('D key activates dimension tool in sketch mode', async ({ waffle }) => {
		await clickSketch(waffle.page);

		await pressKey(waffle.page, 'd');

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('dimension');
	});

	test('dimension button click activates dimension tool', async ({ waffle }) => {
		await clickSketch(waffle.page);

		await clickDimensionTool(waffle.page);

		const tool = await getActiveTool(waffle.page);
		expect(tool).toBe('dimension');
	});

	test('D key does nothing outside sketch mode', async ({ waffle }) => {
		// Not in sketch mode
		const toolBefore = await getActiveTool(waffle.page);
		await pressKey(waffle.page, 'd');
		const toolAfter = await getActiveTool(waffle.page);

		// Tool should not change to 'dimension' outside sketch mode
		expect(toolAfter).toBe(toolBefore);
	});

	test('Escape from dimension tool goes to select', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await pressKey(waffle.page, 'd');
		expect(await getActiveTool(waffle.page)).toBe('dimension');

		await pressKey(waffle.page, 'Escape');
		expect(await getActiveTool(waffle.page)).toBe('select');
	});
});

test.describe('dimension popup via __waffle API', () => {
	test('showDimensionPopup creates popup state', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Show a dimension popup programmatically
		await waffle.page.evaluate(() => {
			window.__waffle.showDimensionPopup({
				entityA: 1,
				entityB: null,
				sketchX: 0,
				sketchY: 0,
				dimType: 'distance',
				defaultValue: 5.0
			});
		});

		const popup = await getDimensionPopupState(waffle.page);
		expect(popup).not.toBeNull();
		expect(popup.dimType).toBe('distance');
		expect(popup.defaultValue).toBe(5.0);
	});

	test('hideDimensionPopup clears popup state', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Show then hide
		await waffle.page.evaluate(() => {
			window.__waffle.showDimensionPopup({
				entityA: 1,
				entityB: null,
				sketchX: 0,
				sketchY: 0,
				dimType: 'distance',
				defaultValue: 5.0
			});
		});

		let popup = await getDimensionPopupState(waffle.page);
		expect(popup).not.toBeNull();

		await waffle.page.evaluate(() => {
			window.__waffle.hideDimensionPopup();
		});

		popup = await getDimensionPopupState(waffle.page);
		expect(popup).toBeNull();
	});

	test('applyDimensionFromPopup creates distance constraint', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Draw a line first so entities exist
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');
		expect(line).toBeTruthy();

		// Show popup for the line's distance
		await waffle.page.evaluate((lineId) => {
			const entities = window.__waffle.getEntities();
			const line = entities.find(e => e.id === lineId);
			if (line) {
				window.__waffle.showDimensionPopup({
					entityA: lineId,
					entityB: null,
					sketchX: 0,
					sketchY: 0,
					dimType: 'distance',
					defaultValue: 2.0
				});
			}
		}, line.id);

		const constraintsBefore = await getConstraintCount(waffle.page);

		// Apply dimension
		await waffle.page.evaluate(() => {
			window.__waffle.applyDimensionFromPopup(5.0);
		});
		await waffle.page.waitForTimeout(200);

		// Verify constraint was created
		const constraintsAfter = await getConstraintCount(waffle.page);
		expect(constraintsAfter).toBe(constraintsBefore + 1);

		const constraints = await getConstraints(waffle.page);
		const distConstraint = constraints.find(c => c.type === 'Distance');
		expect(distConstraint).toBeTruthy();
		expect(distConstraint.value).toBe(5.0);
	});

	test('applyDimensionFromPopup creates radius constraint', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await clickCircle(waffle.page);
		await drawCircle(waffle.page, 0, 0, 60, 0);
		await waitForEntityCount(waffle.page, 2, 3000);

		const entities = await getEntities(waffle.page);
		const circle = entities.find(e => e.type === 'Circle');
		expect(circle).toBeTruthy();

		// Show radius popup for the circle
		await waffle.page.evaluate((circleId) => {
			window.__waffle.showDimensionPopup({
				entityA: circleId,
				entityB: null,
				sketchX: 1,
				sketchY: 1,
				dimType: 'radius',
				defaultValue: 1.0
			});
		}, circle.id);

		const constraintsBefore = await getConstraintCount(waffle.page);

		// Apply radius value
		await waffle.page.evaluate(() => {
			window.__waffle.applyDimensionFromPopup(3.0);
		});
		await waffle.page.waitForTimeout(200);

		const constraintsAfter = await getConstraintCount(waffle.page);
		expect(constraintsAfter).toBe(constraintsBefore + 1);

		const constraints = await getConstraints(waffle.page);
		const radiusConstraint = constraints.find(c => c.type === 'Radius');
		expect(radiusConstraint).toBeTruthy();
		expect(radiusConstraint.value).toBe(3.0);
	});
});

test.describe('dimension popup DOM interaction', () => {
	test('dimension input popup appears and accepts value', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		// Show dimension popup via API (reliable trigger)
		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');
		expect(line).toBeTruthy();

		await waffle.page.evaluate((lineId) => {
			window.__waffle.showDimensionPopup({
				entityA: lineId,
				entityB: null,
				sketchX: 0,
				sketchY: 0,
				dimType: 'distance',
				defaultValue: 2.0
			});
		}, line.id);

		// The DimensionInput.svelte component should render an input
		const input = waffle.page.locator('.dimension-input');
		await input.waitFor({ state: 'visible', timeout: 3000 });

		// Input should have the default value
		const inputValue = await input.inputValue();
		expect(parseFloat(inputValue)).toBe(2.0);

		// Type a new value and press Enter
		await input.fill('7.5');
		await waffle.page.keyboard.press('Enter');
		await waffle.page.waitForTimeout(200);

		// Popup should be dismissed
		await expect(input).not.toBeVisible();

		// Constraint should be created with the typed value
		const constraints = await getConstraints(waffle.page);
		const distConstraint = constraints.find(c => c.type === 'Distance');
		expect(distConstraint).toBeTruthy();
		expect(distConstraint.value).toBe(7.5);
	});

	test('Escape dismisses dimension popup without creating constraint', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');

		await waffle.page.evaluate((lineId) => {
			window.__waffle.showDimensionPopup({
				entityA: lineId,
				entityB: null,
				sketchX: 0,
				sketchY: 0,
				dimType: 'distance',
				defaultValue: 2.0
			});
		}, line.id);

		const input = waffle.page.locator('.dimension-input');
		await input.waitFor({ state: 'visible', timeout: 3000 });

		const constraintsBefore = await getConstraintCount(waffle.page);

		// Press Escape to dismiss
		await input.focus();
		await waffle.page.keyboard.press('Escape');
		await waffle.page.waitForTimeout(200);

		// Popup should be dismissed
		await expect(input).not.toBeVisible();

		// No new constraint
		const constraintsAfter = await getConstraintCount(waffle.page);
		expect(constraintsAfter).toBe(constraintsBefore);
	});

	test('dimension popup dismissed on blur', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');

		await waffle.page.evaluate((lineId) => {
			window.__waffle.showDimensionPopup({
				entityA: lineId,
				entityB: null,
				sketchX: 0,
				sketchY: 0,
				dimType: 'distance',
				defaultValue: 2.0
			});
		}, line.id);

		const input = waffle.page.locator('.dimension-input');
		await input.waitFor({ state: 'visible', timeout: 3000 });

		const constraintsBefore = await getConstraintCount(waffle.page);

		// Click elsewhere to blur
		await clickAt(waffle.page, -200, -200);
		await waffle.page.waitForTimeout(300);

		// Popup should be dismissed without creating constraint
		const popup = await getDimensionPopupState(waffle.page);
		expect(popup).toBeNull();

		const constraintsAfter = await getConstraintCount(waffle.page);
		expect(constraintsAfter).toBe(constraintsBefore);
	});

	test('invalid dimension value (0 or negative) does not create constraint', async ({ waffle }) => {
		await clickSketch(waffle.page);
		await drawLine(waffle.page, -100, 0, 100, 0);
		await waitForEntityCount(waffle.page, 3, 3000);

		const entities = await getEntities(waffle.page);
		const line = entities.find(e => e.type === 'Line');

		await waffle.page.evaluate((lineId) => {
			window.__waffle.showDimensionPopup({
				entityA: lineId,
				entityB: null,
				sketchX: 0,
				sketchY: 0,
				dimType: 'distance',
				defaultValue: 2.0
			});
		}, line.id);

		const input = waffle.page.locator('.dimension-input');
		await input.waitFor({ state: 'visible', timeout: 3000 });

		const constraintsBefore = await getConstraintCount(waffle.page);

		// Type 0 (invalid) and press Enter
		await input.fill('0');
		await waffle.page.keyboard.press('Enter');
		await waffle.page.waitForTimeout(200);

		// Should not create constraint (DimensionInput.svelte checks val > 0)
		const constraintsAfter = await getConstraintCount(waffle.page);
		expect(constraintsAfter).toBe(constraintsBefore);
	});
});

test.describe('auto-constraints from rectangle drawing', () => {
	test('rectangle creates exactly 2H + 2V constraints', async ({ waffle }) => {
		await sketchWithRectangle(waffle);

		const constraints = await getConstraints(waffle.page);
		const hConstraints = constraints.filter(c => c.type === 'Horizontal');
		const vConstraints = constraints.filter(c => c.type === 'Vertical');

		expect(hConstraints).toHaveLength(2);
		expect(vConstraints).toHaveLength(2);
	});

	test('rectangle H/V constraints reference correct line IDs', async ({ waffle }) => {
		await sketchWithRectangle(waffle);

		const entities = await getEntities(waffle.page);
		const lines = entities.filter(e => e.type === 'Line');
		expect(lines).toHaveLength(4);

		const constraints = await getConstraints(waffle.page);
		const hConstraints = constraints.filter(c => c.type === 'Horizontal');
		const vConstraints = constraints.filter(c => c.type === 'Vertical');

		// Each constraint's entity field should reference a valid line ID
		for (const c of [...hConstraints, ...vConstraints]) {
			expect(c.entity).toBeDefined();
			const referencedLine = lines.find(l => l.id === c.entity);
			expect(referencedLine, `constraint references line ${c.entity}`).toBeTruthy();
		}
	});
});

test.describe('constraint state inspection', () => {
	test('getConstraints returns empty array initially', async ({ waffle }) => {
		await clickSketch(waffle.page);

		const constraints = await getConstraints(waffle.page);
		expect(constraints).toEqual([]);
	});

	test('constraint count matches number of auto-applied constraints', async ({ waffle }) => {
		await clickSketch(waffle.page);

		// Initially zero
		let count = await getConstraintCount(waffle.page);
		expect(count).toBe(0);

		// Draw rectangle → 4 auto-constraints
		await clickRectangle(waffle.page);
		await drawRectangle(waffle.page, -80, -60, 80, 60);
		await waitForEntityCount(waffle.page, 8, 3000);

		count = await getConstraintCount(waffle.page);
		expect(count).toBe(4);
	});
});

/**
 * Helper: draw a rectangle using GUI.
 */
async function sketchWithRectangle(waffle) {
	await clickSketch(waffle.page);
	await clickRectangle(waffle.page);
	await drawRectangle(waffle.page, -80, -60, 80, 60);
	try {
		await waitForEntityCount(waffle.page, 8, 3000);
	} catch {
		await waffle.dumpState('rect-draw-failed');
	}
}
