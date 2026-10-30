#!/usr/bin/env node
// Static check for the toolbar icon registry (ui-chrome M13,
// specs/ui_toolbar_icons.md):
//   1. icons.js carries no colour of its own: no hex / rgb() / hsl() values
//      and no fill or stroke other than currentColor / none, so every theme
//      and button state recolours the icons through currentColor;
//   2. every `icon: '<name>'` and `<Icon name="<name>"` used under app/src
//      resolves in the registry (a typo would render nothing, silently).
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const registryPath = join(root, 'app/src/lib/ui/icons/icons.js');
const src = readFileSync(registryPath, 'utf8');
const errors = [];

const body = src.slice(src.indexOf('export const ICONS'));
for (const [re, what] of [
	[/#[0-9a-fA-F]{3,8}\b/g, 'hex colour'],
	[/\b(rgba?|hsla?)\(/g, 'rgb()/hsl() colour'],
	[/\b(fill|stroke)="(?!currentColor"|none")[^"]*"/g, 'fill/stroke other than currentColor/none'],
]) {
	for (const m of body.matchAll(re)) errors.push(`icons.js: ${what}: ${m[0]}`);
}

const { ICON_NAMES } = await import(pathToFileURL(registryPath).href);
const known = new Set(ICON_NAMES);

function walk(dir, out = []) {
	for (const e of readdirSync(dir)) {
		const p = join(dir, e);
		if (statSync(p).isDirectory()) walk(p, out);
		else if (/\.(svelte|js)$/.test(e)) out.push(p);
	}
	return out;
}
for (const f of walk(join(root, 'app/src'))) {
	if (f === registryPath) continue;
	const text = readFileSync(f, 'utf8');
	for (const m of text.matchAll(/\bicon:\s*'([a-z0-9-]+)'|<Icon\s[^>]*\bname="([a-z0-9-]+)"/g)) {
		const name = m[1] ?? m[2];
		if (!known.has(name)) errors.push(`${f.slice(root.length + 1)}: unknown icon "${name}"`);
	}
}

if (errors.length) {
	console.error(`check-icons: ${errors.length} problem(s)`);
	for (const e of errors) console.error('  ' + e);
	process.exit(1);
}
console.log(`check-icons: ok (${known.size} names)`);
