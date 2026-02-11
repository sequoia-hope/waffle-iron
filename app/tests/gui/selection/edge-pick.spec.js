/**
 * Edge picking — programmatic edge selection, hover highlights, and additive selection.
 */
import { test, expect } from '../helpers/waffle-test.js';
import { getCanvasBounds, clickAt } from '../helpers/canvas.js';

const EDGE_REF = {
	kind: { type: 'Edge' },
	anchor: { type: 'DatumPlane', plane: 'XY' },
	selector: { type: 'Role', role: { type: 'SideFace' }, index: 0 },
};

const FACE_REF = {
	kind: { type: 'Face' },
	anchor: { type: 'DatumPlane', plane: 'XY' },
	selector: { type: 'Role', role: { type: 'EndCapPositive' }, index: 0 },
};

test.describe('edge picking', () => {
	test('programmatic edge ref select via API', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate((ref) => window.__waffle.selectRef(ref, false), EDGE_REF);
		await page.waitForTimeout(100);

		const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(1);
		expect(selected[0].kind.type).toBe('Edge');
	});

	test('hover with edge ref highlights', async ({ waffle }) => {
		const page = waffle.page;

		await page.evaluate((ref) => window.__waffle.setHoveredRef(ref), EDGE_REF);
		await page.waitForTimeout(100);

		const hovered = await page.evaluate(() => window.__waffle.getHoveredRef());
		expect(hovered).not.toBeNull();
		expect(hovered.kind.type).toBe('Edge');
	});

	test('shift-click adds to selection', async ({ waffle }) => {
		const page = waffle.page;

		// Select first edge ref
		await page.evaluate((ref) => window.__waffle.selectRef(ref, false), EDGE_REF);
		await page.waitForTimeout(100);

		// Additively select a second edge ref (different index)
		const edgeRef2 = {
			...EDGE_REF,
			selector: { type: 'Role', role: { type: 'SideFace' }, index: 1 },
		};
		await page.evaluate((ref) => window.__waffle.selectRef(ref, true), edgeRef2);
		await page.waitForTimeout(100);

		const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(2);
	});

	test('face selection takes priority conceptually', async ({ waffle }) => {
		const page = waffle.page;

		// Select a face first
		await page.evaluate((ref) => window.__waffle.selectRef(ref, false), FACE_REF);
		await page.waitForTimeout(100);

		// Additively select an edge
		await page.evaluate((ref) => window.__waffle.selectRef(ref, true), EDGE_REF);
		await page.waitForTimeout(100);

		const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(2);

		const kinds = selected.map((r) => r.kind.type);
		expect(kinds).toContain('Face');
		expect(kinds).toContain('Edge');
	});

	test('edge-only selection when no face exists', async ({ waffle }) => {
		const page = waffle.page;

		// Clear any existing selection
		await page.evaluate(() => window.__waffle.clearSelection());
		await page.waitForTimeout(100);

		// Select only an edge ref
		await page.evaluate((ref) => window.__waffle.selectRef(ref, false), EDGE_REF);
		await page.waitForTimeout(100);

		const selected = await page.evaluate(() => window.__waffle.getSelectedRefs());
		expect(selected).toHaveLength(1);
		expect(selected[0].kind.type).toBe('Edge');
	});
});
