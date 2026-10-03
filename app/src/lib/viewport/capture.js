/**
 * The agent capture pass (`specs/agent_mechanical_design.md` §9, increment V1).
 *
 * `viewport_capture` used to read the live drawing buffer: whatever size the
 * canvas happened to be, whatever the pointer happened to be hovering, in the
 * user's theme. An agent cannot reason about a picture whose framing it did not
 * choose and whose colours mean something only to a human, so the capture is
 * now its OWN render pass:
 *
 * - a camera built from the arguments (`view` / `camera` / `frame` / `fit` /
 *   `projection`), using `framing.js` — the same numbers the View Cube and the
 *   F key use — while the user's camera stays exactly where it was;
 * - into an offscreen `WebGLRenderTarget` of exactly `size`, so neither the
 *   canvas size nor the device pixel ratio is in the result;
 * - with per-object overrides (flat legend colours, black edges, enlarged
 *   vertices, no antialiasing, `isolate`/`hide`) applied for the duration and
 *   restored after, so hover and selection cannot reach the image;
 * - then a 2D pass that composites the frame over a fixed background and draws
 *   the labels and their leaders.
 *
 * The result carries `legend` and `labels` BESIDE the PNG: every colour and
 * every piece of drawn text maps back to a reference the agent can query, with
 * no OCR. Tests read the same arrays.
 *
 * Determinism (the §9.3 oracle, increment V3, is not in this increment) is a
 * property of `style: "agent"`: fixed palette, fixed background, no
 * antialiasing, no theme, no hover, labels placed by a deterministic
 * nearest-first sweep. `style: "shaded"` is the user's shading — it is a
 * picture of their viewport, and it moves with their theme by design.
 */
import * as THREE from 'three';
import { STANDARD_VIEWS, fitBoxFor, fitDistance, frameBoxFor } from './framing.js';
import { edgeViewportHeight, withEdgeDepthBias } from './edgeDepthBias.js';

export const CAPTURE_STYLES = ['shaded', 'agent'];
export const COLOR_BY = ['body', 'feature'];
export const LABEL_KINDS = ['body_names', 'face_ids'];
export const PROJECTIONS = ['perspective', 'orthographic'];

/** Size bounds per axis, matching the legacy `max_edge_px` bounds. */
export const MIN_EDGE_PX = 64;
export const MAX_EDGE_PX = 4096;

/**
 * The 24 fixed legend colours, assigned in sorted-id order (§9.1) so the same
 * document always colours the same body the same way. Mid-tone and distinct in
 * hue: flat-shaded against a white ground with black edges and black label
 * text over them.
 */
export const PALETTE = [
	'#3b7dd8', '#d85c3b', '#36a06a', '#b14fc0', '#c9a227', '#2aa3b5',
	'#d4587f', '#6d8f2f', '#8a6ad0', '#c2762a', '#2f8f8f', '#b23b5e',
	'#4f7f4f', '#9b5ba8', '#a8882f', '#3f6fb5', '#c05a3f', '#53a05c',
	'#7a5fc0', '#b8902f', '#357f9b', '#a84f6a', '#5f8f3f', '#8f5fb0'
];

/** Flat background of an `agent`-style capture. Never the theme's. */
export const AGENT_BACKGROUND = '#ffffff';
/** Edge colour of an `agent`-style capture. */
export const AGENT_EDGE_COLOR = 0x000000;
/** Vertex point size (px) of an `agent`-style capture — enlarged, §9.1. */
export const AGENT_VERTEX_SIZE = 8;

/** A capture argument that cannot be honoured. Mapped to a tool failure. */
export class CaptureError extends Error {
	/** @param {string} code @param {string} detail @param {object} [details] */
	constructor(code, detail, details = {}) {
		super(detail);
		this.code = code;
		this.detail = detail;
		this.details = details;
	}
}

/** @param {number} v @param {number} lo @param {number} hi */
const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));

/**
 * The image size this call asks for.
 * `size` is exact (clamped to the per-axis bounds); without it the live canvas
 * aspect is kept and scaled DOWN to `max_edge_px`, which is what the tool did
 * before V1 and what a call with no new arguments still gets.
 * @param {{ size?: {width: number, height: number} | null, max_edge_px?: number }} args
 * @param {{ width: number, height: number }} canvas
 */
export function captureSize(args, canvas) {
	if (args.size) {
		return {
			width: Math.round(clamp(args.size.width, MIN_EDGE_PX, MAX_EDGE_PX)),
			height: Math.round(clamp(args.size.height, MIN_EDGE_PX, MAX_EDGE_PX))
		};
	}
	const maxEdge = clamp(args.max_edge_px ?? 1024, MIN_EDGE_PX, MAX_EDGE_PX);
	const longest = Math.max(canvas.width, canvas.height);
	const scale = Math.min(1, maxEdge / longest);
	return {
		width: Math.max(1, Math.round(canvas.width * scale)),
		height: Math.max(1, Math.round(canvas.height * scale))
	};
}

/**
 * Build the camera this capture renders through, WITHOUT touching the live one.
 *
 * @param {object} opts
 * @param {THREE.Object3D} opts.scene
 * @param {THREE.Camera} opts.liveCamera
 * @param {THREE.Vector3} opts.liveTarget
 * @param {number} opts.aspect
 * @param {any} opts.args
 * @returns {{ camera: THREE.Camera, framed: {min: number[], max: number[]} | null }}
 */
export function buildCaptureCamera({ scene, liveCamera, liveTarget, aspect, args }) {
	const liveOrtho = /** @type {any} */ (liveCamera).isOrthographicCamera === true;
	const projection = args.projection ?? (liveOrtho ? 'orthographic' : 'perspective');
	const fov = /** @type {any} */ (liveCamera).fov ?? 50;

	let target = liveTarget.clone();
	let direction;
	let up;
	let distance;

	if (args.camera) {
		const pos = new THREE.Vector3(...args.camera.position);
		target = new THREE.Vector3(...args.camera.target);
		up = args.camera.up ? new THREE.Vector3(...args.camera.up) : liveCamera.up.clone();
		distance = pos.distanceTo(target);
		if (!(distance > 0)) {
			throw new CaptureError('InvalidArguments', 'camera.position and camera.target are the same point.', {
				camera: args.camera
			});
		}
		direction = pos.clone().sub(target).normalize();
	} else if (args.view) {
		const view = STANDARD_VIEWS[args.view];
		direction = new THREE.Vector3(...view.pos).normalize();
		up = new THREE.Vector3(...view.up);
		distance = liveCamera.position.distanceTo(target) || 0.01;
	} else {
		direction = liveCamera.position.clone().sub(target);
		if (!(direction.length() > 0)) direction.set(1, -1, 1);
		direction.normalize();
		up = liveCamera.up.clone();
		distance = liveCamera.position.distanceTo(target) || 0.01;
	}

	// An explicit `frame` is the framing; `fit` applies only without one. Both
	// resolve BEFORE anything is built, so a frame naming a body this view does
	// not have refuses instead of silently framing something else.
	let box = null;
	if (args.frame) {
		const found = frameBoxFor(scene, args.frame);
		if (found.missing.length > 0) {
			throw new CaptureError('BodyNotFound', 'No visible body with that id in this view.', {
				body_ids: found.missing
			});
		}
		box = found.box;
	} else if (args.fit) {
		box = fitBoxFor(scene);
	}

	let frustumHalf = liveOrtho ? /** @type {any} */ (liveCamera).top : 0.2;
	if (box && !args.camera) {
		const size = box.getSize(new THREE.Vector3());
		const fit = fitDistance({ maxDim: Math.max(size.x, size.y, size.z), projection, fov });
		target = box.getCenter(new THREE.Vector3());
		distance = fit.distance;
		frustumHalf = fit.frustumHalf;
	} else if (box && args.camera) {
		// An explicit camera is the camera; a box alongside it only sets the
		// orthographic zoom, never the position the caller gave.
		const size = box.getSize(new THREE.Vector3());
		frustumHalf = fitDistance({ maxDim: Math.max(size.x, size.y, size.z), projection, fov }).frustumHalf;
	}

	/** @type {THREE.Camera} */
	let camera;
	if (projection === 'orthographic') {
		const fh = frustumHalf > 0 ? frustumHalf : 0.2;
		camera = new THREE.OrthographicCamera(-fh * aspect, fh * aspect, fh, -fh, -1e7, 1e7);
	} else {
		camera = new THREE.PerspectiveCamera(fov, aspect, 1e-4, 1e7);
	}
	camera.up.copy(up);
	camera.position.copy(target).addScaledVector(direction, distance);
	camera.lookAt(target);
	camera.updateMatrixWorld(true);
	/** @type {any} */ (camera).userData.waffleCaptureTarget = target.clone();

	return {
		camera,
		framed: box ? { min: box.min.toArray(), max: box.max.toArray() } : null
	};
}

/** The camera, as the tool reports it. @param {THREE.Camera} camera */
export function describeCamera(camera) {
	const target = /** @type {any} */ (camera).userData?.waffleCaptureTarget ?? new THREE.Vector3();
	return {
		projection: /** @type {any} */ (camera).isOrthographicCamera ? 'orthographic' : 'perspective',
		position: camera.position.toArray(),
		target: target.toArray(),
		up: camera.up.toArray()
	};
}

/**
 * Walk the scene once and bucket what the pass needs to touch.
 * @param {THREE.Object3D} scene
 */
function collectScene(scene) {
	/** @type {THREE.Mesh[]} */
	const models = [];
	/** @type {THREE.Object3D[]} */
	const edges = [];
	/** @type {THREE.Object3D[]} */
	const helpers = [];
	/** @type {THREE.Points[]} */
	const points = [];
	/** @type {THREE.DirectionalLight[]} */
	const lights = [];
	scene.traverse((obj) => {
		const ud = /** @type {any} */ (obj).userData ?? {};
		if (ud.waffleType === 'model') models.push(/** @type {THREE.Mesh} */ (obj));
		else if (ud.waffleType === 'edges') edges.push(obj);
		else if (ud.waffleType === 'helper') helpers.push(obj);
		if (/** @type {any} */ (obj).isPoints) points.push(/** @type {THREE.Points} */ (obj));
		if (/** @type {any} */ (obj).isDirectionalLight) lights.push(/** @type {any} */ (obj));
	});
	return { models, edges, helpers, points, lights };
}

/**
 * The legend for this capture: one entry per drawn body (or per feature), with
 * the palette colour assigned in sorted-id order so it is stable across runs.
 * @param {{ bodyId: string | null, featureId: string | null, name: string | null }[]} drawn
 * @param {'body' | 'feature' | null} colorBy
 */
export function buildLegend(drawn, colorBy) {
	if (!colorBy) return { legend: [], colorOf: () => null };
	/** @type {Map<string, string>} */
	const names = new Map();
	for (const d of drawn) {
		const id = colorBy === 'body' ? d.bodyId : d.featureId;
		if (!id) continue;
		if (!names.has(id)) names.set(id, d.name ?? null);
		else if (names.get(id) == null && d.name) names.set(id, d.name);
	}
	const ids = [...names.keys()].sort();
	/** @type {Map<string, string>} */
	const colors = new Map();
	const legend = ids.map((id, i) => {
		const color = PALETTE[i % PALETTE.length];
		colors.set(id, color);
		return { color, kind: colorBy, id, name: names.get(id) ?? null };
	});
	return {
		legend,
		/** @param {{bodyId: string|null, featureId: string|null}} d */
		colorOf: (d) => colors.get((colorBy === 'body' ? d.bodyId : d.featureId) ?? '') ?? PALETTE[0]
	};
}

/**
 * Area-weighted centroid and normal of one face range, in WORLD space.
 * `start_index`/`end_index` index the body's index buffer, three indices per
 * triangle — the same convention `CadModel` groups on.
 * @param {THREE.Mesh} mesh
 * @param {{ start_index: number, end_index: number }} range
 */
function faceAnchor(mesh, range) {
	const geo = mesh.geometry;
	const pos = /** @type {THREE.BufferAttribute} */ (geo.getAttribute('position'));
	const index = geo.getIndex();
	if (!pos || !index) return null;
	const idx = index.array;
	const a = new THREE.Vector3();
	const b = new THREE.Vector3();
	const c = new THREE.Vector3();
	const ab = new THREE.Vector3();
	const ac = new THREE.Vector3();
	const cross = new THREE.Vector3();
	const centroid = new THREE.Vector3();
	const normal = new THREE.Vector3();
	let area = 0;
	const start = Math.max(0, range.start_index);
	const end = Math.min(idx.length, range.end_index);
	for (let i = start; i + 2 < end; i += 3) {
		a.fromBufferAttribute(pos, idx[i]);
		b.fromBufferAttribute(pos, idx[i + 1]);
		c.fromBufferAttribute(pos, idx[i + 2]);
		ab.subVectors(b, a);
		ac.subVectors(c, a);
		cross.crossVectors(ab, ac);
		const w = cross.length() / 2;
		if (!(w > 0)) continue;
		area += w;
		centroid.addScaledVector(a.add(b).add(c).multiplyScalar(1 / 3), w);
		normal.addScaledVector(cross.normalize(), w);
	}
	if (!(area > 0)) return null;
	centroid.multiplyScalar(1 / area);
	mesh.updateMatrixWorld();
	const world = centroid.clone().applyMatrix4(mesh.matrixWorld);
	const worldNormal = normal
		.normalize()
		.applyMatrix3(new THREE.Matrix3().getNormalMatrix(mesh.matrixWorld))
		.normalize();
	return { point: world, normal: worldNormal };
}

/**
 * Project a world point into image pixels (top-left origin), or null when it
 * falls outside the frame.
 * @param {THREE.Vector3} point
 * @param {THREE.Camera} camera
 * @param {number} width
 * @param {number} height
 */
function toPixels(point, camera, width, height) {
	const ndc = point.clone().project(camera);
	if (!Number.isFinite(ndc.x) || !Number.isFinite(ndc.y)) return null;
	if (ndc.x < -1 || ndc.x > 1 || ndc.y < -1 || ndc.y > 1 || ndc.z < -1 || ndc.z > 1) return null;
	return { x: ((ndc.x + 1) / 2) * width, y: ((1 - ndc.y) / 2) * height, depth: ndc.z };
}

/**
 * First-hit visibility: is `point` the nearest surface along the view ray?
 * @param {THREE.Vector3} point
 * @param {THREE.Camera} camera
 * @param {THREE.Raycaster} raycaster
 * @param {THREE.Object3D[]} targets
 * @param {number} span the scene's size, for the ray origin in orthographic
 */
function firstHitVisible(point, camera, raycaster, targets, span) {
	const ortho = /** @type {any} */ (camera).isOrthographicCamera === true;
	const forward = camera.getWorldDirection(new THREE.Vector3());
	const origin = ortho
		? point.clone().addScaledVector(forward, -(span * 2 + 1))
		: camera.position.clone();
	const dir = point.clone().sub(origin);
	const dist = dir.length();
	if (!(dist > 0)) return true;
	dir.multiplyScalar(1 / dist);
	raycaster.set(origin, dir);
	raycaster.near = 0;
	raycaster.far = dist * 1.01 + span;
	const hits = raycaster.intersectObjects(targets, false);
	if (hits.length === 0) return true;
	const eps = Math.max(1e-9, dist * 1e-3);
	return hits[0].distance >= dist - eps;
}

/**
 * Render one capture and draw its labels.
 *
 * @param {object} ctx
 * @param {THREE.WebGLRenderer} ctx.renderer
 * @param {THREE.Scene} ctx.scene
 * @param {THREE.Camera} ctx.liveCamera
 * @param {THREE.Vector3} ctx.liveTarget
 * @param {{ bodyId: string|null, featureId: string|null, name: string|null,
 *           faceRanges: Array<{geom_ref: any, start_index: number, end_index: number}> }[]} ctx.bodies
 *        the store's render list, for names and face ranges
 * @param {string} ctx.background CSS colour behind the frame in `shaded` style
 * @param {any} args the tool's arguments, already validated
 */
export function renderCapture({ renderer, scene, liveCamera, liveTarget, bodies, background, args }) {
	const style = args.style ?? 'shaded';
	const colorBy = args.color_by ?? (style === 'agent' ? 'body' : null);
	const labelKinds = new Set(args.labels ?? []);
	const canvas = renderer.domElement;
	const { width, height } = captureSize(args, { width: canvas.width || 1, height: canvas.height || 1 });

	const parts = collectScene(scene);
	/** @type {Map<string, any>} */
	const byBodyId = new Map();
	for (const b of bodies) if (b.bodyId) byBodyId.set(b.bodyId, b);

	// Visibility for THIS image only. A named body that is not in the view is
	// named back, never silently dropped.
	/** @type {Set<string> | null} */
	let keep = null;
	if (args.isolate?.length) keep = new Set(args.isolate);
	const hide = new Set(args.hide ?? []);
	if (keep || hide.size > 0) {
		const present = new Set(
			parts.models.map((m) => /** @type {any} */ (m).userData?.bodyId).filter(Boolean)
		);
		const missing = [...(keep ?? []), ...hide].filter((id) => !present.has(id));
		if (missing.length > 0) {
			throw new CaptureError('BodyNotFound', 'No body with that id is drawn in this view.', {
				body_ids: [...new Set(missing)]
			});
		}
	}
	/** @param {THREE.Object3D} obj */
	const hidden = (obj) => {
		const id = /** @type {any} */ (obj).userData?.bodyId;
		if (!id) return false;
		if (hide.has(id)) return true;
		return keep ? !keep.has(id) : false;
	};

	// --- the overrides, every one restored in `restore` -------------------
	/** @type {Array<() => void>} */
	const restore = [];
	/** @param {THREE.Object3D} obj @param {boolean} visible */
	const setVisible = (obj, visible) => {
		if (obj.visible === visible) return;
		const was = obj.visible;
		obj.visible = visible;
		restore.push(() => {
			obj.visible = was;
		});
	};
	/** @param {any} obj @param {THREE.Material | THREE.Material[]} material */
	const setMaterial = (obj, material) => {
		const was = obj.material;
		obj.material = material;
		restore.push(() => {
			obj.material = was;
		});
	};
	/** @type {THREE.Material[]} */
	const disposable = [];

	// EVERYTHING from here to the readback is inside one try/finally: a refusal
	// raised halfway through (a `frame` naming a body that is not drawn) must
	// not leave the user's scene with a body hidden or a material swapped.
	/** @type {any} */
	let out;
	try {
		/** @type {{ bodyId: string|null, featureId: string|null, name: string|null,
		 *           faceRanges: any[], mesh: THREE.Mesh }[]} */
		const drawn = [];
		for (const mesh of parts.models) {
			const id = /** @type {any} */ (mesh).userData?.bodyId ?? null;
			if (hidden(mesh)) {
				setVisible(mesh, false);
				continue;
			}
			const body = id ? byBodyId.get(id) : null;
			drawn.push({
				bodyId: id,
				featureId: body?.featureId ?? null,
				name: body?.name ?? null,
				faceRanges: body?.faceRanges ?? [],
				mesh
			});
		}
		for (const obj of parts.edges) if (hidden(obj)) setVisible(obj, false);

		// The camera is built AFTER `isolate`/`hide` have been applied, because a
		// fit reads what is visible: "isolate this body and frame it" has to frame
		// the body, not the whole model with one body left in it.
		const { camera, framed } = buildCaptureCamera({
			scene,
			liveCamera,
			liveTarget,
			aspect: width / height,
			args
		});

		const { legend, colorOf } = buildLegend(drawn, colorBy);

		if (style === 'agent') {
			// Flat unlit colour per body: no lighting, no theme, and no way for a
			// hover or a selection to reach the image.
			for (const d of drawn) {
				const clip = /** @type {any} */ (d.mesh.material);
				const first = Array.isArray(clip) ? clip[0] : clip;
				const mat = new THREE.MeshBasicMaterial({
					color: new THREE.Color(colorOf(d) ?? PALETTE[0]),
					side: THREE.DoubleSide,
					clippingPlanes: first?.clippingPlanes ?? []
				});
				disposable.push(mat);
				setMaterial(d.mesh, mat);
			}
			// One black edge material for every body. Built with `withEdgeDepthBias`
			// rather than cloned from the live one: `Material.copy` does NOT carry
			// `onBeforeCompile`, so a clone would silently lose the bias and bring
			// the z-fight dashing back.
			if (parts.edges.length > 0) {
				const sample = /** @type {any} */ (parts.edges[0]).material;
				const first = Array.isArray(sample) ? sample[0] : sample;
				const edgeMat = withEdgeDepthBias(
					new THREE.LineBasicMaterial({
						color: AGENT_EDGE_COLOR,
						depthTest: true,
						clippingPlanes: first?.clippingPlanes ?? []
					})
				);
				disposable.push(edgeMat);
				for (const obj of parts.edges) setMaterial(obj, edgeMat);
			}
			for (const pts of parts.points) {
				const mat = /** @type {any} */ (pts).material;
				if (mat && typeof mat.size === 'number') {
					const was = mat.size;
					mat.size = AGENT_VERTEX_SIZE;
					restore.push(() => {
						mat.size = was;
					});
				}
			}
			// The orbit pivot marker is an artefact of the user's gesture.
			for (const h of parts.helpers) setVisible(h, false);
		} else {
			// Shaded: the lights are aimed in CAMERA space every frame from the LIVE
			// camera (Lighting.svelte). A capture through another camera must re-aim
			// them, or a `view: "back"` picture is lit from the user's shoulder.
			const liveQuat = liveCamera.quaternion.clone().invert();
			const quat = camera.quaternion;
			for (const light of parts.lights) {
				const was = light.position.clone();
				const d = was.length();
				if (!(d > 0)) continue;
				const local = was.clone().multiplyScalar(1 / d).applyQuaternion(liveQuat);
				light.position.copy(local.applyQuaternion(quat).multiplyScalar(d));
				restore.push(() => {
					light.position.copy(was);
				});
			}
		}

		// The edge depth bias is measured in pixels of the frame being drawn.
		const wasEdgeHeight = edgeViewportHeight.value;
		edgeViewportHeight.value = height;
		restore.push(() => {
			edgeViewportHeight.value = wasEdgeHeight;
		});

		// --- labels, measured against the camera this pass renders ------------
		const labels = labelKinds.size > 0 ? buildLabels({ drawn, camera, width, height, labelKinds }) : [];

		// --- the offscreen frame ---------------------------------------------
		// `agent` renders 1:1 — no antialiasing is part of the style (§9.1), and a
		// flat colour needs none. `shaded` supersamples and box-filters down, which
		// is deterministic where a multisampled render target's resolve is not (and
		// `readRenderTargetPixels` on one is not portable).
		const ss = style === 'agent' ? 1 : supersample(width, height);
		const rw = width * ss;
		const rh = height * ss;
		let pixels;
		const target = new THREE.WebGLRenderTarget(rw, rh, {
			samples: 0,
			stencilBuffer: true,
			depthBuffer: true
		});
		target.texture.colorSpace = renderer.outputColorSpace;
		const wasTarget = renderer.getRenderTarget();
		try {
			renderer.setRenderTarget(target);
			renderer.render(scene, camera);
			pixels = new Uint8Array(rw * rh * 4);
			renderer.readRenderTargetPixels(target, 0, 0, rw, rh, pixels);
	} finally {
		renderer.setRenderTarget(wasTarget);
		target.dispose();
	}

	out = {
		pixels,
		ss,
		width,
		height,
		camera: describeCamera(camera),
		framed,
		style,
		color_by: colorBy,
		legend,
		labels
	};
	} finally {
		for (let i = restore.length - 1; i >= 0; i--) restore[i]();
		for (const mat of disposable) mat.dispose();
	}

	// The 2D pass is outside the overrides: nothing it reads is in the scene.
	const bg = style === 'agent' ? AGENT_BACKGROUND : background;
	return {
		png: composite({
			pixels: out.pixels,
			width: out.width,
			height: out.height,
			ss: out.ss,
			background: bg,
			labels: out.labels,
			style: out.style
		}),
		width: out.width,
		height: out.height,
		camera: out.camera,
		framed: out.framed,
		style: out.style,
		color_by: out.color_by,
		legend: out.legend,
		labels: out.labels.map((l) => ({
			kind: l.kind,
			text: l.text,
			x: Math.round(l.anchor.x * 10) / 10,
			y: Math.round(l.anchor.y * 10) / 10,
			body_id: l.bodyId,
			ref: l.ref
		}))
	};
}

/** Label box padding and leader length, in pixels, for a 12 px font. */
const LABEL_PAD = 3;
const LEADER = 12;

/**
 * Place the labels: project each anchor, drop the ones the camera cannot see,
 * then sweep nearest-first and drop any whose box would overlap one already
 * placed. Deterministic: the sweep order is depth, then the body's render
 * order, then the face's range start.
 *
 * @param {object} opts
 * @param {{ bodyId: string|null, featureId: string|null, name: string|null,
 *           faceRanges: any[], mesh: THREE.Mesh }[]} opts.drawn
 * @param {THREE.Camera} opts.camera
 * @param {number} opts.width
 * @param {number} opts.height
 * @param {Set<string>} opts.labelKinds
 */
function buildLabels({ drawn, camera, width, height, labelKinds }) {
	const fontPx = clamp(Math.round(height / 40), 10, 20);
	const measure = textMeasurer(fontPx);
	const meshes = drawn.map((d) => d.mesh);
	// Model meshes have picking disabled in some states (`raycast` replaced by a
	// no-op prop), which would make every label "visible". Restore the real
	// raycast for the duration of the test.
	/** @type {Array<() => void>} */
	const undo = [];
	for (const mesh of meshes) {
		if (mesh.raycast !== THREE.Mesh.prototype.raycast) {
			const was = mesh.raycast;
			mesh.raycast = THREE.Mesh.prototype.raycast;
			undo.push(() => {
				mesh.raycast = was;
			});
		}
	}
	const raycaster = new THREE.Raycaster();
	const sceneBox = new THREE.Box3();
	for (const mesh of meshes) sceneBox.expandByObject(mesh);
	const span = sceneBox.isEmpty() ? 1 : sceneBox.getSize(new THREE.Vector3()).length();

	/** @type {{kind: string, text: string, anchor: {x:number,y:number}, depth: number, bodyId: string|null, ref: any, order: number}[]} */
	const candidates = [];
	try {
		for (let bi = 0; bi < drawn.length; bi++) {
			const d = drawn[bi];
			if (labelKinds.has('body_names')) {
				// The top of the body's box, as §9.2 specifies: above the body, so
				// a body label never sits on a face label.
				const box = new THREE.Box3().setFromObject(d.mesh);
				if (!box.isEmpty()) {
					const centre = box.getCenter(new THREE.Vector3());
					centre.z = box.max.z;
					const at = toPixels(centre, camera, width, height);
					if (at) {
						candidates.push({
							kind: 'body_names',
							text: d.name ?? d.bodyId ?? 'body',
							anchor: { x: at.x, y: at.y },
							depth: at.depth,
							bodyId: d.bodyId,
							ref: d.bodyId ? { body_id: d.bodyId } : null,
							order: bi * 1e6
						});
					}
				}
			}
			if (labelKinds.has('face_ids')) {
				const ranges = d.faceRanges ?? [];
				for (let fi = 0; fi < ranges.length; fi++) {
					const range = ranges[fi];
					const anchor = faceAnchor(d.mesh, range);
					if (!anchor) continue;
					// Back-facing: cheap, and correct for the dominant case.
					const toEye = /** @type {any} */ (camera).isOrthographicCamera
						? camera.getWorldDirection(new THREE.Vector3()).negate()
						: camera.position.clone().sub(anchor.point).normalize();
					if (anchor.normal.dot(toEye) <= 0) continue;
					const at = toPixels(anchor.point, camera, width, height);
					if (!at) continue;
					if (!firstHitVisible(anchor.point, camera, raycaster, meshes, span)) continue;
					candidates.push({
						kind: 'face_ids',
						text: `f${fi + 1}`,
						anchor: { x: at.x, y: at.y },
						depth: at.depth,
						bodyId: d.bodyId,
						ref: range.geom_ref ?? null,
						order: bi * 1e6 + fi
					});
				}
			}
		}
	} finally {
		for (const fn of undo) fn();
	}

	candidates.sort((a, b) => a.depth - b.depth || a.order - b.order);

	/** @type {{x0:number,y0:number,x1:number,y1:number}[]} */
	const placed = [];
	/** @type {any[]} */
	const out = [];
	for (const c of candidates) {
		const w = measure(c.text) + LABEL_PAD * 2;
		const h = fontPx + LABEL_PAD * 2;
		// Up and to the right of the anchor; flipped back inside the frame when
		// that would hang off an edge.
		let x0 = c.anchor.x + LEADER;
		let y0 = c.anchor.y - LEADER - h;
		if (x0 + w > width) x0 = c.anchor.x - LEADER - w;
		if (y0 < 0) y0 = c.anchor.y + LEADER;
		if (x0 < 0 || y0 + h > height) continue;
		const box = { x0, y0, x1: x0 + w, y1: y0 + h };
		if (placed.some((p) => box.x0 < p.x1 && box.x1 > p.x0 && box.y0 < p.y1 && box.y1 > p.y0)) continue;
		placed.push(box);
		out.push({ ...c, box, fontPx });
	}
	return out;
}

/**
 * A text measurer on a throwaway 2D context, so label layout does not depend on
 * the canvas the frame is drawn on.
 * @param {number} fontPx
 */
function textMeasurer(fontPx) {
	const c = document.createElement('canvas');
	const ctx = /** @type {CanvasRenderingContext2D} */ (c.getContext('2d'));
	ctx.font = `${fontPx}px monospace`;
	return (/** @type {string} */ text) => ctx.measureText(text).width;
}

/**
 * Composite the read-back frame over the background and draw the labels.
 *
 * The frame comes back bottom-up with PREMULTIPLIED alpha (the renderer's
 * canvas is `alpha: true`), so the rows are flipped and each pixel is resolved
 * against the background here rather than leaning on `drawImage`, which would
 * have to un-premultiply first and lose the dark edges to it.
 *
 * @param {object} opts
 * @param {Uint8Array} opts.pixels
 * @param {number} opts.width
 * @param {number} opts.height
 * @param {number} opts.ss supersample factor the frame was rendered at
 * @param {string} opts.background
 * @param {any[]} opts.labels
 * @param {string} opts.style
 * @returns {string} base64 PNG, without the data-URL prefix
 */
function composite({ pixels, width, height, ss, background, labels, style }) {
	const out = document.createElement('canvas');
	out.width = width;
	out.height = height;
	const ctx = /** @type {CanvasRenderingContext2D} */ (out.getContext('2d'));
	const bg = parseColor(background);
	const image = ctx.createImageData(width, height);
	const data = image.data;
	const rw = width * ss;
	const rh = height * ss;
	const n = ss * ss;
	for (let y = 0; y < height; y++) {
		for (let x = 0; x < width; x++) {
			let r = 0;
			let g = 0;
			let b = 0;
			for (let sy = 0; sy < ss; sy++) {
				// Bottom-up: GL row 0 is the last image row.
				const row = rh - 1 - (y * ss + sy);
				for (let sx = 0; sx < ss; sx++) {
					const src = (row * rw + x * ss + sx) * 4;
					const a = pixels[src + 3] / 255;
					r += pixels[src] + bg[0] * (1 - a);
					g += pixels[src + 1] + bg[1] * (1 - a);
					b += pixels[src + 2] + bg[2] * (1 - a);
				}
			}
			const dst = (y * width + x) * 4;
			data[dst] = Math.round(r / n);
			data[dst + 1] = Math.round(g / n);
			data[dst + 2] = Math.round(b / n);
			data[dst + 3] = 255;
		}
	}
	ctx.putImageData(image, 0, 0);

	if (labels.length > 0) drawLabels(ctx, labels, style);
	const url = out.toDataURL('image/png');
	return url.slice(url.indexOf(',') + 1);
}

/**
 * @param {CanvasRenderingContext2D} ctx
 * @param {any[]} labels
 * @param {string} style
 */
function drawLabels(ctx, labels, style) {
	const ink = style === 'agent' ? '#000000' : '#101010';
	const paper = style === 'agent' ? '#ffffff' : '#f6f6f6';
	ctx.lineWidth = 1;
	ctx.textBaseline = 'top';
	for (const l of labels) {
		ctx.font = `${l.fontPx}px monospace`;
		const { box } = l;
		// Leader from the anchor to the nearest corner of the box.
		const cx = l.anchor.x < box.x0 ? box.x0 : box.x1;
		const cy = l.anchor.y < box.y0 ? box.y0 : box.y1;
		ctx.strokeStyle = ink;
		ctx.beginPath();
		ctx.moveTo(l.anchor.x, l.anchor.y);
		ctx.lineTo(cx, cy);
		ctx.stroke();
		ctx.fillStyle = ink;
		ctx.beginPath();
		ctx.arc(l.anchor.x, l.anchor.y, 2, 0, Math.PI * 2);
		ctx.fill();
		ctx.fillStyle = paper;
		ctx.fillRect(box.x0, box.y0, box.x1 - box.x0, box.y1 - box.y0);
		ctx.strokeStyle = ink;
		ctx.strokeRect(box.x0 + 0.5, box.y0 + 0.5, box.x1 - box.x0 - 1, box.y1 - box.y0 - 1);
		ctx.fillStyle = ink;
		ctx.fillText(l.text, box.x0 + LABEL_PAD, box.y0 + LABEL_PAD);
	}
}

/**
 * Supersample factor for a shaded capture: 2 where the render target fits, else
 * 1. The GL maximum texture size is at least 4096 everywhere WebGL2 runs, and
 * the capture size itself is capped there.
 * @param {number} width @param {number} height
 */
function supersample(width, height) {
	return Math.max(width, height) * 2 <= MAX_EDGE_PX ? 2 : 1;
}

/**
 * A CSS colour as [r, g, b] 0..255. Opaque fallback, never transparent: a
 * transparent background would make the PNG depend on whatever shows it.
 * `getHexString` applies the working-space → sRGB conversion, which is the
 * space the PNG's bytes are in.
 * @param {string} css
 */
function parseColor(css) {
	const c = new THREE.Color(0x000000);
	try {
		c.set(css.trim() || '#000000');
	} catch {
		c.set('#000000');
	}
	const hex = c.getHexString();
	return [
		parseInt(hex.slice(0, 2), 16),
		parseInt(hex.slice(2, 4), 16),
		parseInt(hex.slice(4, 6), 16)
	];
}
