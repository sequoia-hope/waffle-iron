/**
 * Studio Physical Material Builder & Shared Shader Uniforms
 * (`specs/studio_pbr_renderer.md` §1, §4 I3, I5).
 *
 * Injects screen-space Ground Truth Ambient Occlusion (GTAO), Jimenez
 * multi-bounce indirect diffuse occlusion, Lagarde/Jimenez Ground-Truth
 * Specular Occlusion (GTSO), and Screen-Space Reflections (SSR) directly into
 * `THREE.MeshPhysicalMaterial` with a shared `customProgramCacheKey` so
 * multi-body assemblies share compiled WebGL programs.
 */
import * as THREE from 'three';

function make1x1Texture(r, g, b, a) {
	const data = new Uint8Array([r, g, b, a]);
	const tex = new THREE.DataTexture(data, 1, 1, THREE.RGBAFormat, THREE.UnsignedByteType);
	tex.minFilter = THREE.NearestFilter;
	tex.magFilter = THREE.NearestFilter;
	tex.needsUpdate = true;
	return tex;
}

export const FALLBACK_WHITE_AO_TEX = make1x1Texture(255, 255, 255, 255);
export const FALLBACK_EMPTY_SSR_TEX = make1x1Texture(0, 0, 0, 0);

/**
 * Shared uniform wrapper objects bound to every Studio `MeshPhysicalMaterial`.
 * Mutating `.value` on these wrappers updates every Studio material in O(1)
 * without rebuilding materials or recompiling shaders.
 */
export const STUDIO_SHARED_UNIFORMS = {
	uStudioAOTex: { value: FALLBACK_WHITE_AO_TEX },
	uStudioSSRTex: { value: FALLBACK_EMPTY_SSR_TEX },
	uStudioResolution: { value: new THREE.Vector2(1, 1) },
	uStudioAOEnabled: { value: 0.0 },
	uStudioSSREnabled: { value: 0.0 },
	uStudioSceneRadius: { value: 0.06 }
};

/** Reset shared uniforms to safe neutral fallbacks (e.g. during offscreen capture or Shaded mode). */
export function resetStudioSharedUniforms() {
	STUDIO_SHARED_UNIFORMS.uStudioAOTex.value = FALLBACK_WHITE_AO_TEX;
	STUDIO_SHARED_UNIFORMS.uStudioSSRTex.value = FALLBACK_EMPTY_SSR_TEX;
	STUDIO_SHARED_UNIFORMS.uStudioAOEnabled.value = 0.0;
	STUDIO_SHARED_UNIFORMS.uStudioSSREnabled.value = 0.0;
}

const STUDIO_PARS_FRAGMENT = /* glsl */ `
uniform sampler2D uStudioAOTex;
uniform sampler2D uStudioSSRTex;
uniform vec2 uStudioResolution;
uniform float uStudioAOEnabled;
uniform float uStudioSSREnabled;
`;

const STUDIO_SSR_INJECTION = /* glsl */ `
#include <lights_fragment_maps>
#if defined( RE_IndirectSpecular )
if ( uStudioSSREnabled > 0.5 && diffuseColor.a > 0.99 ) {
	vec2 studioSsrUv = clamp( gl_FragCoord.xy / max( uStudioResolution, vec2( 1.0 ) ), vec2( 0.001 ), vec2( 0.999 ) );
	vec4 ssrSample = texture2D( uStudioSSRTex, studioSsrUv );
	float ssrRoughWeight = 1.0 - smoothstep( 0.05, 0.65, material.roughness );
	radiance = mix( radiance, ssrSample.rgb, clamp( ssrSample.a * ssrRoughWeight, 0.0, 1.0 ) );
	#ifdef USE_CLEARCOAT
		float ccWeight = 1.0 - smoothstep( 0.05, 0.55, material.clearcoatRoughness );
		clearcoatRadiance = mix( clearcoatRadiance, ssrSample.rgb, clamp( ssrSample.a * ccWeight, 0.0, 1.0 ) );
	#endif
}
#endif
`;

const STUDIO_AO_INJECTION = /* glsl */ `
#include <aomap_fragment>
if ( uStudioAOEnabled > 0.5 && diffuseColor.a > 0.99 ) {
	vec2 studioAoUv = clamp( gl_FragCoord.xy / max( uStudioResolution, vec2( 1.0 ) ), vec2( 0.001 ), vec2( 0.999 ) );
	// Invariant I5: Jimenez et al. 2016 multi-bounce diffuse AO + GTSO specular occlusion.
	float studioAO = clamp( texture2D( uStudioAOTex, studioAoUv ).r, 0.0, 1.0 );
	vec3 mbA =  2.0404 * diffuseColor.rgb - 0.3324;
	vec3 mbB = -4.7951 * diffuseColor.rgb + 0.6417;
	vec3 mbC =  2.7552 * diffuseColor.rgb + 0.6903;
	vec3 multiBounceAO = max( vec3( studioAO ), ( ( studioAO * mbA + mbB ) * studioAO + mbC ) * studioAO );
	reflectedLight.indirectDiffuse *= multiBounceAO;

	// Subtle micro-shadowing on direct key/fill illumination inside deep cavities
	float directMicroShadow = mix( 1.0, studioAO * studioAO, 0.50 );
	reflectedLight.directDiffuse *= directMicroShadow;
	reflectedLight.directSpecular *= mix( 1.0, studioAO, 0.55 );

	#if defined( USE_CLEARCOAT )
		clearcoatSpecularIndirect *= studioAO;
	#endif
	#if defined( USE_SHEEN )
		sheenSpecularIndirect *= studioAO;
	#endif
	#if defined( STANDARD )
		float studioDotNV = saturate( dot( geometryNormal, geometryViewDir ) );
		reflectedLight.indirectSpecular *= computeSpecularOcclusion( studioDotNV, studioAO, material.roughness );
	#endif
}
`;

/**
 * Analytical tangent frame for UV-less CAD B-Rep meshes when `USE_ANISOTROPY`
 * is active: avoids `dFdx(vUv) == 0` division-by-zero (`NaN`) and aligns
 * brushing tangents circumferentially around the world/view Z axis.
 */
function buildAnalyticalNormalFragmentBegin() {
	const orig = THREE.ShaderChunk.normal_fragment_begin;
	return orig.replace(
		/mat3 tbn = getTangentFrame\([\s\S]*?\);/,
		/* glsl */ `
		vec3 studioRefAxis = abs( normal.z ) < 0.999 ? vec3( 0.0, 0.0, 1.0 ) : vec3( 1.0, 0.0, 0.0 );
		vec3 studioAnisoT = normalize( cross( studioRefAxis, normal ) );
		vec3 studioAnisoB = cross( normal, studioAnisoT );
		mat3 tbn = mat3( studioAnisoT, studioAnisoB, normal );
		`
	);
}

const ANALYTICAL_NORMAL_FRAGMENT_BEGIN = buildAnalyticalNormalFragmentBegin();

// ---------------------------------------------------------------------------
// Shader-Based FDM 3D-Printed Exterior Layer Lines (Invariant I15)
// ---------------------------------------------------------------------------

const FDM_PARS_VERTEX = /* glsl */ `
varying vec3 vFdmPartPos;
varying vec3 vFdmPartNormal;
varying vec3 vFdmUpView;
varying vec3 vFdmRasterAView;
varying vec3 vFdmRasterBView;
`;

const FDM_VERTEX_INJECTION = /* glsl */ `
#include <project_vertex>
vFdmPartPos = vec3( transformed );
vFdmPartNormal = normalize( objectNormal );
vFdmUpView = normalize( normalMatrix * vec3( 0.0, 0.0, 1.0 ) );
vFdmRasterAView = normalize( normalMatrix * vec3( 0.70710678, 0.70710678, 0.0 ) );
vFdmRasterBView = normalize( normalMatrix * vec3( 0.70710678, -0.70710678, 0.0 ) );
`;

const FDM_PARS_FRAGMENT = /* glsl */ `
uniform float uFdmLayerMm;
uniform float uStudioSceneRadius;
varying vec3 vFdmPartPos;
varying vec3 vFdmPartNormal;
varying vec3 vFdmUpView;
varying vec3 vFdmRasterAView;
varying vec3 vFdmRasterBView;
`;

const FDM_ROUGHNESS_GROOVE_INJECTION = /* glsl */ `
#include <roughnessmap_fragment>
float fdmLayerScale = clamp( uFdmLayerMm / 0.24, 0.25, 3.0 );
float fdmPitchM = max( uFdmLayerMm * 0.001, max( uStudioSceneRadius, 0.012 ) * 0.018 * fdmLayerScale );
vec3 fdmPartN = normalize( vFdmPartNormal );

// Perimeter & sloped walls: horizontal Z filament beads + stair-stepping contours
float fdmZPhase = vFdmPartPos.z / max( fdmPitchM, 1e-5 );
float fdmZFrac = fract( fdmZPhase );
float fdmZF = fdmZFrac - 0.5;
float fdmLayerIdx = floor( fdmZPhase );
float fdmFlowJitter = fract( sin( fdmLayerIdx * 127.1 + 311.7 ) * 43758.5453 ) - 0.5;
float fdmWallFw = fwidth( fdmZPhase );
float fdmWallAtten = ( 1.0 - smoothstep( 0.26, 0.72, fdmWallFw ) ) * ( 1.0 - smoothstep( 0.90, 0.985, abs( fdmPartN.z ) ) );

// Flat top/bottom skins (|n_z| -> 1): 45-degree rectilinear nozzle toolpaths
float fdmTopMask = smoothstep( 0.88, 0.975, abs( fdmPartN.z ) );
float fdmTopSign = fdmPartN.z >= 0.0 ? 1.0 : -1.0;
float fdmRCoord = ( vFdmPartPos.x + fdmTopSign * vFdmPartPos.y ) * 0.70710678;
float fdmRPhase = fdmRCoord / max( fdmPitchM * 1.15, 1e-5 );
float fdmRFrac = fract( fdmRPhase );
float fdmRF = fdmRFrac - 0.5;
float fdmTopFw = fwidth( fdmRPhase );
float fdmTopAtten = ( 1.0 - smoothstep( 0.26, 0.72, fdmTopFw ) ) * fdmTopMask;

// Inter-layer V-groove micro-occlusion & seam roughness
float fdmWallGroove = pow( abs( 2.0 * fdmZF ), 2.8 );
float fdmTopGroove = pow( abs( 2.0 * fdmRF ), 2.8 );
float fdmGroove = fdmWallAtten * fdmWallGroove + fdmTopAtten * 0.65 * fdmTopGroove;
diffuseColor.rgb *= ( 1.0 - fdmGroove * 0.28 );
roughnessFactor = clamp( roughnessFactor + fdmGroove * 0.18, 0.04, 1.0 );
`;

const FDM_NORMAL_INJECTION = /* glsl */ `
#include <normal_fragment_maps>
vec3 fdmTangUp = vFdmUpView - normal * dot( normal, vFdmUpView );
vec3 fdmTangDir = length( fdmTangUp ) > 1e-4 ? normalize( fdmTangUp ) : vec3( 0.0 );
float fdmBeadSlope = -sin( 6.2831853 * fdmZFrac ) * ( 0.78 + 0.35 * abs( 2.0 * fdmZF ) );

vec3 fdmRAxis = fdmPartN.z >= 0.0 ? vFdmRasterAView : vFdmRasterBView;
vec3 fdmRTang = fdmRAxis - normal * dot( normal, fdmRAxis );
vec3 fdmRTangDir = length( fdmRTang ) > 1e-4 ? normalize( fdmRTang ) : vec3( 0.0 );
float fdmTopSlope = -sin( 6.2831853 * fdmRFrac );

vec3 fdmNormalDelta =
	fdmTangDir * ( fdmBeadSlope * ( 0.48 + 0.08 * fdmFlowJitter ) * fdmWallAtten ) +
	fdmRTangDir * ( fdmTopSlope * 0.26 * fdmTopAtten );
normal = normalize( normal + fdmNormalDelta );
`;

const FDM_CLEARCOAT_NORMAL_INJECTION = /* glsl */ `
#include <clearcoat_normal_fragment_maps>
#ifdef USE_CLEARCOAT
	clearcoatNormal = normalize( clearcoatNormal + fdmNormalDelta );
#endif
`;

function injectFdmIntoShader(shader, printLayerMm) {
	shader.uniforms.uFdmLayerMm = { value: printLayerMm };
	shader.uniforms.uStudioSceneRadius = STUDIO_SHARED_UNIFORMS.uStudioSceneRadius;
	shader.vertexShader =
		FDM_PARS_VERTEX +
		shader.vertexShader.replace('#include <project_vertex>', FDM_VERTEX_INJECTION);
	shader.fragmentShader =
		FDM_PARS_FRAGMENT +
		shader.fragmentShader
			.replace('#include <roughnessmap_fragment>', FDM_ROUGHNESS_GROOVE_INJECTION)
			.replace('#include <normal_fragment_maps>', FDM_NORMAL_INJECTION)
			.replace('#include <clearcoat_normal_fragment_maps>', FDM_CLEARCOAT_NORMAL_INJECTION);
}

/**
 * Attach FDM 3D-printed exterior layer-line shader hooks onto a Classic Shaded
 * `THREE.MeshStandardMaterial`.
 *
 * @param {THREE.MeshStandardMaterial} mat
 * @param {number} printLayerMm
 */
export function attachFdmShaderToStandardMaterial(mat, printLayerMm) {
	if (!(printLayerMm > 0)) return mat;
	mat.customProgramCacheKey = () => `waffle-standard-fdm-v1`;
	mat.onBeforeCompile = (shader) => {
		injectFdmIntoShader(shader, printLayerMm);
	};
	return mat;
}

/**
 * Build a Studio `THREE.MeshPhysicalMaterial` wired to `STUDIO_SHARED_UNIFORMS`.
 *
 * @param {object} opts
 * @param {THREE.Color} opts.color
 * @param {number} opts.metalness
 * @param {number} opts.roughness
 * @param {boolean} opts.transparent
 * @param {number} opts.opacity
 * @param {any} [opts.finish]
 * @returns {THREE.MeshPhysicalMaterial}
 */
export function createStudioPhysicalMaterial({
	color,
	metalness,
	roughness,
	transparent,
	opacity,
	finish = null
}) {
	const clearcoat = finish?.clearcoat ?? 0.0;
	const clearcoatRoughness = finish?.clearcoatRoughness ?? 0.2;
	const anisotropy = finish?.anisotropy ?? 0.0;
	const anisotropyRotation = finish?.anisotropyRotation ?? 0.0;
	const ior = finish?.ior ?? 1.5;
	const printLayerMm = finish?.printLayerMm ?? 0.0;

	const mat = new THREE.MeshPhysicalMaterial({
		color,
		metalness,
		roughness: Math.max(0.025, roughness),
		clearcoat,
		clearcoatRoughness,
		anisotropy,
		anisotropyRotation,
		ior,
		envMapIntensity: 1.15,
		transparent,
		opacity,
		depthWrite: !transparent,
		side: THREE.DoubleSide
	});

	const ccFlag = clearcoat > 0 ? 1 : 0;
	const anFlag = anisotropy > 0 ? 1 : 0;
	const fdmFlag = printLayerMm > 0 ? 1 : 0;
	mat.customProgramCacheKey = () =>
		`waffle-studio-physical-v1|cc:${ccFlag}|an:${anFlag}|fdm:${fdmFlag}`;

	mat.onBeforeCompile = (shader) => {
		shader.uniforms.uStudioAOTex = STUDIO_SHARED_UNIFORMS.uStudioAOTex;
		shader.uniforms.uStudioSSRTex = STUDIO_SHARED_UNIFORMS.uStudioSSRTex;
		shader.uniforms.uStudioResolution = STUDIO_SHARED_UNIFORMS.uStudioResolution;
		shader.uniforms.uStudioAOEnabled = STUDIO_SHARED_UNIFORMS.uStudioAOEnabled;
		shader.uniforms.uStudioSSREnabled = STUDIO_SHARED_UNIFORMS.uStudioSSREnabled;

		shader.fragmentShader =
			STUDIO_PARS_FRAGMENT +
			shader.fragmentShader
				.replace('#include <normal_fragment_begin>', ANALYTICAL_NORMAL_FRAGMENT_BEGIN)
				.replace('#include <lights_fragment_maps>', STUDIO_SSR_INJECTION)
				.replace('#include <aomap_fragment>', STUDIO_AO_INJECTION);

		if (fdmFlag) {
			injectFdmIntoShader(shader, printLayerMm);
		}

		if (typeof window !== 'undefined' && window.__waffle?.shaderDebug) {
			console.log('Studio Vertex shader:', shader.vertexShader.substring(0, 500));
			console.log('Studio Fragment shader:', shader.fragmentShader.substring(0, 500));
		}
	};

	return mat;
}

/**
 * Build a UV-mapped PBR `THREE.MeshPhysicalMaterial` for physical 3D tabletop
 * stages (`StudioStageSurface.svelte`: wooden workbench, walnut desk, granite
 * countertop, cutting mat). Keeps Three.js's standard UV tangent frame so
 * `normalMap` relief renders cleanly while still receiving Studio GTAO + SSR!
 */
export function createStudioStageMaterial({
	map,
	normalMap = null,
	roughnessMap = null,
	roughness = 0.65,
	metalness = 0.0,
	clearcoat = 0.15,
	clearcoatRoughness = 0.25,
	normalScale = 0.7
}) {
	const mat = new THREE.MeshPhysicalMaterial({
		color: 0xffffff,
		map,
		normalMap,
		normalScale: new THREE.Vector2(normalScale, normalScale),
		roughnessMap,
		roughness,
		metalness,
		clearcoat,
		clearcoatRoughness,
		envMapIntensity: 1.05,
		side: THREE.FrontSide
	});

	const ccFlag = clearcoat > 0 ? 1 : 0;
	const nmFlag = normalMap ? 1 : 0;
	const rmFlag = roughnessMap ? 1 : 0;
	mat.customProgramCacheKey = () =>
		`waffle-studio-stage-v1|cc:${ccFlag}|nm:${nmFlag}|rm:${rmFlag}`;

	mat.onBeforeCompile = (shader) => {
		shader.uniforms.uStudioAOTex = STUDIO_SHARED_UNIFORMS.uStudioAOTex;
		shader.uniforms.uStudioSSRTex = STUDIO_SHARED_UNIFORMS.uStudioSSRTex;
		shader.uniforms.uStudioResolution = STUDIO_SHARED_UNIFORMS.uStudioResolution;
		shader.uniforms.uStudioAOEnabled = STUDIO_SHARED_UNIFORMS.uStudioAOEnabled;
		shader.uniforms.uStudioSSREnabled = STUDIO_SHARED_UNIFORMS.uStudioSSREnabled;

		shader.fragmentShader =
			STUDIO_PARS_FRAGMENT +
			shader.fragmentShader
				.replace('#include <lights_fragment_maps>', STUDIO_SSR_INJECTION)
				.replace('#include <aomap_fragment>', STUDIO_AO_INJECTION);
	};

	return mat;
}
