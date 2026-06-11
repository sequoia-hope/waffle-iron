import { execSync } from 'node:child_process';
import { sveltekit } from '@sveltejs/kit/vite';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import testCaseApiPlugin from './src/lib/vite-plugins/testCaseApi.js';

// Build provenance baked into the bundle at build time (shown in the GUI's
// Debug menu). Date is UTC; commit falls back to 'unknown' outside a git tree.
function buildInfo() {
	let commit = 'unknown';
	try {
		commit = execSync('git rev-parse --short HEAD').toString().trim();
	} catch {
		/* not a git checkout (e.g. tarball build) */
	}
	return { date: new Date().toISOString().slice(0, 10), commit };
}

/** @type {import('vite').UserConfig} */
export default {
	define: {
		__BUILD_INFO__: JSON.stringify(buildInfo())
	},
	plugins: [testCaseApiPlugin(), wasm(), topLevelAwait(), sveltekit()],
	server: {
		port: 5173,
		host: '0.0.0.0',
		allowedHosts: true,
		fs: {
			allow: ['..']
		}
	}
};
