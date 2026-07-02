/**
 * Theme switcher tests — verifies the theme system:
 *   - default theme active on first load
 *   - selecting Retro sets data-theme, changes CSS variables, and persists
 *   - the choice survives a reload (anti-flash script applies it pre-paint)
 *   - switching back to Default clears the retro overrides
 *
 * The theme is pure CSS-variable swapping, so we assert on the resolved
 * `--text-primary` token and on `document.documentElement.dataset.theme`
 * rather than on pixels. No assertion-swallowing — waits throw on timeout.
 */
import { test, expect } from './helpers/waffle-test.js';

const TRIGGER = '[data-testid="theme-switcher-trigger"]';
const RETRO = '[data-testid="theme-option-retro"]';
const DEFAULT_OPT = '[data-testid="theme-option-default"]';

/** Resolved value of a CSS custom property on <html>, trimmed. */
async function cssVar(page, name) {
	return page.evaluate(
		(n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim(),
		name
	);
}

async function dataTheme(page) {
	return page.evaluate(() => document.documentElement.dataset.theme || null);
}

async function storedTheme(page) {
	return page.evaluate(() => localStorage.getItem('waffle:theme'));
}

test.describe('theme switcher', () => {
	test('defaults to the default theme', async ({ waffle }) => {
		const { page } = waffle;
		expect(await dataTheme(page)).toBe('default');
		// Default token — the standard dark editor grey.
		expect(await cssVar(page, '--text-primary')).toBe('#cccccc');
		// Text on the (dark blue) accent status bar is white.
		expect(await cssVar(page, '--text-on-accent')).toBe('#ffffff');
	});

	test('selecting Retro applies phosphor-green tokens and persists', async ({ waffle }) => {
		const { page } = waffle;

		await page.click(TRIGGER);
		await expect(page.locator(RETRO)).toBeVisible();
		await page.click(RETRO);

		// data-theme + resolved tokens flip to retro.
		expect(await dataTheme(page)).toBe('retro');
		expect(await cssVar(page, '--text-primary')).toBe('#33ff66');
		expect(await cssVar(page, '--viewport-bg')).toBe('#05070a');
		// Retro's fills are bright, so text sitting on them flips to near-black
		// (fixes white-on-light-green in the status bar / error toasts).
		expect(await cssVar(page, '--text-on-accent')).toBe('#0b0f0b');
		// Persisted for next visit.
		expect(await storedTheme(page)).toBe('retro');
	});

	test('retro survives a reload (applied before first paint)', async ({ waffle }) => {
		const { page } = waffle;

		await page.click(TRIGGER);
		await page.click(RETRO);
		expect(await dataTheme(page)).toBe('retro');

		await page.reload();
		// The inline anti-flash script sets this synchronously in <head>, so it
		// is correct even before the engine finishes booting.
		await expect(page.locator(TRIGGER)).toBeVisible();
		expect(await dataTheme(page)).toBe('retro');
		expect(await cssVar(page, '--text-primary')).toBe('#33ff66');
	});

	test('switching back to Default clears the retro overrides', async ({ waffle }) => {
		const { page } = waffle;

		await page.click(TRIGGER);
		await page.click(RETRO);
		expect(await cssVar(page, '--text-primary')).toBe('#33ff66');

		await page.click(TRIGGER);
		await expect(page.locator(DEFAULT_OPT)).toBeVisible();
		await page.click(DEFAULT_OPT);

		expect(await dataTheme(page)).toBe('default');
		expect(await cssVar(page, '--text-primary')).toBe('#cccccc');
		expect(await storedTheme(page)).toBe('default');
	});
});
