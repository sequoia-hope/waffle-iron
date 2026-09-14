/**
 * Agent link export and import (specs/waffle_mcp_server.md §2.5 Export, Q5–Q7, O21;
 * `import_step`): the REAL relay over MCP stdio, paired with the REAL page.
 */
import { test, expect } from '@playwright/test';
import { McpRelay, pairAgent, relayTestPort } from './helpers/mcp-relay.js';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

const AGENT = 'agent-export-import-test';
const P = (id, x, y) => ({ type: 'Point', id, x, y });
const L = (id, a, b) => ({ type: 'Line', id, start_id: a, end_id: b });
const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };
const RECT = [P(1, 0, 0), P(2, 0.02, 0), P(3, 0.02, 0.01), P(4, 0, 0.01), L(5, 1, 2), L(6, 2, 3), L(7, 3, 4), L(8, 4, 1)];
/** O1: the 20 × 10 × 5 mm box. */
const BOX_VOLUME_M3 = 0.02 * 0.01 * 0.005;

/** @param {any} result */
function ok(result) {
	expect(result.isError, JSON.stringify(result.structuredContent)).toBe(false);
	return result.structuredContent;
}

/** @param {any} result @param {string} code */
function refused(result, code) {
	expect(result.isError, JSON.stringify(result.structuredContent)).toBe(true);
	expect(result.structuredContent.error.code, JSON.stringify(result.structuredContent.error)).toBe(code);
	return result.structuredContent.error;
}

/** @param {any} result */
const embedded = (result) => result.content.find((c) => c.type === 'resource')?.resource;

test.describe('Agent link export and import', () => {
	/** @type {McpRelay} */
	let relay;

	test.beforeEach(async ({ baseURL }) => {
		const origin = new URL(/** @type {string} */ (baseURL)).origin;
		relay = new McpRelay({ port: await relayTestPort(), appUrl: `${origin}/`, allowOrigin: origin });
		await relay.waitListening();
		await relay.initialize(AGENT);
	});

	test.afterEach(async () => {
		expect(await relay.close()).toBe(0);
	});

	/** O1's box in a new document; returns its body id. @param {string} name */
	async function box(name) {
		ok(await relay.callTool('document_new', { name }));
		const sketch = ok(await relay.callTool('sketch_create', { plane: XY, entities: RECT }));
		ok(
			await relay.callTool('feature_add', {
				operation: {
					type: 'Extrude',
					params: { sketch_id: sketch.feature_id, profile_index: 0, profile_entity_ids: [5, 6, 7, 8], depth: 0.005, symmetric: false, cut: false }
				}
			})
		);
		const summary = ok(await relay.callTool('model_summary'));
		expect(summary.bodies).toHaveLength(1);
		return summary.bodies[0].body_id;
	}

	test('O21/Q7: export_step returns STEP text; import_step brings it back with the same volume', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		const bodyId = await box('Export');
		const v0 = ok(await relay.callTool('body_measure', { body_id: bodyId })).volume_m3;
		expect(Math.abs(v0 - BOX_VOLUME_M3) / BOX_VOLUME_M3).toBeLessThan(1e-9);

		const exported = await relay.callTool('export_step');
		const meta = ok(exported);
		expect(meta).toMatchObject({ deliver: 'agent', mime_type: 'model/step', warnings: [] });
		expect(meta.file_name).toMatch(/\.step$/);
		const step = embedded(exported);
		expect(step.mimeType).toBe('model/step');
		expect(step.text.startsWith('ISO-10303-21;')).toBe(true);
		expect(meta.bytes).toBe(Buffer.byteLength(step.text, 'utf8'));

		// S3: leave the exported document only once it is stored.
		ok(await relay.callTool('document_save'));
		ok(await relay.callTool('document_new', { name: 'Reimport' }));
		const imported = ok(await relay.callTool('import_step', { file_name: 'box.step', step_text: step.text }));
		expect(imported.features_added).toEqual([imported.feature_id]);
		const summary = ok(await relay.callTool('model_summary'));
		expect(summary.features.map((f) => [f.kind, f.provenance?.type])).toEqual([['ImportedBody', 'Import']]);
		expect(summary.bodies).toHaveLength(1);
		const v1 = ok(await relay.callTool('body_measure', { body_id: summary.bodies[0].body_id })).volume_m3;
		// O21: within 1e-9 m³ of O1. STEP writes coordinates as decimal reals, so
		// the round trip is not bit-exact: measured 6.7e-8 relative on this box
		// (2026-09-14) — bounded here at 1e-6 relative.
		expect(Math.abs(v1 - v0)).toBeLessThanOrEqual(1e-9);
		expect(Math.abs(v1 - v0) / v0).toBeLessThan(1e-6);
		// No placement dialog: the agent's page is not left busy.
		expect(await page.evaluate(() => window.__waffle.getImportDialogState?.() ?? null)).toBeFalsy();

		// One undo step.
		ok(await relay.callTool('undo'));
		expect(ok(await relay.callTool('model_summary')).bodies).toEqual([]);
		expectNoAnyCrash(crashes);
	});

	test('Q5/A14: exports refuse an empty Part and an unknown body; export_stl is a binary STL', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		ok(await relay.callTool('document_new', { name: 'Empty' }));
		refused(await relay.callTool('export_step'), 'NothingToExport');
		refused(await relay.callTool('export_stl'), 'NothingToExport');

		const bodyId = await box('Stl');
		refused(await relay.callTool('export_stl', { body_id: 'no-such-body' }), 'BodyNotFound');
		for (const args of [{ body_id: bodyId }, {}]) {
			const result = await relay.callTool('export_stl', args);
			const meta = ok(result);
			expect(meta).toMatchObject({ deliver: 'agent', mime_type: 'model/stl', warnings: [] });
			const stl = embedded(result);
			expect(stl.mimeType).toBe('model/stl');
			const bytes = Buffer.from(stl.blob, 'base64');
			// Binary STL: 80-byte header, u32 triangle count, 50 bytes per triangle. A box is 12 triangles.
			const triangles = bytes.readUInt32LE(80);
			expect(triangles).toBe(12);
			expect(bytes.length).toBe(84 + 50 * triangles);
			expect(meta.bytes).toBe(bytes.length);
		}
		expectNoAnyCrash(crashes);
	});

	test('deliver "download" hands the file to the browser and embeds nothing', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		await box('Download');
		for (const [tool, extension] of [['export_step', '.step'], ['export_stl', '.stl']]) {
			const [download, result] = await Promise.all([
				page.waitForEvent('download'),
				relay.callTool(tool, { deliver: 'download' })
			]);
			expect(download.suggestedFilename().endsWith(extension)).toBe(true);
			expect(ok(result).deliver).toBe('download');
			expect(embedded(result)).toBeUndefined();
		}
		expectNoAnyCrash(crashes);
	});

	test('import_step: text that is not STEP is refused and changes nothing', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await pairAgent(page, relay, AGENT);
		ok(await relay.callTool('document_new', { name: 'Junk' }));
		const error = refused(
			await relay.callTool('import_step', { file_name: 'junk.step', step_text: 'this is not a STEP file' }),
			'FeatureRebuildFailed'
		);
		expect(error.details.rolled_back).toBe(true);
		expect(error.details.engine_error.message).toContain('STEP parse failed');
		const summary = ok(await relay.callTool('model_summary'));
		expect(summary.features).toEqual([]);
		expect(summary.errors).toEqual([]);
		expectNoAnyCrash(crashes);
	});
});
