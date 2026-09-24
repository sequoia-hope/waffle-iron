/**
 * Viewport tool implementations (specs/waffle_mcp_server.md §2.5 `viewport_view`,
 * `viewport_capture`, Q4). The camera and the renderer live in the viewport's
 * Threlte components, which the tools reach through synchronous window events:
 * the component fills in the event's `detail` (CameraControls answers
 * 'waffle-agent-view', AgentCapture answers 'waffle-agent-capture'). A detail left
 * unfilled means no viewport is mounted. Neither tool changes the model.
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

/** @type {Record<string, { run: (args: any) => any }>} */
export const VIEWPORT_QUERIES = {
	viewport_view: {
		run: ({ view, fit = true, frame = null }) => {
			// The schema cannot say "one of body_ids / point, and radius with
			// point" — check it here rather than framing something arbitrary.
			if (frame) {
				const hasBodies = Array.isArray(frame.body_ids) && frame.body_ids.length > 0;
				const hasPoint = Array.isArray(frame.point);
				if (hasBodies === hasPoint) {
					throw fail('InvalidArguments', 'frame takes either body_ids or point (with radius), not both.', {
						frame
					});
				}
				if (hasPoint && !(frame.radius > 0)) {
					throw fail('InvalidArguments', 'frame.point needs a positive frame.radius (meters).', { frame });
				}
			}
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
		run: ({ max_edge_px = 1024 }) => {
			const detail = dispatch('waffle-agent-capture', { maxEdge: max_edge_px, image: null, unavailable: null });
			if (detail.unavailable) throw fail('ViewportUnavailable', detail.unavailable, { reason: 'empty' });
			const image = /** @type {{ png: string, width: number, height: number } | null} */ (detail.image);
			if (!image) throw fail('ViewportUnavailable', NOT_MOUNTED, { reason: 'not_mounted' });
			const structured = { mime_type: 'image/png', width: image.width, height: image.height };
			return {
				content: [{ type: 'image', data: image.png, mimeType: 'image/png' }],
				structuredContent: structured,
				isError: false
			};
		}
	}
};
