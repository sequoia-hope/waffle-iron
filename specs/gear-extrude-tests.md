# Gear Extrusion Tests

## Purpose

Exercise the `gear_profile()` helper from test-harness through extrude and boolean
pipelines. Tests may fail initially; goal is to have red tests in place for future work.

## Test Cases

| Test | Description | Oracle |
|------|-------------|--------|
| r1 | gear_profile(12, 2.0, 20deg) extruded depth=5 | Non-empty solid |
| r2 | Same geometry | V ~ cross-section * depth |
| r3 | Same geometry | Watertight mesh |
| r4 | gear extrude union with box | Combined volume |
| r5 | box minus gear extrude | Box volume minus gear volume |

## Profile

gear_profile(teeth=12, module=2.0, pressure_angle=20deg) produces an involute gear
outline as a polygon profile suitable for extrusion.
