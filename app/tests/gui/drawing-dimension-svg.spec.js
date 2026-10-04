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
		// FOUR places of inches, not the primary's two. D3 gave both units one
		// precision and this test pinned `[1.57 in]`, which is the defect D3's
		// own notes recorded: two places of inches is 0.254 mm, 25× coarser
		// than the 0.01 mm it restates (ASME Y14.5 §1.6.2). M1 derives the
		// dual's places instead — see "the dual value has its own precision"
		// below for the three sources and their order.
		expect(r.texts).toEqual(['40.00 [1.5748 in]']);
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
		//
		// The stand-in used to be `FeatureControlFrame`, which M1 implements;
		// a kind this build genuinely does not know has to be one no version
		// has (a balloon / item-number callout is the plausible next one).
		const r = await renderAndQuery(
			page,
			{ layout: plateLayout([{ type: 'Balloon', anchor: { type: 'Point', at: [0, 0] } }]) },
			{ counts: { ann: 'g.wi-annotations > *' } }
		);
		expect(r.counts.ann).toBe(0);
		expect(r.warnings.length).toBe(1);
		expect(r.warnings[0]).toContain('Balloon');
	});

	test("a section cap's hole is not hatched, which is what even-odd buys", async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// D4b. The Rust side pins that the kernel reports one outer loop and
		// one `hole: true` loop; this is the half it cannot reach — that the
		// SCANLINE then leaves the hole empty. A count of `line.wi-hatch`
		// cannot answer it: a renderer that ignored `hole` entirely would draw
		// hatch lines straight across the bore and still produce a plausible
		// count.
		//
		// The measurement is a clearance, which is exact and scale-free: no
		// point of any hatch line may fall inside the hole. The hole's
		// boundary reaches the markup as a chord polygon inscribed in the
		// circle, so the lines clear the TRUE circle by up to one sagitta —
		// measured at 6.5 µm on this 6 mm hole, hence a pin at 0.9 r with
		// 0.59 mm of margin rather than a tight one that the chord density
		// would move.
		const R = 0.006;
		const cap = (holed) => ({
			layout: {
				curves: [
					edge(line([-0.015, -0.02], [0.015, -0.02])),
					edge(line([0.015, -0.02], [0.015, 0.02])),
					edge(line([0.015, 0.02], [-0.015, 0.02])),
					edge(line([-0.015, 0.02], [-0.015, -0.02]))
				],
				annotations: [],
				hatch: [
					{
						curves: [
							line([-0.015, -0.02], [0.015, -0.02]),
							line([0.015, -0.02], [0.015, 0.02]),
							line([0.015, 0.02], [-0.015, 0.02]),
							line([-0.015, 0.02], [-0.015, -0.02])
						],
						hole: false
					},
					...(holed
						? [
								{
									curves: [
										{
											type: 'Circle',
											center: [0, 0],
											radius: R,
											start_angle: 0,
											end_angle: 2 * Math.PI
										}
									],
									hole: true
								}
							]
						: [])
				]
			}
		});

		const measure = (input) =>
			page.evaluate((i) => {
				const out = window.__waffle.renderDrawingSvg(i);
				const doc = new DOMParser().parseFromString(out.svg, 'image/svg+xml');
				const lines = Array.from(doc.querySelectorAll('line.wi-hatch')).map((el) =>
					['x1', 'y1', 'x2', 'y2'].map((a) => Number(el.getAttribute(a)))
				);
				// The cap is centred on the drawing, so the hole's centre is
				// the centre of the drawn box in paper space — derived from
				// the hatch's own extent rather than assumed, so the check
				// does not depend on the margin the renderer chose.
				const xs = lines.flatMap(([a, , c]) => [a, c]);
				const ys = lines.flatMap(([, b, , d]) => [b, d]);
				const cx = (Math.min(...xs) + Math.max(...xs)) / 2;
				const cy = (Math.min(...ys) + Math.max(...ys)) / 2;
				let closest = Infinity;
				let total = 0;
				for (const [x1, y1, x2, y2] of lines) {
					total += Math.hypot(x2 - x1, y2 - y1);
					for (let t = 0; t <= 1; t += 0.02) {
						const d = Math.hypot(x1 + (x2 - x1) * t - cx, y1 + (y2 - y1) * t - cy);
						if (d < closest) closest = d;
					}
				}
				return { count: lines.length, total, closest, warnings: out.warnings };
			}, input);

		const solid = await measure(cap(false));
		const holed = await measure(cap(true));
		expect(solid.warnings).toEqual([]);
		expect(holed.warnings).toEqual([]);
		expect(solid.count, 'a solid cap is hatched').toBeGreaterThan(5);

		// The hole removes ink and SPLITS the scanlines that cross it, so the
		// holed cap draws more lines of less total length. Either alone could
		// be met by accident; together they cannot.
		expect(holed.total).toBeLessThan(solid.total);
		expect(holed.count).toBeGreaterThan(solid.count);

		// And the hole itself is empty. This is the assertion that fails if
		// `hole` is ignored, if the even-odd pairing is off by one, or if the
		// half-open crossing test double-counts a vertex at the hole's
		// extremes — the classic bug the half-open test exists to prevent.
		expect(
			holed.closest,
			`a hatch line reached ${holed.closest} mm from the hole centre; the hole is 6 mm`
		).toBeGreaterThan(0.9 * R * 1000);
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

/**
 * M1 — tolerance types, precision, material (`specs/drawings_and_mbd.md` §9).
 *
 * The same discipline as the D3 suite above: the SVG DOM, never pixels, and
 * every number arrives already resolved in the layout record.
 *
 * ## The assertion that matters most
 *
 * `a fit prints the ENGINE resolved band` feeds the renderer a fit whose
 * deviations are deliberately NOT what ISO 286 gives for H7/g6 at ⌀25 (the
 * real union band is +0.021 / −0.020). A JavaScript lookup table would print
 * the real one and fail. That is the only way a test can prove the ABSENCE of
 * a second implementation of what `H7` means.
 */

/** A `ToleranceLayout`, in METRES as the engine emits it. */
function tolerance(type, { deviations = null, limits = null, hole, shaft } = {}) {
	const display = { type };
	if (hole !== undefined) display.hole = hole;
	if (shaft !== undefined) display.shaft = shaft;
	const out = { display };
	if (deviations) out.deviations = deviations;
	if (limits) out.limits = limits;
	return out;
}

/** The plate's width dimension carrying `tol`. */
function toleranced(tol, extra = {}) {
	return { ...WIDTH_DIMENSION, tolerance: tol, ...extra };
}

/** A ⌀25 bore, for the ISO 286 fit cases — a fit needs a nominal SIZE. */
const BORE_R = 0.0125;
const BORE_CIRCLE = {
	type: 'Circle',
	center: [0.02, 0.0125],
	radius: BORE_R,
	start_angle: 0,
	end_angle: Math.PI * 2
};
function boreLayout(annotation) {
	return {
		curves: [edge(BORE_CIRCLE)],
		bbox: [
			[0.0075, 0],
			[0.0325, 0.025]
		],
		annotations: [annotation]
	};
}
function boreDiameter(extra) {
	return {
		type: 'Dimension',
		kind: { type: 'Diameter' },
		anchors: [{ type: 'Curve', curve: BORE_CIRCLE }],
		value: 2 * BORE_R,
		precision: 2,
		placement: { dx: 0, dy: 0 },
		...extra
	};
}

test.describe('M1 tolerance, precision and material', () => {
	test('a symmetric tolerance prints one ± magnitude at the band precision', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					toleranced(
						tolerance('Symmetric', { deviations: [1e-4, -1e-4], limits: [0.0401, 0.0399] })
					)
				])
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		// 0.1 mm is exact at the dimension's own two places, so the band does
		// not widen the precision.
		expect(r.texts).toEqual(['40.00 ±0.10']);
	});

	test('a bilateral tolerance prints both deviations, finer than the dimension', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					toleranced(
						tolerance('Deviations', {
							deviations: [2.1e-5, -5e-6],
							limits: [0.040021, 0.039995]
						})
					)
				])
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		// THE assertion of the band-precision rule: a ±0.021 band under a
		// two-place dimension must not print as ±0.02, which is a different
		// tolerance. And a true minus sign, not a hyphen.
		expect(r.texts).toEqual(['40.00 +0.021 / −0.005']);
	});

	test('a zero deviation prints as a bare 0 (ASME Y14.5 §2.3.2)', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					toleranced(
						tolerance('Deviations', { deviations: [2.1e-5, 0], limits: [0.040021, 0.04] })
					)
				])
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		expect(r.texts).toEqual(['40.00 +0.021 / 0']);
	});

	test('a limits tolerance stacks the two sizes, upper above lower', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					toleranced(
						tolerance('Limits', {
							deviations: [2.1e-5, -5e-6],
							limits: [0.040021, 0.039995]
						})
					)
				])
			},
			{ texts: 'text.wi-dim-value', attrs: { y: ['text.wi-dim-value', 'y'] } }
		);
		expect(r.warnings).toEqual([]);
		// A limit dimension prints the two SIZES and no nominal, both at the
		// precision the finer of them needs.
		expect(r.texts).toEqual(['40.021', '39.995']);
		// And the upper limit reads ABOVE the lower on the paper: SVG y runs
		// down, so the first line's y must be the smaller.
		expect(Number(r.attrs.y[0])).toBeLessThan(Number(r.attrs.y[1]));
	});

	test('a fit prints its class text after the value', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: boreLayout(
					boreDiameter({
						tolerance: tolerance('Fit', {
							hole: 'H7',
							shaft: 'g6',
							deviations: [2.1e-5, -2e-5],
							limits: [0.025021, 0.02498]
						})
					})
				)
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		expect(r.texts).toEqual(['⌀25.00 H7/g6']);
	});

	test('a fit prints the ENGINE resolved band, not an ISO 286 table of its own', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// Deliberately NOT the ISO 286 band for H7/g6 at ⌀25, which is
		// +0.021 / −0.020. A JavaScript lookup table would print that, and
		// this assertion would fail — which is the point: the renderer must
		// have no way to answer "what does H7 mean" for itself.
		const r = await renderAndQuery(
			page,
			{
				layout: boreLayout(
					boreDiameter({
						tolerance: tolerance('Fit', {
							hole: 'H7',
							shaft: 'g6',
							deviations: [1.23e-4, -4.56e-4],
							limits: [0.025123, 0.024544]
						})
					})
				),
				display: { fitBand: true }
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		expect(r.texts).toEqual(['⌀25.00 H7/g6 (+0.123 / −0.456)']);
	});

	test('a single-sided fit prints only the class it has', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: boreLayout(
					boreDiameter({
						tolerance: tolerance('Fit', {
							hole: 'H7',
							deviations: [2.1e-5, 0],
							limits: [0.025021, 0.025]
						})
					})
				)
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		expect(r.texts).toEqual(['⌀25.00 H7']);
	});

	test('a basic dimension is drawn in a box', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{ layout: plateLayout([toleranced(tolerance('Basic'))]) },
			{ counts: { box: 'rect.wi-dim-basic' }, texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		// The value, unchanged — a basic dimension is the theoretically exact
		// size, its variation controlled by a feature control frame instead.
		expect(r.texts).toEqual(['40.00']);
		expect(r.counts.box).toBe(1);
	});

	test('a tolerance form this build does not know still prints the value, and says so', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{ layout: plateLayout([toleranced(tolerance('Statistical'))]) },
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.texts).toEqual(['40.00']);
		expect(r.warnings).toEqual(['a tolerance of display type Statistical was not printed']);
	});

	test('a feature control frame draws its compartments, symbol and datum letters', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{
						type: 'FeatureControlFrame',
						tolerance: {
							characteristic: { type: 'Position' },
							value: { magnitude: 0.0002, dimension: 'Length' },
							modifier: { type: 'Mmc' },
							datums: [{ label: 'A' }, { label: 'B', modifier: { type: 'Mmc' } }],
							zone: { type: 'Diametral' }
						},
						anchor: { type: 'Point', at: [0.02, 0.0125] },
						placement: { dx: 0, dy: 0 }
					}
				])
			},
			{
				counts: {
					cells: 'rect.wi-dim-frame',
					leader: 'line.wi-dim-leader',
					dot: 'circle.wi-dim-leader'
				},
				texts: 'text.wi-dim-frame',
				attrs: {
					x: ['rect.wi-dim-frame', 'x'],
					w: ['rect.wi-dim-frame', 'width'],
					h: ['rect.wi-dim-frame', 'height']
				}
			}
		);
		expect(r.warnings).toEqual([]);
		// Four compartments: symbol | zone + value + modifier | datum A | datum B.
		expect(r.counts.cells).toBe(4);
		expect(r.texts).toEqual(['⌖', '⌀0.20Ⓜ', 'A', 'BⓂ']);
		// A leader with a dot, because the anchor is a POINT (a vertex or a
		// face's representative point) rather than a curve — ISO 128-22.
		expect(r.counts.leader).toBe(2);
		expect(r.counts.dot).toBe(1);
		// The compartments abut, and share one height.
		for (let i = 1; i < 4; i++) {
			expect(Number(r.attrs.x[i])).toBeCloseTo(
				Number(r.attrs.x[i - 1]) + Number(r.attrs.w[i - 1]),
				3
			);
			expect(Number(r.attrs.h[i])).toBeCloseTo(Number(r.attrs.h[0]), 6);
		}
	});

	test('a form characteristic frame has no datum compartment', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{
						type: 'FeatureControlFrame',
						tolerance: {
							characteristic: { type: 'Flatness' },
							value: { magnitude: 5e-5, dimension: 'Length' },
							zone: { type: 'Width' }
						},
						anchor: { type: 'Curve', curve: line([0, 0], [PLATE_W, 0]) },
						placement: { dx: 0, dy: 0 }
					}
				])
			},
			{
				counts: { cells: 'rect.wi-dim-frame', arrow: 'polygon.wi-dim-arrow' },
				texts: 'text.wi-dim-frame'
			}
		);
		expect(r.warnings).toEqual([]);
		expect(r.counts.cells).toBe(2);
		// A width zone prints no prefix, and 0.05 mm is exact at the
		// document's own two places, so the band rule adds none.
		expect(r.texts).toEqual(['⏥', '0.05']);
		// A curve anchor terminates in an arrowhead, not a dot.
		expect(r.counts.arrow).toBe(1);
	});

	test('a frame with no characteristic draws nothing and names it', async ({ page, waffle }) => {
		await waffle.waitForReady();
		// A feature control frame says "this characteristic, within this
		// zone". Without the characteristic there is no control to draw, so
		// the frame is omitted rather than drawn with an empty compartment —
		// the same rule an unknown annotation kind follows. Rust's
		// `GeometricTolerance` cannot be built this way; a record that is
		// came from something that did not go through it.
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{
						type: 'FeatureControlFrame',
						tolerance: { value: { magnitude: 0.0002, dimension: 'Length' } },
						anchor: { type: 'Point', at: [0.02, 0.0125] },
						placement: { dx: 0, dy: 0 }
					}
				])
			},
			{ counts: { ann: 'g.wi-annotations > *' } }
		);
		expect(r.counts.ann).toBe(0);
		expect(r.warnings).toEqual([
			'a geometric tolerance characteristic ? has no symbol',
			'an annotation of type FeatureControlFrame laid out nothing'
		]);
	});

	test('the dual value has its own precision, independent of the primary', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		// DERIVED, nobody having said: two places of millimetres is 0.01 mm,
		// so the inch value needs four to be no coarser (ASME Y14.5 §1.6.2).
		// Printing it at the primary's two places — D3's behaviour — restated
		// a 0.01 mm value to 0.254 mm.
		const derived = await renderAndQuery(
			page,
			{ layout: plateLayout([{ ...WIDTH_DIMENSION, dual_unit: 'in' }]) },
			{ texts: 'text.wi-dim-value' }
		);
		expect(derived.warnings).toEqual([]);
		expect(derived.texts).toEqual(['40.00 [1.5748 in]']);

		// The annotation's own `dual_precision` wins over the derivation.
		const annotated = await renderAndQuery(
			page,
			{ layout: plateLayout([{ ...WIDTH_DIMENSION, dual_unit: 'in', dual_precision: 1 }]) },
			{ texts: 'text.wi-dim-value' }
		);
		expect(annotated.texts).toEqual(['40.00 [1.6 in]']);

		// The document's setting wins over the derivation, and moves neither
		// the primary's places nor the annotation's own three.
		const fromDocument = await renderAndQuery(
			page,
			{
				layout: plateLayout([{ ...WIDTH_DIMENSION, precision: 3, dual_unit: 'in' }]),
				display: { dualPrecision: 2 }
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(fromDocument.texts).toEqual(['40.000 [1.57 in]']);
	});

	test('a null precision is an ABSENCE and not zero decimal places', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();

		// The M1 review's fourth defect. Both precision clamps tested
		// `Number.isFinite(Number(p))`, and `Number(null)` is 0 — which is a
		// LEGAL precision, so an absent setting was indistinguishable from an
		// author asking for whole millimetres. A 40 mm plate printed `40`.
		//
		// `renderDrawingSvg`'s `display` is a public door (the agent link and
		// the console both reach it), so a null arriving here is not
		// hypothetical; and `mirrorSessionDocument` documented the contract
		// that a null means "fall back", three lines above the `??` defaults
		// that were the only thing making it true.
		const nulled = await renderAndQuery(
			page,
			{
				layout: plateLayout([{ ...WIDTH_DIMENSION, precision: null }]),
				display: { precision: null }
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(nulled.texts).toEqual(['40.00']);

		// Zero IS a legal precision, and still means zero — the fix must not
		// have turned the absence check into a falsiness check.
		const zero = await renderAndQuery(
			page,
			{ layout: plateLayout([{ ...WIDTH_DIMENSION, precision: 0 }]) },
			{ texts: 'text.wi-dim-value' }
		);
		expect(zero.texts).toEqual(['40']);

		// And a document-level zero reaches a dimension that names none.
		const zeroDocument = await renderAndQuery(
			page,
			{
				layout: plateLayout([{ ...WIDTH_DIMENSION, precision: null }]),
				display: { precision: 0 }
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(zeroDocument.texts).toEqual(['40']);
	});

	test('a document dual unit applies to an annotation that names none', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([{ ...WIDTH_DIMENSION, dual_unit: null }]),
				display: { dualUnit: 'in' }
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		expect(r.texts).toEqual(['40.00 [1.5748 in]']);
	});

	test('inch values print as whole-plus-fraction when the document asks', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const atInches = async (meters, display) =>
			(
				await renderAndQuery(
					page,
					{
						layout: plateLayout([{ ...WIDTH_DIMENSION, value: meters }]),
						unit: 'in',
						display: { inchFraction: true, ...display }
					},
					{ texts: 'text.wi-dim-value' }
				)
			).texts[0];

		// 1.5 in exactly: whole plus fraction, joined by an ASCII hyphen.
		expect(await atInches(1.5 * 0.0254)).toBe('1-1/2');
		// Under one inch there is no whole part.
		expect(await atInches(0.375 * 0.0254)).toBe('3/8');
		// 8/16 reduces to 1/2 — the denominator is a power of two, so the
		// reduction is by halving.
		expect(await atInches(2.5 * 0.0254)).toBe('2-1/2');
		// THE carry: 15.99/16 rounds to 16/16, which is one whole inch. It
		// must not print `1-16/16` or `0-16/16`.
		expect(await atInches((15.99 / 16) * 0.0254)).toBe('1');
		// A finer denominator resolves what 1/16 cannot …
		expect(await atInches((5 / 64) * 0.0254, { inchDenominator: 64 })).toBe('5/64');
		// … and at 1/16 the same value rounds to the nearest sixteenth.
		expect(await atInches((5 / 64) * 0.0254)).toBe('1/16');
	});

	test('a negative fractional inch carries a true minus, distinct from the separator', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{
						type: 'Dimension',
						kind: { type: 'Ordinate', axis: { type: 'U' } },
						anchors: [{ type: 'Point', at: [-0.0381, 0.0125] }],
						value: -1.5 * 0.0254,
						precision: 2,
						placement: { dx: 0, dy: 0 }
					}
				]),
				unit: 'in',
				display: { inchFraction: true }
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		// U+2212 for the sign, an ASCII hyphen between the whole and the
		// fraction: the two look alike and must not BE alike.
		expect(r.texts).toEqual(['−1-1/2']);
	});

	test('a negative decimal value carries the same true minus', async ({ page, waffle }) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					{
						type: 'Dimension',
						kind: { type: 'Ordinate', axis: { type: 'U' } },
						anchors: [{ type: 'Point', at: [-0.012, 0.0125] }],
						value: -0.012,
						precision: 2,
						placement: { dx: 0, dy: 0 }
					}
				])
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		// U+2212, not `toFixed`'s ASCII hyphen: one minus sign across the
		// module, so the one in a deviation, in a fractional inch and in an
		// ordinate is the same character, and the hyphen keeps its one job of
		// joining `1-1/2`.
		expect(r.texts).toEqual(['−12.00']);
	});

	test('a denominator that is not a drafting fraction falls back to a decimal, loudly', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([{ ...WIDTH_DIMENSION, precision: 3 }]),
				unit: 'in',
				display: { inchFraction: true, inchDenominator: 10 }
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.texts).toEqual(['1.575']);
		expect(r.warnings).toEqual([
			'1/10 is not a drafting fraction — inch values printed as decimals'
		]);
	});

	test('a tolerance band stays a decimal in a fractional-inch document', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					toleranced(
						tolerance('Symmetric', { deviations: [1e-4, -1e-4], limits: [0.0401, 0.0399] }),
						{ value: 1.5 * 0.0254 }
					)
				]),
				unit: 'in',
				display: { inchFraction: true }
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		// The coarsest drafting fraction is 1/64 inch = 0.4 mm, larger than
		// any band a drawing carries: a ±0.1 mm band rounded to a fraction
		// would print as 0. So the nominal is a fraction and the band is not.
		//
		// Six places because that is the cap: ±0.1 mm is 0.003937007874… in,
		// which no decimal count represents exactly, and the rule is "the
		// fewest that do, else the cap". Verbose, and the honest conversion —
		// rounding it to ±0.004 would be a band 2 % wider than the one the
		// engine resolved.
		expect(r.texts).toEqual(['1-1/2 ±0.003937']);
	});

	test('a non-finite tolerance number prints an em dash, never NaN', async ({ page, waffle }) => {
		await waffle.waitForReady();
		// `null` for a deviation is what a record built by something other
		// than `ToleranceLayout::resolve` would carry.
		const r = await renderAndQuery(
			page,
			{
				layout: plateLayout([
					toleranced({ display: { type: 'Deviations' }, deviations: [null, null] })
				])
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.texts).toEqual(['40.00 —']);
		expect(r.warnings).toEqual(['a deviation tolerance carries no finite pair of deviations']);
		// And nothing anywhere in the document is a NaN.
		for (const pair of r.allNumbers) expect(pair).not.toContain('NaN');
	});

	test('an angular tolerance is read in degrees, like its dimension', async ({ page, waffle }) => {
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
							value: Math.PI / 2,
							precision: 1,
							// ±0.5°, in RADIANS — the unit the dimension's own
							// value is in, which is the unit its tolerance shares.
							tolerance: tolerance('Symmetric', {
								deviations: [(0.5 * Math.PI) / 180, (-0.5 * Math.PI) / 180],
								limits: [(90.5 * Math.PI) / 180, (89.5 * Math.PI) / 180]
							}),
							placement: { dx: 0, dy: 0 }
						}
					]
				}
			},
			{ texts: 'text.wi-dim-value' }
		);
		expect(r.warnings).toEqual([]);
		expect(r.texts).toEqual(['90.0° ±0.5']);
	});

	test('a toleranced render is still byte-identical under a key reordering', async ({
		page,
		waffle
	}) => {
		await waffle.waitForReady();
		const input = {
			layout: plateLayout([
				toleranced(
					tolerance('Deviations', {
						deviations: [2.1e-5, -5e-6],
						limits: [0.040021, 0.039995]
					})
				)
			]),
			display: { dualUnit: 'in' }
		};
		const a = await render(page, input);
		const b = await render(page, reverseKeys(input));
		expect(b.svg).toBe(a.svg);
		expect(a.warnings).toEqual([]);
	});
});
