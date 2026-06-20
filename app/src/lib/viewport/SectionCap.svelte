<script>
	/**
	 * Capped section view — stencil cap.
	 *
	 * Implements the standard three.js stencil-cap technique (mirroring the
	 * official `examples/webgl_clipping_stencil`). For each solid body geometry:
	 *
	 *  1. Back-face pass: render the body's BACK faces, writing the stencil
	 *     buffer only (no color, no depth), incrementing the stencil where back
	 *     faces are seen.
	 *  2. Front-face pass: render the FRONT faces, decrementing the stencil.
	 *
	 * After both passes the stencil is non-zero exactly where the clip plane
	 * cuts through solid interior. A large cap quad lying ON the clip plane is
	 * then drawn with `stencilFunc: NotEqual, stencilRef: 0` so it fills only
	 * the interior of the cut — giving a solid section instead of a hollow
	 * X-ray. The cap quad is NOT clipped by its own plane (it lies on it) but
	 * IS clipped by any other active plane (single-plane MVP → none).
	 *
	 * Threlte v8 / Svelte 5 notes:
	 *  - The cap quad is oriented via a `rotation={[e.x,e.y,e.z]}` euler array,
	 *    NOT a quaternion prop (Threlte silently drops quaternion props).
	 *  - Stencil materials are reused per-frame; render order is set so the
	 *    stencil passes draw before the cap.
	 */
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import { getMeshes, getSectionState } from '$lib/engine/store.svelte.js';
	import { buildSectionClipPlane } from './sectionPlane.js';

	const CAP_COLOR = new THREE.Color(0x8899aa);
	/** Half-size of the cap quad (meters). Large enough to cover any model. */
	const CAP_HALF = 1000;

	// Build BufferGeometries for each body (same source as CadModel: getMeshes()).
	let bodyGeometries = $derived.by(() => {
		const meshData = getMeshes();
		if (!meshData || meshData.length === 0) return [];
		return meshData.map((m) => {
			const geo = new THREE.BufferGeometry();
			geo.setAttribute('position', new THREE.BufferAttribute(m.vertices, 3));
			if (m.normals && m.normals.length > 0) {
				geo.setAttribute('normal', new THREE.BufferAttribute(m.normals, 3));
			}
			if (m.indices && m.indices.length > 0) {
				geo.setIndex(new THREE.BufferAttribute(m.indices, 1));
			}
			if (!m.normals || m.normals.length === 0) geo.computeVertexNormals();
			return geo;
		});
	});

	// Section clip plane (null when inactive).
	let clipPlane = $derived.by(() => {
		const s = getSectionState();
		if (!s.active || !s.plane) return null;
		return buildSectionClipPlane(s.plane, s.flipped, s.offset);
	});

	let active = $derived.by(() => {
		const s = getSectionState();
		return s.active && !!s.plane && bodyGeometries.length > 0 && !!clipPlane;
	});

	// Cap quad transform: a plane lying on the clip plane. PlaneGeometry's
	// default normal is +Z, so orient +Z onto the clip plane normal and place
	// the quad at the projection of the origin onto the plane.
	let capTransform = $derived.by(() => {
		const plane = clipPlane;
		if (!plane) return { position: [0, 0, 0], rotation: [0, 0, 0] };
		const n = plane.normal.clone().normalize();
		const q = new THREE.Quaternion().setFromUnitVectors(new THREE.Vector3(0, 0, 1), n);
		const e = new THREE.Euler().setFromQuaternion(q);
		// A point on the plane: -constant * normal.
		const p = n.clone().multiplyScalar(-plane.constant);
		return { position: [p.x, p.y, p.z], rotation: [e.x, e.y, e.z] };
	});

	/**
	 * Back-face stencil material — increments stencil on back faces.
	 * Writes stencil only (no color, no depth).
	 */
	function makeBackStencilMat(plane) {
		const m = new THREE.MeshBasicMaterial();
		m.depthWrite = false;
		m.depthTest = false;
		m.colorWrite = false;
		m.side = THREE.BackSide;
		m.clippingPlanes = [plane];
		m.stencilWrite = true;
		m.stencilFunc = THREE.AlwaysStencilFunc;
		m.stencilFail = THREE.IncrementWrapStencilOp;
		m.stencilZFail = THREE.IncrementWrapStencilOp;
		m.stencilZPass = THREE.IncrementWrapStencilOp;
		return m;
	}

	/**
	 * Front-face stencil material — decrements stencil on front faces.
	 */
	function makeFrontStencilMat(plane) {
		const m = new THREE.MeshBasicMaterial();
		m.depthWrite = false;
		m.depthTest = false;
		m.colorWrite = false;
		m.side = THREE.FrontSide;
		m.clippingPlanes = [plane];
		m.stencilWrite = true;
		m.stencilFunc = THREE.AlwaysStencilFunc;
		m.stencilFail = THREE.DecrementWrapStencilOp;
		m.stencilZFail = THREE.DecrementWrapStencilOp;
		m.stencilZPass = THREE.DecrementWrapStencilOp;
		return m;
	}

	// Per-body stencil materials (rebuilt when the clip plane changes).
	let backMats = $derived.by(() =>
		clipPlane ? bodyGeometries.map(() => makeBackStencilMat(clipPlane)) : []
	);
	let frontMats = $derived.by(() =>
		clipPlane ? bodyGeometries.map(() => makeFrontStencilMat(clipPlane)) : []
	);

	// Cap fill material — draws only where stencil != 0 (inside the solid), and
	// resets the stencil to 0 afterward so passes don't leak between frames.
	let capMaterial = $derived.by(() => {
		const m = new THREE.MeshStandardMaterial({
			color: CAP_COLOR,
			metalness: 0.1,
			roughness: 0.75,
			side: THREE.DoubleSide
		});
		// The cap lies on the clip plane → must NOT be clipped by it.
		m.clippingPlanes = [];
		m.stencilWrite = true;
		m.stencilRef = 0;
		m.stencilFunc = THREE.NotEqualStencilFunc;
		m.stencilFail = THREE.ReplaceStencilOp;
		m.stencilZFail = THREE.ReplaceStencilOp;
		m.stencilZPass = THREE.ReplaceStencilOp;
		return m;
	});

	const capGeometry = new THREE.PlaneGeometry(CAP_HALF * 2, CAP_HALF * 2);
</script>

{#if active}
	<!-- Stencil passes: write the stencil buffer where the plane cuts solid. -->
	{#each bodyGeometries as geo, i (i)}
		<T.Mesh
			geometry={geo}
			material={backMats[i]}
			frustumCulled={false}
			renderOrder={1}
			raycast={() => {}}
		/>
		<T.Mesh
			geometry={geo}
			material={frontMats[i]}
			frustumCulled={false}
			renderOrder={2}
			raycast={() => {}}
		/>
	{/each}

	<!-- Cap fill quad: drawn after the stencil passes, only where stencil != 0. -->
	<T.Mesh
		geometry={capGeometry}
		material={capMaterial}
		position={capTransform.position}
		rotation={capTransform.rotation}
		frustumCulled={false}
		renderOrder={3}
		raycast={() => {}}
	/>
{/if}

{#if false}{/if}
