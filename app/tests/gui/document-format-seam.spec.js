/**
 * Document-format seam regressions (docs/FILE_FORMAT.md §14, fixed 2026-08-28):
 *  - §14.2 `created` was re-stamped "now" on every save
 *  - §14.3 v3 `display_unit` was reset to mm on open (v2-only read path)
 *  - §14.4 File→Open kept stale JS tab state / dropped tabs from downloads
 *  - §13   files can demand a newer reader (`min_reader_version`) and must be
 *          refused cleanly, not with parse noise
 * These pin the JS writer's envelope — the production writer of the format.
 */
import { test, expect } from './helpers/waffle-test.js';
import { seedDocument, getDocumentFromDB } from './helpers/waffle-test.js';
import { test as rawTest } from '@playwright/test';

const T0 = '2020-01-02T03:04:05.000Z';

/** A two-tab v3 document; second tab active. */
function twoTabDoc({ created = T0, unit = 'in', minReader } = {}) {
	const doc = {
		format: 'waffle-iron',
		version: 3,
		document: { name: 'Two Tabs', created, modified: created, display_unit: unit },
		tabs: [
			{ id: 'tab-a', name: 'Part 1', kind: { type: 'Part', features: { features: [], active_index: null } } },
			{ id: 'tab-b', name: 'Part 2', kind: { type: 'Part', features: { features: [], active_index: null } } }
		],
		active_tab: 'tab-b'
	};
	if (minReader !== undefined) doc.min_reader_version = minReader;
	return doc;
}

/** Open a seeded storage doc via the /doc/{id} route and wait for adoption. */
async function openSeededDoc(page, doc) {
	await page.goto('/home');
	await seedDocument(page, doc);
	await page.goto(`/doc/${doc.id}`);
	await page.waitForURL('/', { timeout: 15000 });
	await page.waitForFunction(() => typeof window.__waffle !== 'undefined', { timeout: 30000 });
	await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, { timeout: 30000 });
	await page.waitForFunction(
		(id) => window.__waffle?.getDocumentState?.()?.activeDocId === id,
		doc.id,
		{ timeout: 15000 }
	);
}

rawTest.describe('Document format seam', () => {
	rawTest('storage save preserves created + display_unit, writes min_reader_version, keeps all tabs', async ({ page }) => {
		const doc = { id: 'seam001', json: JSON.stringify(twoTabDoc()), created: Date.now(), modified: Date.now() };
		await openSeededDoc(page, doc);

		// The open must adopt the stored metadata (created + unit + tabs).
		const state = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(state.documentCreated).toBe(T0);
		expect(state.documentDisplayUnit).toBe('in');
		expect(state.documentTabs.length).toBe(2);
		expect(state.activeTabId).toBe('tab-b');

		// The writer's envelope: created preserved, modified fresh, unit kept,
		// min_reader_version present, both tabs intact.
		const written = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		expect(written.format).toBe('waffle-iron');
		expect(written.version).toBe(3);
		expect(written.min_reader_version).toBe(3);
		expect(written.document.created).toBe(T0);
		expect(written.document.modified).not.toBe(T0);
		expect(written.document.display_unit).toBe('in');
		expect(written.tabs.map((t) => t.id)).toEqual(['tab-a', 'tab-b']);
		expect(written.active_tab).toBe('tab-b');

		// And the real Ctrl+S storage path writes the same envelope to IndexedDB.
		await page.keyboard.press('Control+s');
		await page.waitForTimeout(1500);
		const stored = await getDocumentFromDB(page, 'seam001');
		expect(stored).toBeTruthy();
		const storedJson = JSON.parse(stored.json);
		expect(storedJson.document.created).toBe(T0);
		expect(storedJson.min_reader_version).toBe(3);
		expect(storedJson.tabs.length).toBe(2);
	});

	rawTest('File→Open adopts the file tabs and detaches from the storage doc', async ({ page }) => {
		// Start attached to a storage doc, then open a different two-tab file.
		const doc = {
			id: 'seam002',
			json: JSON.stringify(twoTabDoc({ unit: 'mm' })),
			created: Date.now(),
			modified: Date.now()
		};
		await openSeededDoc(page, doc);

		const fileJson = JSON.stringify(twoTabDoc({ created: '2021-05-06T07:08:09.000Z', unit: 'cm' }));
		const fcPromise = page.waitForEvent('filechooser');
		await page.evaluate(() => { window.__waffle.loadProject(); });
		const fc = await fcPromise;
		await fc.setFiles({ name: 'opened-doc.waffle', mimeType: 'application/json', buffer: Buffer.from(fileJson) });

		await page.waitForFunction(
			() => window.__waffle.getDocumentState().activeDocId !== 'seam002',
			{ timeout: 15000 }
		);
		const state = await page.evaluate(() => window.__waffle.getDocumentState());
		// Re-homed to a FRESH storage doc (autosave must not overwrite seam002)...
		expect(state.activeDocId).toBeTruthy();
		expect(state.activeDocId).not.toBe('seam002');
		// ...and the file's structure + metadata adopted; name comes from the filename.
		expect(state.documentTabs.length).toBe(2);
		expect(state.activeTabId).toBe('tab-b');
		expect(state.documentCreated).toBe('2021-05-06T07:08:09.000Z');
		expect(state.documentDisplayUnit).toBe('cm');
		expect(state.documentName).toBe('opened-doc');

		// A save after the open carries the whole opened document.
		const written = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		expect(written.tabs.length).toBe(2);
		expect(written.document.created).toBe('2021-05-06T07:08:09.000Z');

		// The storage doc was not clobbered by the open.
		const stored = await getDocumentFromDB(page, 'seam002');
		expect(JSON.parse(stored.json).document.display_unit).toBe('mm');
	});

	test('a file demanding a newer reader is refused cleanly', async ({ waffle }) => {
		const page = waffle.page;
		const before = await page.evaluate(() => window.__waffle.getDocumentState());
		const fileJson = JSON.stringify(twoTabDoc({ minReader: 99 }));

		const fcPromise = page.waitForEvent('filechooser');
		await page.evaluate(() => { window.__waffle.loadProject(); });
		const fc = await fcPromise;
		await fc.setFiles({ name: 'from-the-future.waffle', mimeType: 'application/json', buffer: Buffer.from(fileJson) });

		await page.waitForFunction(
			() => (window.__waffle.getToasts() || []).some(
				(t) => t.level === 'error' && /newer version/i.test(t.message)
			),
			{ timeout: 10000 }
		);
		// Nothing was adopted — document state is exactly what it was.
		const after = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(after.activeDocId).toBe(before.activeDocId);
		expect(after.activeTabId).toBe(before.activeTabId);
		expect(after.documentTabs).toEqual(before.documentTabs);
		const tree = await page.evaluate(() => window.__waffle.getFeatureTree());
		expect(tree.features.length).toBe(0);
	});

	test('direct-`/` editor save carries the live tree and the envelope fields', async ({ waffle }) => {
		const page = waffle.page;
		// Test SETUP via API (drawing interactions have their own specs).
		await page.evaluate(() => {
			window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]);
			window.__waffle.addSketchEntity({ id: 1, type: 'Point', x: 0, y: 0, construction: false });
			window.__waffle.addSketchEntity({ id: 2, type: 'Point', x: 0.01, y: 0, construction: false });
			window.__waffle.addSketchEntity({ id: 10, type: 'Line', start_id: 1, end_id: 2, construction: false });
			window.__waffle.finishSketch();
		});
		await page.waitForFunction(
			() => window.__waffle.getFeatureTree().features.length === 1,
			{ timeout: 15000 }
		);

		const written = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		expect(written.tabs.length).toBe(1);
		expect(written.active_tab).toBe(written.tabs[0].id);
		expect(written.tabs[0].kind.features.features.length).toBe(1);
		expect(written.min_reader_version).toBe(3);
		expect(written.document.created).toBeTruthy();

		// created is latched: a second save keeps it.
		const again = JSON.parse(await page.evaluate(() => window.__waffle.buildDocumentJson()));
		expect(again.document.created).toBe(written.document.created);
	});
});
