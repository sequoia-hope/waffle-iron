<script>
	import { interactivity } from '@threlte/extras';
	import CadModel from './CadModel.svelte';
	import CameraControls from './CameraControls.svelte';
	import Lighting from './Lighting.svelte';
	import EdgeOverlay from './EdgeOverlay.svelte';
	import GridFloor from './GridFloor.svelte';
	import SketchPlane from './SketchPlane.svelte';
	import DatumVis from './DatumVis.svelte';
	import BoxSelect from './BoxSelect.svelte';
	import SketchRenderer from '$lib/sketch/SketchRenderer.svelte';
	import SketchInteraction from '$lib/sketch/SketchInteraction.svelte';
	import DimensionLabels from '$lib/sketch/DimensionLabels.svelte';

	// Enable raycaster-based interactivity for all child meshes
	interactivity();
</script>

<Lighting />
<CameraControls />
<CadModel />
<EdgeOverlay />
<GridFloor />
<SketchPlane />
<DatumVis />
<BoxSelect />
<SketchRenderer />
<SketchInteraction />
<DimensionLabels />
