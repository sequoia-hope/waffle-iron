/**
 * Regression test for Review 1 Item 3:
 * `.waffle-index.json` writes in `GitProvider` must use compare-and-swap on the
 * index revision, re-reading and merging single-entry upserts/deletions onto the
 * latest remote index instead of clobbering concurrent entries or renames.
 */
import { test as rawTest, expect } from '@playwright/test';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

function v4Doc(id, name) {
	const tabId = '33333333-4444-4555-8666-777777777777';
	return JSON.stringify({
		format: 'waffle-iron',
		version: 4,
		min_reader_version: 4,
		document: {
			id,
			name,
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
	});
}

rawTest.describe('GitProvider .waffle-index.json compare-and-swap merge', () => {
	rawTest.afterEach(async ({ page }, testInfo) => {
		if (testInfo.status === testInfo.expectedStatus) await page.goto('about:blank');
	});

	rawTest(
		'merges concurrent creations, renames, 409 index conflicts, and deletions without losing entries',
		async ({ page }) => {
			const crashes = collectCrashErrors(page);
			const files = new Map();
			let seq = 0;
			let holdPutPath = null;
			let releaseHeldPut = null;
			let heldPutEntered = false;
			let injectConcurrentOnNextIndexPut = null;
			let index409Count = 0;

			await page.route('https://api.github.com/repos/acme/index-cas/contents/**', async (route) => {
				const req = route.request();
				const url = new URL(req.url());
				const relPath = decodeURIComponent(
					url.pathname.replace('/repos/acme/index-cas/contents/', '')
				);
				const method = req.method();

				if (method === 'GET') {
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

				if (method === 'PUT') {
					const body = req.postDataJSON?.() ?? {};
					if (holdPutPath === relPath) {
						holdPutPath = null;
						heldPutEntered = true;
						await new Promise((resolve) => {
							releaseHeldPut = resolve;
						});
					}
					if (relPath === '.waffle-index.json' && injectConcurrentOnNextIndexPut) {
						const inject = injectConcurrentOnNextIndexPut;
						injectConcurrentOnNextIndexPut = null;
						inject();
					}
					const current = files.get(relPath) ?? null;
					const reqSha = body.sha ?? null;
					if (current && reqSha !== current.sha) {
						if (relPath === '.waffle-index.json') index409Count += 1;
						return route.fulfill({
							status: reqSha === null ? 422 : 409,
							contentType: 'application/json',
							body: JSON.stringify({
								message: `${relPath} is at ${current.sha} but expected ${reqSha}`
							})
						});
					}
					if (!current && reqSha !== null) {
						return route.fulfill({
							status: 409,
							contentType: 'application/json',
							body: '{"message":"File does not exist"}'
						});
					}
					const text = Buffer.from(body.content, 'base64').toString('utf8');
					const nextSha = `sha-${++seq}`;
					files.set(relPath, { text, sha: nextSha });
					return route.fulfill({
						status: 200,
						contentType: 'application/json',
						body: JSON.stringify({ content: { sha: nextSha } })
					});
				}

				if (method === 'DELETE') {
					const body = req.postDataJSON?.() ?? {};
					const current = files.get(relPath) ?? null;
					if (current && body.sha && body.sha !== current.sha) {
						return route.fulfill({
							status: 409,
							contentType: 'application/json',
							body: '{"message":"Delete sha mismatch"}'
						});
					}
					files.delete(relPath);
					return route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
				}

				return route.fulfill({ status: 405, body: '' });
			});

			await page.goto('/home');
			await page.evaluate(() => {
				localStorage.setItem(
					'waffle-host-tokens',
					JSON.stringify([
						{ host_url: 'https://github.com', token: 'ghp_test', kind: 'github' }
					])
				);
			});

			const cfg = {
				id: 'git:github.com/acme/index-cas',
				kind: 'github',
				remote: 'https://github.com/acme/index-cas',
				branch: 'main',
				folder: '',
				label: 'GitHub · acme/index-cas'
			};

			const doc0Id = '30000000-1111-4222-8333-000000000000';
			const docBId = '30000000-1111-4222-8333-00000000000b';
			const docCId = '30000000-1111-4222-8333-00000000000c';
			const docDId = '30000000-1111-4222-8333-00000000000d';
			const docEId = '30000000-1111-4222-8333-00000000000e';

			// Seed doc0 ("Alpha" -> alpha.waffle).
			const seedRes = await page.evaluate(
				async ({ cfg, id, json }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const p = new GitProvider(cfg);
					return p.put({ id, json, created: 1000, modified: 1000 }, { base: null });
				},
				{ cfg, id: doc0Id, json: v4Doc(doc0Id, 'Alpha') }
			);
			expect('rev' in seedRes).toBe(true);

			// 1. Start Client B creating "Beta" (`beta.waffle`), holding `PUT beta.waffle`
			//    after Client B has already read `.waffle-index.json` (`[alpha.waffle]`).
			holdPutPath = 'beta.waffle';
			heldPutEntered = false;
			await page.evaluate(
				({ cfg, id, json }) => {
					window.__clientBPut = { settled: false, res: null };
					void (async () => {
						const { GitProvider } = await import('/src/lib/storage/git-provider.js');
						const pB = new GitProvider(cfg);
						window.__clientBPut.res = await pB.put(
							{ id, json, created: 2000, modified: 2000 },
							{ base: null }
						);
						window.__clientBPut.settled = true;
					})();
				},
				{ cfg, id: docBId, json: v4Doc(docBId, 'Beta') }
			);
			await expect.poll(() => heldPutEntered, { timeout: 10000 }).toBe(true);

			// Meanwhile Client A renames doc0 (`alpha.waffle` -> `alpha-v2.waffle`)
			// AND creates docC (`gamma.waffle`).
			await page.evaluate(
				async ({ cfg, doc0Id, doc0Json, doc0Base, docCId, docCJson }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const pA = new GitProvider(cfg);
					const r1 = await pA.put(
						{ id: doc0Id, json: doc0Json, created: 1000, modified: 1500 },
						{ base: doc0Base }
					);
					if ('conflict' in r1) throw new Error('unexpected rename conflict');
					const r2 = await pA.put(
						{ id: docCId, json: docCJson, created: 1600, modified: 1600 },
						{ base: null }
					);
					if ('conflict' in r2) throw new Error('unexpected docC conflict');
				},
				{
					cfg,
					doc0Id,
					doc0Json: v4Doc(doc0Id, 'Alpha V2'),
					doc0Base: seedRes.rev,
					docCId,
					docCJson: v4Doc(docCId, 'Gamma')
				}
			);

			// Release Client B: it must NOT revert doc0 back to deleted `alpha.waffle`
			// or drop `gamma.waffle`.
			releaseHeldPut();
			await page.waitForFunction(() => window.__clientBPut?.settled === true, null, {
				timeout: 10000
			});

			const indexAfterMerge = JSON.parse(files.get('.waffle-index.json').text);
			expect(
				indexAfterMerge.map((e) => ({ id: e.id, filename: e.filename }))
			).toEqual([
				{ id: doc0Id, filename: 'alpha-v2.waffle' },
				{ id: docCId, filename: 'gamma.waffle' },
				{ id: docBId, filename: 'beta.waffle' }
			]);
			expect(files.has('alpha.waffle')).toBe(false);

			// 2. True simultaneous `PUT .waffle-index.json` race (`409 Conflict` on first attempt):
			//    inject `docD` (`delta.waffle`) right as Client E sends `PUT .waffle-index.json`.
			injectConcurrentOnNextIndexPut = () => {
				const deltaJson = v4Doc(docDId, 'Delta');
				files.set('delta.waffle', { text: deltaJson, sha: `sha-${++seq}` });
				const curEntries = JSON.parse(files.get('.waffle-index.json').text);
				curEntries.push({
					id: docDId,
					name: 'Delta',
					filename: 'delta.waffle',
					created: 3000,
					modified: 3000,
					displayUnit: 'mm',
					tabCount: 1,
					previewMesh: null
				});
				files.set('.waffle-index.json', {
					text: JSON.stringify(curEntries, null, 2),
					sha: `sha-${++seq}`
				});
			};

			const clientERes = await page.evaluate(
				async ({ cfg, id, json }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const pE = new GitProvider(cfg);
					const putRes = await pE.put(
						{ id, json, created: 4000, modified: 4000 },
						{ base: null }
					);
					const listed = await pE.list();
					const loadedDoc0 = await pE.get('30000000-1111-4222-8333-000000000000');
					return { putRes, listedIds: listed.map((d) => d.id), loadedDoc0 };
				},
				{ cfg, id: docEId, json: v4Doc(docEId, 'Epsilon') }
			);

			expect(index409Count).toBe(1);
			expect('rev' in clientERes.putRes).toBe(true);
			expect(clientERes.listedIds.sort()).toEqual(
				[doc0Id, docBId, docCId, docDId, docEId].sort()
			);
			expect(JSON.parse(clientERes.loadedDoc0.doc.json).document.name).toBe('Alpha V2');

			expectNoAnyCrash(crashes);
		}
	);
});
