import { test, expect } from '@playwright/test';

/**
 * The sketch corpus is served READ-ONLY (S4 of
 * `specs/agent_mechanical_design.md` §10.4).
 *
 * This spec exists because of a real loss. §10.4 said to list the corpus in
 * "the empty second manifest slot", `app/tests/cases/manifest.json`, and the
 * first version of the generator did — which put thirteen committed `.waffle`
 * files behind `/api/test-cases`, the CRUD endpoint the Tests browser panel
 * owns. `test-case-browser.spec.js` exercises that endpoint's DELETE as part
 * of its own coverage, so one GUI run unlinked every case file and emptied the
 * manifest (measured 2026-10-08, recovered from git).
 *
 * So the corpus has `/api/sketch-cases`, GET-only, beside `/api/assay-cases`.
 * The assertions below are the two properties that matter: the corpus is
 * reachable, and nothing reachable can delete it.
 */
test.describe('the sketch corpus API is read-only', () => {
	test('the corpus lists every committed case with its authored verdict', async ({ request }) => {
		const res = await request.get('/api/sketch-cases');
		expect(res.ok()).toBe(true);
		const body = await res.json();

		expect(body.count).toBe(body.cases.length);
		// Thirteen at S4, and never zero: an empty listing is what the deletion
		// looked like, so it must fail rather than pass vacuously.
		expect(body.count).toBeGreaterThanOrEqual(13);

		const ids = body.cases.map((c) => c.id);
		expect(ids).toEqual([...ids].sort());
		for (const id of ['S0001', 'S0003', 'S0006', 'S0010']) {
			expect(ids).toContain(id);
		}
		// Each entry carries enough to populate a picker without fetching the
		// case: what it is, what it exercises, and what it should solve to.
		const first = body.cases.find((c) => c.id === 'S0003');
		expect(first.description).toContain('fully constrained');
		expect(first.status).toBe('FullyConstrained');
		expect(first.dof).toBe(0);
		expect(first.exercises).toContain('fully-constrained');
	});

	test('a case serves as a loadable document, and its metadata beside it', async ({ request }) => {
		const doc = await request.get('/api/sketch-cases/S0003');
		expect(doc.ok()).toBe(true);
		const waffle = await doc.json();
		expect(waffle.format).toBe('waffle-iron');
		const features = waffle.tabs[0].kind.features.features;
		expect(features).toHaveLength(1);
		expect(features[0].operation.type).toBe('Sketch');
		// Eight entities: four corners and four lines.
		expect(features[0].operation.sketch.entities).toHaveLength(8);

		const meta = await request.get('/api/sketch-cases/S0003/meta');
		expect(meta.ok()).toBe(true);
		const expectations = (await meta.json()).expectations;
		expect(expectations.dof).toBe(0);
		expect(expectations.params).toBe(8);
		expect(expectations.regions).toHaveLength(1);
	});

	test('every write verb is refused, and the case survives it', async ({ request }) => {
		for (const attempt of [
			() => request.delete('/api/sketch-cases/S0003'),
			() => request.post('/api/sketch-cases', { data: { name: 'x' } }),
			() => request.patch('/api/sketch-cases/S0003', { data: { name: 'x' } })
		]) {
			const res = await attempt();
			expect(res.status()).toBe(405);
			expect((await res.json()).error).toContain('read-only');
		}
		// The file is still there, which is the property the 405s exist for.
		const after = await request.get('/api/sketch-cases/S0003');
		expect(after.ok()).toBe(true);
	});

	test('a case id cannot walk out of the corpus directory', async ({ request }) => {
		for (const id of ['..%2F..%2Fmanifest', 'S0003%2F..%2F..%2Fmanifest']) {
			const res = await request.get(`/api/sketch-cases/${id}`);
			expect(res.ok()).toBe(false);
		}
	});

	test('the generic test-case manifest is still the empty slot it was', async ({ request }) => {
		// The corpus must not be in here: that is the endpoint whose DELETE
		// removed it. If a future change lists it here again, this fails and
		// says why.
		const res = await request.get('/api/test-cases');
		expect(res.ok()).toBe(true);
		const ids = (await res.json()).cases.map((c) => c.id);
		expect(ids.filter((id) => /^S\d{4}$/.test(id))).toEqual([]);
	});
});
