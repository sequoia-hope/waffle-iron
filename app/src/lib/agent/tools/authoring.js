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
	'operation is an Operation: {"type":"Extrude","params":{…}}, Revolve, Pipe, Sweep, BooleanCombine, UnionAll, DatumPlane, MateConnector, ' +
	'PatternCircular, PatternLinear, PatternMirror, Script, a full Sketch, or a Sketch3d. A Sketch3d is SPATIAL ' +
	'reference geometry — lines and arcs in 3D and the points that define them — and produces NO body: it is the ' +
	'path a sweep or a frame runs along, and it is not extrudable (no plane, no region, no profile). ' +
	'{"type":"Sketch3d","sketch":{"id": a uuid, "entities":[…]}} with entities: ' +
	'{"type":"Point", id, xyz:[x,y,z], attach?, xyz_expr?: [expr|null, …] (mm-space), construction?}, ' +
	'{"type":"Line", id, start_id, end_id}, {"type":"Arc", id, start_id, end_id, via_id} (three distinct ' +
	'non-collinear points — via_id is any point ON the arc), and {"type":"Fillet", id, at_point_id, radius (m), ' +
	'radius_expr?} which rounds the corner where exactly two STRAIGHT segments meet, exactly tangent to both. ' +
	'Two points coincide by SHARING a point id, not by a constraint — there is no 3D solver. A point derives its ' +
	'position instead of stating it with attach: {"type":"AlongAxis", from: another point id, axis:"X"|"Y"|"Z", ' +
	'distance} (the axis-locked run most frame geometry is made of), {"type":"Offset", from, delta:[x,y,z]}, ' +
	'{"type":"Vertex", reference: a Vertex GeomRef}, {"type":"EdgePoint", reference: an Edge GeomRef, t: 0..1 ' +
	'along it by arc length}, or {"type":"OnPlane", reference: a planar face or datum plane, uv}. Read back what ' +
	'it evaluated to with sketch3d_get — an attached point resolves only at rebuild, so the xyz you wrote is just ' +
	'a hint. A UnionAll folds EVERY live body of the part (or ' +
	'params.targets {type:"Selected", bodies:[Solid GeomRefs]}) into connected solids with ONE feature — a balanced ' +
	'tree of pairwise unions with a bounding-box fast path — so prefer it over a chain of BooleanCombine steps on many ' +
	'overlapping bodies: params {targets?: {type:"All"}}. A BooleanCombine whose operand was already consumed by an ' +
	'earlier feature is refused (chain onto that feature\'s own output instead). A Pipe sweeps a circle along an OPEN, tangent-continuous ' +
	'chain of sketch lines and arcs (construction geometry is fine) as ONE solid: params {sketch_id, entity_ids: [the ' +
	'path entities, any order], radius (m), inner_radius? (m, hollow), combine?, targets?} — no boolean between segments, ' +
	'so a handlebar or hose is one body; a corner or a bend tighter than the tube radius is refused. A Sweep carries a planar sketch ' +
	'PROFILE (a plain polygon for now — a circle, arc-bearing or holed section is refused as a capability wall, never chord-approximated) ' +
	'along a path of lines and arcs as ONE solid, no boolean between segments: params {sketch_id: the SECTION sketch, profile_index ' +
	'(or profile_entity_ids, or region: {outer: [[u,v],…]} for a sub-region), path: {type:"Sketch", sketch_id, entity_ids: [lines/arcs, ' +
	'any order, construction fine — the path sketch may be the section sketch or another]} or {type:"Sketch3d", sketch_id, entity_id?: ' +
	'a member of the chain wanted (omit when the 3D sketch has one chain)}, combine?, targets?}. The path may be OPEN (two caps) or CLOSED ' +
	'(a ring, no caps). Where the path STARTS: an open sketch path at the free end holding the first listed entity; a closed sketch path ' +
	'at the START point of the FIRST listed entity, walking that entity\'s own direction; a Sketch3d chain at its first edge\'s start as ' +
	'sketch3d_get orders it. That start point is where the section pierces. Corners between two STRAIGHT segments are mitred; a bend (arc) must be entered and left tangentially. The pierce ' +
	'rule: the section sketch plane must be perpendicular to the path\'s first segment and contain the path\'s start point — draw the ' +
	'section on a plane through the path start, normal to its first segment; nothing auto-centres it (an offset section is a legitimate ' +
	'offset member). Lengths in meters. A Script runs a custom feature script (Rhai) that ' +
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

const APPEARANCE_SCHEMA = {
	type: 'object',
	description: 'How the material looks, for the viewport and for a future AP242 STYLED_ITEM.',
	properties: {
		color: {
			type: 'array',
			items: { type: 'number', minimum: 0, maximum: 1 },
			minItems: 3,
			maxItems: 3,
			description: 'sRGB, each channel 0..1.'
		},
		metalness: { type: 'number', minimum: 0, maximum: 1 },
		roughness: { type: 'number', minimum: 0, maximum: 1 }
	},
	required: ['color'],
	additionalProperties: false
};

export const materialListTool = {
	name: 'material_list',
	description:
		'Read the open Part\'s material table and which body is made of what. Each material is ' +
		'{name, density_kg_m3, appearance?}; bodies lists {body_id, material, resolves}; dangling lists ' +
		'assignments whose material has been deleted — those bodies have NO mass and measure_mass refuses ' +
		'them by name rather than reporting the volume in kilograms. The table is per-Part (a document-level ' +
		'table arrives with P2 of specs/agent_mechanical_design.md §6).',
	inputSchema: { type: 'object', properties: {}, additionalProperties: false },
	outputSchema: {
		type: 'object',
		properties: {
			materials: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						name: { type: 'string' },
						density_kg_m3: { type: 'number' },
						appearance: APPEARANCE_SCHEMA
					},
					required: ['name', 'density_kg_m3']
				}
			},
			bodies: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						body_id: { type: 'string' },
						material: { type: 'string' },
						resolves: { type: 'boolean' }
					},
					required: ['body_id', 'material', 'resolves']
				}
			},
			dangling: {
				type: 'array',
				items: {
					type: 'object',
					properties: { body_id: { type: 'string' }, material: { type: 'string' } },
					required: ['body_id', 'material']
				}
			}
		},
		required: ['materials', 'bodies', 'dangling'],
		additionalProperties: false
	},
	annotations: { title: 'List materials', readOnlyHint: true }
};

export const materialSetTool = {
	name: 'material_set',
	description:
		'Add, change, rename or delete a material in the open Part\'s table (one undo step). ' +
		'Exactly one mode per call: rename_to renames (every body made of it follows), delete:true removes it ' +
		'(every body made of it is left with no material, which is loud rather than silent), and otherwise ' +
		'the call is an upsert needing density_kg_m3 — a positive number of kg/m³ (aluminium 2700, steel 7850, ' +
		'brass 8500, ABS 1040, PLA 1240). Materials are referred to BY NAME, so two with one name is refused ' +
		'and a rename is the only way to change one. Assign a material to a body with body_material_set.',
	inputSchema: {
		type: 'object',
		properties: {
			name: { type: 'string', minLength: 1, description: 'The material to add, change, rename or delete.' },
			density_kg_m3: { type: 'number', exclusiveMinimum: 0, description: 'Required for an upsert.' },
			appearance: APPEARANCE_SCHEMA,
			rename_to: { type: 'string', minLength: 1, description: 'Rename mode: the new name.' },
			delete: { type: 'boolean', description: 'Delete mode.' }
		},
		required: ['name'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema(),
	annotations: edit('Set material', { idempotentHint: true })
};

export const bodyMaterialSetTool = {
	name: 'body_material_set',
	description:
		'Say what a body is made of (one undo step). material names an entry in the table (add it with ' +
		'material_set first — an unknown name is refused, not created); material:null clears it. A body with ' +
		'no material has no mass: measure_mass and a mass(body) expression both refuse by name rather than ' +
		'defaulting the density to 1, where the mass would be numerically the volume.',
	inputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string', description: 'Body id or name from model_summary.bodies.' },
			material: { type: ['string', 'null'], description: 'A material name, or null to clear.' }
		},
		required: ['body_id', 'material'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema(),
	annotations: edit('Set body material', { idempotentHint: true })
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
		'Write the design-parameter table and rebuild (one undo step). By default parameters REPLACES the ' +
		'table, so an omitted parameter is removed; with merge:true the rows are applied over the current ' +
		'table and delete removes parameters by name or id, so you can set one value without re-sending the ' +
		'rest. Expressions are mm-space and may reference other parameters by name, and may MEASURE the model ' +
		'by entity name (distance, angle, length, radius, area, volume — see expression_evaluate); a parameter ' +
		'that measures acquires a rebuild dependency on the feature that owns the measured entity, and may only ' +
		'measure geometry earlier in the tree than the first feature that reads it. Keep a parameter\'s id ' +
		'to preserve it; omit id for a new one. A row whose id names an existing parameter with a DIFFERENT ' +
		'name is a rename: every expression that reads it — other parameters and feature fields alike — is ' +
		'rewritten, so use this rather than deleting and re-adding. A failing expression is reported per ' +
		'parameter, not rolled back; a delete of a parameter something still reads is refused naming the ' +
		'dependents (read them first with parameters_get). scope chooses which table: tab (the default) ' +
		'the open Part tab\'s, document the document-wide one that every tab resolves through after its ' +
		'own (NOT an undo step, and a RENAME there is refused — the rewrite would have to reach every ' +
		'tab and this call can only reach the open one), or instance, which with instance_id and ' +
		'overrides pins magnitudes on one placed instance of a part so it builds its own solid.',
	inputSchema: {
		type: 'object',
		properties: {
			scope: {
				type: 'string',
				enum: ['tab', 'document', 'instance'],
				description: 'Which table to write. Default tab.'
			},
			instance_id: uuid('Required for scope instance: the assembly instance to parameterise.'),
			overrides: {
				type: ['object', 'null'],
				description:
					'scope instance only: {parameter name: working-space magnitude} (mm for a length, degrees ' +
					'for an angle). The DIMENSION comes from the part\'s parameter, not from the number, so a ' +
					'Count parameter still refuses a fraction. With merge:true a name set to null removes just ' +
					'that override; null here clears them all. A name the part does not declare is a loud ' +
					'rebuild error naming it.',
				additionalProperties: { type: ['number', 'null'] }
			},
			parameters: {
				type: 'array',
				description:
					'The complete table, or (with merge:true) just the rows to set. Required without merge.',
				items: {
					type: 'object',
					properties: {
						id: uuid('Existing parameter id (from parameters_get); omit for a new parameter.'),
						name: { type: 'string', pattern: '^[A-Za-z_][A-Za-z0-9_]*$' },
						expression: { type: 'string', minLength: 1 },
						unit: {
							type: ['string', 'null'],
							enum: ['Length', 'Angle', 'Count', 'Ratio', 'Mass', 'Density', null],
							description:
								'Declare the dimension the expression must produce; every field that reads this ' +
								'parameter is then checked against it. Omitting this keeps what the parameter has ' +
								'(in both modes) and null clears it.'
						},
						comment: {
							type: ['string', 'null'],
							description: 'Omitting this keeps the current comment; null clears it.'
						}
					},
					additionalProperties: false
				}
			},
			merge: {
				type: 'boolean',
				description:
					'Apply parameters over the current table instead of replacing it. Required for delete.'
			},
			delete: {
				type: 'array',
				items: { type: 'string' },
				description:
					'Parameters to remove, by name or id. Needs merge:true (without merge, omitting a parameter ' +
					'already removes it). Refused if anything still reads one of them.'
			}
		},
		additionalProperties: false
	},
	outputSchema: commandOutputSchema({
		parameters: {
			type: 'array',
			description: 'The table as the rebuild evaluated it.',
			items: {
				type: 'object',
				properties: {
					id: { type: 'string' },
					name: { type: 'string' },
					expression: {
						type: 'string',
						description:
							'Echoed, because after a rename or a merge the table holds expressions you did not send.'
					},
					value_mm: { type: ['number', 'null'] },
					unit: { type: 'string', enum: ['Length', 'Angle', 'Count', 'Ratio', 'Mass', 'Density'] },
					comment: { type: 'string' },
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
