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
import { CAPTURE_STYLES, COLOR_BY_KINDS, LABEL_KINDS, PROJECTIONS, VIEWS } from './tools/viewport.js';

const NOT_MOUNTED = 'No 3D viewport is mounted in this tab.';

/**
 * Every closed vocabulary and every number is checked HERE, not left to the
 * schema. The relay validates arguments against `inputSchema` before it
 * forwards them, but the in-page executor (`window.__waffleAgentExecutor`, what
 * the in-app agent calls) does not — and an unknown value did not refuse, it
 * drew the wrong picture: `style: "wireframe"` came back as a shaded image
 * reporting `style: "wireframe"`, `labels: ["dimensions"]` drew nothing, and
 * `view: "nope"` threw inside the capture component, where the exception cannot
 * reach the caller (an exception from a `dispatchEvent` listener is reported to
 * the page, not propagated), so the tool blamed a missing viewport.
 * @param {string} name @param {any} value @param {string[]} allowed
 */
function checkEnum(name, value, allowed) {
	if (value == null) return;
	if (typeof value !== 'string' || !allowed.includes(value)) {
		throw fail('InvalidArguments', `${name} must be one of ${allowed.join(', ')}.`, { [name]: value });
	}
}

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
	// A non-numeric point does not refuse further down: it builds a NaN box and
	// then a NaN camera, and the answer is a blank image with a NaN `framed`.
	if (hasPoint && (frame.point.length !== 3 || !frame.point.every((/** @type {any} */ v) => Number.isFinite(v)))) {
		throw fail('InvalidArguments', 'frame.point is three world coordinates in meters.', { frame });
	}
	if (hasBodies && frame.body_ids.some((/** @type {any} */ id) => typeof id !== 'string')) {
		throw fail('InvalidArguments', 'frame.body_ids is a list of body ids.', { frame });
	}
}

/** @type {Record<string, { run: (args: any) => any }>} */
export const VIEWPORT_QUERIES = {
	viewport_view: {
		run: ({ view, fit = true, frame = null }) => {
			checkEnum('view', view, VIEWS);
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

			checkEnum('view', view, VIEWS);
			checkEnum('style', style, CAPTURE_STYLES);
			checkEnum('projection', projection, PROJECTIONS);
			checkEnum('color_by', color_by, COLOR_BY_KINDS);
			if (labels != null) {
				if (!Array.isArray(labels)) {
					throw fail('InvalidArguments', 'labels is a list of label kinds.', { labels });
				}
				for (const kind of labels) checkEnum('labels', kind, LABEL_KINDS);
			}
			for (const [name, value] of [
				['isolate', isolate],
				['hide', hide]
			]) {
				if (value == null) continue;
				if (!Array.isArray(value) || value.some((id) => typeof id !== 'string')) {
					throw fail('InvalidArguments', `${name} is a list of body ids.`, { [name]: value });
				}
			}
			if (size != null) {
				const positive = (/** @type {any} */ v) => Number.isFinite(v) && v > 0;
				if (typeof size !== 'object' || !positive(size.width) || !positive(size.height)) {
					throw fail('InvalidArguments', 'size needs a positive width and height in pixels.', { size });
				}
			}
			if (!Number.isFinite(max_edge_px) || max_edge_px <= 0) {
				throw fail('InvalidArguments', 'max_edge_px is a pixel count.', { max_edge_px });
			}
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
