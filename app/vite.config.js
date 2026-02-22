import { sveltekit } from '@sveltejs/kit/vite';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import testCaseApiPlugin from './src/lib/vite-plugins/testCaseApi.js';

/** @type {import('vite').UserConfig} */
export default {
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
