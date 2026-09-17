/**
 * Read-only agent tools (specs/waffle_mcp_server.md §2.5 Inspection). Definitions
 * only. `selection_get` is implemented in `../queries.js` (viewport state stays
 * with the host); the rest run in the engine
 * (`crates/wasm-bridge/src/tools/inspect.rs`, S3), routed by `../executor.js`.
 */
import { noArguments, uuid } from './common.js';
import { defsFor, engineRef } from './engineSchemas.js';

const readOnly = (title) => ({ title, readOnlyHint: true });

export const featureGetTool = {
	name: 'feature_get',
	description:
		'One feature of the open Part: its full Operation JSON (the same shape feature_edit takes), name, ' +
		'suppression, provenance and rebuild error. Lengths in meters, angles in degrees.',
	inputSchema: {
		type: 'object',
		properties: { feature_id: uuid('Feature id from model_summary.') },
		required: ['feature_id'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			feature_id: { type: 'string' },
			name: { type: 'string' },
			suppressed: { type: 'boolean' },
			operation: { type: 'object' },
			provenance: { type: 'object' },
			error: { type: 'string' }
		},
		required: ['feature_id', 'name', 'suppressed', 'operation', 'provenance']
	},
	annotations: readOnly('Get feature')
};

export const selectionGetTool = {
	name: 'selection_get',
	description:
		"The user's current viewport selection: each picked face, edge or vertex as a GeomRef (a face ref is " +
		'usable as sketch_create plane), its kind and the body it belongs to; a selected datum plane has kind ' +
		'"DatumPlane" and a plane {origin, normal} to pass as sketch_create plane. Also the feature selected in ' +
		'the tree. An empty selection is not an error.',
	inputSchema: noArguments,
	outputSchema: {
		type: 'object',
		properties: {
			selection: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						geom_ref: { type: 'object' },
						kind: { type: 'string', description: 'Face | Edge | Vertex …' },
						body_id: { type: ['string', 'null'] },
						signature: { type: 'object' }
					},
					required: ['geom_ref', 'kind', 'body_id']
				}
			},
			selected_feature_id: { type: ['string', 'null'] }
		},
		required: ['selection', 'selected_feature_id']
	},
	annotations: readOnly('Get selection')
};

const measuredValue = { type: 'number' };

export const bodyMeasureTool = {
	name: 'body_measure',
	description:
		'Volume (m³), surface area (m²), bounding box (m), topology counts and closedness of one body. ' +
		'method is "exact" when both quantities were integrated from the B-Rep, else "mesh" (render-mesh ' +
		'values, low on curved faces); methods and exact_unavailable say which quantity fell back and why. ' +
		'The bounding box always comes from the render mesh.',
	inputSchema: {
		type: 'object',
		properties: { body_id: { type: 'string', description: 'Body id from model_summary.bodies.' } },
		required: ['body_id'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string' },
			volume_m3: measuredValue,
			surface_area_m2: measuredValue,
			method: { type: 'string', enum: ['exact', 'mesh'] },
			methods: {
				type: 'object',
				properties: { volume: { type: 'string' }, surface_area: { type: 'string' } }
			},
			exact_unavailable: { type: 'object' },
			bbox_min: { type: 'array', items: { type: 'number' } },
			bbox_max: { type: 'array', items: { type: 'number' } },
			face_count: { type: 'integer' },
			edge_count: { type: 'integer' },
			vertex_count: { type: 'integer' },
			closed: { type: 'boolean' }
		},
		required: [
			'body_id',
			'volume_m3',
			'surface_area_m2',
			'method',
			'bbox_min',
			'bbox_max',
			'face_count',
			'edge_count',
			'vertex_count',
			'closed'
		]
	},
	annotations: readOnly('Measure body')
};

export const faceListTool = {
	name: 'face_list',
	description:
		'Every face of a body as the GeomRef the viewport hands out when the user picks it (usable as a ' +
		'sketch_create plane), with its topological signature, in a deterministic order. filter narrows ' +
		'the list with TopoQuery filters (tie_break is ignored).',
	inputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string', description: 'Body id from model_summary.bodies.' },
			filter: engineRef('TopoQuery')
		},
		required: ['body_id'],
		additionalProperties: false,
		$defs: defsFor('TopoQuery')
	},
	outputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string' },
			faces: {
				type: 'array',
				items: {
					type: 'object',
					properties: { geom_ref: { type: 'object' }, signature: { type: 'object' } },
					required: ['geom_ref', 'signature']
				}
			}
		},
		required: ['body_id', 'faces']
	},
	annotations: readOnly('List faces')
};

export const sketchRegionsTool = {
	name: 'sketch_regions',
	description:
		'The closed regions of a completed sketch. A region equal to one whole loop carries ' +
		'profile_entity_ids — pass that list as ExtrudeParams/RevolveParams.profile_entity_ids to extrude ' +
		'exactly that loop. Sub-regions of overlapping shapes have profile_entity_ids null. area_m2 in m².',
	inputSchema: {
		type: 'object',
		properties: { feature_id: uuid('Id of a Sketch feature.') },
		required: ['feature_id'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			feature_id: { type: 'string' },
			regions: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						profile_entity_ids: { type: ['array', 'null'], items: { type: 'integer' } },
						area_m2: { type: 'number' }
					},
					required: ['profile_entity_ids', 'area_m2']
				}
			}
		},
		required: ['feature_id', 'regions']
	},
	annotations: readOnly('Sketch regions')
};

export const expressionEvaluateTool = {
	name: 'expression_evaluate',
	description:
		'Evaluate an expression against the design parameters, as a dimension field would. mm-space: a bare ' +
		'number means millimeters for lengths (degrees for angles); unit suffixes (in, cm, …) and parameter ' +
		'names are allowed. Returns value_mm, or value_mm null with the evaluation error.',
	inputSchema: {
		type: 'object',
		properties: { expression: { type: 'string', description: 'e.g. "width / 2" or "1.5in".' } },
		required: ['expression'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			expression: { type: 'string' },
			value_mm: { type: ['number', 'null'] },
			error: { type: 'string' }
		},
		required: ['expression', 'value_mm']
	},
	annotations: readOnly('Evaluate expression')
};
