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
	],
	webServer: {
		command: 'npm run dev',
		url: baseURL,
		reuseExistingServer: true,
		timeout: 30000,
	},
});
