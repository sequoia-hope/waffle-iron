/**
 * The sketch's constraints as a list in the Properties panel (user request
 * 2026-10-30: "a ui to show sketch constraints and select and delete them,
 * and over constraints should make them visible automatically").
 *
 * Real pointer input draws the geometry and picks the rows; the conflict is
 * made the way a user makes one (Vertical applied to a line the snap already
 * made Horizontal, through the toolbar).
 */
import { test, expect } from './helpers/waffle-test.js';
import { clickSketch, clickRectangle, clickDimension } from './helpers/toolbar.js';
import { drawRectangle, clickAt } from './helpers/canvas.js';
import { waitForEntityCount } from './helpers/state.js';
import { getConstraintCount } from './helpers/constraint.js';

const rows = (page) => page.locator('[data-testid^="constraint-row-"]');

test.describe('sketch constraint list', () => {
	test('lists each constraint with a picture; a row selects it; × deletes it, undoably', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		await clickRectangle(page);
		await drawRectangle(page, -120, -80, 120, 80);
		await waitForEntityCount(page, 8, 5000);

		await expect(page.getByTestId('constraint-list')).toBeVisible();
		await expect(rows(page)).toHaveCount(4); // 2 Horizontal + 2 Vertical
		for (const row of await rows(page).all()) {
			await expect(row.locator('svg.wi-icon').first()).toBeVisible();
		}
		await expect(rows(page).filter({ hasText: 'Horizontal' })).toHaveCount(2);
		await expect(rows(page).filter({ hasText: 'Vertical' })).toHaveCount(2);

		// A row selects the same constraint a badge click does.
		const first = rows(page).first();
		const index = Number((await first.getAttribute('data-testid')).replace('constraint-row-', ''));
		await first.click();
		await expect(first).toHaveAttribute('aria-selected', 'true');
		expect(await page.evaluate(() => window.__waffle.getSelectedConstraintIndex())).toBe(index);

		// × deletes it; one undo brings it back.
		await page.getByTestId(`constraint-delete-${index}`).click();
		await expect(rows(page)).toHaveCount(3);
		expect(await getConstraintCount(page)).toBe(3);
		await page.keyboard.press('Control+z');
		await expect(rows(page)).toHaveCount(4);
	});

	test('a conflict is shown without asking: rows turn red and the first scrolls into view', async ({ waffle }) => {
		const page = waffle.page;
		await clickSketch(page);
		// Enough rows that the list scrolls: three rectangles = 12 constraints.
		await clickRectangle(page);
		await drawRectangle(page, -200, -120, -120, -60);
		await drawRectangle(page, -60, -120, 20, -60);
		await drawRectangle(page, 80, -120, 160, -60);
		await waitForEntityCount(page, 24, 8000);
		await expect(page.getByTestId('constraint-list-conflicts')).toHaveCount(0);

		// A real conflict, made with the Dimension tool: the LAST rectangle's
		// vertical sides force its top and bottom to the same length, so a
		// width of 50 on one and 80 on the other cannot both hold. (H + V on
		// one line is no conflict: the solver shrinks the line to a point.)
		// Each edge is aimed at where it IS, since the first dimension
		// scales the whole sketch.
		const horizontalEdgeMids = () =>
			page.evaluate(() => {
				const w = window.__waffle;
				const pos = w.getPositions();
				return w
					.getEntities()
					.filter((e) => e.type === 'Line')
					.slice(-4)
					.map((l) => [pos.get(l.start_id), pos.get(l.end_id)])
					.filter(([a, b]) => Math.abs(a.y - b.y) < 1e-9)
					.map(([a, b]) => w.sketchToScreenOffset((a.x + b.x) / 2, (a.y + b.y) / 2));
			});
		for (const [k, value, dy] of [[0, '50', -25], [1, '80', 25]]) {
			const mids = await horizontalEdgeMids();
			expect(mids.length).toBe(2);
			mids.sort((p, q) => p.y - q.y);
			const m = mids[k];
			await clickDimension(page);
			await clickAt(page, m.x, m.y);
			await clickAt(page, m.x, m.y + dy);
			const input = page.locator('.dimension-input');
			await input.waitFor({ state: 'visible', timeout: 3000 });
			await input.fill(value);
			await page.keyboard.press('Enter');
			await expect(input).not.toBeVisible();
		}

		await expect(page.getByTestId('constraint-list-conflicts')).toBeVisible();
		const failed = page.locator('[data-testid^="constraint-row-"][data-failed="true"]');
		await expect(failed.first()).toBeVisible();
		await expect(failed.first()).toBeInViewport();
	});
});
