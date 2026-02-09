<script>
	import { T, useThrelte } from '@threlte/core';
	import { OrbitControls } from '@threlte/extras';
</script>

<T.PerspectiveCamera makeDefault position={[5, 5, 5]} fov={50} near={0.1} far={1000}>
	<OrbitControls
		enableDamping
		dampingFactor={0.15}
		minDistance={1}
		maxDistance={100}
	/>
</T.PerspectiveCamera>
