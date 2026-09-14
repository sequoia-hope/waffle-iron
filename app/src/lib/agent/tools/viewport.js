/**
 * Viewport agent tools (specs/waffle_mcp_server.md §2.5 Viewport, Q4). Definitions
 * only; implementations are in `../viewport.js`.
 */

/** The standard views of the View Cube and the viewport context menu. */
export const VIEWS = ['front', 'back', 'top', 'bottom', 'left', 'right', 'iso'];

const vec3 = { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 };

export const viewportViewTool = {
	name: 'viewport_view',
	description:
		'Point the user\'s 3D view: snap to a standard view and/or fit everything in view, as the View Cube and the F ' +
		'key do. Only the camera moves; the model and the undo history are unchanged. Returns the resulting camera ' +
		'(world meters). Refused with ViewportUnavailable while the viewport is hidden.',
	inputSchema: {
		type: 'object',
		properties: {
			view: { type: 'string', enum: VIEWS, description: 'Standard view to snap to; omit to keep the current direction.' },
			fit: {
				type: 'boolean',
				default: true,
				description: 'Frame all visible bodies (or, with none, the sketches) after snapping.'
			}
		},
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			view: { type: ['string', 'null'] },
			fitted: { type: 'boolean' },
			camera: {
				type: 'object',
				properties: {
					projection: { type: 'string', enum: ['perspective', 'orthographic'] },
					position: vec3,
					target: vec3,
					up: vec3
				},
				required: ['projection', 'position', 'target', 'up']
			}
		},
		required: ['view', 'fitted', 'camera']
	},
	annotations: { title: 'Set view', readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false }
};

export const viewportCaptureTool = {
	name: 'viewport_capture',
	description:
		'A PNG image of the user\'s 3D view as it is now. The camera is not moved: call viewport_view first to frame ' +
		'the model. Refused with ViewportUnavailable while the viewport is hidden.',
	inputSchema: {
		type: 'object',
		properties: {
			max_edge_px: {
				type: 'integer',
				minimum: 64,
				maximum: 4096,
				default: 1024,
				description: 'Longest image edge in pixels; the view is scaled down (never up) to fit.'
			}
		},
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			mime_type: { type: 'string', enum: ['image/png'] },
			width: { type: 'integer' },
			height: { type: 'integer' }
		},
		required: ['mime_type', 'width', 'height']
	},
	annotations: { title: 'Capture viewport', readOnlyHint: true }
};
