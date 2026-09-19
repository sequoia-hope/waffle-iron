// @feature name="Spur gear" version=1
// @param tooth_count: int = 20 min=3 max=400
// @param module_m: length = 0.002
// @param pressure_angle_deg: angle = 20
// @param backlash: length = 0
// @param center_x: length = 0
// @param center_y: length = 0
// @param rotation_offset: number = 0
// @param internal: bool = false
// @param face_width: length = 0.01
// @param plane: plane
// @output body: main
//
// The involute spur gear, as a script. `gear_sketch` is a line-for-line
// port of `waffle_types::gear::generate_gear_profile` (external gears):
// the same point order, the same entity-id sequence (one counter across
// points and curves), and the same floating-point expression order, so
// its entities and positions are bit-identical to the built-in generator's
// (the A-M2 parity oracle). An internal (ring) gear defers to the built-in
// `gear` entity — its profile is a hole to be combined with a user rim.

fn involute(a) {
    tan(a) - a
}

fn involute_point(base_r, roll) {
    [base_r * (cos(roll) + roll * sin(roll)), base_r * (sin(roll) - roll * cos(roll))]
}

fn tf(cx, cy, ca, sa, x, y) {
    [cx + x * ca - y * sa, cy + x * sa + y * ca]
}

fn fmax(a, b) {
    if a > b { a } else { b }
}

// Draw the external gear profile into `sk`. Returns the center point id.
fn gear_sketch(sk, p) {
    let n = p.tooth_count;
    let alpha = to_radians(p.pressure_angle_deg);
    let pitch_r = n.to_float() * p.module_m / 2.0;
    let base_r = pitch_r * cos(alpha);
    let addendum_r = pitch_r + p.module_m;
    let dedendum_r = pitch_r - 1.25 * p.module_m;
    let root_r = fmax(dedendum_r, base_r * 0.5);

    let angular_pitch = 2.0 * PI() / n.to_float();
    let inv_alpha = involute(alpha);
    let half_tooth_angle = angular_pitch / 4.0 + inv_alpha;
    let backlash_angle = p.backlash / (2.0 * pitch_r);

    let cx = p.center_x;
    let cy = p.center_y;
    let rot = p.rotation_offset;
    let ca = cos(rot);
    let sa = sin(rot);

    let center_pt = tf(cx, cy, ca, sa, 0.0, 0.0);
    let center_idx = sk.point(center_pt[0], center_pt[1]);

    let q = addendum_r / base_r;
    let max_roll_angle = sqrt(q * q - 1.0);
    let num_inv_samples = 12;

    let tooth_right_root = [];
    let tooth_left_root = [];
    for tooth in 0..n {
        let tooth_angle = tooth.to_float() * angular_pitch;
        let right_angle = tooth_angle + half_tooth_angle - backlash_angle;
        let left_angle = tooth_angle - half_tooth_angle + backlash_angle;

        let rr = tf(cx, cy, ca, sa, root_r * cos(right_angle), root_r * sin(right_angle));
        tooth_right_root.push(sk.point(rr[0], rr[1]));

        let lr = tf(cx, cy, ca, sa, root_r * cos(left_angle), root_r * sin(left_angle));
        tooth_left_root.push(sk.point(lr[0], lr[1]));
    }

    for tooth in 0..n {
        let tooth_angle = tooth.to_float() * angular_pitch;
        let right_start_angle = tooth_angle + half_tooth_angle - backlash_angle;
        let left_start_angle = tooth_angle - half_tooth_angle + backlash_angle;

        // Left involute points (base → tip).
        let left_pts = [];
        for i in 0..=num_inv_samples {
            let t = i.to_float() / num_inv_samples.to_float();
            let roll = t * max_roll_angle;
            let pt = involute_point(base_r, roll);
            let inv_angle = atan(pt[1], pt[0]);
            let r = sqrt(pt[0] * pt[0] + pt[1] * pt[1]);
            let adjusted = left_start_angle + inv_angle;
            left_pts.push(tf(cx, cy, ca, sa, r * cos(adjusted), r * sin(adjusted)));
        }

        let left_base_id = sk.point(left_pts[0][0], left_pts[0][1]);
        sk.line(tooth_left_root[tooth], left_base_id);

        let left_ids = [left_base_id];
        for k in 1..(left_pts.len() - 1) {
            left_ids.push(sk.point(left_pts[k][0], left_pts[k][1]));
        }
        let left_tip_id = sk.point(left_pts[left_pts.len() - 1][0], left_pts[left_pts.len() - 1][1]);
        left_ids.push(left_tip_id);
        sk.spline(left_ids);

        // Right involute points (tip → base).
        let right_pts = [];
        for i in range(num_inv_samples, -1, -1) {
            let t = i.to_float() / num_inv_samples.to_float();
            let roll = t * max_roll_angle;
            let pt = involute_point(base_r, roll);
            let inv_angle = atan(pt[1], pt[0]);
            let r = sqrt(pt[0] * pt[0] + pt[1] * pt[1]);
            let adjusted = right_start_angle - inv_angle;
            right_pts.push(tf(cx, cy, ca, sa, r * cos(adjusted), r * sin(adjusted)));
        }

        let right_tip_id = sk.point(right_pts[0][0], right_pts[0][1]);
        sk.arc(center_idx, left_tip_id, right_tip_id);

        let right_ids = [right_tip_id];
        for k in 1..(right_pts.len() - 1) {
            right_ids.push(sk.point(right_pts[k][0], right_pts[k][1]));
        }
        let right_base_id = sk.point(right_pts[right_pts.len() - 1][0], right_pts[right_pts.len() - 1][1]);
        right_ids.push(right_base_id);
        sk.spline(right_ids);

        sk.line(right_base_id, tooth_right_root[tooth]);

        let next_tooth = (tooth + 1) % n;
        sk.arc(center_idx, tooth_right_root[tooth], tooth_left_root[next_tooth]);
    }
    center_idx
}

fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    if p.internal {
        sk.gear(#{
            tooth_count: p.tooth_count, module_m: p.module_m,
            pressure_angle_deg: p.pressure_angle_deg, backlash: p.backlash,
            center_x: p.center_x, center_y: p.center_y,
            rotation_offset: p.rotation_offset, internal: true
        });
    } else {
        gear_sketch(sk, p);
    }
    let regions = sk.finish().regions();
    if regions.len() != 1 {
        ctx.fail("gear profile did not close into exactly one region (" + regions.len() + ")");
    }
    ctx.extrude(regions[0], #{ depth: p.face_width, combine: "NewBody" })
}
