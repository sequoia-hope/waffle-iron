/**
 * The sheet PDF (`specs/drawings_and_mbd.md` §8, D4b): one page, true to
 * scale, written in the page with no server and no dependency.
 *
 * ## Why not a library, and why not the print dialog
 *
 * §8 says "PDF is the browser's print path over the SVG". `window.print()`
 * does produce a true-to-scale sheet — but it produces it in a dialog, with
 * the browser's own margins and headers, and it hands back NOTHING: no bytes
 * to deliver through the export door, nothing an agent can ask for, nothing a
 * test can assert on. The export door's shape (`deliver`, `file_name`,
 * `mime_type`, `bytes`) needs a file.
 *
 * A library would do it — `jspdf` plus `svg2pdf.js` is the usual pair — and
 * was rejected on three counts. The app's CSP allows scripts from a short CDN
 * allowlist and the app bundles nothing but `three` and Threlte, so a PDF
 * library would be the first dependency added for an export path; `svg2pdf`
 * brings its own SVG parser, which would be a SECOND reading of our markup
 * with its own idea of what our markup means; and the whole of what this
 * sheet needs is lines, filled polygons, circles and single-line text in one
 * base-14 font. That is a few hundred lines of PDF operators, which is less
 * than the glue would be.
 *
 * ## The PDF is written FROM the SVG, which makes them the same drawing
 *
 * The input is the string `renderSheetSvg` produced — not the sheet data.
 * That is deliberate: the PDF is then provably the drawing on the screen
 * rather than a second rendering of the same geometry, which is the mistake
 * `DrawingView.svelte` and `export_svg` both refuse. The scanner knows
 * exactly the vocabulary `svg.js` and `sheet.js` emit (`svg`, `g`, `defs`,
 * `clipPath`, `rect`, `line`, `circle`, `polygon`, `path` with `M`/`L`/`l`/`Z`
 * only, `text`, `title`) and **reports anything else** rather than dropping
 * it: a renderer change this writer does not understand becomes a warning on
 * the export, not a line missing from a manufacturing drawing.
 *
 * ## Ink, not theme
 *
 * The paints in the SVG are `var(--drawing-…)` references, which a PDF has no
 * way to resolve. They are mapped to a PRINT palette — black ink on white
 * paper — rather than to whatever the viewer's theme resolves to. A drawing
 * exported from the dark theme with a black background would waste a
 * cartridge and print the lines invisibly; a sheet is paper, and paper is
 * white.
 *
 * ## Units
 *
 * A PDF user unit is 1/72 inch. The content stream opens with one `cm` that
 * makes user space PAPER MILLIMETRES WITH Y DOWN — the SVG's own
 * coordinates — so every number from the markup is written unchanged and a
 * 0.5 mm line is `0.5 w`. Text is the one thing that cannot ride a flipped
 * axis (the glyphs would be upside down), so each text run sets a text matrix
 * that flips back.
 */

/** PDF points per millimetre. */
const PT_PER_MM = 72 / 25.4;

/**
 * The print palette: which ink each drawing token becomes.
 *
 * Black for everything that is a line of the drawing, a mid grey for hidden
 * detail and hatching so they read as lighter than an outline (ISO 128's line
 * groups are about WIDTH, but a grey hidden line survives a photocopier
 * better than a 0.35 mm black one), and white paper.
 */
export const PRINT_PALETTE = {
	'--drawing-paper': [1, 1, 1],
	'--drawing-ink': [0, 0, 0],
	'--drawing-hidden': [0.45, 0.45, 0.45],
	'--drawing-annotation': [0, 0, 0],
	'--drawing-hatch': [0.35, 0.35, 0.35]
};

/**
 * Helvetica's advance widths, in 1/1000 em, for the printable ASCII range —
 * the Adobe base-14 metrics.
 *
 * Needed because `text-anchor="middle"` and `"end"` are SVG features a PDF
 * has no equivalent for: the writer has to measure the string and move the
 * origin itself. Getting them wrong does not break the file, it moves every
 * dimension's number off its line, which is why they are the real table and
 * not an average.
 */
const HELVETICA_WIDTHS = {
	' ': 278, '!': 278, '"': 355, '#': 556, $: 556, '%': 889, '&': 667, "'": 191,
	'(': 333, ')': 333, '*': 389, '+': 584, ',': 278, '-': 333, '.': 278, '/': 278,
	0: 556, 1: 556, 2: 556, 3: 556, 4: 556, 5: 556, 6: 556, 7: 556, 8: 556, 9: 556,
	':': 278, ';': 278, '<': 584, '=': 584, '>': 584, '?': 556, '@': 1015,
	A: 667, B: 667, C: 722, D: 722, E: 667, F: 611, G: 778, H: 722, I: 278, J: 500,
	K: 667, L: 556, M: 833, N: 722, O: 778, P: 667, Q: 778, R: 722, S: 667, T: 611,
	U: 722, V: 667, W: 944, X: 667, Y: 667, Z: 611,
	'[': 278, '\\': 278, ']': 278, '^': 469, _: 556, '`': 333,
	a: 556, b: 556, c: 500, d: 556, e: 556, f: 278, g: 556, h: 556, i: 222, j: 222,
	k: 500, l: 222, m: 833, n: 556, o: 556, p: 556, q: 556, r: 333, s: 500, t: 278,
	u: 556, v: 500, w: 722, x: 500, y: 500, z: 500,
	'{': 334, '|': 260, '}': 334, '~': 584,
	// The two non-ASCII glyphs a dimension prints, at their WinAnsi code
	// points: the degree sign and the Ø a diameter becomes (see `toWinAnsi`).
	'°': 400,
	'Ø': 778,
	'±': 584
};

/** The width of `text` at `size` mm, in mm. */
function textWidthMm(text, size) {
	let em = 0;
	for (const ch of text) em += HELVETICA_WIDTHS[ch] ?? 556;
	return (em / 1000) * size;
}

/**
 * One string as WinAnsi bytes, with the substitutions a drawing needs.
 *
 * `⌀` (U+2300, DIAMETER SIGN) is NOT in WinAnsiEncoding and is not in any
 * base-14 font, so it becomes `Ø` (U+00D8) — the substitution CAD systems
 * have used for the diameter symbol since before Unicode had one, and the
 * glyph a drafter reads as "diameter". It is reported, because a sheet whose
 * diameter symbol changed shape between the screen and the file should say
 * so. The typographic minus and multiplication signs fold to ASCII; anything
 * else outside Latin-1 becomes `?` and is named.
 */
function toWinAnsi(text, warnings) {
	const bytes = [];
	for (const ch of String(text ?? '')) {
		let out = ch;
		if (ch === '⌀' || ch === '⊘') {
			out = 'Ø';
			warnings.push(`the diameter sign ${ch} is written as Ø, which no base-14 font has as ⌀`);
		} else if (ch === '−') {
			out = '-';
		} else if (ch === '×') {
			out = 'x';
		}
		const code = out.codePointAt(0);
		if (code <= 0xff) {
			bytes.push(code);
		} else {
			bytes.push(0x3f);
			warnings.push(`the character ${ch} is not in WinAnsiEncoding and was written as ?`);
		}
	}
	return bytes;
}

/** A PDF literal string's bytes, with `(`, `)` and `\` escaped. */
function pdfString(text, warnings) {
	const out = [0x28];
	for (const b of toWinAnsi(text, warnings)) {
		if (b === 0x28 || b === 0x29 || b === 0x5c) out.push(0x5c);
		out.push(b);
	}
	out.push(0x29);
	return out;
}

/** A number for the content stream: fixed decimals, no exponent, no `-0`. */
function num(x) {
	if (!Number.isFinite(x)) return '0';
	const r = Number(x.toFixed(3));
	return String(r === 0 ? 0 : r);
}

/** Parse `name="value"` pairs out of a tag's attribute text. */
function parseAttrs(text) {
	/** @type {Record<string, string>} */
	const out = {};
	const re = /([A-Za-z_:][-A-Za-z0-9_:.]*)\s*=\s*"([^"]*)"/g;
	let m;
	while ((m = re.exec(text)) !== null) {
		out[m[1]] = m[2]
			.replaceAll('&quot;', '"')
			.replaceAll('&gt;', '>')
			.replaceAll('&lt;', '<')
			.replaceAll('&amp;', '&');
	}
	return out;
}

function numAttr(attrs, name, fallback = 0) {
	const v = Number.parseFloat(attrs[name]);
	return Number.isFinite(v) ? v : fallback;
}

/** The ink a `var(--token)` paint means, or `null` for `none`/absent. */
function ink(value, palette, warnings) {
	if (!value || value === 'none') return null;
	const m = /^var\(\s*(--[A-Za-z0-9-]+)\s*\)$/.exec(value.trim());
	if (m) {
		const rgb = palette[m[1]];
		if (rgb) return rgb;
		warnings.push(`the paint ${value} has no print ink and was written as black`);
		return [0, 0, 0];
	}
	// A literal colour would be a renderer that stopped going through the
	// tokens (style.js: "every paint is a `var(--drawing-…)` reference, never
	// a literal"). Named rather than guessed at.
	warnings.push(`the paint "${value}" is not a drawing token and was written as black`);
	return [0, 0, 0];
}

/**
 * The sheet SVG as a one-page PDF.
 *
 * @param {object} input
 * @param {string} input.svg the string `renderSheetSvg` produced
 * @param {number} input.widthMm
 * @param {number} input.heightMm
 * @param {Record<string, number[]>} [input.palette] token → RGB, 0–1
 * @returns {{ bytes: Uint8Array, pages: number, warnings: string[] }}
 */
export function renderSheetPdf({ svg, widthMm, heightMm, palette = PRINT_PALETTE }) {
	const warnings = [];
	const ops = [];
	const w = Number(widthMm);
	const h = Number(heightMm);
	if (!(Number.isFinite(w) && w > 0 && Number.isFinite(h) && h > 0)) {
		throw new Error(`a sheet PDF needs a positive paper size, got ${widthMm} × ${heightMm}`);
	}

	// User space becomes paper millimetres with y DOWN, which is the SVG's own
	// frame — so every coordinate below is written as the markup had it.
	ops.push(`q ${num(PT_PER_MM)} 0 0 ${num(-PT_PER_MM)} 0 ${num(h * PT_PER_MM)} cm`);

	/** Transform stack: user = (x * sx + tx, y * sy + ty), in paper mm. */
	const stack = [{ sx: 1, sy: 1, tx: 0, ty: 0 }];
	const top = () => stack[stack.length - 1];
	const px = (x) => x * top().sx + top().tx;
	const py = (y) => y * top().sy + top().ty;
	/** A length scales by the transform; ours is uniform (see the tripwire). */
	const plen = (v) => v * Math.abs(top().sx);

	/** `<clipPath id>` → the circle it holds, in the space it was declared in. */
	const clips = new Map();
	/** Per open element, whether it pushed a `q`/`Q` pair for a clip. */
	const openTags = [];
	let clipTarget = null;

	const paint = (attrs, pathOps) => {
		const fill = ink(attrs.fill, palette, warnings);
		const stroke = ink(attrs.stroke, palette, warnings);
		if (!fill && !stroke) return;
		ops.push('q');
		if (fill) ops.push(`${num(fill[0])} ${num(fill[1])} ${num(fill[2])} rg`);
		if (stroke) {
			ops.push(`${num(stroke[0])} ${num(stroke[1])} ${num(stroke[2])} RG`);
			ops.push(`${num(plen(numAttr(attrs, 'stroke-width', 0.25)))} w`);
			const dash = (attrs['stroke-dasharray'] ?? '')
				.split(/[\s,]+/)
				.filter((s) => s.length > 0)
				.map((s) => num(plen(Number.parseFloat(s))));
			ops.push(dash.length > 0 ? `[${dash.join(' ')}] 0 d` : '[] 0 d');
			// Round caps, as the SVG curves ask for: a butt-capped 0.5 mm line
			// leaves a visible notch at every corner of a projected outline.
			ops.push('1 J 1 j');
		}
		ops.push(...pathOps);
		ops.push(fill && stroke ? 'B' : fill ? 'f' : 'S');
		ops.push('Q');
	};

	/** A circle as four Béziers — PDF has no circle operator. */
	const circleOps = (cx, cy, r) => {
		// 4/3·tan(π/8): the control-point offset that makes a cubic Bézier
		// quadrant match a circle to within 0.02 % of the radius.
		const k = r * 0.5522847498307936;
		const [x, y] = [px(cx), py(cy)];
		const kk = plen(k);
		return [
			`${num(x + plen(r))} ${num(y)} m`,
			`${num(x + plen(r))} ${num(y + kk)} ${num(x + kk)} ${num(y + plen(r))} ${num(x)} ${num(y + plen(r))} c`,
			`${num(x - kk)} ${num(y + plen(r))} ${num(x - plen(r))} ${num(y + kk)} ${num(x - plen(r))} ${num(y)} c`,
			`${num(x - plen(r))} ${num(y - kk)} ${num(x - kk)} ${num(y - plen(r))} ${num(x)} ${num(y - plen(r))} c`,
			`${num(x + kk)} ${num(y - plen(r))} ${num(x + plen(r))} ${num(y - kk)} ${num(x + plen(r))} ${num(y)} c`,
			'h'
		];
	};

	const source = String(svg ?? '');
	let i = 0;
	while (i < source.length) {
		const lt = source.indexOf('<', i);
		if (lt < 0) break;
		const gt = source.indexOf('>', lt);
		if (gt < 0) {
			warnings.push('the markup ended inside a tag');
			break;
		}
		const raw = source.slice(lt + 1, gt);
		i = gt + 1;
		if (raw.startsWith('?') || raw.startsWith('!')) continue;
		const closing = raw.startsWith('/');
		const body = closing ? raw.slice(1) : raw;
		const selfClosing = body.endsWith('/');
		const inner = selfClosing ? body.slice(0, -1) : body;
		const space = inner.search(/\s/);
		const name = (space < 0 ? inner : inner.slice(0, space)).trim();
		const attrs = parseAttrs(space < 0 ? '' : inner.slice(space));

		if (closing) {
			const opened = openTags.pop();
			if (opened?.name === 'svg') stack.pop();
			if (opened?.clipped) ops.push('Q');
			if (opened?.name === 'clipPath') clipTarget = null;
			continue;
		}

		switch (name) {
			case 'svg': {
				// The ROOT svg has no x/y/viewBox offset to apply beyond its
				// own viewBox; a NESTED one positions a view on the sheet.
				const vb = (attrs.viewBox ?? '').split(/[\s,]+/).map(Number);
				const parent = top();
				let entry = { ...parent };
				if (vb.length === 4 && vb.every(Number.isFinite) && vb[2] > 0 && vb[3] > 0) {
					const x = numAttr(attrs, 'x', 0);
					const y = numAttr(attrs, 'y', 0);
					const width = numAttr(attrs, 'width', vb[2]);
					const height = numAttr(attrs, 'height', vb[3]);
					const sx = width / vb[2];
					const sy = height / vb[3];
					if (Math.abs(sx - sy) > 1e-9) {
						// Our composition always sizes a nested view to its own
						// viewBox, so this cannot happen — and if it does, the
						// uniform-scale assumption below (stroke widths, text)
						// is wrong and the drawing is quietly distorted.
						warnings.push(
							`a nested view is scaled ${sx} × ${sy}; the PDF assumes a uniform scale`
						);
					}
					entry = {
						sx: parent.sx * sx,
						sy: parent.sy * sy,
						tx: parent.sx * (x - vb[0] * sx) + parent.tx,
						ty: parent.sy * (y - vb[1] * sy) + parent.ty
					};
				}
				stack.push(entry);
				openTags.push({ name: 'svg', clipped: false });
				if (selfClosing) {
					stack.pop();
					openTags.pop();
				}
				break;
			}
			case 'defs':
			case 'clipPath': {
				if (name === 'clipPath') clipTarget = attrs.id ?? '';
				if (!selfClosing) openTags.push({ name, clipped: false });
				break;
			}
			case 'g': {
				const ref = /^url\(#(.+)\)$/.exec(attrs['clip-path'] ?? '');
				let clipped = false;
				if (ref) {
					const circle = clips.get(ref[1]);
					if (circle) {
						ops.push('q');
						ops.push(...circleOps(circle.cx, circle.cy, circle.r));
						ops.push('W n');
						clipped = true;
					} else {
						warnings.push(`the clip path #${ref[1]} was not found; that view is not cropped`);
					}
				}
				if (!selfClosing) openTags.push({ name: 'g', clipped });
				else if (clipped) ops.push('Q');
				break;
			}
			case 'title': {
				// Accessibility text, not drawing. Skipped to its close.
				const close = source.indexOf('</title>', i);
				i = close < 0 ? source.length : close + '</title>'.length;
				break;
			}
			case 'rect': {
				const x = px(numAttr(attrs, 'x'));
				const y = py(numAttr(attrs, 'y'));
				const rw = plen(numAttr(attrs, 'width'));
				const rh = plen(numAttr(attrs, 'height'));
				paint(attrs, [`${num(x)} ${num(y)} ${num(rw)} ${num(rh)} re`]);
				break;
			}
			case 'line': {
				const a = [px(numAttr(attrs, 'x1')), py(numAttr(attrs, 'y1'))];
				const b = [px(numAttr(attrs, 'x2')), py(numAttr(attrs, 'y2'))];
				paint(attrs, [`${num(a[0])} ${num(a[1])} m`, `${num(b[0])} ${num(b[1])} l`]);
				break;
			}
			case 'circle': {
				const cx = numAttr(attrs, 'cx');
				const cy = numAttr(attrs, 'cy');
				const r = numAttr(attrs, 'r');
				if (clipTarget !== null) {
					// Inside a `<clipPath>`: recorded, not drawn.
					clips.set(clipTarget, { cx, cy, r });
					break;
				}
				paint(attrs, circleOps(cx, cy, r));
				break;
			}
			case 'polygon': {
				const pts = (attrs.points ?? '')
					.split(/\s+/)
					.filter((p) => p.length > 0)
					.map((p) => p.split(',').map(Number));
				if (pts.length < 2 || !pts.every((p) => p.length === 2 && p.every(Number.isFinite))) {
					warnings.push('a polygon had no usable points');
					break;
				}
				const path = pts.map(
					([x, y], k) => `${num(px(x))} ${num(py(y))} ${k === 0 ? 'm' : 'l'}`
				);
				path.push('h');
				paint(attrs, path);
				break;
			}
			case 'path': {
				const { path, unknown } = pathOperators(attrs.d ?? '', px, py);
				for (const cmd of unknown) {
					warnings.push(`the path command "${cmd}" is not written to the PDF`);
				}
				if (path.length === 0) break;
				paint(attrs, path);
				break;
			}
			case 'text': {
				const close = source.indexOf('</text>', i);
				const content = close < 0 ? '' : source.slice(i, close);
				i = close < 0 ? source.length : close + '</text>'.length;
				const text = content
					.replaceAll('&quot;', '"')
					.replaceAll('&gt;', '>')
					.replaceAll('&lt;', '<')
					.replaceAll('&amp;', '&');
				if (text.length === 0) break;
				const size = plen(numAttr(attrs, 'font-size', 3.5));
				const fill = ink(attrs.fill, palette, warnings) ?? [0, 0, 0];
				let x = px(numAttr(attrs, 'x'));
				let y = py(numAttr(attrs, 'y'));
				// SVG anchors and baselines, which PDF has neither of: the
				// writer measures the string and moves the origin.
				const width = textWidthMm(text, size);
				if (attrs['text-anchor'] === 'middle') x -= width / 2;
				else if (attrs['text-anchor'] === 'end') x -= width;
				const baseline = attrs['dominant-baseline'] ?? 'auto';
				// Helvetica's cap height is 0.717 em; centring on it puts the
				// visual middle of a line of capitals and digits on `y`, which
				// is what `middle` means for a dimension's number. An
				// approximation of a typographic rule, named as one.
				if (baseline === 'middle' || baseline === 'central') y += 0.717 * size * 0.5;
				else if (baseline === 'hanging') y += 0.717 * size;
				const rot = /^rotate\(\s*([-0-9.]+)/.exec(attrs.transform ?? '');
				ops.push('q');
				ops.push(`${num(fill[0])} ${num(fill[1])} ${num(fill[2])} rg`);
				ops.push('BT');
				ops.push(`/F1 ${num(size)} Tf`);
				if (rot) {
					// Rotation about the text's own anchor, composed with the
					// y-flip that keeps the glyphs upright.
					const a = (Number(rot[1]) * Math.PI) / 180;
					const [c, s] = [Math.cos(a), Math.sin(a)];
					const [ax, ay] = [px(numAttr(attrs, 'x')), py(numAttr(attrs, 'y'))];
					// `[cos, sin, sin, −cos]` is the SVG rotation composed with
					// the y-flip that keeps the glyphs upright: flip then
					// rotate, which in PDF's `[a b c d]` is
					// `a = cos, b = sin, c = sin, d = −cos`. The translation is
					// the ANCHOR-ADJUSTED origin rotated about the anchor,
					// which is what SVG does — it positions the text and then
					// turns the whole thing.
					const dx = x - ax;
					const dy = y - ay;
					ops.push(
						`${num(c)} ${num(s)} ${num(s)} ${num(-c)} ` +
							`${num(ax + c * dx - s * dy)} ${num(ay + s * dx + c * dy)} Tm`
					);
				} else {
					ops.push(`1 0 0 -1 ${num(x)} ${num(y)} Tm`);
				}
				ops.push(`${latin1(pdfString(text, warnings))} Tj`);
				ops.push('ET');
				ops.push('Q');
				break;
			}
			default:
				// The tripwire. A renderer change this writer does not know
				// becomes a warning on the export rather than a line missing
				// from a manufacturing drawing.
				warnings.push(`the element <${name}> is not written to the PDF`);
				if (!selfClosing) openTags.push({ name, clipped: false });
				break;
		}
	}
	ops.push('Q');

	const bytes = assemble(ops.join('\n'), w * PT_PER_MM, h * PT_PER_MM);
	return { bytes, pages: 1, warnings };
}

/** Latin-1 bytes back to a JS string, so the stream can be built as text. */
function latin1(bytes) {
	return bytes.map((b) => String.fromCharCode(b)).join('');
}

/**
 * The `M`/`L`/`l`/`Z` subset `svg.js` emits, as PDF path operators.
 *
 * Only that subset, because only that subset is produced: every arc is
 * sampled into line segments before it reaches the markup (`conicPoints`).
 * Anything else is RETURNED as unknown rather than skipped, so the caller can
 * warn — a path command silently dropped is a curve missing from the drawing.
 */
function pathOperators(d, px, py) {
	const path = [];
	const unknown = [];
	const tokens = String(d).match(/[A-Za-z]|-?\d*\.?\d+(?:e[-+]?\d+)?/gi) ?? [];
	let k = 0;
	let cursor = [0, 0];
	let started = false;
	while (k < tokens.length) {
		const cmd = tokens[k++];
		if (!/^[A-Za-z]$/.test(cmd)) continue;
		switch (cmd) {
			case 'M':
			case 'L': {
				const x = Number(tokens[k++]);
				const y = Number(tokens[k++]);
				if (!Number.isFinite(x) || !Number.isFinite(y)) break;
				cursor = [x, y];
				path.push(`${num(px(x))} ${num(py(y))} ${cmd === 'M' ? 'm' : 'l'}`);
				started = true;
				break;
			}
			case 'l': {
				const dx = Number(tokens[k++]);
				const dy = Number(tokens[k++]);
				if (!Number.isFinite(dx) || !Number.isFinite(dy)) break;
				cursor = [cursor[0] + dx, cursor[1] + dy];
				// A zero-length `l 0 0` is how a line seen END-ON is drawn
				// (`curvePath`); with a round cap it is a dot, and PDF needs
				// the segment to exist for the cap to be painted.
				path.push(`${num(px(cursor[0]))} ${num(py(cursor[1]))} l`);
				started = true;
				break;
			}
			case 'Z':
			case 'z':
				if (started) path.push('h');
				break;
			default:
				unknown.push(cmd);
				break;
		}
	}
	return { path, unknown };
}

/**
 * The PDF file around one content stream.
 *
 * Uncompressed, because a sheet's stream is tens of kilobytes and `pako` would
 * be the dependency this module exists to avoid; the xref offsets are counted
 * in BYTES over the Latin-1 encoding, which is why the whole file is built as
 * a byte array rather than as a string with a length in UTF-16 code units.
 */
function assemble(content, widthPt, heightPt) {
	const objects = [
		'<< /Type /Catalog /Pages 2 0 R >>',
		'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
		`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${num(widthPt)} ${num(heightPt)}] ` +
			`/Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>`,
		null, // the stream, built below
		'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>'
	];

	/** @type {number[]} */
	const out = [];
	const push = (text) => {
		for (let i = 0; i < text.length; i++) out.push(text.charCodeAt(i) & 0xff);
	};
	push('%PDF-1.4\n');
	// A binary comment line, as the spec recommends, so a transfer agent that
	// sniffs the first bytes treats the file as binary rather than as text and
	// does not translate its line endings.
	out.push(0x25, 0xe2, 0xe3, 0xcf, 0xd3, 0x0a);

	const offsets = [];
	const streamBytes = [];
	for (let i = 0; i < content.length; i++) streamBytes.push(content.charCodeAt(i) & 0xff);

	for (let i = 0; i < objects.length; i++) {
		offsets.push(out.length);
		push(`${i + 1} 0 obj\n`);
		if (objects[i] === null) {
			push(`<< /Length ${streamBytes.length} >>\nstream\n`);
			// One at a time rather than a spread: a sheet's stream runs to
			// tens of thousands of bytes and `push(...array)` passes every
			// element as an argument, which overflows the call stack.
			for (const b of streamBytes) out.push(b);
			push('\nendstream\n');
		} else {
			push(`${objects[i]}\n`);
		}
		push('endobj\n');
	}

	const xref = out.length;
	push(`xref\n0 ${objects.length + 1}\n`);
	push('0000000000 65535 f \n');
	for (const at of offsets) {
		push(`${String(at).padStart(10, '0')} 00000 n \n`);
	}
	push(`trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`);
	return new Uint8Array(out);
}
