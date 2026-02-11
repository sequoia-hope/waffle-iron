/**
 * Infrastructure tests — screenshot baseline system.
 *
 * Verifies that the screenshot helper correctly captures baselines
 * and applies masks to volatile UI elements.
 */
import { test, expect } from '../helpers/waffle-test.js';
import { assertScreenshot } from '../helpers/screenshot.js';

test.describe('screenshot baseline infrastructure', () => {
	test('initial screenshot matches itself', async ({ waffle }) => {
		// On first run this creates the baseline; on subsequent runs it compares.
		// Either way, the assertion should pass.
		await assertScreenshot(waffle.page, 'initial-viewport.png');
	});

	test('screenshot with mask excludes status bar', async ({ waffle }) => {
		// Provide an additional custom mask on top of the default status-message mask
		const customMask = waffle.page.locator('[data-testid="statusbar"]');

		// Should complete without error — the masked region is excluded from diff
		await assertScreenshot(waffle.page, 'masked-statusbar.png', {
			mask: [customMask],
		});
	});
});
