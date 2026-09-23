/**
 * Custom-feature-script agent tools (specs/custom_features_and_modeling_roadmap.md
 * §A8, A-M4). Definitions only; implementations are the engine's
 * (`crates/wasm-bridge/src/tools/script.rs`), routed by `../executor.js`.
 *
 * The authoring loop: write the script → `script_run_check` (with args for a
 * dry run) → `script_source_add` → `script_feature_add` → `feature_get` the
 * node's error → `script_source_update`. The reference for the script language
 * and API is docs/CUSTOM_FEATURE_SCRIPTS.md.
 */
import { UNITS_NOTE, commandOutputSchema, onErrorSchema, uuid } from './common.js';

const readOnly = (title) => ({ title, readOnlyHint: true, openWorldHint: false });
const edit = (title, extra = {}) => ({ title, readOnlyHint: false, destructiveHint: false, openWorldHint: false, ...extra });

const SCRIPT_NOTE =
	'A script is Rhai text with a header of `// @feature name="…" version=N`, `// @param name: type [= default] ' +
	'[min=..] [max=..]` (types: int, number, length (meters), angle (degrees), bool, string, plane, body, face, ' +
	'edge) and `// @output name: main|body|face|edge|connector` lines, then `fn feature(ctx, p) { … }` calling ' +
	'ctx.sketch(p.plane) → sk.point/line/circle/arc/rect/polygon/gear/sprocket → sk.finish().regions(), ' +
	'ctx.extrude / revolve / boolean / pattern_circular / pattern_linear / pipe / union_all / mate_connector, ' +
	'and query chains (.faces().normal_near([0,0,1], 5).largest_area()). Its return value names the node\'s ' +
	'outputs. See docs/CUSTOM_FEATURE_SCRIPTS.md. ';

const scriptArgs = {
	type: 'object',
	description:
		'Values for the script\'s @param declarations, by name, in MODEL units: int/number as numbers, length in ' +
		'meters, angle in degrees, bool, string; a plane as {origin, normal} or a datum plane id; a body/face/edge as a ' +
		'GeomRef (from model_summary bodies, face_list, selection_get). Undeclared names are refused; declared ' +
		'parameters without a default are required.',
	additionalProperties: true
};

const interfaceSchema = {
	type: 'object',
	description: 'The header the script declares.',
	properties: {
		name: { type: 'string' },
		version: { type: 'integer' },
		params: {
			type: 'array',
			items: {
				type: 'object',
				properties: {
					name: { type: 'string' },
					type: { type: 'string', enum: ['int', 'number', 'length', 'angle', 'bool', 'string', 'plane', 'body', 'face', 'edge'] },
					default: { description: 'Number, boolean or string; absent for a required parameter.' },
					min: { type: 'number' },
					max: { type: 'number' }
				},
				required: ['name', 'type']
			}
		},
		outputs: {
			type: 'array',
			items: {
				type: 'object',
				properties: { name: { type: 'string' }, kind: { type: 'string', enum: ['main', 'body', 'face', 'edge', 'connector'] } },
				required: ['name', 'kind']
			}
		}
	},
	required: ['name', 'version', 'params', 'outputs']
};

const checkError = {
	type: 'object',
	properties: {
		stage: { type: 'string', description: 'header | parse | args | runtime | limit | fail | child' },
		reason: { type: 'string', description: 'Verbatim; header failures start with "line N:", parse failures end with "(line N, position M)".' }
	},
	required: ['stage', 'reason']
};

const checkSchema = {
	ok: { type: 'boolean', description: 'Header parsed, script compiled, entry function present.' },
	interface: interfaceSchema,
	error: checkError,
	dry_run: {
		type: 'object',
		description: 'Present when args were given and the check passed.',
		properties: {
			ok: { type: 'boolean' },
			error: checkError,
			children: { type: 'array', items: { type: 'string' }, description: 'Recorded child operations in order (sketch, extrude, …).' },
			logs: { type: 'array', items: { type: 'string' } },
			outputs: { type: 'array', items: { type: 'string' }, description: 'Output names the return value provides.' }
		},
		required: ['ok']
	}
};

export const scriptRunCheckTool = {
	name: 'script_run_check',
	description:
		'Check a custom feature script without adding anything: parse its header, compile it and confirm the entry ' +
		'function. Pass text (unsaved script) or source_id (a stored script source). With args the script is also ' +
		'DRY-RUN against a recorder with no kernel — runtime errors, ctx.fail, limits and the @output contract are ' +
		'exercised and the recorded child operations listed — so run it with the args you intend to use before ' +
		'script_feature_add. Errors carry the stage and the interpreter\'s message verbatim. ' +
		SCRIPT_NOTE +
		UNITS_NOTE,
	inputSchema: {
		type: 'object',
		properties: {
			text: { type: 'string', description: 'Script text to check (unsaved).' },
			source_id: uuid('A stored script source (from script_source_add / script_source_get).'),
			entry: { type: 'string', description: 'Entry function name (default "feature").' },
			args: scriptArgs
		},
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: { ...checkSchema, source_id: { type: 'string' }, entry: { type: 'string' } },
		required: ['ok', 'entry']
	},
	annotations: readOnly('Check script')
};

export const scriptSourceAddTool = {
	name: 'script_source_add',
	description:
		'Add a custom feature script to the document as an embedded Script source (saved with the file, shareable ' +
		'by many nodes). Pass text, or library ("gear" or "sprocket") for one of the built-in scripts. Refused with ' +
		'InvalidScript (stage + reason) when the script does not check — run script_run_check first. The name ' +
		'defaults to the header\'s @feature name. Not an undo step (sources are assets); the source stays in the ' +
		'document even with no node using it. ' +
		SCRIPT_NOTE,
	inputSchema: {
		type: 'object',
		properties: {
			text: { type: 'string', description: 'The script text.' },
			library: { type: 'string', enum: ['gear', 'sprocket'], description: 'A built-in script instead of text.' },
			name: { type: 'string', description: 'Display name (default: the header\'s @feature name).' }
		},
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			source_id: { type: 'string', description: 'Pass to script_feature_add.' },
			name: { type: 'string' },
			interface: interfaceSchema,
			...commandOutputSchema().properties
		},
		required: ['source_id', 'name']
	},
	annotations: edit('Add script source')
};

export const scriptSourceGetTool = {
	name: 'script_source_get',
	description:
		'Read a script source: its text, its check (interface or error) and the features using it. Without ' +
		'source_id, list the document\'s script sources and the built-in library names.',
	inputSchema: {
		type: 'object',
		properties: { source_id: uuid('The script source; omit to list them all.') },
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			source_id: { type: 'string' },
			name: { type: 'string' },
			text: { type: 'string' },
			check: { type: 'object', properties: checkSchema, required: ['ok'] },
			features: {
				type: 'array',
				items: { type: 'object', properties: { feature_id: { type: 'string' }, name: { type: 'string' } } }
			},
			scripts: {
				type: 'array',
				description: 'List form: every Script source of the document.',
				items: {
					type: 'object',
					properties: {
						source_id: { type: 'string' },
						name: { type: 'string' },
						available: { type: 'boolean' },
						ok: { type: ['boolean', 'null'] },
						feature_name: { type: ['string', 'null'] },
						features: { type: 'array', items: { type: 'object' } }
					}
				}
			},
			library: { type: 'array', items: { type: 'string' } }
		}
	},
	annotations: readOnly('Get script source')
};

export const scriptSourceUpdateTool = {
	name: 'script_source_update',
	description:
		'Replace a script source\'s text and rebuild: every node using it regenerates. Refused with InvalidScript ' +
		'when the new text does not check. A node the new text newly breaks rolls the text back (rolled_back, with ' +
		'the node\'s error) unless on_error is "keep". Not an undo step: sources are outside the feature-level ' +
		'history, so undo does not restore the previous text.',
	inputSchema: {
		type: 'object',
		properties: {
			source_id: uuid('The script source to replace.'),
			text: { type: 'string', description: 'The new script text.' },
			on_error: onErrorSchema
		},
		required: ['source_id', 'text'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema({ source_id: { type: 'string' }, interface: interfaceSchema }),
	annotations: edit('Update script source')
};

export const scriptFeatureAddTool = {
	name: 'script_feature_add',
	description:
		'Add one custom feature node running a script source, at the end of the tree (one undo step). The node ' +
		'takes the script\'s declared name; feature_edit with a Script operation later changes its args, and ' +
		'feature_get shows its error. args are model units; arg_exprs drive int/number/length/angle parameters ' +
		'from design-parameter expressions instead (mm-space, converted by the parameter\'s type). Equivalent to ' +
		'feature_add with {"type":"Script","params":{source_id, entry, args, arg_exprs}}. A node that fails to ' +
		'build is rolled back by default with the script\'s typed error (stage args | runtime | child …). ' +
		UNITS_NOTE,
	inputSchema: {
		type: 'object',
		properties: {
			source_id: uuid('The script source (from script_source_add / script_source_get).'),
			entry: { type: 'string', description: 'Entry function (default "feature").' },
			args: scriptArgs,
			arg_exprs: {
				type: 'object',
				description: 'Parameter name → expression over design parameters (mm-space), for numeric parameters.',
				additionalProperties: { type: 'string' }
			},
			on_error: onErrorSchema
		},
		required: ['source_id'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema({ feature_id: { type: 'string' } }),
	annotations: edit('Add script feature')
};
