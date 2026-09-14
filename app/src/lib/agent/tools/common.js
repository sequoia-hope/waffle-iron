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
