<script>
	/**
	 * Z-Up Studio Ground Contact-Shadow Catcher (`specs/studio_pbr_renderer.md` §1, §4 I8).
	 *
	 * Computes a cached height-weighted vertical projection of visible solid bodies
	 * into a blurred `128x128` contact-shadow texture once per geometry/visibility
	 * change, and renders a single non-pickable (`raycast={() => {}}`) floor quad
	 * slightly below `bbox.min.z`.
	 */
	import { T, useThrelte, useTask } from '@threlte/core';
	import * as THREE from 'three';
	import {
		getMeshes,
		getSketchMode,
		getSectionState,
		isBodyVisible
	} from '$lib/engine/store.svelte.js';
	import { getSettings } from '$lib/ui/settings.svelte.js';
	import { placementMatrix } from '../placement.js';

	const { camera } = useThrelte();

	const TEX_RES = 128;

	let settings = $derived(getSettings());
	let inSketch = $derived(getSketchMode()?.active ?? false);
	let inSection = $derived(getSectionState()?.active ?? false);

	let cameraAboveFloor = $state(true);

	/** Separable 1D blur pass with radius `R` over a `RES x RES` Float32Array. */
	function blurSeparable(grid, tmp, res, radius) {
		// Horizontal
		for (let y = 0; y < res; y++) {
			const row = y * res;
			for (let x = 0; x < res; x++) {
				let sum = 0;
				let wSum = 0;
				for (let k = -radius; k <= radius; k++) {
					const sx = Math.min(res - 1, Math.max(0, x + k));
					const w = radius + 1 - Math.abs(k);
					sum += grid[row + sx] * w;
					wSum += w;
				}
				tmp[row + x] = sum / wSum;
			}
		}
		// Vertical
		for (let y = 0; y < res; y++) {
			for (let x = 0; x < res; x++) {
				let sum = 0;
				let wSum = 0;
				for (let k = -radius; k <= radius; k++) {
					const sy = Math.min(res - 1, Math.max(0, y + k));
					const w = radius + 1 - Math.abs(k);
					sum += tmp[sy * res + x] * w;
					wSum += w;
				}
				grid[y * res + x] = sum / wSum;
			}
		}
	}

	/**
	 * Compute world-space floor bounds and bake a soft height-weighted contact
	 * shadow texture across the XY footprint of all visible solid bodies.
	 */
	let shadowData = $derived.by(() => {
		if (settings.viewportShading !== 'studio' || !settings.studioGroundShadow) return null;
		const rawMeshes = getMeshes();
		if (!rawMeshes || rawMeshes.length === 0) return null;
		const visible = rawMeshes.filter((m) => !m.context && isBodyVisible(m.bodyId) && m.vertices?.length >= 9);
		if (visible.length === 0) return null;

		let minX = Infinity, maxX = -Infinity;
		let minY = Infinity, maxY = -Infinity;
		let minZ = Infinity, maxZ = -Infinity;

		const mats = visible.map((m) => placementMatrix(m.transform));
		const vTmp = new THREE.Vector3();

		for (let mi = 0; mi < visible.length; mi++) {
			const verts = visible[mi].vertices;
			const mat = mats[mi];
			const isIdentity = !visible[mi].transform;
			for (let i = 0; i < verts.length; i += 3) {
				let x = verts[i], y = verts[i + 1], z = verts[i + 2];
				if (!isIdentity) {
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

		const spanX = Math.max(maxX - minX, 1e-5);
		const spanY = Math.max(maxY - minY, 1e-5);
		const spanZ = Math.max(maxZ - minZ, 1e-5);
		const maxSpan = Math.max(spanX, spanY, spanZ);

		// Pad the floor quad by 65% around the part XY bounds so the soft penumbra
		// apron tapers cleanly to zero before reaching the quad border.
		const padX = Math.max(spanX, spanY) * 0.65;
		const padY = Math.max(spanX, spanY) * 0.65;
		const quadMinX = minX - padX;
		const quadMaxX = maxX + padX;
		const quadMinY = minY - padY;
		const quadMaxY = maxY + padY;
		const quadW = quadMaxX - quadMinX;
		const quadH = quadMaxY - quadMinY;

		const grid = new Float32Array(TEX_RES * TEX_RES);
		const tmp = new Float32Array(TEX_RES * TEX_RES);
		const falloffZ = Math.max(spanZ * 0.85, maxSpan * 0.35);

		const p0 = new THREE.Vector3();
		const p1 = new THREE.Vector3();
		const p2 = new THREE.Vector3();

		for (let mi = 0; mi < visible.length; mi++) {
			const m = visible[mi];
			const verts = m.vertices;
			const idx = m.indices;
			const mat = mats[mi];
			const isIdentity = !m.transform;
			const triCount = idx ? Math.floor(idx.length / 3) : Math.floor(verts.length / 9);

			for (let t = 0; t < triCount; t++) {
				const i0 = idx ? idx[t * 3] * 3 : t * 9;
				const i1 = idx ? idx[t * 3 + 1] * 3 : t * 9 + 3;
				const i2 = idx ? idx[t * 3 + 2] * 3 : t * 9 + 6;
				p0.set(verts[i0], verts[i0 + 1], verts[i0 + 2]);
				p1.set(verts[i1], verts[i1 + 1], verts[i1 + 2]);
				p2.set(verts[i2], verts[i2 + 1], verts[i2 + 2]);
				if (!isIdentity) {
					p0.applyMatrix4(mat);
					p1.applyMatrix4(mat);
					p2.applyMatrix4(mat);
				}

				const triMinZ = Math.min(p0.z, p1.z, p2.z);
				const heightWeight = Math.exp(-Math.max(0, triMinZ - minZ) / falloffZ);
				if (heightWeight < 0.03) continue;

				const u0 = ((p0.x - quadMinX) / quadW) * (TEX_RES - 1);
				const v0 = ((p0.y - quadMinY) / quadH) * (TEX_RES - 1);
				const u1 = ((p1.x - quadMinX) / quadW) * (TEX_RES - 1);
				const v1 = ((p1.y - quadMinY) / quadH) * (TEX_RES - 1);
				const u2 = ((p2.x - quadMinX) / quadW) * (TEX_RES - 1);
				const v2 = ((p2.y - quadMinY) / quadH) * (TEX_RES - 1);

				const bx0 = Math.max(0, Math.floor(Math.min(u0, u1, u2)));
				const bx1 = Math.min(TEX_RES - 1, Math.ceil(Math.max(u0, u1, u2)));
				const by0 = Math.max(0, Math.floor(Math.min(v0, v1, v2)));
				const by1 = Math.min(TEX_RES - 1, Math.ceil(Math.max(v0, v1, v2)));

				const area = (u1 - u0) * (v2 - v0) - (u2 - u0) * (v1 - v0);
				if (Math.abs(area) < 0.25) {
					// Sub-texel triangle splat
					const cx = Math.min(TEX_RES - 1, Math.max(0, Math.round((u0 + u1 + u2) / 3)));
					const cy = Math.min(TEX_RES - 1, Math.round((v0 + v1 + v2) / 3));
					if (cy >= 0 && grid[cy * TEX_RES + cx] < heightWeight) {
						grid[cy * TEX_RES + cx] = heightWeight;
					}
					continue;
				}
				const sign = area > 0 ? 1 : -1;
				for (let py = by0; py <= by1; py++) {
					const ry = py + 0.5;
					for (let px = bx0; px <= bx1; px++) {
						const rx = px + 0.5;
						const e0 = ((u1 - u0) * (ry - v0) - (v1 - v0) * (rx - u0)) * sign;
						const e1 = ((u2 - u1) * (ry - v1) - (v2 - v1) * (rx - u1)) * sign;
						const e2 = ((u0 - u2) * (ry - v2) - (v0 - v2) * (rx - u2)) * sign;
						if (e0 >= -0.35 && e1 >= -0.35 && e2 >= -0.35) {
							const cell = py * TEX_RES + px;
							if (grid[cell] < heightWeight) grid[cell] = heightWeight;
						}
					}
				}
			}
		}

		// Two-scale penumbra: crisp contact core + broad ambient ground shadow apron
		const core = new Float32Array(grid);
		blurSeparable(core, tmp, TEX_RES, 5);
		blurSeparable(grid, tmp, TEX_RES, 11);
		blurSeparable(grid, tmp, TEX_RES, 11);

		const rgba = new Uint8Array(TEX_RES * TEX_RES * 4);
		for (let y = 0; y < TEX_RES; y++) {
			const edgeY = Math.min(y, TEX_RES - 1 - y) / (TEX_RES * 0.16);
			for (let x = 0; x < TEX_RES; x++) {
				const edgeX = Math.min(x, TEX_RES - 1 - x) / (TEX_RES * 0.16);
				const borderFade = Math.min(1, Math.max(0, Math.min(edgeX, edgeY)));
				const idx = y * TEX_RES + x;
				const combined = Math.min(1, core[idx] * 0.55 + grid[idx] * 0.72) * borderFade;
				const alpha = Math.round(Math.min(1, combined * 0.78) * 255);
				const o = idx * 4;
				rgba[o] = 0;
				rgba[o + 1] = 0;
				rgba[o + 2] = 0;
				rgba[o + 3] = alpha;
			}
		}

		const texture = new THREE.DataTexture(rgba, TEX_RES, TEX_RES, THREE.RGBAFormat, THREE.UnsignedByteType);
		texture.minFilter = THREE.LinearFilter;
		texture.magFilter = THREE.LinearFilter;
		texture.wrapS = THREE.ClampToEdgeWrapping;
		texture.wrapT = THREE.ClampToEdgeWrapping;
		texture.needsUpdate = true;

		const geometry = new THREE.PlaneGeometry(quadW, quadH);
		const material = new THREE.MeshBasicMaterial({
			map: texture,
			transparent: true,
			depthWrite: false,
			toneMapped: false,
			side: THREE.FrontSide
		});

		const floorZ = minZ - maxSpan * 0.0015;
		return {
			geometry,
			material,
			texture,
			floorZ,
			position: [(quadMinX + quadMaxX) * 0.5, (quadMinY + quadMaxY) * 0.5, floorZ]
		};
	});

	// Dispose stale geometry/texture/material when shadowData rebuilds
	let prevData = null;
	$effect(() => {
		const curr = shadowData;
		if (prevData && prevData !== curr) {
			prevData.geometry.dispose();
			prevData.material.dispose();
			prevData.texture.dispose();
		}
		prevData = curr;
		return () => {
			if (prevData) {
				prevData.geometry.dispose();
				prevData.material.dispose();
				prevData.texture.dispose();
				prevData = null;
			}
		};
	});

	// Invariant I8: hide the floor shadow when viewing upward from below floorZ.
	// autoInvalidate: false so this check never forces idle frames to render!
	const _camDir = new THREE.Vector3();
	useTask(
		() => {
			const cam = camera.current;
			const sd = shadowData;
			if (!cam || !sd) return;
			cam.getWorldDirection(_camDir);
			// Looking down onto the +Z floor means _camDir.z < -0.02 (or camera above floorZ)
			const nextAbove = cam.position.z > sd.floorZ && _camDir.z < -0.02;
			if (nextAbove !== cameraAboveFloor) {
				cameraAboveFloor = nextAbove;
			}
		},
		{ autoInvalidate: false }
	);

	let visible = $derived(
		!!shadowData && !inSketch && !inSection && cameraAboveFloor
	);
</script>

{#if visible && shadowData}
	<T.Mesh
		geometry={shadowData.geometry}
		material={shadowData.material}
		position={shadowData.position}
		rotation={[0, 0, 0]}
		renderOrder={-10}
		frustumCulled={false}
		userData={{ waffleType: 'studioGroundShadow' }}
		raycast={() => {}}
	/>
{/if}

{#if false}{/if}
