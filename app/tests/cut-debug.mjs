import { chromium } from 'playwright';

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });

const errors = [];
page.on('console', msg => {
    if (msg.type() === 'error' || msg.text().includes('error') || msg.text().includes('Error') || msg.text().includes('None') || msg.text().includes('failed'))
        errors.push(`[${msg.type()}] ${msg.text()}`);
});

await page.goto('http://localhost:8083', { waitUntil: 'networkidle' });
await page.waitForFunction(() => window.__waffle?.getState()?.engineReady, { timeout: 15000 });
console.log('Engine ready');

const canvas = await page.$('canvas');
const box = await canvas.boundingBox();
const cx = box.x + box.width / 2;
const cy = box.y + box.height / 2;

// Step 1: Create a box (sketch rect + extrude)
await page.evaluate(() => window.__waffle.enterSketch([0,0,0], [0,0,1]));
await page.waitForTimeout(300);
await page.evaluate(() => window.__waffle.setTool('rectangle'));
await page.waitForTimeout(100);
await page.mouse.click(cx - 80, cy + 40);
await page.waitForTimeout(200);
await page.mouse.click(cx + 80, cy - 40);
await page.waitForTimeout(200);
await page.evaluate(() => window.__waffle.setTool('select'));
await page.evaluate(() => window.__waffle.finishSketch());
await page.waitForTimeout(500);

// Extrude as boss
await page.evaluate(() => window.__waffle.showExtrudeDialog());
await page.waitForTimeout(300);
await page.locator('[data-testid="extrude-apply"]').click();
await page.waitForTimeout(500);

const meshes1 = await page.evaluate(() => window.__waffle.getMeshes());
console.log('After boss extrude:', meshes1.length, 'meshes');
console.log('Boss bbox:', await page.evaluate(() => JSON.stringify(window.__waffle.getMeshBoundingBox())));

// Step 2: Create a second sketch on the top face for the cut
// The top face is at z=10 (default depth). Sketch on XY plane at z=10
await page.evaluate(() => window.__waffle.enterSketch([0,0,10], [0,0,1]));
await page.waitForTimeout(300);
await page.evaluate(() => window.__waffle.setTool('rectangle'));
await page.waitForTimeout(100);

// Draw a smaller rect (subset of the boss) for the cut
await page.mouse.click(cx - 40, cy + 20);
await page.waitForTimeout(200);
await page.mouse.click(cx + 40, cy - 20);
await page.waitForTimeout(200);
await page.evaluate(() => window.__waffle.setTool('select'));
await page.evaluate(() => window.__waffle.finishSketch());
await page.waitForTimeout(500);

// Extrude as cut
await page.evaluate(() => window.__waffle.showExtrudeDialog());
await page.waitForTimeout(300);

// Check cut checkbox
await page.locator('[data-testid="extrude-cut"]').click();
await page.waitForTimeout(200);

console.log('About to apply cut extrude...');
await page.locator('[data-testid="extrude-apply"]').click();
await page.waitForTimeout(1000);

const meshes2 = await page.evaluate(() => window.__waffle.getMeshes());
console.log('After cut extrude:', meshes2.length, 'meshes');

const state = await page.evaluate(() => window.__waffle.getState());
console.log('Engine state:', JSON.stringify(state));

const tree = await page.evaluate(() => JSON.parse(JSON.stringify(window.__waffle.getFeatureTree())));
console.log('Features:', tree.features.length);
for (const f of tree.features) {
    console.log(`  ${f.name} (${f.operation?.type}): status=${f.status || 'ok'}`);
}

// Check for any error toasts
const toasts = await page.evaluate(() => window.__waffle.getToasts());
console.log('Toasts:', JSON.stringify(toasts));

console.log('\nErrors/warnings:');
for (const e of errors) console.log(' ', e);

await page.screenshot({ path: '/home/claude/workspace/app/tests/cut-debug.png' });

await browser.close();
