import { sveltekit } from '@sveltejs/kit/vite';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';

/** @type {import('vite').UserConfig} */
export default {
	plugins: [wasm(), topLevelAwait(), sveltekit()],
	server: {
		fs: {
			allow: ['..']
		}
	},
	preview: {
		allowedHosts: ['loaf.cama-minor.ts.net']
	}
};
