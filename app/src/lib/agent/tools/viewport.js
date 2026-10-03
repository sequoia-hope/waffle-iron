/**
 * Viewport agent tools (specs/waffle_mcp_server.md §2.5 Viewport, Q4). Definitions
 * only; implementations are in `../viewport.js`.
 */

/** The standard views of the View Cube and the viewport context menu. */
export const VIEWS = ['front', 'back', 'top', 'bottom', 'left', 'right', 'iso'];

/** Label overlays V1 draws (`specs/agent_mechanical_design.md` §9.1). */
export const LABEL_KINDS = ['body_names', 'face_ids'];

/**
 * The capture's remaining closed vocabularies. Named here, beside the schema
 * that publishes them, and imported by `../viewport.js` to refuse anything
 * else: the relay validates arguments against this schema, but the in-page
 * executor does not, so the implementation has to check the same lists or an
 * unknown value gets a picture it did not ask for.
 */
export const CAPTURE_STYLES = ['shaded', 'agent'];
export const PROJECTIONS = ['perspective', 'orthographic'];
export const COLOR_BY_KINDS = ['body', 'feature'];

const vec3 = { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 };

export const viewportViewTool = {
	name: 'viewport_view',
	description:
		'Point the user\'s 3D view: snap to a standard view and fit either everything (as the View Cube and the F ' +
		'key do) or the region `frame` names — some bodies, or a radius about a point — which is how you inspect a ' +
		'detail of a large model. Standard views are in MODEL space, where up is +Z: "front" is an elevation along ' +
		'+Y, "top" looks down, "iso" is the front-right-top three-quarter. Only the camera moves; the model and the ' +
		'undo history are unchanged. Returns the resulting camera (world meters). Refused with ViewportUnavailable ' +
		'while the viewport is hidden.',
	inputSchema: {
		type: 'object',
		properties: {
			view: { type: 'string', enum: VIEWS, description: 'Standard view to snap to; omit to keep the current direction.' },
			fit: {
				type: 'boolean',
				default: true,
				description: 'Frame all visible bodies (or, with none, the sketches) after snapping. Ignored when `frame` is given.'
			},
			frame: {
				type: 'object',
				description:
					'Frame a REGION instead of the whole model: either `body_ids` (the union of those bodies\' boxes) ' +
					'or `point` + `radius` (a cube of half-size `radius` about that world point). Not both.',
				properties: {
					body_ids: {
						type: 'array',
						items: { type: 'string' },
						minItems: 1,
						description: 'Body ids from model_summary.bodies.'
					},
					point: { ...vec3, description: 'World point in meters.' },
					radius: {
						type: 'number',
						exclusiveMinimum: 0,
						description: 'Half-size in meters of the region framed about `point`. Required with `point`.'
					}
				},
				additionalProperties: false
			}
		},
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			view: { type: ['string', 'null'] },
			fitted: { type: 'boolean' },
			framed: {
				type: ['object', 'null'],
				description: 'The box actually framed when `frame` was given, else null.',
				properties: { min: vec3, max: vec3 },
				required: ['min', 'max']
			},
			camera: {
				type: 'object',
				properties: {
					projection: { type: 'string', enum: PROJECTIONS },
					position: vec3,
					target: vec3,
					up: vec3
				},
				required: ['projection', 'position', 'target', 'up']
			}
		},
		required: ['view', 'fitted', 'framed', 'camera']
	},
	annotations: { title: 'Set view', readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false }
};

export const viewportCaptureTool = {
	name: 'viewport_capture',
	description:
		'A PNG image of the 3D model, rendered in its OWN offscreen pass: every argument below applies to this image ' +
		'only and the user\'s camera, selection and body visibility are left exactly as they were. With no arguments ' +
		'it is a picture of the view as it is now. `view`/`camera`/`frame`/`fit` frame it (the `viewport_view` ' +
		'vocabulary, in MODEL space where up is +Z), `size` fixes the output pixels, and `style:"agent"` draws flat ' +
		'per-body colours with black edges on a white ground — no theme, no lighting, no hover or selection ' +
		'highlight. The answer carries `legend` (which colour is which body or feature) and `labels` (the text drawn ' +
		'into the image, each with its pixel position and the reference it names), so read those rather than the ' +
		'pixels to map the picture back to ids you can query. Refused with ViewportUnavailable while the viewport is ' +
		'hidden, and with BodyNotFound when `frame`/`isolate`/`hide` names a body this view does not draw.',
	inputSchema: {
		type: 'object',
		properties: {
			view: { type: 'string', enum: VIEWS, description: 'Standard view to look from; omit to keep the current direction.' },
			camera: {
				type: 'object',
				description: 'An explicit camera in world meters, instead of `view`. Not both.',
				properties: {
					position: { ...vec3, description: 'Eye point, world meters.' },
					target: { ...vec3, description: 'Point looked at, world meters.' },
					up: { ...vec3, description: 'Up direction; defaults to the viewport\'s current up.' }
				},
				required: ['position', 'target'],
				additionalProperties: false
			},
			projection: {
				type: 'string',
				enum: PROJECTIONS,
				description: 'Projection for this image; defaults to the viewport\'s current one.'
			},
			fit: {
				type: 'boolean',
				description:
					'Frame all visible bodies (or, with none, the sketches). Defaults to true when `view` or `frame` is ' +
					'given and false otherwise, so a bare capture keeps the user\'s framing.'
			},
			frame: {
				type: 'object',
				description:
					'Frame a REGION instead of the whole model: either `body_ids` (the union of those bodies\' boxes) ' +
					'or `point` + `radius` (a cube of half-size `radius` about that world point). Not both.',
				properties: {
					body_ids: { type: 'array', items: { type: 'string' }, minItems: 1, description: 'Body ids from model_summary.bodies.' },
					point: { ...vec3, description: 'World point in meters.' },
					radius: { type: 'number', exclusiveMinimum: 0, description: 'Half-size in meters of the region framed about `point`.' }
				},
				additionalProperties: false
			},
			size: {
				type: 'object',
				description: 'Exact output size in pixels. Without it the viewport\'s aspect is kept and scaled to `max_edge_px`.',
				properties: {
					width: { type: 'integer', minimum: 64, maximum: 4096 },
					height: { type: 'integer', minimum: 64, maximum: 4096 }
				},
				required: ['width', 'height'],
				additionalProperties: false
			},
			max_edge_px: {
				type: 'integer',
				minimum: 64,
				maximum: 4096,
				default: 1024,
				description: 'Longest image edge in pixels when `size` is absent; the view is scaled down (never up) to fit.'
			},
			style: {
				type: 'string',
				enum: CAPTURE_STYLES,
				default: 'shaded',
				description:
					'"shaded" is the user\'s shading and theme. "agent" is flat unlit per-body colours, black edges, ' +
					'enlarged vertices, a white ground and no antialiasing — the one to read mechanically.'
			},
			color_by: {
				type: 'string',
				enum: COLOR_BY_KINDS,
				description:
					'What the flat colours mean; needs style:"agent" (defaults to "body" there). Colours come from a ' +
					'fixed palette of 24, assigned in sorted-id order, and are reported in `legend`.'
			},
			labels: {
				type: 'array',
				items: { type: 'string', enum: LABEL_KINDS },
				description:
					'Text overlays drawn into the image with leader lines. Each one also comes back in `labels` with its ' +
					'pixel position and the reference it names. A label the camera cannot see, or one that would overlap ' +
					'a label already placed, is drawn in neither the image nor the array.'
			},
			isolate: {
				type: 'array',
				items: { type: 'string' },
				minItems: 1,
				description: 'Show only these bodies in this image. The user\'s own visibility is unchanged.'
			},
			hide: {
				type: 'array',
				items: { type: 'string' },
				minItems: 1,
				description: 'Hide these bodies in this image. The user\'s own visibility is unchanged.'
			}
		},
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			mime_type: { type: 'string', enum: ['image/png'] },
			width: { type: 'integer' },
			height: { type: 'integer' },
			size: {
				type: 'object',
				properties: { width: { type: 'integer' }, height: { type: 'integer' } },
				required: ['width', 'height']
			},
			style: { type: 'string', enum: CAPTURE_STYLES },
			color_by: { type: ['string', 'null'], enum: [...COLOR_BY_KINDS, null] },
			camera: {
				type: 'object',
				description: 'The camera this image was rendered through — not the user\'s, which did not move.',
				properties: {
					projection: { type: 'string', enum: PROJECTIONS },
					position: vec3,
					target: vec3,
					up: vec3
				},
				required: ['projection', 'position', 'target', 'up']
			},
			framed: {
				type: ['object', 'null'],
				description: 'The box actually framed when `frame` or `fit` applied, else null.',
				properties: { min: vec3, max: vec3 },
				required: ['min', 'max']
			},
			legend: {
				type: 'array',
				description: 'One entry per colour in the image.',
				items: {
					type: 'object',
					properties: {
						color: { type: 'string', description: 'CSS hex, as drawn.' },
						kind: { type: 'string', enum: COLOR_BY_KINDS },
						id: { type: 'string' },
						name: { type: ['string', 'null'] }
					},
					required: ['color', 'kind', 'id', 'name']
				}
			},
			labels: {
				type: 'array',
				description: 'The text drawn into the image, each at its anchor in image pixels (top-left origin).',
				items: {
					type: 'object',
					properties: {
						kind: { type: 'string', enum: LABEL_KINDS },
						text: { type: 'string', description: 'Exactly the characters drawn.' },
						x: { type: 'number' },
						y: { type: 'number' },
						body_id: { type: ['string', 'null'] },
						ref: {
							description: 'What the label names: a GeomRef for a face, `{body_id}` for a body.',
							type: ['object', 'null']
						}
					},
					required: ['kind', 'text', 'x', 'y', 'body_id', 'ref']
				}
			}
		},
		required: ['mime_type', 'width', 'height', 'size', 'style', 'color_by', 'camera', 'legend', 'labels']
	},
	annotations: { title: 'Capture viewport', readOnlyHint: true }
};
