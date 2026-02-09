<script>
	import { useThrelte } from '@threlte/core';
	import { onMount } from 'svelte';
	import * as THREE from 'three';

	const { camera, renderer } = useThrelte();

	// Separate scene/camera for the orientation gizmo
	const gizmoScene = new THREE.Scene();
	const gizmoCamera = new THREE.OrthographicCamera(-1.8, 1.8, 1.8, -1.8, 0.1, 100);
	gizmoCamera.position.set(0, 0, 5);

	const axisLength = 0.9;
	const labelOffset = 1.15;
	const gizmoGroup = new THREE.Group();

	function addAxis(dir, color) {
		const pts = new Float32Array([0, 0, 0, dir[0] * axisLength, dir[1] * axisLength, dir[2] * axisLength]);
		const geo = new THREE.BufferGeometry();
		geo.setAttribute('position', new THREE.BufferAttribute(pts, 3));
		const mat = new THREE.LineBasicMaterial({ color, depthTest: false });
		return new THREE.LineSegments(geo, mat);
	}

	function addCone(dir, color) {
		const geo = new THREE.ConeGeometry(0.06, 0.2, 8);
		const mat = new THREE.MeshBasicMaterial({ color, depthTest: false });
		const mesh = new THREE.Mesh(geo, mat);
		mesh.position.set(dir[0] * axisLength, dir[1] * axisLength, dir[2] * axisLength);
		const q = new THREE.Quaternion();
		q.setFromUnitVectors(new THREE.Vector3(0, 1, 0), new THREE.Vector3(...dir));
		mesh.quaternion.copy(q);
		return mesh;
	}

	function addLabel(text, position, color) {
		const canvas = document.createElement('canvas');
		canvas.width = 64;
		canvas.height = 64;
		const ctx = canvas.getContext('2d');
		if (!ctx) return null;
		ctx.fillStyle = color;
		ctx.font = 'bold 48px sans-serif';
		ctx.textAlign = 'center';
		ctx.textBaseline = 'middle';
		ctx.fillText(text, 32, 32);

		const texture = new THREE.CanvasTexture(canvas);
		const mat = new THREE.SpriteMaterial({ map: texture, depthTest: false });
		const sprite = new THREE.Sprite(mat);
		sprite.position.set(...position);
		sprite.scale.set(0.35, 0.35, 0.35);
		return sprite;
	}

	// Build gizmo
	gizmoGroup.add(addAxis([1, 0, 0], 0xff4444));
	gizmoGroup.add(addCone([1, 0, 0], 0xff4444));
	const xl = addLabel('X', [labelOffset, 0, 0], '#ff4444');
	if (xl) gizmoGroup.add(xl);

	gizmoGroup.add(addAxis([0, 1, 0], 0x44cc44));
	gizmoGroup.add(addCone([0, 1, 0], 0x44cc44));
	const yl = addLabel('Y', [0, labelOffset, 0], '#44cc44');
	if (yl) gizmoGroup.add(yl);

	gizmoGroup.add(addAxis([0, 0, 1], 0x4488ff));
	gizmoGroup.add(addCone([0, 0, 1], 0x4488ff));
	const zl = addLabel('Z', [0, 0, labelOffset], '#4488ff');
	if (zl) gizmoGroup.add(zl);

	gizmoScene.add(gizmoGroup);

	const gizmoSize = 120;

	onMount(() => {
		let animId;

		function renderGizmo() {
			if (!renderer || !camera.current) {
				animId = requestAnimationFrame(renderGizmo);
				return;
			}

			const gl = /** @type {THREE.WebGLRenderer} */ (renderer);

			// Sync gizmo rotation with main camera
			const q = camera.current.quaternion.clone().invert();
			gizmoGroup.quaternion.copy(q);

			// Render in bottom-right corner
			const dim = gl.getSize(new THREE.Vector2());
			gl.autoClear = false;
			gl.setViewport(dim.x - gizmoSize, 0, gizmoSize, gizmoSize);
			gl.setScissor(dim.x - gizmoSize, 0, gizmoSize, gizmoSize);
			gl.setScissorTest(true);
			gl.clearDepth();
			gl.render(gizmoScene, gizmoCamera);
			gl.setScissorTest(false);
			gl.setViewport(0, 0, dim.x, dim.y);
			gl.autoClear = true;

			animId = requestAnimationFrame(renderGizmo);
		}

		animId = requestAnimationFrame(renderGizmo);
		return () => cancelAnimationFrame(animId);
	});
</script>
