/**
 * Agent query implementations that stay in the page (specs/waffle_mcp_server.md
 * §2.5 Inspection, §3.4; specs/waffle_server_mode.md §3.3).
 *
 * Queries change nothing. Only `selection_get` lives here now: it reads the
 * user's viewport selection, which is host state the engine deliberately does
 * not model. The other read-only tools — `model_summary`, `feature_get`,
 * `body_measure`, `face_list`, `sketch_regions`, `expression_evaluate` — run in
 * the engine (`crates/wasm-bridge/src/tools/{summary,inspect}.rs`, S3 C5b);
 * `executor.js` routes them there. `requireBody` and `ask` remain for the
 * export queries, which keep their `deliver:"download"` half in the page.
 */
import {
	computeFacePlane,
	geomRefEquals,
	getBodies,
	getMeshes,
	getSelectedFeatureId,
	getSelectedRefs
} from '$lib/engine/store.svelte.js';
import { isDatumPlaneRef } from '$lib/engine/planes.js';
import { fail, plain, toolOk } from './results.js';

/** @param {string} id */
export function requireBody(id) {
	if (!getBodies().some((b) => b.bodyId === id)) {
		throw fail('BodyNotFound', `No body with id ${id} in the open Part.`, { body_id: id });
	}
}

/**
 * The body whose rendered face or edge ranges carry `ref`, or null.
 * @param {any} ref
 */
function bodyForRef(ref) {
	for (const mesh of getMeshes()) {
		const ranges = [...(mesh.faceRanges ?? []), ...(mesh.edges?.ranges ?? [])];
		if (ranges.some((r) => r.geom_ref && geomRefEquals(r.geom_ref, ref))) return mesh.bodyId ?? null;
	}
	return null;
}

/**
 * @param {object} message
 * @param {string} expected - the EngineToUi `type` a successful answer has
 * @param {(m: object) => Promise<any>} send
 */
export async function ask(message, expected, send) {
	let response;
	try {
		response = await send(message);
	} catch (err) {
		throw fail('Internal', `${message.type} failed: ${err?.message ?? String(err)}`, {
			engine_error: { kind: err?.kind ?? null, message: String(err?.message ?? err) }
		});
	}
	if (response?.type !== expected) {
		throw fail('Internal', `${message.type} answered ${response?.type ?? 'nothing'}, expected ${expected}.`, {});
	}
	return response;
}

/**
 * @typedef {{ send: (message: object) => Promise<any> }} QueryEnv
 * @type {Record<string, { engine?: boolean, run: (args: any, env: QueryEnv) => any }>}
 */
export const QUERIES = {
	selection_get: {
		run: () => {
			const selection = getSelectedRefs().map((ref) => {
				const geomRef = plain(ref);
				const datum = isDatumPlaneRef(geomRef);
				/** @type {Record<string, unknown>} */
				const row = {
					geom_ref: geomRef,
					kind: datum ? 'DatumPlane' : (geomRef.kind?.type ?? 'Unknown'),
					body_id: datum ? null : bodyForRef(ref)
				};
				if (geomRef.selector?.type === 'Signature') row.signature = geomRef.selector.signature;
				// A datum plane's viewport ref is not a document GeomRef; its plane is what sketch_create takes.
				const plane = datum ? computeFacePlane(ref) : null;
				if (plane) row.plane = { origin: [...plane.origin], normal: [...plane.normal] };
				return row;
			});
			return toolOk({ selection, selected_feature_id: getSelectedFeatureId() ?? null });
		}
	}
};
