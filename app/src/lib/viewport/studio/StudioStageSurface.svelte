<script>
	/**
	 * Grounded 3D Physical Tabletop Stage (`specs/studio_pbr_renderer.md` I13).
	 *
	 * Places a 3D chamfered slab (Wooden Workbench, Walnut Office Desk, Polished
	 * Granite Countertop, or Green Cutting Mat) flush beneath `bbox.min.z`:
	 *   - Participates in Studio G-buffer, GTAO, and SSR (`waffleType: 'studio-stage'`)
	 *     so parts cast physical contact occlusion and reflections onto the table
	 *   - Non-pickable (`raycast={() => {}}`) and ignored by `Fit All` framing /
	 *     `getMeshes()` (`waffleType !== 'model'`)
	 *   - Hides in Classic Shaded mode, Sketch mode, Section view, empty scenes,
	 *     `surfaceId === 'none'`, or when looking upward from below `bbox.min.z`.
	 */
	import { T, useThrelte, useTask } from '@threlte/core';
	import { onDestroy } from 'svelte';
	import * as THREE from 'three';
	import {
		getMeshes,
		getSketchMode,
		getSectionState,
		isBodyVisible
	} from '$lib/engine/store.svelte.js';
	import { getSettings } from '$lib/ui/settings.svelte.js';
	import { placementMatrix } from '../placement.js';
	import { resolveStudioSurface } from './studioEnvironment.js';
	import { createStudioStageMaterial } from './studioMaterial.js';
	import { updateStudioStats, bumpStudioSurfaceRevision } from './studioState.js';

	const { renderer, camera, invalidate } = useThrelte();

	const SURFACE_CONFIGS = {
		workbench: {
			diffUrl: '/surfaces/workbench_diff.webp',
			norUrl: '/surfaces/workbench_nor.webp',
			roughUrl: '/surfaces/workbench_rough.webp',
			fallbackColor: 0x8b6240,
			roughness: 0.82,
			metalness: 0.0,
			clearcoat: 0.10,
			clearcoatRoughness: 0.40,
			normalScale: 0.85,
			thicknessRatio: 0.075,
			tileScale: 1.75
		},
		desk: {
			diffUrl: '/surfaces/desk_diff.webp',
			norUrl: '/surfaces/desk_nor.webp',
			roughUrl: '/surfaces/desk_rough.webp',
			fallbackColor: 0x62422c,
			roughness: 0.52,
			metalness: 0.0,
			clearcoat: 0.36,
			clearcoatRoughness: 0.18,
			normalScale: 0.55,
			thicknessRatio: 0.045,
			tileScale: 1.65
		},
		granite: {
			diffUrl: '/surfaces/granite_diff.webp',
			norUrl: '/surfaces/granite_nor.webp',
			roughUrl: '/surfaces/granite_rough.webp',
			fallbackColor: 0x484a4e,
			roughness: 0.14,
			metalness: 0.02,
			clearcoat: 0.92,
			clearcoatRoughness: 0.05,
			normalScale: 0.35,
			thicknessRatio: 0.055,
			tileScale: 1.25
		},
		'cutting-mat': {
			diffUrl: '/surfaces/cutting_mat_diff.webp',
			norUrl: null,
			roughUrl: null,
			fallbackColor: 0x1e5246,
			roughness: 0.72,
			metalness: 0.0,
			clearcoat: 0.04,
			clearcoatRoughness: 0.5,
			normalScale: 0.0,
			thicknessRatio: 0.010,
			tileScale: 1.35
		}
	};

	/** @type {Map<string, THREE.MeshPhysicalMaterial>} */
	const MAT_CACHE = new Map();
	/** @type {Map<string, Promise<THREE.MeshPhysicalMaterial>>} */
	const MAT_PROMISES = new Map();
	/** @type {THREE.Texture[]} */
	const OWNED_TEXTURES = [];

	function loadTex(url, isSrgb) {
		const loader = new THREE.TextureLoader();
		return new Promise((resolve, reject) => {
			loader.load(
				url,
				(tex) => {
					tex.colorSpace = isSrgb ? THREE.SRGBColorSpace : THREE.NoColorSpace;
					tex.wrapS = THREE.RepeatWrapping;
					tex.wrapT = THREE.RepeatWrapping;
					tex.generateMipmaps = true;
					tex.minFilter = THREE.LinearMipmapLinearFilter;
					tex.magFilter = THREE.LinearFilter;
					tex.anisotropy = Math.min(16, renderer?.capabilities?.getMaxAnisotropy?.() || 4);
					tex.needsUpdate = true;
					OWNED_TEXTURES.push(tex);
					resolve(tex);
				},
				undefined,
				reject
			);
		});
	}

	function loadStageMaterial(surfaceId) {
		const cached = MAT_CACHE.get(surfaceId);
		if (cached) return Promise.resolve(cached);
		let pending = MAT_PROMISES.get(surfaceId);
		if (pending) return pending;

		const cfg = SURFACE_CONFIGS[surfaceId];
		if (!cfg) return Promise.reject(new Error(`Unknown stage surface: ${surfaceId}`));

		pending = Promise.all([
			loadTex(cfg.diffUrl, true),
			cfg.norUrl ? loadTex(cfg.norUrl, false) : Promise.resolve(null),
			cfg.roughUrl ? loadTex(cfg.roughUrl, false) : Promise.resolve(null)
		])
			.then(([diff, nor, rough]) => {
				const mat = createStudioStageMaterial({
					map: diff,
					normalMap: nor,
					roughnessMap: rough,
					roughness: cfg.roughness,
					metalness: cfg.metalness,
					clearcoat: cfg.clearcoat,
					clearcoatRoughness: cfg.clearcoatRoughness,
					normalScale: cfg.normalScale
				});
				MAT_CACHE.set(surfaceId, mat);
				return mat;
			})
			.catch((err) => {
				MAT_PROMISES.delete(surfaceId);
				throw err;
			});

		MAT_PROMISES.set(surfaceId, pending);
		return pending;
	}

	/**
	 * Build a watertight 3D chamfered rectangular bench/countertop slab with
	 * outward unit normals and world-proportioned continuous box UVs.
	 */
	function buildChamferedSlabGeometry({ x0, x1, y0, y1, zBottom, zTop, bevel, tileMeters }) {
		const xi0 = x0 + bevel;
		const xi1 = x1 - bevel;
		const yi0 = y0 + bevel;
		const yi1 = y1 - bevel;
		const zb = zTop - bevel;
		const invTile = 1.0 / Math.max(tileMeters, 1e-5);
		const s2 = Math.SQRT1_2;

		/** @type {number[]} */
		const pos = [];
		/** @type {number[]} */
		const nor = [];
		/** @type {number[]} */
		const uvs = [];
		/** @type {number[]} */
		const idx = [];

		function addQuad(a, b, c, d, nx, ny, nz, uva, uvb, uvc, uvd) {
			const base = pos.length / 3;
			pos.push(a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2], d[0], d[1], d[2]);
			nor.push(nx, ny, nz, nx, ny, nz, nx, ny, nz, nx, ny, nz);
			uvs.push(uva[0], uva[1], uvb[0], uvb[1], uvc[0], uvc[1], uvd[0], uvd[1]);
			idx.push(base, base + 1, base + 2, base, base + 2, base + 3);
		}

		// Top face (+Z)
		addQuad(
			[xi0, yi0, zTop],
			[xi1, yi0, zTop],
			[xi1, yi1, zTop],
			[xi0, yi1, zTop],
			0, 0, 1,
			[xi0 * invTile, yi0 * invTile],
			[xi1 * invTile, yi0 * invTile],
			[xi1 * invTile, yi1 * invTile],
			[xi0 * invTile, yi1 * invTile]
		);

		const bevDrop = bevel * Math.SQRT2;
		const sideDrop = Math.max(0, zb - zBottom);

		// Front (-Y) chamfer + vertical apron
		addQuad(
			[x0, y0, zb],
			[x1, y0, zb],
			[xi1, yi0, zTop],
			[xi0, yi0, zTop],
			0, -s2, s2,
			[x0 * invTile, (yi0 - bevDrop) * invTile],
			[x1 * invTile, (yi0 - bevDrop) * invTile],
			[xi1 * invTile, yi0 * invTile],
			[xi0 * invTile, yi0 * invTile]
		);
		addQuad(
			[x0, y0, zBottom],
			[x1, y0, zBottom],
			[x1, y0, zb],
			[x0, y0, zb],
			0, -1, 0,
			[x0 * invTile, (yi0 - bevDrop - sideDrop) * invTile],
			[x1 * invTile, (yi0 - bevDrop - sideDrop) * invTile],
			[x1 * invTile, (yi0 - bevDrop) * invTile],
			[x0 * invTile, (yi0 - bevDrop) * invTile]
		);

		// Right (+X) chamfer + vertical apron
		addQuad(
			[x1, y0, zb],
			[x1, y1, zb],
			[xi1, yi1, zTop],
			[xi1, yi0, zTop],
			s2, 0, s2,
			[(xi1 + bevDrop) * invTile, y0 * invTile],
			[(xi1 + bevDrop) * invTile, y1 * invTile],
			[xi1 * invTile, yi1 * invTile],
			[xi1 * invTile, yi0 * invTile]
		);
		addQuad(
			[x1, y0, zBottom],
			[x1, y1, zBottom],
			[x1, y1, zb],
			[x1, y0, zb],
			1, 0, 0,
			[(xi1 + bevDrop + sideDrop) * invTile, y0 * invTile],
			[(xi1 + bevDrop + sideDrop) * invTile, y1 * invTile],
			[(xi1 + bevDrop) * invTile, y1 * invTile],
			[(xi1 + bevDrop) * invTile, y0 * invTile]
		);

		// Back (+Y) chamfer + vertical apron
		addQuad(
			[x1, y1, zb],
			[x0, y1, zb],
			[xi0, yi1, zTop],
			[xi1, yi1, zTop],
			0, s2, s2,
			[x1 * invTile, (yi1 + bevDrop) * invTile],
			[x0 * invTile, (yi1 + bevDrop) * invTile],
			[xi0 * invTile, yi1 * invTile],
			[xi1 * invTile, yi1 * invTile]
		);
		addQuad(
			[x1, y1, zBottom],
			[x0, y1, zBottom],
			[x0, y1, zb],
			[x1, y1, zb],
			0, 1, 0,
			[x1 * invTile, (yi1 + bevDrop + sideDrop) * invTile],
			[x0 * invTile, (yi1 + bevDrop + sideDrop) * invTile],
			[x0 * invTile, (yi1 + bevDrop) * invTile],
			[x1 * invTile, (yi1 + bevDrop) * invTile]
		);

		// Left (-X) chamfer + vertical apron
		addQuad(
			[x0, y1, zb],
			[x0, y0, zb],
			[xi0, yi0, zTop],
			[xi0, yi1, zTop],
			-s2, 0, s2,
			[(xi0 - bevDrop) * invTile, y1 * invTile],
			[(xi0 - bevDrop) * invTile, y0 * invTile],
			[xi0 * invTile, yi0 * invTile],
			[xi0 * invTile, yi1 * invTile]
		);
		addQuad(
			[x0, y1, zBottom],
			[x0, y0, zBottom],
			[x0, y0, zb],
			[x0, y1, zb],
			-1, 0, 0,
			[(xi0 - bevDrop - sideDrop) * invTile, y1 * invTile],
			[(xi0 - bevDrop - sideDrop) * invTile, y0 * invTile],
			[(xi0 - bevDrop) * invTile, y0 * invTile],
			[(xi0 - bevDrop) * invTile, y1 * invTile]
		);

		// Bottom (-Z)
		addQuad(
			[x0, y1, zBottom],
			[x1, y1, zBottom],
			[x1, y0, zBottom],
			[x0, y0, zBottom],
			0, 0, -1,
			[x0 * invTile, y1 * invTile],
			[x1 * invTile, y1 * invTile],
			[x1 * invTile, y0 * invTile],
			[x0 * invTile, y0 * invTile]
		);

		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3));
		geo.setAttribute('normal', new THREE.Float32BufferAttribute(nor, 3));
		geo.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2));
		geo.setIndex(idx);
		geo.computeBoundingSphere();
		return geo;
	}

	let settings = $derived(getSettings());
	let studioActive = $derived(settings.viewportShading === 'studio');
	let surfaceId = $derived(resolveStudioSurface(settings.studioSurface, settings.studioEnvironment));
	let inSketch = $derived(getSketchMode()?.active ?? false);
	let inSection = $derived(getSectionState()?.active ?? false);

	let cameraAboveFloor = $state(true);
	/** @type {THREE.Material | null} */
	let activeMaterial = $state(null);
	let matReqSeq = 0;

	const fallbackMat = new THREE.MeshStandardMaterial({ color: 0x77624c, roughness: 0.75 });

	// Load & bind PBR tabletop material for the active surfaceId
	$effect(() => {
		const active = studioActive;
		const sid = surfaceId;

		if (!active || sid === 'none') {
			matReqSeq += 1;
			activeMaterial = null;
			updateStudioStats({
				surfaceId: sid,
				surfaceLoaded: true,
				surfaceLoading: false
			});
			bumpStudioSurfaceRevision();
			if (active) invalidate();
			return;
		}

		const cached = MAT_CACHE.get(sid);
		if (cached) {
			activeMaterial = cached;
			updateStudioStats({
				surfaceId: sid,
				surfaceLoaded: true,
				surfaceLoading: false
			});
			bumpStudioSurfaceRevision();
			invalidate();
			return;
		}

		const cfg = SURFACE_CONFIGS[sid];
		fallbackMat.color.setHex(cfg?.fallbackColor ?? 0x77624c);
		activeMaterial = fallbackMat;
		const req = ++matReqSeq;
		updateStudioStats({
			surfaceId: sid,
			surfaceLoaded: false,
			surfaceLoading: true
		});
		invalidate();

		loadStageMaterial(sid)
			.then((mat) => {
				if (req !== matReqSeq || !studioActive) return;
				activeMaterial = mat;
				updateStudioStats({
					surfaceId: sid,
					surfaceLoaded: true,
					surfaceLoading: false
				});
				bumpStudioSurfaceRevision();
				invalidate();
			})
			.catch(() => {
				if (req !== matReqSeq || !studioActive) return;
				updateStudioStats({
					surfaceId: sid,
					surfaceLoaded: true,
					surfaceLoading: false
				});
				bumpStudioSurfaceRevision();
				invalidate();
			});
	});

	let slabData = $derived.by(() => {
		if (!studioActive || surfaceId === 'none') return null;
		const cfg = SURFACE_CONFIGS[surfaceId];
		if (!cfg) return null;

		const rawMeshes = getMeshes();
		if (!rawMeshes || rawMeshes.length === 0) return null;
		const visible = rawMeshes.filter(
			(m) => !m.context && isBodyVisible(m.bodyId) && m.vertices?.length >= 9
		);
		if (visible.length === 0) return null;

		let minX = Infinity, maxX = -Infinity;
		let minY = Infinity, maxY = -Infinity;
		let minZ = Infinity, maxZ = -Infinity;
		const vTmp = new THREE.Vector3();

		for (let mi = 0; mi < visible.length; mi++) {
			const verts = visible[mi].vertices;
			const mat = visible[mi].transform ? placementMatrix(visible[mi].transform) : null;
			const step = Math.max(1, Math.floor(verts.length / (3 * 4096))) * 3;
			for (let i = 0; i < verts.length; i += step) {
				let x = verts[i], y = verts[i + 1], z = verts[i + 2];
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

		if (!Number.isFinite(minZ)) return null;

		const spanX = Math.max(maxX - minX, 1e-4);
		const spanY = Math.max(maxY - minY, 1e-4);
		const spanZ = Math.max(maxZ - minZ, 1e-4);
		const maxSpan = Math.max(spanX, spanY, spanZ);

		// Frame the tabletop like a real shop workbench / office desk / countertop:
		// generous width & foreground depth, with a natural back edge behind the part
		// so the 4K room backdrop rises visibly behind the bench in ISO & Front views!
		const x0 = minX - maxSpan * 1.45;
		const x1 = maxX + maxSpan * 1.45;
		const y0 = minY - maxSpan * 1.35;
		const y1 = maxY + maxSpan * 0.72;
		const zTop = minZ - maxSpan * 0.0007;
		const thickness = Math.max(maxSpan * cfg.thicknessRatio, 1e-4);
		const zBottom = zTop - thickness;
		const bevel = Math.min(thickness * 0.22, maxSpan * 0.008);
		const tileMeters = maxSpan * cfg.tileScale;

		const geometry = buildChamferedSlabGeometry({
			x0,
			x1,
			y0,
			y1,
			zBottom,
			zTop,
			bevel,
			tileMeters
		});

		return { geometry, zTop };
	});

	let prevGeo = null;
	$effect(() => {
		const g = slabData?.geometry ?? null;
		if (prevGeo && prevGeo !== g) {
			prevGeo.dispose();
		}
		prevGeo = g;
	});

	useTask(
		() => {
			const cam = camera.current;
			if (!cam || !slabData) return;
			const nextAbove = cam.position.z >= slabData.zTop;
			if (nextAbove !== cameraAboveFloor) {
				cameraAboveFloor = nextAbove;
				invalidate();
			}
		},
		{ autoInvalidate: false }
	);

	onDestroy(() => {
		prevGeo?.dispose();
		fallbackMat.dispose();
		for (const m of MAT_CACHE.values()) m.dispose();
		MAT_CACHE.clear();
		MAT_PROMISES.clear();
		for (const t of OWNED_TEXTURES) t.dispose();
		OWNED_TEXTURES.length = 0;
	});

	let showSlab = $derived(
		Boolean(
			slabData &&
				activeMaterial &&
				studioActive &&
				surfaceId !== 'none' &&
				!inSketch &&
				!inSection &&
				cameraAboveFloor
		)
	);
</script>

{#if showSlab && slabData && activeMaterial}
	<T.Mesh
		geometry={slabData.geometry}
		material={activeMaterial}
		renderOrder={0}
		raycast={() => {}}
		userData={{ waffleType: 'studio-stage' }}
	/>
{/if}
