#!/usr/bin/env node
/**
 * Emit the agent-link tool manifest consumed by the relay
 * (specs/waffle_mcp_server.md §2.4) from app/src/lib/agent/tools/.
 *
 *   node app/scripts/gen-agent-manifest.mjs            # write the relay's bundled copy
 *   node app/scripts/gen-agent-manifest.mjs --stdout   # print it (the relay test diffs this)
 *
 * Output is deterministic: keys sorted recursively, 2-space indent, trailing newline.
 */
import { createHash } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { TOOLS } from '../src/lib/agent/tools/index.js';
import { buildManifest, sortKeysDeep } from '../src/lib/agent/tools/manifest.js';

const here = dirname(fileURLToPath(import.meta.url));
const target = resolve(here, '../../relay/src/waffle_mcp_relay/agent-tools.manifest.json');

const manifest = await buildManifest(TOOLS, (text) =>
	createHash('sha256').update(text, 'utf8').digest('hex')
);
const text = `${JSON.stringify(sortKeysDeep(manifest), null, 2)}\n`;

if (process.argv.includes('--stdout')) {
	process.stdout.write(text);
} else {
	writeFileSync(target, text);
	console.error(`wrote ${target} (manifest_hash ${manifest.manifest_hash})`);
}
