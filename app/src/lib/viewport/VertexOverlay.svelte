<script>
	import { T, useThrelte } from '@threlte/core';
	import { onMount } from 'svelte';
	import * as THREE from 'three';
	import {
		getMeshes,
		getHoveredRef,
		getSelectedRefs,
		setHoveredRef,
		selectRef,
		geomRefEquals,
		getCameraObject,
		getSketchMode,
		getSectionState,
		isBodyVisible,
		setRenderedVertexCount,
		isProjectToolActive
	} from '$lib/engine/store.svelte.js';
	import { buildSectionClipPlane } from './sectionPlane.js';

	const { renderer } = useThrelte();

	const DEFAULT_COLOR = new THREE.Color(0x666688);
	const HOVER_COLOR = new THREE.Color(0x88ccff);
	const SELECTED_COLOR = new THREE.Color(0x44aaff);

	/** Pixel threshold for vertex picking */
	const VERTEX_PICK_THRESHOLD = 8;

	/** Deduplication epsilon for vertex positions */
	const DEDUP_EPS = 1e-4;

	/**
	 * Extract unique vertex positions from edge polyline endpoints.
	 * Each edge range's first and last 3D point are topological vertices.
	 * @param {Array<any>} meshData
	 * @returns {Array<{ position: THREE.Vector3, featureId: string }>}
	 */
	function extractVertices(meshData) {
		const vertices = [];
		const seen = [];

		for (const mesh of meshData) {
			// Hiding a body hides its vertices too (mirrors the face/edge filters).
			if (!isBodyVisible(mesh.bodyId)) continue;
			if (!mesh.edges || !mesh.edges.vertices || !mesh.edges.ranges) continue;
			const verts = mesh.edges.vertices;

			for (const range of mesh.edges.ranges) {
				const si = range.start_index;
				const ei = range.end_index;
				if (ei - si < 2) continue; // Need at least 2 vertices

				// First and last vertex of edge polyline
				const endpoints = [
					new THREE.Vector3(verts[si * 3], verts[si * 3 + 1], verts[si * 3 + 2]),
					new THREE.Vector3(verts[(ei - 1) * 3], verts[(ei - 1) * 3 + 1], verts[(ei - 1) * 3 + 2])
				];

				for (const pos of endpoints) {
					// Deduplicate by position
					const isDup = seen.some(s => s.distanceToSquared(pos) < DEDUP_EPS * DEDUP_EPS);
					if (!isDup) {
						seen.push(pos.clone());
						vertices.push({ position: pos, featureId: mesh.featureId });
					}
				}
			}
		}
		return vertices;
	}

	let vertices = $derived.by(() => {
		const meshData = getMeshes();
		if (!meshData || meshData.length === 0) return [];
		return extractVertices(meshData);
	});

	/**
	 * Create a synthetic GeomRef for a vertex (identified by position).
	 */
	function makeVertexRef(vertex) {
		const p = vertex.position;
		return {
			kind: { type: 'Vertex' },
			anchor: { type: 'FeatureOutput', feature_id: vertex.featureId, output_key: { type: 'Main' } },
			selector: {
				type: 'Position',
				x: Math.round(p.x * 1e6) / 1e6,
				y: Math.round(p.y * 1e6) / 1e6,
				z: Math.round(p.z * 1e6) / 1e6
			}
		};
	}

	// Publish the real rendered vertex count for GUI test introspection.
	$effect(() => setRenderedVertexCount(vertices.length));

	/**
	 * Build Points geometry from extracted vertices with hover/selection colors.
	 */
	let pointsGeometry = $derived.by(() => {
		if (vertices.length === 0) return null;

		const hovRef = getHoveredRef();
		const selRefs = getSelectedRefs();

		const positions = new Float32Array(vertices.length * 3);
		const colors = new Float32Array(vertices.length * 3);

		for (let i = 0; i < vertices.length; i++) {
			const v = vertices[i];
			positions[i * 3] = v.position.x;
			positions[i * 3 + 1] = v.position.y;
			positions[i * 3 + 2] = v.position.z;

			let color = DEFAULT_COLOR;
			const vertRef = makeVertexRef(v);
			if (selRefs.some(r => geomRefEquals(r, vertRef))) {
				color = SELECTED_COLOR;
			} else if (hovRef && geomRefEquals(hovRef, vertRef)) {
				color = HOVER_COLOR;
			}
			colors[i * 3] = color.r;
			colors[i * 3 + 1] = color.g;
			colors[i * 3 + 2] = color.b;
		}

		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.BufferAttribute(positions, 3));
		geo.setAttribute('color', new THREE.BufferAttribute(colors, 3));
		return geo;
	});

	/**
	 * Find the vertex closest to a screen position within threshold.
	 * @param {number} clientX
	 * @param {number} clientY
	 * @returns {{ vertex: any, ref: any, screenDist: number } | null}
	 */
	function pickVertexAtScreen(clientX, clientY) {
		const camera = getCameraObject();
		if (!camera || !renderer || vertices.length === 0) return null;

		const canvas = renderer.domElement;
		const rect = canvas.getBoundingClientRect();
		const screenX = clientX - rect.left;
		const screenY = clientY - rect.top;

		let best = null;
		let bestDist = VERTEX_PICK_THRESHOLD;

		const tempVec = new THREE.Vector3();
		for (const v of vertices) {
			tempVec.copy(v.position).project(camera);

			// Skip vertices behind camera
			if (tempVec.z > 1) continue;

			const px = (tempVec.x * 0.5 + 0.5) * rect.width;
			const py = (-tempVec.y * 0.5 + 0.5) * rect.height;

			const dist = Math.sqrt((px - screenX) ** 2 + (py - screenY) ** 2);
			if (dist < bestDist) {
				bestDist = dist;
				best = { vertex: v, ref: makeVertexRef(v), screenDist: dist };
			}
		}

		return best;
	}

	// Export for EdgeOverlay to check vertex priority
	export { pickVertexAtScreen };

	function handlePointerMove(e) {
		// In sketch mode, only the project tool needs to hover model vertices.
		if (getSketchMode()?.active && !isProjectToolActive()) return;

		const hit = pickVertexAtScreen(e.clientX, e.clientY);
		if (hit) {
			setHoveredRef(hit.ref);
		}
	}

	function handleClick(e) {
		if (getSketchMode()?.active && !isProjectToolActive()) return;

		const hit = pickVertexAtScreen(e.clientX, e.clientY);
		if (hit) {
			selectRef(hit.ref, e.shiftKey);
		}
	}

	onMount(() => {
		const canvas = renderer?.domElement;
		if (!canvas) return;

		canvas.addEventListener('pointermove', handlePointerMove);
		canvas.addEventListener('click', handleClick);

		return () => {
			canvas.removeEventListener('pointermove', handlePointerMove);
			canvas.removeEventListener('click', handleClick);
		};
	});

	// Capped section view: clip vertices on the removed side with the SAME plane
	// CadModel/EdgeOverlay use. Bound to the PointsMaterial via `pointsMaterial`.
	let pointsMaterial = $state(null);
	let sectionClipPlane = $derived.by(() => {
		const s = getSectionState();
		if (!s.active || !s.plane) return null;
		return buildSectionClipPlane(s.plane, s.flipped, s.offset);
	});

	$effect(() => {
		const plane = sectionClipPlane;
		if (!pointsMaterial) return;
		pointsMaterial.clippingPlanes = plane ? [plane] : [];
		pointsMaterial.needsUpdate = true;
	});
</script>

{#if pointsGeometry}
	<T.Points geometry={pointsGeometry} renderOrder={10}>
		<T.PointsMaterial
			bind:ref={pointsMaterial}
			size={4}
			sizeAttenuation={false}
			vertexColors
			depthTest={true}
			transparent
			opacity={0.9}
		/>
	</T.Points>
{/if}
