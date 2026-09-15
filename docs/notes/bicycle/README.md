# Bicycle frame + fork via the agent link: handoff

Built by Claude Code over the agent link (MCP) on 2026-09-14/15 in the
browser-local document **"Bike frame"** (storage id `0bbcd408-5833-45ab-8dff-f2fbf3041a1a`).
**The document was most likely wiped** by a tab reload followed by an autosave
of an empty engine tree (failure log F10). Everything needed to rebuild is in
this folder.

Failure log with repros and test ideas: `../agent_bicycle_session_failures_2026_09_14.md` (F1–F10).

## Files

| File | What |
|---|---|
| `recipe.py` → `recipe.json` | Ordered MCP calls (`parameters_set`, `sketch_create`, `feature_add`, `document_save`) that rebuild everything, cuts last, with a save after every cut. `$ref:<name>` placeholders stand for feature ids returned by earlier steps. Each cut has a `note`: WORKED / NOT YET RUN / BLOCKED. |
| `bike_geom.py` | Computes stays, fork, dropouts and cope tools, and **verifies by sampling** that after the planned cuts nothing overlaps, no coped end sticks out, and no cut leaves a fragment. `python3 bike_geom.py [seatstay_offset_mm]` (default 5.5, the only clean value found: 4.0 and 5.0 leave the seatstays overlapping each other). Writes `bike_geom.json`. |
| `bike_geom_ye5.5.json` | The verified output used for the build (origins, normals, depths, plate polygons, crown, steerer). |
| `regions.py` | Extrude regions with vertices EXACTLY on the circle (8 minor arcs; outer CCW, hole CW). Run it directly to print them all. |

## Coordinate frame and geometry

- Meters, **Z up, +X forward**. BB centered at the origin, shell axis along Y.
- Sketch in-plane axes (`waffle-types/src/sketch_plane.rs`): ref = Z unless the
  normal ≈ ±Z (then X); x = ref × n, y = n × x. A **+Y normal gives u = −X,
  v = +Z**; a normal −h (crown) gives u = +Y, v = f.
- Head tube: bottom (0.446016, 0, 0.388637), axis h = (−0.292372, 0, 0.956305)
  normalized (73° head angle), 150 mm, OD 44 / wall 1.5.
- Top tube: from (−0.150531, 0, 0.508175) along +X for 560 mm (runs seat-tube axis → head-tube axis), OD 25.4 / 0.9.
- Down tube: origin along (0.718286, 0, 0.695747) normalized, 606.698 mm (ends on the head-tube axis), OD 31.8 / 0.9.
- Seat tube: origin along (−0.284015, 0, 0.95882) normalized, 560 mm, OD 28.6 / 0.9 (top protrudes above the top tube).
- BB shell: y −34…+34 mm, OD 40 / wall 2.5.
- Rear axle (−0.40398, ±0.0724…, 0.07): BB drop 70, chainstay 410 center to center, 130 mm OLD.
  - Chainstays: OD 22.2 / 0.8, start (0, ±0.02, 0).
  - Seatstays: OD 16 / 0.8, meet the seat tube axis at t = 0.5 with y = ±5.5 mm.
  - Stays end 25 mm from the axle, and their centerlines cross the dropout plate mid-thickness 35 mm from the axle.
- Rear dropouts: 6 mm plates, inner faces at |y| = 65 mm, vertical slot 10 mm wide.
- Fork (all positions computed in `bike_geom.py`):
  - Crown race seat C = HT bottom − 12 mm·h.
  - Axle-to-crown **335 mm, chosen so the front axle is level with the rear**. A typical 370 mm fork would drop the front axle 33 mm; the head tube sits low for 700c.
  - Rake 45 mm, front axle (0.590489, 0, 0.07), **wheelbase 994.5 mm**.
  - Crown: stadium, centers ±40 mm, r 16, 20 mm thick along −h.
  - Steerer: OD 28.6 / 1.6, 220 mm up from C.
  - Blades: straight and raked, OD 22 / 1.0, from mid-crown (y ±40 mm) to the dropouts (inner faces |y| = 50 mm, 100 mm OLD).
- Parameters (mm) drive radii and some depths only. **Positions and angles are frozen numbers.**

## Cut plan (copes and slots) and results

Copes are tool extrudes: `combine: Cut` with explicit `targets` (Solid GeomRef
→ `FeatureOutput{feature_id, output_key}`). Tools reuse sketches of solid circles
or plates. **A Cut's outputs are `/Main`, `/Body:1`, … in target order.**

| Cut | Targets | Result |
|---|---|---|
| Head-tube tool (r 22, symmetric 200 mm per side about HT mid) | top tube, down tube | **Worked** |
| BB tool (r 20, Y, symmetric 45 mm) | seat tube | **Worked** |
| BB tool | down tube (**must target both outputs of the head-tube cut**) | **Worked**; single-target silently deleted the top tube (F9) |
| BB tool | chainstay L; then chainstay R (separate features) | **Worked** |
| Rear plate L | chainstay L, seatstay L | **Worked** |
| Rear plate R | chainstay R, seatstay R | not run (tab reloaded) |
| DT tool (r 15.9, 100 mm from origin) | seat tube (ST/DT overlap just above the shell) | not run |
| Front plates | blade L; blade R | not run |
| Crown (stadium) | both blades | not run |
| Seat-tube tool (r 14.3, 0…600 mm) | top tube rear, both seatstay tops | **BLOCKED (F8)**: top tube → `TessellationFailed`; fresh seatstay → Yang Stage-4 `LocalRefinementRequired` |

Head tube, BB shell and steerer are "through" members and are never cut. The
steerer sits in the head-tube bore (radial clearance 6.2 mm) and abuts the crown top.

## Traps (all hit this session)

1. **Normalize every sketch normal to full double precision.** 6-decimal normals are
   stored verbatim: whole-circle extrudes fail `ProfileCircleFrameNotOrthonormal`, and
   region extrudes build circles scaled by |n| that every boolean later rejects (F1/F2).
   Axis-exact normals are fine.
2. Annulus tubes are sub-regions (`profile_entity_ids: null`), so pass an explicit
   `region` with `boundary_entity_ids: [1,2]`. The rebuild re-derives the vertices from
   the sketch, so vertex precision in the payload does not matter; the normal does.
3. **F9:** never cut only some outputs of a multi-output Cut. The siblings vanish silently.
4. **F7/F4/F10:** after the first cylinder cut, every rebuild takes minutes. Parallel
   heavy calls dropped the tab 3×; the last reload came back EMPTY and autosave overwrote
   the stored doc. Run **one boolean per call**, `document_save` after each verified cut,
   and back up `feature_get` of every feature to disk periodically.
5. View names assume Y-up: `"top"` = side profile of this Z-up model (upside down).
6. After a Cut, re-apply body names (a name can follow the `Main` slot to the wrong body).

## Next steps

1. Confirm what the stored "Bike frame" holds (open it on desktop). If empty, rebuild
   from `recipe.json`. Consider fixing F10 (autosave during restore) and F9 (per-output
   consumption) first; both are small, engine/app-side, and make the rebuild safe.
2. Finish the NOT YET RUN cuts one at a time.
3. Seat cluster (F8): kernel work (Yang Stage-4 relocation / tessellation) or a
   different joint design (e.g. seatstays joining the seat tube lower or wrapping to the
   rear face) re-verified with `bike_geom.py`.
4. After that: wheels, seatpost/saddle, cranks, bars.
