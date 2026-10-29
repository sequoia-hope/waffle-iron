/**
 * The exported sheet SVG resolves the drawing's CSS variables (REVIEW.md
 * §3.3, remediation Q6). Pure: no browser.
 *
 * On screen a sheet paints with `var(--drawing-…)` and `var(--font-ui)` so it
 * follows the theme. A file opened outside the app has no stylesheet: every
 * `var()` paint is invalid there, strokes vanish and fills turn black. The
 * export writes the same print palette the PDF uses, and a literal font.
 */
import { test, expect } from '@playwright/test';
import { standaloneSheetSvg, PRINT_FONT_FAMILY } from '../../src/lib/drawings/standalone.js';
import { PRINT_PALETTE } from '../../src/lib/drawings/pdf.js';
import { DRAWING_TOKENS, DEFAULT_STYLE } from '../../src/lib/drawings/style.js';

test('every drawing token and the UI font resolve to literals', () => {
	const paints = Object.values(DRAWING_TOKENS)
		.map((v, i) => `<line x1="0" y1="${i}" x2="1" y2="${i}" stroke="${v}" fill="${v}"/>`)
		.join('');
	const svg = `<svg xmlns="http://www.w3.org/2000/svg">${paints}<text font-family="${DEFAULT_STYLE.fontFamily}">A</text></svg>`;
	const out = standaloneSheetSvg(svg);
	expect(out.svg).not.toContain('var(');
	expect(out.warnings).toEqual([]);
	expect(out.svg).toContain(`font-family="${PRINT_FONT_FAMILY}"`);
	// The palette is the PDF's, so the two exports print the same sheet.
	expect(out.svg).toContain('stroke="#000000"'); // ink
	expect(out.svg).toContain('fill="#ffffff"'); // paper
	const grey = Math.round(PRINT_PALETTE['--drawing-hidden'][0] * 255).toString(16).padStart(2, '0');
	expect(out.svg).toContain(`stroke="#${grey}${grey}${grey}"`);
});

test('an unknown variable is written black and named, never left as var()', () => {
	const out = standaloneSheetSvg('<svg><path d="M0 0" stroke="var(--something-new)"/></svg>');
	expect(out.svg).toBe('<svg><path d="M0 0" stroke="#000000"/></svg>');
	expect(out.warnings).toEqual([expect.stringContaining('--something-new')]);
});
