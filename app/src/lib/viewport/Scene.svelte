<script>
	import { interactivity } from '@threlte/extras';
	import { useThrelte } from '@threlte/core';
	import { setSceneRefs } from '$lib/engine/store.svelte.js';
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
	import ConnectorFrames from './ConnectorFrames.svelte';
	import AgentCapture from './AgentCapture.svelte';

	// Enable raycaster-based interactivity for all child meshes
	interactivity();

	// Enable local clipping once so per-material clippingPlanes (used by the
	// capped section view) take effect. Threlte v8 `renderer` is a plain
	// THREE.WebGLRenderer (not a store). Guard so it's set once.
	const { renderer, scene } = useThrelte();
	if (renderer && !renderer.localClippingEnabled) {
		renderer.localClippingEnabled = true;
	}
	// Measurement only (`window.__waffle.getRenderStats()`): what a frame costs.
	setSceneRefs(scene, renderer);
</script>

<Lighting />
<CameraControls />
<CadModel />
<EdgeOverlay />
<VertexOverlay />
<GhostPreview />
<ConnectorFrames />

<SectionCap />

<SketchPlane />
<DatumVis />
<BoxSelect />
<SketchRenderer />
<InactiveSketchRenderer />
<SketchInteraction />
<DimensionLabels />
<AgentCapture />
