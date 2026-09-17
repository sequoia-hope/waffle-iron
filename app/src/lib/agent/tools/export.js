/**
 * Export and import agent tools (specs/waffle_mcp_server.md §2.5 Export, Q5–Q7;
 * `import_step`). Definitions only; the export queries are implemented in
 * `../export.js` (their `deliver:"download"` half needs the page) and
 * `import_step` in the engine (`crates/wasm-bridge/src/tools/author.rs`, S3).
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
