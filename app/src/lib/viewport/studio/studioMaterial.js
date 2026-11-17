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
	uStudioSSREnabled: { value: 0.0 }
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
	mat.customProgramCacheKey = () => `waffle-studio-physical-v1|cc:${ccFlag}|an:${anFlag}`;

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

		if (typeof window !== 'undefined' && window.__waffle?.shaderDebug) {
			console.log('Studio Vertex shader:', shader.vertexShader.substring(0, 500));
			console.log('Studio Fragment shader:', shader.fragmentShader.substring(0, 500));
		}
	};

	return mat;
}
