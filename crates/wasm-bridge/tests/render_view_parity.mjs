// Census of the render data a wasm bundle hands the web worker, for
// render_view_parity.rs (see its header for the regeneration recipe).
//
//   node --stack-size=8000 crates/wasm-bridge/tests/render_view_parity.mjs <pkg dir> <out.json>
//
// Replays fixtures/render_view/scenarios.json through `process_message` and,
// after each message, calls the accessors `worker.js` calls, recording lengths
// and FNV-1a 64 hashes of the exact bytes and strings. The output goes to a
// file because the bundle logs slow messages to stdout.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { webcrypto } from 'node:crypto';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The engine mints UUIDs through `crypto.getRandomValues`, which Node 18 does
// not expose as a global.
globalThis.crypto ??= webcrypto;

const [pkgDir, outPath] = process.argv.slice(2);
if (!pkgDir) {
	console.error('usage: render_view_parity.mjs <pkg dir> [out.json]');
	console.error('  with out.json: write the census; without: check it against golden.json');
	process.exit(2);
}
const here = path.dirname(fileURLToPath(import.meta.url));
const goldenPath = path.join(here, 'fixtures/render_view/golden.json');
const { scenarios } = JSON.parse(
	fs.readFileSync(path.join(here, 'fixtures/render_view/scenarios.json'), 'utf8')
);

// The web-target glue resolves nothing relative to itself when handed the
// bytes; a copy with an .mjs name imports as ESM wherever the pkg dir lives.
const glueDir = fs.mkdtempSync(path.join(os.tmpdir(), 'render-view-parity-'));
const gluePath = path.join(glueDir, 'wasm_bridge.mjs');
fs.copyFileSync(path.join(pkgDir, 'wasm_bridge.js'), gluePath);
const wb = await import(pathToFileURL(gluePath).href);
fs.rmSync(glueDir, { recursive: true });
await wb.default({ module_or_path: fs.readFileSync(path.join(pkgDir, 'wasm_bridge_bg.wasm')) });

const PRIME = 0x100000001b3n;
const MASK = 0xffffffffffffffffn;
function fnv(bytes) {
	let h = 0xcbf29ce484222325n;
	for (const b of bytes) {
		h = ((h ^ BigInt(b)) * PRIME) & MASK;
	}
	return h.toString(16).padStart(16, '0');
}

// Hash views at once: the next wasm call may grow memory and detach them.
const blob = (view) => ({
	n: view.length,
	fnv: fnv(new Uint8Array(view.buffer, view.byteOffset, view.byteLength))
});
const encoder = new TextEncoder();
const text = (s) => {
	const bytes = encoder.encode(s);
	return { n: bytes.length, fnv: fnv(bytes) };
};

function census(response) {
	const bodies = [];
	const count = wb.get_body_count();
	for (let b = 0; b < count; b++) {
		bodies.push({
			vertices: blob(wb.get_body_vertices(b)),
			normals: blob(wb.get_body_normals(b)),
			indices: blob(wb.get_body_indices(b)),
			faces: text(wb.get_body_face_data(b)),
			edge_vertices: blob(wb.get_body_edge_vertices(b)),
			edges: text(wb.get_body_edge_data(b))
		});
	}
	const features = [];
	const featureCount = JSON.parse(wb.get_feature_tree()).features.length;
	for (let f = 0; f < featureCount; f++) {
		features.push({
			mesh_json: text(wb.get_mesh_json(f)),
			vertices: blob(wb.get_mesh_vertices(f)),
			normals: blob(wb.get_mesh_normals(f)),
			indices: blob(wb.get_mesh_indices(f)),
			faces: text(wb.get_face_data(f)),
			edge_vertices: blob(wb.get_edge_vertices(f)),
			edges: text(wb.get_edge_data(f))
		});
	}
	return {
		response_type: JSON.parse(response).type,
		response: text(response),
		metadata: text(wb.get_body_metadata()),
		bodies,
		legacy: {
			mesh_count: wb.get_mesh_count(),
			renderable: Array.from(wb.get_renderable_feature_indices()),
			features
		}
	};
}

const out = [];
for (const { name, messages } of scenarios) {
	wb.init();
	const steps = messages.map((msg) => census(wb.process_message(JSON.stringify(msg))));
	out.push({ name, steps });
	console.error(`${name}: ${steps.length} step(s), bodies ${steps.map((s) => s.bodies.length)}`);
}
const written = JSON.stringify({ scenarios: out }, null, 1) + '\n';
if (outPath) {
	fs.writeFileSync(outPath, written);
} else if (written === fs.readFileSync(goldenPath, 'utf8')) {
	console.error('census matches golden.json');
} else {
	console.error('census DIFFERS from golden.json (rerun with an output path to diff it)');
	process.exit(1);
}
