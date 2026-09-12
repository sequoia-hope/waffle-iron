/**
 * Settings modal — the gear button opens a large modal with sections; toggles
 * persist (localStorage + reload); Appearance edits CSS color tokens live and
 * a complete scheme round-trips through the copy/paste text box.
 */
import { test, expect } from './helpers/waffle-test.js';

const GEAR = '[data-testid="toolbar-btn-settings"]';
const MODAL = '[data-testid="settings-modal"]';

async function cssVar(page, name) {
	return page.evaluate(
		(n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim(),
		name
	);
}

/** Set a color input's value the way a user's picker would (value + input event). */
async function pickColor(page, testId, hex) {
	await page.evaluate(({ id, v }) => {
		const el = document.querySelector(`[data-testid="${id}"]`);
		el.value = v;
		el.dispatchEvent(new Event('input', { bubbles: true }));
	}, { id: testId, v: hex });
}

test.describe('settings modal', () => {
	test('gear button opens and closes the modal', async ({ waffle }) => {
		const page = waffle.page;
		await page.locator(GEAR).click();
		await expect(page.locator(MODAL)).toBeVisible();
		await expect(page.locator('[data-testid="settings-section-general"]')).toBeVisible();
		await expect(page.locator('[data-testid="settings-section-appearance"]')).toBeVisible();
		await page.keyboard.press('Escape');
		await expect(page.locator(MODAL)).not.toBeVisible();
		await page.locator(GEAR).click();
		await page.locator('[data-testid="settings-close"]').click();
		await expect(page.locator(MODAL)).not.toBeVisible();
	});

	test('extrude auto-select toggle persists across reload', async ({ waffle }) => {
		const page = waffle.page;
		// The fixture seeds auto-select ON for legacy specs; turn it off here.
		await page.locator(GEAR).click();
		const box = page.locator('[data-testid="setting-extrude-auto-select"]');
		await expect(box).toBeChecked();
		await box.click();
		await expect(box).not.toBeChecked();
		let s = await page.evaluate(() => window.__waffle.getSettings());
		expect(s.extrudeAutoSelectRegion).toBe(false);

		await page.reload();
		await waffle.waitForReady();
		s = await page.evaluate(() => window.__waffle.getSettings());
		expect(s.extrudeAutoSelectRegion).toBe(false);
		await page.locator(GEAR).click();
		await expect(page.locator('[data-testid="setting-extrude-auto-select"]')).not.toBeChecked();
	});

	test('appearance: color override applies live, scheme copies and pastes, reset restores', async ({ waffle }) => {
		const page = waffle.page;
		const baseAccent = await cssVar(page, '--accent');
		expect(baseAccent).not.toBe('#ff0000');

		await page.locator(GEAR).click();
		await page.locator('[data-testid="settings-section-appearance"]').click();
		await pickColor(page, 'color-accent', '#ff0000');
		expect(await cssVar(page, '--accent')).toBe('#ff0000');
		await pickColor(page, 'color-sketch-default', '#123456');
		expect(await cssVar(page, '--sketch-default')).toBe('#123456');

		// Show the full scheme: it lists the base theme and every token.
		await page.locator('[data-testid="scheme-show"]').click();
		const text = await page.locator('[data-testid="scheme-text"]').inputValue();
		const scheme = JSON.parse(text);
		expect(scheme.format).toBe('waffle-color-scheme');
		expect(scheme.theme).toBe('default');
		expect(scheme.colors['--accent']).toBe('#ff0000');
		expect(scheme.colors['--sketch-default']).toBe('#123456');
		expect(scheme.colors['--bg-primary']).toMatch(/^#[0-9a-f]{6}$/);

		// Paste a modified scheme and apply it.
		scheme.colors['--accent'] = '#00ff00';
		await page.locator('[data-testid="scheme-text"]').fill(JSON.stringify(scheme));
		await page.locator('[data-testid="scheme-apply"]').click();
		expect(await cssVar(page, '--accent')).toBe('#00ff00');
		expect(await cssVar(page, '--sketch-default')).toBe('#123456');

		// Garbage is rejected loudly, not silently.
		await page.locator('[data-testid="scheme-text"]').fill('not json');
		await page.locator('[data-testid="scheme-apply"]').click();
		await expect(page.locator('[data-testid="scheme-error"]')).toBeVisible();
		expect(await cssVar(page, '--accent')).toBe('#00ff00');

		// Overrides survive a reload…
		await page.reload();
		await waffle.waitForReady();
		expect(await cssVar(page, '--accent')).toBe('#00ff00');

		// …and "Reset colors" returns to the theme baseline.
		await page.locator(GEAR).click();
		await page.locator('[data-testid="settings-section-appearance"]').click();
		await page.locator('[data-testid="settings-clear-colors"]').click();
		expect(await cssVar(page, '--accent')).toBe(baseAccent);
		await expect(page.locator('[data-testid="settings-clear-colors"]')).not.toBeVisible();
	});

	test('appearance: choosing a base theme in the modal switches it', async ({ waffle }) => {
		const page = waffle.page;
		await page.locator(GEAR).click();
		await page.locator('[data-testid="settings-section-appearance"]').click();
		await page.locator('[data-testid="settings-theme-retro"]').click();
		expect(await page.evaluate(() => document.documentElement.dataset.theme)).toBe('retro');
		await page.locator('[data-testid="settings-theme-default"]').click();
		expect(await page.evaluate(() => document.documentElement.dataset.theme)).toBe('default');
	});
});
