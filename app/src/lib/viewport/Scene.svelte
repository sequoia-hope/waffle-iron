<script>
	import { interactivity } from '@threlte/extras';
	import { useThrelte } from '@threlte/core';
	import CadModel from './CadModel.svelte';
	import CameraControls from './CameraControls.svelte';
	import Lighting from './Lighting.svelte';
	import EdgeOverlay from './EdgeOverlay.svelte';
	import VertexOverlay from './VertexOverlay.svelte';
	import SketchPlane from './SketchPlane.svelte';
	import DatumVis from './DatumVis.svelte';
	import BoxSelect from './BoxSelect.svelte';
	import SectionCap from './SectionCap.svelte';
	import SketchRenderer from '$lib/sketch/SketchRenderer.svelte';
	import InactiveSketchRenderer from '$lib/sketch/InactiveSketchRenderer.svelte';
	import SketchInteraction from '$lib/sketch/SketchInteraction.svelte';
	import DimensionLabels from '$lib/sketch/DimensionLabels.svelte';
	import GhostPreview from './GhostPreview.svelte';

	// Enable raycaster-based interactivity for all child meshes
	interactivity();

	// Enable local clipping once so per-material clippingPlanes (used by the
	// capped section view) take effect. Threlte v8 `renderer` is a plain
	// THREE.WebGLRenderer (not a store). Guard so it's set once.
	const { renderer } = useThrelte();
	if (renderer && !renderer.localClippingEnabled) {
		renderer.localClippingEnabled = true;
	}
</script>

<Lighting />
<CameraControls />
<CadModel />
<EdgeOverlay />
<VertexOverlay />
<GhostPreview />

<SectionCap />

<SketchPlane />
<DatumVis />
<BoxSelect />
<SketchRenderer />
<InactiveSketchRenderer />
<SketchInteraction />
<DimensionLabels />
