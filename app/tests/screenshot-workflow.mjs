#!/usr/bin/env node
/**
 * Standalone Playwright screenshot script for the sketch workflow.
 * Captures visual proof at each step and generates an HTML gallery.
 *
 * Usage: node app/tests/screenshot-workflow.mjs
 * Then:  python3 -m http.server 8080 --directory app/screenshots/
 */

import { chromium } from 'playwright';
import { mkdirSync, writeFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const outDir = join(__dirname, '..', 'screenshots');
mkdirSync(outDir, { recursive: true });

const BASE_URL = process.env.BASE_URL || 'http://localhost:5173';
const consoleLogs = [];
const screenshots = [];

async function screenshot(page, name, description) {
	const path = join(outDir, `${name}.png`);
	await page.screenshot({ path, fullPage: true });

	let stateInfo = null;
	try {
		stateInfo = await page.evaluate(() => {
			if (!window.__waffle) return { error: '__waffle not available' };
			const s = window.__waffle.getState();
			return {
				sketchActive: s.sketchMode?.active ?? false,
				activeTool: s.activeTool,
				featureCount: s.featureTree?.length ?? 0,
				engineReady: s.engineReady,
			};
		});
	} catch (e) {
		stateInfo = { error: e.message };
	}

	screenshots.push({ name, description, stateInfo });
	console.log(`  [screenshot] ${name}: ${description}`);
	if (stateInfo) console.log(`    state: ${JSON.stringify(stateInfo)}`);
}

function generateHtml() {
	const rows = screenshots
		.map(
			(s) => `
		<div class="step">
			<h2>${s.name}</h2>
			<p>${s.description}</p>
			<pre class="state">${JSON.stringify(s.stateInfo, null, 2)}</pre>
			<img src="${s.name}.png" alt="${s.name}" />
		</div>`
		)
		.join('\n');

	const logLines = consoleLogs
		.map((l) => `<div class="log-${l.type}">[${l.type}] ${escapeHtml(l.text)}</div>`)
		.join('\n');

	return `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Waffle Iron - Sketch Workflow Screenshots</title>
<style>
  body { font-family: system-ui, sans-serif; max-width: 1200px; margin: 0 auto; padding: 20px; background: #1e1e1e; color: #ccc; }
  h1 { color: #4ec9b0; }
  h2 { color: #569cd6; margin-top: 30px; }
  .step { border: 1px solid #333; border-radius: 8px; padding: 16px; margin: 16px 0; background: #252525; }
  .step img { max-width: 100%; border: 1px solid #444; border-radius: 4px; margin-top: 8px; }
  .state { background: #1a1a1a; padding: 8px; border-radius: 4px; font-size: 13px; color: #9cdcfe; overflow-x: auto; }
  .console-log { margin-top: 30px; border: 1px solid #333; border-radius: 8px; padding: 16px; background: #1a1a1a; }
  .console-log h2 { color: #dcdcaa; }
  .log-log { color: #888; }
  .log-warn { color: #ce9178; }
  .log-error { color: #f44747; font-weight: bold; }
  .log-info { color: #569cd6; }
  .summary { background: #2d2d2d; border: 2px solid #4ec9b0; border-radius: 8px; padding: 16px; margin: 16px 0; }
  .summary.fail { border-color: #f44747; }
  .summary.pass { border-color: #4ec9b0; }
</style>
</head>
<body>
<h1>Waffle Iron - Sketch Workflow Visual Proof</h1>
<p>Generated: ${new Date().toISOString()}</p>
${rows}
<div class="console-log">
  <h2>Browser Console Output (${consoleLogs.length} messages)</h2>
  ${logLines || '<div class="log-info">No console messages captured.</div>'}
</div>
</body>
</html>`;
}

function escapeHtml(str) {
	return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

async function run() {
	console.log('Launching browser...');
	const browser = await chromium.launch({
		headless: true,
		args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-webgl', '--no-sandbox'],
	});

	const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
	const page = await context.newPage();

	// Capture all console messages
	page.on('console', (msg) => {
		consoleLogs.push({ type: msg.type(), text: msg.text() });
	});
	page.on('pageerror', (err) => {
		consoleLogs.push({ type: 'error', text: `PAGE ERROR: ${err.message}` });
	});

	try {
		// Step 1: Navigate and wait for engine
		console.log('Step 1: Loading app...');
		await page.goto(BASE_URL, { waitUntil: 'networkidle', timeout: 30000 });

		const waffleReady = await page
			.waitForFunction(() => typeof window.__waffle !== 'undefined', { timeout: 15000 })
			.then(() => true)
			.catch(() => false);

		if (!waffleReady) {
			await screenshot(page, '01-app-loaded-FAIL', 'Engine failed to initialize - __waffle not available');
			console.error('FAIL: __waffle API not available after 15s');
			return;
		}

		await screenshot(page, '01-app-loaded', 'App loaded, engine ready, __waffle API available');

		// Step 2: Click the Sketch toolbar button (via DOM click)
		console.log('Step 2: Clicking Sketch button...');
		const sketchBtn = page.locator('[data-testid="toolbar-btn-sketch"]');
		await sketchBtn.click();

		// Wait for sketch mode to activate
		const sketchActivated = await page
			.waitForFunction(() => window.__waffle?.getState()?.sketchMode?.active === true, {
				timeout: 5000,
			})
			.then(() => true)
			.catch(() => false);

		if (sketchActivated) {
			await screenshot(page, '02-sketch-clicked', 'Sketch mode activated via toolbar button click');
		} else {
			await screenshot(
				page,
				'02-sketch-clicked-FAIL',
				'Toolbar click did NOT activate sketch mode. Trying __waffle.enterSketch() as diagnostic...'
			);

			// Diagnostic: try the API directly
			await page.evaluate(() => window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]));
			const apiWorked = await page
				.waitForFunction(() => window.__waffle?.getState()?.sketchMode?.active === true, {
					timeout: 3000,
				})
				.then(() => true)
				.catch(() => false);

			await screenshot(
				page,
				'02b-api-diagnostic',
				`Direct __waffle.enterSketch() ${apiWorked ? 'WORKED' : 'ALSO FAILED'}`
			);
		}

		// Step 3: Select Circle tool
		console.log('Step 3: Selecting Circle tool...');
		const circleBtn = page.locator('[data-testid="toolbar-btn-circle"]');
		if (await circleBtn.isVisible({ timeout: 2000 }).catch(() => false)) {
			await circleBtn.click();
			await page.waitForTimeout(300);
		} else {
			// Fallback: set via API
			await page.evaluate(() => window.__waffle.setTool('circle'));
			await page.waitForTimeout(300);
		}

		await screenshot(page, '03-circle-tool', 'Circle tool selected');

		// Step 4: Draw a circle (center click + edge click)
		console.log('Step 4: Drawing circle...');
		const canvas = page.locator('canvas');
		const box = await canvas.boundingBox();

		if (box) {
			const centerX = Math.round(box.x + box.width * 0.5);
			const centerY = Math.round(box.y + box.height * 0.5);
			const edgeX = Math.round(box.x + box.width * 0.65);
			const edgeY = Math.round(box.y + box.height * 0.5);

			// Click center
			await page.mouse.click(centerX, centerY);
			await page.waitForTimeout(500);

			// Click edge to complete circle
			await page.mouse.click(edgeX, edgeY);
			await page.waitForTimeout(500);

			const entities = await page.evaluate(() => window.__waffle.getEntities());
			await screenshot(
				page,
				'04-circle-drawn',
				`Circle drawn. Entities: ${entities.length} (${entities.map((e) => e.type).join(', ')})`
			);
		} else {
			await screenshot(page, '04-circle-drawn-FAIL', 'Canvas not visible - could not draw circle');
		}

		// Step 5: Finish sketch
		console.log('Step 5: Finishing sketch...');
		const finishBtn = page.locator('[data-testid="toolbar-btn-finish-sketch"]');
		if (await finishBtn.isVisible({ timeout: 2000 }).catch(() => false)) {
			await finishBtn.click();
			await page.waitForTimeout(1000);
		} else {
			await page.evaluate(() => window.__waffle.finishSketch());
			await page.waitForTimeout(1000);
		}

		await screenshot(page, '05-sketch-finished', 'Sketch finished');

		// Step 6: Final state - feature tree
		console.log('Step 6: Capturing final state...');
		const finalState = await page.evaluate(() => {
			const s = window.__waffle.getState();
			return {
				sketchActive: s.sketchMode?.active ?? false,
				activeTool: s.activeTool,
				featureTree: s.featureTree,
			};
		});

		await screenshot(
			page,
			'06-feature-tree',
			`Final state: ${finalState.featureTree?.length ?? 0} features in tree. Sketch active: ${finalState.sketchActive}`
		);

		// Summary
		const errors = consoleLogs.filter((l) => l.type === 'error');
		console.log(`\nDone! ${screenshots.length} screenshots captured.`);
		console.log(`Console: ${consoleLogs.length} messages (${errors.length} errors)`);
		if (errors.length > 0) {
			console.log('Errors:');
			errors.forEach((e) => console.log(`  ${e.text}`));
		}
	} catch (err) {
		console.error('Script error:', err.message);
		await screenshot(page, 'XX-error', `Script error: ${err.message}`);
	} finally {
		// Generate HTML gallery
		const html = generateHtml();
		writeFileSync(join(outDir, 'index.html'), html);
		console.log(`\nHTML gallery: ${join(outDir, 'index.html')}`);
		console.log(`Serve with: python3 -m http.server 8080 --directory ${outDir}`);

		await browser.close();
	}
}

run().catch((err) => {
	console.error('Fatal error:', err);
	process.exit(1);
});
