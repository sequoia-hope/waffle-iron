/**
 * Assembly agent tools (specs/waffle_mcp_server.md §2.5 Assemblies): the open
 * Assembly tab's instances, mate connectors and mates, authored through the
 * same store flows the Assembly panel uses (`EditAssembly`). Definitions only;
 * implementations are in `../assembly.js`.
 */
import { defsFor, engineRef } from './engineSchemas.js';
import { noArguments, UNITS_NOTE, uuid } from './common.js';

const readOnly = (title) => ({ title, readOnlyHint: true });
const edits = (title) => ({ title, readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false });
const deletes = (title) => ({ title, readOnlyHint: false, destructiveHint: true, idempotentHint: true, openWorldHint: false });

const vec3 = (description) => ({
	type: 'array',
	items: { type: 'number' },
	minItems: 3,
	maxItems: 3,
	description
});

const NOT_AN_UNDO_STEP =
	'An assembly edit is not an undo step (undo/redo act on the feature tree of a Part tab); reverse it with ' +
	'the matching delete or edit tool.';

const REQUIRES_ASSEMBLY_TAB =
	'Requires an Assembly tab to be active (tab_add kind:"Assembly", or tab_switch to one); on a Part tab it is ' +
	'refused with TabKindNotSupported.';

/**
 * An instance placement the agent can write: the document's Transform
 * (translation in meters, unit quaternion [x, y, z, w]) or, instead of the
 * quaternion, Euler angles in degrees (three.js intrinsic XYZ — what the
 * Assembly panel shows).
 */
const transformInput = {
	type: 'object',
	description:
		'Placement of the instance in the assembly: translation_m (meters) and either rotation_quat [x, y, z, w] ' +
		'(unit quaternion, the document form) or rotation_euler_deg [x, y, z] (degrees, intrinsic XYZ as the ' +
		'Assembly panel shows). Omitted fields keep their current value (identity for a new instance).',
	properties: {
		translation_m: vec3('Meters.'),
		rotation_quat: { type: 'array', items: { type: 'number' }, minItems: 4, maxItems: 4 },
		rotation_euler_deg: vec3('Degrees, intrinsic XYZ order.')
	},
	additionalProperties: false
};

/** A world frame as the evaluation reports it (an orthonormal basis at a point). */
const frameSchema = {
	type: 'object',
	properties: {
		origin: vec3('Meters.'),
		x_axis: vec3(),
		y_axis: vec3(),
		z_axis: vec3()
	},
	required: ['origin', 'x_axis', 'y_axis', 'z_axis']
};

// The output shapes below spell out the document's Transform and Frame
// instead of `$ref`ing the engine definitions: every assembly tool returns
// the state, and the engine `$defs` (long doc comments) would ride along ten
// times in `tools/list`. Inputs still refer to the engine definitions.
const transformOutput = {
	type: 'object',
	description: 'A rigid transform: translation_m (meters) then rotation_quat [x, y, z, w] (unit quaternion).',
	properties: { translation_m: vec3(), rotation_quat: { type: 'array', items: { type: 'number' }, minItems: 4, maxItems: 4 } },
	required: ['translation_m', 'rotation_quat']
};

const partFrameOutput = {
	type: 'object',
	description: 'A frame in the PART\'s coordinates: origin, primary (z) axis, secondary (x) axis; zero x_axis = any perpendicular.',
	properties: { origin: vec3('Meters.'), z_axis: vec3(), x_axis: vec3() },
	required: ['origin', 'z_axis', 'x_axis']
};

const mateKindEnum = { type: 'string', enum: ['Fastened', 'Revolute', 'Slider', 'Cylindrical', 'Planar', 'Ball'] };

const MATE_KIND_NOTE =
	'kind: Fastened removes all six degrees of freedom (solved exactly); Revolute frees rotation about z; Slider ' +
	'frees translation along z; Cylindrical frees both; Planar frees xy translation and rotation about z; Ball ' +
	'frees all rotation. flip (default true) makes the two z axes OPPOSE, which is what two outward face normals ' +
	'placed against each other need; false aligns them. rotation_deg (Fastened only) turns b about z after alignment.';

/** The open assembly as every assembly tool returns it. */
export const assemblyStateSchema = {
	type: 'object',
	properties: {
		tab_id: { type: 'string' },
		name: { type: 'string', description: 'The Assembly tab\'s name.' },
		instances: {
			type: 'array',
			description: 'In assembly order.',
			items: {
				type: 'object',
				properties: {
					id: { type: 'string' },
					name: { type: 'string' },
					source: {
						type: 'object',
						description: 'The part: a tab of this document (source_id null) or of a linked .waffle source.',
						properties: { tab_id: { type: 'string' }, source_id: { type: ['string', 'null'] } },
						required: ['tab_id', 'source_id']
					},
					part_name: { type: 'string', description: 'The source tab\'s name, for reading.' },
					transform: transformOutput,
					fixed: { type: 'boolean', description: 'Grounded: never moved by mates.' },
					suppressed: { type: 'boolean' },
					placement: {
						anyOf: [transformOutput, { type: 'null' }],
						description: 'The solved world placement (mates applied); null while suppressed or not yet evaluated.'
					}
				},
				required: ['id', 'name', 'source', 'part_name', 'transform', 'fixed', 'suppressed', 'placement']
			}
		},
		connectors: {
			type: 'array',
			items: {
				type: 'object',
				properties: {
					id: { type: 'string' },
					name: { type: 'string' },
					instance_path: { type: 'array', items: { type: 'string' } },
					part_connector: { type: ['string', 'null'], description: 'The part\'s MateConnector feature id, when made from one.' },
					geom_ref: { type: ['object', 'null'], description: 'The face or edge the frame is derived from, in the part\'s space.' },
					frame: partFrameOutput,
					anchor: { type: 'string', enum: ['middle', 'positive_end', 'negative_end'] },
					flip_z: { type: 'boolean' },
					rotation_deg: { type: 'number' },
					offset_m: vec3(),
					world_frame: {
						anyOf: [{ ...frameSchema, properties: { ...frameSchema.properties, kind: { type: ['string', 'null'] } } }, { type: 'null' }],
						description: 'The evaluated frame in world coordinates and what it was derived from; null when it failed (see errors).'
					}
				},
				required: ['id', 'name', 'instance_path', 'part_connector', 'geom_ref', 'frame', 'anchor', 'flip_z', 'rotation_deg', 'offset_m', 'world_frame']
			}
		},
		mates: {
			type: 'array',
			items: {
				type: 'object',
				properties: {
					id: { type: 'string' },
					name: { type: 'string' },
					kind: { type: 'object', description: 'The document MateKind: {type, flip?, rotation_deg?}.' },
					connectors: { type: 'array', items: { type: 'string' }, minItems: 2, maxItems: 2 },
					suppressed: { type: 'boolean' }
				},
				required: ['id', 'name', 'kind', 'connectors', 'suppressed']
			}
		},
		part_connectors: {
			type: 'array',
			description:
				'Named MateConnector features of the placed parts, in world coordinates: pass feature_id and instance_path ' +
				'to connector_add as part_connector.',
			items: {
				type: 'object',
				properties: {
					feature_id: { type: 'string' },
					name: { type: 'string' },
					instance_path: { type: 'array', items: { type: 'string' } },
					kind: { type: ['string', 'null'] },
					origin: vec3(),
					x_axis: vec3(),
					y_axis: vec3(),
					z_axis: vec3()
				},
				required: ['feature_id', 'name', 'instance_path', 'origin', 'x_axis', 'y_axis', 'z_axis']
			}
		},
		available_parts: {
			type: 'array',
			description:
				'Every tab an instance can be made of: the document\'s Part tabs, its other Assembly tabs (sub-assemblies), ' +
				'and the tabs of available linked sources.',
			items: {
				type: 'object',
				properties: {
					tab_id: { type: 'string' },
					name: { type: 'string' },
					kind: { type: 'string' },
					source_id: { type: ['string', 'null'] },
					source_name: { type: ['string', 'null'] }
				},
				required: ['tab_id', 'name', 'kind', 'source_id', 'source_name']
			}
		},
		errors: { type: 'array', items: { type: 'string' }, description: 'The evaluation\'s problems, verbatim.' },
		warnings: { type: 'array', items: { type: 'string' } }
	},
	required: ['tab_id', 'name', 'instances', 'connectors', 'mates', 'part_connectors', 'available_parts', 'errors', 'warnings']
};

/** @param {Record<string, object>} extra */
function withState(extra) {
	return {
		...assemblyStateSchema,
		properties: { ...extra, ...assemblyStateSchema.properties },
		required: [...Object.keys(extra), ...assemblyStateSchema.required]
	};
}

export const assemblyGetTool = {
	name: 'assembly_get',
	description:
		'The open Assembly tab: its instances with solved placements, mate connectors with evaluated world frames, ' +
		'mates, the placed parts\' named connectors, the parts an instance can be made of, and the evaluation\'s ' +
		'errors and warnings. ' +
		REQUIRES_ASSEMBLY_TAB +
		' ' +
		UNITS_NOTE,
	inputSchema: noArguments,
	outputSchema: assemblyStateSchema,
	annotations: readOnly('Get assembly')
};

export const instanceAddTool = {
	name: 'instance_add',
	description:
		'Place an instance of a part in the open assembly, as the Assembly panel\'s "Add instance" does: a Part tab of ' +
		'this document, another Assembly tab (a sub-assembly), or a tab of a linked source (tab_id + source_id) — ' +
		'see assembly_get.available_parts. The first instance is implicitly grounded; fixed grounds this one explicitly. The ' +
		'transform is where the part sits until a mate moves it (mates never move a fixed instance). ' +
		'Returns instance_id with the assembly state. ' +
		REQUIRES_ASSEMBLY_TAB +
		' ' +
		NOT_AN_UNDO_STEP,
	inputSchema: {
		type: 'object',
		properties: {
			tab_id: { type: 'string', description: 'The part\'s tab id.' },
			source_id: { type: 'string', description: 'A linked .waffle source id (document_info.sources) when the tab is in one.' },
			name: { type: 'string', minLength: 1, description: 'Default: "<part name> N".' },
			transform: transformInput,
			fixed: { type: 'boolean', default: false }
		},
		required: ['tab_id'],
		additionalProperties: false
	},
	outputSchema: withState({ instance_id: { type: 'string' } }),
	annotations: edits('Add instance')
};

export const instanceEditTool = {
	name: 'instance_edit',
	description:
		'Change an instance of the open assembly: its name, placement transform (only the fields given change), ' +
		'grounding (fixed) or suppression. The assembly is re-solved. ' +
		REQUIRES_ASSEMBLY_TAB +
		' ' +
		NOT_AN_UNDO_STEP,
	inputSchema: {
		type: 'object',
		properties: {
			instance_id: uuid('Instance id from assembly_get.'),
			name: { type: 'string', minLength: 1 },
			transform: transformInput,
			fixed: { type: 'boolean' },
			suppressed: { type: 'boolean' }
		},
		required: ['instance_id'],
		additionalProperties: false
	},
	outputSchema: assemblyStateSchema,
	annotations: edits('Edit instance')
};

export const instanceDeleteTool = {
	name: 'instance_delete',
	description:
		'Remove an instance from the open assembly, with every connector on it and every mate using those ' +
		'connectors. ' +
		REQUIRES_ASSEMBLY_TAB +
		' ' +
		NOT_AN_UNDO_STEP,
	inputSchema: {
		type: 'object',
		properties: { instance_id: uuid('Instance id from assembly_get.') },
		required: ['instance_id'],
		additionalProperties: false
	},
	outputSchema: assemblyStateSchema,
	annotations: deletes('Delete instance')
};

export const connectorAddTool = {
	name: 'connector_add',
	description:
		'Add a mate connector (a named frame) on an instance of the open assembly, from ONE of: part_connector (the ' +
		'id of a MateConnector feature of the instance\'s part — see assembly_get.part_connectors or the part\'s ' +
		'model_summary.connectors), geom_ref (a face or edge of the PART, e.g. from face_list on the part tab or ' +
		'selection_get on a clicked instance: a planar face gives its normal as z at its centroid, a cylindrical ' +
		'face or circular edge gives its axis — refused with ConnectorRefused when no frame can be derived), or ' +
		'frame (explicit, in the part\'s coordinates: origin, z_axis, x_axis; zero x_axis = any perpendicular). ' +
		'With none of the three the connector sits at the part\'s origin, z up. instance_path is [instance_id] ' +
		'for a part instance or [instance_id, member_id, …] for a member of a sub-assembly instance. Adjust it ' +
		'afterwards with connector_edit. Returns connector_id with the assembly state. ' +
		REQUIRES_ASSEMBLY_TAB +
		' ' +
		NOT_AN_UNDO_STEP,
	inputSchema: {
		type: 'object',
		properties: {
			instance_path: { type: 'array', items: { type: 'string' }, minItems: 1, description: 'Instance id(s) from assembly_get.' },
			part_connector: uuid('MateConnector feature id of the instance\'s part.'),
			geom_ref: engineRef('GeomRef'),
			frame: engineRef('Frame'),
			name: { type: 'string', minLength: 1 }
		},
		required: ['instance_path'],
		additionalProperties: false,
		$defs: defsFor('GeomRef', 'Frame')
	},
	outputSchema: withState({ connector_id: { type: 'string' } }),
	annotations: edits('Add connector')
};

export const connectorEditTool = {
	name: 'connector_edit',
	description:
		'Adjust a mate connector of the open assembly (each field only when given): name; anchor — where on a ' +
		'rotational face\'s axis the frame sits (middle, positive_end, negative_end; ignored for other picks); ' +
		'flip_z reverses z (a 180° turn about x); rotation_deg turns about z after the flip (what a Fastened ' +
		'mate\'s in-plane alignment uses); offset_m moves along the connector\'s OWN axes after the turn, meters. ' +
		'Defaults (middle, false, 0, [0,0,0]) mean "as derived". ' +
		REQUIRES_ASSEMBLY_TAB +
		' ' +
		NOT_AN_UNDO_STEP,
	inputSchema: {
		type: 'object',
		properties: {
			connector_id: uuid('Connector id from assembly_get.'),
			name: { type: 'string', minLength: 1 },
			anchor: engineRef('AxialAnchor'),
			flip_z: { type: 'boolean' },
			rotation_deg: { type: 'number' },
			offset_m: vec3('Meters, along the connector\'s own x, y, z.')
		},
		required: ['connector_id'],
		additionalProperties: false,
		$defs: defsFor('AxialAnchor')
	},
	outputSchema: assemblyStateSchema,
	annotations: edits('Edit connector')
};

export const connectorDeleteTool = {
	name: 'connector_delete',
	description:
		'Remove a mate connector from the open assembly, with every mate using it. ' + REQUIRES_ASSEMBLY_TAB + ' ' + NOT_AN_UNDO_STEP,
	inputSchema: {
		type: 'object',
		properties: { connector_id: uuid('Connector id from assembly_get.') },
		required: ['connector_id'],
		additionalProperties: false
	},
	outputSchema: assemblyStateSchema,
	annotations: deletes('Delete connector')
};

export const mateAddTool = {
	name: 'mate_add',
	description:
		'Mate two connectors of the open assembly, as the Assembly panel\'s "Add mate" does: connector b\'s instance ' +
		'is placed so its frame meets a\'s (a grounded instance never moves; a mate between two connectors on the ' +
		'same instance is reported, not solved). ' +
		MATE_KIND_NOTE +
		' The assembly is re-solved; read the placements and any errors from the returned state. Returns mate_id. ' +
		REQUIRES_ASSEMBLY_TAB +
		' ' +
		NOT_AN_UNDO_STEP,
	inputSchema: {
		type: 'object',
		properties: {
			a: uuid('Connector id (the reference side).'),
			b: uuid('Connector id (the side that moves).'),
			kind: { ...mateKindEnum, default: 'Fastened' },
			flip: { type: 'boolean', default: true },
			rotation_deg: { type: 'number', default: 0 },
			name: { type: 'string', minLength: 1, description: 'Default: "<kind> N".' }
		},
		required: ['a', 'b'],
		additionalProperties: false
	},
	outputSchema: withState({ mate_id: { type: 'string' } }),
	annotations: edits('Add mate')
};

export const mateEditTool = {
	name: 'mate_edit',
	description:
		'Change a mate of the open assembly (each field only when given): name, kind, flip, rotation_deg, suppressed. ' +
		MATE_KIND_NOTE +
		' ' +
		REQUIRES_ASSEMBLY_TAB +
		' ' +
		NOT_AN_UNDO_STEP,
	inputSchema: {
		type: 'object',
		properties: {
			mate_id: uuid('Mate id from assembly_get.'),
			name: { type: 'string', minLength: 1 },
			kind: mateKindEnum,
			flip: { type: 'boolean' },
			rotation_deg: { type: 'number' },
			suppressed: { type: 'boolean' }
		},
		required: ['mate_id'],
		additionalProperties: false
	},
	outputSchema: assemblyStateSchema,
	annotations: edits('Edit mate')
};

export const mateDeleteTool = {
	name: 'mate_delete',
	description: 'Remove a mate from the open assembly; its connectors stay. ' + REQUIRES_ASSEMBLY_TAB + ' ' + NOT_AN_UNDO_STEP,
	inputSchema: {
		type: 'object',
		properties: { mate_id: uuid('Mate id from assembly_get.') },
		required: ['mate_id'],
		additionalProperties: false
	},
	outputSchema: assemblyStateSchema,
	annotations: deletes('Delete mate')
};
