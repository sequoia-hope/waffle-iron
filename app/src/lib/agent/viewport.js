/**
 * Viewport tool implementations (specs/waffle_mcp_server.md §2.5 `viewport_view`,
 * `viewport_capture`, Q4; `viewport_capture`'s arguments extended by
 * specs/agent_mechanical_design.md §9, increment V1). The camera and the
 * renderer live in the viewport's Threlte components, which the tools reach
 * through synchronous window events: the component fills in the event's
 * `detail` (CameraControls answers 'waffle-agent-view', AgentCapture answers
 * 'waffle-agent-capture'). A detail left unfilled means no viewport is mounted.
 * Neither tool changes the model.
 *
 * `viewport_view` MOVES the user's camera. `viewport_capture` never does: every
 * framing argument applies to that one image, rendered in its own offscreen
 * pass, so an agent can look from six directions without disturbing what the
 * user is looking at.
 */
import { fail, toolOk } from './results.js';

const NOT_MOUNTED = 'No 3D viewport is mounted in this tab.';

/**
 * @param {string} name
 * @param {Record<string, unknown>} detail
 */
function dispatch(name, detail) {
	if (document.visibilityState === 'hidden') {
		throw fail('ViewportUnavailable', 'The Waffle Iron tab is in the background, so its viewport does not render.', {
			reason: 'hidden'
		});
	}
	window.dispatchEvent(new CustomEvent(name, { detail }));
	return detail;
}

/**
 * `frame` cannot be expressed as "one of body_ids / point, and radius with
 * point" in JSON Schema — check it here rather than framing something
 * arbitrary. Shared by both tools so they refuse the same shapes.
 * @param {any} frame
 */
function checkFrame(frame) {
	if (!frame) return;
	const hasBodies = Array.isArray(frame.body_ids) && frame.body_ids.length > 0;
	const hasPoint = Array.isArray(frame.point);
	if (hasBodies === hasPoint) {
		throw fail('InvalidArguments', 'frame takes either body_ids or point (with radius), not both.', { frame });
	}
	if (hasPoint && !(frame.radius > 0)) {
		throw fail('InvalidArguments', 'frame.point needs a positive frame.radius (meters).', { frame });
	}
}

/** @type {Record<string, { run: (args: any) => any }>} */
export const VIEWPORT_QUERIES = {
	viewport_view: {
		run: ({ view, fit = true, frame = null }) => {
			checkFrame(frame);
			const detail = dispatch('waffle-agent-view', {
				view: view ?? null,
				fit,
				frame,
				camera: null,
				framed: null,
				missing_body_ids: null
			});
			if (detail.missing_body_ids?.length) {
				throw fail('BodyNotFound', 'No visible body with that id in this view.', {
					body_ids: detail.missing_body_ids
				});
			}
			if (!detail.camera) throw fail('ViewportUnavailable', NOT_MOUNTED, { reason: 'not_mounted' });
			return toolOk({
				view: view ?? null,
				fitted: frame ? true : fit,
				framed: detail.framed ?? null,
				camera: detail.camera
			});
		}
	},

	viewport_capture: {
		run: (args) => {
			const {
				view = null,
				camera = null,
				projection = null,
				frame = null,
				fit = null,
				size = null,
				style = 'shaded',
				color_by = null,
				labels = [],
				isolate = null,
				hide = null,
				max_edge_px = 1024
			} = args ?? {};

			checkFrame(frame);
			// Contradictions are refused, never resolved by precedence: an agent
			// that asked for both a named view and an explicit camera does not
			// know which one it got, and the picture would not say.
			if (camera && view) {
				throw fail('InvalidArguments', 'camera and view both set the direction; pass one.', { view, camera });
			}
			if (camera && (!Array.isArray(camera.position) || !Array.isArray(camera.target))) {
				throw fail('InvalidArguments', 'camera needs position and target (world meters).', { camera });
			}
			if (color_by && style !== 'agent') {
				throw fail(
					'InvalidArguments',
					'color_by needs style:"agent" — a shaded capture is the user\'s shading, so a legend colour would not be in it.',
					{ style, color_by }
				);
			}
			if (isolate?.length && hide?.length) {
				const both = isolate.filter((/** @type {string} */ id) => hide.includes(id));
				if (both.length > 0) {
					throw fail('InvalidArguments', 'isolate and hide name the same body.', { body_ids: both });
				}
			}

			const detail = dispatch('waffle-agent-capture', {
				args: {
					view,
					camera,
					projection,
					frame,
					// A named view or a frame implies the fit that makes it useful;
					// a bare capture keeps the user's framing, as it did before V1.
					fit: fit ?? Boolean(view || frame),
					size,
					style,
					color_by,
					labels,
					isolate,
					hide,
					max_edge_px
				},
				result: null,
				refused: null,
				unavailable: null
			});
			if (detail.refused) {
				const r = /** @type {{code: string, detail: string, details: object}} */ (detail.refused);
				throw fail(r.code, r.detail, r.details);
			}
			if (detail.unavailable) throw fail('ViewportUnavailable', detail.unavailable, { reason: 'empty' });
			const image = /** @type {any} */ (detail.result);
			if (!image) throw fail('ViewportUnavailable', NOT_MOUNTED, { reason: 'not_mounted' });
			const structured = {
				mime_type: 'image/png',
				width: image.width,
				height: image.height,
				size: { width: image.width, height: image.height },
				style: image.style,
				color_by: image.color_by,
				camera: image.camera,
				framed: image.framed,
				legend: image.legend,
				labels: image.labels
			};
			return {
				content: [{ type: 'image', data: image.png, mimeType: 'image/png' }],
				structuredContent: structured,
				isError: false
			};
		}
	}
};
