/**
 * Schema fragments shared by the agent tool definitions (specs/waffle_mcp_server.md §2.5).
 * Plain data, loaded by the page and by the manifest generator.
 */

export const uuid = (description) => ({ type: 'string', format: 'uuid', description });

export const onErrorSchema = {
	type: 'string',
	enum: ['rollback', 'keep'],
	default: 'rollback',
	description:
		'What to do when the step makes a feature fail to rebuild. "rollback" (default) undoes the step and returns ' +
		'an error; "keep" leaves it in place and reports kept_with_error with the failing features.'
};

/** The `ModelDelta` every command returns (spec §2.5). */
export const modelDeltaProperties = {
	features_added: { type: 'array', items: { type: 'string' }, description: 'Feature ids added, in tree order.' },
	features_changed: {
		type: 'array',
		items: { type: 'string' },
		description: 'Feature ids whose definition or provenance changed.'
	},
	features_removed: { type: 'array', items: { type: 'string' } },
	order_changed: { type: 'boolean', description: 'The relative order of surviving features changed.' },
	bodies_added: { type: 'array', items: { type: 'string' } },
	bodies_removed: { type: 'array', items: { type: 'string' } },
	errors: {
		type: 'array',
		description: 'Every current rebuild error in tree order, verbatim from the engine; kind is the typed class when known.',
		items: {
			type: 'object',
			properties: {
				feature_id: { type: 'string' },
				message: { type: 'string' },
				kind: { type: 'object', description: 'Engine ErrorKind, e.g. {"type":"NotSupported","operation":"…"}.' }
			},
			required: ['feature_id', 'message']
		}
	},
	warnings: { type: 'array', items: { type: 'string' }, description: 'Rebuild warnings, verbatim.' },
	kept_with_error: {
		type: 'boolean',
		description: 'Present and true when on_error was "keep" and the step left features failing.'
	}
};

/** @param {Record<string, object>} [extra] extra result properties */
export function commandOutputSchema(extra = {}) {
	return {
		type: 'object',
		properties: { ...extra, ...modelDeltaProperties },
		required: ['features_added', 'features_changed', 'features_removed', 'bodies_added', 'bodies_removed', 'errors', 'warnings']
	};
}

export const noArguments = { type: 'object', properties: {}, additionalProperties: false };

export const UNITS_NOTE =
	'Units: lengths in meters, angles in degrees; *_expr expressions are mm-space (a bare 20 means 20 mm).';

/**
 * The solver state `sketch_create`, `sketch_edit` and `sketch_solve_state` all
 * report (`specs/agent_mechanical_design.md` §10.2/§10.3, S2/S3).
 *
 * Shared, not copied per tool: the three tools report the same object because
 * it comes from one `SketchSolveReport`, and three hand-maintained copies of a
 * schema is three answers to "what does dof mean".
 */
export const sketchStateSchema = {
	type: 'object',
	description:
		'Solver state as data. INDEX SPACE: conflicts, redundant and residuals[].index index the constraint ' +
		'array that was solved — the sketch\'s own `constraints` of them, followed by any transient_constraints ' +
		'(a MovePoint pin). So an index below `constraints` names a stored constraint.',
	properties: {
		status: {
			type: 'string',
			description: 'FullyConstrained | UnderConstrained | OverConstrained | SolveFailed | Unsolved'
		},
		dof: {
			type: 'integer',
			description:
				'Degrees of freedom left (params − rank). Present for every verdict, including the failures where ' +
				'the top-level dof of sketch_create is null.'
		},
		params: { type: 'integer', description: 'Solver parameters: 2 per point, 1 per circle radius.' },
		rank: { type: 'integer', description: 'Rank of the final constraint Jacobian.' },
		rows: {
			type: 'integer',
			description: 'Residual rows handed to the solver; a multi-row constraint contributes several. rank < rows on a satisfied system is redundancy.'
		},
		constraints: { type: 'integer', description: 'How many constraints the sketch stores (see INDEX SPACE above).' },
		conflicts: {
			type: 'array',
			items: { type: 'integer' },
			description: 'Driving constraints whose residual exceeds tolerance, worst first. Always populated, not only when over-constrained.'
		},
		redundant: {
			type: 'array',
			items: { type: 'integer' },
			description: 'On a SATISFIED system with rank < rows: the dependent constraints, each adding no rank to those before it.'
		},
		residuals: {
			type: 'array',
			description: 'One entry per constraint, in declaration order.',
			items: {
				type: 'object',
				properties: {
					index: { type: 'integer' },
					kind: { type: 'string' },
					residual: {
						type: ['number', 'null'],
						description:
							'Largest UNWEIGHTED residual over the constraint\'s rows; meters for a length. null when it ' +
							'could not be compiled at all (only reachable for a reference dimension) — never a stand-in zero.'
					},
					satisfied: { type: 'boolean' },
					reference: { type: 'boolean', description: 'A driven dimension: measured, never driving, never an offender.' }
				},
				required: ['index', 'kind', 'satisfied', 'reference']
			}
		},
		moved: {
			type: 'array',
			description: 'Points the solve displaced against the positions it started from, worst first.',
			items: {
				type: 'object',
				properties: {
					id: { type: 'integer' },
					dx: { type: 'number' },
					dy: { type: 'number' },
					distance: { type: 'number' }
				},
				required: ['id', 'dx', 'dy', 'distance']
			}
		},
		free: {
			type: 'array',
			description:
				'A null-space basis of the final Jacobian: dof directions the geometry can still move in. ' +
				'free.length === dof by construction. Components below the noise floor are omitted.',
			items: {
				type: 'object',
				properties: {
					basis: { type: 'integer' },
					components: {
						type: 'array',
						items: {
							type: 'object',
							description: '{"type":"Point",id,dx,dy} or {"type":"Radius",entity,dr}.',
							properties: { type: { type: 'string' } },
							required: ['type']
						}
					}
				},
				required: ['basis', 'components']
			}
		},
		convergence: {
			type: 'object',
			description: 'Why Levenberg-Marquardt stopped — about the solve, not about the sketch.',
			properties: {
				termination: { type: 'object', description: 'Typed reason, e.g. {"type":"Converged","ftol":true,"xtol":false}.' },
				successful: { type: 'boolean', description: "LM's own view, which is not the same as \"the sketch solved\"." },
				evaluations: { type: 'integer' },
				residual_inf: { type: 'number', description: 'Infinity-norm over the WEIGHTED rows: the number the verdict is made on.' },
				tolerance: { type: 'number' }
			},
			required: ['termination', 'successful', 'evaluations', 'residual_inf', 'tolerance']
		},
		positions: {
			type: 'object',
			description: 'Solved point coordinates in sketch meters, by point id: {"4": [x, y]}. Ascending id order.',
			additionalProperties: { type: 'array', items: { type: 'number' }, minItems: 2, maxItems: 2 }
		},
		radii: {
			type: 'object',
			description:
				'Solved circle radii by entity id. A Radius/Diameter constraint solves a radius parameter, which ' +
				'never travels through positions. Arcs are absent — their radius is the centre→start distance.',
			additionalProperties: { type: 'number' }
		},
		transient_constraints: {
			type: 'array',
			description: 'Present only when the batch held a MovePoint: the pins that drove this one solve and were then dropped.',
			items: {
				type: 'object',
				properties: { index: { type: 'integer' }, kind: { type: 'string' } },
				required: ['index', 'kind']
			}
		}
	},
	required: [
		'status',
		'dof',
		'params',
		'rank',
		'rows',
		'constraints',
		'conflicts',
		'redundant',
		'residuals',
		'moved',
		'free',
		'convergence',
		'positions',
		'radii'
	]
};

/** The closed loops a sketch tool reports; a region's ids go to an Extrude. */
export const sketchRegionsSchema = {
	type: 'array',
	items: {
		type: 'object',
		properties: {
			profile_entity_ids: { type: ['array', 'null'], items: { type: 'integer' } },
			area_m2: { type: 'number' }
		}
	}
};
