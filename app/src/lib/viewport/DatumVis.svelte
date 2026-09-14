<script>
	import { T } from '@threlte/core';
	import * as THREE from 'three';
	import {
		selectRef,
		setHoveredRef,
		getHoveredRef,
		getSelectedRefs,
		geomRefEquals,
		getFeatureTree,
		getSketchMode,
		isPlaneVisible,
		isAxisVisible,
		computeFacePlane
	} from '$lib/engine/store.svelte.js';
	import { getAllPlanes, makePlaneRef, resolvePlane, PLANE_HALF_SIZE } from '$lib/engine/planes.js';
	import { AXIS_COLORS } from '$lib/config.js';
	import { getTheme } from '$lib/ui/theme.svelte.js';
	import { getColorVersion } from '$lib/ui/settings.svelte.js';

	let inSketchMode = $derived(!!getSketchMode()?.active);

	// Plane visibility against the viewport ground: the fill is a faint veil
	// that never hides the model, the border carries the contrast.
	const FILL_OPACITY = 0.07;
	const HOVER_OPACITY = 0.18;
	const SELECTED_OPACITY = 0.32;
	const BORDER_OPACITY = 0.6;

	/**
	 * Resolve a CSS custom property on <html> to a hex color, falling back to
	 * `fallbackHex` when the var is unset or we're off-DOM (SSR). Mirrors the
	 * helper in EdgeOverlay.svelte.
	 * @param {string | undefined} name
	 * @param {number} fallbackHex
	 */
	function cssHex(name, fallbackHex) {
		if (name && typeof document !== 'undefined') {
			const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
			if (v) return new THREE.Color(v).getHex();
		}
		return fallbackHex;
	}

	// --- Data-driven plane rendering ---

	const planeGeometry = new THREE.PlaneGeometry(PLANE_HALF_SIZE * 2, PLANE_HALF_SIZE * 2);

	// Compute rotation from plane normal using quaternion
	function computeRotation(normal) {
		const quat = new THREE.Quaternion().setFromUnitVectors(
			new THREE.Vector3(0, 0, 1),
			new THREE.Vector3(normal[0], normal[1], normal[2])
		);
		const euler = new THREE.Euler().setFromQuaternion(quat);
		return [euler.x, euler.y, euler.z];
	}

	let features = $derived(getFeatureTree()?.features ?? []);

	// Features up to the rollback point (active_index). User datum planes created
	// past the rollback are hidden along with the rest of the rolled-back timeline;
	// built-in planes are always added by getAllPlanes regardless of this slice.
	let activeFeatures = $derived.by(() => {
		const ai = getFeatureTree()?.active_index;
		if (ai === null || ai === undefined) return features;
		return features.slice(0, ai + 1);
	});

	// Reactive plane data: built-in + user planes. Reading getTheme() and
	// getColorVersion() rebuilds the materials on a theme switch or a per-token
	// override (built-in plane colors are theme tokens, see app.css).
	let planeData = $derived.by(() => {
		void getTheme(); void getColorVersion();
		return getAllPlanes(activeFeatures).map((plane) => {
			let resolved;
			try {
				resolved = resolvePlane(plane.definition, features, computeFacePlane);
			} catch {
				resolved = { origin: [0, 0, 0], normal: [0, 0, 1] };
			}
			// A themed plane shows one hue in every state; the opacity says hover/selected.
			const base = cssHex(plane.colorToken, plane.color);
			const colors = plane.colorToken
				? { base, hover: base, selected: base, border: base }
				: { base, hover: plane.hoverColor, selected: plane.selectedColor, border: plane.borderColor };
			return {
				plane,
				colors,
				ref: makePlaneRef(plane.id),
				position: resolved.origin,
				rotation: computeRotation(resolved.normal),
				fillMaterial: new THREE.MeshBasicMaterial({
					color: colors.base,
					transparent: true,
					opacity: FILL_OPACITY,
					side: THREE.DoubleSide,
					depthWrite: false
				}),
				borderMaterial: new THREE.LineBasicMaterial({
					color: colors.border,
					transparent: true,
					opacity: BORDER_OPACITY
				}),
			};
		});
	});

	/**
	 * Get opacity and color for a datum plane based on hover/selection state.
	 */
	function getPlaneStyle(ref, colors) {
		const selected = getSelectedRefs().some((r) => geomRefEquals(r, ref));
		const hovered = geomRefEquals(getHoveredRef(), ref);

		if (selected) return { opacity: SELECTED_OPACITY, color: colors.selected };
		if (hovered) return { opacity: HOVER_OPACITY, color: colors.hover };
		return { opacity: FILL_OPACITY, color: colors.base };
	}

	// Reactive style derivations
	let styles = $derived(planeData.map((d) => getPlaneStyle(d.ref, d.colors)));

	// Update materials reactively
	$effect(() => {
		for (let i = 0; i < planeData.length; i++) {
			planeData[i].fillMaterial.opacity = styles[i].opacity;
			planeData[i].fillMaterial.color.setHex(styles[i].color);
		}
	});

	// Event handlers
	function handleClick(ref, event) {
		event.stopPropagation();
		const additive = event.nativeEvent?.shiftKey ?? false;
		selectRef(ref, additive);
	}

	function handlePointerEnter(ref, event) {
		if (event) event.stopPropagation();
		setHoveredRef(ref);
	}

	function handlePointerLeave(ref) {
		if (geomRefEquals(getHoveredRef(), ref)) {
			setHoveredRef(null);
		}
	}

	// Border geometry
	function buildPlaneBorder(size) {
		const s = size;
		const pts = new Float32Array([
			-s, -s, 0, s, -s, 0,
			s, -s, 0, s, s, 0,
			s, s, 0, -s, s, 0,
			-s, s, 0, -s, -s, 0
		]);
		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.BufferAttribute(pts, 3));
		return geo;
	}

	const borderGeo = buildPlaneBorder(PLANE_HALF_SIZE);

	// --- Origin Triad (scaled to match plane size) ---

	const axisLength = PLANE_HALF_SIZE * 0.6;

	function buildAxisLine(dir, length) {
		const pts = new Float32Array([0, 0, 0, dir[0] * length, dir[1] * length, dir[2] * length]);
		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.BufferAttribute(pts, 3));
		return geo;
	}

	const xAxisGeo = buildAxisLine([1, 0, 0], axisLength);
	const yAxisGeo = buildAxisLine([0, 1, 0], axisLength);
	const zAxisGeo = buildAxisLine([0, 0, 1], axisLength);

	const xAxisMaterial = new THREE.LineBasicMaterial({ color: AXIS_COLORS.x });
	const yAxisMaterial = new THREE.LineBasicMaterial({ color: AXIS_COLORS.y });
	const zAxisMaterial = new THREE.LineBasicMaterial({ color: AXIS_COLORS.z });

	// Arrowhead cones (scaled proportionally)
	const coneGeo = new THREE.ConeGeometry(axisLength * 0.025, axisLength * 0.09, 8);

	const xConeMaterial = new THREE.MeshBasicMaterial({ color: AXIS_COLORS.x });
	const yConeMaterial = new THREE.MeshBasicMaterial({ color: AXIS_COLORS.y });
	const zConeMaterial = new THREE.MeshBasicMaterial({ color: AXIS_COLORS.z });

	// Cone rotations to point along each axis
	const xConeRotation = [0, 0, -Math.PI / 2];
	const yConeRotation = [0, 0, 0];
	const zConeRotation = [Math.PI / 2, 0, 0];

</script>

<!-- Datum Planes (per-plane visibility) -->
<!-- During sketch mode, disable raycaster interactivity so clicks pass through to canvas -->
{#each planeData as pd, i (pd.plane.id)}
{#if isPlaneVisible(pd.plane.id)}
	<T.Group position={pd.position} rotation={pd.rotation}>
		{#if inSketchMode}
			<T.Mesh
				geometry={planeGeometry}
				material={pd.fillMaterial}
				raycast={() => {}}
			/>
		{:else}
			<T.Mesh
				geometry={planeGeometry}
				material={pd.fillMaterial}
				onclick={(e) => handleClick(pd.ref, e)}
				onpointerenter={(e) => handlePointerEnter(pd.ref, e)}
				onpointerleave={() => handlePointerLeave(pd.ref)}
			/>
		{/if}
		<T.LineSegments geometry={borderGeo} material={pd.borderMaterial} />
	</T.Group>
{/if}
{/each}

<!-- Origin Triad (per-axis visibility) -->
{#if isAxisVisible('x')}
<T.Group>
	<T.LineSegments geometry={xAxisGeo} material={xAxisMaterial} />
	<T.Mesh
		geometry={coneGeo}
		material={xConeMaterial}
		position={[axisLength, 0, 0]}
		rotation={xConeRotation}
	/>
</T.Group>
{/if}
{#if isAxisVisible('y')}
<T.Group>
	<T.LineSegments geometry={yAxisGeo} material={yAxisMaterial} />
	<T.Mesh
		geometry={coneGeo}
		material={yConeMaterial}
		position={[0, axisLength, 0]}
		rotation={yConeRotation}
	/>
</T.Group>
{/if}
{#if isAxisVisible('z')}
<T.Group>
	<T.LineSegments geometry={zAxisGeo} material={zAxisMaterial} />
	<T.Mesh
		geometry={coneGeo}
		material={zConeMaterial}
		position={[0, 0, axisLength]}
		rotation={zConeRotation}
	/>
</T.Group>
{/if}
