/**
 * v4 Phase 2 P2-5a (specs/waffle_v4_document_model.md §4 inv. 1, §7.1): the
 * local storage record is keyed by the document's own identity. A new
 * document's record key IS its `document.id` and the editor's storage id
 * equals it; `/doc/<document.id>` opens a legacy 8-char-keyed record whose
 * JSON carries that identity; File→Open of an export of a stored document
 * re-homes to that record instead of forking a copy.
 */
import { test as rawTest, expect } from '@playwright/test';
import { seedDocument, getDocumentFromDB } from './helpers/waffle-test.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const DOC_ID = '0d0c1d00-1111-4222-8333-444455556666';

function v4Doc({ id = DOC_ID, name = 'Identity Doc', unit = 'mm' } = {}) {
	const tab = '9068ef01-1111-4222-8333-444455556666';
	return {
		format: 'waffle-iron',
		version: 4,
		min_reader_version: 4,
		document: { id, name, created: '2026-09-01T00:00:00.000Z', modified: '2026-09-01T00:00:00.000Z', display_unit: unit },
		sources: [],
		tabs: [{ id: tab, name: 'Part 1', kind: { type: 'Part', features: { features: [], active_index: null } } }],
		active_tab: tab
	};
}

async function waitForEditorDoc(page, predicate, arg) {
	await page.waitForURL('/', { timeout: 15000 });
	await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, { timeout: 30000 });
	await page.waitForFunction(predicate, arg, { timeout: 15000 });
}

rawTest.describe('Document identity as the storage key', () => {
	rawTest('a new document is stored under its own document.id', async ({ page }) => {
		await page.goto('/home');
		await page.waitForTimeout(500);
		await page.locator('[data-testid="new-document-btn"]').click();
		await page.waitForURL((url) => url.pathname.startsWith('/doc/'), { timeout: 10000 });
		const key = new URL(page.url()).pathname.split('/').pop();
		expect(key).toMatch(UUID_RE);
		await waitForEditorDoc(page, (k) => window.__waffle.getDocumentState().activeDocId === k, key);
		const state = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(state.documentId).toBe(key);
		const stored = await getDocumentFromDB(page, key);
		expect(JSON.parse(stored.json).document.id).toBe(key);
	});

	rawTest('/doc/<document.id> opens a legacy 8-char-keyed record by its identity', async ({ page }) => {
		await page.goto('/home');
		await seedDocument(page, { id: 'legacy01', json: JSON.stringify(v4Doc()), created: Date.now(), modified: Date.now() });
		await page.goto(`/doc/${DOC_ID}`);
		await waitForEditorDoc(page, () => window.__waffle.getDocumentState().activeDocId === 'legacy01');
		const state = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(state.documentId).toBe(DOC_ID);
		expect(state.documentName).toBe('Identity Doc');
		// The legacy key still works too, and a wrong identity opens nothing.
		await page.goto(`/doc/legacy01`);
		await waitForEditorDoc(page, () => window.__waffle.getDocumentState().activeDocId === 'legacy01');
		const missing = await page.evaluate(async () => {
			const { getStore } = await import('/src/lib/storage/index.js');
			return getStore().get('ffffffff-1111-4222-8333-444455556666');
		});
		expect(missing).toBeNull();
	});

	rawTest('File→Open of an export re-homes to the stored record with the same identity', async ({ page }) => {
		// A stored document keyed by its identity, opened in the editor.
		await page.goto('/home');
		const stored = { id: DOC_ID, json: JSON.stringify(v4Doc({ unit: 'in' })), created: Date.now(), modified: Date.now() };
		await seedDocument(page, stored);
		await page.goto(`/doc/${DOC_ID}`);
		await waitForEditorDoc(page, (id) => window.__waffle.getDocumentState().activeDocId === id, DOC_ID);

		// Open an export of the SAME document (same document.id, newer unit).
		const fcPromise = page.waitForEvent('filechooser');
		await page.evaluate(() => { window.__waffle.loadProject(); });
		const fc = await fcPromise;
		await fc.setFiles({ name: 'identity-doc.waffle', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(v4Doc({ unit: 'cm' }))) });
		await page.waitForFunction(() => window.__waffle.getDocumentState().documentDisplayUnit === 'cm', { timeout: 15000 });
		const state = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(state.activeDocId).toBe(DOC_ID);
		expect(state.documentId).toBe(DOC_ID);

		// Ctrl+S refuses to overwrite silently until the person confirms Replace,
		// then writes the opened content into THAT record.
		await page.keyboard.press('Control+s');
		await expect(page.locator('[data-testid="document-conflict-dialog"]')).toBeVisible({ timeout: 10000 });
		await page.locator('[data-testid="conflict-replace-btn"]').click();
		await expect(page.locator('[data-testid="document-conflict-dialog"]')).toBeHidden({ timeout: 10000 });
		await page.waitForFunction(async (id) => {
			return await new Promise((resolve) => {
				const req = indexedDB.open('waffle-iron', 1);
				req.onsuccess = () => {
					const get = req.result.transaction('documents', 'readonly').objectStore('documents').get(id);
					get.onsuccess = () => resolve(JSON.parse(get.result?.json ?? '{}')?.document?.display_unit === 'cm');
					get.onerror = () => resolve(false);
				};
				req.onerror = () => resolve(false);
			});
		}, DOC_ID, { timeout: 15000 });

		// A DIFFERENT document (no identity) still gets its own fresh record.
		const fc2Promise = page.waitForEvent('filechooser');
		await page.evaluate(() => { window.__waffle.loadProject(); });
		const fc2 = await fc2Promise;
		const legacy = { ...v4Doc({ name: 'Other' }), version: 3 };
		delete legacy.document.id;
		delete legacy.sources;
		delete legacy.min_reader_version;
		await fc2.setFiles({ name: 'other.waffle', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(legacy)) });
		await page.waitForFunction((id) => window.__waffle.getDocumentState().activeDocId !== id, DOC_ID, { timeout: 15000 });
		const other = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(other.activeDocId).toMatch(UUID_RE);
		expect(other.documentId).toBe(other.activeDocId);
	});
});
