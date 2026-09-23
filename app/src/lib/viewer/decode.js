/**
 * `raw/1` mesh blobs (`crates/waffle-host/src/viewer.rs`,
 * specs/waffle_server_mode.md §4.5): `u32 LE header_len (a multiple of 4) |
 * JSON header, zero-padded | Float32 positions | Float32 normals | Uint32
 * indices | Float32 edge polylines`, all little-endian. The header carries the
 * counts and the picking metadata (`face_ranges`, `edge_ranges`) exactly as
 * the engine's face and edge entries.
 *
 * The result is the per-body object the worker builds for the editor
 * (`app/src/lib/engine/worker.js` `collectBodies`), so the viewport, the edge
 * and vertex overlays and picking render a streamed body exactly as a local
 * one. The typed arrays are views over the blob's buffer (every section is
 * 4-byte aligned), so decoding copies nothing.
 */

const decoder = new TextDecoder();

/**
 * @param {any} entry - the body's entry in the snapshot (`bodies[i]`)
 * @param {ArrayBuffer} buffer - the blob
 */
export function decodeMesh(entry, buffer) {
	const view = new DataView(buffer);
	const headerLen = view.getUint32(0, true);
	const header = JSON.parse(decoder.decode(new Uint8Array(buffer, 4, headerLen)).replace(/\0+$/, ''));
	if (header.encoding !== 'raw/1') throw new Error(`unknown mesh encoding ${header.encoding}`);
	let offset = 4 + headerLen;
	const vertexCount = header.vertex_count;
	const vertices = new Float32Array(buffer, offset, vertexCount * 3);
	offset += vertexCount * 12;
	const normals = new Float32Array(buffer, offset, vertexCount * 3);
	offset += vertexCount * 12;
	const indices = new Uint32Array(buffer, offset, header.index_count);
	offset += header.index_count * 4;
	const edgeVertexCount = header.edge_vertex_count;
	const edgeVertices = new Float32Array(buffer, offset, edgeVertexCount * 3);
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
		edges: edgeVertexCount > 0 ? { vertices: edgeVertices, ranges: header.edge_ranges ?? [] } : null
	};
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
