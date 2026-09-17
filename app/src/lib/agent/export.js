/**
 * The page's half of the export pair (specs/waffle_mcp_server.md §2.5 Export,
 * Q5–Q7; specs/waffle_server_mode.md §3.3): handing a `deliver:"download"`
 * file to the browser, exactly as the Export menu does.
 *
 * The semantics — the NothingToExport / BodyNotFound gates, the file name,
 * the byte count, the 16 MiB cap on an inline result and the result's shape —
 * run in the engine (`crates/wasm-bridge/src/tools/export.rs`, S3 C6). For a
 * download the engine's answer describes the file to the agent and carries
 * the file itself OUT OF BAND, in `download`, for this page to deliver; the
 * executor strips that field before the answer reaches the relay.
 */
import { triggerStepDownload, triggerStlDownload } from '$lib/engine/store.svelte.js';

/**
 * Deliver an exported file to the user: STEP is text, STL is base64 of the
 * binary file, mirroring an MCP embedded resource.
 * @param {{ file_name: string, mime_type: string, text?: string, blob?: string }} file
 */
export function deliverDownload(file) {
	if (typeof file.text === 'string') {
		triggerStepDownload(file.text, file.file_name);
	} else if (typeof file.blob === 'string') {
		// The store's helper appends `.stl` itself.
		triggerStlDownload(file.blob, file.file_name.replace(/\.stl$/, ''));
	} else {
		throw new Error(`The engine's download for ${file.file_name} carries neither text nor blob.`);
	}
}
