<script>
	import { interactivity } from '@threlte/extras';
	import CadModel from './CadModel.svelte';
	import CameraControls from './CameraControls.svelte';
	import Lighting from './Lighting.svelte';
	import EdgeOverlay from './EdgeOverlay.svelte';
	import GridFloor from './GridFloor.svelte';
	import SketchPlane from './SketchPlane.svelte';
	import DatumVis from './DatumVis.svelte';

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
