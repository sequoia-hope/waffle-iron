/**
 * `viewport_capture`'s agent-legible views (specs/agent_mechanical_design.md §9,
 * increment V1): the capture arguments, the offscreen pass, the legend and the
 * body/face labels.
 *
 * The oracle for the image is NOT the pixels. V1's contract is that the legend
 * and the labels come back BESIDE the PNG, so an agent (and this test) can read
 * what is in the picture without OCR: the size is asserted on the PNG's own
 * IHDR header as well as on `structuredContent`, and everything else is
 * asserted on those two arrays.
 *
 * The capture runs through the page's own executor (`window.__waffleAgentExecutor`),
 * which is the same entry point the relay calls — no relay process needed, and
 * the viewport, store and tool layer are all the real ones.
 *
 * §9.3's byte-identical determinism oracle is increment V3 and is not asserted
 * here; what IS asserted is the property it rests on — two captures with the
 * same arguments describe the same image, and neither moves the user's camera.
 */
import { test, expect } from '@playwright/test';
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';
import { PALETTE } from '../../src/lib/viewport/capture.js';

/** A 40 mm square centred on (cx, 0) in the sketch plane. */
const SQUARE = (cx, s = 0.04) => [
	{ type: 'Point', id: 1, x: cx - s / 2, y: -s / 2 },
	{ type: 'Point', id: 2, x: cx + s / 2, y: -s / 2 },
	{ type: 'Point', id: 3, x: cx + s / 2, y: s / 2 },
	{ type: 'Point', id: 4, x: cx - s / 2, y: s / 2 },
	{ type: 'Line', id: 5, start_id: 1, end_id: 2 },
	{ type: 'Line', id: 6, start_id: 2, end_id: 3 },
	{ type: 'Line', id: 7, start_id: 3, end_id: 4 },
	{ type: 'Line', id: 8, start_id: 4, end_id: 1 }
];

const XY = { origin: [0, 0, 0], normal: [0, 0, 1] };

/** Width and height out of a base64 PNG's IHDR — the image's own account of its size. */
function pngSize(base64) {
	const buf = Buffer.from(base64, 'base64');
	expect(buf.subarray(0, 8).toString('hex')).toBe('89504e470d0a1a0a');
	expect(buf.subarray(12, 16).toString('ascii')).toBe('IHDR');
	return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
}

/**
 * The largest edge of a framed box. Which world axis the sketch's own x lands
 * on is the sketch plane's business, so the extent is asserted, not the axis.
 * @param {{min: number[], max: number[]}} box
 */
function widest(box) {
	return Math.max(...box.max.map((m, i) => m - box.min[i]));
}

/**
 * Two 20 mm-tall boxes, 100 mm apart so they cannot be auto-unioned, and the
 * executor hook ready. Returns the body ids in the order the engine reports.
 * @param {import('@playwright/test').Page} page
 */
async function twoBoxes(page) {
	await page.goto('/');
	await page.waitForFunction(() => window.__waffle?.getState()?.engineReady === true, null, { timeout: 60000 });
	await page.waitForFunction(() => typeof window.__waffleAgentExecutor?.executeTool === 'function', null, {
		timeout: 20000
	});
	return page.evaluate(
		async ({ squares }) => {
			const api = window.__waffleAgentExecutor;
			const ctx = {
				agentName: 'capture-views-test',
				isPaused: () => false,
				pause: () => {},
				isCancelled: () => false
			};
			const call = async (tool, args) => {
				const r = await api.executeTool(tool, args, ctx);
				if (r.isError) throw new Error(`${tool}: ${JSON.stringify(r.structuredContent)}`);
				return r.structuredContent;
			};
			for (const entities of squares) {
				const sketch = await call('sketch_create', { plane: { origin: [0, 0, 0], normal: [0, 0, 1] }, entities });
				await call('feature_add', {
					operation: {
						type: 'Extrude',
						params: {
							sketch_id: sketch.feature_id,
							profile_index: 0,
							profile_entity_ids: [5, 6, 7, 8],
							depth: 0.02,
							symmetric: false,
							cut: false
						}
					}
				});
			}
			const summary = await call('model_summary', {});
			return summary.bodies.map((b) => ({ id: b.body_id, name: b.name ?? null }));
		},
		{ squares: [SQUARE(0), SQUARE(0.1)] }
	);
}

/**
 * The live camera once it has STOPPED moving: two identical samples in a row.
 *
 * The assertion downstream is exact — the capture must not move the user's
 * camera by so much as a float — and sampling before the view has settled made
 * it a question about the app's own damping instead, which fails whenever the
 * machine is loaded (measured: green at one Playwright worker, red at two).
 * Waiting for rest keeps the assertion exact and makes it about the capture.
 * @param {import('@playwright/test').Page} page
 */
async function cameraAtRest(page) {
	await page.waitForFunction(
		() => {
			const s = window.__waffle.getCameraState();
			const key = JSON.stringify([s.position, s.target, s.up, s.zoom]);
			const previous = window.__captureRestKey;
			window.__captureRestKey = key;
			return previous === key;
		},
		null,
		{ timeout: 15000, polling: 150 }
	);
	return page.evaluate(() => window.__waffle.getCameraState());
}

/**
 * One `viewport_capture` through the page's executor.
 * @param {import('@playwright/test').Page} page
 * @param {object} args
 */
function capture(page, args) {
	return page.evaluate(async (args) => {
		const api = window.__waffleAgentExecutor;
		const ctx = {
			agentName: 'capture-views-test',
			isPaused: () => false,
			pause: () => {},
			isCancelled: () => false
		};
		const r = await api.executeTool('viewport_capture', args, ctx);
		return {
			isError: r.isError,
			structured: r.structuredContent,
			png: r.content?.[0]?.data ?? null,
			mime: r.content?.[0]?.mimeType ?? null
		};
	}, args);
}

test.describe('viewport_capture — agent-legible views (V1)', () => {
	test('size, legend and body labels come back beside the PNG', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const bodies = await twoBoxes(page);
		expect(bodies).toHaveLength(2);

		const shot = await capture(page, {
			view: 'iso',
			size: { width: 320, height: 240 },
			style: 'agent',
			labels: ['body_names']
		});
		expect(shot.isError, JSON.stringify(shot.structured)).toBe(false);

		// The size is the one asked for, in the answer AND in the image itself —
		// not the live canvas's 1280x720 scaled to some longest edge.
		expect(shot.structured.width).toBe(320);
		expect(shot.structured.height).toBe(240);
		expect(shot.structured.size).toEqual({ width: 320, height: 240 });
		expect(shot.mime).toBe('image/png');
		expect(pngSize(shot.png)).toEqual({ width: 320, height: 240 });

		// The legend names a colour per body, in sorted-id order, from the
		// fixed palette: every colour in the image maps back to a body id.
		expect(shot.structured.style).toBe('agent');
		expect(shot.structured.color_by).toBe('body');
		const ids = bodies.map((b) => b.id).sort();
		expect(shot.structured.legend.map((l) => l.id)).toEqual(ids);
		for (const entry of shot.structured.legend) {
			expect(entry.kind).toBe('body');
			// From the fixed table, not merely hex-shaped: the palette is the
			// contract an agent matches a pixel against.
			expect(PALETTE).toContain(entry.color);
		}
		expect(new Set(shot.structured.legend.map((l) => l.color)).size).toBe(2);

		// Both bodies are labelled, each at a pixel position inside the image,
		// with the name the body carries and the id it belongs to.
		const labels = shot.structured.labels;
		expect(labels.map((l) => l.body_id).sort()).toEqual(ids);
		for (const label of labels) {
			expect(label.kind).toBe('body_names');
			expect(label.text.length).toBeGreaterThan(0);
			expect(label.ref).toEqual({ body_id: label.body_id });
			expect(label.x).toBeGreaterThanOrEqual(0);
			expect(label.x).toBeLessThanOrEqual(320);
			expect(label.y).toBeGreaterThanOrEqual(0);
			expect(label.y).toBeLessThanOrEqual(240);
		}
		const named = bodies.find((b) => b.id === labels[0].body_id);
		if (named?.name) expect(labels[0].text).toBe(named.name);
		expectNoAnyCrash(crashes);

		// The capture framed the model: `iso` implies the fit, and the framed box
		// spans both boxes — 40 mm wide, 100 mm apart — and is 20 mm tall.
		expect(shot.structured.framed).not.toBeNull();
		expect(shot.structured.framed.max[2]).toBeCloseTo(0.02, 3);
		expect(widest(shot.structured.framed)).toBeCloseTo(0.14, 3);

		// Without `projection` the capture keeps the viewport's own (the app
		// opens orthographic); with it, this image alone switches.
		const live = await page.evaluate(() => window.__waffle.getCameraProjection());
		expect(shot.structured.camera.projection).toBe(live);
		const persp = await capture(page, {
			view: 'iso',
			size: { width: 320, height: 240 },
			projection: 'perspective'
		});
		expect(persp.isError, JSON.stringify(persp.structured)).toBe(false);
		expect(persp.structured.camera.projection).toBe('perspective');
		expect(await page.evaluate(() => window.__waffle.getCameraProjection())).toBe(live);
	});

	test('a face label names a queryable GeomRef, and only the faces the camera sees are drawn', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const bodies = await twoBoxes(page);
		const target = bodies[0].id;

		const shot = await capture(page, {
			view: 'top',
			size: { width: 400, height: 400 },
			style: 'agent',
			labels: ['face_ids'],
			isolate: [target]
		});
		expect(shot.isError, JSON.stringify(shot.structured)).toBe(false);

		// `isolate` is per-image visibility: one body drawn, one legend entry —
		// and the fit frames THAT body, not the pair with one of them missing.
		expect(shot.structured.legend.map((l) => l.id)).toEqual([target]);
		expect(widest(shot.structured.framed)).toBeCloseTo(0.04, 3);

		// A box from directly above shows exactly ONE face: the four sides are
		// edge-on or back-facing to this camera and the bottom is hidden by the
		// first-hit test, so neither the image nor the array carries them.
		const labels = shot.structured.labels;
		expect(labels).toHaveLength(1);
		const [label] = labels;
		expect(label.kind).toBe('face_ids');
		expect(label.text).toMatch(/^f\d+$/);
		expect(label.body_id).toBe(target);
		// The ref is the engine's own GeomRef for that face, so an agent can take
		// it straight to entity_meta / face_list without parsing the picture.
		expect(label.ref).toBeTruthy();
		expect(typeof label.ref).toBe('object');
		expect(label.ref.selector ?? label.ref.anchor).toBeTruthy();
		// Looking down the top face, the label's anchor is the face centroid: the
		// middle of the frame.
		expect(label.x).toBeCloseTo(200, -1);
		expect(label.y).toBeCloseTo(200, -1);

		// Every face the body has is a label candidate; one survives the view.
		const faceRanges = await page.evaluate(
			(id) => window.__waffle.getMeshes().find((m) => m.bodyId === id)?.faceRangeCount ?? 0,
			target
		);
		expect(faceRanges).toBeGreaterThan(1);
		expectNoAnyCrash(crashes);
	});

	test('the capture renders offscreen: the user\'s camera does not move and two calls agree', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await twoBoxes(page);
		const before = await cameraAtRest(page);

		const first = await capture(page, {
			view: 'front',
			size: { width: 256, height: 256 },
			style: 'agent',
			labels: ['body_names']
		});
		expect(first.isError, JSON.stringify(first.structured)).toBe(false);
		// A `front` capture looks along +Y; the live camera is still wherever the
		// user left it, which is NOT that.
		expect(first.structured.camera.position[1]).toBeLessThan(first.structured.camera.target[1]);

		const after = await page.evaluate(() => window.__waffle.getCameraState());
		expect(after.position).toEqual(before.position);
		expect(after.target).toEqual(before.target);
		expect(after.up).toEqual(before.up);

		// Same arguments, same described image: the pass reads nothing that
		// drifts between calls (this is the property §9.3's byte oracle, V3,
		// will then pin on the pixels).
		const second = await capture(page, {
			view: 'front',
			size: { width: 256, height: 256 },
			style: 'agent',
			labels: ['body_names']
		});
		expect(second.structured.legend).toEqual(first.structured.legend);
		expect(second.structured.labels).toEqual(first.structured.labels);
		expect(second.structured.camera).toEqual(first.structured.camera);
		expectNoAnyCrash(crashes);
	});

	test('a capture with no new arguments is the pre-V1 call: the live view, shaded, unlabelled', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		await twoBoxes(page);
		const shot = await capture(page, { max_edge_px: 256 });
		expect(shot.isError, JSON.stringify(shot.structured)).toBe(false);
		expect(shot.structured.mime_type).toBe('image/png');
		expect(Math.max(shot.structured.width, shot.structured.height)).toBe(256);
		expect(pngSize(shot.png)).toEqual({ width: shot.structured.width, height: shot.structured.height });
		expect(shot.structured.style).toBe('shaded');
		expect(shot.structured.color_by).toBeNull();
		expect(shot.structured.legend).toEqual([]);
		expect(shot.structured.labels).toEqual([]);
		// No `view`, no `frame` ⇒ no fit: the user's framing is kept.
		expect(shot.structured.framed).toBeNull();
		expectNoAnyCrash(crashes);
	});

	test('contradictory and unresolvable arguments are refused, loudly and by name', async ({ page }) => {
		const crashes = collectCrashErrors(page);
		const bodies = await twoBoxes(page);

		const both = await capture(page, { view: 'top', camera: { position: [0, 0, 1], target: [0, 0, 0] } });
		expect(both.isError).toBe(true);
		expect(both.structured.error.code).toBe('InvalidArguments');

		const colored = await capture(page, { style: 'shaded', color_by: 'body' });
		expect(colored.isError).toBe(true);
		expect(colored.structured.error.code).toBe('InvalidArguments');

		const missing = await capture(page, { style: 'agent', isolate: ['no-such-body'] });
		expect(missing.isError).toBe(true);
		expect(missing.structured.error.code).toBe('BodyNotFound');
		expect(missing.structured.error.details.body_ids).toEqual(['no-such-body']);

		const framed = await capture(page, { frame: { body_ids: ['no-such-body'] } });
		expect(framed.isError).toBe(true);
		expect(framed.structured.error.code).toBe('BodyNotFound');

		const clashing = await capture(page, { style: 'agent', isolate: [bodies[0].id], hide: [bodies[0].id] });
		expect(clashing.isError).toBe(true);
		expect(clashing.structured.error.code).toBe('InvalidArguments');

		// Out-of-vocabulary values refuse by name too. The relay validates
		// arguments against `inputSchema`, but this executor does not, and an
		// unchecked value did not refuse: it drew a shaded image that reported
		// `style: "wireframe"`, or threw inside the capture component where the
		// exception cannot reach the caller and the tool blamed a missing
		// viewport instead.
		for (const args of [
			{ view: 'nope' },
			{ style: 'wireframe' },
			{ projection: 'fisheye' },
			{ labels: ['dimensions'] },
			{ style: 'agent', color_by: 'material' },
			{ size: { width: 'big', height: 10 } },
			{ isolate: [7] },
			{ frame: { point: ['x', 0, 0], radius: 1 } }
		]) {
			const refused = await capture(page, args);
			expect(refused.isError, JSON.stringify(args)).toBe(true);
			expect(refused.structured.error.code, JSON.stringify(args)).toBe('InvalidArguments');
		}
		expectNoAnyCrash(crashes);
	});
});
