/**
 * Regression test for Review 1 Item 1:
 * An in-flight save holding `saveChain` combined with a queued user save and an
 * agent mutating tool (`committed()` -> `commitPendingAutoSave({ via: 'agent' })`)
 * must never deadlock between `engineLock` and `saveChain`.
 *
 * Verifies both invariants:
 * 1. `committed()` releases `engineLock` before waiting on `saveChain` (even while
 *    storage I/O in `saveChain` is still blocked, `getEngineLockHolder()` returns
 *    to `null` and user engine queries remain responsive).
 * 2. Queued user saves compose outside `saveChain`, so draining `saveChain` never
 *    waits on `engineLock`, all saves + the agent tool settle cleanly, and the
 *    post-tool model is persisted to storage without user-origin sends leaking into
 *    the agent step's compose.
 */
import { test as rawTest, expect } from '@playwright/test';
import { seedDocument, getDocumentFromDB } from './helpers/waffle-test.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

function v4Doc(id) {
	const tabId = '11111111-2222-4333-8444-555555555555';
	return {
		format: 'waffle-iron',
		version: 4,
		min_reader_version: 4,
		document: {
			id,
			name: 'Deadlock Probe',
			created: '2026-10-01T00:00:00.000Z',
			modified: '2026-10-01T00:00:00.000Z',
			display_unit: 'mm'
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

rawTest.describe('Agent save + user save queue non-deadlock', () => {
	rawTest.afterEach(async ({ page }, testInfo) => {
		if (testInfo.status === testInfo.expectedStatus) await page.goto('about:blank');
	});

	rawTest(
		'queued user save and concurrent agent mutating tool do not deadlock on engineLock vs saveChain',
		async ({ page }) => {
			const crashes = collectCrashErrors(page);
			const docId = 'dead10cc-1111-4222-8333-000000000001';

			await page.goto('/home');
			await seedDocument(page, {
				id: docId,
				json: JSON.stringify(v4Doc(docId)),
				created: 1000,
				modified: 1000
			});

			await page.goto(`/doc/${docId}`);
			await page.waitForURL('/', { timeout: 15000 });
			await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, {
				timeout: 30000
			});
			await page.waitForFunction(
				(id) =>
					window.__waffle?.getDocumentState?.()?.activeDocId === id &&
					window.__waffle?.getDocumentLoadBusyReason?.() === null &&
					typeof window.__waffleAgentExecutor?.executeTool === 'function',
				docId,
				{ timeout: 15000 }
			);

			// Gate the very first IndexedDBStore.put so `saveChain` stays occupied
			// deterministically while a second user save and an agent command collide.
			await page.evaluate(async () => {
				const { getStore } = await import('/src/lib/storage/index.js');
				const store = getStore();
				const origPut = store.put.bind(store);
				let gatedOnce = false;
				let releaseGate = () => {};
				const gatePromise = new Promise((resolve) => {
					releaseGate = resolve;
				});

				window.__deadlockProbe = {
					firstPutBlocked: false,
					putCalls: 0,
					save1Ok: null,
					save2Ok: null,
					agentResult: null,
					allSettled: false,
					release() {
						store.put = origPut;
						releaseGate();
					}
				};

				store.put = async (doc, opts) => {
					window.__deadlockProbe.putCalls += 1;
					if (!gatedOnce) {
						gatedOnce = true;
						window.__deadlockProbe.firstPutBlocked = true;
						await gatePromise;
					}
					return origPut(doc, opts);
				};
			});

			// 1. Start User Save #1; wait until it is parked inside `store.put` holding `saveChain`.
			await page.evaluate(() => {
				void window.__waffle.saveToStorage().then((ok) => {
					window.__deadlockProbe.save1Ok = ok;
				});
			});
			await page.waitForFunction(
				() => window.__deadlockProbe?.firstPutBlocked === true,
				null,
				{ timeout: 10000 }
			);

			// 2. While `saveChain` is occupied, queue User Save #2 and run an agent
			//    mutating command (`tab_add`) whose `committed()` step flushes autosave.
			await page.evaluate(() => {
				const ctx = {
					agentName: 'deadlock-probe-agent',
					isPaused: () => false,
					pause: () => {},
					isCancelled: () => false
				};
				const save2Promise = window.__waffle.saveToStorage().then((ok) => {
					window.__deadlockProbe.save2Ok = ok;
				});
				const agentPromise = window.__waffleAgentExecutor
					.executeTool('tab_add', { name: 'Agent Part', kind: 'Part' }, ctx)
					.then((res) => {
						window.__deadlockProbe.agentResult = res;
					});
				void Promise.all([save2Promise, agentPromise]).then(() => {
					window.__deadlockProbe.allSettled = true;
				});
			});

			// 3. Even BEFORE `saveChain` is released, the agent's `tab_add` + snapshot
			//    compose must finish and release `engineLock` (`holder === null`), rather
			//    than holding `engineLock` while waiting on `saveChain`.
			await page.waitForFunction(
				() =>
					(window.__waffle?.getDocumentState?.()?.documentTabs?.length ?? 0) === 2 &&
					window.__waffle?.getEngineLockHolder?.() === null,
				null,
				{ timeout: 10000 }
			);

			// 4. Release the blocked first `store.put` and assert all saves + the agent
			//    tool settle cleanly without deadlocking.
			await page.evaluate(() => window.__deadlockProbe.release());
			await page.waitForFunction(
				() =>
					window.__deadlockProbe?.allSettled === true &&
					window.__deadlockProbe?.save1Ok !== null,
				null,
				{ timeout: 10000 }
			);

			const outcome = await page.evaluate(() => ({
				save1Ok: window.__deadlockProbe.save1Ok,
				save2Ok: window.__deadlockProbe.save2Ok,
				putCalls: window.__deadlockProbe.putCalls,
				agentResult: window.__deadlockProbe.agentResult,
				lockHolder: window.__waffle.getEngineLockHolder(),
				pendingAutoSave: window.__waffle.hasPendingAutoSave(),
				conflict: window.__waffle.getDocumentState().conflict
			}));

			expect(outcome.save1Ok).toBe(true);
			expect(outcome.save2Ok).toBe(true);
			expect(outcome.agentResult.isError).toBe(false);
			expect(outcome.agentResult.structuredContent.autosave_error).toBeUndefined();
			expect(outcome.lockHolder).toBeNull();
			expect(outcome.pendingAutoSave).toBe(false);
			expect(outcome.conflict).toBeNull();

			const stored = await getDocumentFromDB(page, docId);
			expect(stored).not.toBeNull();
			const storedParsed = JSON.parse(stored.json);
			expect(storedParsed.tabs.map((t) => t.name)).toEqual(['Part 1', 'Agent Part']);

			expectNoAnyCrash(crashes);
		}
	);
});
