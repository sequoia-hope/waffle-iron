/**
 * Export tool implementations (specs/waffle_mcp_server.md §2.5 Export, Q5–Q7).
 * Queries: they change nothing, but they send a bridge message, so they run
 * under the engine lock. "download" hands the file to the browser exactly as the
 * Export menu does.
 */
import { getBodies, getProjectName, triggerStepDownload, triggerStlDownload } from '$lib/engine/store.svelte.js';
import { ask, requireBody } from './queries.js';
import { fail } from './results.js';

/** Q6: the largest file returned inline to the agent. */
export const MAX_AGENT_PAYLOAD_BYTES = 16 * 1024 * 1024;

/** @param {string | undefined} name */
const safeName = (name) => (name || 'model').replace(/[^\w.-]+/g, '_');

/** Q5 */
function requireBodies() {
	if (getBodies().length === 0) throw fail('NothingToExport', 'The open Part has no bodies to export.', {});
}

/** Decoded size of standard padded base64. @param {string} b64 */
function base64Bytes(b64) {
	const pad = b64.endsWith('==') ? 2 : b64.endsWith('=') ? 1 : 0;
	return Math.floor((b64.length * 3) / 4) - pad;
}

/**
 * @param {{ deliver: string, fileName: string, mimeType: string, bytes: number, warnings: string[],
 *   resource: { text: string } | { blob: string } }} file
 */
function exportResult({ deliver, fileName, mimeType, bytes, warnings, resource }) {
	const structured = { deliver, file_name: fileName, mime_type: mimeType, bytes, warnings };
	const content = [{ type: 'text', text: JSON.stringify(structured) }];
	if (deliver === 'agent') {
		content.push({ type: 'resource', resource: { uri: `waffle://export/${encodeURIComponent(fileName)}`, mimeType, ...resource } });
	}
	return { content, structuredContent: structured, isError: false };
}

/** Q6 @param {string} deliver @param {string} fileName @param {number} bytes */
function checkPayload(deliver, fileName, bytes) {
	if (deliver !== 'agent' || bytes <= MAX_AGENT_PAYLOAD_BYTES) return;
	throw fail(
		'PayloadTooLarge',
		`${fileName} is ${bytes} bytes, over the ${MAX_AGENT_PAYLOAD_BYTES}-byte limit for a result. Use deliver: "download".`,
		{ bytes }
	);
}

/** @type {Record<string, { engine: boolean, run: (args: any, env: { send: (m: object) => Promise<any> }) => Promise<object> }>} */
export const EXPORT_QUERIES = {
	export_step: {
		engine: true,
		run: async ({ deliver = 'agent' }, { send }) => {
			requireBodies();
			const r = await ask({ type: 'ExportStep' }, 'ExportReady', send);
			const fileName = `${safeName(getProjectName())}.step`;
			const bytes = new TextEncoder().encode(r.step_data).length;
			checkPayload(deliver, fileName, bytes);
			if (deliver === 'download') triggerStepDownload(r.step_data, fileName);
			return exportResult({
				deliver,
				fileName,
				mimeType: 'model/step',
				bytes,
				warnings: r.warnings ?? [],
				resource: { text: r.step_data }
			});
		}
	},

	export_stl: {
		engine: true,
		run: async ({ body_id, deliver = 'agent' }, { send }) => {
			requireBodies();
			if (body_id != null) requireBody(body_id);
			const message = body_id != null ? { type: 'ExportBodyStl', body_id } : { type: 'ExportStl' };
			const r = await ask(message, 'StlExportReady', send);
			const base = safeName(body_id != null ? getBodies().find((b) => b.bodyId === body_id)?.name : getProjectName());
			const fileName = `${base}.stl`;
			const bytes = base64Bytes(r.stl_data);
			checkPayload(deliver, fileName, bytes);
			if (deliver === 'download') triggerStlDownload(r.stl_data, base);
			return exportResult({ deliver, fileName, mimeType: 'model/stl', bytes, warnings: [], resource: { blob: r.stl_data } });
		}
	}
};
