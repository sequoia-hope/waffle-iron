<script>
	/**
	 * Studio PBR Lighting Pipeline Orchestrator (`specs/studio_pbr_renderer.md`).
	 *
	 * Inactive (`viewportShading === 'shaded'`):
	 *   - Leaves Threlte's stock `autoRender` active (`scene.environment = null`),
	 *     incurring zero per-frame overhead and preserving classic Shaded baselines.
	 *
	 * Active (`viewportShading === 'studio'`):
	 *   1. Procedural HDR Studio Softbox PMREM IBL (`studioEnvironment.js`)
	 *   2. Exact linear view-space G-Buffer (`normal.xyz`, `linearDepth`)
	 *   3. Horizon-based GTAO + depth/normal-weighted separable bilateral denoiser
	 *   4. View-space Screen-Space Reflections (SSR) with binary hit refinement
	 *   5. Linear HDR scene render (with stencil buffer for `SectionCap`)
	 *   6. 8-sample Halton (2, 3) sub-pixel progressive accumulation -> Khronos PBR
	 *      Neutral / AgX tone-mapped composite, halting at 0% idle GPU once settled.
	 */
	import { useThrelte, useTask } from '@threlte/core';
	import { onDestroy } from 'svelte';
	import * as THREE from 'three';
	import {
		getMeshes,
		getHoveredRef,
		getSelectedRefs,
		getSelectedBodyId,
		getHoveredBodyId,
		getSelectedFeatureId,
		getGhostFeatureId,
		getSketchMode,
		getSectionState,
		getShowDatumPlanes,
		getShowOriginTriad,
		isBodyVisible
	} from '$lib/engine/store.svelte.js';
	import { getSettings, getColorVersion } from '$lib/ui/settings.svelte.js';
	import { getTheme } from '$lib/ui/theme.svelte.js';
	import { buildSectionClipPlane } from '../sectionPlane.js';
	import { placementMatrix } from '../placement.js';
	import { getStudioEnvironmentTarget, disposeStudioEnvironments } from './studioEnvironment.js';
	import {
		STUDIO_SHARED_UNIFORMS,
		FALLBACK_WHITE_AO_TEX,
		FALLBACK_EMPTY_SSR_TEX,
		resetStudioSharedUniforms
	} from './studioMaterial.js';
	import {
		haltonSubpixelOffset,
		extractCameraProjParams,
		createGBufferMaterial,
		createGTAOMaterial,
		createBilateralBlurMaterial,
		createSSRMaterial,
		createAccumulateMaterial,
		createCompositeMaterial
	} from './studioShaders.js';
	import { updateStudioStats, setStudioFreshnessCheck } from './studioState.js';

	const TARGET_SAMPLES = 8;

	const { renderer, scene, camera, autoRender, renderStage, invalidate } = useThrelte();

	let settings = $derived(getSettings());
	let studioActive = $derived(settings.viewportShading === 'studio');

	// Detect WebGL2 color-renderable float FBO support (Invariant §6 fallback)
	const floatRenderable = Boolean(
		renderer?.capabilities?.isWebGL2 &&
			(renderer.extensions.has('EXT_color_buffer_float') ||
				renderer.extensions.has('EXT_color_buffer_half_float'))
	);
	const packedGBuffer = !floatRenderable;
	const hdrTargetType = floatRenderable ? THREE.HalfFloatType : THREE.UnsignedByteType;

	// Fullscreen post-processing quad
	const quadScene = new THREE.Scene();
	const quadCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
	const quadGeo = new THREE.PlaneGeometry(2, 2);
	const quadMesh = new THREE.Mesh(quadGeo);
	quadMesh.frustumCulled = false;
	quadScene.add(quadMesh);

	const gbufferMat = createGBufferMaterial(packedGBuffer);
	const gtaoMat = createGTAOMaterial(packedGBuffer);
	const blurMat = createBilateralBlurMaterial(packedGBuffer);
	const ssrMat = createSSRMaterial(packedGBuffer);
	const accumMat = createAccumulateMaterial();
	const compositeMat = createCompositeMaterial();

	/** @type {THREE.WebGLRenderTarget | null} */
	let gbufferTarget = null;
	/** @type {THREE.WebGLRenderTarget | null} */
	let aoRawTarget = null;
	/** @type {THREE.WebGLRenderTarget | null} */
	let aoBlurTarget = null;
	/** @type {THREE.WebGLRenderTarget | null} */
	let aoFinalTarget = null;
	/** @type {THREE.WebGLRenderTarget | null} */
	let ssrTarget = null;
	/** @type {THREE.WebGLRenderTarget | null} */
	let frameTarget = null;
	/** @type {THREE.WebGLRenderTarget | null} */
	let accumTargetA = null;
	/** @type {THREE.WebGLRenderTarget | null} */
	let accumTargetB = null;
	/** @type {THREE.WebGLRenderTarget | null} */
	let prevLitTarget = null;

	let allocW = 0;
	let allocH = 0;
	let aoW = 0;
	let aoH = 0;
	let prevLitValid = false;

	let sampleCount = 0;
	let passCount = 0;
	let lastSig = '';
	let accumReadIsA = true;

	const _drawSize = new THREE.Vector2();
	const _savedClearColor = new THREE.Color();

	function disposeTargets() {
		for (const rt of [
			gbufferTarget,
			aoRawTarget,
			aoBlurTarget,
			aoFinalTarget,
			ssrTarget,
			frameTarget,
			accumTargetA,
			accumTargetB,
			prevLitTarget
		]) {
			rt?.dispose();
		}
		gbufferTarget = null;
		aoRawTarget = null;
		aoBlurTarget = null;
		aoFinalTarget = null;
		ssrTarget = null;
		frameTarget = null;
		accumTargetA = null;
		accumTargetB = null;
		prevLitTarget = null;
		allocW = 0;
		allocH = 0;
		prevLitValid = false;
	}

	function ensureTargets(w, h) {
		if (w === allocW && h === allocH && frameTarget) return;
		disposeTargets();
		allocW = w;
		allocH = h;
		const scale = Math.max(w, h) > 1600 ? 0.65 : 1.0;
		aoW = Math.max(1, Math.round(w * scale));
		aoH = Math.max(1, Math.round(h * scale));

		gbufferTarget = new THREE.WebGLRenderTarget(aoW, aoH, {
			type: hdrTargetType,
			format: THREE.RGBAFormat,
			minFilter: THREE.NearestFilter,
			magFilter: THREE.NearestFilter,
			depthBuffer: true,
			stencilBuffer: false
		});

		const makeAux = (tw, th, filter = THREE.LinearFilter) =>
			new THREE.WebGLRenderTarget(tw, th, {
				type: hdrTargetType,
				format: THREE.RGBAFormat,
				minFilter: filter,
				magFilter: filter,
				depthBuffer: false,
				stencilBuffer: false
			});

		aoRawTarget = makeAux(aoW, aoH, THREE.LinearFilter);
		aoBlurTarget = makeAux(aoW, aoH, THREE.LinearFilter);
		aoFinalTarget = makeAux(aoW, aoH, THREE.LinearFilter);
		ssrTarget = makeAux(aoW, aoH, THREE.LinearFilter);

		frameTarget = new THREE.WebGLRenderTarget(w, h, {
			type: hdrTargetType,
			format: THREE.RGBAFormat,
			minFilter: THREE.LinearFilter,
			magFilter: THREE.LinearFilter,
			depthBuffer: true,
			// Stencil buffer required by SectionCap.svelte
			stencilBuffer: true
		});

		accumTargetA = makeAux(w, h, THREE.LinearFilter);
		accumTargetB = makeAux(w, h, THREE.LinearFilter);
		prevLitTarget = makeAux(w, h, THREE.LinearFilter);
	}

	// Cache world bounding sphere radius per meshes array reference
	let cachedMeshesRef = null;
	let cachedVisKey = '';
	let cachedSceneRadius = 0.15;

	function getSceneRadius() {
		const meshes = getMeshes();
		if (!meshes || meshes.length === 0) return 0.15;
		const visKey = meshes.map((m) => (isBodyVisible(m.bodyId) ? '1' : '0')).join('');
		if (meshes === cachedMeshesRef && visKey === cachedVisKey) {
			return cachedSceneRadius;
		}
		cachedMeshesRef = meshes;
		cachedVisKey = visKey;

		let minX = Infinity, maxX = -Infinity;
		let minY = Infinity, maxY = -Infinity;
		let minZ = Infinity, maxZ = -Infinity;
		const vTmp = new THREE.Vector3();

		for (const m of meshes) {
			if (!isBodyVisible(m.bodyId) || !m.vertices?.length) continue;
			const mat = m.transform ? placementMatrix(m.transform) : null;
			const v = m.vertices;
			// Stride large meshes so radius estimation stays < 0.1ms even on 500k verts
			const vertCount = Math.floor(v.length / 3);
			const step = Math.max(1, Math.floor(vertCount / 2048)) * 3;
			for (let i = 0; i < v.length; i += step) {
				let x = v[i], y = v[i + 1], z = v[i + 2];
				if (mat) {
					vTmp.set(x, y, z).applyMatrix4(mat);
					x = vTmp.x; y = vTmp.y; z = vTmp.z;
				}
				if (x < minX) minX = x;
				if (x > maxX) maxX = x;
				if (y < minY) minY = y;
				if (y > maxY) maxY = y;
				if (z < minZ) minZ = z;
				if (z > maxZ) maxZ = z;
			}
		}

		if (!Number.isFinite(minX)) {
			cachedSceneRadius = 0.15;
			return cachedSceneRadius;
		}
		const dx = maxX - minX;
		const dy = maxY - minY;
		const dz = maxZ - minZ;
		cachedSceneRadius = Math.max(0.5 * Math.sqrt(dx * dx + dy * dy + dz * dz), 1e-4);
		return cachedSceneRadius;
	}

	/**
	 * Build a deterministic state signature for progressive accumulation reset.
	 * Any camera, geometry, material, selection, hover, section, or setting change
	 * changes this string and restarts sampleCount from 0.
	 */
	function computeStateSignature(cam, w, h) {
		const mw = cam.matrixWorld.elements;
		const pm = cam.projectionMatrix.elements;
		let camKey = '';
		for (let i = 0; i < 16; i++) {
			camKey += mw[i].toFixed(5) + ',' + pm[i].toFixed(5) + ';';
		}
		const meshes = getMeshes();
		const meshKey = meshes
			.map(
				(m) =>
					`${m.bodyId}:${m.triangleCount}:${isBodyVisible(m.bodyId) ? 1 : 0}:${m.material ?? ''}:${
						m.appearance ? m.appearance.color.join(',') + '/' + m.appearance.metalness + '/' + m.appearance.roughness : ''
					}`
			)
			.join('|');
		const hRef = getHoveredRef();
		const sRefs = getSelectedRefs();
		const sec = getSectionState();
		const sk = getSketchMode();
		return [
			w,
			h,
			camKey,
			meshKey,
			hRef ? JSON.stringify(hRef) : '',
			sRefs.length ? JSON.stringify(sRefs) : '',
			getSelectedBodyId() ?? '',
			getHoveredBodyId() ?? '',
			getSelectedFeatureId() ?? '',
			getGhostFeatureId() ?? '',
			sk?.active ? 1 : 0,
			sec?.active ? `${sec.flipped}:${sec.offset}:${JSON.stringify(sec.plane)}` : '0',
			getShowDatumPlanes() ? 1 : 0,
			getShowOriginTriad() ? 1 : 0,
			getTheme(),
			getColorVersion(),
			settings.studioEnvironment,
			settings.studioAO ? 1 : 0,
			settings.studioSSR ? 1 : 0,
			settings.studioGroundShadow ? 1 : 0,
			settings.studioToneMapping
		].join('#');
	}

	function renderFullscreenPass(material, target) {
		quadMesh.material = material;
		renderer.setRenderTarget(target);
		renderer.render(quadScene, quadCamera);
	}

	/**
	 * Render only visible opaque CAD solid meshes (`waffleType === 'model'`) into
	 * `gbufferTarget`, excluding edges, vertices, datums, sketches, caps, and floor shadows.
	 */
	function renderGBufferPass(cam) {
		const sec = getSectionState();
		const clipPlane = sec?.active && sec?.plane ? buildSectionClipPlane(sec.plane, sec.flipped, sec.offset) : null;
		gbufferMat.clippingPlanes = clipPlane ? [clipPlane] : [];
		gbufferMat.uniforms.uNear.value = /** @type {any} */ (cam).near ?? 0.01;
		gbufferMat.uniforms.uFar.value = /** @type {any} */ (cam).far ?? 5000.0;

		/** @type {THREE.Object3D[]} */
		const hidden = [];
		scene.traverse((obj) => {
			if (!obj.visible) return;
			const isRenderableLeaf =
				/** @type {any} */ (obj).isMesh ||
				/** @type {any} */ (obj).isLine ||
				/** @type {any} */ (obj).isPoints;
			if (!isRenderableLeaf) return;

			if (obj.userData?.waffleType !== 'model' || !/** @type {any} */ (obj).isMesh) {
				obj.visible = false;
				hidden.push(obj);
				return;
			}
			const mat = /** @type {any} */ (obj).material;
			const isTrans = Array.isArray(mat)
				? mat.some((m) => m?.transparent)
				: Boolean(mat?.transparent);
			if (isTrans) {
				obj.visible = false;
				hidden.push(obj);
			}
		});

		const prevOverride = scene.overrideMaterial;
		try {
			scene.overrideMaterial = gbufferMat;
			renderer.setRenderTarget(gbufferTarget);
			renderer.setClearColor(0x000000, 0.0);
			renderer.clear(true, true, false);
			renderer.render(scene, cam);
		} finally {
			scene.overrideMaterial = prevOverride;
			for (const obj of hidden) {
				obj.visible = true;
			}
		}
	}

	// Reactively synchronize Threlte autoRender and scene.environment with mode & rig preset
	$effect(() => {
		const active = studioActive;
		const envPreset = settings.studioEnvironment;

		if (!renderer || !scene) return;

		if (!active) {
			autoRender.set(true);
			scene.environment = null;
			resetStudioSharedUniforms();
			lastSig = '';
			sampleCount = 0;
			prevLitValid = false;
			updateStudioStats({
				active: false,
				environmentLoaded: false,
				sampleCount: 0,
				settled: false,
				aoEnabled: false,
				ssrEnabled: false,
				groundShadowVisible: false,
				floatRenderable
			});
			invalidate();
			return;
		}

		autoRender.set(false);
		const envTarget = getStudioEnvironmentTarget(renderer, envPreset);
		scene.environment = envTarget.texture;
		lastSig = '';
		sampleCount = 0;
		updateStudioStats({
			active: true,
			environmentLoaded: Boolean(envTarget?.texture),
			sampleCount: 0,
			targetSamples: TARGET_SAMPLES,
			settled: false,
			aoEnabled: Boolean(settings.studioAO),
			ssrEnabled: Boolean(settings.studioSSR),
			groundShadowVisible: Boolean(settings.studioGroundShadow),
			floatRenderable
		});
		invalidate();
	});

	setStudioFreshnessCheck(() => {
		if (!studioActive || !renderer || !scene) return false;
		const cam = camera.current;
		if (!cam || !lastSig) return false;
		renderer.getDrawingBufferSize(_drawSize);
		const drawW = Math.floor(_drawSize.x);
		const drawH = Math.floor(_drawSize.y);
		if (drawW <= 0 || drawH <= 0) return false;
		cam.updateMatrixWorld();
		return computeStateSignature(cam, drawW, drawH) === lastSig;
	});

	// Custom Studio render task on Threlte's renderStage (autoInvalidate: false for 0% idle GPU!)
	useTask(
		() => {
			if (!studioActive || !renderer || !scene) return;
			const cam = camera.current;
			if (!cam) return;

			renderer.getDrawingBufferSize(_drawSize);
			const drawW = Math.floor(_drawSize.x);
			const drawH = Math.floor(_drawSize.y);
			if (drawW <= 0 || drawH <= 0) return;

			ensureTargets(drawW, drawH);

			// Ensure world & projection matrices are current before computing signature
			cam.updateMatrixWorld();
			const sig = computeStateSignature(cam, drawW, drawH);
			if (sig !== lastSig) {
				lastSig = sig;
				sampleCount = 0;
				prevLitValid = false;
			}

			// Invariant I7: once TARGET_SAMPLES have converged and nothing changed,
			// execute zero GPU passes and stay completely idle.
			if (sampleCount >= TARGET_SAMPLES) {
				return;
			}

			const savedTarget = renderer.getRenderTarget();
			const savedClearAlpha = renderer.getClearAlpha();
			renderer.getClearColor(_savedClearColor);

			try {
				const sceneRadius = getSceneRadius();
				const needGBuffer = Boolean(settings.studioAO || settings.studioSSR);

				// Seed prevLitTarget once before sample 0 when SSR is enabled so every
				// accumulated sample (0..TARGET_SAMPLES-1) includes full SSR radiance!
				if (settings.studioSSR && !prevLitValid) {
					STUDIO_SHARED_UNIFORMS.uStudioAOTex.value = FALLBACK_WHITE_AO_TEX;
					STUDIO_SHARED_UNIFORMS.uStudioSSRTex.value = FALLBACK_EMPTY_SSR_TEX;
					STUDIO_SHARED_UNIFORMS.uStudioAOEnabled.value = 0.0;
					STUDIO_SHARED_UNIFORMS.uStudioSSREnabled.value = 0.0;
					STUDIO_SHARED_UNIFORMS.uStudioResolution.value.set(drawW, drawH);

					renderer.setRenderTarget(prevLitTarget);
					renderer.setClearColor(0x000000, 0.0);
					renderer.clear(true, true, false);
					renderer.render(scene, cam);
					prevLitValid = true;
				}

				const jitter = haltonSubpixelOffset(sampleCount);
				if (jitter.x !== 0 || jitter.y !== 0) {
					/** @type {any} */ (cam).setViewOffset(drawW, drawH, jitter.x, jitter.y, drawW, drawH);
				}

				try {
					const projInfo = extractCameraProjParams(cam);

					if (needGBuffer) {
						renderGBufferPass(cam);
					}

					// Pass B: Horizon-based GTAO + separable depth/normal bilateral denoise
					if (settings.studioAO) {
						const worldRadius = Math.max(sceneRadius * 0.22, 1e-4);
						gtaoMat.uniforms.tGBuffer.value = gbufferTarget.texture;
						gtaoMat.uniforms.uNear.value = projInfo.near;
						gtaoMat.uniforms.uFar.value = projInfo.far;
						gtaoMat.uniforms.uIsOrtho.value = projInfo.isOrtho;
						gtaoMat.uniforms.uProjParams.value.copy(projInfo.projParams);
						gtaoMat.uniforms.uResolution.value.set(aoW, aoH);
						gtaoMat.uniforms.uWorldRadius.value = worldRadius;
						gtaoMat.uniforms.uFrameOffset.value = sampleCount;
						renderFullscreenPass(gtaoMat, aoRawTarget);

						blurMat.uniforms.tGBuffer.value = gbufferTarget.texture;
						blurMat.uniforms.uNear.value = projInfo.near;
						blurMat.uniforms.uFar.value = projInfo.far;
						blurMat.uniforms.uIsOrtho.value = projInfo.isOrtho;
						blurMat.uniforms.uProjParams.value.copy(projInfo.projParams);
						blurMat.uniforms.uDepthSigma.value = Math.max(sceneRadius * 0.06, 1e-4);

						blurMat.uniforms.tAO.value = aoRawTarget.texture;
						blurMat.uniforms.uTexelStep.value.set(1.0 / aoW, 0.0);
						renderFullscreenPass(blurMat, aoBlurTarget);

						blurMat.uniforms.tAO.value = aoBlurTarget.texture;
						blurMat.uniforms.uTexelStep.value.set(0.0, 1.0 / aoH);
						renderFullscreenPass(blurMat, aoFinalTarget);

						STUDIO_SHARED_UNIFORMS.uStudioAOTex.value = aoFinalTarget.texture;
						STUDIO_SHARED_UNIFORMS.uStudioAOEnabled.value = 1.0;
					} else {
						STUDIO_SHARED_UNIFORMS.uStudioAOTex.value = FALLBACK_WHITE_AO_TEX;
						STUDIO_SHARED_UNIFORMS.uStudioAOEnabled.value = 0.0;
					}

					// Pass C: View-space Screen-Space Reflections
					if (settings.studioSSR && prevLitValid) {
						ssrMat.uniforms.tGBuffer.value = gbufferTarget.texture;
						ssrMat.uniforms.tColor.value = prevLitTarget.texture;
						ssrMat.uniforms.uNear.value = projInfo.near;
						ssrMat.uniforms.uFar.value = projInfo.far;
						ssrMat.uniforms.uIsOrtho.value = projInfo.isOrtho;
						ssrMat.uniforms.uProjParams.value.copy(projInfo.projParams);
						ssrMat.uniforms.uSceneRadius.value = sceneRadius;
						ssrMat.uniforms.uFrameOffset.value = sampleCount;
						renderFullscreenPass(ssrMat, ssrTarget);

						STUDIO_SHARED_UNIFORMS.uStudioSSRTex.value = ssrTarget.texture;
						STUDIO_SHARED_UNIFORMS.uStudioSSREnabled.value = 1.0;
					} else {
						STUDIO_SHARED_UNIFORMS.uStudioSSRTex.value = FALLBACK_EMPTY_SSR_TEX;
						STUDIO_SHARED_UNIFORMS.uStudioSSREnabled.value = 0.0;
					}

					// Pass D: Main linear-HDR lit scene render (with stencil for SectionCap)
					STUDIO_SHARED_UNIFORMS.uStudioResolution.value.set(drawW, drawH);
					renderer.setRenderTarget(frameTarget);
					renderer.setClearColor(0x000000, 0.0);
					renderer.clear(true, true, true);
					renderer.render(scene, cam);
				} finally {
					STUDIO_SHARED_UNIFORMS.uStudioAOEnabled.value = 0.0;
					STUDIO_SHARED_UNIFORMS.uStudioSSREnabled.value = 0.0;
					// Invariant I7: always clear camera sub-pixel jitter immediately so
					// raycasting / picking sees the exact unjittered projection matrix.
					if (jitter.x !== 0 || jitter.y !== 0) {
						/** @type {any} */ (cam).clearViewOffset();
					}
				}

				// Pass E: Progressive Halton accumulation in linear HDR space
				const prevAccum = accumReadIsA ? accumTargetA : accumTargetB;
				const nextAccum = accumReadIsA ? accumTargetB : accumTargetA;
				accumMat.uniforms.tPrev.value = prevAccum.texture;
				accumMat.uniforms.tCurr.value = frameTarget.texture;
				accumMat.uniforms.uWeight.value = sampleCount === 0 ? 1.0 : 1.0 / (sampleCount + 1);
				renderFullscreenPass(accumMat, nextAccum);
				accumReadIsA = !accumReadIsA;

				// Copy converged linear HDR radiance into prevLitTarget for next frame's SSR
				accumMat.uniforms.tPrev.value = nextAccum.texture;
				accumMat.uniforms.tCurr.value = nextAccum.texture;
				accumMat.uniforms.uWeight.value = 1.0;
				renderFullscreenPass(accumMat, prevLitTarget);
				prevLitValid = true;

				// Pass F: Tone-map & output to canvas (preserving CSS --viewport-bg where alpha == 0)
				compositeMat.uniforms.tAccum.value = nextAccum.texture;
				compositeMat.uniforms.uToneMapping.value = settings.studioToneMapping === 'agx' ? 1 : 0;
				compositeMat.uniforms.uExposure.value = 1.05;
				renderer.setRenderTarget(null);
				renderer.setClearColor(0x000000, 0.0);
				renderer.clear(true, true, false);
				quadMesh.material = compositeMat;
				renderer.render(quadScene, quadCamera);

				sampleCount += 1;
				passCount += 1;
				const settled = sampleCount >= TARGET_SAMPLES;

				updateStudioStats({
					active: true,
					environmentLoaded: Boolean(scene.environment),
					sampleCount,
					targetSamples: TARGET_SAMPLES,
					settled,
					passCount,
					aoEnabled: Boolean(settings.studioAO),
					ssrEnabled: Boolean(settings.studioSSR),
					groundShadowVisible: Boolean(settings.studioGroundShadow),
					floatRenderable
				});

				if (!settled) {
					invalidate();
				}
			} finally {
				renderer.setClearColor(_savedClearColor, savedClearAlpha);
				if (savedTarget !== null) {
					renderer.setRenderTarget(savedTarget);
				}
			}
		},
		{ stage: renderStage, autoInvalidate: false }
	);

	onDestroy(() => {
		setStudioFreshnessCheck(null);
		resetStudioSharedUniforms();
		if (scene) scene.environment = null;
		if (renderer) disposeStudioEnvironments(renderer);
		disposeTargets();
		quadGeo.dispose();
		gbufferMat.dispose();
		gtaoMat.dispose();
		blurMat.dispose();
		ssrMat.dispose();
		accumMat.dispose();
		compositeMat.dispose();
	});
</script>

{#if false}{/if}
