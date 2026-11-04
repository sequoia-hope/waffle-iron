import adapter from '@sveltejs/adapter-static';

const outDir = process.env.WAFFLE_BUILD_OUT_DIR || 'build';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	kit: {
		adapter: adapter({
			pages: outDir,
			assets: outDir,
			fallback: 'index.html'
		}),
		paths: {
			base: process.env.BASE_PATH || ''
		}
	}
};

export default config;
