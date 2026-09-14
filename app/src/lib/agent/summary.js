/**
 * `model_summary` implementation as a pure function of store state
 * (specs/waffle_mcp_server.md §2.5, I14). No store import: the executor passes
 * the state in, so the same input always yields the same structured content.
 */

/**
 * @param {{
 *   documentName: string,
 *   featureTree: { features: Array<any>, active_index: number | null, provenance?: Record<string, any>, parameters?: Array<any> },
 *   featureErrors: Map<string, string>,
 *   bodies: Array<{ bodyId: string | null, featureId: string, name: string }>,
 *   warnings: Iterable<string>,
 *   connectors?: Array<{ feature_id: string, name: string, kind?: string, origin: number[], z_axis: number[], x_axis: number[] }>,
 * }} state
 */
export function summarizeModel({ documentName, featureTree, featureErrors, bodies, warnings, connectors = [] }) {
	const features = featureTree?.features ?? [];
	const provenance = featureTree?.provenance ?? {};
	const inTree = new Set();

	const summaryFeatures = features.map((f) => {
		inTree.add(f.id);
		/** @type {Record<string, unknown>} */
		const out = {
			id: f.id,
			name: f.name,
			kind: f.operation?.type ?? 'Unknown',
			suppressed: f.suppressed === true,
			// Only the origin: `Provenance.at` is a timestamp and would break I14 determinism.
			provenance: provenance[f.id]?.origin ?? { type: 'User' }
		};
		const error = featureErrors.get(f.id);
		if (error !== undefined) out.error = error;
		return out;
	});

	const errors = features
		.filter((f) => featureErrors.has(f.id))
		.map((f) => ({ feature_id: f.id, message: featureErrors.get(f.id) }));
	// Errors for ids the tree does not hold (should not happen) are still reported, never dropped.
	const orphans = [...featureErrors.keys()].filter((id) => !inTree.has(id)).sort();
	for (const id of orphans) errors.push({ feature_id: id, message: featureErrors.get(id) });

	return {
		document_name: documentName ?? '',
		features: summaryFeatures,
		rollback_index: featureTree?.active_index ?? null,
		bodies: bodies.map((b) => ({ body_id: b.bodyId ?? null, name: b.name, feature_id: b.featureId })),
		errors,
		warnings: [...warnings],
		parameters: (featureTree?.parameters ?? []).map((p) => {
			/** @type {Record<string, unknown>} */
			const row = {
				id: p.id,
				name: p.name,
				expression: p.expression,
				value_mm: typeof p.value === 'number' ? p.value : null
			};
			if (p.error) row.error = p.error;
			return row;
		}),
		// Named mate connectors as the engine evaluated them (part coordinates,
		// meters); a connector whose feature failed is in `errors`, not here.
		connectors: connectors.map((c) => ({
			feature_id: c.feature_id,
			name: c.name,
			kind: c.kind ?? null,
			origin_m: [...c.origin],
			z_axis: [...c.z_axis],
			x_axis: [...c.x_axis]
		}))
	};
}
