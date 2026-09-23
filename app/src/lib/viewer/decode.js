/**
 * Mesh blobs from the host (`crates/waffle-host/src/{viewer,mq}.rs`,
 * specs/waffle_server_mode.md §4.5), in the two encodings a viewer can ask
 * for.
 *
 * - **`raw/1`**: `u32 LE header_len (a multiple of 4) | JSON header,
 *   zero-padded | Float32 positions | Float32 normals | Uint32 indices |
 *   Float32 edge polylines`, all little-endian. The typed arrays are views
 *   over the blob's buffer (every section is 4-byte aligned), so decoding
 *   copies nothing.
 * - **`mq/1`**: the same header plus quantization fields, then
 *   `gzip(positions ‖ normals ‖ indices ‖ edges)` with each stream through
 *   the meshoptimizer codec (the glTF `EXT_meshopt_compression` scheme).
 *   Positions and edges are 16-bit per axis inside the body's box, normals
 *   octahedral in 8 bits; the index buffer is exact, so picking is
 *   unchanged. Measured at 0.21 of `raw/1` over the gravel bike (V6).
 *
 * Both produce the same arrays, which `meshFromEntry` wraps in the per-body
 * object the worker builds for the editor (`$lib/engine/worker.js`
 * `collectBodies`), so the viewport, the overlays and picking draw a
 * streamed body exactly as a local one.
 *
 * `mq/1` needs `DecompressionStream('gzip')` and the meshopt wasm decoder;
 * [`compactSupported`] says whether this browser has them, and the viewer
 * only asks for the encoding when it does.
 */
import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js';

const decoder = new TextDecoder();

/** Whether this browser can decode `mq/1` (gzip stream + the meshopt wasm). */
export function compactSupported() {
	return typeof DecompressionStream === 'function' && MeshoptDecoder.supported !== false;
}

/**
 * The JSON header a blob starts with, and where its payload begins.
 * @param {ArrayBuffer} buffer
 */
function readHeader(buffer) {
	const headerLen = new DataView(buffer).getUint32(0, true);
	const text = decoder.decode(new Uint8Array(buffer, 4, headerLen)).replace(/\0+$/, '');
	return { header: JSON.parse(text), offset: 4 + headerLen };
}

/** @param {ArrayBuffer} buffer */
function decodeRawArrays(buffer) {
	const { header, offset: start } = readHeader(buffer);
	let offset = start;
	const vertexCount = header.vertex_count;
	const vertices = new Float32Array(buffer, offset, vertexCount * 3);
	offset += vertexCount * 12;
	const normals = new Float32Array(buffer, offset, vertexCount * 3);
	offset += vertexCount * 12;
	const indices = new Uint32Array(buffer, offset, header.index_count);
	offset += header.index_count * 4;
	const edgeVertexCount = header.edge_vertex_count;
	const edgeVertices = new Float32Array(buffer, offset, edgeVertexCount * 3);
	return { header, vertices, normals, indices, edgeVertices };
}

/** @param {Uint8Array} bytes */
async function gunzip(bytes) {
	const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'));
	return new Uint8Array(await new Response(stream).arrayBuffer());
}

/**
 * Undo the 16-bit quantization: `min + q / 65535 * extent`, per axis.
 * @param {Uint16Array} q - four components per point (x, y, z, unused)
 * @param {number[]} min @param {number[]} extent @param {number} count
 */
function dequantize(q, min, extent, count) {
	const out = new Float32Array(count * 3);
	for (let i = 0; i < count; i++) {
		for (let k = 0; k < 3; k++) {
			out[i * 3 + k] = min[k] + (q[i * 4 + k] / 65535) * extent[k];
		}
	}
	return out;
}

/** @param {ArrayBuffer} buffer */
async function decodeCompactArrays(buffer) {
	const { header, offset } = readHeader(buffer);
	if (!compactSupported()) throw new Error('this browser cannot decode mq/1');
	await MeshoptDecoder.ready;
	const streams = await gunzip(new Uint8Array(buffer, offset));
	const { positions: lp, normals: ln, indices: li, edges: le } = header.streams;
	let at = 0;
	const take = (n) => {
		const slice = streams.subarray(at, at + n);
		at += n;
		return slice;
	};
	const vertexCount = header.vertex_count;
	const { min, extent } = header.frame;

	const quantized = new Uint8Array(vertexCount * 8);
	MeshoptDecoder.decodeVertexBuffer(quantized, vertexCount, 8, take(lp));
	const vertices = dequantize(new Uint16Array(quantized.buffer), min, extent, vertexCount);

	const oct = new Uint8Array(vertexCount * 4);
	MeshoptDecoder.decodeVertexBuffer(oct, vertexCount, 4, take(ln), 'OCTAHEDRAL');
	const signed = new Int8Array(oct.buffer);
	const normals = new Float32Array(vertexCount * 3);
	for (let i = 0; i < vertexCount; i++) {
		for (let k = 0; k < 3; k++) normals[i * 3 + k] = signed[i * 4 + k] / 127;
	}

	let indices = new Uint32Array(0);
	if (header.index_count > 0) {
		const target = new Uint8Array(header.index_count * 4);
		MeshoptDecoder.decodeIndexBuffer(target, header.index_count, 4, take(li));
		indices = new Uint32Array(target.buffer);
	}

	let edgeVertices = new Float32Array(0);
	if (header.edge_vertex_count > 0) {
		const target = new Uint8Array(header.edge_vertex_count * 8);
		MeshoptDecoder.decodeVertexBuffer(target, header.edge_vertex_count, 8, take(le));
		edgeVertices = dequantize(new Uint16Array(target.buffer), min, extent, header.edge_vertex_count);
	}
	return { header, vertices, normals, indices, edgeVertices };
}

/**
 * A blob's arrays, whichever encoding it is in.
 * @param {ArrayBuffer} buffer
 * @returns {Promise<{header: any, vertices: Float32Array, normals: Float32Array, indices: Uint32Array, edgeVertices: Float32Array}>}
 */
export async function decodeArrays(buffer) {
	const { header } = readHeader(buffer);
	if (header.encoding === 'raw/1') return decodeRawArrays(buffer);
	if (header.encoding === 'mq/1') return decodeCompactArrays(buffer);
	throw new Error(`unknown mesh encoding ${header.encoding}`);
}

/**
 * The body object the editor's viewport draws, from the snapshot's entry and
 * the arrays its blob decoded to. The entry carries everything that can
 * change without the geometry changing (a rename, a placement), so a cached
 * decode is reused across updates.
 * @param {any} entry - the body's entry in the snapshot (`bodies[i]`)
 * @param {{header: any, vertices: Float32Array, normals: Float32Array, indices: Uint32Array, edgeVertices: Float32Array}} arrays
 */
export function meshFromEntry(entry, arrays) {
	const { header, vertices, normals, indices, edgeVertices } = arrays;
	return {
		bodyIndex: entry.body_index ?? 0,
		bodyId: entry.bodyId ?? `${entry.featureId}/Main`,
		name: entry.name ?? null,
		featureIndex: entry.featureIndex,
		featureId: entry.featureId,
		outputKey: entry.outputKey ?? null,
		outputIndex: entry.outputIndex ?? 0,
		instanceId: entry.instanceId ?? null,
		instancePath: entry.instancePath ?? null,
		instanceName: entry.instanceName ?? null,
		partTabId: entry.partTabId ?? null,
		leafPartTabId: entry.leafPartTabId ?? null,
		leafPartSourceId: entry.leafPartSourceId ?? null,
		transform: entry.transform ?? null,
		context: entry.context === true,
		vertices,
		normals,
		indices,
		triangleCount: indices.length / 3,
		faceRanges: header.face_ranges ?? [],
		edges: edgeVertices.length > 0 ? { vertices: edgeVertices, ranges: header.edge_ranges ?? [] } : null
	};
}

/**
 * One blob, decoded and wrapped (the single-body path).
 * @param {any} entry @param {ArrayBuffer} buffer
 */
export async function decodeMesh(entry, buffer) {
	return meshFromEntry(entry, await decodeArrays(buffer));
}

/**
 * A binary viewer frame from the relay: `u32 BE header_len | header JSON | payload`.
 * @param {ArrayBuffer} raw
 * @returns {{ header: any, payload: ArrayBuffer }}
 */
export function decodeBlobFrame(raw) {
	const view = new DataView(raw);
	const headerLen = view.getUint32(0, false);
	const header = JSON.parse(decoder.decode(new Uint8Array(raw, 4, headerLen)));
	return { header, payload: raw.slice(4 + headerLen) };
}
