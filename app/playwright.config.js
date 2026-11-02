import { defineConfig } from '@playwright/test';

// PW_BASE_URL points the suite at a dev server other than the default one
// (e.g. a git worktree's own `npm run dev`); unset, behavior is unchanged.
const baseURL = process.env.PW_BASE_URL || 'http://localhost:5173';

export default defineConfig({
	testDir: './tests',
	timeout: 60000,
	retries: 0,
	workers: parseInt(process.env.PW_WORKERS || '4', 10),
	outputDir: './test-results',
	// PW_REPORTER overrides Playwright's own default (`dot` on CI, `list`
	// locally); the nightly cross-browser workflow asks for `list` so a
	// timed-out run still names every test it reached.
	reporter: process.env.PW_REPORTER || (process.env.CI ? 'dot' : 'list'),
	use: {
		baseURL,
		headless: true,
		viewport: { width: 1280, height: 720 },
		screenshot: 'only-on-failure',
		trace: 'retain-on-failure',
		launchOptions: {
			args: [
				'--use-gl=angle',
				'--use-angle=swiftshader',
				'--enable-webgl',
				'--no-sandbox',
			],
		},
	},
	expect: {
		timeout: 10000,
		toHaveScreenshot: {
			maxDiffPixelRatio: 0.01,
			threshold: 0.2,
		},
	},
	projects: [
		{
			name: 'chromium',
			use: { browserName: 'chromium' },
			testIgnore: '**/mobile/**',
		},
		{
			name: 'mobile-portrait',
			use: {
				browserName: 'chromium',
				viewport: { width: 440, height: 956 },
				isMobile: true,
				hasTouch: true,
				deviceScaleFactor: 3,
			},
			testDir: './tests/gui/mobile',
		},
		{
			name: 'mobile-landscape',
			use: {
				browserName: 'chromium',
				viewport: { width: 956, height: 440 },
				isMobile: true,
				hasTouch: true,
				deviceScaleFactor: 3,
			},
			testDir: './tests/gui/mobile',
		},
		// Cross-browser projects (remediation plan G5, 2026-10-29): Firefox and
		// WebKit had never run a spec. Opt-in via PW_CROSS_BROWSER=1 so a plain
		// local `test.sh gui-fast` does not demand browsers that are not
		// installed; CI's allow-fail `gui-fast-cross-browser` job sets it. The
		// top-level launchOptions are Chromium/ANGLE flags, so both projects
		// clear them.
		//
		// Firefox needs its own route to software WebGL, as Chromium gets one
		// from SwiftShader above: on a GPU-less runner its blocklist refuses
		// the context ("AllowWebgl2:false restricts context creation on this
		// system"), Canvas.svelte throws, the engine never reports ready, and
		// every spec times out in the fixture — 193 of 506 every night from
		// 2026-10-29. `webgl.force-enabled` overrides the blocklist. Past
		// the blocklist, headless Firefox still finds no GL driver without an
		// X display: the nightly workflow runs it under `xvfb-run`, where it
		// gets Mesa's llvmpipe (probe run 41434044751, 2026-11-02).
		...(process.env.PW_CROSS_BROWSER === '1'
			? [
					{
						name: 'firefox',
						use: {
							browserName: 'firefox',
							launchOptions: {
								args: [],
								firefoxUserPrefs: { 'webgl.force-enabled': true },
							},
						},
						testIgnore: '**/mobile/**',
					},
					{
						name: 'webkit',
						use: { browserName: 'webkit', launchOptions: { args: [] } },
						testIgnore: '**/mobile/**',
					},
				]
			: []),
	],
	webServer: {
		command: 'npm run dev',
		url: baseURL,
		reuseExistingServer: true,
		timeout: 30000,
		// Surface the dev server's own output in the run log ([WebServer]
		// lines). Two gui-fast failures (2026-10-27 sketch-edit, 2026-10-28
		// drawing-view-placement) died on "Execution context was destroyed …
		// because of a navigation" mid-test; a Vite dependency re-optimization
		// full-reload is the suspect, and with stdout ignored it is invisible.
		stdout: 'pipe',
	},
});
