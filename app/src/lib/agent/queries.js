/**
 * Agent query implementations (specs/waffle_mcp_server.md §2.5 Inspection, §3.4).
 *
 * Queries change nothing. Those marked `engine` send a bridge message and run
 * under the engine lock (I6); the rest read store state only.
 */
import {
	computeFacePlane,
	geomRefEquals,
	getBodies,
	getDocumentName,
	getFeatureErrors,
	getFeatureTree,
	getMeshes,
	getRebuildWarnings,
	getSelectedFeatureId,
	getSelectedRefs,
	sketchRegionsRequest
} from '$lib/engine/store.svelte.js';
import { isDatumPlaneRef } from '$lib/engine/planes.js';
import { fail, plain, toolOk } from './results.js';
import { summarizeModel } from './summary.js';

/** @param {string} id */
export function requireFeature(id) {
	const feature = getFeatureTree()?.features?.find((f) => f.id === id);
	if (!feature) throw fail('FeatureNotFound', `No feature with id ${id} in the open Part.`, { feature_id: id });
	return feature;
}

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
	model_summary: {
		run: () =>
			toolOk(
				summarizeModel({
					documentName: getDocumentName(),
					featureTree: plain(getFeatureTree()),
					featureErrors: new Map(getFeatureErrors()),
					bodies: getBodies(),
					warnings: getRebuildWarnings()
				})
			)
	},

	feature_get: {
		run: ({ feature_id }) => {
			const feature = plain(requireFeature(feature_id));
			const tree = getFeatureTree();
			/** @type {Record<string, unknown>} */
			const out = {
				feature_id: feature.id,
				name: feature.name,
				suppressed: feature.suppressed === true,
				operation: feature.operation,
				provenance: plain(tree?.provenance?.[feature.id]?.origin) ?? { type: 'User' }
			};
			const error = getFeatureErrors().get(feature.id);
			if (error !== undefined) out.error = error;
			return toolOk(out);
		}
	},

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
	},

	body_measure: {
		engine: true,
		run: async ({ body_id }, { send }) => {
			requireBody(body_id);
			const r = await ask({ type: 'MeasureBody', body_id }, 'BodyMeasured', send);
			const exact = r.volume_m3.method === 'exact' && r.surface_area_m2.method === 'exact';
			/** @type {Record<string, unknown>} */
			const out = {
				body_id: r.body_id,
				volume_m3: r.volume_m3.value,
				surface_area_m2: r.surface_area_m2.value,
				method: exact ? 'exact' : 'mesh',
				methods: { volume: r.volume_m3.method, surface_area: r.surface_area_m2.method },
				bbox_min: r.bbox_min,
				bbox_max: r.bbox_max,
				face_count: r.face_count,
				edge_count: r.edge_count,
				vertex_count: r.vertex_count,
				closed: r.closed
			};
			const unavailable = {};
			if (r.volume_m3.exact_unavailable) unavailable.volume = r.volume_m3.exact_unavailable;
			if (r.surface_area_m2.exact_unavailable) unavailable.surface_area = r.surface_area_m2.exact_unavailable;
			if (Object.keys(unavailable).length > 0) out.exact_unavailable = unavailable;
			return toolOk(out);
		}
	},

	face_list: {
		engine: true,
		run: async ({ body_id, filter }, { send }) => {
			requireBody(body_id);
			const message = { type: 'ListFaces', body_id, ...(filter ? { filter: plain(filter) } : {}) };
			const r = await ask(message, 'FacesListed', send);
			return toolOk({ body_id: r.body_id, faces: r.faces });
		}
	},

	sketch_regions: {
		engine: true,
		run: async ({ feature_id }, { send }) => {
			const feature = plain(requireFeature(feature_id));
			const kind = feature.operation?.type;
			if (kind !== 'Sketch') {
				throw fail('OperationKindMismatch', `Feature ${feature_id} is a ${kind}, not a Sketch.`, {
					expected: 'Sketch',
					got: kind ?? null
				});
			}
			const request = await sketchRegionsRequest(feature, send);
			const r = await ask(request, 'RegionsComputed', send);
			return toolOk({
				feature_id,
				regions: (r.regions ?? []).map((region) => ({
					profile_entity_ids: region.profile_entity_ids ?? null,
					area_m2: region.area
				}))
			});
		}
	},

	expression_evaluate: {
		engine: true,
		run: async ({ expression }, { send }) => {
			const r = await ask({ type: 'EvaluateExpression', expression }, 'ExpressionEvaluated', send);
			/** @type {Record<string, unknown>} */
			const out = { expression, value_mm: r.value ?? null };
			if (r.error) out.error = r.error;
			return toolOk(out);
		}
	}
};
