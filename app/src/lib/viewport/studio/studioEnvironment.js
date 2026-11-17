/**
 * Studio HDR Lighting Environments (`specs/studio_pbr_renderer.md` §1, §2.1, I9–I11).
 *
 * Provides both:
 *   1. Zero-latency procedural HDR 3-point softbox rigs (`studio-softbox`,
 *      `daylight-workshop`, `darkroom-rim`), and
 *   2. Self-hosted CC0 1K (1024×512) Radiance `.hdr` environment maps
 *      (`hdri-outdoor-sunny`, `hdri-outdoor-plaza`, `hdri-workbench`,
 *      `hdri-machine-shop`, `hdri-office`) plus user-loaded `.hdr` / `.exr` files.
 *
 * Every environment is baked into a prefiltered mipmapped radiance cubemap (`PMREM`)
 * in Waffle Iron's right-handed Z-up world frame (`+Z` sky/ceiling, `-Z` floor,
 * `det = +1`) and cached per `(renderer, presetId)`.
 */
import * as THREE from 'three';
import { RGBELoader } from 'three/examples/jsm/loaders/RGBELoader.js';
import { EXRLoader } from 'three/examples/jsm/loaders/EXRLoader.js';

export const STUDIO_ENVIRONMENTS = Object.freeze([
	// Curated CC0 1K photographic HDRIs (`app/static/hdri/ATTRIBUTION.md`)
	{
		id: 'hdri-outdoor-sunny',
		label: 'Outdoor Sunny Park',
		group: 'HDRI Environments',
		kind: 'hdri',
		url: '/hdri/outdoor_sunny.hdr',
		exposure: 2.35,
		baseYawDeg: -35
	},
	{
		id: 'hdri-outdoor-plaza',
		label: 'Outdoor Overcast Plaza',
		group: 'HDRI Environments',
		kind: 'hdri',
		url: '/hdri/outdoor_plaza.hdr',
		exposure: 1.25,
		baseYawDeg: -110
	},
	{
		id: 'hdri-workbench',
		label: 'Indoor Workbench',
		group: 'HDRI Environments',
		kind: 'hdri',
		url: '/hdri/indoor_workbench.hdr',
		exposure: 1.25,
		baseYawDeg: -75
	},
	{
		id: 'hdri-machine-shop',
		label: 'Machine Shop',
		group: 'HDRI Environments',
		kind: 'hdri',
		url: '/hdri/machine_shop.hdr',
		exposure: 1.55,
		baseYawDeg: -45
	},
	{
		id: 'hdri-office',
		label: 'Indoor Office',
		group: 'HDRI Environments',
		kind: 'hdri',
		url: '/hdri/indoor_office.hdr',
		exposure: 1.45,
		baseYawDeg: -85
	},
	// Zero-latency procedural studio softbox rigs
	{
		id: 'studio-softbox',
		label: 'Studio Softbox (3-Point)',
		group: 'Studio Rigs',
		kind: 'procedural'
	},
	{
		id: 'daylight-workshop',
		label: 'Daylight Workshop (Rig)',
		group: 'Studio Rigs',
		kind: 'procedural'
	},
	{
		id: 'darkroom-rim',
		label: 'Darkroom Rim Contrast',
		group: 'Studio Rigs',
		kind: 'procedural'
	}
]);

const PRESET_BY_ID = new Map(STUDIO_ENVIRONMENTS.map((e) => [e.id, e]));

/**
 * Whether `presetId` refers to a photographic HDRI (built-in or session custom)
 * rather than a synthetic softbox rig.
 * @param {string} presetId
 */
export function isHdriEnvironment(presetId) {
	if (presetId === 'custom') return customEnvRecord !== null;
	return PRESET_BY_ID.get(presetId)?.kind === 'hdri';
}

/**
 * Compute Three.js CubeUV atlas sampling constants (`CUBEUV_TEXEL_WIDTH`,
 * `CUBEUV_TEXEL_HEIGHT`, `CUBEUV_MAX_MIP`) for a PMREM `WebGLRenderTarget`.
 * Matches `generateCubeUVSize` in Three.js `WebGLProgram.js`.
 *
 * @param {THREE.WebGLRenderTarget | null | undefined} envTarget
 * @returns {THREE.Vector3}
 */
export function getEnvCubeUVParams(envTarget) {
	const imageHeight = envTarget?.height || 1024;
	const maxMip = Math.log2(imageHeight) - 2;
	const texelHeight = 1.0 / imageHeight;
	const texelWidth = 1.0 / (3 * Math.max(Math.pow(2, maxMip), 7 * 16));
	return new THREE.Vector3(texelWidth, texelHeight, maxMip);
}

// ---------------------------------------------------------------------------
// Procedural Softbox & Cyclorama Builders
// ---------------------------------------------------------------------------

function makeSoftboxPanel({ width, height, position, color, intensity }) {
	const geo = new THREE.PlaneGeometry(width, height);
	const hdrColor = new THREE.Color(color).multiplyScalar(intensity);
	const mat = new THREE.ShaderMaterial({
		side: THREE.DoubleSide,
		uniforms: {
			uColor: { value: hdrColor }
		},
		vertexShader: /* glsl */ `
			varying vec2 vUv;
			void main() {
				vUv = uv;
				gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
			}
		`,
		fragmentShader: /* glsl */ `
			uniform vec3 uColor;
			varying vec2 vUv;
			void main() {
				vec2 d = abs(vUv * 2.0 - 1.0);
				float edge = 1.0 - smoothstep(0.55, 1.0, max(d.x, d.y));
				float core = 0.65 + 0.35 * (1.0 - dot(d, d) * 0.5);
				gl_FragColor = vec4(uColor * edge * core, 1.0);
			}
		`
	});
	const mesh = new THREE.Mesh(geo, mat);
	mesh.position.set(position[0], position[1], position[2]);
	mesh.lookAt(0, 0, 0);
	return { mesh, geo, mat };
}

function makeCycloramaDome({ zenithColor, horizonColor, nadirColor }) {
	const geo = new THREE.SphereGeometry(40, 32, 24);
	const mat = new THREE.ShaderMaterial({
		side: THREE.BackSide,
		depthWrite: false,
		uniforms: {
			uZenith: { value: new THREE.Color(zenithColor) },
			uHorizon: { value: new THREE.Color(horizonColor) },
			uNadir: { value: new THREE.Color(nadirColor) }
		},
		vertexShader: /* glsl */ `
			varying vec3 vWorldDir;
			void main() {
				vWorldDir = normalize(position);
				gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
			}
		`,
		fragmentShader: /* glsl */ `
			uniform vec3 uZenith;
			uniform vec3 uHorizon;
			uniform vec3 uNadir;
			varying vec3 vWorldDir;
			void main() {
				float z = clamp(vWorldDir.z, -1.0, 1.0);
				vec3 col = z >= 0.0
					? mix(uHorizon, uZenith, pow(z, 0.65))
					: mix(uHorizon, uNadir, pow(-z, 0.75));
				gl_FragColor = vec4(col, 1.0);
			}
		`
	});
	const mesh = new THREE.Mesh(geo, mat);
	return { mesh, geo, mat };
}

/**
 * Z-up right-handed equirectangular sky sphere (Invariant I9):
 * Maps Waffle Iron world `d = (x, y, z)` (`+Z` overhead sky, `-Z` floor, `-Y` front)
 * onto standard Y-up equirectangular coordinates `(u, v)` with `det = +1` (no mirror flip).
 */
function makeZUpEquirectDome(hdrTexture, baseYawDeg = 0) {
	const geo = new THREE.SphereGeometry(40, 64, 32);
	const baseYawRad = (baseYawDeg * Math.PI) / 180;
	const mat = new THREE.ShaderMaterial({
		side: THREE.BackSide,
		depthWrite: false,
		uniforms: {
			tEquirect: { value: hdrTexture },
			uBaseYaw: { value: baseYawRad }
		},
		vertexShader: /* glsl */ `
			varying vec3 vWorldDir;
			void main() {
				vWorldDir = normalize(position);
				gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
			}
		`,
		fragmentShader: /* glsl */ `
			uniform sampler2D tEquirect;
			uniform float uBaseYaw;
			varying vec3 vWorldDir;
			void main() {
				vec3 d = normalize(vWorldDir);
				// Right-handed Z-up (+X right, -Y front, +Z up) -> equirect (phi, z):
				// det([[1, 0, 0], [0, 0, 1], [0, -1, 0]]) = +1.
				float phi = atan(-d.y, d.x) + uBaseYaw;
				float u = phi * 0.15915494309189535 + 0.5;
				float v = clamp(asin(clamp(d.z, -1.0, 1.0)) * 0.3183098861837907 + 0.5, 0.0, 1.0);
				gl_FragColor = vec4(texture2D(tEquirect, vec2(u, v)).rgb, 1.0);
			}
		`
	});
	const mesh = new THREE.Mesh(geo, mat);
	return { mesh, geo, mat };
}

function proceduralPresetConfig(presetId) {
	switch (presetId) {
		case 'daylight-workshop':
			return {
				dome: {
					zenithColor: new THREE.Color(0.95, 1.12, 1.38),
					horizonColor: new THREE.Color(0.78, 0.82, 0.86),
					nadirColor: new THREE.Color(0.28, 0.27, 0.25)
				},
				panels: [
					{ width: 24, height: 24, position: [2, -4, 18], color: 0xfffaf0, intensity: 4.8 },
					{ width: 18, height: 12, position: [-15, -12, 8], color: 0xd6ebff, intensity: 3.2 },
					{ width: 16, height: 10, position: [14, 12, 6], color: 0xffeed8, intensity: 2.6 },
					{ width: 22, height: 6, position: [12, -14, -2], color: 0xeef4ff, intensity: 2.2 }
				]
			};
		case 'darkroom-rim':
			return {
				dome: {
					zenithColor: new THREE.Color(0.26, 0.30, 0.38),
					horizonColor: new THREE.Color(0.20, 0.22, 0.26),
					nadirColor: new THREE.Color(0.08, 0.08, 0.10)
				},
				panels: [
					{ width: 22, height: 7, position: [4, -6, 17], color: 0xffffff, intensity: 7.5 },
					{ width: 10, height: 18, position: [-16, 8, 7], color: 0x8ec5ff, intensity: 5.5 },
					{ width: 12, height: 16, position: [15, -13, 6], color: 0xffd2a6, intensity: 5.0 },
					{ width: 18, height: 8, position: [-6, -16, -3], color: 0xd0daea, intensity: 2.0 }
				]
			};
		case 'studio-softbox':
		default:
			return {
				dome: {
					zenithColor: new THREE.Color(0.72, 0.78, 0.88),
					horizonColor: new THREE.Color(0.55, 0.58, 0.64),
					nadirColor: new THREE.Color(0.22, 0.23, 0.25)
				},
				panels: [
					{ width: 22, height: 18, position: [6, -7, 17], color: 0xfffbf4, intensity: 5.6 },
					{ width: 18, height: 14, position: [-15, -11, 8], color: 0xdcebff, intensity: 3.4 },
					{ width: 18, height: 14, position: [-11, 12, 11], color: 0xfff2e2, intensity: 4.2 },
					{ width: 16, height: 12, position: [14, 12, -4], color: 0xf5f8ff, intensity: 3.2 },
					{ width: 18, height: 10, position: [-10, -14, -5], color: 0xeef3fa, intensity: 2.6 }
				]
			};
	}
}

// ---------------------------------------------------------------------------
// Radiance RGBE (.hdr) & OpenEXR (.exr) Decoding + Firefly Shoulder (I10)
// ---------------------------------------------------------------------------

/**
 * Parse an `.hdr` or `.exr` ArrayBuffer into a normalized `HalfFloatType`
 * equirectangular `DataTexture` with row `height - 1` = Zenith (`v = 1`)
 * and row `0` = Nadir (`v = 0`), independent of WebGL `UNPACK_FLIP_Y_WEBGL`.
 *
 * Applies smooth logarithmic shoulder compression above `K = 32.0` (Invariant I10)
 * so `100,000+` nit solar discs never overflow `HalfFloat` (`65504`) or ring in PMREM.
 *
 * @param {ArrayBuffer} arrayBuffer
 * @param {'hdr' | 'exr'} format
 * @param {{ exposure?: number, autoNormalize?: boolean }} [opts]
 * @returns {THREE.DataTexture}
 */
export function decodeEquirectHdrBuffer(arrayBuffer, format = 'hdr', opts = {}) {
	const isExr = format === 'exr';
	const parsed = isExr
		? new EXRLoader().setDataType(THREE.FloatType).parse(arrayBuffer)
		: new RGBELoader().setDataType(THREE.FloatType).parse(arrayBuffer);

	const w = parsed.width;
	const h = parsed.height;
	const src = /** @type {Float32Array} */ (parsed.data);
	const stride = parsed.format === THREE.RedFormat ? 1 : 4;
	// RGBELoader.parse stores Zenith at srcY = 0; EXRLoader.parse stores Zenith at srcY = h - 1.
	const flipRows = !isExr;

	let exposure = opts.exposure ?? 1.0;
	if (opts.autoNormalize) {
		let weightedSum = 0;
		for (let y = 0; y < h; y++) {
			const theta = ((y + 0.5) / h) * Math.PI;
			const dOmega = Math.sin(theta) * (2 * Math.PI / w) * (Math.PI / h);
			const rowOff = y * w * stride;
			for (let x = 0; x < w; x++) {
				const idx = rowOff + x * stride;
				const r = src[idx] || 0;
				const g = stride > 1 ? src[idx + 1] || 0 : r;
				const b = stride > 1 ? src[idx + 2] || 0 : r;
				const lum = Math.max(0, 0.2126 * r + 0.7152 * g + 0.0722 * b);
				weightedSum += Math.min(lum, 25.0) * dOmega;
			}
		}
		const clampMean = weightedSum / (4 * Math.PI);
		if (clampMean > 1e-5 && (clampMean < 0.25 || clampMean > 1.8)) {
			exposure *= Math.min(8.0, Math.max(0.1, 0.78 / clampMean));
		}
	}

	const KNEE = 32.0;
	const halfData = new Uint16Array(w * h * 4);
	const halfOne = THREE.DataUtils.toHalfFloat(1.0);

	for (let dstY = 0; dstY < h; dstY++) {
		const srcY = flipRows ? h - 1 - dstY : dstY;
		const srcRow = srcY * w * stride;
		const dstRow = dstY * w * 4;
		for (let x = 0; x < w; x++) {
			const si = srcRow + x * stride;
			let r = Math.max(0, (Number.isFinite(src[si]) ? src[si] : 0) * exposure);
			let g = stride > 1 ? Math.max(0, (Number.isFinite(src[si + 1]) ? src[si + 1] : 0) * exposure) : r;
			let b = stride > 1 ? Math.max(0, (Number.isFinite(src[si + 2]) ? src[si + 2] : 0) * exposure) : r;

			const peak = Math.max(r, g, b);
			if (peak > KNEE) {
				const compressed = KNEE + 8.0 * Math.log(1.0 + (peak - KNEE) / 8.0);
				const k = compressed / peak;
				r *= k;
				g *= k;
				b *= k;
			}

			const di = dstRow + x * 4;
			halfData[di] = THREE.DataUtils.toHalfFloat(Math.min(r, 65000));
			halfData[di + 1] = THREE.DataUtils.toHalfFloat(Math.min(g, 65000));
			halfData[di + 2] = THREE.DataUtils.toHalfFloat(Math.min(b, 65000));
			halfData[di + 3] = halfOne;
		}
	}

	const tex = new THREE.DataTexture(halfData, w, h, THREE.RGBAFormat, THREE.HalfFloatType);
	tex.colorSpace = THREE.LinearSRGBColorSpace;
	tex.wrapS = THREE.RepeatWrapping;
	tex.wrapT = THREE.ClampToEdgeWrapping;
	tex.minFilter = THREE.LinearFilter;
	tex.magFilter = THREE.LinearFilter;
	tex.generateMipmaps = false;
	tex.flipY = false;
	tex.needsUpdate = true;
	return tex;
}

// ---------------------------------------------------------------------------
// Caching, Async HDRI Loader & Session Custom HDRI
// ---------------------------------------------------------------------------

/** @type {WeakMap<THREE.WebGLRenderer, Map<string, THREE.WebGLRenderTarget>>} */
const ENV_CACHE = new WeakMap();

/** Shared decoded equirectangular DataTextures across renderers by preset id. @type {Map<string, Promise<THREE.DataTexture>>} */
const HDRI_TEX_PROMISES = new Map();

/** Session-scoped custom uploaded HDRI. @type {{ name: string, tex: THREE.DataTexture, version: number } | null} */
let customEnvRecord = null;
let customEnvCounter = 0;
const customListeners = new Set();

export function hasCustomStudioEnvironment() {
	return customEnvRecord !== null;
}

export function getCustomStudioEnvironmentName() {
	return customEnvRecord?.name ?? null;
}

export function getCustomStudioEnvironmentVersion() {
	return customEnvRecord?.version ?? 0;
}

/** @param {() => void} fn */
export function subscribeCustomStudioEnvironment(fn) {
	customListeners.add(fn);
	return () => customListeners.delete(fn);
}

/**
 * Decode a user-selected `.hdr` or `.exr` `File` (or `{ name, arrayBuffer }`)
 * and register it as the session `'custom'` Studio environment.
 *
 * @param {{ name: string, arrayBuffer: () => Promise<ArrayBuffer> }} file
 */
export async function setCustomStudioEnvironmentFromFile(file) {
	const name = String(file?.name || 'Custom HDRI');
	const ext = name.toLowerCase().endsWith('.exr') ? 'exr' : 'hdr';
	const buf = await file.arrayBuffer();
	const tex = decodeEquirectHdrBuffer(buf, ext, { exposure: 1.0, autoNormalize: true });
	if (customEnvRecord?.tex) {
		customEnvRecord.tex.dispose();
	}
	customEnvCounter += 1;
	customEnvRecord = { name, tex, version: customEnvCounter };
	for (const fn of customListeners) {
		fn();
	}
	return { id: 'custom', name, version: customEnvCounter };
}

function getCacheMap(renderer) {
	let byPreset = ENV_CACHE.get(renderer);
	if (!byPreset) {
		byPreset = new Map();
		ENV_CACHE.set(renderer, byPreset);
	}
	return byPreset;
}

function cacheKeyFor(presetId) {
	if (presetId === 'custom') {
		return customEnvRecord ? `custom:${customEnvRecord.version}` : 'studio-softbox';
	}
	return PRESET_BY_ID.has(presetId) ? presetId : 'studio-softbox';
}

function bakeSceneToPmrem(renderer, envScene, sigma = 0) {
	const prevTarget = renderer.getRenderTarget();
	const prevToneMapping = renderer.toneMapping;
	renderer.toneMapping = THREE.NoToneMapping;

	const pmrem = new THREE.PMREMGenerator(renderer);
	pmrem.compileCubemapShader();
	const envTarget = pmrem.fromScene(envScene, sigma);
	pmrem.dispose();

	renderer.toneMapping = prevToneMapping;
	renderer.setRenderTarget(prevTarget);
	return envTarget;
}

function buildProceduralTarget(renderer, presetId) {
	const envScene = new THREE.Scene();
	const cfg = proceduralPresetConfig(presetId);
	const disposables = [];

	const dome = makeCycloramaDome(cfg.dome);
	envScene.add(dome.mesh);
	disposables.push(dome);

	for (const p of cfg.panels) {
		const panel = makeSoftboxPanel(p);
		envScene.add(panel.mesh);
		disposables.push(panel);
	}

	const envTarget = bakeSceneToPmrem(renderer, envScene, 0.025);
	for (const d of disposables) {
		d.geo.dispose();
		d.mat.dispose();
	}
	return envTarget;
}

function buildEquirectTarget(renderer, hdrTex, baseYawDeg = 0) {
	const envScene = new THREE.Scene();
	const dome = makeZUpEquirectDome(hdrTex, baseYawDeg);
	envScene.add(dome.mesh);
	const envTarget = bakeSceneToPmrem(renderer, envScene, 0.0);
	dome.geo.dispose();
	dome.mat.dispose();
	return envTarget;
}

function fetchPresetHdrTexture(preset) {
	let pending = HDRI_TEX_PROMISES.get(preset.id);
	if (pending) return pending;

	pending = fetch(preset.url)
		.then((res) => {
			if (!res.ok) {
				throw new Error(`Failed to fetch HDRI ${preset.url} (HTTP ${res.status})`);
			}
			return res.arrayBuffer();
		})
		.then((buf) =>
			decodeEquirectHdrBuffer(buf, 'hdr', {
				exposure: preset.exposure ?? 1.0,
				autoNormalize: false
			})
		)
		.catch((err) => {
			HDRI_TEX_PROMISES.delete(preset.id);
			throw err;
		});

	HDRI_TEX_PROMISES.set(preset.id, pending);
	return pending;
}

/**
 * Synchronous lookup: returns the cached `WebGLRenderTarget` if already built
 * (or builds immediately when `presetId` is procedural or an already-uploaded custom HDRI),
 * otherwise returns `null` so the caller can keep a fallback PMREM while
 * `loadStudioEnvironmentTarget` resolves.
 *
 * @param {THREE.WebGLRenderer} renderer
 * @param {string} presetId
 * @returns {THREE.WebGLRenderTarget | null}
 */
export function getStudioEnvironmentTarget(renderer, presetId = 'studio-softbox') {
	const byPreset = getCacheMap(renderer);
	const key = cacheKeyFor(presetId);
	const cached = byPreset.get(key);
	if (cached) return cached;

	if (presetId === 'custom' && customEnvRecord) {
		const target = buildEquirectTarget(renderer, customEnvRecord.tex, 0);
		byPreset.set(key, target);
		return target;
	}

	const meta = PRESET_BY_ID.get(key);
	if (!meta || meta.kind === 'procedural') {
		const procKey = meta ? meta.id : 'studio-softbox';
		const existingProc = byPreset.get(procKey);
		if (existingProc) return existingProc;
		const target = buildProceduralTarget(renderer, procKey);
		byPreset.set(procKey, target);
		return target;
	}

	return null;
}

/**
 * Load (or retrieve from cache) the PMREM target for any Studio environment preset,
 * including asynchronous `.hdr` fetches.
 *
 * @param {THREE.WebGLRenderer} renderer
 * @param {string} presetId
 * @returns {Promise<{ target: THREE.WebGLRenderTarget, resolvedId: string }>}
 */
export async function loadStudioEnvironmentTarget(renderer, presetId = 'studio-softbox') {
	const sync = getStudioEnvironmentTarget(renderer, presetId);
	if (sync) {
		const resolvedId =
			presetId === 'custom'
				? customEnvRecord
					? 'custom'
					: 'studio-softbox'
				: PRESET_BY_ID.has(presetId)
					? presetId
					: 'studio-softbox';
		return { target: sync, resolvedId };
	}

	const meta = PRESET_BY_ID.get(presetId);
	if (!meta || meta.kind !== 'hdri') {
		const fallback = getStudioEnvironmentTarget(renderer, 'studio-softbox');
		return { target: fallback, resolvedId: 'studio-softbox' };
	}

	const hdrTex = await fetchPresetHdrTexture(meta);
	const byPreset = getCacheMap(renderer);
	let target = byPreset.get(meta.id);
	if (!target) {
		target = buildEquirectTarget(renderer, hdrTex, meta.baseYawDeg ?? 0);
		byPreset.set(meta.id, target);
	}
	return { target, resolvedId: meta.id };
}

/**
 * Dispose cached PMREM render targets associated with a renderer.
 * @param {THREE.WebGLRenderer} renderer
 */
export function disposeStudioEnvironments(renderer) {
	const byPreset = ENV_CACHE.get(renderer);
	if (!byPreset) return;
	for (const rt of byPreset.values()) {
		rt.dispose();
	}
	byPreset.clear();
}
