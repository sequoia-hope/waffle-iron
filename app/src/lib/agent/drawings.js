/**
 * `export_svg` (`specs/drawings_and_mbd.md` §8, D4a) — the one export that
 * stays in the PAGE.
 *
 * §8 says both exports are wasm-bridge tools, and the DXF one is. The SVG one
 * cannot be, and the reason is §3's own rule: "Rust produces curves and
 * numbers; the app draws them." The sheet's SVG is produced by
 * `$lib/drawings/sheet.js` over `$lib/drawings/svg.js`, and a second renderer
 * in Rust would be a second source of truth for the same geometry — the exact
 * thing `DrawingView.svelte` refuses for a single view, where the comparison
 * is between a declarative component and the same `renderViewSvg` string.
 * Exporting from here means the file is BYTE-IDENTICAL to what the sheet
 * shows, which a Rust writer could only approximate.
 *
 * What it keeps from the engine is the data: the layouts come from
 * `ModelUpdated.drawing` — curves and already-measured numbers, with no
 * `GeomRef` and no kernel handle in them — so this module cannot invent a
 * dimension value either.
 *
 * It answers in the export door's own shape (`deliver`, `file_name`,
 * `mime_type`, `bytes`, `warnings`) and delivers a download the way the
 * executor delivers the engine's, so an agent cannot tell the two apart.
 */
import { getDocumentDisplayUnit, getDrawingSheet, getDrawingStatus, triggerStepDownload } from '$lib/engine/store.svelte.js';
import { renderSheetSvg } from '$lib/drawings/sheet.js';
import { fail, toolOk } from './results.js';

/** Q6: the largest file returned inline to the agent (16 MiB), as in Rust. */
const MAX_AGENT_PAYLOAD_BYTES = 16 * 1024 * 1024;

/** A file name from the sheet's own name, as `safe_name` does in Rust. */
function safeName(name) {
	const cleaned = (name ?? '')
		.replace(/[^A-Za-z0-9 ._-]+/g, '_')
		.trim()
		.slice(0, 80);
	return cleaned || 'Drawing';
}

export const DRAWING_QUERIES = {
	export_svg: {
		/**
		 * @param {{ sheet_id?: string, deliver?: string }} args
		 */
		run: (args = {}) => {
			const deliver = args.deliver ?? 'agent';
			if (deliver !== 'agent' && deliver !== 'download') {
				throw fail('InvalidArguments', 'deliver must be "agent" or "download".', { deliver });
			}
			const status = getDrawingStatus();
			if (!status) {
				throw fail(
					'TabKindNotSupported',
					'export_svg exports a drawing sheet; the active tab is not a Drawing tab ' +
						'(tab_add kind:"Drawing" or tab_switch).',
					{}
				);
			}
			const sheet = getDrawingSheet(args.sheet_id ?? null);
			if (!sheet) {
				// Named-but-absent is refused, not quietly answered with the
				// first sheet: a caller that asked for one sheet and got
				// another would export the wrong drawing under the right name.
				throw fail(
					'NotFound',
					args.sheet_id
						? `This drawing has no sheet ${args.sheet_id}.`
						: 'This drawing has no sheet to export.',
					{ sheet_id: args.sheet_id ?? null }
				);
			}
			const rendered = renderSheetSvg({
				sheet,
				unit: getDocumentDisplayUnit(),
				documentPrecision: 2
			});
			if (rendered.views === 0) {
				// An empty sheet is not a drawing. Refused rather than
				// delivered: a blank SVG is indistinguishable from a
				// successful export of a part with no edges.
				throw fail('NothingToExport', 'No view of this sheet produced a drawing.', {
					warnings: rendered.warnings
				});
			}
			const fileName = `${safeName(sheet.name)}.svg`;
			// UTF-8 length, the number the engine's exports report.
			const bytes = new TextEncoder().encode(rendered.svg).length;
			if (deliver === 'agent' && bytes > MAX_AGENT_PAYLOAD_BYTES) {
				throw fail(
					'PayloadTooLarge',
					`${fileName} is ${bytes} bytes, over the 16 MiB inline limit; use deliver "download".`,
					{ file_name: fileName, bytes }
				);
			}
			const structured = {
				deliver,
				file_name: fileName,
				mime_type: 'image/svg+xml',
				bytes,
				warnings: rendered.warnings
			};
			if (deliver === 'download') {
				triggerStepDownload(rendered.svg, fileName);
				return toolOk(structured);
			}
			const result = toolOk(structured);
			result.content.push({
				type: 'resource',
				resource: {
					uri: `waffle://export/${encodeURIComponent(fileName)}`,
					mimeType: 'image/svg+xml',
					text: rendered.svg
				}
			});
			return result;
		}
	}
};
