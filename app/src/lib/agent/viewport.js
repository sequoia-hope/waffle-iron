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
		run: ({ view, fit = true }) => {
			const detail = dispatch('waffle-agent-view', { view: view ?? null, fit, camera: null });
			if (!detail.camera) throw fail('ViewportUnavailable', NOT_MOUNTED, { reason: 'not_mounted' });
			return toolOk({ view: view ?? null, fitted: fit, camera: detail.camera });
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
