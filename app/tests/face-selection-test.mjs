#!/usr/bin/env node
/**
 * Face selection end-to-end test.
 *
 * Creates a sketch + extrude, then clicks on the 3D model faces
 * and verifies that face selection works (hoveredRef/selectedRefs populate).
 *
 * Usage: node app/tests/face-selection-test.mjs
 */

import { chromium } from 'playwright';
import { mkdirSync, writeFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const outDir = join(__dirname, '..', 'screenshots', 'face-selection');
mkdirSync(outDir, { recursive: true });

const BASE_URL = process.env.BASE_URL || 'http://localhost:5174';
const consoleLogs = [];
const results = [];

function log(msg) {
	console.log(`  [test] ${msg}`);
}

function logResult(name, pass, detail) {
	const status = pass ? 'PASS' : 'FAIL';
	console.log(`  [${status}] ${name}: ${detail}`);
	results.push({ name, pass, detail });
}

async function screenshot(page, name) {
	const path = join(outDir, `${name}.png`);
	await page.screenshot({ path, fullPage: true });
}

async function waitForEngine(page, timeout = 15000) {
	await page.waitForFunction(
		() => window.__waffle && window.__waffle.getState().engineReady,
		{ timeout }
	);
}

async function sleep(ms) {
	return new Promise((r) => setTimeout(r, ms));
}

(async () => {
	console.log('\n=== Face Selection End-to-End Test ===\n');
	console.log(`URL: ${BASE_URL}`);

	const browser = await chromium.launch({ headless: true });
	const context = await browser.newContext({
		viewport: { width: 1280, height: 720 }
	});
	const page = await context.newPage();

	page.on('console', (msg) => {
		const text = msg.text();
		consoleLogs.push({ type: msg.type(), text });
		if (text.includes('error') || text.includes('Error') || text.includes('FAIL')) {
			console.log(`  [console.${msg.type()}] ${text}`);
		}
	});

	try {
		// ── Step 1: Load app ──
		log('Loading app...');
		await page.goto(BASE_URL, { waitUntil: 'networkidle' });
		await waitForEngine(page);
		log('Engine ready');
		await screenshot(page, '01-app-loaded');

		// ── Step 2: Enter sketch mode on Top (XY) plane ──
		log('Entering sketch mode...');
		const sketchBtn = page.locator('button:has-text("Sketch")');
		await sketchBtn.click();
		await sleep(500);

		// Select top plane via test API (inline plane selection mode)
		await page.evaluate(() => {
			window.__waffle?.selectRef({
				kind: { type: 'Face' },
				anchor: { type: 'DatumPlane', id: '00000000-0000-0000-0000-000000000002' }
			});
		});
		await sleep(500);

		await page.waitForFunction(
			() => window.__waffle?.getState().sketchMode?.active === true,
			{ timeout: 5000 }
		);
		log('Sketch mode active');

		// ── Step 3: Draw rectangle via API ──
		log('Drawing rectangle...');
		await page.evaluate(() => {
			const w = window.__waffle;
			w.addSketchEntity({ type: 'Point', id: 1, x: -20, y: -20, construction: false });
			w.addSketchEntity({ type: 'Point', id: 2, x: 20, y: -20, construction: false });
			w.addSketchEntity({ type: 'Point', id: 3, x: 20, y: 20, construction: false });
			w.addSketchEntity({ type: 'Point', id: 4, x: -20, y: 20, construction: false });
			w.addSketchEntity({ type: 'Line', id: 10, start_id: 1, end_id: 2, construction: false });
			w.addSketchEntity({ type: 'Line', id: 11, start_id: 2, end_id: 3, construction: false });
			w.addSketchEntity({ type: 'Line', id: 12, start_id: 3, end_id: 4, construction: false });
			w.addSketchEntity({ type: 'Line', id: 13, start_id: 4, end_id: 1, construction: false });
		});
		await sleep(500);

		// ── Step 4: Finish sketch ──
		log('Finishing sketch...');
		const finishBtn = page.locator('button:has-text("Finish")');
		if (await finishBtn.isVisible({ timeout: 2000 })) {
			await finishBtn.click();
		} else {
			await page.keyboard.press('Escape');
		}
		await sleep(1000);

		// Wait for sketch feature
		await page.waitForFunction(
			() => (window.__waffle?.getFeatureTree()?.features || []).length >= 1,
			{ timeout: 5000 }
		);
		const ft1 = await page.evaluate(() => window.__waffle.getFeatureTree()?.features || []);
		logResult('Sketch created', ft1.length >= 1, `Features: ${ft1.length}`);
		await screenshot(page, '02-sketch-done');

		// ── Step 5: Extrude ──
		log('Extruding...');
		// Must show extrude dialog first (sets extrudeDialogState with sketchId)
		await page.evaluate(() => {
			window.__waffle.showExtrudeDialog();
		});
		await sleep(500);
		await page.evaluate(() => {
			window.__waffle.applyExtrude(15, 0, false);
		});
		await sleep(2000);

		await page.waitForFunction(
			() => (window.__waffle?.getFeatureTree()?.features || []).length >= 2,
			{ timeout: 10000 }
		);
		const ft2 = await page.evaluate(() => window.__waffle.getFeatureTree()?.features || []);
		logResult('Extrude created', ft2.length >= 2, `Features: ${ft2.length}`);
		await screenshot(page, '03-extruded');

		// ── Step 6: Check mesh face data ──
		log('Checking face data...');
		const meshInfo = await page.evaluate(() => {
			const meshes = window.__waffle.getMeshes();
			return meshes.map((m) => ({
				featureId: m.featureId,
				triangleCount: m.triangleCount,
				faceRangeCount: m.faceRangeCount,
				faceRanges: m.faceRanges.map((fr) => ({
					start: fr.start_index,
					end: fr.end_index,
					hasGeomRef: !!fr.geom_ref,
					selectorKeys: fr.geom_ref?.selector ? Object.keys(fr.geom_ref.selector) : [],
					selectorJson: JSON.stringify(fr.geom_ref?.selector)
				}))
			}));
		});

		if (meshInfo.length > 0) {
			const m = meshInfo[0];
			log(`Mesh: ${m.triangleCount} triangles, ${m.faceRangeCount} face ranges`);
			for (const fr of m.faceRanges) {
				log(`  face [${fr.start}-${fr.end}] geomRef=${fr.hasGeomRef} keys=${fr.selectorKeys} selector=${fr.selectorJson}`);
			}
			logResult('Mesh has face ranges', m.faceRangeCount > 0, `${m.faceRangeCount} ranges`);

			const allRole = m.faceRanges.every(
				(fr) => fr.selectorJson?.includes('"type":"Role"')
			);
			logResult(
				'All faces use Role selector (not Signature fallback)',
				allRole,
				allRole
					? 'all Role-based'
					: `some use: ${[...new Set(m.faceRanges.map((fr) => fr.selectorKeys.join(',')))].join('; ')}`
			);
			logResult(
				'Expected 6 faces for extruded rectangle',
				m.faceRangeCount === 6,
				`Got ${m.faceRangeCount}`
			);
		} else {
			logResult('Mesh has face ranges', false, 'No meshes found');
		}

		// ── Step 7: Test face hover/click ──
		log('Testing face selection...');

		// Switch to Iso view so we can see 3 faces of the box
		const isoBtn = page.locator('button:has-text("Iso")');
		if (await isoBtn.isVisible({ timeout: 1000 })) {
			await isoBtn.click();
			await sleep(1000);
		}
		await screenshot(page, '03b-iso-view');

		const canvas = page.locator('canvas').first();
		const canvasBox = await canvas.boundingBox();
		if (!canvasBox) {
			logResult('Canvas found', false, 'No canvas');
		} else {
			const cx = canvasBox.x + canvasBox.width / 2;
			const cy = canvasBox.y + canvasBox.height / 2;

			// Scan a wider grid to hit different faces from the iso view
			const positions = [];
			for (let dx = -200; dx <= 200; dx += 40) {
				for (let dy = -200; dy <= 200; dy += 40) {
					positions.push([cx + dx, cy + dy]);
				}
			}

			let hoverCount = 0;
			const selectedFaces = new Set();

			for (const [x, y] of positions) {
				await page.mouse.move(x, y);
				await sleep(100);
				const hRef = await page.evaluate(() => window.__waffle?.getHoveredRef?.());
				if (hRef) {
					hoverCount++;
					await page.mouse.click(x, y);
					await sleep(100);
					const sRefs = await page.evaluate(() => window.__waffle?.getSelectedRefs?.() ?? []);
					if (sRefs.length > 0) {
						const key = JSON.stringify(sRefs[0]?.selector);
						selectedFaces.add(key);
					}
				}
			}

			log(`Hover hits: ${hoverCount}/${positions.length}, distinct faces selected: ${selectedFaces.size}`);
			logResult('Face hover works', hoverCount > 0, `${hoverCount} hover hits`);
			logResult(
				'Face click selects faces',
				selectedFaces.size > 0,
				`${selectedFaces.size} distinct faces: ${[...selectedFaces].join(', ')}`
			);
			logResult(
				'Multiple faces selectable',
				selectedFaces.size >= 2,
				`${selectedFaces.size} distinct faces`
			);
			await screenshot(page, '04-face-selection');
		}

		await screenshot(page, '05-final');
	} catch (err) {
		console.error(`\n  [ERROR] ${err.message}`);
		console.error(err.stack);
		await screenshot(page, '99-error');
	} finally {
		await browser.close();

		console.log('\n=== Results ===\n');
		let passCount = 0;
		let failCount = 0;
		for (const r of results) {
			console.log(`  [${r.pass ? 'PASS' : 'FAIL'}] ${r.name}: ${r.detail}`);
			if (r.pass) passCount++;
			else failCount++;
		}
		console.log(`\n  ${passCount} passed, ${failCount} failed\n`);

		const logPath = join(outDir, 'console.log');
		writeFileSync(logPath, consoleLogs.map((l) => `[${l.type}] ${l.text}`).join('\n'));
		log(`Console log saved to ${logPath}`);

		if (failCount > 0) process.exit(1);
	}
})();
