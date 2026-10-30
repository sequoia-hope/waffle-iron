// Icon registry for toolbar and menu buttons (ui-chrome M13,
// `specs/ui_toolbar_icons.md`). Each value is the INNER markup of a
// `0 0 24 24` SVG rendered by `Icon.svelte`, which sets stroke = currentColor,
// stroke-width 2, round caps/joins and fill none. Colour comes ONLY from
// `currentColor` (enforced by `scripts/check-icons.mjs`), so every theme and
// button state (active, disabled) recolours the icons for free.
//
// Classes on child elements (styled in `Icon.svelte`):
//   (none)  the created / primary entity — full stroke
//   .c      construction or reference geometry — dashed, faded
//   .axis   an axis or mirror line — dash-dot, faded
//   .p      a point — filled dot
//   .d      a dimension or annotation stroke — thin
//   .a      an arrowhead — filled, no stroke
//   .f      a shaded face or overlap — translucent fill
//
// GENERIC icons below are copied from Lucide 1.54.0 (lucide-static), under
// its ISC licence, names unchanged:
//
//   ISC License — Copyright (c) 2026 Lucide Icons and Contributors.
//   Permission to use, copy, modify, and/or distribute this software for any
//   purpose with or without fee is hereby granted, provided that the above
//   copyright notice and this permission notice appear in all copies.
//   THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
//   WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
//   MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
//   ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
//   WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
//   ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR
//   IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
//
// CAD icons further down are drawn for Waffle Iron on Lucide's grid rules
// (24 grid, 2-unit margin, 2 px stroke).

/** @type {Record<string, string>} */
export const ICONS = {
	// ---- generic (Lucide)
	'house': '<path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"/><path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
	'save': '<path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"/><path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7"/><path d="M7 3v4a1 1 0 0 0 1 1h7"/>',
	'folder-open': '<path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2"/>',
	'download': '<path d="M12 15V3"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/>',
	'undo-2': '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11"/>',
	'redo-2': '<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5A5.5 5.5 0 0 0 4 14.5A5.5 5.5 0 0 0 9.5 20H13"/>',
	'settings': '<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/><circle cx="12" cy="12" r="3"/>',
	'ellipsis-vertical': '<circle cx="12" cy="12" r="1"/><circle cx="12" cy="5" r="1"/><circle cx="12" cy="19" r="1"/>',
	'chevron-down': '<path d="m6 9 6 6 6-6"/>',
	'chevrons-right': '<path d="m6 17 5-5-5-5"/><path d="m13 17 5-5-5-5"/>',
	'flask-conical': '<path d="M14 2v6a2 2 0 0 0 .245.96l5.51 10.08A2 2 0 0 1 18 22H6a2 2 0 0 1-1.755-2.96l5.51-10.08A2 2 0 0 0 10 8V2"/><path d="M6.453 15h11.094"/><path d="M8.5 2h7"/>',
	'book-open': '<path d="M12 5v16"/><path d="M20.001 19A2 2 0 0022 17V5a2 2 0 00-1.999-2L16 3.002A5 5 0 0012 5a5 5 0 00-4-2H4a2 2 0 00-2 2v12a2 2 0 001.999 2H8a5 5 0 014 2 5 5 0 014-2z"/>',
	'microscope': '<path d="M6 18h8"/><path d="M3 22h18"/><path d="M14 22a7 7 0 1 0 0-14h-1"/><path d="M9 14h2"/><path d="M9 12a2 2 0 0 1-2-2V6h6v4a2 2 0 0 1-2 2Z"/><path d="M12 6V3a1 1 0 0 0-1-1H9a1 1 0 0 0-1 1v3"/>',
	'bug': '<path d="M12 20v-9"/><path d="M14 7a4 4 0 0 1 4 4v3a6 6 0 0 1-12 0v-3a4 4 0 0 1 4-4z"/><path d="M14.12 3.88 16 2"/><path d="M21 21a4 4 0 0 0-3.81-4"/><path d="M21 5a4 4 0 0 1-3.55 3.97"/><path d="M22 13h-4"/><path d="M3 21a4 4 0 0 1 3.81-4"/><path d="M3 5a4 4 0 0 0 3.55 3.97"/><path d="M6 13H2"/><path d="m8 2 1.88 1.88"/><path d="M9 7.13V6a3 3 0 1 1 6 0v1.13"/>',
	'info': '<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>',
	'palette': '<path d="M12 22a1 1 0 0 1 0-20 10 9 0 0 1 10 9 5 5 0 0 1-5 5h-2.25a1.75 1.75 0 0 0-1.4 2.8l.3.4a1.75 1.75 0 0 1-1.4 2.8z"/><circle cx="13.5" cy="6.5" r=".5" fill="currentColor"/><circle cx="17.5" cy="10.5" r=".5" fill="currentColor"/><circle cx="6.5" cy="12.5" r=".5" fill="currentColor"/><circle cx="8.5" cy="7.5" r=".5" fill="currentColor"/>',
	'rotate-cw': '<path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/>',
	'mouse-pointer-2': '<path d="M4.037 4.688a.495.495 0 0 1 .651-.651l16 6.5a.5.5 0 0 1-.063.947l-6.124 1.58a2 2 0 0 0-1.438 1.435l-1.579 6.126a.5.5 0 0 1-.947.063z"/>',
	'eye': '<path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0"/><circle cx="12" cy="12" r="3"/>',
	'eye-off': '<path d="M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49"/><path d="M14.084 14.158a3 3 0 0 1-4.242-4.242"/><path d="M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143"/><path d="m2 2 20 20"/>',
	'trash-2': '<path d="M10 11v6"/><path d="M14 11v6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>',
	'pencil': '<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/><path d="m15 5 4 4"/>',
	'maximize': '<path d="M8 3H5a2 2 0 0 0-2 2v3"/><path d="M21 8V5a2 2 0 0 0-2-2h-3"/><path d="M3 16v3a2 2 0 0 0 2 2h3"/><path d="M16 21h3a2 2 0 0 0 2-2v-3"/>',
	'check': '<path d="M20 6 9 17l-5-5"/>',
	'scissors': '<circle cx="6" cy="6" r="3"/><path d="M8.12 8.12 12 12"/><path d="M20 4 8.12 15.88"/><circle cx="6" cy="18" r="3"/><path d="M14.8 14.8 20 20"/>',
	'text-cursor-input': '<path d="M12 20h-1a2 2 0 0 1-2-2 2 2 0 0 1-2 2H6"/><path d="M13 8h7a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2h-7"/><path d="M5 16H4a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2h1"/><path d="M6 4h1a2 2 0 0 1 2 2 2 2 0 0 1 2-2h1"/><path d="M9 6v12"/>',
	'cog': '<path d="M11 10.27 7 3.34"/><path d="m11 13.73-4 6.93"/><path d="M12 22v-2"/><path d="M12 2v2"/><path d="M14 12h8"/><path d="m17 20.66-1-1.73"/><path d="m17 3.34-1 1.73"/><path d="M2 12h2"/><path d="m20.66 17-1.73-1"/><path d="m20.66 7-1.73 1"/><path d="m3.34 17 1.73-1"/><path d="m3.34 7 1.73 1"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="12" r="8"/>',
	'link': '<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>',
	'box': '<path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"/><path d="m3.3 7 8.7 5 8.7-5"/><path d="M12 22V12"/>',
	'sketch': '<path class="c" d="M3 9h11v12H3z"/><path d="M15 3.5l3.5 3.5L10 15.5H6.5V12z"/>',
	'point': '<path class="c" d="M12 3v5M12 16v5M3 12h5M16 12h5"/><circle class="p" cx="12" cy="12" r="2.5"/>',
	'line': '<path d="M5 19L19 5"/><circle class="p" cx="5" cy="19" r="2"/><circle class="p" cx="19" cy="5" r="2"/>',
	'polyline': '<path d="M3 18l6-10 6 8 6-11"/><circle class="p" cx="3" cy="18" r="1.75"/><circle class="p" cx="9" cy="8" r="1.75"/><circle class="p" cx="15" cy="16" r="1.75"/><circle class="p" cx="21" cy="5" r="1.75"/>',
	'rectangle': '<rect x="4" y="6" width="16" height="12"/><circle class="p" cx="4" cy="18" r="2"/><circle class="p" cx="20" cy="6" r="2"/>',
	'rectangle-center': '<rect x="4" y="6" width="16" height="12"/><path class="c" d="M12 12l8-6"/><circle class="p" cx="12" cy="12" r="2"/><circle class="p" cx="20" cy="6" r="2"/>',
	'circle': '<circle cx="12" cy="12" r="8"/><path class="c" d="M12 12l5.66-5.66"/><circle class="p" cx="12" cy="12" r="1.75"/><circle class="p" cx="17.66" cy="6.34" r="1.75"/>',
	'arc': '<path d="M4 17a8 8 0 0 1 16 0"/><path class="c" d="M4 17h16"/><circle class="p" cx="4" cy="17" r="1.75"/><circle class="p" cx="20" cy="17" r="1.75"/><circle class="p" cx="12" cy="17" r="1.75"/>',
	'project': '<path class="c" d="M4 3h11l4 3H8z"/><path d="M12 8v7"/><path d="M9 12.5l3 3 3-3"/><path d="M5 20h14"/><circle class="p" cx="5" cy="20" r="1.75"/><circle class="p" cx="19" cy="20" r="1.75"/>',
	'offset': '<rect class="c" x="7" y="7" width="10" height="10" rx="2"/><rect x="3" y="3" width="18" height="18" rx="5"/>',
	'slot': '<path d="M8 7h8a5 5 0 0 1 0 10H8A5 5 0 0 1 8 7z"/><path class="c" d="M8 12h8"/><circle class="p" cx="8" cy="12" r="1.5"/><circle class="p" cx="16" cy="12" r="1.5"/>',
	'gear': '<path d="M11 10.27 7 3.34"/><path d="m11 13.73-4 6.93"/><path d="M12 22v-2"/><path d="M12 2v2"/><path d="M14 12h8"/><path d="m17 20.66-1-1.73"/><path d="m17 3.34-1 1.73"/><path d="M2 12h2"/><path d="m20.66 17-1.73-1"/><path d="m20.66 7-1.73 1"/><path d="m3.34 17 1.73-1"/><path d="m3.34 7 1.73 1"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="12" r="8"/>',
	'sprocket': '<circle cx="12" cy="12" r="6"/><circle cx="12" cy="12" r="2"/><path d="M12 2l1.5 4h-3zM12 22l-1.5-4h3zM2 12l4-1.5v3zM22 12l-4 1.5v-3zM4.9 4.9l3.9 1.8-2.1 2.1zM19.1 19.1l-3.9-1.8 2.1-2.1zM4.9 19.1l1.8-3.9 2.1 2.1zM19.1 4.9l-1.8 3.9-2.1-2.1z"/>',
	'planetary': '<circle cx="12" cy="12" r="9.5"/><circle cx="12" cy="12" r="2.5"/><circle cx="12" cy="6.5" r="2.5"/><circle cx="12" cy="17.5" r="2.5"/>',
	'trim': '<path d="M3 12h18"/><path d="M12 3v9"/><path class="c" d="M12 12v9"/><path d="M16 16l4 4M20 16l-4 4"/>',
	'sketch-fillet': '<path class="c" d="M5 19V5h14"/><path d="M5 19v-6a8 8 0 0 1 8-8h6"/>',
	'construction': '<path class="c" d="M5 19L19 5"/><circle class="p" cx="5" cy="19" r="2"/><circle class="p" cx="19" cy="5" r="2"/>',
	'dimension': '<path class="d" d="M4 6v12M20 6v12M5 12h4M15 12h4"/><path class="a" d="M4 12l3.5-2.2v4.4zM20 12l-3.5-2.2v4.4z"/>',
	'constraint-horizontal': '<path d="M4 12h16"/><circle class="p" cx="4" cy="12" r="2"/><circle class="p" cx="20" cy="12" r="2"/><path class="c" d="M4 18h16"/>',
	'constraint-vertical': '<path d="M12 4v16"/><circle class="p" cx="12" cy="4" r="2"/><circle class="p" cx="12" cy="20" r="2"/><path class="c" d="M18 4v16"/>',
	'constraint-coincident': '<path d="M3 20l9-8"/><path d="M12 12l9 3"/><circle class="d" cx="12" cy="12" r="4.5"/><circle class="p" cx="12" cy="12" r="2"/>',
	'constraint-perpendicular': '<path d="M5 4v15h15"/><path class="d" d="M5 14h5v5"/>',
	'constraint-parallel': '<path d="M4 18L14 4"/><path d="M10 20L20 6"/>',
	'constraint-equal': '<path d="M4 8h16"/><path d="M4 16h16"/><path class="d" d="M11 5v6M13 5v6M11 13v6M13 13v6"/>',
	'constraint-tangent': '<circle cx="12" cy="14" r="6"/><path d="M3 8h18"/><circle class="p" cx="12" cy="8" r="1.75"/>',
	'constraint-midpoint': '<path d="M3 12h18"/><path class="c" d="M3 7v10M21 7v10"/><circle cx="12" cy="12" r="2.5"/>',
	'constraint-fix': '<path d="M12 14v5"/><path d="M8.5 4h7l-1 5 3 3v2h-11v-2l3-3z"/><circle class="p" cx="12" cy="20" r="1.75"/>',
	'constraint-angle': '<path d="M4 20L18 6"/><path d="M4 20h16"/><path class="d" d="M14 20a10 10 0 0 0-2.9-7.1"/>',
	'constraint-symmetric-h': '<path class="axis" d="M3 12h18"/><circle class="p" cx="12" cy="5" r="2"/><circle class="p" cx="12" cy="19" r="2"/><path class="d" d="M12 8v2M12 14v2"/>',
	'constraint-symmetric-v': '<path class="axis" d="M12 3v18"/><circle class="p" cx="5" cy="12" r="2"/><circle class="p" cx="19" cy="12" r="2"/><path class="d" d="M8 12h2M14 12h2"/>',
	'constraint-symmetric': '<path class="axis" d="M5 19L19 5"/><circle class="p" cx="6" cy="7" r="2"/><circle class="p" cx="17" cy="18" r="2"/>',
	'constraint-point-on-line': '<path d="M3 18L21 6"/><circle class="p" cx="12" cy="12" r="2.5"/>',
	'constraint-point-on-circle': '<circle cx="12" cy="12" r="8"/><circle class="p" cx="17.66" cy="6.34" r="2.5"/>',
	'constraint-equal-radius': '<circle cx="7" cy="12" r="4.5"/><circle cx="17" cy="12" r="4.5"/><path class="d" d="M10 21h4M10 3h4"/>',
	'constraint-length-ratio': '<path d="M3 8h8"/><path d="M3 16h18"/><path class="c" d="M11 5v6M21 13v6"/>',
	'constraint-h-distance': '<circle class="p" cx="5" cy="18" r="1.75"/><circle class="p" cx="19" cy="18" r="1.75"/><path class="d" d="M5 15V5M19 15V5M7 9h10"/><path class="a" d="M5 9l3.5-2.2v4.4zM19 9l-3.5-2.2v4.4z"/>',
	'constraint-v-distance': '<circle class="p" cx="18" cy="5" r="1.75"/><circle class="p" cx="18" cy="19" r="1.75"/><path class="d" d="M15 5H5M15 19H5M9 7v10"/><path class="a" d="M9 5l-2.2 3.5h4.4zM9 19l-2.2-3.5h4.4z"/>',
	'extrude': '<path class="c" d="M3 19l3-3h12l-3 3z"/><path d="M3 10l3-3h12l-3 3z"/><path d="M3 10v9M15 10v9M18 7v9"/>',
	'revolve': '<path class="axis" d="M6 2v20"/><path d="M6 8h5v8H6"/><path d="M15 5.5a7 7 0 0 1 0 13"/><path class="a" d="M14 19.5l2.5-3.3 1 3.6z"/>',
	'pipe': '<path d="M4 19c0-7 6-7 9-7s7 0 7-7" stroke-width="5" stroke-opacity=".3"/><path d="M4 19c0-7 6-7 9-7s7 0 7-7"/><ellipse cx="20" cy="5" rx="2.5" ry="1.2"/>',
	'sweep': '<path class="c" d="M4 20c0-8 8-8 8-8s8 0 8-8"/><rect x="2" y="16" width="5" height="5"/><rect x="17.5" y="2" width="5" height="5"/>',
	'loft': '<rect x="4" y="15" width="16" height="5"/><ellipse cx="12" cy="5" rx="5" ry="2"/><path d="M4 15L7 5M20 15L17 5"/>',
	'script': '<path d="M8 4H6a2 2 0 0 0-2 2v4l-2 2 2 2v4a2 2 0 0 0 2 2h2"/><path d="M16 4h2a2 2 0 0 1 2 2v4l2 2-2 2v4a2 2 0 0 1-2 2h-2"/><path d="M12 8.5l3 1.5v4l-3 1.5-3-1.5v-4z"/>',
	'fillet': '<path class="c" d="M4 20V4h16"/><path d="M4 20v-8a8 8 0 0 1 8-8h8"/><path d="M8 20v-7a5 5 0 0 1 5-5h7"/>',
	'chamfer': '<path class="c" d="M4 20V4h16"/><path d="M4 20v-9l7-7h9"/><path d="M8 20v-7.5L12.5 8H20"/>',
	'shell': '<path d="M3 8v10l9 4 9-4V8"/><path d="M6 9.5v7l6 2.7 6-2.7v-7"/><path d="M3 8l3 1.5M21 8l-3 1.5M12 22v-2.8"/>',
	'boolean': '<rect x="3" y="3" width="12" height="12"/><rect x="9" y="9" width="12" height="12"/><path class="f" d="M9 9h6v6H9z"/>',
	'datum-plane': '<path d="M3 18l4-6h14l-4 6z"/><path d="M12 15V4"/><path class="a" d="M12 2.5l-2.2 3.5h4.4z"/>',
	'mate-connector': '<circle cx="12" cy="12" r="9"/><path d="M12 12V5M12 12l6 3.5M12 12l-6 3.5"/><circle class="p" cx="12" cy="12" r="1.75"/>',
	'import-step': '<path d="M12 2v6M9.5 5.5L12 8l2.5-2.5"/><path d="M4 13l8-4 8 4v6l-8 4-8-4z"/><path d="M4 13l8 4 8-4M12 17v6"/>',
	'import-link': '<path d="M4 9l8-4 8 4v8l-8 4-8-4z"/><path d="M4 9l8 4 8-4M12 13v8"/><path d="M15.5 4.5l1-1a2.1 2.1 0 0 1 3 3l-1 1M18.5 7.5l-2 2a2.1 2.1 0 0 1-3-3"/>',
	'import-kicad': '<rect x="3" y="10" width="18" height="11" rx="1"/><circle cx="7" cy="15.5" r="1.5"/><circle cx="17" cy="17" r="1.5"/><path d="M8.5 15.5h3.5l3.5 1.5"/><path d="M12 2v5M9.5 4.5L12 7l2.5-2.5"/>',
	'link-kicad': '<rect x="3" y="10" width="18" height="11" rx="1"/><circle cx="7" cy="15.5" r="1.5"/><circle cx="17" cy="17" r="1.5"/><path d="M8.5 15.5h3.5l3.5 1.5"/><path d="M15.5 4.5l1-1a2.1 2.1 0 0 1 3 3l-1 1M18.5 7.5l-2 2a2.1 2.1 0 0 1-3-3"/>',
	'pattern-linear': '<rect x="2" y="9" width="6" height="6"/><rect class="c" x="9" y="9" width="6" height="6"/><rect class="c" x="16" y="9" width="6" height="6"/>',
	'pattern-circular': '<circle class="axis" cx="12" cy="12" r="7"/><circle cx="12" cy="5" r="2.5"/><circle class="c" cx="18.1" cy="15.5" r="2.5"/><circle class="c" cx="5.9" cy="15.5" r="2.5"/>',
	'mirror': '<path class="axis" d="M12 3v18"/><path d="M9 6L3 18h6z"/><path class="c" d="M15 6l6 12h-6z"/>',
	'toggle-planes': '<path d="M3 16l4-4h14l-4 4z"/><path d="M8 3h8v13H8z"/>',
	'toggle-axes': '<path d="M6 18V5M6 18h13M6 18l-3.5 3.5"/><path class="a" d="M6 3l-2.2 3.5h4.4zM21 18l-3.5-2.2v4.4z"/>',
	'section': '<path d="M4 9l8-4 8 4v8l-8 4-8-4z"/><path d="M4 9l8 4 8-4M12 13v8"/><path class="f" d="M4 9l8 4v8l-8-4z"/>',
	'section-flip': '<path d="M12 3v18"/><path d="M5 10a7 7 0 0 1 14 0"/><path class="a" d="M19 12.5l-2.2-3.5h4.4zM5 12.5L2.8 9h4.4z"/>',
	'section-clear': '<path d="M4 9l8-4 8 4v8l-8 4-8-4z"/><path d="M4 9l8 4 8-4M12 13v8"/><path d="M16 2l5 5M21 2l-5 5"/>',
	'wireframe': '<path d="M4 8l8-4 8 4v8l-8 4-8-4z"/><path d="M12 12v8"/><path d="M4 8l8 4 8-4"/><path class="c" d="M4 16l8-4 8 4M12 4v8"/>',
	'export-waffle': '<path d="M12 3v9M8.5 8.5L12 12l3.5-3.5"/><rect x="4" y="15" width="16" height="6" rx="1"/><path d="M9.3 15v6M14.7 15v6"/>',
	'export-stl': '<path d="M6 2h8l5 5v15H6z"/><path d="M14 2v5h5"/><path d="M8.5 19l3.5-7 3.5 7z"/>',
	'export-step': '<path d="M6 2h8l5 5v15H6z"/><path d="M14 2v5h5"/><path d="M9 14l3-1.5 3 1.5v3.5L12 19l-3-1.5z"/>',
	'place-view': '<rect x="2" y="4" width="20" height="16" rx="1"/><rect class="c" x="5" y="7" width="7" height="6"/><path d="M17 11v6M14 14h6"/>',
	'project-view': '<rect x="2" y="7" width="8" height="8"/><rect x="14" y="7" width="8" height="8"/><path class="c" d="M10 9h4M10 13h4"/>',
	'dim-distance': '<circle class="p" cx="4" cy="20" r="1.75"/><circle class="p" cx="20" cy="4" r="1.75"/><path class="d" d="M3 15L15 3M9 21L21 9M7.5 15.5l8-8"/><path class="a" d="M6 17l1.5-4 2.5 2.5zM17 6l-1.5 4L13 7.5z"/>',
	'dim-pointline': '<path d="M3 20h18"/><circle class="p" cx="12" cy="4" r="2"/><path class="d" d="M12 8v9"/><path class="a" d="M12 7l-2.2 3.5h4.4zM12 19l-2.2-3.5h4.4z"/>',
	'dim-angle': '<path d="M4 20L18 6"/><path d="M4 20h16"/><path class="d" d="M14 20a10 10 0 0 0-2.9-7.1"/><path class="a" d="M14 19.5l-2-3h4zM11.1 12.9l3.4.8-2.4 2.4z"/>',
	'dim-radius': '<circle cx="12" cy="12" r="9"/><path class="d" d="M12 12l5-5"/><path class="a" d="M18.4 5.6l-1 3.9-2.9-2.9z"/><circle class="p" cx="12" cy="12" r="1.5"/>',
	'dim-diameter': '<circle cx="12" cy="12" r="9"/><path class="d" d="M7.5 16.5l9-9"/><path class="a" d="M18.4 5.6l-1 3.9-2.9-2.9zM5.6 18.4l1-3.9 2.9 2.9z"/>',
	'note': '<rect x="9" y="3" width="12" height="9" rx="1"/><path d="M12 7.5h6"/><path class="d" d="M9 12l-4.5 6.5"/><path class="a" d="M3 21l.5-4 3 2z"/>',
	'datum': '<rect x="12" y="3" width="9" height="9"/><path class="d" d="M14.5 10l2-5 2 5M15.3 8.3h2.4"/><path d="M12 12l-4 4"/><path class="a" d="M3 21l2.5-7 4.5 4.5z"/>',
};

/** Alternative names that share a glyph. @type {Record<string, string>} */
const ALIASES = {
	'dim-hdistance': 'constraint-h-distance',
	'dim-vdistance': 'constraint-v-distance',
	'select': 'mouse-pointer-2',
};

/**
 * The inner SVG markup for `name`, following aliases, or `undefined`.
 * @param {string} name
 */
export function iconMarkup(name) {
	return ICONS[name] ?? ICONS[ALIASES[name]];
}

/** Every name `iconMarkup` resolves (registry and aliases). */
export const ICON_NAMES = [...Object.keys(ICONS), ...Object.keys(ALIASES)];
