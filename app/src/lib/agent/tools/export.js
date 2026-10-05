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
		'With `locator` the source is LINKED (git file at the resolved commit); without it, embedded. With ' +
		'`step_text` (the board\'s own STEP export, `kicad-cli pcb export step`) each footprint the STEP models ' +
		'becomes an instance of that product placed as KiCad placed it; the rest stay placeholders, loudly. ' +
		'Opens the Board tab. specs/kicad_board_link.md.',
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
			step_text: { type: 'string', description: 'The board STEP export (ISO 10303-21 text) beside the board: its per-footprint products become the component models.' },
			step_file_name: { type: 'string', description: 'Name recorded on the STEP source, e.g. "main.step".' },
			step_locator: { type: 'object', description: 'Where the STEP lives (a v4 Locator); omit for an embedded copy.' },
			step_resolved_commit: { type: 'string', description: 'The commit the STEP was fetched at (git locators).' },
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
		component_count: { type: 'integer' },
		board_step_source_id: { type: ['string', 'null'], description: 'The Step source supplying component models, when a readable STEP was given.' }
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

export const exportDxfTool = {
	name: 'export_dxf',
	description:
		'On a Part or Assembly tab: export ONE orthographic view of the whole model (every live body of the open Part, or an open ' +
		"assembly's instances at their world placements) as an R12 DXF drawing in millimetres — the " +
		'flat-pattern file a laser, waterjet or plasma table consumes. Every edge of the model plus every ' +
		"curved face's silhouette — the outline where the surface turns away — is drawn, with hidden-line " +
		'removal: the lines the solid stands in front of land on the HIDDEN layer and the rest on VISIBLE, ' +
		'so switching HIDDEN off leaves a cuttable outline. Both layers are CONTINUOUS — the dashed ' +
		"hidden-line type's dash pitch depends on a sheet scale this one-view export does not have. " +
		'Lines, circles and arcs are written as true DXF ' +
		'entities and a projected ellipse as a polyline within 0.01 mm of it; an intersection or spline ' +
		'curve is a polyline at the render chord density instead, which is about 0.1% of its own radius, so ' +
		'on a large part it is looser than 0.01 mm. Refused with NothingToExport when ' +
		'there are no bodies, InvalidArgument for a view it cannot name, and PayloadTooLarge above 16 MiB for ' +
		'deliver "agent". ' +
		'On a DRAWING tab the sheet is exported instead: every view of it, each at its own scale and ' +
		'position, in one file in sheet millimetres — or one view alone, at the paper origin, with ' +
		'view_id. The projection arguments below describe a view of the MODEL and are refused on a ' +
		'drawing tab (its views carry their own projections); sheet_id and view_id are refused off one. ' +
		'specs/drawings_and_mbd.md D1a + D1b + D1c + D4a.',
	inputSchema: {
		type: 'object',
		properties: {
			view: {
				type: 'string',
				enum: ['top', 'bottom', 'front', 'back', 'right', 'left', 'iso'],
				default: 'top',
				description:
					'Named view: the six orthographic ones, or iso — the isometric from (+1, +1, +1) with +z up, ' +
					"the same table a drawing sheet's view argument uses (D4f). Omit for \"top\", the " +
					'flat-pattern view.'
			},
			direction: {
				type: 'array',
				items: { type: 'number' },
				minItems: 3,
				maxItems: 3,
				description:
					'Direction of SIGHT as [x, y, z], away from the viewer — for an axis no named view covers. ' +
					'Mutually exclusive with view.'
			},
			up: {
				type: 'array',
				items: { type: 'number' },
				minItems: 3,
				maxItems: 3,
				description:
					"Which world direction points up on the paper. Omit to take the named view's own up, or " +
					'to let the engine pick one for a free direction.'
			},
			sheet_id: {
				type: 'string',
				description: 'DRAWING tab only: which sheet. Omit for its first (and usually only) one.'
			},
			view_id: {
				type: 'string',
				description:
					'DRAWING tab only: export just this view, alone and at the paper origin — what a cutting ' +
					'table wants from a sheet it should not read the rest of.'
			},
			deliver
		},
		additionalProperties: false
	},
	outputSchema: exportResultSchema,
	annotations: { title: 'Export DXF', readOnlyHint: true, openWorldHint: false }
};

export const exportSvgTool = {
	name: 'export_svg',
	description:
		'Export the open DRAWING tab\'s sheet as SVG — the same markup the sheet shows on screen, at paper ' +
		'size in millimetres, so the browser\'s print path is true to scale. One SVG user unit is one paper ' +
		'millimetre; dimension text carries the measured value at the annotation\'s precision. Refused with ' +
		'TabKindNotSupported off a Drawing tab and NothingToExport when no view of the sheet rebuilt. ' +
		'specs/drawings_and_mbd.md §8 D4a.',
	inputSchema: {
		type: 'object',
		properties: {
			sheet_id: { type: 'string', description: 'Which sheet. Omit for the first one.' },
			deliver
		},
		additionalProperties: false
	},
	outputSchema: exportResultSchema,
	annotations: { title: 'Export SVG', readOnlyHint: true, openWorldHint: false }
};

export const exportPdfTool = {
	name: 'export_pdf',
	description:
		"Export the open DRAWING tab's sheet as a one-page PDF at paper size — the printable deliverable. " +
		'It is written from the same markup export_svg returns, so it is the drawing on the screen rather ' +
		'than a second rendering of it, and it is true to scale: a 1:1 view measures its real size on the ' +
		'printed page. Ink is BLACK ON WHITE whatever the UI theme, because a sheet is paper. Text is ' +
		'Helvetica (a base-14 font, so nothing is embedded) and a diameter sign prints as Ø, which is ' +
		'reported in warnings. Answers with bytes and pages; the file rides back base64 for deliver ' +
		'"agent". Refused with TabKindNotSupported off a Drawing tab, NothingToExport when no view of the ' +
		'sheet rebuilt, and PayloadTooLarge above the 16 MiB inline limit. specs/drawings_and_mbd.md §8 D4b.',
	inputSchema: {
		type: 'object',
		properties: {
			sheet_id: { type: 'string', description: 'Which sheet. Omit for the first one.' },
			deliver
		},
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			...exportResultSchema.properties,
			pages: { type: 'integer', description: 'Pages written. One sheet is one page.' }
		},
		required: exportResultSchema.required
	},
	annotations: { title: 'Export PDF', readOnlyHint: true, openWorldHint: false }
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
