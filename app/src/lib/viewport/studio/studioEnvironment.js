/**
 * Procedural HDR Studio Lighting Rigs (`specs/studio_pbr_renderer.md` §1, §2.1).
 *
 * Generates prefiltered mipmapped radiance environment maps (`PMREM`) in
 * Waffle Iron's Z-up world frame (`+Z` overhead ceiling, `-Z` floor) with
 * zero external network fetches. Cached per `(renderer, presetId)`.
 */
import * as THREE from 'three';

export const STUDIO_ENVIRONMENTS = Object.freeze([
	{ id: 'studio-softbox', label: 'Studio Softbox (3-Point)' },
	{ id: 'daylight-workshop', label: 'Daylight Workshop' },
	{ id: 'darkroom-rim', label: 'Darkroom Rim Contrast' }
]);

/**
 * Create a soft-edged emissive studio softbox panel oriented toward the origin.
 * Uses a procedural UV falloff shader so curved CAD faces (cylinders, cones,
 * tori, fillets) display smooth continuous highlight ribbons instead of harsh
 * polygon edges.
 */
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

/**
 * Create a Z-up spherical cyclorama dome providing smooth sky/horizon/floor
 * base illumination so no surface normal ever falls into a dead black void.
 */
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
				// Waffle Iron world is Z-up: vWorldDir.z in [-1, +1]
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

function presetConfig(presetId) {
	switch (presetId) {
		case 'daylight-workshop':
			return {
				dome: {
					zenithColor: new THREE.Color(0.95, 1.12, 1.38),
					horizonColor: new THREE.Color(0.78, 0.82, 0.86),
					nadirColor: new THREE.Color(0.28, 0.27, 0.25)
				},
				panels: [
					// Large skylight overhead (+Z)
					{ width: 24, height: 24, position: [2, -4, 18], color: 0xfffaf0, intensity: 4.8 },
					// North window fill (-Y / -X)
					{ width: 18, height: 12, position: [-15, -12, 8], color: 0xd6ebff, intensity: 3.2 },
					// Warm bench bounce (+X / +Y)
					{ width: 16, height: 10, position: [14, 12, 6], color: 0xffeed8, intensity: 2.6 },
					// Low horizon strip so vertical walls catch crisp reflections
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
					// Dramatic narrow overhead strip
					{ width: 22, height: 7, position: [4, -6, 17], color: 0xffffff, intensity: 7.5 },
					// Cool rim kick left
					{ width: 10, height: 18, position: [-16, 8, 7], color: 0x8ec5ff, intensity: 5.5 },
					// Warm rim kick right-front
					{ width: 12, height: 16, position: [15, -13, 6], color: 0xffd2a6, intensity: 5.0 },
					// Subtle front-low fill card
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
					// Primary overhead key softbox (+Z, slightly front-right)
					{ width: 22, height: 18, position: [6, -7, 17], color: 0xfffbf4, intensity: 5.6 },
					// Broad front-left cool fill softbox
					{ width: 18, height: 14, position: [-15, -11, 8], color: 0xdcebff, intensity: 3.4 },
					// Back-left warm rim softbox (lights +Z reflections from iso view!)
					{ width: 18, height: 14, position: [-11, 12, 11], color: 0xfff2e2, intensity: 4.2 },
					// Right-rear kick strip (lights +X face reflections from iso view!)
					{ width: 16, height: 12, position: [14, 12, -4], color: 0xf5f8ff, intensity: 3.2 },
					// Front-low reflector card (lights -Y face reflections from iso view!)
					{ width: 18, height: 10, position: [-10, -14, -5], color: 0xeef3fa, intensity: 2.6 }
				]
			};
	}
}

/** @type {WeakMap<THREE.WebGLRenderer, Map<string, THREE.WebGLRenderTarget>>} */
const ENV_CACHE = new WeakMap();

/**
 * Build (or retrieve from cache) the prefiltered PMREM render target for the
 * given renderer and studio preset id.
 *
 * @param {THREE.WebGLRenderer} renderer
 * @param {string} presetId
 * @returns {THREE.WebGLRenderTarget}
 */
export function getStudioEnvironmentTarget(renderer, presetId = 'studio-softbox') {
	let byPreset = ENV_CACHE.get(renderer);
	if (!byPreset) {
		byPreset = new Map();
		ENV_CACHE.set(renderer, byPreset);
	}
	const key = STUDIO_ENVIRONMENTS.some((e) => e.id === presetId) ? presetId : 'studio-softbox';
	const cached = byPreset.get(key);
	if (cached) return cached;

	const envScene = new THREE.Scene();
	const cfg = presetConfig(key);
	const disposables = [];

	const dome = makeCycloramaDome(cfg.dome);
	envScene.add(dome.mesh);
	disposables.push(dome);

	for (const p of cfg.panels) {
		const panel = makeSoftboxPanel(p);
		envScene.add(panel.mesh);
		disposables.push(panel);
	}

	// Preserve renderer render-target / toneMapping state across PMREM generation.
	const prevTarget = renderer.getRenderTarget();
	const prevToneMapping = renderer.toneMapping;
	renderer.toneMapping = THREE.NoToneMapping;

	const pmrem = new THREE.PMREMGenerator(renderer);
	pmrem.compileCubemapShader();
	const envTarget = pmrem.fromScene(envScene, 0.025);

	pmrem.dispose();
	for (const d of disposables) {
		d.geo.dispose();
		d.mat.dispose();
	}

	renderer.toneMapping = prevToneMapping;
	renderer.setRenderTarget(prevTarget);

	byPreset.set(key, envTarget);
	return envTarget;
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
