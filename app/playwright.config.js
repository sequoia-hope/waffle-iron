import { defineConfig } from '@playwright/test';

export default defineConfig({
	testDir: './tests',
	timeout: 60000,
	retries: 0,
	outputDir: './test-results',
	use: {
		baseURL: 'http://localhost:5173',
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
	},
	projects: [
		{
			name: 'chromium',
			use: { browserName: 'chromium' },
		},
	],
	webServer: {
		command: 'npm run dev',
		port: 5173,
		reuseExistingServer: true,
		timeout: 30000,
	},
});
