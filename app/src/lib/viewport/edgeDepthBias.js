/**
 * The edge overlay's screen-constant depth bias, shared by `EdgeOverlay` (which
 * draws the edges) and `capture.js` (which renders a frame at a size the live
 * canvas does not have).
 *
 * Edges lie exactly ON the faces they bound, so an unbiased depth test is a
 * coin flip per pixel: the line and the triangle interpolate depth differently,
 * and the edge renders as a dashed line that alternates with the solid.
 * `polygonOffset` cannot fix it — WebGL applies polygon offset to filled
 * triangles only, never to GL_LINES. Instead each edge vertex is pulled toward
 * the camera ALONG ITS VIEW RAY (so its screen position is unchanged) by a
 * screen-constant `EDGE_DEPTH_BIAS_PX`. Edges on visible faces then always win;
 * edges genuinely behind the part are hidden by far more than a couple of pixels
 * and stay hidden.
 *
 * The bias is measured in PIXELS, so the shader needs the height of the frame
 * being drawn. `EdgeOverlay` refreshes it from the canvas every frame; an
 * offscreen capture sets it to the capture height for the duration of its pass
 * and restores it, or every edge in a capture smaller than the canvas is biased
 * too little and the dashing comes back.
 */
import * as THREE from 'three';

export const EDGE_DEPTH_BIAS_PX = 2;

/**
 * Height in CSS pixels of the frame the edge shaders are drawing, shared by
 * every edge material's program. One uniform object, mutated in place.
 */
export const edgeViewportHeight = { value: 1 };

/**
 * Patch a line material's vertex shader with the view-ray depth pull.
 * @template {THREE.Material} M
 * @param {M} mat
 * @returns {M}
 */
export function withEdgeDepthBias(mat) {
	mat.onBeforeCompile = (shader) => {
		shader.uniforms.edgeViewportHeight = edgeViewportHeight;
		shader.vertexShader = shader.vertexShader
			.replace('void main() {', 'uniform float edgeViewportHeight;\nvoid main() {')
			.replace(
				'#include <project_vertex>',
				`#include <project_vertex>
				{
					// World units per pixel at this vertex's depth.
					float edgeWpp = 2.0 / ( projectionMatrix[ 1 ][ 1 ] * edgeViewportHeight );
					vec4 edgeMv = mvPosition;
					// Orthographic iff the projection has no perspective divide
					// (the built-in isOrthographic uniform is not uploaded for
					// LineBasicMaterial, so it cannot be trusted here).
					if ( projectionMatrix[ 3 ][ 3 ] == 1.0 ) {
						edgeMv.z += ${EDGE_DEPTH_BIAS_PX.toFixed(1)} * edgeWpp;
					} else {
						float edgeDist = -mvPosition.z;
						float edgePull = min( ${EDGE_DEPTH_BIAS_PX.toFixed(1)} * edgeWpp * edgeDist, 0.5 * edgeDist );
						edgeMv.xyz += normalize( -mvPosition.xyz ) * edgePull;
					}
					// mvPosition itself is left untouched so section clipping
					// (vClipPosition) still cuts at the true edge position.
					gl_Position = projectionMatrix * edgeMv;
				}`
			);
	};
	mat.customProgramCacheKey = () => 'waffle-edge-depth-bias';
	return mat;
}
