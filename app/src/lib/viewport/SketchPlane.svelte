<script>
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import { getSketchMode } from '$lib/engine/store.svelte.js';

	// Derive sketch plane properties from store
	let sketchMode = $derived(getSketchMode());

	// Sketch plane grid material (opaque, visible when in sketch mode)
	const planeMaterial = new THREE.MeshBasicMaterial({
		color: 0x334455,
		transparent: true,
		opacity: 0.15,
		side: THREE.DoubleSide,
		depthWrite: false
	});

	const borderMaterial = new THREE.LineBasicMaterial({
		color: 0x5588aa,
		linewidth: 1
	});

	// Grid lines on the sketch plane
	const gridMaterial = new THREE.LineBasicMaterial({
		color: 0x445566,
		transparent: true,
		opacity: 0.3
	});

	/**
	 * Build a grid of line segments on the sketch plane.
	 * @param {number} size - half-extent of the grid
	 * @param {number} divisions - number of divisions per side
	 */
	function buildGridGeometry(size, divisions) {
		const points = [];
		const step = (size * 2) / divisions;
		for (let i = 0; i <= divisions; i++) {
			const pos = -size + i * step;
			// Lines along X
			points.push(-size, pos, 0, size, pos, 0);
			// Lines along Y
			points.push(pos, -size, 0, pos, size, 0);
		}
		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.Float32BufferAttribute(points, 3));
		return geo;
	}

	const gridGeometry = buildGridGeometry(10, 20);

	// Square plane geometry for the sketch plane background
	const planeGeometry = new THREE.PlaneGeometry(20, 20);

	// Border around the sketch plane
	function buildBorderGeometry(size) {
		const s = size;
		const points = new Float32Array([
			-s, -s, 0, s, -s, 0,
			s, -s, 0, s, s, 0,
			s, s, 0, -s, s, 0,
			-s, s, 0, -s, -s, 0
		]);
		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.BufferAttribute(points, 3));
		return geo;
	}

	const borderGeometry = buildBorderGeometry(10);

	/**
	 * Build quaternion to orient the sketch plane from a normal vector.
	 * @param {[number, number, number]} normal
	 * @returns {THREE.Quaternion}
	 */
	function normalToQuaternion(normal) {
		const q = new THREE.Quaternion();
		const from = new THREE.Vector3(0, 0, 1);
		const to = new THREE.Vector3(normal[0], normal[1], normal[2]).normalize();
		q.setFromUnitVectors(from, to);
		return q;
	}

	let planePosition = $derived(sketchMode?.origin || [0, 0, 0]);
	let planeQuaternion = $derived(normalToQuaternion(sketchMode?.normal || [0, 0, 1]));
</script>

{#if sketchMode?.active}
	<T.Group position={planePosition} quaternion={planeQuaternion}>
		<!-- Semi-transparent plane background -->
		<T.Mesh geometry={planeGeometry} material={planeMaterial} />

		<!-- Grid lines -->
		<T.LineSegments geometry={gridGeometry} material={gridMaterial} />

		<!-- Border -->
		<T.LineSegments geometry={borderGeometry} material={borderMaterial} />
	</T.Group>
{/if}
