/**
 * Agent query implementations that stay in the page (specs/waffle_mcp_server.md
 * §2.5 Inspection, §3.4; specs/waffle_server_mode.md §3.3).
 *
 * Queries change nothing. Only `selection_get` lives here now: it reads the
 * user's viewport selection, which is host state the engine deliberately does
 * not model. The other read-only tools — `model_summary`, `feature_get`,
 * `body_measure`, `face_list`, `sketch_regions`, `expression_evaluate`, and
 * the export pair — run in the engine
 * (`crates/wasm-bridge/src/tools/{summary,inspect,export}.rs`, S3 C5b/C6);
 * `executor.js` routes them there.
 */
import {
	computeFacePlane,
	geomRefEquals,
	getMeshes,
	getSelectedFeatureId,
	getSelectedRefs
} from '$lib/engine/store.svelte.js';
import { isDatumPlaneRef } from '$lib/engine/planes.js';
import { plain, toolOk } from './results.js';

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
