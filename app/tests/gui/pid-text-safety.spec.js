/**
 * **A persistent id must survive JavaScript.**
 *
 * A `.waffle` carries persistent entity ids (`Selector::Pid`, and everything
 * else `waffle_types::pid_str` governs). Since format v10 they are written as
 * decimal strings, which JavaScript handles exactly — but a STORED file may be
 * any earlier version, where they are JSON numbers, and a JSON number in
 * JavaScript is an `f64`: `JSON.parse` turns every id above `2^53` into a
 * different entity.
 *
 * Two app paths used to `JSON.parse` a whole stored document, mutate one
 * metadata field, and `JSON.stringify` it back:
 *
 *   - the home page's RENAME (`routes/home/+page.svelte`), which wrote the
 *     damaged text straight back to storage — permanent, silent, and it
 *     unanchors every drawing dimension and entity name in the document;
 *   - the examples panel's OPEN (`loadExample`), which handed the damaged text
 *     to the engine.
 *
 * Both now go through `editDocumentMeta`, which parses only the `document`
 * metadata object and splices it back into the original text. These tests pin
 * that: the id in the file is byte-identical afterwards, and the old route is
 * kept beside them as a NEGATIVE CONTROL so the test proves the hazard it
 * guards rather than asserting a tautology.
 */
import { test as rawTest, expect } from '@playwright/test';
import { seedDocument, getDocumentFromDB } from './helpers/waffle-test.js';
import { editDocumentMeta } from '../../src/lib/engine/format.js';
import { clickToolbarAction } from './helpers/toolbar.js';

/** The id D4a measured being rounded, and the worst case for an `f64`. */
const MEASURED = '2216071694111992607';
const TOP_BIT = '18446744073709551615';

/**
 * A stored document whose feature tree carries a name over a `Selector::Pid`.
 * `numeric: true` writes the ids as JSON NUMBERS — a pre-v10 file, which is
 * the dangerous case and the one a user's existing storage actually holds.
 */
function docWithPid({ id, name, numeric }) {
	const pid = numeric ? MEASURED : `"${MEASURED}"`;
	const root = numeric ? TOP_BIT : `"${TOP_BIT}"`;
	const json = `{
  "format": "waffle-iron",
  "version": ${numeric ? 8 : 10},
  "min_reader_version": ${numeric ? 8 : 10},
  "document": { "id": "${id}", "name": "${name}", "created": "2020-01-02T03:04:05.000Z", "modified": "2020-01-02T03:04:05.000Z" },
  "sources": [],
  "tabs": [
    { "id": "6f1c2a4e-1111-4222-8333-44445555aaaa", "name": "Part 1",
      "kind": { "type": "Part", "features": { "features": [], "active_index": null,
        "names": { "plate.top_face": {
          "target": { "kind": { "type": "Face" },
            "anchor": { "type": "FeatureOutput", "feature_id": "6f1c2a4e-1111-4222-8333-44445555bbbb", "output_key": { "type": "Main" } },
            "selector": { "type": "Pid", "pid": ${pid}, "root_pid": ${root} },
            "policy": { "type": "Strict" } },
          "kind": { "type": "Face" },
          "created": { "origin": { "type": "User" } } } } } } }
  ],
  "active_tab": "6f1c2a4e-1111-4222-8333-44445555aaaa"
}`;
	return { id, json, created: Date.parse('2020-01-02T03:04:05.000Z'), modified: Date.now() };
}

/** The `pid`/`root_pid` text exactly as it appears in a document's bytes. */
function pidTextIn(json) {
	const m = /"pid":\s*("?\d+"?),\s*"root_pid":\s*("?\d+"?)/.exec(json);
	return m ? { pid: m[1], root_pid: m[2] } : null;
}

// ───────────────────────── the helper, on its own ─────────────────────────

rawTest.describe('editDocumentMeta', () => {
	// Node-level: a pure function, tested without a browser. The two app call
	// sites are covered through the UI below.
	rawTest('renames a numeric-pid document without touching the pid', () => {
		const before = docWithPid({ id: 'num00001', name: 'Old Name', numeric: true }).json;
		expect(pidTextIn(before)).toEqual({ pid: MEASURED, root_pid: TOP_BIT });

		const after = editDocumentMeta(before, (meta) => {
			meta.name = 'New Name';
		});
		expect(JSON.parse(after).document.name).toBe('New Name');
		// Byte-identical ids — the whole point.
		expect(pidTextIn(after)).toEqual({ pid: MEASURED, root_pid: TOP_BIT });
		// And nothing outside `document` moved at all.
		const tabsOf = (t) => t.slice(t.indexOf('"sources"'));
		expect(tabsOf(after)).toBe(tabsOf(before));
	});

	// THE NEGATIVE CONTROL. Without this the test above could pass against a
	// helper that did nothing at all, and it is the measurement that justifies
	// the helper existing.
	rawTest('where parse → stringify would have changed the entity', () => {
		const before = docWithPid({ id: 'num00002', name: 'Old Name', numeric: true }).json;
		const parsed = JSON.parse(before);
		parsed.document.name = 'New Name';
		const damaged = JSON.stringify(parsed);

		const was = pidTextIn(before);
		const now = pidTextIn(damaged);
		expect(now).not.toEqual(was);
		// Specifically: a different id, not a rounder one.
		expect(now.pid).toBe('2216071694111992600');
		expect(BigInt(now.pid) === BigInt(was.pid)).toBe(false);
	});

	rawTest('a v10 string pid survives either route, which is why the file flipped', () => {
		const before = docWithPid({ id: 'str00001', name: 'Old Name', numeric: false }).json;
		const viaHelper = editDocumentMeta(before, (meta) => {
			meta.name = 'New';
		});
		const viaParse = JSON.stringify(
			(() => {
				const p = JSON.parse(before);
				p.document.name = 'New';
				return p;
			})()
		);
		expect(pidTextIn(viaHelper)).toEqual({ pid: `"${MEASURED}"`, root_pid: `"${TOP_BIT}"` });
		expect(pidTextIn(viaParse)).toEqual({ pid: `"${MEASURED}"`, root_pid: `"${TOP_BIT}"` });
	});

	rawTest('refuses a file with no metadata object rather than guessing', () => {
		expect(() => editDocumentMeta('{"format":"waffle-iron","tabs":[]}', () => {})).toThrow(
			/no `document` or `project` object/
		);
	});

	rawTest('is not fooled by the word "document" inside a string value', () => {
		// A regex-based edit would splice over whichever match came first.
		const text = `{"active_tab":"document","document":{"name":"a \\"document\\" of sorts"},"tabs":[]}`;
		const after = editDocumentMeta(text, (meta) => {
			meta.name = 'renamed';
		});
		const back = JSON.parse(after);
		expect(back.document.name).toBe('renamed');
		expect(back.active_tab).toBe('document');
	});
});

// ──────────────────────── the real path, in the app ────────────────────────

rawTest.describe('a stored pid survives the app', () => {
	rawTest('renaming from the home page leaves the pid byte-identical', async ({ page }) => {
		const doc = docWithPid({ id: 'pid00001', name: 'Old Name', numeric: true });
		await page.goto('/home');
		await seedDocument(page, doc);
		await page.goto('/home');
		await expect(page.locator('[data-testid="document-card"]')).toBeVisible({ timeout: 10000 });

		await page.locator('[data-testid="document-card"]').first().click({ button: 'right' });
		await page.locator('[data-testid="doc-ctx-rename"]').click();
		const input = page.locator('[data-testid="doc-rename-input"]');
		await expect(input).toBeVisible();
		await input.fill('Renamed Twice Over');
		await input.press('Enter');
		await expect(page.locator('.card-name')).toContainText('Renamed Twice Over');

		const stored = await getDocumentFromDB(page, 'pid00001');
		expect(stored).toBeTruthy();
		// The rename landed…
		expect(JSON.parse(stored.json).document.name).toBe('Renamed Twice Over');
		// …and the identity the document is built on did not move.
		expect(pidTextIn(stored.json)).toEqual({ pid: MEASURED, root_pid: TOP_BIT });
	});

	// The second of the two sites, end to end: the example's bytes → the JS
	// open path → the engine → back out in `ModelUpdated`'s feature tree. If
	// `loadExample` still round-tripped the document, the id the page gets
	// back here would be the rounded one.
	rawTest('opening an example delivers the pid the file holds', async ({ page }) => {
		const fixture = docWithPid({
			id: '6f1c2a4e-1111-4222-8333-44445555cccc',
			name: 'Shipped',
			numeric: true
		}).json;
		await page.route('**/examples/manifest.json', (route) =>
			route.fulfill({
				contentType: 'application/json',
				body: JSON.stringify({
					examples: [
						{
							id: 'pid-fixture',
							name: 'Pid Fixture',
							filename: 'pid-fixture.waffle',
							description: 'a numeric pid above 2^53'
						}
					]
				})
			})
		);
		await page.route('**/examples/pid-fixture.waffle', (route) =>
			route.fulfill({ contentType: 'application/json', body: fixture })
		);

		await page.goto('/');
		await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, {
			timeout: 30000
		});
		await clickToolbarAction(page, 'examples');
		await page.getByTestId('example-pid-fixture').click();

		// The name table arrives with the document.
		await page.waitForFunction(
			() => !!window.__waffle?.getFeatureTree()?.names?.['plate.top_face'],
			null,
			{ timeout: 30000 }
		);
		const names = JSON.parse(
			await page.evaluate(() => JSON.stringify(window.__waffle.getFeatureTree().names))
		);
		const selector = names['plate.top_face'].target.selector;
		// A STRING, and the id the file holds — not the `f64` of it.
		expect(selector.type).toBe('Pid');
		expect(selector.pid).toBe(MEASURED);
		expect(selector.root_pid).toBe(TOP_BIT);
	});

	rawTest('and so does a v10 string pid', async ({ page }) => {
		const doc = docWithPid({ id: 'pid00002', name: 'Old Name', numeric: false });
		await page.goto('/home');
		await seedDocument(page, doc);
		await page.goto('/home');
		await expect(page.locator('[data-testid="document-card"]')).toBeVisible({ timeout: 10000 });

		await page.locator('[data-testid="document-card"]').first().click({ button: 'right' });
		await page.locator('[data-testid="doc-ctx-rename"]').click();
		const input = page.locator('[data-testid="doc-rename-input"]');
		await expect(input).toBeVisible();
		await input.fill('String Pid Doc');
		await input.press('Enter');
		await expect(page.locator('.card-name')).toContainText('String Pid Doc');

		const stored = await getDocumentFromDB(page, 'pid00002');
		expect(pidTextIn(stored.json)).toEqual({ pid: `"${MEASURED}"`, root_pid: `"${TOP_BIT}"` });
	});
});
