/**
 * KiCad board link (specs/kicad_board_link.md, oracle O10): paste a GitHub
 * link to a `.kicad_pcb` (the host API mocked, as git-links.spec.js does),
 * get an exact board and a board assembly; hover a component ⇒ a card with
 * its reference, value and footprint; click ⇒ the detail panel with pads,
 * nets and a link to the file at the resolved commit.
 */
import fs from 'fs';
import { test, expect } from './helpers/waffle-test.js';
import { moveToWorld, clickWorld } from './helpers/worldToScreen.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const RECT_V8 = fs.readFileSync(
	new URL('../../../crates/kicad-pcb/tests/fixtures/rect_v8.kicad_pcb', import.meta.url),
	'utf8'
);
const SHA = '9fceb02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const b64 = (s) => Buffer.from(s, 'utf8').toString('base64');

/** Serve the fixture as `acme/board` on a mocked GitHub API. */
async function mockGithub(page) {
	await page.route('https://api.github.com/**', async (route) => {
		const url = route.request().url();
		if (url.endsWith('/repos/acme/board/commits/main')) {
			return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ sha: SHA }) });
		}
		if (url.includes('/repos/acme/board/contents/hw/rect_v8.kicad_pcb?ref=' + SHA)) {
			return route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ content: b64(RECT_V8), sha: 'ce01', encoding: 'base64' })
			});
		}
		return route.fulfill({ status: 500, body: 'unexpected ' + url });
	});
}

const fitAll = (page) => page.evaluate(() => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f' })));

test.describe('KiCad board link', () => {
	test('a pasted GitHub link becomes an exact board and an assembly with board data on hover and click', async ({ waffle }) => {
		const page = waffle.page;
		const crashes = collectCrashErrors(page);
		await mockGithub(page);

		// The link dialog in its KiCad form, driven for real.
		await page.evaluate(() => window.__waffle.showImportLinkDialog('kicad'));
		const dialog = page.locator('[data-testid="import-link-dialog"]');
		await expect(dialog).toBeVisible();
		await expect(dialog).toHaveAttribute('data-kind', 'kicad');
		await page.locator('[data-testid="import-link-url"]').fill('https://github.com/acme/board/blob/main/hw/rect_v8.kicad_pcb');
		await page.locator('[data-testid="import-link-submit"]').click();
		await expect(dialog).not.toBeVisible({ timeout: 30000 });

		// Tabs: the board (active), two placeholders, the assembly.
		await page.waitForFunction(
			() => (window.__waffle.getDocumentState().documentTabs ?? []).some((t) => t.name === 'rect_v8 assembly'),
			{ timeout: 30000 }
		);
		const doc = await page.evaluate(() => window.__waffle.getDocumentState());
		const names = doc.documentTabs.map((t) => t.name);
		expect(names).toEqual(expect.arrayContaining(['rect_v8', 'R_0603_1608Metric placeholder', 'C_0805_2012Metric placeholder', 'rect_v8 assembly']));
		expect(doc.documentTabs.find((t) => t.id === doc.activeTabId)?.name).toBe('rect_v8');

		// The source is linked, at the resolved commit.
		const sources = await page.evaluate(() => window.__waffle.getSources());
		expect(sources).toHaveLength(1);
		expect(sources[0].kind).toBe('KicadPcb');
		expect(sources[0].resolved?.commit).toBe(SHA);
		expect(sources[0].pack).toBe(false);

		// The board renders; its bounding box tells the viewport's unit.
		await page.waitForFunction(() => (window.__waffle.getMeshes() ?? []).some((m) => m.triangleCount > 0), { timeout: 30000 });
		await fitAll(page);
		await page.waitForTimeout(300);
		const aabb = await page.evaluate(() => window.__waffle.getMeshBoundingBox());
		const u = aabb.max[0] / 0.05; // the board is 50 mm wide
		expect(Math.abs(aabb.max[2] - aabb.min[2] - 0.0016 * u)).toBeLessThan(1e-6 * u);

		// Hover the board's top face away from any footprint ⇒ the board card.
		await moveToWorld(page, [0.040 * u, -0.025 * u, 0.0016 * u]);
		const card = page.locator('[data-testid="kicad-hover-card"]');
		await expect(card).toBeVisible({ timeout: 5000 });
		await expect(card.locator('[data-testid="kicad-card-board"]')).toHaveText('Waffle test board');

		// The assembly: board + R1 + C1 placeholders.
		const asm = doc.documentTabs.find((t) => t.name === 'rect_v8 assembly');
		await page.evaluate((id) => window.__waffle.switchTab(id), asm.id);
		await page.waitForFunction(() => (window.__waffle.getMeshes() ?? []).length >= 3, { timeout: 30000 });
		await fitAll(page);
		await page.waitForTimeout(300);

		// R1 sits at (20, −15) mm on the copper top; its 1 mm placeholder's
		// top is at z = 2.6 mm. Hover it ⇒ the component card.
		const r1Top = [0.020 * u, -0.015 * u, 0.0026 * u];
		await moveToWorld(page, r1Top);
		await expect(card).toBeVisible({ timeout: 5000 });
		await expect(card.locator('[data-testid="kicad-card-reference"]')).toHaveText('R1');
		await expect(card).toContainText('10k');
		await expect(card).toContainText('R_0603_1608Metric');

		// Click ⇒ the detail panel: pads with nets, the datasheet, the source
		// at its commit on GitHub.
		await clickWorld(page, r1Top);
		const panel = page.locator('[data-testid="kicad-detail-panel"]');
		await expect(panel).toBeVisible({ timeout: 5000 });
		await expect(panel.locator('[data-testid="kicad-detail-reference"]')).toHaveText('R1');
		await expect(panel.locator('[data-testid="kicad-detail-pads"]')).toContainText('GND');
		await expect(panel.locator('[data-testid="kicad-detail-pads"]')).toContainText('VCC');
		await expect(panel.locator('[data-testid="kicad-detail-datasheet"]')).toHaveAttribute('href', 'https://example.invalid/r.pdf');
		await expect(panel.locator('[data-testid="kicad-detail-source"]')).toHaveAttribute(
			'href',
			`https://github.com/acme/board/blob/${SHA}/hw/rect_v8.kicad_pcb`
		);

		// Leaving the body hides the card; closing the panel hides it.
		await page.mouse.move(2, 2);
		await expect(card).not.toBeVisible({ timeout: 5000 });
		await panel.locator('[data-testid="kicad-detail-close"]').click();
		await expect(panel).not.toBeVisible();

		expectNoAnyCrash(crashes);
	});

	test('a refused file lands nothing and says why', async ({ waffle }) => {
		const page = waffle.page;
		const legacy = '(kicad_pcb (version 20171130) (host pcbnew 5.1.10) (general (thickness 1.6)))';
		const ok = await page.evaluate((t) => window.__waffle.importKicadFromText('old.kicad_pcb', t), legacy);
		expect(ok).toBe(false);
		const doc = await page.evaluate(() => window.__waffle.getDocumentState());
		// No board tab, no assembly tab, no source.
		expect((doc.documentTabs ?? []).filter((t) => /old|assembly|placeholder/.test(t.name))).toHaveLength(0);
		expect(await page.evaluate(() => window.__waffle.getSources())).toHaveLength(0);
	});
});
