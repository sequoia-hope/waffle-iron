/**
 * Model snapshots and the `ModelDelta` of one agent step
 * (specs/waffle_mcp_server.md §2.5, A2/A3, I3). Pure: no store imports, so the
 * executor passes state in and the GUI suite can test these functions directly.
 */
import { canonicalJson } from './tools/manifest.js';

/**
 * @typedef {{
 *   tree: { features: Array<any>, active_index?: number | null, provenance?: Record<string, any>, parameters?: Array<any> },
 *   errors: Map<string, string>,
 *   bodyIds: string[],
 * }} ModelSnapshot
 */

/**
 * A plain, detached copy of the model state one agent step can change.
 * @param {{ featureTree: any, featureErrors: Map<string, string> | Iterable<[string, string]>, bodies: Array<{ bodyId: string | null }> }} state
 * @returns {ModelSnapshot}
 */
export function takeSnapshot({ featureTree, featureErrors, bodies }) {
	return {
		tree: JSON.parse(JSON.stringify(featureTree ?? { features: [], active_index: null })),
		errors: new Map(featureErrors),
		bodyIds: bodies.map((b) => b.bodyId).filter((id) => id != null)
	};
}

/**
 * Whether two snapshots hold the same document model (canonical feature-tree
 * JSON: features, rollback index, provenance, parameters, body names).
 * @param {ModelSnapshot} a
 * @param {ModelSnapshot} b
 */
export function sameModel(a, b) {
	return canonicalJson(a.tree) === canonicalJson(b.tree);
}

/**
 * Features whose rebuild error is new or changed between `before` and
 * `after`, in `after`'s tree order (errors for ids outside the tree last,
 * sorted). An error that merely persists is not "new".
 * @param {ModelSnapshot} before
 * @param {ModelSnapshot} after
 * @returns {Array<{ feature_id: string, message: string }>}
 */
export function newlyErroring(before, after) {
	const out = [];
	const order = after.tree.features.map((f) => f.id);
	const inTree = new Set(order);
	const ids = [...order, ...[...after.errors.keys()].filter((id) => !inTree.has(id)).sort()];
	for (const id of ids) {
		const message = after.errors.get(id);
		if (message !== undefined && before.errors.get(id) !== message) {
			out.push({ feature_id: id, message });
		}
	}
	return out;
}

/**
 * @param {ModelSnapshot} snap
 * @param {string} id
 */
function featureRecord(snap, id) {
	const feature = snap.tree.features.find((f) => f.id === id);
	return canonicalJson({ feature, origin: snap.tree.provenance?.[id]?.origin ?? { type: 'User' } });
}

/**
 * The spec's `ModelDelta` between two snapshots.
 *
 * `features_changed` lists features present in both whose definition or
 * provenance origin changed; a pure reorder does not change a record, so it
 * is reported by `order_changed`. `errors` lists every CURRENT rebuild error
 * (tree order), typed with `kind` when the engine's answer carried one.
 *
 * @param {ModelSnapshot} before
 * @param {ModelSnapshot} after
 * @param {{ typedErrors?: Array<{ feature_id: string, kind?: any, message: string }>, warnings?: Iterable<string> }} [extra]
 */
export function modelDelta(before, after, { typedErrors = [], warnings = [] } = {}) {
	const beforeIds = before.tree.features.map((f) => f.id);
	const afterIds = after.tree.features.map((f) => f.id);
	const beforeSet = new Set(beforeIds);
	const afterSet = new Set(afterIds);

	const common = afterIds.filter((id) => beforeSet.has(id));
	const commonBefore = beforeIds.filter((id) => afterSet.has(id));

	const kinds = new Map(typedErrors.map((e) => [e.feature_id, e.kind]));
	const inTree = new Set(afterIds);
	const errorIds = [...afterIds.filter((id) => after.errors.has(id)), ...[...after.errors.keys()].filter((id) => !inTree.has(id)).sort()];

	const beforeBodies = new Set(before.bodyIds);
	const afterBodies = new Set(after.bodyIds);

	return {
		features_added: afterIds.filter((id) => !beforeSet.has(id)),
		features_changed: common.filter((id) => featureRecord(before, id) !== featureRecord(after, id)),
		features_removed: beforeIds.filter((id) => !afterSet.has(id)),
		order_changed: common.some((id, i) => commonBefore[i] !== id),
		bodies_added: after.bodyIds.filter((id) => !beforeBodies.has(id)),
		bodies_removed: before.bodyIds.filter((id) => !afterBodies.has(id)),
		errors: errorIds.map((id) => {
			/** @type {{ feature_id: string, message: string | undefined, kind?: any }} */
			const row = { feature_id: id, message: after.errors.get(id) };
			if (kinds.has(id)) row.kind = kinds.get(id);
			return row;
		}),
		warnings: [...warnings]
	};
}
