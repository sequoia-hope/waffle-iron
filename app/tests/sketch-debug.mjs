/**
 * Quick test: does sketch mode work in production build?
 */
import { chromium } from 'playwright';

const browser = await chromium.launch({
  headless: true,
  args: ['--use-gl=angle', '--use-angle=swiftshader', '--no-sandbox']
});
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
const logs = [];
page.on('console', m => logs.push(`[${m.type()}] ${m.text()}`));
page.on('pageerror', e => logs.push(`[PAGE ERROR] ${e.message}`));

console.log('Loading production build...');
await page.goto('http://localhost:5173', { waitUntil: 'networkidle' });
await page.waitForTimeout(4000);

const waffleLogs = logs.filter(l => l.includes('[waffle'));
console.log('Init logs:', waffleLogs.length ? waffleLogs.join('\n  ') : '(none)');

const hasWaffle = await page.evaluate(() => !!window.__waffle);
console.log('__waffle exists:', hasWaffle);

if (hasWaffle) {
  const diag = await page.evaluate(() => window.__waffle.diagnose());
  console.log('Engine ready:', diag.engineReady);
  console.log('Status:', diag.statusMessage);

  // Click sketch
  await page.locator('[data-testid="toolbar-btn-sketch"]').click();
  await page.waitForTimeout(2000);

  const after = await page.evaluate(() => window.__waffle.diagnose());
  console.log('After sketch click - active:', after.sketchMode.active, 'tool:', after.activeTool);

  // Draw
  const box = await page.locator('canvas').boundingBox();
  if (box) {
    await page.mouse.click(box.x + box.width/2 - 50, box.y + box.height/2);
    await page.waitForTimeout(300);
    await page.mouse.click(box.x + box.width/2 + 50, box.y + box.height/2);
    await page.waitForTimeout(500);
    const draw = await page.evaluate(() => window.__waffle.diagnose());
    console.log('After draw - entities:', draw.entityCount);
  }

  await page.screenshot({ path: '/tmp/prod-sketch-test.png' });
  console.log('Screenshot: /tmp/prod-sketch-test.png');
} else {
  console.log('ALL LOGS:');
  logs.forEach(l => console.log(' ', l));
  await page.screenshot({ path: '/tmp/prod-no-waffle.png' });
}

const errors = logs.filter(l => l.includes('[error]') || l.includes('PAGE ERROR'));
if (errors.length) {
  console.log('\nErrors:');
  errors.forEach(l => console.log(' ', l));
}

await browser.close();
