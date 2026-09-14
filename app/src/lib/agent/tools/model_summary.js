/**
 * `model_summary` — the read-only overview of the open Part (specs/waffle_mcp_server.md §2.5).
 * Definition only (plain data); the implementation lives in `../summary.js`.
 */

const featureSchema = {
	type: 'object',
	properties: {
		id: { type: 'string', description: 'Feature id (lowercase hyphenated UUID).' },
		name: { type: 'string' },
		kind: {
			type: 'string',
			description: 'Operation variant, e.g. "Sketch", "Extrude", "Revolve", "BooleanCombine".'
		},
		suppressed: { type: 'boolean' },
		provenance: {
			type: 'object',
			description:
				'Who created the feature: {"type":"User"} | {"type":"Agent","name"} | {"type":"Import","source_id"} | {"type":"Derived","source_id","rule"}.',
			properties: { type: { type: 'string' } },
			required: ['type']
		},
		error: { type: 'string', description: 'Rebuild error message; absent when the feature built.' }
	},
	required: ['id', 'name', 'kind', 'suppressed', 'provenance']
};

export const modelSummaryTool = {
	name: 'model_summary',
	description:
		'Summarize the Part open in the paired Waffle Iron tab: features in tree order ' +
		'(id, name, kind, suppressed, provenance, rebuild error), the rollback index, bodies, ' +
		'rebuild errors and warnings, design parameters, and the part\'s named mate connectors. Parameter ' +
		'expressions and values are mm-space; connector origins are meters in part coordinates; nothing else ' +
		'carries lengths. Reads page state only; changes nothing.',
	inputSchema: { type: 'object', properties: {}, additionalProperties: false },
	outputSchema: {
		type: 'object',
		properties: {
			connectors: {
				type: 'array',
				description:
					'Named mate connectors (MateConnector features that built), in tree order: the frame an assembly ' +
					'mate uses on every instance of this part.',
				items: {
					type: 'object',
					properties: {
						feature_id: { type: 'string' },
						name: { type: 'string' },
						kind: {
							type: ['string', 'null'],
							description: 'What the frame was derived from ("planar face", "cylindrical face", …); null for an explicit frame.'
						},
						origin_m: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 },
						z_axis: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 },
						x_axis: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 }
					},
					required: ['feature_id', 'name', 'kind', 'origin_m', 'z_axis', 'x_axis']
				}
			},
			document_name: { type: 'string' },
			features: { type: 'array', items: featureSchema },
			rollback_index: {
				type: ['integer', 'null'],
				description:
					'Index of the last active feature; features after it are rolled back. null when every feature is active.'
			},
			bodies: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						body_id: { type: ['string', 'null'] },
						name: { type: 'string' },
						feature_id: { type: 'string' }
					},
					required: ['body_id', 'name', 'feature_id']
				}
			},
			errors: {
				type: 'array',
				description: 'Rebuild errors, in tree order, verbatim from the engine.',
				items: {
					type: 'object',
					properties: { feature_id: { type: 'string' }, message: { type: 'string' } },
					required: ['feature_id', 'message']
				}
			},
			warnings: {
				type: 'array',
				description: 'Non-fatal rebuild warnings, verbatim from the engine.',
				items: { type: 'string' }
			},
			parameters: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						id: { type: 'string' },
						name: { type: 'string' },
						expression: { type: 'string', description: 'mm-space expression.' },
						value_mm: { type: ['number', 'null'], description: 'Evaluated value, mm-space.' },
						error: { type: 'string' }
					},
					required: ['id', 'name', 'expression', 'value_mm']
				}
			}
		},
		required: ['document_name', 'features', 'rollback_index', 'bodies', 'errors', 'warnings', 'parameters', 'connectors']
	},
	annotations: { title: 'Model summary', readOnlyHint: true }
};
