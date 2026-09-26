/**
 * Export and import agent tools (specs/waffle_mcp_server.md §2.5 Export, Q5–Q7;
 * `import_step`). Definitions only; all three are implemented in the engine
 * (`crates/wasm-bridge/src/tools/{export,author}.rs`, S3), and `../export.js`
 * keeps the one half that needs the page: handing a `deliver:"download"` file
 * to the browser.
 */
import { commandOutputSchema, onErrorSchema } from './common.js';

export const importStepTool = {
	name: 'import_step',
	description:
		'Import STEP (ISO 10303-21) text as a new imported-body feature at the identity placement (one undo step, ' +
		'Import provenance), as the Import menu does but without its placement dialog. A step that fails to build is ' +
		'rolled back by default.',
	inputSchema: {
		type: 'object',
		properties: {
			file_name: { type: 'string', minLength: 1, description: 'Name recorded on the feature, e.g. "bracket.step".' },
			step_text: { type: 'string', minLength: 1, description: 'The STEP file contents.' },
			on_error: onErrorSchema
		},
		required: ['file_name', 'step_text'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema({ feature_id: { type: 'string' } }),
	annotations: { title: 'Import STEP', readOnlyHint: false, destructiveHint: false, openWorldHint: false }
};

export const kicadLinkTool = {
	name: 'kicad_link',
	description:
		'Link a KiCad board (`.kicad_pcb` text): the board outline becomes an exact solid in a new Board Part tab ' +
		'(Derived provenance), each footprint shape a placeholder Part, and the footprints a Board assembly tab ' +
		'(one instance per footprint keyed by its uuid in external_key, a mate connector per mounting hole). ' +
		'With `locator` the source is LINKED (git file at the resolved commit); without it, embedded. Opens the ' +
		'Board tab. specs/kicad_board_link.md.',
	inputSchema: {
		type: 'object',
		properties: {
			file_name: { type: 'string', minLength: 1, description: 'Name recorded on the source, e.g. "main.kicad_pcb".' },
			pcb_text: { type: 'string', minLength: 1, description: 'The .kicad_pcb file contents (KiCad 6 or newer).' },
			locator: {
				type: 'object',
				description: 'Where the file lives (v4 Locator: {type:"Git", remote, path, ref, host?} or {type:"Url", url}). Omit for an embedded copy.'
			},
			resolved_commit: { type: 'string', description: 'The commit the text was fetched at (git locators).' },
			on_error: onErrorSchema
		},
		required: ['file_name', 'pcb_text'],
		additionalProperties: false
	},
	outputSchema: commandOutputSchema({
		source_id: { type: 'string' },
		board_tab: { type: 'string' },
		assembly_tab: { type: 'string' },
		placeholder_tabs: { type: 'object', additionalProperties: { type: 'string' }, description: 'footprint name → placeholder Part tab id' },
		board: { type: 'object', description: 'BoardMeta: title, rev, date, company, comments, copper_layers, thickness_m, net_count, footprint_count.' },
		component_count: { type: 'integer' }
	}),
	annotations: { title: 'Link KiCad board', readOnlyHint: false, destructiveHint: false, openWorldHint: false }
};

const nullableObject = { type: ['object', 'null'] };

export const entityMetaTool = {
	name: 'entity_meta',
	description:
		'What a linked KiCad board knows about a body or an assembly instance: the board record (title, rev, layers, ' +
		'thickness, nets) and, for a footprint instance, the component record (reference, value, footprint, datasheet, ' +
		'side, pads with their nets) plus the source it came from. Every field null for anything that derives from ' +
		'no board — not an error.',
	inputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string', description: 'A render body id ("{instance…}/{feature}/{key}" in an assembly, "{feature}/{key}" in a part).' },
			instance_path: { type: 'array', items: { type: 'string' }, description: 'An assembly instance path; the first id is the top-level instance.' }
		},
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: { board: nullableObject, component: nullableObject, source: nullableObject },
		required: ['board', 'component', 'source']
	},
	annotations: { title: 'Board data', readOnlyHint: true, destructiveHint: false, openWorldHint: false }
};

const deliver = {
	type: 'string',
	enum: ['agent', 'download'],
	default: 'agent',
	description:
		'"agent": the file comes back in this result as an embedded resource. "download": the browser downloads it ' +
		'for the user, as the Export menu does, and the result only describes it.'
};

const exportResultSchema = {
	type: 'object',
	properties: {
		deliver: { type: 'string', enum: ['agent', 'download'] },
		file_name: { type: 'string' },
		mime_type: { type: 'string' },
		bytes: { type: 'integer', description: 'Size of the exported file.' },
		warnings: { type: 'array', items: { type: 'string' }, description: 'What the exporter left out, verbatim.' }
	},
	required: ['deliver', 'file_name', 'mime_type', 'bytes', 'warnings']
};

export const exportStepTool = {
	name: 'export_step',
	description:
		'Export the whole model (every live body of the open Part) as an analytic STEP AP214 file. Anything the ' +
		'exporter leaves out is listed verbatim in warnings. Refused with NothingToExport when there are no bodies, and ' +
		'with PayloadTooLarge above 16 MiB for deliver "agent" (use "download").',
	inputSchema: { type: 'object', properties: { deliver }, additionalProperties: false },
	outputSchema: exportResultSchema,
	annotations: { title: 'Export STEP', readOnlyHint: true, openWorldHint: false }
};

export const exportStlTool = {
	name: 'export_stl',
	description:
		'Export the render mesh as a binary STL: one body (body_id from model_summary) or, without body_id, every body ' +
		'merged. Refused with NothingToExport, BodyNotFound, or PayloadTooLarge above 16 MiB for deliver "agent".',
	inputSchema: {
		type: 'object',
		properties: { body_id: { type: 'string', description: 'Body id from model_summary.bodies; omit for all bodies.' }, deliver },
		additionalProperties: false
	},
	outputSchema: exportResultSchema,
	annotations: { title: 'Export STL', readOnlyHint: true, openWorldHint: false }
};
