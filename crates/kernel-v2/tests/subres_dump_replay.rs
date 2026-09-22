//! Scratch replay of a `KV2_SUBRES_DUMP` chart polygon through the flood-fill
//! CDT (dev tool; run with `KV2_SUBRES_DUMP=<path> cargo test ... -- --ignored`).

use cad_primitives::Point2;

#[test]
#[ignore]
fn replay_dump() {
    let path = std::env::var("KV2_SUBRES_DUMP").expect("KV2_SUBRES_DUMP");
    let text = std::fs::read_to_string(&path).expect("read dump");
    let mut pts: Vec<Point2> = Vec::new();
    let mut outer: Vec<u32> = Vec::new();
    let mut holes: Vec<Vec<u32>> = Vec::new();
    let mut dumped: Vec<[u32; 3]> = Vec::new();
    let mut x: [u32; 3] = [0; 3];
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f[0] {
            "P" => pts.push(Point2::new(f[2].parse().unwrap(), f[3].parse().unwrap())),
            "O" => outer = f[1..].iter().map(|s| s.parse().unwrap()).collect(),
            "H" => holes.push(f[1..].iter().map(|s| s.parse().unwrap()).collect()),
            "T" => dumped.push([
                f[1].parse().unwrap(),
                f[2].parse().unwrap(),
                f[3].parse().unwrap(),
            ]),
            "X" => {
                x = [
                    f[1].parse().unwrap(),
                    f[2].parse().unwrap(),
                    f[3].parse().unwrap(),
                ]
            }
            _ => {}
        }
    }
    eprintln!(
        "npts={} outer={} holes={} dumped_tris={}",
        pts.len(),
        outer.len(),
        holes.len(),
        dumped.len()
    );
    let tris = yang_rs::cdt_polygon_with_holes_floodfill(&pts, &outer, &holes).expect("cdt");
    eprintln!("raw cdt tris={}", tris.len());
    let has = |ts: &[[u32; 3]], t: [u32; 3]| {
        ts.iter().any(|u| {
            let mut a = *u;
            a.sort();
            let mut b = t;
            b.sort();
            a == b
        })
    };
    eprintln!(
        "offender {x:?}: in raw cdt = {}, in dumped = {}",
        has(&tris, x),
        has(&dumped, x)
    );
    // exact orientation of the offender and its 2D area
    {
        let (a, b, c) = (pts[x[0] as usize], pts[x[1] as usize], pts[x[2] as usize]);
        let ar = (b.x() - a.x()) * (c.y() - a.y()) - (b.y() - a.y()) * (c.x() - a.x());
        eprintln!("f64 signed 2*area(offender) = {ar:e}");
    }
    // outer ring signed area
    let mut area = 0.0;
    for i in 0..outer.len() {
        let a = pts[outer[i] as usize];
        let b = pts[outer[(i + 1) % outer.len()] as usize];
        area += a.x() * b.y() - b.x() * a.y();
    }
    eprintln!("outer signed area = {:e}", 0.5 * area);
    for v in x {
        eprintln!(
            "  vertex {v}: p2=({:e},{:e})",
            pts[v as usize].x(),
            pts[v as usize].y()
        );
        for t in &tris {
            if t.contains(&v) {
                eprintln!("    raw tri {t:?}");
            }
        }
    }
    // Every raw-CDT triangle over three consecutive outer vertices.
    let n = outer.len();
    let mut n_ears = 0;
    for i in 0..n {
        let t = [outer[i], outer[(i + 1) % n], outer[(i + 2) % n]];
        if has(&tris, t) {
            n_ears += 1;
            let a = pts[t[0] as usize];
            let b = pts[t[1] as usize];
            let c = pts[t[2] as usize];
            let ar = 0.5 * ((b.x() - a.x()) * (c.y() - a.y()) - (b.y() - a.y()) * (c.x() - a.x()));
            eprintln!("  consecutive ear {t:?} area2d={ar:e}");
        }
    }
    eprintln!("consecutive ears in raw cdt: {n_ears}");
    // Differences between the raw CDT and the dumped (post-flip) set.
    let only_raw: Vec<_> = tris.iter().filter(|t| !has(&dumped, **t)).collect();
    let only_dumped: Vec<_> = dumped.iter().filter(|t| !has(&tris, **t)).collect();
    eprintln!("only in raw: {only_raw:?}\nonly in dumped: {only_dumped:?}");
}
