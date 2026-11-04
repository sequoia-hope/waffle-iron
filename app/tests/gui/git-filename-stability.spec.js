/**
 * Regression test for Review 1 Item 4:
 * 1. An existing `.waffle-index.json` entry keeps its `filename` unless the user
 *    renames THAT document (preventing `x-<id8>.waffle` -> `x.waffle` flips when
 *    sibling documents are renamed/deleted, which would break share links and
 *    `Relative` locators, and preventing legacy shared-filename entries from
 *    migrating on an ordinary save).
 * 2. `GitProvider` never deletes a file path that another `.waffle-index.json`
 *    entry still references (on rename or on delete).
 */
import { test as rawTest, expect } from '@playwright/test';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

function v4Doc(id, name, unit = 'mm') {
	const tabId = '44444444-5555-4666-8777-888888888888';
	return JSON.stringify({
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
	});
}

rawTest.describe('GitProvider filename stability and shared-path safety', () => {
	rawTest.afterEach(async ({ page }, testInfo) => {
		if (testInfo.status === testInfo.expectedStatus) await page.goto('about:blank');
	});

	rawTest(
		'never orphans legacy shared filenames on save/rename/delete and never flips disambiguated filenames when siblings change',
		async ({ page }) => {
			const crashes = collectCrashErrors(page);
			const files = new Map();
			let seq = 0;

			await page.route('https://api.github.com/repos/acme/filename-stable/contents/**', async (route) => {
				const req = route.request();
				const url = new URL(req.url());
				const relPath = decodeURIComponent(
					url.pathname.replace('/repos/acme/filename-stable/contents/', '')
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
					const current = files.get(relPath) ?? null;
					const reqSha = body.sha ?? null;
					if (current && reqSha !== current.sha) {
						return route.fulfill({
							status: reqSha === null ? 422 : 409,
							contentType: 'application/json',
							body: '{"message":"Conflict"}'
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
				id: 'git:github.com/acme/filename-stable',
				kind: 'github',
				remote: 'https://github.com/acme/filename-stable',
				branch: 'main',
				folder: '',
				label: 'GitHub · acme/filename-stable'
			};

			// --- Part A: Legacy repo where two entries already share `untitled.waffle` ---
			const legacyA = '4000000a-1111-4222-8333-00000000000a';
			const legacyB = '4000000b-1111-4222-8333-00000000000b';
			const sharedC = '4000000c-1111-4222-8333-00000000000c';
			const sharedD = '4000000d-1111-4222-8333-00000000000d';

			files.set('untitled.waffle', {
				text: v4Doc(legacyA, 'Untitled', 'mm'),
				sha: 'sha-legacy-untitled'
			});
			files.set('shared.waffle', {
				text: v4Doc(sharedC, 'Shared', 'mm'),
				sha: 'sha-legacy-shared'
			});
			files.set('.waffle-index.json', {
				text: JSON.stringify(
					[
						{
							id: legacyA,
							name: 'Untitled',
							filename: 'untitled.waffle',
							created: 1000,
							modified: 1000
						},
						{
							id: legacyB,
							name: 'Untitled',
							filename: 'untitled.waffle',
							created: 1000,
							modified: 1000
						},
						{
							id: sharedC,
							name: 'Shared',
							filename: 'shared.waffle',
							created: 1000,
							modified: 1000
						},
						{
							id: sharedD,
							name: 'Shared',
							filename: 'shared.waffle',
							created: 1000,
							modified: 1000
						}
					],
					null,
					2
				),
				sha: 'sha-legacy-idx'
			});

			const legacyOutcome = await page.evaluate(
				async ({ cfg, legacyA, legacyB, sharedC, sharedD, saveSameNameJson, renameAJson }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const p = new GitProvider(cfg);

					// 1. Ordinary save on `legacyA` (same name 'Untitled') keeps `untitled.waffle`
					//    and does not orphan `legacyB`.
					const readA0 = await p.get(legacyA);
					const saveSame = await p.put(
						{ id: legacyA, json: saveSameNameJson, created: 1000, modified: 2000 },
						{ base: readA0.rev }
					);
					const locatorAAfterSave = await p.getLocator(legacyA);
					const readBAfterSave = await p.get(legacyB);

					// 2. Explicit rename of `legacyA` ('Untitled' -> 'Bracket') writes `bracket.waffle`
					//    and does NOT delete `untitled.waffle` because `legacyB` still references it.
					const renameA = await p.put(
						{ id: legacyA, json: renameAJson, created: 1000, modified: 3000 },
						{ base: saveSame.rev }
					);
					const locatorAAfterRename = await p.getLocator(legacyA);
					const readBAfterRename = await p.get(legacyB);

					// 3. Deleting `sharedC` removes its index entry WITHOUT deleting `shared.waffle`
					//    while `sharedD` still references it; deleting `sharedD` then deletes `shared.waffle`.
					await p.delete(sharedC);
					const readDAfterCDeleted = await p.get(sharedD);
					await p.delete(sharedD);
					const readDAfterBothDeleted = await p.get(sharedD);

					return {
						saveSame,
						locatorAAfterSave,
						readBAfterSave,
						renameA,
						locatorAAfterRename,
						readBAfterRename,
						readDAfterCDeleted,
						readDAfterBothDeleted
					};
				},
				{
					cfg,
					legacyA,
					legacyB,
					sharedC,
					sharedD,
					saveSameNameJson: v4Doc(legacyA, 'Untitled', 'cm'),
					renameAJson: v4Doc(legacyA, 'Bracket', 'cm')
				}
			);

			expect('rev' in legacyOutcome.saveSame).toBe(true);
			expect(legacyOutcome.locatorAAfterSave.path).toBe('untitled.waffle');
			expect(legacyOutcome.readBAfterSave).not.toBeNull();

			expect('rev' in legacyOutcome.renameA).toBe(true);
			expect(legacyOutcome.locatorAAfterRename.path).toBe('bracket.waffle');
			expect(legacyOutcome.readBAfterRename).not.toBeNull();
			expect(files.has('untitled.waffle')).toBe(true);
			expect(files.has('bracket.waffle')).toBe(true);

			expect(legacyOutcome.readDAfterCDeleted).not.toBeNull();
			expect(legacyOutcome.readDAfterBothDeleted).toBeNull();
			expect(files.has('shared.waffle')).toBe(false);

			// --- Part B: Disambiguated `part-<id8>.waffle` stays stable when sibling `part.waffle` changes ---
			const part1 = '40000001-1111-4222-8333-000000000001';
			const part2 = '40000002-1111-4222-8333-000000000002';

			const stabilityOutcome = await page.evaluate(
				async ({ cfg, part1, part2, p1Json, p1RenamedJson, p2Json0, p2Json1, p2RenamedJson }) => {
					const { GitProvider } = await import('/src/lib/storage/git-provider.js');
					const p = new GitProvider(cfg);

					const c1 = await p.put(
						{ id: part1, json: p1Json, created: 5000, modified: 5000 },
						{ base: null }
					);
					const c2 = await p.put(
						{ id: part2, json: p2Json0, created: 5100, modified: 5100 },
						{ base: null }
					);
					const loc2Before = await p.getLocator(part2);
					const share2Before = await p.getShareUrl(part2);

					// Rename `part1` ('Part' -> 'Frame'), freeing `part.waffle`.
					const r1 = await p.put(
						{ id: part1, json: p1RenamedJson, created: 5000, modified: 5200 },
						{ base: c1.rev }
					);

					// Save `part2` again WITHOUT renaming `part2`: its filename MUST stay
					// `part-40000002.waffle` rather than flipping to `part.waffle`.
					const s2 = await p.put(
						{ id: part2, json: p2Json1, created: 5100, modified: 5300 },
						{ base: c2.rev }
					);
					const loc2AfterSiblingRename = await p.getLocator(part2);
					const share2AfterSiblingRename = await p.getShareUrl(part2);

					// Renaming `part2` itself ('Part' -> 'Pin') DOES move it to `pin.waffle`.
					const r2 = await p.put(
						{ id: part2, json: p2RenamedJson, created: 5100, modified: 5400 },
						{ base: s2.rev }
					);
					const loc2AfterSelfRename = await p.getLocator(part2);

					return {
						c1,
						c2,
						r1,
						s2,
						r2,
						loc2Before,
						share2Before,
						loc2AfterSiblingRename,
						share2AfterSiblingRename,
						loc2AfterSelfRename
					};
				},
				{
					cfg,
					part1,
					part2,
					p1Json: v4Doc(part1, 'Part', 'mm'),
					p1RenamedJson: v4Doc(part1, 'Frame', 'mm'),
					p2Json0: v4Doc(part2, 'Part', 'mm'),
					p2Json1: v4Doc(part2, 'Part', 'in'),
					p2RenamedJson: v4Doc(part2, 'Pin', 'in')
				}
			);

			expect(stabilityOutcome.loc2Before.path).toBe('part-40000002.waffle');
			expect(stabilityOutcome.loc2AfterSiblingRename.path).toBe('part-40000002.waffle');
			expect(stabilityOutcome.share2AfterSiblingRename).toBe(stabilityOutcome.share2Before);
			expect(stabilityOutcome.loc2AfterSelfRename.path).toBe('pin.waffle');
			expect(files.has('part-40000002.waffle')).toBe(false);
			expect(files.has('pin.waffle')).toBe(true);

			expectNoAnyCrash(crashes);
		}
	);
});
