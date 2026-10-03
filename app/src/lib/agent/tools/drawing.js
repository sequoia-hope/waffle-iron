/**
 * The drawing tools' MCP definitions (`specs/drawings_and_mbd.md` §8, D4a):
 * `drawing_view_add`, `drawing_view_edit`, `drawing_annotation_add`.
 *
 * All three run in the engine (`crates/wasm-bridge/src/tools/drawing.rs`) and
 * all three need a Drawing tab active, which is the mirror of the feature
 * tools' Part-tab gate and the assembly tools' Assembly-tab one.
 *
 * Note what is NOT here: a dimension has no `value` argument. Its number is
 * measured from the model on every rebuild; a drawing whose dimension was
 * typed in is the failure the whole increment exists to prevent, so the
 * authoring door does not offer one.
 */

const NAMED_VIEWS = ['front', 'back', 'left', 'right', 'top', 'bottom', 'iso'];
const PROJECTED_DIRECTIONS = ['left', 'right', 'up', 'down'];
const ANNOTATIONS = ['Dimension', 'Note', 'CentreMark', 'CentreLine', 'Datum'];
const DIMENSION_KINDS = [
	'Distance',
	'PointLineDistance',
	'HDistance',
	'VDistance',
	'Angle',
	'Radius',
	'Diameter'
];

/** The evaluated drawing every one of these tools answers with. */
const drawingStateSchema = {
	type: 'object',
	properties: {
		tab_id: { type: 'string' },
		projection_angle: { type: 'object', description: 'Third or First angle.' },
		sheets: {
			type: 'array',
			description:
				'Each sheet with its size and its views (ids, projections, scales, placements, how many ' +
				'curves each drew and how many anchors it offers — anchor_list with include_anchors).',
			items: { type: 'object' }
		},
		declines: {
			type: 'object',
			description:
				'What the projections declined to decide, by counter name (D1c). Every counter but ' +
				'cross_body means a line the drawing does not carry, so a large count is a degenerate ' +
				'view rather than a clean one.'
		},
		errors: { type: 'array', items: { type: 'string' }, description: 'Per-view failures, each naming its view.' },
		warnings: { type: 'array', items: { type: 'string' } }
	},
	required: ['tab_id', 'sheets', 'errors', 'warnings']
};

export const drawingViewAddTool = {
	name: 'drawing_view_add',
	description:
		'Add a view of a Part or Assembly tab to the open DRAWING tab\'s sheet. The view is a projection ' +
		'direction plus a position on the paper: give view for a named direction (front, top, iso, …), ' +
		'direction+up for one nothing names, or parent_view_id+direction_from_parent for a view projected ' +
		'from another — which follows the document\'s projection standard (third angle by default, so the ' +
		'view placed to the right of its parent shows the right-hand side). Hidden lines and curved faces\' ' +
		'silhouettes are drawn by default. Answers with the whole evaluated drawing, which is where the ' +
		'view ids for the other tools come from. Refused with TabKindNotSupported off a Drawing tab, ' +
		'TabNotFound for a tab the document does not have, and InvalidArgument for a scale that is not a ' +
		'positive ratio or more than one way of naming the projection. specs/drawings_and_mbd.md §8 D4a.',
	inputSchema: {
		type: 'object',
		properties: {
			tab_id: { type: 'string', description: 'The Part or Assembly tab this view draws.' },
			bodies: {
				type: 'array',
				items: { type: 'string' },
				description:
					"Which of that tab's bodies, by the names model_summary reports. Omit for every live body."
			},
			view: { type: 'string', enum: NAMED_VIEWS, default: 'front', description: 'A named direction.' },
			direction: {
				type: 'array',
				items: { type: 'number' },
				minItems: 3,
				maxItems: 3,
				description: 'Direction of SIGHT as [x, y, z], away from the viewer. Mutually exclusive with view.'
			},
			up: {
				type: 'array',
				items: { type: 'number' },
				minItems: 3,
				maxItems: 3,
				description: 'Which world direction points up on the paper, for a free direction.'
			},
			parent_view_id: {
				type: 'string',
				description:
					'Project from this view of the same sheet instead of naming a direction. Mutually ' +
					'exclusive with view and direction.'
			},
			direction_from_parent: {
				type: 'string',
				enum: PROJECTED_DIRECTIONS,
				default: 'right',
				description:
					'Where the projected view sits from its parent ON PAPER. What it SHOWS follows from the ' +
					"document's projection standard."
			},
			sheet_id: { type: 'string', description: 'Which sheet. Omit for the first one.' },
			name: { type: 'string', description: "The view's label. Omit for the projection's own name." },
			scale: {
				type: 'number',
				exclusiveMinimum: 0,
				default: 1,
				description: 'Paper length per model length: 1 is 1:1, 0.1 is 1:10.'
			},
			placement_mm: {
				type: 'array',
				items: { type: 'number' },
				minItems: 2,
				maxItems: 2,
				description:
					"Where the view's drawn CENTRE sits on the sheet, millimetres from the bottom-left " +
					'corner. Omit to have it placed clear of its parent (a projected view) or in the middle ' +
					'of the sheet.'
			},
			include_anchors: {
				type: 'boolean',
				default: false,
				description:
					'Also list, per view, the PERSISTENT IDS an annotation can anchor on (anchor_list): ' +
					'every entity the view drew, each with its shape on the drawing (Point, Line, ' +
					'Circle, Ellipse, Polyline), its witness point in view-plane meters and its radius ' +
					'where it has one. The shape is what tells a wall from a corner — picking two ' +
					'anchors by position alone can take two corners and measure the diagonal. Each pid ' +
					'is a decimal STRING, because a persistent id is a 64-bit number and JSON numbers ' +
					'in JavaScript are not exact above 2^53; pass it back verbatim. Omitted by default ' +
					'— a real part\'s view has thousands of edges — but it is where the anchors for ' +
					'drawing_annotation_add come from.'
			},
		},
		required: ['tab_id'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: { view_id: { type: 'string' }, ...drawingStateSchema.properties },
		required: ['view_id', ...drawingStateSchema.required]
	},
	annotations: { title: 'Add drawing view', readOnlyHint: false, destructiveHint: false, openWorldHint: false }
};

export const drawingViewEditTool = {
	name: 'drawing_view_edit',
	description:
		"Change one view of the open DRAWING tab: its name, scale, position on the sheet, which bodies it " +
		'draws, and whether it draws hidden lines and silhouettes. Every argument but view_id is optional ' +
		'and only what is given changes. Refused with NotFound for a view the drawing does not have and ' +
		'InvalidArgument for a scale that is not a positive ratio. specs/drawings_and_mbd.md §8 D4a.',
	inputSchema: {
		type: 'object',
		properties: {
			view_id: { type: 'string' },
			name: { type: 'string' },
			scale: { type: 'number', exclusiveMinimum: 0 },
			placement_mm: { type: 'array', items: { type: 'number' }, minItems: 2, maxItems: 2 },
			bodies: { type: 'array', items: { type: 'string' } },
			hidden_lines: { type: 'boolean', description: 'Draw the edges the part hides (D1c).' },
			silhouettes: { type: 'boolean', description: "Draw curved faces' outlines (D1b)." },
			include_anchors: {
				type: 'boolean',
				default: false,
				description:
					'Also list, per view, the PERSISTENT IDS an annotation can anchor on (anchor_list): ' +
					'every entity the view drew, each with its shape on the drawing (Point, Line, ' +
					'Circle, Ellipse, Polyline), its witness point in view-plane meters and its radius ' +
					'where it has one. The shape is what tells a wall from a corner — picking two ' +
					'anchors by position alone can take two corners and measure the diagonal. Each pid ' +
					'is a decimal STRING, because a persistent id is a 64-bit number and JSON numbers ' +
					'in JavaScript are not exact above 2^53; pass it back verbatim. Omitted by default ' +
					'— a real part\'s view has thousands of edges — but it is where the anchors for ' +
					'drawing_annotation_add come from.'
			},
		},
		required: ['view_id'],
		additionalProperties: false
	},
	outputSchema: drawingStateSchema,
	annotations: { title: 'Edit drawing view', readOnlyHint: false, destructiveHint: false, openWorldHint: false }
};

export const drawingAnnotationAddTool = {
	name: 'drawing_annotation_add',
	description:
		'Add an annotation to one view of the open DRAWING tab: a Dimension, a Note, a CentreMark, a ' +
		'CentreLine or a Datum. A dimension takes NO value — its number is measured from the model on ' +
		'every rebuild, which is what keeps a drawing from disagreeing with the part it is of. Anchors are ' +
		'PERSISTENT ids of the entities measured (edges, faces, vertices), so an annotation never ' +
		'silently rebinds to a different edge; an anchor whose entity this view does not draw is refused ' +
		'with AnnotationNotMeasurable and nothing is added. Refused with NotFound for a view the drawing ' +
		'does not have and InvalidArgument for the wrong number of anchors for the kind. ' +
		'specs/drawings_and_mbd.md §7 + §8 D4a.',
	inputSchema: {
		type: 'object',
		properties: {
			view_id: { type: 'string' },
			annotation: { type: 'string', enum: ANNOTATIONS, default: 'Dimension' },
			kind: {
				type: 'string',
				enum: DIMENSION_KINDS,
				default: 'Distance',
				description:
					'Dimension only. Distance is an ALIGNED dimension (between two parallel edges, or ' +
					'between two points); HDistance and VDistance are the horizontal and vertical ' +
					'components; Angle is in degrees when printed and measures the ACUTE angle between two ' +
					'straight edges.'
			},
			anchors: {
				type: 'array',
				description:
					'The entities measured, in the order the kind expects: 1 for Radius, Diameter, ' +
					'CentreMark and Datum, 2 for the distances, Angle and CentreLine. Each is a ' +
					'persistent id — a number or a decimal STRING (use the string: an id above 2^53 ' +
					'is not exact as a JSON number) meaning an edge — or {pid, kind} with kind Edge, ' +
					'Face or Vertex. The ids come from anchor_list on drawing_view_add / _edit with ' +
					'include_anchors.',
				items: {
					oneOf: [
						{ type: 'integer', minimum: 0 },
						{ type: 'string', pattern: '^[0-9]+$' },
						{
							type: 'object',
							properties: {
								pid: {
									oneOf: [{ type: 'integer', minimum: 0 }, { type: 'string', pattern: '^[0-9]+$' }]
								},
								kind: { type: 'string', enum: ['Edge', 'Face', 'Vertex'], default: 'Edge' }
							},
							required: ['pid'],
							additionalProperties: false
						}
					]
				}
			},
			text: { type: 'string', description: 'Note only: the text.' },
			label: { type: 'string', description: 'Datum only: the letter (A, B, …).' },
			precision: {
				type: 'integer',
				minimum: 0,
				maximum: 9,
				description: 'Decimal places shown. Omit for the document setting.'
			},
			dual_unit: {
				type: 'string',
				description: 'A second unit shown in brackets beneath the primary one ("in", "mm", …).'
			},
			placement: {
				type: 'array',
				items: { type: 'number' },
				minItems: 2,
				maxItems: 2,
				description: "A cosmetic offset of the whole annotation from where the layout puts it, in the view's own units (meters)."
			}
		},
		required: ['view_id'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: { annotation_index: { type: 'integer' }, ...drawingStateSchema.properties },
		required: ['annotation_index', ...drawingStateSchema.required]
	},
	annotations: {
		title: 'Add drawing annotation',
		readOnlyHint: false,
		destructiveHint: false,
		openWorldHint: false
	}
};
