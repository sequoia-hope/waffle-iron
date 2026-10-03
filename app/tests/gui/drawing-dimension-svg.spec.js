/**
 * The SVG dimension renderer (`specs/drawings_and_mbd.md` §7, increment D3).
 *
 * Asserts on the SVG **DOM** — element counts, text content, coordinate
 * finiteness, theme tokens — not on pixels. A screenshot baseline for a
 * drawing rots on every font or antialiasing change and tells you nothing
 * about whether the number is right; the DOM says exactly that.
 *
 * ## Where the number comes from, and what each suite owns
 *
 * The measurement itself is Rust's and is pinned there:
 * `waffle_types::annotation::measure` for the rules, and
 * `crates/test-harness/tests/d3_annotation_measure.rs` for a real box
 * projected top-down measuring its authored 40 mm. This spec takes the
 * already-measured `ViewLayout` the engine will emit (D4a wires it) and pins
 * the half Rust cannot reach: that the renderer PRINTS that value at the
 * stated precision, and that the geometry it draws agrees with the text.
 *
 * That last check is what keeps the two halves honest: the dimension line's
 * drawn length in paper mm must equal the printed number times the view
 * scale. A renderer that took its text from one place and its arrows from
 * another would pass a text-only assertion and fail this one.
 */
import { test, expect } from './helpers/waffle-test.js';

/** A 40 × 25 mm plate's top view, as `ViewLayout` JSON (meters, v up). */
const PLATE_W = 0.04;
const PLATE_D = 0.025;

function line(start, end) {
	return { type: 'Line', start, end };
}

function edge(curve, visibility = 'Visible') {
	return { geometry: curve, visibility, kind: 'Edge' };
}

/** The plate outline: four edges, plus one hidden edge to exercise the dash. */
function plateLayout(annotations) {
	return {
		curves: [
			edge(line([0, 0], [PLATE_W, 0])),
			edge(line([PLATE_W, 0], [PLATE_W, PLATE_D])),
			edge(line([PLATE_W, PLATE_D], [0, PLATE_D])),
			edge(line([0, PLATE_D], [0, 0])),
			edge(line([0.01, 0], [0.01, PLATE_D]), 'Hidden')
		],
		bbox: [
			[0, 0],
			[PLATE_W, PLATE_D]
		],
		annotations
	};
}

/** The two vertical walls as anchors, for a horizontal width dimension. */
const WIDTH_DIMENSION = {
	type: 'Dimension',
	kind: { type: 'HDistance' },
	anchors: [
		{ type: 'Curve', curve: line([0, 0], [0, PLATE_D]) },
		{ type: 'Curve', curve: line([PLATE_W, 0], [PLATE_W, PLATE_D]) }
	],
	value: PLATE_W,
	precision: 2,
	placement: { dx: 0, dy: 0 }
};

const HOLE_R = 0.005;
const HOLE_CIRCLE = {
	type: 'Circle',
	center: [0.02, 0.0125],
	radius: HOLE_R,
	start_angle: 0,
	end_angle: Math.PI * 2
};

/**
 * The same value with every object's keys in the opposite order. Arrays keep
 * their order — that is data, not serialization — so the result is the same
 * record under a different key order, and must render to the same bytes.
 */
function reverseKeys(value) {
	if (Array.isArray(value)) return value.map(reverseKeys);
	if (value === null || typeof value !== 'object') return value;
	return Object.fromEntries(
		Object.entries(value)
			.reverse()
			.map(([k, v]) => [k, reverseKeys(v)])
	);
}

/** Call the renderer in the page and get back its result. */
async function render(page, input) {
	return page.evaluate((i) => window.__waffle.renderDrawingSvg(i), input);
}

/**
 * Render and parse, returning the root `<svg>` element handle plus the raw
 * string. The SVG is parsed with DOMParser in the page rather than injected
 * into the live document, so the spec cannot accidentally depend on the
 * app's own layout.
 */
async function renderAndQuery(page, input, query) {
	return page.evaluate(
		({ i, q }) => {
			const out = window.__waffle.renderDrawingSvg(i);
			const doc = new DOMParser().parseFromString(out.svg, 'image/svg+xml');
			const err = doc.querySelector('parsererror');
			if (err) throw new Error(`the SVG does not parse: ${err.textContent}`);
			const root = doc.documentElement;
			const sel = (s) => Array.from(root.querySelectorAll(s));
			return {
				warnings: out.warnings,
				widthMm: out.widthMm,
				heightMm: out.heightMm,
				svg: out.svg,
				viewBox: root.getAttribute('viewBox'),
				width: root.getAttribute('width'),
				counts: Object.fromEntries(
					Object.entries(q.counts ?? {}).map(([k, s]) => [k, sel(s).length])
				),
				texts: sel(q.texts ?? 'text').map((t) => t.textContent),
				attrs: Object.fromEntries(
					Object.entries(q.attrs ?? {}).map(([k, [s, a]]) => [
						k,
						sel(s).map((e) => e.getAttribute(a))
					])
				),
				// Every attribute in the whole document as `name=value`, for
				// the NaN tripwire and the colour-literal check below.
				allNumbers: sel('*').flatMap((e) =>
					Array.from(e.attributes).map((a) => `${a.name}=${a.value}`)
				)
			};
		},
		{ i: input, q: query ?? {} }
	);
}

test.describe('D3 SVG dimension renderer', () => {
	test('a linear dimension draws two extension lines, a dimension line and two arrowheads', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(page, { layout: plateLayout([WIDTH_DIMENSION]) }, {
			counts: {
				svg: 'g.wi-annotations',
				extension: 'line.wi-dim-extension',
				dimension: 'line.wi-dim-dimension',
				arrow: 'polygon.wi-dim-arrow',
				value: 'text.wi-dim-value',
				curves: 'g.wi-curves path.wi-curve'
			},
			texts: 'text.wi-dim-value'
		});

		expect(r.warnings).toEqual([]);
		// §7's render list for a linear dimension, item by item.
		expect(r.counts.extension).toBe(2);
		expect(r.counts.dimension).toBe(1);
		expect(r.counts.arrow).toBe(2);
		expect(r.counts.value).toBe(1);
		// The five projected curves of the fixture, all drawn.
		expect(r.counts.curves).toBe(5);
	});

	test('the value text is the measured number at the stated precision', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// 0.04 m = 40 mm. Two decimals as the annotation asks.
		const two = await renderAndQuery(page, { layout: plateLayout([WIDTH_DIMENSION]) }, {
			texts: 'text.wi-dim-value'
		});
		expect(two.texts).toEqual(['40.00']);

		// The annotation's own precision wins over the document's.
		const three = await renderAndQuery(
			page,
			{
				layout: plateLayout([{ ...WIDTH_DIMENSION, precision: 3 }]),
				documentPrecision: 1
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(three.texts).toEqual(['40.000']);

		// With no annotation precision, the document's applies.
		const doc = await renderAndQuery(
			page,
			{
				layout: plateLayout([{ ...WIDTH_DIMENSION, precision: null }]),
				documentPrecision: 0
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(doc.texts).toEqual(['40']);

		// And the display unit converts: 40 mm is 1.575 in.
		const inches = await renderAndQuery(
			page,
			{ layout: plateLayout([{ ...WIDTH_DIMENSION, precision: 3 }]), unit: 'in' },
			{ texts: 'text.wi-dim-value' }
		);
		expect(inches.texts).toEqual(['1.575']);
	});

	test('a dual unit is bracketed after the primary value', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([{ ...WIDTH_DIMENSION, precision: 2, dual_unit: 'in' }]),
				unit: 'mm'
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.texts).toEqual(['40.00 [1.57 in]']);
	});

	test('the dimension line that is drawn is as long as the number that is printed', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// The cross-check between the two halves of the renderer. At 1:1 the
		// drawn dimension line must span 40 paper mm, because the printed
		// value says 40.00 mm — a renderer whose text and geometry came from
		// different places would pass the text assertion and fail this.
		for (const scale of [1, 0.5, 2]) {
			const r = await renderAndQuery(
				page,
				{ layout: plateLayout([WIDTH_DIMENSION]), scale },
				{
					texts: 'text.wi-dim-value',
					attrs: {
						x1: ['line.wi-dim-dimension', 'x1'],
						x2: ['line.wi-dim-dimension', 'x2'],
						y1: ['line.wi-dim-dimension', 'y1'],
						y2: ['line.wi-dim-dimension', 'y2']
					}
				}
			);
			const printedMm = Number(r.texts[0]);
			const drawn = Math.hypot(
				Number(r.attrs.x2[0]) - Number(r.attrs.x1[0]),
				Number(r.attrs.y2[0]) - Number(r.attrs.y1[0])
			);
			expect(printedMm).toBeCloseTo(40, 6);
			expect(drawn).toBeCloseTo(printedMm * scale, 3);
		}
	});

	test('the dimension line clears the part instead of crossing it', async ({ page, waffle }) => {
		await waffle.waitForReady();
		// A witness point is a wall's MIDPOINT, not the part's extreme. Both
		// walls' midpoints sit at mid-height, so offsetting 10 mm from them
		// alone put the dimension line 2.5 mm inside this 40 × 25 mm plate —
		// line, arrowheads and extension lines all on top of the outline. The
		// offset is measured from the view's box, so the line must land clear
		// of the paper box's 0 .. 25 mm.
		const r = await renderAndQuery(
			page,
			{ layout: plateLayout([WIDTH_DIMENSION]) },
			{
				attrs: {
					dimY: ['line.wi-dim-dimension', 'y1'],
					extY: ['line.wi-dim-extension', 'y2'],
					arrows: ['polygon.wi-dim-arrow', 'points']
				}
			}
		);
		const partBottomMm = 25;
		const dimY = Number(r.attrs.dimY[0]);
		expect(dimY).toBeGreaterThanOrEqual(partBottomMm);
		// Every arrowhead vertex, too — the heads are what a reader sees
		// sitting on the outline.
		for (const pts of r.attrs.arrows) {
			for (const v of pts.split(' ')) {
				expect(Number(v.split(',')[1])).toBeGreaterThanOrEqual(partBottomMm);
			}
		}
		// And the extension lines reach past the dimension line, not back
		// into the part.
		for (const y of r.attrs.extY) expect(Number(y)).toBeGreaterThan(dimY);
	});

	test('a radial dimension draws a leader with one arrowhead and an R prefix', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: {
					curves: [edge(HOLE_CIRCLE)],
					bbox: [
						[0.015, 0.0075],
						[0.025, 0.0175]
					],
					annotations: [
						{
							type: 'Dimension',
							kind: { type: 'Radius' },
							anchors: [{ type: 'Curve', curve: HOLE_CIRCLE }],
							value: HOLE_R,
							precision: 2,
							placement: { dx: 0, dy: 0 }
						}
					]
				}
			},
			{
				counts: {
					arrow: 'polygon.wi-dim-arrow',
					leader: 'line.wi-dim-leader',
					dimension: 'line.wi-dim-dimension'
				},
				texts: 'text.wi-dim-value'
			}
		);
		expect(r.warnings).toEqual([]);
		expect(r.counts.arrow).toBe(1);
		expect(r.counts.leader).toBe(1);
		expect(r.counts.dimension).toBe(1);
		expect(r.texts).toEqual(['R5.00']);
	});

	test('a diameter dimension spans the circle and prints the diameter symbol', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: {
					curves: [edge(HOLE_CIRCLE)],
					bbox: [
						[0.015, 0.0075],
						[0.025, 0.0175]
					],
					annotations: [
						{
							type: 'Dimension',
							kind: { type: 'Diameter' },
							anchors: [{ type: 'Curve', curve: HOLE_CIRCLE }],
							value: 2 * HOLE_R,
							precision: 1,
							placement: { dx: 0, dy: 0 }
						}
					]
				}
			},
			{ counts: { arrow: 'polygon.wi-dim-arrow' }, texts: 'text.wi-dim-value' }
		);
		// Two heads: a diameter is dimensioned rim to rim.
		expect(r.counts.arrow).toBe(2);
		expect(r.texts).toEqual(['⌀10.0']);
	});

	test('an angular dimension draws an arc and prints degrees', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: {
					curves: [],
					bbox: [
						[0, 0],
						[0.02, 0.02]
					],
					annotations: [
						{
							type: 'Dimension',
							kind: { type: 'Angle' },
							anchors: [
								{ type: 'Curve', curve: line([0, 0], [0.02, 0]) },
								{ type: 'Curve', curve: line([0, 0], [0, 0.02]) }
							],
							// 90° in radians, as `measure` returns it.
							value: Math.PI / 2,
							precision: 1,
							placement: { dx: 0, dy: 0 }
						}
					]
				}
			},
			{
				counts: { arc: 'path.wi-dim-dimension', arrow: 'polygon.wi-dim-arrow' },
				texts: 'text.wi-dim-value'
			}
		);
		expect(r.warnings).toEqual([]);
		expect(r.counts.arc).toBe(1);
		expect(r.counts.arrow).toBe(2);
		expect(r.texts).toEqual(['90.0°']);
	});

	test('a note with a leader draws a two-segment leader and its text', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{
						type: 'Note',
						text: 'DEBURR ALL EDGES',
						leader: { type: 'Point', at: [0.02, 0.0125] },
						placement: { dx: 0, dy: 0 }
					}
				])
			},
			{
				counts: { leader: 'line.wi-dim-leader', dot: 'circle.wi-dim-leader' },
				texts: 'text.wi-dim-note'
			}
		);
		expect(r.counts.leader).toBe(2);
		// A leader onto a face terminates in a dot, not an arrow (ISO 128-22).
		expect(r.counts.dot).toBe(1);
		expect(r.texts).toEqual(['DEBURR ALL EDGES']);
	});

	test('a centre mark and a centre line draw dash-dotted strokes', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{ type: 'CentreMark', at: [0.02, 0.0125], half_size: HOLE_R },
					{ type: 'CentreLine', from: [0, 0.0125], to: [PLATE_W, 0.0125] }
				])
			},
			{
				counts: { centre: 'line.wi-dim-centre' },
				attrs: { dash: ['line.wi-dim-centre', 'stroke-dasharray'] }
			}
		);
		// Two arms of the cross plus the centre line.
		expect(r.counts.centre).toBe(3);
		for (const d of r.attrs.dash) expect(d).toBe('12 2 2 2');
	});

	test('a datum draws a filled triangle, a box and its letter', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{
						type: 'Datum',
						label: 'A',
						anchor: { type: 'Curve', curve: line([0, 0], [PLATE_W, 0]) },
						placement: { dx: 0, dy: 0 }
					}
				])
			},
			{
				counts: { tri: 'polygon.wi-dim-arrow', box: 'rect.wi-dim-datum' },
				texts: 'text.wi-dim-datum'
			}
		);
		expect(r.counts.tri).toBe(1);
		expect(r.counts.box).toBe(1);
		expect(r.texts).toEqual(['A']);
	});

	test('a hidden curve is dashed and a visible one is not', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{ layout: plateLayout([]) },
			{
				counts: {
					visible: 'path.wi-curve-visible',
					hidden: 'path.wi-curve-hidden'
				},
				attrs: {
					hiddenDash: ['path.wi-curve-hidden', 'stroke-dasharray'],
					visibleDash: ['path.wi-curve-visible', 'stroke-dasharray'],
					hiddenWidth: ['path.wi-curve-hidden', 'stroke-width'],
					visibleWidth: ['path.wi-curve-visible', 'stroke-width']
				}
			}
		);
		expect(r.counts.visible).toBe(4);
		expect(r.counts.hidden).toBe(1);
		expect(r.attrs.hiddenDash).toEqual(['4 2']);
		// A visible outline carries no dash attribute at all.
		expect(r.attrs.visibleDash).toEqual([null, null, null, null]);
		// ISO 128-20 width group 0.5: outlines wide, hidden detail narrower.
		expect(r.attrs.hiddenWidth).toEqual(['0.35']);
		expect(r.attrs.visibleWidth).toEqual(['0.5', '0.5', '0.5', '0.5']);
	});

	test('no coordinate anywhere in the output is NaN or infinite', async ({ page, waffle }) => {
		await waffle.waitForReady();
		// Every annotation kind at once, so one pass covers every code path
		// that can emit a coordinate.
		const r = await renderAndQuery(page, {
			layout: plateLayout([
				WIDTH_DIMENSION,
				{ ...WIDTH_DIMENSION, kind: { type: 'Distance' } },
				{ ...WIDTH_DIMENSION, kind: { type: 'VDistance' } },
				{
					type: 'Dimension',
					kind: { type: 'Ordinate', axis: { type: 'U' } },
					anchors: [{ type: 'Point', at: [0.02, 0.0125] }],
					value: 0.02,
					precision: 2,
					placement: { dx: 0, dy: 0 }
				},
				{
					type: 'Dimension',
					kind: { type: 'Radius' },
					anchors: [{ type: 'Curve', curve: HOLE_CIRCLE }],
					value: HOLE_R,
					precision: 2,
					placement: { dx: 0, dy: 0 }
				},
				{ type: 'CentreMark', at: [0.02, 0.0125], half_size: HOLE_R },
				{ type: 'CentreLine', from: [0, 0.0125], to: [PLATE_W, 0.0125] },
				{
					type: 'Note',
					text: 'N',
					leader: { type: 'Point', at: [0.02, 0.0125] },
					placement: { dx: 0, dy: 0 }
				},
				{
					type: 'Datum',
					label: 'B',
					anchor: { type: 'Point', at: [0, 0] },
					placement: { dx: 0, dy: 0 }
				}
			])
		});
		expect(r.warnings).toEqual([]);
		const bad = r.allNumbers.filter((s) => /\b(NaN|Infinity|-Infinity|undefined)\b/.test(s));
		expect(bad).toEqual([]);
		expect(Number.isFinite(r.widthMm)).toBe(true);
		expect(Number.isFinite(r.heightMm)).toBe(true);
		expect(r.viewBox.split(' ').every((v) => Number.isFinite(Number(v)))).toBe(true);
	});

	test('a non-finite measured value prints a dash rather than NaN', async ({ page, waffle }) => {
		await waffle.waitForReady();
		// `measure` refuses a non-finite value in Rust, so a record carrying
		// one was built by something that did not. The renderer must say so
		// visibly, not print "NaN" where a machinist reads a size.
		const r = await renderAndQuery(
			page,
			{ layout: plateLayout([{ ...WIDTH_DIMENSION, value: null }]) },
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.texts).toEqual(['—']);
		const bad = r.allNumbers.filter((s) => /\bNaN\b/.test(s));
		expect(bad).toEqual([]);
	});

	test("a radial arrowhead sits on an oblique hole's rim, not at its major radius", async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// A circular hole seen obliquely projects to an ellipse whose MAJOR
		// radius is the hole's true radius — which is the number printed, and
		// is not how far the rim is in any other direction. At a 45° leader
		// the rim of this 8 mm × 5.657 mm ellipse is 6.532 mm out
		// (1/√((cos45/8)² + (sin45/5.657)²)), so an arrow placed at 8 mm
		// floats 1.5 mm off the curve it points at.
		const R = 0.008;
		const CENTRE = [0.02, 0.0125];
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{
						type: 'Dimension',
						kind: { type: 'Radius' },
						anchors: [
							{
								type: 'Curve',
								curve: {
									type: 'Ellipse',
									center: CENTRE,
									major_axis: [1, 0],
									major_radius: R,
									minor_radius: R / Math.SQRT2,
									start_param: 0,
									end_param: Math.PI * 2
								}
							}
						],
						value: R,
						precision: 2,
						placement: { dx: 0, dy: 0 }
					}
				])
			},
			{ texts: 'text.wi-dim-value', attrs: { arrow: ['polygon.wi-dim-arrow', 'points'] } }
		);
		// The printed value is still the TRUE radius, 8 mm.
		expect(r.texts).toEqual(['R8.00']);
		// The paper origin is the bbox's top-left, so the centre is here.
		const centreMm = [CENTRE[0] * 1000, (0.025 - CENTRE[1]) * 1000];
		const tip = r.attrs.arrow[0].split(' ')[0].split(',').map(Number);
		const reach = Math.hypot(tip[0] - centreMm[0], tip[1] - centreMm[1]);
		expect(reach).toBeCloseTo(6.532, 2);
	});

	test('an unknown display unit withholds the number instead of converting by one', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// `units.js` returns the value unchanged for a key it does not know,
		// so a 40 mm feature would print "0.04" under a label the reader
		// takes at face value. A length with no known unit has no legible
		// value: it is withheld, and the render says why.
		const r = await renderAndQuery(
			page,
			{ layout: plateLayout([WIDTH_DIMENSION]), unit: 'furlong' },
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.texts).toEqual(['—']);
		expect(r.warnings.join('\n')).toContain('furlong');

		// A bad DUAL unit drops only the bracket; the primary is still right.
		const dual = await renderAndQuery(
			page,
			{ layout: plateLayout([{ ...WIDTH_DIMENSION, dual_unit: 'furlong' }]), unit: 'mm' },
			{ texts: 'text.wi-dim-value' }
		);
		expect(dual.texts).toEqual(['40.00']);
		expect(dual.warnings.join('\n')).toContain('furlong');
	});

	test('the rounding rule is half away from zero, at the stated places', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// ISO 129-1 expects a stated rule. Ours is `toFixed` on the exact
		// binary value of the double AFTER the unit conversion: a
		// representable half rounds away from zero, not to even (40.125 mm ⇒
		// "40.13", 2.5 mm at zero places ⇒ "3" and not "2").
		//
		// The third case is the one worth pinning: as a literal, `1.005`
		// stores a hair BELOW the half and `(1.005).toFixed(2)` is "1.00" —
		// but the value here is 0.001005 m and the ×1000 lands it a hair
		// ABOVE, so the drawing prints "1.01". The thing rounded is the
		// converted double, never the number a user typed in metres.
		const cases = [
			[0.040125, 2, '40.13'],
			[0.0025, 0, '3'],
			[0.001005, 2, '1.01'],
			[-0.0000001, 2, '0.00'] // never "-0.00"
		];
		for (const [value, precision, expected] of cases) {
			const r = await renderAndQuery(
				page,
				{ layout: plateLayout([{ ...WIDTH_DIMENSION, value, precision }]) },
				{ texts: 'text.wi-dim-value' }
			);
			expect(r.texts, `${value} at ${precision} places`).toEqual([expected]);
		}
	});

	test('a style-supplied string cannot break out of the markup', async ({ page, waffle }) => {
		await waffle.waitForReady();
		// `style` is the document-settings seam, so from D4a its strings are
		// document data — and the output goes to `{@html}`. An unescaped `"`
		// in a font family would close the attribute and let the rest be read
		// as markup.
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([WIDTH_DIMENSION]),
				style: { fontFamily: 'x" onload="alert(1)', hiddenDash: ['2" x="y'] }
			},
			{ counts: { injected: '[onload]', texts: 'text' } }
		);
		expect(r.counts.injected).toBe(0);
		expect(r.svg).not.toContain('onload="alert(1)"');
		expect(r.svg).toContain('&quot;');
	});

	test('every paint is a CSS variable, so the sheet follows the theme', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(page, {
			layout: plateLayout([WIDTH_DIMENSION, { type: 'CentreMark', at: [0.02, 0.0125], half_size: HOLE_R }])
		});
		// No hex or rgb() literal may appear in a stroke or fill.
		const literals = r.allNumbers.filter(
			(s) => /^(stroke|fill)=/.test(s) && !/var\(--drawing-/.test(s) && !/=none$/.test(s)
		);
		expect(literals).toEqual([]);
		expect(r.svg).toContain('var(--drawing-ink)');
		expect(r.svg).toContain('var(--drawing-paper)');
		expect(r.svg).toContain('var(--drawing-annotation)');
	});

	test('the renderer is deterministic — the same input gives the same bytes', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// What makes a V3-style byte oracle possible, and what `export_svg`
		// (D4a) will rely on to write the same markup the sheet shows.
		const input = {
			layout: plateLayout([
				WIDTH_DIMENSION,
				{
					type: 'Dimension',
					kind: { type: 'Radius' },
					anchors: [{ type: 'Curve', curve: HOLE_CIRCLE }],
					value: HOLE_R,
					precision: 2,
					placement: { dx: 0.001, dy: -0.002 }
				}
			])
		};
		const a = await render(page, input);
		const b = await render(page, input);
		expect(b.svg).toBe(a.svg);
		expect(a.svg.length).toBeGreaterThan(500);
		// And a changed input changes the bytes — otherwise the check above
		// would pass on a renderer that ignored its argument.
		const c = await render(page, { ...input, scale: 0.5 });
		expect(c.svg).not.toBe(a.svg);

		// The same record with its object keys in a different order is the
		// same record. JSON from the engine carries no key-order guarantee,
		// so a renderer that iterated `Object.entries` anywhere in its
		// geometry would make the byte oracle depend on serialization order.
		const reordered = await render(page, reverseKeys(input));
		expect(reordered.svg).toBe(a.svg);
	});

	test('the architectural-tick option replaces arrowheads with ticks', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{ layout: plateLayout([WIDTH_DIMENSION]), style: { architecturalTicks: true } },
			{ counts: { arrow: 'polygon.wi-dim-arrow', tick: 'line.wi-dim-tick' } }
		);
		expect(r.counts.arrow).toBe(0);
		expect(r.counts.tick).toBe(2);
	});

	test('an unknown annotation kind draws nothing and says so', async ({ page, waffle }) => {
		await waffle.waitForReady();
		// Forward compatibility: a record from a newer build. A placeholder
		// glyph on a manufacturing drawing is worse than a visible absence,
		// so nothing is drawn — but the omission is reported, never silent.
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{ type: 'FeatureControlFrame', anchor: { type: 'Point', at: [0, 0] } }
				])
			},
			{ counts: { ann: 'g.wi-annotations > *' } }
		);
		expect(r.counts.ann).toBe(0);
		expect(r.warnings.length).toBe(1);
		expect(r.warnings[0]).toContain('FeatureControlFrame');
	});

	test('an empty layout still renders a valid, finite sheet', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{ layout: { curves: [], annotations: [] } },
			{ counts: { paper: 'rect.wi-paper', curves: 'path.wi-curve' } }
		);
		expect(r.counts.paper).toBe(1);
		expect(r.counts.curves).toBe(0);
		expect(r.widthMm).toBeGreaterThan(0);
		expect(r.heightMm).toBeGreaterThan(0);
	});
});
