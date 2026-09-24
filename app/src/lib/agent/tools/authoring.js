/**
 * Authoring agent tools (specs/waffle_mcp_server.md §2.5 Authoring). Definitions
 * only; implementations are the engine's (`crates/wasm-bridge/src/tools/{author,sketch}.rs`,
 * S3), routed by `../executor.js`. Every command is one undo step
 * for the user (I5) unless its description says otherwise.
 */
import { UNITS_NOTE, commandOutputSchema, onErrorSchema, uuid } from './common.js';
import { defsFor, engineRef } from './engineSchemas.js';

const vec3 = (description) => ({ type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3, description });

const edit = (title, extra = {}) => ({ title, readOnlyHint: false, destructiveHint: false, openWorldHint: false, ...extra });

export const sketchCreateTool = {
	name: 'sketch_create',
	description:
		'Create a sketch in one call: the entities and constraints are solved by the page and committed as a ' +
		'Sketch feature (one undo step). Sketch coordinates (Point x, y; Circle radius) are meters in the ' +
		'plane. plane is either a face or datum GeomRef (from selection_get or face_list) or an explicit ' +
		'{origin, normal} in world meters; EITHER form takes an optional x_axis — the world direction the ' +
		'sketch\'s +x points along, which is how you orient a rectangular member or a keyway without ' +
		'reproducing the engine\'s own basis. Without one the engine picks the in-plane axes, so read the ' +
		'result back (it answers with the plane basis it used) rather than assuming +x/+y. Entity ids are ' +
		'unsigned integers unique within the ' +
		'sketch; Lines/Arcs/Circles name Point ids. An over-constrained or failed solve is rolled back by ' +
		'default (SketchSolveFailed). regions lists the closed loops: pass a region\'s profile_entity_ids to ' +
		'an Extrude/Revolve. ' +
		UNITS_NOTE,
	inputSchema: {
		type: 'object',
		properties: {
			plane: {
				description:
					'A face/datum GeomRef, or {origin, normal}. Either may carry x_axis: the world direction ' +
					'the sketch\'s +x axis points along (its in-plane part is used). Parallel to the normal, ' +
					'or zero-length, is refused.',
				anyOf: [
					{
						allOf: [engineRef('GeomRef')],
						properties: { x_axis: vec3('Optional: the sketch +x direction in world space.') }
					},
					{
						type: 'object',
						properties: {
							origin: vec3('World point on the plane (m).'),
							normal: vec3('Plane normal.'),
							x_axis: vec3('Optional: the sketch +x direction in world space.')
						},
						required: ['origin', 'normal'],
						additionalProperties: false
					}
				]
			},
			entities: { type: 'array', items: engineRef('SketchEntity'), minItems: 1 },
			constraints: { type: 'array', items: engineRef('SketchConstraint'), default: [] },
			on_error: onErrorSchema
		},
		required: ['plane', 'entities'],
		additionalProperties: false,
		$defs: defsFor('GeomRef', 'SketchEntity', 'SketchConstraint')
	},
	outputSchema: commandOutputSchema({
		feature_id: { type: 'string' },
		solve_status: { type: 'string', description: 'FullyConstrained | UnderConstrained | OverConstrained | SolveFailed' },
		dof: { type: ['integer', 'null'] },
		plane: {
			type: 'object',
			description:
				'The basis the sketch got: sketch (x, y) is origin + x·x_axis + y·y_axis in world meters. ' +
				'Pass x_axis in to choose it; read it back to place anything oriented.',
			properties: {
				origin: vec3('Plane origin (m).'),
				normal: vec3('Unit plane normal.'),
				x_axis: vec3('Unit world direction of sketch +x.'),
				y_axis: vec3('Unit world direction of sketch +y.')
			},
			required: ['origin', 'normal', 'x_axis', 'y_axis']
		},
		regions: {
			type: 'array',
			items: {
				type: 'object',
				properties: {
					profile_entity_ids: { type: ['array', 'null'], items: { type: 'integer' } },
					area_m2: { type: 'number' }
				}
			}
		}
	}),
	annotations: edit('Create sketch')
};

const operationNote =
	'operation is an Operation: {"type":"Extrude","params":{…}}, Revolve, Pipe, BooleanCombine, UnionAll, DatumPlane, MateConnector, ' +
	'PatternCircular, PatternLinear, PatternMirror, Script, or a full Sketch. A UnionAll folds EVERY live body of the part (or ' +
	'params.targets {type:"Selected", bodies:[Solid GeomRefs]}) into connected solids with ONE feature — a balanced ' +
	'tree of pairwise unions with a bounding-box fast path — so prefer it over a chain of BooleanCombine steps on many ' +
	'overlapping bodies: params {targets?: {type:"All"}}. A BooleanCombine whose operand was already consumed by an ' +
	'earlier feature is refused (chain onto that feature\'s own output instead). A Pipe sweeps a circle along an OPEN, tangent-continuous ' +
	'chain of sketch lines and arcs (construction geometry is fine) as ONE solid: params {sketch_id, entity_ids: [the ' +
	'path entities, any order], radius (m), inner_radius? (m, hollow), combine?, targets?} — no boolean between segments, ' +
	'so a handlebar or hose is one body; a corner or a bend tighter than the tube radius is refused. A Script runs a custom feature script (Rhai) that ' +
	'the document carries as a `Script` source: params {source_id, entry?: "feature", args: {name: value in model ' +
	'units, or {origin, normal} / a datum plane id for a plane param}, arg_exprs?: {name: "expression"}}; the ' +
	'script declares its parameters in `// @param name: type` header lines and calls ctx.sketch / extrude / ' +
	'revolve / boolean over the same operations as these tools; add the source with script_source_add and prefer ' +
	'script_feature_add (same node, takes the args directly) — see docs/CUSTOM_FEATURE_SCRIPTS.md. ' +
	'A PatternCircular/PatternLinear/PatternMirror copies seed ' +
	'BODIES (params.seeds: Solid GeomRefs {kind:"Solid", anchor:{type:"FeatureOutput", feature_id, output_key}}, ' +
	'from model_summary bodies, or {"type":"All"} for EVERY live body at that point in the tree — which is how you ' +
	'say "take everything I just built and turn it four times" without listing it) — it does not re-run the seed feature. Circular: axis {method:"explicit", origin, ' +
	'direction} or {method:"entity", geom_ref: a cylindrical/conical face, a circular edge, or a straight edge}, ' +
	'count (instances INCLUDING the seed, ≥ 2), angle_deg (TOTAL sweep; 360 spaces 360/count apart, otherwise the ' +
	'last instance lands at angle_deg), skip?: [instance indices ≥ 1]. Linear: direction (AxisRef, direction only), ' +
	'count, spacing (m; negative reverses), second?: {direction, count, spacing} for a grid (index i + j·count). ' +
	'Mirror: plane {method:"explicit", origin: a point on the plane, direction: the plane NORMAL} or ' +
	'{method:"entity", geom_ref: a planar face or datum plane}; one copy, the seed\'s mirror image (no count, no ' +
	'skip). ' +
	'All: combine?: NewBody (default: every instance its own body) | Add (folds targets + instances into connected ' +
	'bodies) | Cut | Intersect, with targets?: explicit Solid GeomRefs (never auto by position; Cut/Intersect need ' +
	'one). The pattern takes custody of its seeds (their features are consumed) and emits every instance: Main = ' +
	'the seed itself, then Body:1… — so chain later booleans onto the PATTERN\'s outputs, not the seed feature\'s. ' +
	'A MateConnector is a named frame on the part that assemblies mate its instances by ' +
	'(model_summary lists the evaluated ones): params {name, geom_ref?: a Face or Edge GeomRef (face_list, ' +
	'selection_get), frame?: {origin, z_axis, x_axis} in part coordinates when there is no geom_ref, anchor?: ' +
	'"middle"|"positive_end"|"negative_end" along a cylindrical/conical/toroidal face\'s axis, flip_z?, ' +
	'rotation_deg?, offset_m?: [x, y, z] along its own axes}; a pick with no frame (a vertex, a freeform face) ' +
	'fails the feature. Address a sketch loop with params.sketch_id = the Sketch feature id and ' +
	'params.profile_entity_ids = the loop\'s entity ids (from sketch_create or sketch_regions); profile_index ' +
	'is then ignored but still required (use 0). Fillet, Chamfer and Shell are refused (Deferred); STEP ' +
	'imports are not authored here. A step whose feature or any downstream feature newly fails to rebuild is ' +
	'rolled back by default; kernel capability limits (NotSupported) are reported verbatim — do not retry ' +
	'them with altered parameters. ';

export const featureAddTool = {
	name: 'feature_add',
	description: `Add a feature at the end of the feature tree (one undo step). ${operationNote}${UNITS_NOTE}`,
	inputSchema: {
		type: 'object',
		properties: { operation: engineRef('Operation'), on_error: onErrorSchema },
		required: ['operation'],
		additionalProperties: false,
		$defs: defsFor('Operation')
	},
	outputSchema: commandOutputSchema({ feature_id: { type: 'string' } }),
	annotations: edit('Add feature')
};

export const featureEditTool = {
	name: 'feature_edit',
	description:
		'Replace a feature\'s operation (same type; read it with feature_get first) and rebuild (one undo step). ' +
		'The feature becomes agent-authored. Imported and derived features cannot be edited. ' +
		operationNote +
		UNITS_NOTE,
	inputSchema: {
		type: 'object',
		properties: {
			feature_id: uuid('Feature to edit.'),
			operation: engineRef('Operation'),
			on_error: onErrorSchema
		},
		required: ['feature_id', 'operation'],
		additionalProperties: false,
		$defs: defsFor('Operation')
	},
	outputSchema: commandOutputSchema(),
	annotations: edit('Edit feature')
};

export const featureDeleteTool = {
	name: 'feature_delete',
	description:
		'Delete a feature (one undo step). Features that depended on it may start failing; their errors are ' +
		'listed in errors and the delete is not rolled back.',
	inputSchema: {
		type: 'object',
		properties: { feature_id: uuid('Feature to delete.') },
		required: ['feature_id'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema(),
	annotations: edit('Delete feature', { destructiveHint: true })
};

export const featureSuppressTool = {
	name: 'feature_suppress',
	description: 'Suppress (skip in the rebuild) or unsuppress a feature (one undo step).',
	inputSchema: {
		type: 'object',
		properties: { feature_id: uuid('Feature to (un)suppress.'), suppressed: { type: 'boolean' } },
		required: ['feature_id', 'suppressed'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema(),
	annotations: edit('Suppress feature', { idempotentHint: true })
};

export const featureReorderTool = {
	name: 'feature_reorder',
	description: 'Move a feature to a new zero-based position in the tree and rebuild (one undo step).',
	inputSchema: {
		type: 'object',
		properties: {
			feature_id: uuid('Feature to move.'),
			new_position: { type: 'integer', minimum: 0, description: 'Zero-based index in the tree.' }
		},
		required: ['feature_id', 'new_position'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema(),
	annotations: edit('Reorder feature')
};

export const featureRenameTool = {
	name: 'feature_rename',
	description: 'Rename a feature (one undo step).',
	inputSchema: {
		type: 'object',
		properties: { feature_id: uuid('Feature to rename.'), new_name: { type: 'string', minLength: 1 } },
		required: ['feature_id', 'new_name'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema(),
	annotations: edit('Rename feature', { idempotentHint: true })
};

export const bodyRenameTool = {
	name: 'body_rename',
	description: 'Set a body\'s display name; an empty new_name reverts to the derived name (one undo step).',
	inputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string', description: 'Body id from model_summary.bodies.' },
			new_name: { type: 'string' }
		},
		required: ['body_id', 'new_name'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema(),
	annotations: edit('Rename body', { idempotentHint: true })
};

export const rollbackSetTool = {
	name: 'rollback_set',
	description:
		'Set the rollback bar: features after index are rolled back (not built); null makes every feature ' +
		'active. Not an undo step: undo does not move the rollback bar.',
	inputSchema: {
		type: 'object',
		properties: {
			index: { type: ['integer', 'null'], minimum: 0, description: 'Index of the last active feature, or null.' }
		},
		required: ['index'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema(),
	annotations: edit('Set rollback', { idempotentHint: true })
};

export const parametersSetTool = {
	name: 'parameters_set',
	description:
		'Replace the design-parameter table with the COMPLETE list (omitted parameters are removed) and rebuild ' +
		'(one undo step). Expressions are mm-space and may reference other parameters by name. Keep a ' +
		'parameter\'s id to preserve it; omit id for a new one. A failing expression is reported per ' +
		'parameter, not rolled back.',
	inputSchema: {
		type: 'object',
		properties: {
			parameters: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						id: uuid('Existing parameter id (from model_summary); omit for a new parameter.'),
						name: { type: 'string', pattern: '^[A-Za-z_][A-Za-z0-9_]*$' },
						expression: { type: 'string', minLength: 1 }
					},
					required: ['name', 'expression'],
					additionalProperties: false
				}
			}
		},
		required: ['parameters'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema({
		parameters: {
			type: 'array',
			items: {
				type: 'object',
				properties: {
					id: { type: 'string' },
					name: { type: 'string' },
					value_mm: { type: ['number', 'null'] },
					error: { type: 'string' }
				}
			}
		}
	}),
	annotations: edit('Set parameters')
};

export const undoTool = {
	name: 'undo',
	description: 'Undo the last feature-level step in the document (yours or the user\'s).',
	inputSchema: { type: 'object', properties: {}, additionalProperties: false },
	outputSchema: commandOutputSchema(),
	annotations: edit('Undo')
};

export const redoTool = {
	name: 'redo',
	description: 'Redo the last undone feature-level step.',
	inputSchema: { type: 'object', properties: {}, additionalProperties: false },
	outputSchema: commandOutputSchema(),
	annotations: edit('Redo')
};
