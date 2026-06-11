// Sync the assay corpus + results from the dev source of truth
// (app/tests/cases/assay) into the static dir served by production builds
// (app/static/assay). Runs automatically via the npm `prebuild` hook so the
// deployed site can never drift from the repo corpus again (it was frozen at
// a 2026-03-21 snapshot showing the legacy kernel's score).
import { cpSync, rmSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const appDir = dirname(dirname(fileURLToPath(import.meta.url)));
const src = join(appDir, 'tests', 'cases', 'assay');
const dst = join(appDir, 'static', 'assay');

if (!existsSync(src)) {
	console.error(`sync-assay: source missing: ${src}`);
	process.exit(1);
}
rmSync(dst, { recursive: true, force: true });
cpSync(src, dst, { recursive: true });
console.log(`sync-assay: ${src} -> ${dst}`);
