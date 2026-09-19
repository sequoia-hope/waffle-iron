// @feature name="Roller-chain sprocket" version=1
// @param tooth_count: int = 20 min=5 max=400
// @param pitch: length = 0.0127
// @param roller_diameter: length = 0.00851
// @param center_x: length = 0
// @param center_y: length = 0
// @param rotation_offset: number = 0
// @param seating_radius: length = 0
// @param flank_radius: length = 0
// @param tip_diameter: length = 0
// @param seating_angle_deg: angle = 0
// @param face_width: length = 0.005
// @param plane: plane
// @output body: main
//
// The ISO 606 roller-chain sprocket, as a script. `sprocket_sketch` is a
// line-for-line port of `waffle_types::sprocket::generate_sprocket_profile`:
// the same point order (centre, then per gap its roller centre, seating
// ends, flank centres and tip points), the same entity-id sequence (one
// counter across points and arcs), the same four arcs per gap, and the same
// floating-point expression order, so its entities and positions are
// bit-identical to the built-in generator's (the A-M5 parity oracle,
// `tests/script_sprocket_parity.rs`). The default pitch / roller are ISO 08B
// (1/2″). A zero `seating_radius` / `flank_radius` / `tip_diameter` /
// `seating_angle_deg` means the ISO 606 mid-range value, as the generator's
// `None` does.

fn mid(r) {
    0.5 * (r[0] + r[1])
}

// `waffle_types::sprocket::sprocket_dimensions`: resolve the ISO 606 ranges
// and the values this sprocket uses (metres and degrees).
fn sprocket_dimensions(ctx, p) {
    let z = p.tooth_count;
    if z < 5 {
        ctx.fail("sprocket: tooth_count " + z + " is below the minimum 5");
    }
    let pitch = p.pitch;
    let d1 = p.roller_diameter;
    if !(pitch > 0.0) {
        ctx.fail("sprocket: pitch must be finite and positive (got " + pitch + ")");
    }
    if !(d1 > 0.0) {
        ctx.fail("sprocket: roller_diameter must be finite and positive (got " + d1 + ")");
    }
    let zf = z.to_float();

    let d = pitch / sin(PI() / zf);
    // 0.069·∛d1 is written with d1 in mm.
    let d1_mm = d1 * 1000.0;
    let ri_range = [0.505 * d1, 0.505 * d1 + 0.000069 * cbrt(d1_mm)];
    let re_range = [0.12 * d1 * (zf + 2.0), 0.008 * d1 * (zf * zf + 180.0)];
    let da_range = [d + (1.0 - 1.6 / zf) * pitch - d1, d + 1.25 * pitch - d1];
    let alpha_range = [120.0 - 90.0 / zf, 140.0 - 90.0 / zf];

    let ri = if p.seating_radius > 0.0 { p.seating_radius } else { mid(ri_range) };
    let re = if p.flank_radius > 0.0 { p.flank_radius } else { mid(re_range) };
    let da = if p.tip_diameter > 0.0 { p.tip_diameter } else { mid(da_range) };
    let alpha = if p.seating_angle_deg > 0.0 { p.seating_angle_deg } else { mid(alpha_range) };
    if ri < 0.5 * d1 {
        ctx.fail("sprocket: seating radius " + ri + " is smaller than the roller radius " + (0.5 * d1));
    }
    if alpha >= 180.0 {
        ctx.fail("sprocket: seating angle " + alpha + "° leaves no room for the flanks (must be < 180°)");
    }

    #{
        pitch_diameter: d,
        tip_diameter: da,
        seating_radius: ri,
        flank_radius: re,
        seating_angle_deg: alpha
    }
}

// `waffle_types::sprocket::gap_template`: the one gap the whole profile is
// made of, in the frame where the gap is centred on the +x axis and the
// sprocket centre is the origin.
fn gap_template(ctx, dims, z) {
    let rp = 0.5 * dims.pitch_diameter;
    let ra = 0.5 * dims.tip_diameter;
    let ri = dims.seating_radius;
    let re = dims.flank_radius;
    let beta = 0.5 * to_radians(dims.seating_angle_deg);

    let seat_center = [rp, 0.0];
    // Unit vector from the roller centre to the +y seating end: the inward
    // direction (−x) rotated by −β.
    let u = [-cos(beta), sin(beta)];
    let seat_end_plus = [rp + ri * u[0], ri * u[1]];
    let flank_center_plus = [rp + (ri + re) * u[0], (ri + re) * u[1]];

    // Flank circle (centre C', radius re) ∩ tip circle (origin, radius ra).
    let cx = flank_center_plus[0];
    let cy = flank_center_plus[1];
    let dist = hypot(cx, cy);
    let reach = dist + re;
    if reach <= ra || dist <= 0.0 {
        ctx.fail("sprocket: the flank arc never reaches the tip circle (tip diameter " + dims.tip_diameter + ", flank reaches " + (2.0 * reach) + ")");
    }
    let a = (ra * ra - re * re + dist * dist) / (2.0 * dist);
    let h2 = ra * ra - a * a;
    if h2 < 0.0 {
        ctx.fail("sprocket: the flank arc never reaches the tip circle (tip diameter " + dims.tip_diameter + ", flank reaches " + (2.0 * reach) + ")");
    }
    let h = sqrt(h2);
    let ux = cx / dist;
    let uy = cy / dist;
    let mid = [a * ux, a * uy];
    let candidates = [
        [mid[0] - h * uy, mid[1] + h * ux],
        [mid[0] + h * uy, mid[1] - h * ux]
    ];

    // The flank leaves the seating end counter-clockwise about its centre
    // (the tooth surface is convex); take the intersection reached first.
    let phi_seat = atan(seat_end_plus[1] - cy, seat_end_plus[0] - cx);
    let have_best = false;
    let best = [0.0, 0.0];
    let best_sweep = 0.0;
    for c in candidates {
        let phi = atan(c[1] - cy, c[0] - cx);
        let sweep = phi - phi_seat;
        while sweep <= 0.0 {
            sweep += 2.0 * PI();
        }
        while sweep > 2.0 * PI() {
            sweep -= 2.0 * PI();
        }
        if sweep < PI() && (!have_best || sweep < best_sweep) {
            have_best = true;
            best = c;
            best_sweep = sweep;
        }
    }
    if !have_best {
        ctx.fail("sprocket: the flank arc never reaches the tip circle (tip diameter " + dims.tip_diameter + ", flank reaches " + (2.0 * reach) + ")");
    }
    let tip_plus = best;

    // The tip arc between this gap's +y flank and the next gap's −y flank
    // must have positive length: the flank tip's polar angle γ < π/z.
    let gamma = atan(tip_plus[1], tip_plus[0]);
    let half_pitch_angle = PI() / z.to_float();
    if gamma <= 0.0 || gamma >= half_pitch_angle * (1.0 - 1e-9) {
        ctx.fail("sprocket: neighbouring flanks cross below the tip circle (tip diameter " + dims.tip_diameter + " is too large for " + z + " teeth)");
    }

    #{
        seat_center: seat_center,
        seat_end_plus: seat_end_plus,
        seat_end_minus: [seat_end_plus[0], -seat_end_plus[1]],
        flank_center_plus: flank_center_plus,
        flank_center_minus: [flank_center_plus[0], -flank_center_plus[1]],
        tip_plus: tip_plus,
        tip_minus: [tip_plus[0], -tip_plus[1]]
    }
}

fn place(cx, cy, ca, sa, pt) {
    [cx + pt[0] * ca - pt[1] * sa, cy + pt[0] * sa + pt[1] * ca]
}

// Draw the sprocket profile into `sk`. Returns the centre point id.
fn sprocket_sketch(ctx, sk, p) {
    let dims = sprocket_dimensions(ctx, p);
    let z = p.tooth_count;
    let template = gap_template(ctx, dims, z);

    let cx = p.center_x;
    let cy = p.center_y;
    let pitch_angle = 2.0 * PI() / z.to_float();

    let center_id = sk.point(cx, cy);

    let gaps = [];
    for k in 0..z {
        let theta = p.rotation_offset + k.to_float() * pitch_angle;
        let ca = cos(theta);
        let sa = sin(theta);
        let sc = place(cx, cy, ca, sa, template.seat_center);
        let sp = place(cx, cy, ca, sa, template.seat_end_plus);
        let sm = place(cx, cy, ca, sa, template.seat_end_minus);
        let fp = place(cx, cy, ca, sa, template.flank_center_plus);
        let fm = place(cx, cy, ca, sa, template.flank_center_minus);
        let tp = place(cx, cy, ca, sa, template.tip_plus);
        let tm = place(cx, cy, ca, sa, template.tip_minus);
        gaps.push(#{
            seat_center: sk.point(sc[0], sc[1]),
            seat_plus: sk.point(sp[0], sp[1]),
            seat_minus: sk.point(sm[0], sm[1]),
            flank_center_plus: sk.point(fp[0], fp[1]),
            flank_center_minus: sk.point(fm[0], fm[1]),
            tip_plus: sk.point(tp[0], tp[1]),
            tip_minus: sk.point(tm[0], tm[1])
        });
    }

    for k in 0..z {
        let g = gaps[k];
        let next = gaps[(k + 1) % z];
        // Seating arc: concave, so CCW about the roller centre runs +y → −y.
        sk.arc(g.seat_center, g.seat_plus, g.seat_minus);
        // Flanks: convex, CCW about their own centres from the seat outward
        // on the +y side and from the tip inward on the −y side.
        sk.arc(g.flank_center_plus, g.seat_plus, g.tip_plus);
        sk.arc(g.flank_center_minus, g.tip_minus, g.seat_minus);
        // Tip arc to the next gap, CCW about the sprocket centre.
        sk.arc(center_id, g.tip_plus, next.tip_minus);
    }
    center_id
}

fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sprocket_sketch(ctx, sk, p);
    let regions = sk.finish().regions();
    if regions.len() != 1 {
        ctx.fail("sprocket profile did not close into exactly one region (" + regions.len() + ")");
    }
    ctx.extrude(regions[0], #{ depth: p.face_width, combine: "NewBody" })
}
