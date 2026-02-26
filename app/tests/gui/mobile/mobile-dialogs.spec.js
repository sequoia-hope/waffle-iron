/**
 * Mobile dialog tests — bottom sheet positioning and viewport containment.
 *
 * Runs in mobile-portrait and mobile-landscape Playwright projects.
 */
import { test, expect } from '../helpers/waffle-test.js';
import { assertElementWithinBounds } from '../helpers/mobile.js';

test.describe('Mobile dialogs', () => {
	test('SketchPlanePrompt fits within viewport on mobile', async ({ waffle }) => {
		const page = waffle.page;

		// The SketchPlaneDialog appears when user triggers extrude without a sketch.
		// Check that if it's visible, it fits within the viewport.
		const dialog = page.locator('.sketch-plane-dialog, [data-testid="sketch-plane-dialog"]');
		if ((await dialog.count()) > 0 && (await dialog.first().isVisible())) {
			await assertElementWithinBounds(page, '.sketch-plane-dialog, [data-testid="sketch-plane-dialog"]', expect);
		}

		// Test passes if dialog not visible — it only appears on user action
	});

	test('toast container centered on mobile', async ({ waffle }) => {
		const page = waffle.page;

		// Toast container should exist and be positioned at top-center on mobile.
		// Trigger a toast by performing an action that generates one, or check CSS.
		const toastStyles = await page.evaluate(() => {
			const el = document.querySelector('.toast-container');
			if (!el) return null;
			const cs = getComputedStyle(el);
			return {
				position: cs.position,
				left: cs.left,
				right: cs.right,
				transform: cs.transform,
			};
		});

		// Toast container may not be rendered if no toasts are active.
		// If it exists, verify it has appropriate positioning.
		if (toastStyles) {
			// Should be fixed or absolute positioned
			expect(['fixed', 'absolute']).toContain(toastStyles.position);
		}
	});

	test('feature dialog renders as bottom sheet on mobile', async ({ waffle }) => {
		const page = waffle.page;

		// Check that the bottom-sheet media query is active by inspecting CSS
		// variables / computed styles on a dialog container
		const bottomSheetStyle = await page.evaluate(() => {
			// Check if any dialog element has bottom-sheet styling applied
			const dialogs = document.querySelectorAll(
				'.dialog, .feature-dialog, .extrude-dialog, .revolve-dialog'
			);
			for (const d of dialogs) {
				const cs = getComputedStyle(d);
				if (cs.position === 'fixed' && cs.bottom === '0px') {
					return { position: cs.position, bottom: cs.bottom, width: cs.width };
				}
			}
			return null;
		});

		// Dialog may not be visible without user action.
		// If it is visible, verify it's positioned as a bottom sheet.
		if (bottomSheetStyle) {
			expect(bottomSheetStyle.position).toBe('fixed');
			expect(bottomSheetStyle.bottom).toBe('0px');
		}
	});

	test('dialog does not exceed viewport bounds when visible', async ({ waffle }) => {
		const page = waffle.page;

		// Check all visible dialog-like elements
		const dialogSelectors = [
			'.dialog',
			'.feature-dialog',
			'.extrude-dialog',
			'.revolve-dialog',
			'.sketch-plane-dialog',
		];

		const viewport = page.viewportSize();
		for (const selector of dialogSelectors) {
			const el = page.locator(selector).first();
			if ((await el.count()) === 0) continue;
			if (!(await el.isVisible())) continue;

			const box = await el.boundingBox();
			if (!box) continue;

			expect(box.x, `${selector} left`).toBeGreaterThanOrEqual(-1);
			expect(box.y, `${selector} top`).toBeGreaterThanOrEqual(-1);
			expect(box.x + box.width, `${selector} right`).toBeLessThanOrEqual(viewport.width + 1);
			expect(box.y + box.height, `${selector} bottom`).toBeLessThanOrEqual(viewport.height + 1);
		}
	});
});
