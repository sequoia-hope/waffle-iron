/**
 * Studio Lighting Pipeline Shaders (`specs/studio_pbr_renderer.md` §1, §4 I4–I7).
 *
 * Implements:
 *   - Linear view-space G-Buffer (`normal.xyz`, `linearDepth`) supporting both
 *     `OrthographicCamera` and `PerspectiveCamera`, `logarithmicDepthBuffer`,
 *     section clipping planes, and an `RGBA8` octahedral/16-bit fallback when
 *     `EXT_color_buffer_float` is absent.
 *   - Horizon-based Ground Truth Ambient Occlusion (GTAO, Jimenez et al. 2016)
 *     with model-scale-adaptive world radius + 2-pass bilateral depth/normal blur.
 *   - View-space Screen-Space Reflections (SSR) with binary hit refinement.
 *   - Halton (2, 3) sub-pixel progressive accumulation + premultiplied-alpha
 *     Khronos PBR Neutral / AgX composite output pass.
 */
import * as THREE from 'three';

/** Radical inverse in `base` for low-discrepancy Halton sub-pixel jitter. */
function halton(index, base) {
	let result = 0;
	let f = 1 / base;
	let i = index;
	while (i > 0) {
		result += f * (i % base);
		i = Math.floor(i / base);
		f /= base;
	}
	return result;
}

/**
 * Sub-pixel camera jitter in `[-0.375, +0.375]` pixels for sample `index`.
 * Sample 0 is unjittered `(0, 0)` so interactive single-frame orbits are crisp.
 * @param {number} sampleIndex
 * @returns {{ x: number, y: number }}
 */
export function haltonSubpixelOffset(sampleIndex) {
	if (sampleIndex <= 0) return { x: 0, y: 0 };
	return {
		x: (halton(sampleIndex, 2) - 0.5) * 0.75,
		y: (halton(sampleIndex, 3) - 0.5) * 0.75
	};
}

/**
 * Extract projection reconstruction parameters from any Three.js camera
 * (`OrthographicCamera` or `PerspectiveCamera`), honoring active `setViewOffset`.
 *
 * @param {THREE.Camera} cam
 */
export function extractCameraProjParams(cam) {
	const e = cam.projectionMatrix.elements;
	const isOrtho = /** @type {any} */ (cam).isOrthographicCamera === true;
	return {
		isOrtho: isOrtho ? 1.0 : 0.0,
		near: /** @type {any} */ (cam).near ?? 0.01,
		far: /** @type {any} */ (cam).far ?? 5000,
		// (proj[0], proj[5], ortho ? proj[12] : proj[8], ortho ? proj[13] : proj[9])
		projParams: new THREE.Vector4(e[0], e[5], isOrtho ? e[12] : e[8], isOrtho ? e[13] : e[9])
	};
}

const COMMON_GBUFFER_DECODE = /* glsl */ `
uniform float uPackedGBuffer;
uniform float uNear;
uniform float uFar;
uniform float uIsOrtho;
uniform vec4 uProjParams;

vec2 octWrap(vec2 v) {
	return (1.0 - abs(v.yx)) * vec2(v.x >= 0.0 ? 1.0 : -1.0, v.y >= 0.0 ? 1.0 : -1.0);
}

vec3 octDecode(vec2 f) {
	f = f * 2.0 - 1.0;
	vec3 n = vec3(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));
	float t = clamp(-n.z, 0.0, 1.0);
	n.xy += vec2(n.x >= 0.0 ? -t : t, n.y >= 0.0 ? -t : t);
	return normalize(n);
}

// Returns vec4(unitViewNormal.xyz, linearDepth); linearDepth <= 0.0 means empty background.
vec4 readGBuffer(sampler2D tGBuffer, vec2 uv) {
	vec4 raw = texture2D(tGBuffer, uv);
	if (uPackedGBuffer < 0.5) {
		if (raw.a <= 0.0) return vec4(0.0, 0.0, 1.0, 0.0);
		vec3 n = normalize(raw.rgb * 2.0 - 1.0);
		return vec4(n, raw.a);
	} else {
		if (raw.b == 0.0 && raw.a == 0.0) return vec4(0.0, 0.0, 1.0, 0.0);
		float dNorm = (raw.b * 255.0 + raw.a) / 256.0;
		float linearDepth = mix(uNear, uFar, dNorm);
		vec3 n = octDecode(raw.rg);
		return vec4(n, linearDepth);
	}
}

// Invariant I4: exact view-space 3D position for both Orthographic and Perspective cameras.
vec3 reconstructViewPos(vec2 uv, float linearDepth) {
	vec2 ndc = uv * 2.0 - 1.0;
	if (uIsOrtho > 0.5) {
		return vec3(
			(ndc.x - uProjParams.z) / uProjParams.x,
			(ndc.y - uProjParams.w) / uProjParams.y,
			-linearDepth
		);
	} else {
		return vec3(
			(ndc.x + uProjParams.z) * linearDepth / uProjParams.x,
			(ndc.y + uProjParams.w) * linearDepth / uProjParams.y,
			-linearDepth
		);
	}
}

vec2 projectViewPosToUv(vec3 P) {
	vec2 ndc;
	if (uIsOrtho > 0.5) {
		ndc = vec2(
			uProjParams.x * P.x + uProjParams.z,
			uProjParams.y * P.y + uProjParams.w
		);
	} else {
		float invZ = 1.0 / max(-P.z, 1e-6);
		ndc = vec2(
			uProjParams.x * P.x * invZ - uProjParams.z,
			uProjParams.y * P.y * invZ - uProjParams.w
		);
	}
	return ndc * 0.5 + 0.5;
}
`;

const FULLSCREEN_VERTEX = /* glsl */ `
varying vec2 vUv;
void main() {
	vUv = uv;
	gl_Position = vec4(position.xy, 0.0, 1.0);
}
`;

/**
 * Create the linear-depth + view-normal G-buffer material rendered on opaque CAD bodies.
 * @param {boolean} packed
 */
export function createGBufferMaterial(packed = false) {
	return new THREE.ShaderMaterial({
		side: THREE.DoubleSide,
		clipping: true,
		blending: THREE.NoBlending,
		uniforms: {
			uPackedGBuffer: { value: packed ? 1.0 : 0.0 },
			uNear: { value: 0.01 },
			uFar: { value: 5000.0 }
		},
		vertexShader: /* glsl */ `
			#include <common>
			#include <clipping_planes_pars_vertex>
			#include <logdepthbuf_pars_vertex>
			varying vec3 vViewPos;
			varying vec3 vViewNormal;
			void main() {
				vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
				vViewPos = mvPosition.xyz;
				vViewNormal = normalize(normalMatrix * normal);
				gl_Position = projectionMatrix * mvPosition;
				#include <logdepthbuf_vertex>
				#include <clipping_planes_vertex>
			}
		`,
		fragmentShader: /* glsl */ `
			#include <common>
			#include <clipping_planes_pars_fragment>
			#include <logdepthbuf_pars_fragment>
			uniform float uPackedGBuffer;
			uniform float uNear;
			uniform float uFar;
			varying vec3 vViewPos;
			varying vec3 vViewNormal;

			vec2 octEncode(vec3 n) {
				n /= (abs(n.x) + abs(n.y) + abs(n.z));
				vec2 wrapped = (1.0 - abs(n.yx)) * vec2(n.x >= 0.0 ? 1.0 : -1.0, n.y >= 0.0 ? 1.0 : -1.0);
				vec2 oct = n.z >= 0.0 ? n.xy : wrapped;
				return oct * 0.5 + 0.5;
			}

			void main() {
				#include <clipping_planes_fragment>
				#include <logdepthbuf_fragment>
				vec3 n = normalize(vViewNormal) * (gl_FrontFacing ? 1.0 : -1.0);
				float linearDepth = max(-vViewPos.z, 1e-6);
				if (uPackedGBuffer < 0.5) {
					gl_FragColor = vec4(n * 0.5 + 0.5, linearDepth);
				} else {
					vec2 oct = octEncode(n);
					float dNorm = clamp((linearDepth - uNear) / max(uFar - uNear, 1e-4), 1.0 / 65535.0, 1.0);
					float scaled = dNorm * 255.999;
					float hi = floor(scaled) / 255.0;
					float lo = fract(scaled);
					gl_FragColor = vec4(oct, hi, lo);
				}
			}
		`
	});
}

/**
 * Horizon-based GTAO pass (Jimenez et al. 2016).
 */
export function createGTAOMaterial(packed = false) {
	return new THREE.ShaderMaterial({
		depthTest: false,
		depthWrite: false,
		blending: THREE.NoBlending,
		uniforms: {
			tGBuffer: { value: null },
			uPackedGBuffer: { value: packed ? 1.0 : 0.0 },
			uNear: { value: 0.01 },
			uFar: { value: 5000.0 },
			uIsOrtho: { value: 1.0 },
			uProjParams: { value: new THREE.Vector4(1, 1, 0, 0) },
			uResolution: { value: new THREE.Vector2(512, 512) },
			uWorldRadius: { value: 0.05 },
			uFrameOffset: { value: 0.0 }
		},
		vertexShader: FULLSCREEN_VERTEX,
		fragmentShader:
			COMMON_GBUFFER_DECODE +
			/* glsl */ `
			uniform sampler2D tGBuffer;
			uniform vec2 uResolution;
			uniform float uWorldRadius;
			uniform float uFrameOffset;
			varying vec2 vUv;

			float interleavedGradientNoise(vec2 pixel) {
				return fract(52.9829189 * fract(0.06711056 * pixel.x + 0.00583715 * pixel.y));
			}

			void main() {
				vec4 centerG = readGBuffer(tGBuffer, vUv);
				float centerDepth = centerG.a;
				if (centerDepth <= 0.0) {
					gl_FragColor = vec4(1.0);
					return;
				}
				vec3 N = centerG.xyz;
				vec3 P = reconstructViewPos(vUv, centerDepth);

				// Project world-space AO radius into UV space (exact in both Ortho & Perspective!)
				float uvRadiusY = uIsOrtho > 0.5
					? 0.5 * uWorldRadius * abs(uProjParams.y)
					: 0.5 * uWorldRadius * abs(uProjParams.y) / max(centerDepth, 1e-4);
				float aspect = uResolution.x / max(uResolution.y, 1.0);
				float maxPxRadius = clamp(uvRadiusY * uResolution.y, 5.0, 56.0);
				vec2 maxUvRadius = vec2(maxPxRadius / max(uResolution.x, 1.0), maxPxRadius / max(uResolution.y, 1.0));

				vec2 pixel = floor(vUv * uResolution);
				float noise = interleavedGradientNoise(pixel + vec2(uFrameOffset * 17.0, uFrameOffset * 31.0));
				float jitterStep = fract(noise + uFrameOffset * 0.61803398875);
				float baseAngle = (noise + uFrameOffset * 0.25) * 3.14159265359;

				const int DIRECTIONS = 4;
				const int STEPS = 6;
				float totalVis = 0.0;
				float invMaxDist = 1.0 / max(uWorldRadius * 1.35, 1e-6);

				for (int d = 0; d < DIRECTIONS; d++) {
					float angle = baseAngle + (float(d) / float(DIRECTIONS)) * 3.14159265359;
					vec2 dir = vec2(cos(angle), sin(angle));

					float hPos = 0.0;
					float hNeg = 0.0;

					for (int s = 0; s < STEPS; s++) {
						float fracStep = (float(s) + 0.25 + 0.75 * jitterStep) / float(STEPS);
						// Quadratic step distribution concentrates precision in tight CAD corners
						float t = mix(fracStep, fracStep * fracStep, 0.45);
						vec2 offsetUv = dir * maxUvRadius * t;

						vec2 uv1 = vUv + offsetUv;
						if (uv1.x > 0.001 && uv1.x < 0.999 && uv1.y > 0.001 && uv1.y < 0.999) {
							vec4 g1 = readGBuffer(tGBuffer, uv1);
							if (g1.a > 0.0) {
								vec3 delta1 = reconstructViewPos(uv1, g1.a) - P;
								float dist1 = length(delta1);
								if (dist1 > 1e-6) {
									float sinH1 = dot(N, delta1 / dist1) - 0.03;
									float falloff1 = clamp(1.0 - (dist1 * invMaxDist) * (dist1 * invMaxDist), 0.0, 1.0);
									hPos = max(hPos, clamp(sinH1, 0.0, 1.0) * falloff1);
								}
							}
						}

						vec2 uv2 = vUv - offsetUv;
						if (uv2.x > 0.001 && uv2.x < 0.999 && uv2.y > 0.001 && uv2.y < 0.999) {
							vec4 g2 = readGBuffer(tGBuffer, uv2);
							if (g2.a > 0.0) {
								vec3 delta2 = reconstructViewPos(uv2, g2.a) - P;
								float dist2 = length(delta2);
								if (dist2 > 1e-6) {
									float sinH2 = dot(N, delta2 / dist2) - 0.03;
									float falloff2 = clamp(1.0 - (dist2 * invMaxDist) * (dist2 * invMaxDist), 0.0, 1.0);
									hNeg = max(hNeg, clamp(sinH2, 0.0, 1.0) * falloff2);
								}
							}
						}
					}

					// Horizon integral over the slice hemisphere
					float sliceVis = 1.0 - 0.5 * (hPos * hPos * 1.15 + hNeg * hNeg * 1.15);
					totalVis += clamp(sliceVis, 0.05, 1.0);
				}

				float ao = pow(clamp(totalVis / float(DIRECTIONS), 0.05, 1.0), 1.35);
				gl_FragColor = vec4(ao, ao, ao, 1.0);
			}
		`
	});
}

/**
 * Edge-preserving separable bilateral filter (depth + normal weighted) for GTAO.
 */
export function createBilateralBlurMaterial(packed = false) {
	return new THREE.ShaderMaterial({
		depthTest: false,
		depthWrite: false,
		blending: THREE.NoBlending,
		uniforms: {
			tAO: { value: null },
			tGBuffer: { value: null },
			uPackedGBuffer: { value: packed ? 1.0 : 0.0 },
			uNear: { value: 0.01 },
			uFar: { value: 5000.0 },
			uIsOrtho: { value: 1.0 },
			uProjParams: { value: new THREE.Vector4(1, 1, 0, 0) },
			uTexelStep: { value: new THREE.Vector2(1 / 512, 0) },
			uDepthSigma: { value: 0.02 }
		},
		vertexShader: FULLSCREEN_VERTEX,
		fragmentShader:
			COMMON_GBUFFER_DECODE +
			/* glsl */ `
			uniform sampler2D tAO;
			uniform sampler2D tGBuffer;
			uniform vec2 uTexelStep;
			uniform float uDepthSigma;
			varying vec2 vUv;

			void main() {
				vec4 cG = readGBuffer(tGBuffer, vUv);
				float cAO = texture2D(tAO, vUv).r;
				if (cG.a <= 0.0) {
					gl_FragColor = vec4(1.0);
					return;
				}
				float sum = cAO;
				float wSum = 1.0;
				float invSigma = 1.0 / max(uDepthSigma, 1e-5);

				for (int i = -3; i <= 3; i++) {
					if (i == 0) continue;
					vec2 sUv = vUv + uTexelStep * float(i);
					vec4 sG = readGBuffer(tGBuffer, sUv);
					if (sG.a <= 0.0) continue;
					float spatialW = exp(-float(i * i) * 0.22);
					float normalW = pow(max(0.0, dot(cG.xyz, sG.xyz)), 8.0);
					float depthDiff = abs(cG.a - sG.a) * invSigma;
					float depthW = exp(-depthDiff * depthDiff);
					float w = spatialW * normalW * depthW;
					sum += texture2D(tAO, sUv).r * w;
					wSum += w;
				}
				float outAO = sum / max(wSum, 1e-5);
				gl_FragColor = vec4(outAO, outAO, outAO, 1.0);
			}
		`
	});
}

/**
 * View-space Screen-Space Reflection (SSR) ray-marching pass.
 * Supports both Orthographic (`I = vec3(0, 0, -1)`) and Perspective (`I = normalize(P)`).
 */
export function createSSRMaterial(packed = false) {
	return new THREE.ShaderMaterial({
		depthTest: false,
		depthWrite: false,
		blending: THREE.NoBlending,
		uniforms: {
			tGBuffer: { value: null },
			tColor: { value: null },
			uPackedGBuffer: { value: packed ? 1.0 : 0.0 },
			uNear: { value: 0.01 },
			uFar: { value: 5000.0 },
			uIsOrtho: { value: 1.0 },
			uProjParams: { value: new THREE.Vector4(1, 1, 0, 0) },
			uSceneRadius: { value: 0.2 },
			uFrameOffset: { value: 0.0 }
		},
		vertexShader: FULLSCREEN_VERTEX,
		fragmentShader:
			COMMON_GBUFFER_DECODE +
			/* glsl */ `
			uniform sampler2D tGBuffer;
			uniform sampler2D tColor;
			uniform float uSceneRadius;
			uniform float uFrameOffset;
			varying vec2 vUv;

			void main() {
				vec4 cG = readGBuffer(tGBuffer, vUv);
				if (cG.a <= 0.0) {
					gl_FragColor = vec4(0.0);
					return;
				}
				vec3 N = cG.xyz;
				vec3 P = reconstructViewPos(vUv, cG.a);
				vec3 incident = uIsOrtho > 0.5 ? vec3(0.0, 0.0, -1.0) : normalize(P);
				vec3 R = normalize(reflect(incident, N));

				// Skip rays pointing almost straight back into the camera lens
				if (R.z > 0.85) {
					gl_FragColor = vec4(0.0);
					return;
				}

				const int STEPS = 28;
				float maxDist = max(uSceneRadius * 2.75, 1e-4);
				float thickness = max(uSceneRadius * 0.42, 1e-4);
				float startBias = max(uSceneRadius * 0.008, 1e-5);
				float jitter = fract(sin(dot(vUv + vec2(uFrameOffset * 0.13), vec2(12.9898, 78.233))) * 43758.5453);

				float prevT = startBias;
				bool hit = false;
				float hitT = 0.0;

				for (int i = 1; i <= STEPS; i++) {
					float u = (float(i) - 0.5 + 0.5 * jitter) / float(STEPS);
					// Quadratic stride covers both tight gaps and full-part spans cleanly
					float t = startBias + maxDist * mix(u, u * u, 0.55);
					vec3 samplePos = P + R * t;
					vec2 sUv = projectViewPosToUv(samplePos);
					if (sUv.x <= 0.002 || sUv.x >= 0.998 || sUv.y <= 0.002 || sUv.y >= 0.998) {
						break;
					}
					vec4 sG = readGBuffer(tGBuffer, sUv);
					float rayDepth = -samplePos.z;
					if (sG.a > 0.0 && rayDepth >= sG.a - thickness * 0.04 && (rayDepth - sG.a) < thickness) {
						// Avoid self-intersection on flat coplanar neighbors
						vec3 hitPos = reconstructViewPos(sUv, sG.a);
						if (abs(dot(N, hitPos - P)) > startBias * 0.35) {
							hitT = t;
							hit = true;
							break;
						}
					}
					prevT = t;
				}

				if (!hit) {
					gl_FragColor = vec4(0.0);
					return;
				}

				// 5-step binary refinement between prevT and hitT
				float lo = prevT;
				float hi = hitT;
				vec2 bestUv = projectViewPosToUv(P + R * hitT);
				for (int b = 0; b < 5; b++) {
					float mid = 0.5 * (lo + hi);
					vec3 midPos = P + R * mid;
					vec2 mUv = projectViewPosToUv(midPos);
					vec4 mG = readGBuffer(tGBuffer, mUv);
					if (mG.a > 0.0 && (-midPos.z) >= mG.a) {
						hi = mid;
						bestUv = mUv;
					} else {
						lo = mid;
					}
				}

				vec4 hitColor = texture2D(tColor, bestUv);
				if (hitColor.a < 0.5) {
					gl_FragColor = vec4(0.0);
					return;
				}

				// Smooth screen-edge and distance confidence fade
				vec2 edgeDist = min(bestUv, 1.0 - bestUv);
				float edgeFade = smoothstep(0.01, 0.08, min(edgeDist.x, edgeDist.y));
				float distFade = 1.0 - smoothstep(maxDist * 0.65, maxDist, hi);
				float camBackFade = 1.0 - smoothstep(0.45, 0.85, R.z);
				float confidence = clamp(edgeFade * distFade * camBackFade * 0.92, 0.0, 1.0);

				gl_FragColor = vec4(hitColor.rgb, confidence);
			}
		`
	});
}

/**
 * Progressive linear-HDR accumulation pass.
 */
export function createAccumulateMaterial() {
	return new THREE.ShaderMaterial({
		depthTest: false,
		depthWrite: false,
		blending: THREE.NoBlending,
		uniforms: {
			tPrev: { value: null },
			tCurr: { value: null },
			uWeight: { value: 1.0 }
		},
		vertexShader: FULLSCREEN_VERTEX,
		fragmentShader: /* glsl */ `
			uniform sampler2D tPrev;
			uniform sampler2D tCurr;
			uniform float uWeight;
			varying vec2 vUv;
			void main() {
				vec4 curr = texture2D(tCurr, vUv);
				if (uWeight >= 0.999) {
					gl_FragColor = curr;
				} else {
					vec4 prev = texture2D(tPrev, vUv);
					gl_FragColor = mix(prev, curr, uWeight);
				}
			}
		`
	});
}

/**
 * Final composite pass to the canvas:
 *   - Preserves empty background (`alpha == 0`) with zero color shift (Invariant I6)
 *   - Unpremultiplies translucent pixels (`0 < alpha < 1`, e.g. contact shadow / clear plastic),
 *     applies Khronos PBR Neutral or AgX tone mapping + Linear-to-sRGB, and re-premultiplies
 *     for WebGL's premultiplied-alpha canvas compositor.
 */
export function createCompositeMaterial() {
	return new THREE.ShaderMaterial({
		depthTest: false,
		depthWrite: false,
		blending: THREE.NoBlending,
		uniforms: {
			tAccum: { value: null },
			tEnvMap: { value: null },
			tSharpBackdrop: { value: null },
			uSharpBackdropMode: { value: 0 }, // 0 = CubeUV fallback, 1 = 4K gamma-2 HDR WebP, 2 = custom linear HDR
			uBackdropBaseYaw: { value: 0.0 },
			uBackdropBlur: { value: 0.0 },
			uEnvCubeUVParams: { value: new THREE.Vector3(1 / 768, 1 / 1024, 8) },
			uShowBackdrop: { value: 0.0 },
			uIsOrtho: { value: 1.0 },
			uProjParams: { value: new THREE.Vector4(1, 1, 0, 0) },
			uCamWorldRot: { value: new THREE.Matrix3() },
			uEnvRotMat: { value: new THREE.Matrix3() },
			uToneMapping: { value: 0 }, // 0 = Khronos PBR Neutral, 1 = AgX
			uExposure: { value: 1.05 }
		},
		vertexShader: FULLSCREEN_VERTEX,
		fragmentShader: /* glsl */ `
			uniform sampler2D tAccum;
			uniform sampler2D tEnvMap;
			uniform sampler2D tSharpBackdrop;
			uniform int uSharpBackdropMode;
			uniform float uBackdropBaseYaw;
			uniform float uBackdropBlur;
			uniform vec3 uEnvCubeUVParams;
			uniform float uShowBackdrop;
			uniform float uIsOrtho;
			uniform vec4 uProjParams;
			uniform mat3 uCamWorldRot;
			uniform mat3 uEnvRotMat;
			uniform int uToneMapping;
			uniform float uExposure;
			varying vec2 vUv;

			float studioGetCubeFace(vec3 direction) {
				vec3 absDir = abs(direction);
				if (absDir.x > absDir.z) {
					if (absDir.x > absDir.y) return direction.x > 0.0 ? 0.0 : 3.0;
					return direction.y > 0.0 ? 1.0 : 4.0;
				} else {
					if (absDir.z > absDir.y) return direction.z > 0.0 ? 2.0 : 5.0;
					return direction.y > 0.0 ? 1.0 : 4.0;
				}
			}

			vec2 studioGetCubeUV(vec3 direction, float face) {
				vec2 uv;
				if (face == 0.0) uv = vec2(direction.z, direction.y) / abs(direction.x);
				else if (face == 1.0) uv = vec2(-direction.x, -direction.z) / abs(direction.y);
				else if (face == 2.0) uv = vec2(-direction.x, direction.y) / abs(direction.z);
				else if (face == 3.0) uv = vec2(-direction.z, direction.y) / abs(direction.x);
				else if (face == 4.0) uv = vec2(-direction.x, direction.z) / abs(direction.y);
				else uv = vec2(direction.x, direction.y) / abs(direction.z);
				return 0.5 * (uv + 1.0);
			}

			vec3 studioBilinearCubeUV(sampler2D envMap, vec3 direction, float mipInt) {
				float face = studioGetCubeFace(direction);
				float filterInt = max(4.0 - mipInt, 0.0);
				mipInt = max(mipInt, 4.0);
				float faceSize = exp2(mipInt);
				highp vec2 uv = studioGetCubeUV(direction, face) * (faceSize - 2.0) + 1.0;
				if (face > 2.0) {
					uv.y += faceSize;
					face -= 3.0;
				}
				uv.x += face * faceSize;
				uv.x += filterInt * 3.0 * 16.0;
				uv.y += 4.0 * (exp2(uEnvCubeUVParams.z) - faceSize);
				uv.x *= uEnvCubeUVParams.x;
				uv.y *= uEnvCubeUVParams.y;
				return texture2D(envMap, uv).rgb;
			}

			float studioRoughnessToMip(float roughness) {
				if (roughness >= 0.8) return (1.0 - roughness) * (-1.0 - (-2.0)) / (1.0 - 0.8) - 2.0;
				if (roughness >= 0.4) return (0.8 - roughness) * (2.0 - (-1.0)) / (0.8 - 0.4) - 1.0;
				if (roughness >= 0.305) return (0.4 - roughness) * (3.0 - 2.0) / (0.4 - 0.305) + 2.0;
				if (roughness >= 0.21) return (0.305 - roughness) * (4.0 - 3.0) / (0.305 - 0.21) + 3.0;
				return -2.0 * log2(max(1.16 * roughness, 1e-4));
			}

			vec3 studioSampleCubeUV(sampler2D envMap, vec3 sampleDir, float roughness) {
				float mip = clamp(studioRoughnessToMip(roughness), -2.0, uEnvCubeUVParams.z);
				float mipF = fract(mip);
				float mipInt = floor(mip);
				vec3 c0 = studioBilinearCubeUV(envMap, sampleDir, mipInt);
				if (mipF == 0.0) return c0;
				vec3 c1 = studioBilinearCubeUV(envMap, sampleDir, mipInt + 1.0);
				return mix(c0, c1, mipF);
			}

			vec3 studioNeutralToneMap(vec3 color) {
				const float StartCompression = 0.8 - 0.04;
				const float Desaturation = 0.15;
				float x = min(color.r, min(color.g, color.b));
				float offset = x < 0.08 ? x - 6.25 * x * x : 0.04;
				color -= offset;
				float peak = max(color.r, max(color.g, color.b));
				if (peak < StartCompression) return max(color, vec3(0.0));
				float d = 1.0 - StartCompression;
				float newPeak = 1.0 - d * d / (peak + d - StartCompression);
				color *= newPeak / peak;
				float g = 1.0 - 1.0 / (Desaturation * (peak - newPeak) + 1.0);
				return max(mix(color, vec3(newPeak), g), vec3(0.0));
			}

			vec3 studioAgxContrast(vec3 x) {
				vec3 x2 = x * x;
				vec3 x4 = x2 * x2;
				return + 15.5 * x4 * x2
					- 40.14 * x4 * x
					+ 31.96 * x4
					- 6.868 * x2 * x
					+ 0.4298 * x2
					+ 0.1191 * x
					- 0.00232;
			}

			vec3 studioAgxToneMap(vec3 color) {
				const mat3 AgXInsetMatrix = mat3(
					vec3(0.856627153315983, 0.137318972929847, 0.11189821299995),
					vec3(0.0951212405381588, 0.761241990602591, 0.0767994186031903),
					vec3(0.0482516061458583, 0.101439036467562, 0.811302368396859)
				);
				const mat3 AgXOutsetMatrix = mat3(
					vec3(1.1271005818144368, -0.1413297634984383, -0.14132976349843826),
					vec3(-0.11060664309660323, 1.157823702216272, -0.11060664309660294),
					vec3(-0.016493938717834573, -0.016493938717834257, 1.2519364065950405)
				);
				const float AgxMinEv = -12.47393;
				const float AgxMaxEv = 4.026069;
				color = AgXInsetMatrix * max(color, vec3(1e-10));
				color = clamp(log2(color), AgxMinEv, AgxMaxEv);
				color = (color - AgxMinEv) / (AgxMaxEv - AgxMinEv);
				color = studioAgxContrast(color);
				color = AgXOutsetMatrix * color;
				return pow(max(vec3(0.0), color), vec3(2.2));
			}

			vec3 studioLinearToSrgb(vec3 c) {
				vec3 lo = c * 12.92;
				vec3 hi = 1.055 * pow(max(c, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055;
				return mix(lo, hi, step(vec3(0.0031308), c));
			}

			void main() {
				vec4 texel = texture2D(tAccum, vUv);
				float a = clamp(texel.a, 0.0, 1.0);

				if (uShowBackdrop > 0.5) {
					vec2 ndc = vUv * 2.0 - 1.0;
					vec3 viewRay;
					if (uIsOrtho > 0.5) {
						float aspect = abs(uProjParams.y) / max(abs(uProjParams.x), 1e-4);
						viewRay = normalize(vec3(ndc.x * aspect * 0.42, ndc.y * 0.42, -1.0));
					} else {
						viewRay = normalize(vec3(
							(ndc.x + uProjParams.z) / uProjParams.x,
							(ndc.y + uProjParams.w) / uProjParams.y,
							-1.0
						));
					}
					vec3 worldRay = normalize(uCamWorldRot * viewRay);
					vec3 envDir = normalize(uEnvRotMat * worldRay);
					vec3 bgLinear;
					if (uSharpBackdropMode == 1) {
						float phi = atan(-envDir.y, envDir.x) + uBackdropBaseYaw;
						float u = phi * 0.15915494309189535 + 0.5;
						float v = clamp(asin(clamp(envDir.z, -1.0, 1.0)) * 0.3183098861837907 + 0.5, 0.0, 1.0);
						float lod = clamp(uBackdropBlur, 0.0, 1.0) * 6.5;
						vec3 enc = texture2DLodEXT(tSharpBackdrop, vec2(u, v), lod).rgb;
						bgLinear = enc * enc * 6.25 * 0.94;
					} else if (uSharpBackdropMode == 2) {
						float phi = atan(-envDir.y, envDir.x) + uBackdropBaseYaw;
						float u = phi * 0.15915494309189535 + 0.5;
						float v = clamp(asin(clamp(envDir.z, -1.0, 1.0)) * 0.3183098861837907 + 0.5, 0.0, 1.0);
						if (uBackdropBlur > 0.02) {
							bgLinear = studioSampleCubeUV(tEnvMap, envDir, clamp(uBackdropBlur * 0.45, 0.02, 0.5)) * 0.94;
						} else {
							bgLinear = texture2D(tSharpBackdrop, vec2(u, v)).rgb * 0.94;
						}
					} else {
						float fallbackRough = mix(0.03, 0.38, clamp(uBackdropBlur, 0.0, 1.0));
						bgLinear = studioSampleCubeUV(tEnvMap, envDir, fallbackRough) * 0.92;
					}
					vec3 combined = (max(texel.rgb, vec3(0.0)) + bgLinear * (1.0 - a)) * uExposure;
					vec3 mapped = uToneMapping == 1 ? studioAgxToneMap(combined) : studioNeutralToneMap(combined);
					vec3 srgb = clamp(studioLinearToSrgb(mapped), 0.0, 1.0);
					gl_FragColor = vec4(srgb, 1.0);
					return;
				}

				// Invariant I6: pure background passes through untouched so CSS --viewport-bg is exact
				if (texel.a <= 1e-5) {
					gl_FragColor = vec4(0.0);
					return;
				}
				vec3 unpremul = max(texel.rgb / max(a, 1e-4), vec3(0.0)) * uExposure;
				vec3 mapped = uToneMapping == 1 ? studioAgxToneMap(unpremul) : studioNeutralToneMap(unpremul);
				vec3 srgb = clamp(studioLinearToSrgb(mapped), 0.0, 1.0);
				gl_FragColor = vec4(srgb * a, a);
			}
		`
	});
}
