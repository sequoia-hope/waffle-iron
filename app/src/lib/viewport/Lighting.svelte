<script>
	import { T } from '@threlte/core';
</script>

<!-- Ambient fill -->
<T.AmbientLight intensity={0.4} />

<!-- Main key light -->
<T.DirectionalLight position={[10, 15, 10]} intensity={0.8} castShadow={false} />

<!-- Secondary fill light (opposite side) -->
<T.DirectionalLight position={[-8, 10, -5]} intensity={0.3} />

<!-- Hemisphere for subtle sky/ground gradient -->
<T.HemisphereLight args={[0x8888cc, 0x444422, 0.3]} />
