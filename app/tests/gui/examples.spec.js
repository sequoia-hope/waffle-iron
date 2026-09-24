/**
 * The Examples panel (toolbar → Examples, beside Assay): lists the official
 * examples shipped in `app/static/examples/`, and opening one loads a COPY as
 * a new document — every tab adopted, the active assembly evaluated, bodies
 * rendered — under a fresh identity, so the shipped file is never written.
 */
import { test, expect } from '@playwright/test';
import { gunzipSync } from 'node:zlib';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const EXAMPLES_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../../static/examples');

/**
 * A shipped example as it sits in the repo: gzipped bytes and the JSON inside.
 *
 * Read from disk rather than over HTTP because the transport may inflate it
 * for us — Vite's dev server serves a `.gz` with `Content-Encoding: gzip`,
 * a static host generally does not — and what is being pinned here is the
 * SHIPPED ARTIFACT (FEATURE_NOTES §6), not what a particular server does with
 * it. The app copes with both, by sniffing the gzip magic.
 * @param {string} name
 */
function shippedExample(name) {
	const bytes = readFileSync(resolve(EXAMPLES_DIR, name));
	expect([bytes[0], bytes[1]], `${name} is gzip`).toEqual([0x1f, 0x8b]);
	return { bytes, json: JSON.parse(gunzipSync(bytes).toString('utf-8')) };
}

/** @param {import('@playwright/test').Page} page */
async function documentInfo(page) {
	return page.evaluate(() => window.__waffle.getDocumentInfo());
}

test.describe('Examples panel', () => {
	test.beforeEach(async ({ page }) => {
		await page.goto('/');
		await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, { timeout: 30000 });
	});

	test('lists the shipped examples and opens the gravel bike as a new document', async ({ page }) => {
		// The heaviest document in the suite by a wide margin: twelve tabs whose
		// active one is an assembly of fourteen instances, which the WASM engine
		// rebuilds and tessellates into 1.15 M triangles before anything renders.
		// ~24 s locally; a shared 4-core runner takes several times that, and the
		// 60 s default cut it off at zero bodies (CI, 2026-09-23). Still a real
		// bound — a document that never builds fails.
		test.setTimeout(300000);
		const crashes = collectCrashErrors(page);
		const before = await documentInfo(page);

		await page.getByTestId('toolbar-btn-examples').click();
		await expect(page.getByTestId('examples-browser')).toBeVisible();
		const bike = page.getByTestId('example-gravel-bike-v2');
		await expect(bike).toBeVisible();
		await expect(bike).toContainText('Gravel bike v2');
		await expect(page.getByTestId('examples-count')).toHaveText(/^[1-9]\d*$/);

		// Closes from its own header and reopens from the toolbar (the open panel
		// covers the toolbar's right end, as the Assay browser does).
		await page.getByTestId('examples-browser-close').click();
		await expect(page.getByTestId('examples-browser')).toBeHidden();
		await page.getByTestId('toolbar-btn-examples').click();
		await expect(page.getByTestId('examples-browser')).toBeVisible();

		await bike.click();
		// The whole document arrives: twelve tabs, the assembly active, and its
		// placed parts rendered (the bike is fourteen instances).
		await expect.poll(async () => (await documentInfo(page)).tabs?.length ?? 0, { timeout: 240000 }).toBe(12);
		const info = await documentInfo(page);
		expect(info.name).toBe('Gravel bike v2');
		expect(info.tabs.map((t) => t.name)).toEqual([
			'Frame', 'Fork', 'Wheel 700c', 'Cassette 11-42', 'Crankset', 'Chain', 'Cockpit',
			'Seatpost & saddle', 'Rear derailleur', 'Brake caliper', 'Water bottle', 'Gravel bike'
		]);
		const active = info.tabs.find((t) => t.id === info.activeTab);
		expect(active?.name).toBe('Gravel bike');
		await expect
			.poll(async () => page.evaluate(() => (window.__waffle.getMeshes() ?? []).filter((m) => m.triangleCount > 0).length), { timeout: 240000 })
			.toBeGreaterThanOrEqual(14);
		// A copy: the example opens under an identity of its own, never the file's.
		expect(info.storageId).not.toBe(before.storageId);
		const shipped = shippedExample('gravel-bike-v2.waffle.gz').json;
		expect(info.documentId).not.toBe(shipped.document.id);
		expect(info.documentId).toBe(info.storageId);

		// The details name the generator, served beside the document.
		await expect(page.getByTestId('examples-details')).toContainText('Gravel bike v2');
		const generator = page.getByTestId('example-generator-link');
		await expect(generator).toHaveAttribute('href', /gravel-bike-v2\.py$/);
		expect((await page.request.get('/examples/gravel-bike-v2.py')).ok()).toBe(true);
		expectNoAnyCrash(crashes);
	});

	test('opens the Eiffel Tower: eleven tabs and its whole lattice', async ({ page }) => {
		// 1,052 authored features raised to 1,964 bodies by four-fold
		// PatternCircular — the largest body count in the suite, though only
		// ~26 k triangles, since every body is a box. ~7 s locally; a shared
		// runner takes several times that (the bike's budget applies here too).
		test.setTimeout(300000);
		const crashes = collectCrashErrors(page);
		const before = await documentInfo(page);

		await page.getByTestId('toolbar-btn-examples').click();
		const tower = page.getByTestId('example-eiffel-tower');
		await expect(tower).toBeVisible();
		await expect(tower).toContainText('Eiffel Tower');
		await tower.click();

		await expect.poll(async () => (await documentInfo(page)).tabs?.length ?? 0, { timeout: 240000 }).toBe(11);
		const info = await documentInfo(page);
		expect(info.name).toBe('Eiffel Tower');
		expect(info.tabs.map((t) => t.name)).toEqual([
			'Piers', 'Legs', 'Arches', 'First platform', 'Second platform', 'Upper pylon',
			'Intermediate platform', 'Top platform', 'Campanile', 'Antenna mast', 'Eiffel Tower'
		]);
		expect(info.tabs.find((t) => t.id === info.activeTab)?.name).toBe('Eiffel Tower');

		// The whole lattice arrives, not a prefix of it: a quiet half-build
		// would still show a tower.
		await expect
			.poll(async () => page.evaluate(() => (window.__waffle.getMeshes() ?? []).filter((m) => m.triangleCount > 0).length), { timeout: 240000 })
			.toBe(1964);

		// A copy, as the bike is — and the file it copies is GZIPPED
		// (FEATURE_NOTES §6): 3.5 MB of pretty-printed JSON, ~177 KB shipped.
		// A plain `.waffle` would open just as well and nothing else would
		// notice, so the artifact itself is what gets pinned.
		expect(info.storageId).not.toBe(before.storageId);
		const { bytes, json: shipped } = shippedExample('eiffel-tower.waffle.gz');
		expect(bytes.length, 'an order of magnitude smaller than the JSON').toBeLessThan(400000);
		expect(gunzipSync(bytes).length, 'and that JSON is megabytes').toBeGreaterThan(2_000_000);
		expect(info.documentId).not.toBe(shipped.document.id);
		// It reaches the browser compressed too: whether the host labels it
		// `Content-Encoding: gzip` (Vite) or hands over the raw bytes (a
		// static host), the wire carries the compressed length.
		const res = await page.request.get('/examples/eiffel-tower.waffle.gz');
		expect(res.ok()).toBe(true);
		expect(Number(res.headers()['content-length'])).toBe(bytes.length);

		// The other half of `fetchExampleDocument`: on a STATIC host nothing
		// inflates the file in transit, so the browser itself has to. Vite's
		// dev server always labels it, so that branch would otherwise never
		// run under test — feed the shipped bytes to the same inflate here.
		const inflatedId = await page.evaluate(async (b64) => {
			const raw = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
			const stream = new Blob([raw]).stream().pipeThrough(new DecompressionStream('gzip'));
			return JSON.parse(await new Response(stream).text()).document.id;
		}, bytes.toString('base64'));
		expect(inflatedId).toBe(shipped.document.id);
		expect((await page.request.get('/examples/eiffel-tower.py')).ok()).toBe(true);
		expectNoAnyCrash(crashes);
	});
});
