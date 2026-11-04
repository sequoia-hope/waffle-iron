/**
 * DocumentStore revisions, conflict refusal, cross-tab watch, and conflict resolution UI:
 * - `get(id)` returns `{ doc, rev } | null`;
 * - `put(doc, { base })` refuses stale `base` with `{ conflict: { theirs_rev } }` and never overwrites;
 * - `watch(fn)` notifies listeners across same-origin tabs via `BroadcastChannel`;
 * - an unchanged save (`Ctrl+S` without edits, both within one second and across a timestamp tick) is never a conflict;
 * - two tabs editing the same document cannot overwrite each other silently;
 * - `Reload` adopts the latest stored version and allows subsequent saves;
 * - `Save as a copy` stores the conflicted tab's work under a new `document.id` while leaving the other tab's record untouched.
 */
import { test as rawTest, expect } from '@playwright/test';
import { seedDocument, getDocumentFromDB } from './helpers/waffle-test.js';
import { createExtrudedBox } from './helpers/geometry.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const REV_RE = /^git-blob-sha1:[0-9a-f]{40}$/;

function v4Doc({
	id,
	name = 'Shared Document',
	unit = 'mm',
	tabId = '9068ef01-1111-4222-8333-444455556666'
}) {
	return {
		format: 'waffle-iron',
		version: 4,
		min_reader_version: 4,
		document: {
			id,
			name,
			created: '2026-10-01T00:00:00.000Z',
			modified: '2026-10-01T00:00:00.000Z',
			display_unit: unit
		},
		sources: [],
		tabs: [
			{
				id: tabId,
				name: 'Part 1',
				kind: { type: 'Part', features: { features: [], active_index: null } }
			}
		],
		active_tab: tabId
	};
}

/**
 * Open a stored document via `/doc/<id>` and wait until `openDocumentRecord` settles.
 * @param {import('@playwright/test').Page} page
 * @param {string} docId
 */
async function openStoredDocInTab(page, docId) {
	await page.goto(`/doc/${docId}`);
	await page.waitForURL('/', { timeout: 15000 });
	await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, {
		timeout: 30000
	});
	await page.waitForFunction(
		(id) =>
			window.__waffle?.getDocumentState?.()?.activeDocId === id &&
			window.__waffle?.getDocumentLoadBusyReason?.() === null,
		docId,
		{ timeout: 15000 }
	);
}

/**
 * Press `Ctrl+S` and wait for the `'Saved'` toast.
 * @param {import('@playwright/test').Page} page
 */
async function pressCtrlSAndWaitSaved(page) {
	await page.evaluate(() => window.__waffle.dismissAllToasts());
	await page.keyboard.press('Control+s');
	await page.waitForFunction(
		() => (window.__waffle.getToasts() || []).some((t) => t.message === 'Saved'),
		null,
		{ timeout: 10000 }
	);
}

rawTest.describe('DocumentStore revisions and conflict resolution', () => {
	rawTest.describe.configure({ timeout: 180000 });
	rawTest.afterEach(async ({ page }, testInfo) => {
		if (testInfo.status === testInfo.expectedStatus) await page.goto('about:blank');
	});

	rawTest(
		'IndexedDBStore and GitProvider enforce { doc, rev }, put(doc, { base }) refusal, and cross-tab watch()',
		async ({ context, page: pageA }) => {
			const crashesA = collectCrashErrors(pageA);
			const pageB = await context.newPage();
			const crashesB = collectCrashErrors(pageB);

			await pageA.goto('/home');
			await pageB.goto('/home');

			const docId = 'c0f11c70-1111-4222-8333-000000000001';

			// Subscribe to `watch()` in pageB before pageA writes.
			await pageB.evaluate(async () => {
				const { getStore } = await import('/src/lib/storage/index.js');
				window.__watchEvents = [];
				window.__unwatchStore = getStore().watch((event) => {
					window.__watchEvents.push(event);
				});
			});

			const idbChecks = await pageA.evaluate(
				async ({ id, json0, json1, json2 }) => {
					const { getStore } = await import('/src/lib/storage/index.js');
					const store = getStore();

					const missing = await store.get(id);
					const wrongCreate = await store.put(
						{ id, json: json0, created: 1000, modified: 1000 },
						{ base: 'git-blob-sha1:0000000000000000000000000000000000000000' }
					);
					const stillMissing = await store.get(id);

					const created = await store.put(
						{ id, json: json0, created: 1000, modified: 1000 },
						{ base: null }
					);
					const read0 = await store.get(id);

					// Stale `base: null` when a record already exists must be refused without overwriting.
					const staleNull = await store.put(
						{ id, json: json1, created: 1000, modified: 2000 },
						{ base: null }
					);
					const afterStaleNull = await store.get(id);

					// Matching `base` updates the record and advances `rev`.
					const updated = await store.put(
						{ id, json: json1, created: 1000, modified: 2000 },
						{ base: created.rev }
					);
					const read1 = await store.get(id);

					// Stale `base: created.rev` after `updated.rev` must be refused without overwriting.
					const stalePrev = await store.put(
						{ id, json: json2, created: 1000, modified: 3000 },
						{ base: created.rev }
					);
					const afterStalePrev = await store.get(id);

					return {
						missing,
						wrongCreate,
						stillMissing,
						created,
						read0,
						staleNull,
						afterStaleNull,
						updated,
						read1,
						stalePrev,
						afterStalePrev
					};
				},
				{
					id: docId,
					json0: JSON.stringify(v4Doc({ id: docId, name: 'Rev Zero' })),
					json1: JSON.stringify(v4Doc({ id: docId, name: 'Rev One' })),
					json2: JSON.stringify(v4Doc({ id: docId, name: 'Stale Overwrite Attempt' }))
				}
			);

			expect(idbChecks.missing).toBeNull();
			expect(idbChecks.wrongCreate).toEqual({ conflict: { theirs_rev: null } });
			expect(idbChecks.stillMissing).toBeNull();
			expect(idbChecks.created.rev).toMatch(REV_RE);
			expect(idbChecks.read0.rev).toBe(idbChecks.created.rev);
			expect(JSON.parse(idbChecks.read0.doc.json).document.name).toBe('Rev Zero');

			expect(idbChecks.staleNull).toEqual({ conflict: { theirs_rev: idbChecks.created.rev } });
			expect(JSON.parse(idbChecks.afterStaleNull.doc.json).document.name).toBe('Rev Zero');

			expect(idbChecks.updated.rev).toMatch(REV_RE);
			expect(idbChecks.updated.rev).not.toBe(idbChecks.created.rev);
			expect(idbChecks.read1.rev).toBe(idbChecks.updated.rev);
			expect(JSON.parse(idbChecks.read1.doc.json).document.name).toBe('Rev One');

			expect(idbChecks.stalePrev).toEqual({ conflict: { theirs_rev: idbChecks.updated.rev } });
			expect(JSON.parse(idbChecks.afterStalePrev.doc.json).document.name).toBe('Rev One');

			// Verify cross-tab watch() delivered both committed changes to pageB (and none for refused puts).
			await expect
				.poll(
					() =>
						pageB.evaluate(() =>
							(window.__watchEvents || []).map((e) => ({
								type: e.type,
								id: e.id,
								rev: e.rev,
								provider: e.provider
							}))
						),
					{ timeout: 5000 }
				)
				.toEqual([
					{ type: 'changed', id: docId, rev: idbChecks.created.rev, provider: 'local' },
					{ type: 'changed', id: docId, rev: idbChecks.updated.rev, provider: 'local' }
				]);

			await pageB.evaluate(() => window.__unwatchStore?.());

			// Verify GitProvider explicit `base` check against an in-memory GitHub Contents API mock.
			const files = new Map();
			let revSeq = 0;
			await pageA.route('https://api.github.com/repos/acme/cad/contents/**', async (route) => {
				const req = route.request();
				const url = new URL(req.url());
				const relPath = decodeURIComponent(
					url.pathname.replace('/repos/acme/cad/contents/', '')
				);
				if (req.method() === 'GET') {
					if (!files.has(relPath)) {
						return route.fulfill({
							status: 404,
							contentType: 'application/json',
							body: '{"message":"Not Found"}'
						});
					}
					const entry = files.get(relPath);
					return route.fulfill({
						status: 200,
						contentType: 'application/json',
						body: JSON.stringify({
							content: Buffer.from(entry.text, 'utf8').toString('base64'),
							sha: entry.sha,
							encoding: 'base64'
						})
					});
				}
				if (req.method() === 'PUT') {
					const body = req.postDataJSON?.() ?? {};
					const existingEntry = files.get(relPath) ?? null;
					if (existingEntry ? body.sha !== existingEntry.sha : Boolean(body.sha)) {
						return route.fulfill({
							status: 409,
							contentType: 'application/json',
							body: '{"message":"sha mismatch"}'
						});
					}
					const text = Buffer.from(body.content, 'base64').toString('utf8');
					const sha = `gitsha-${++revSeq}-${text.length}`;
					files.set(relPath, { text, sha });
					return route.fulfill({
						status: 200,
						contentType: 'application/json',
						body: JSON.stringify({ content: { sha } })
					});
				}
				if (req.method() === 'DELETE') {
					if (!files.has(relPath)) {
						return route.fulfill({
							status: 404,
							contentType: 'application/json',
							body: '{"message":"Not Found"}'
						});
					}
					const body = req.postDataJSON?.() ?? {};
					const entry = files.get(relPath);
					if (!body.sha || entry.sha !== body.sha) {
						return route.fulfill({
							status: 409,
							contentType: 'application/json',
							body: '{"message":"sha mismatch"}'
						});
					}
					files.delete(relPath);
					return route.fulfill({
						status: 200,
						contentType: 'application/json',
						body: '{"content":null}'
					});
				}
				return route.fulfill({ status: 405, body: '' });
			});

			const gitChecks = await pageA.evaluate(
				async ({ id, json0, json1 }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const { setHostToken } = await import('/src/lib/storage/git/tokens.js');
					setHostToken('https://github.com', 'ghp_test', { kind: 'github' });
					const provider = new GitProvider({
						id: 'git:github.com/acme/cad',
						kind: 'github',
						remote: 'https://github.com/acme/cad',
						branch: 'main',
						folder: '',
						label: 'GitHub · acme/cad'
					});
					const c0 = await provider.put(
						{ id, json: json0, created: 1000, modified: 1000 },
						{ base: null }
					);
					const got0 = await provider.get(id);
					const conflictNull = await provider.put(
						{ id, json: json1, created: 1000, modified: 2000 },
						{ base: null }
					);
					const gotAfterRefusal = await provider.get(id);
					const c1 = await provider.put(
						{ id, json: json1, created: 1000, modified: 2000 },
						{ base: c0.rev }
					);
					return { c0, got0, conflictNull, gotAfterRefusal, c1 };
				},
				{
					id: docId,
					json0: JSON.stringify(v4Doc({ id: docId, name: 'Git Zero' })),
					json1: JSON.stringify(v4Doc({ id: docId, name: 'Git One' }))
				}
			);

			expect(gitChecks.c0.rev).toMatch(/^gitsha-/);
			expect(gitChecks.got0.rev).toBe(gitChecks.c0.rev);
			expect(gitChecks.conflictNull).toEqual({ conflict: { theirs_rev: gitChecks.c0.rev } });
			expect(JSON.parse(gitChecks.gotAfterRefusal.doc.json).document.name).toBe('Git Zero');
			expect(gitChecks.c1.rev).not.toBe(gitChecks.c0.rev);
			expect(files.has('git-zero.waffle')).toBe(false);
			expect(files.has('git-one.waffle')).toBe(true);

			await pageB.close();
			expectNoAnyCrash(crashesA);
			expectNoAnyCrash(crashesB);
		}
	);

	rawTest('an unchanged save is not a conflict', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const docId = 'c0f11c70-1111-4222-8333-000000000002';

		await page.goto('/home');
		await seedDocument(page, {
			id: docId,
			json: JSON.stringify(v4Doc({ id: docId, name: 'Unchanged Save Doc' })),
			created: Date.now(),
			modified: Date.now()
		});

		await openStoredDocInTab(page, docId);
		const initial = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(initial.activeDocRev).toMatch(REV_RE);
		expect(initial.conflict).toBeNull();

		// First Ctrl+S with no edits.
		await pressCtrlSAndWaitSaved(page);
		await expect(page.getByTestId('document-conflict-dialog')).toHaveCount(0);

		// Immediate second Ctrl+S (same-second / identical bytes check).
		await pressCtrlSAndWaitSaved(page);
		await expect(page.getByTestId('document-conflict-dialog')).toHaveCount(0);

		// Third Ctrl+S after >1s so Rust `SaveDocument`'s whole-second `document.modified` advances.
		await page.waitForTimeout(1100);
		await pressCtrlSAndWaitSaved(page);
		await expect(page.getByTestId('document-conflict-dialog')).toHaveCount(0);

		const after = await page.evaluate(() => window.__waffle.getDocumentState());
		expect(after.conflict).toBeNull();
		const stored = await getDocumentFromDB(page, docId);
		expect(stored.rev).toBe(after.activeDocRev);
		expectNoAnyCrash(crashes);
	});

	rawTest(
		'two tabs editing one document cannot overwrite each other silently, and Reload adopts the stored version',
		async ({ context, page: pageA }) => {
			const crashesA = collectCrashErrors(pageA);
			const pageB = await context.newPage();
			const crashesB = collectCrashErrors(pageB);
			const docId = 'c0f11c70-1111-4222-8333-000000000003';

			await pageA.goto('/home');
			await seedDocument(pageA, {
				id: docId,
				json: JSON.stringify(v4Doc({ id: docId, name: 'Shared Bracket' })),
				created: Date.now(),
				modified: Date.now()
			});

			await openStoredDocInTab(pageA, docId);
			await openStoredDocInTab(pageB, docId);

			const rev0A = (await pageA.evaluate(() => window.__waffle.getDocumentState())).activeDocRev;
			const rev0B = (await pageB.evaluate(() => window.__waffle.getDocumentState())).activeDocRev;
			expect(rev0A).toMatch(REV_RE);
			expect(rev0B).toBe(rev0A);

			// Tab A builds an extruded box and saves it (`revA`).
			await createExtrudedBox(pageA);
			await pressCtrlSAndWaitSaved(pageA);
			const revA = (await pageA.evaluate(() => window.__waffle.getDocumentState())).activeDocRev;
			expect(revA).not.toBe(rev0A);

			const storedAfterA = await getDocumentFromDB(pageA, docId);
			expect(storedAfterA.rev).toBe(revA);
			expect(JSON.parse(storedAfterA.json).tabs).toHaveLength(1);
			expect(JSON.parse(storedAfterA.json).tabs[0].kind.features.features).toHaveLength(2);

			// Tab B (still on `rev0B`) makes a conflicting edit (adds a second tab) and presses Ctrl+S.
			await pageB.evaluate(() => void window.__waffle.addTab('Part'));
			await pageB.waitForFunction(
				() => window.__waffle.getDocumentState().documentTabs.length === 2,
				null,
				{ timeout: 10000 }
			);
			await pageB.keyboard.press('Control+s');

			// Conflict dialog must appear in Tab B, and Tab A's stored record must be untouched.
			await expect(pageB.getByTestId('document-conflict-dialog')).toBeVisible({ timeout: 10000 });
			const storedAfterRefusal = await getDocumentFromDB(pageB, docId);
			expect(storedAfterRefusal.rev).toBe(revA);
			expect(storedAfterRefusal.json).toBe(storedAfterA.json);

			// Clicking Reload loads Tab A's saved version into Tab B and clears the conflict.
			await pageB.getByTestId('conflict-reload-btn').click();
			await expect(pageB.getByTestId('document-conflict-dialog')).toHaveCount(0);
			await pageB.waitForFunction(
				(expectedRev) => {
					const ds = window.__waffle.getDocumentState();
					const meshes = window.__waffle.getMeshes() ?? [];
					return (
						ds.activeDocRev === expectedRev &&
						ds.conflict === null &&
						ds.documentTabs.length === 1 &&
						(window.__waffle.getFeatureTree()?.features?.length ?? 0) === 2 &&
						meshes.some((m) => m.triangleCount > 0)
					);
				},
				revA,
				{ timeout: 15000 }
			);

			// Subsequent edits in Tab B now save cleanly on top of `revA`.
			await pageB.evaluate(() => void window.__waffle.addTab('Part'));
			await pageB.waitForFunction(
				() => window.__waffle.getDocumentState().documentTabs.length === 2,
				null,
				{ timeout: 10000 }
			);
			await pressCtrlSAndWaitSaved(pageB);
			await expect(pageB.getByTestId('document-conflict-dialog')).toHaveCount(0);
			const storedAfterReloadSave = await getDocumentFromDB(pageB, docId);
			expect(JSON.parse(storedAfterReloadSave.json).tabs).toHaveLength(2);
			expect(JSON.parse(storedAfterReloadSave.json).tabs[0].kind.features.features).toHaveLength(2);

			await pageB.close();
			expectNoAnyCrash(crashesA);
			expectNoAnyCrash(crashesB);
		}
	);

	rawTest(
		'Save as a copy preserves both tabs’ work under distinct document identities',
		async ({ context, page: pageA }) => {
			const crashesA = collectCrashErrors(pageA);
			const pageB = await context.newPage();
			const crashesB = collectCrashErrors(pageB);
			const docId = 'c0f11c70-1111-4222-8333-000000000004';

			await pageA.goto('/home');
			await seedDocument(pageA, {
				id: docId,
				json: JSON.stringify(v4Doc({ id: docId, name: 'Housing' })),
				created: Date.now(),
				modified: Date.now()
			});

			await openStoredDocInTab(pageA, docId);
			await openStoredDocInTab(pageB, docId);

			// Tab A adds an extruded box and saves.
			await createExtrudedBox(pageA);
			await pressCtrlSAndWaitSaved(pageA);
			const storedA = await getDocumentFromDB(pageA, docId);

			// Tab B adds an Assembly tab and attempts to save, triggering a conflict.
			await pageB.evaluate(() => void window.__waffle.addTab('Assembly'));
			await pageB.waitForFunction(
				() => window.__waffle.getDocumentState().documentTabs.length === 2,
				null,
				{ timeout: 10000 }
			);
			await pageB.keyboard.press('Control+s');
			await expect(pageB.getByTestId('document-conflict-dialog')).toBeVisible({ timeout: 10000 });

			// Click "Save as a copy" in Tab B.
			await pageB.getByTestId('conflict-save-copy-btn').click();
			await expect(pageB.getByTestId('document-conflict-dialog')).toHaveCount(0);

			await pageB.waitForFunction(
				(origId) => {
					const ds = window.__waffle.getDocumentState();
					return ds.activeDocId !== origId && ds.activeDocRev !== null && ds.conflict === null;
				},
				docId,
				{ timeout: 15000 }
			);

			const stateB = await pageB.evaluate(() => window.__waffle.getDocumentState());
			expect(stateB.activeDocId).toMatch(UUID_RE);
			expect(stateB.activeDocId).not.toBe(docId);
			expect(stateB.documentId).toBe(stateB.activeDocId);
			expect(stateB.documentName).toBe('Housing (copy)');
			expect(stateB.activeDocRev).toMatch(REV_RE);

			// Original record still holds Tab A's exact bytes; new record holds Tab B's work.
			const origRecord = await getDocumentFromDB(pageB, docId);
			expect(origRecord.rev).toBe(storedA.rev);
			expect(origRecord.json).toBe(storedA.json);
			expect(JSON.parse(origRecord.json).tabs[0].kind.features.features).toHaveLength(2);

			const copyRecord = await getDocumentFromDB(pageB, stateB.activeDocId);
			expect(copyRecord).toBeTruthy();
			expect(copyRecord.rev).toBe(stateB.activeDocRev);
			const parsedCopy = JSON.parse(copyRecord.json);
			expect(parsedCopy.document.id).toBe(stateB.activeDocId);
			expect(parsedCopy.document.name).toBe('Housing (copy)');
			expect(parsedCopy.tabs).toHaveLength(2);
			expect(parsedCopy.tabs.map((t) => t.kind.type)).toEqual(['Part', 'Assembly']);

			// Both documents appear on the Home screen.
			await pageB.goto('/home');
			await expect(pageB.locator('[data-testid="document-card"]')).toHaveCount(2, {
				timeout: 10000
			});
			await expect(pageB.locator('[data-testid="home-page"]')).toContainText('Housing');
			await expect(pageB.locator('[data-testid="home-page"]')).toContainText('Housing (copy)');

			await pageB.close();
			expectNoAnyCrash(crashesA);
			expectNoAnyCrash(crashesB);
		}
	);
});
