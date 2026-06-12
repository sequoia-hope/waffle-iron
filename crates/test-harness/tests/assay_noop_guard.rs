//! Corpus no-op guard (PR-ASSAY-NOOP, 2026-06-12): every assay operation
//! must CHANGE the result volume, and a union's result must not equal a
//! single operand (no swallowed bosses, no bosses swallowing the body, no
//! cuts through pure free space). The generator repairs these at
//! generation time (`fix_noop_operations`); this test re-derives the
//! verdict independently from the WRITTEN corpus files so a generator or
//! corpus regression is loud.
//!
//! Decidable classes only: extrude-vs-extrude with the engine's plane
//! basis. Cases containing revolve ops are skipped for the CUT check (the
//! repair models revolve bodies by a conservative AABB; this independent
//! oracle stays simpler and just skips them).
use std::fs;
use std::path::PathBuf;

fn basis(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let r = if n[2].abs() < 0.99 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let c = [
        r[1] * n[2] - r[2] * n[1],
        r[2] * n[0] - r[0] * n[2],
        r[0] * n[1] - r[1] * n[0],
    ];
    let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    let u = if l > 1e-12 {
        [c[0] / l, c[1] / l, c[2] / l]
    } else {
        [1.0, 0.0, 0.0]
    };
    let v = [
        n[1] * u[2] - n[2] * u[1],
        n[2] * u[0] - n[0] * u[2],
        n[0] * u[1] - n[1] * u[0],
    ];
    (u, v)
}

#[derive(Clone)]
struct Tool {
    o: [f64; 3],
    u: [f64; 3],
    v: [f64; 3],
    n: [f64; 3],
    hw: f64,
    hh: f64,
    span: (f64, f64),
}
impl Tool {
    fn w(&self, x: f64, y: f64, h: f64) -> [f64; 3] {
        [
            self.o[0] + x * self.u[0] + y * self.v[0] + h * self.n[0],
            self.o[1] + x * self.u[1] + y * self.v[1] + h * self.n[1],
            self.o[2] + x * self.u[2] + y * self.v[2] + h * self.n[2],
        ]
    }
    fn aabb(&self) -> ([f64; 3], [f64; 3]) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for &sx in &[-self.hw, self.hw] {
            for &sy in &[-self.hh, self.hh] {
                for &sh in &[self.span.0, self.span.1] {
                    let p = self.w(sx, sy, sh);
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
            }
        }
        (lo, hi)
    }
    fn contains(&self, p: [f64; 3]) -> bool {
        let d = [p[0] - self.o[0], p[1] - self.o[1], p[2] - self.o[2]];
        let x = d[0] * self.u[0] + d[1] * self.u[1] + d[2] * self.u[2];
        let y = d[0] * self.v[0] + d[1] * self.v[1] + d[2] * self.v[2];
        let h = d[0] * self.n[0] + d[1] * self.n[1] + d[2] * self.n[2];
        x.abs() <= self.hw && y.abs() <= self.hh && h >= self.span.0 && h <= self.span.1
    }
}

#[test]
fn scan() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay");
    let mut counts = (0, 0, 0, 0); // swallowed_boss, swallowing_boss, freespace_cut, cases_affected
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map(|e| e != "waffle").unwrap_or(true) {
            continue;
        }
        let id = path.file_stem().unwrap().to_str().unwrap().to_string();
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let feats = &json["tabs"][0]["kind"]["features"]["features"];
        let mut bosses: Vec<Tool> = Vec::new();
        let mut flags: Vec<String> = Vec::new();
        let mut has_revolve = false;
        let mut sketch: Option<([f64; 3], [f64; 3], f64, f64)> = None; // origin, n, hw, hh
        let mut op_i = 0;
        for f in feats.as_array().unwrap() {
            let op = &f["operation"];
            match op["type"].as_str().unwrap() {
                "Sketch" => {
                    let s = &op["sketch"];
                    let o: Vec<f64> = s["plane_origin"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_f64().unwrap())
                        .collect();
                    let n: Vec<f64> = s["plane_normal"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_f64().unwrap())
                        .collect();
                    let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    // in-plane bbox of positions (or circle radius)
                    let (mut hw, mut hh) = (0.0f64, 0.0f64);
                    if let Some(pos) = s["solved_positions"].as_object() {
                        for (_k, v) in pos {
                            let a = v.as_array().unwrap();
                            hw = hw.max(a[0].as_f64().unwrap().abs());
                            hh = hh.max(a[1].as_f64().unwrap().abs());
                        }
                    }
                    if let Some(profiles) = s["solved_profiles"].as_array() {
                        for p in profiles {
                            if let Some(c) = p["circle"].as_object() {
                                let r = c["radius"].as_f64().unwrap();
                                hw = hw.max(r);
                                hh = hh.max(r);
                            }
                        }
                    }
                    sketch = Some((
                        [o[0], o[1], o[2]],
                        [n[0] / nl, n[1] / nl, n[2] / nl],
                        hw,
                        hh,
                    ));
                }
                "Extrude" => {
                    op_i += 1;
                    let Some((o, n, hw, hh)) = sketch else {
                        continue;
                    };
                    if hw <= 0.0 || hh <= 0.0 {
                        bosses.clear();
                        flags.push(format!("op{op_i}:undecidable"));
                        continue;
                    }
                    let p = &op["params"];
                    let depth = p["depth"].as_f64().unwrap();
                    let cut = p["cut"].as_bool().unwrap_or(false);
                    // honor an explicit flipped direction
                    let flipped = p["direction"]
                        .as_array()
                        .map(|d| {
                            let dx = [
                                d[0].as_f64().unwrap(),
                                d[1].as_f64().unwrap(),
                                d[2].as_f64().unwrap(),
                            ];
                            dx[0] * n[0] + dx[1] * n[1] + dx[2] * n[2] < 0.0
                        })
                        .unwrap_or(false);
                    let (u, v) = basis(n);
                    let mk = |span: (f64, f64)| Tool {
                        o,
                        u,
                        v,
                        n,
                        hw,
                        hh,
                        span,
                    };
                    if !cut {
                        let t = mk(if flipped { (-depth, 0.0) } else { (0.0, depth) });
                        if !bosses.is_empty() {
                            let corners: Vec<[f64; 3]> = {
                                let mut c = vec![];
                                for &sx in &[-hw, hw] {
                                    for &sy in &[-hh, hh] {
                                        for &sh in &[t.span.0, t.span.1] {
                                            c.push(t.w(sx, sy, sh));
                                        }
                                    }
                                }
                                c
                            };
                            if bosses
                                .iter()
                                .any(|b| corners.iter().all(|&c| b.contains(c)))
                            {
                                flags.push(format!("op{op_i}:swallowed_boss"));
                            } else if bosses.iter().all(|b| {
                                let mut c = vec![];
                                for &sx in &[-b.hw, b.hw] {
                                    for &sy in &[-b.hh, b.hh] {
                                        for &sh in &[b.span.0, b.span.1] {
                                            c.push(b.w(sx, sy, sh));
                                        }
                                    }
                                }
                                c.iter().all(|&p| t.contains(p))
                            }) {
                                flags.push(format!("op{op_i}:swallows_body"));
                            }
                        }
                        bosses.push(t);
                    } else {
                        // approximate aim: toward body mid
                        let span = (-depth, 0.0);
                        let t1 = mk(span);
                        let t2 = mk((0.0, depth));
                        if !bosses.is_empty() {
                            let dj = |t: &Tool| {
                                let ta = t.aabb();
                                bosses.iter().all(|b| {
                                    let bb = b.aabb();
                                    !(0..3).all(|k| ta.0[k] <= bb.1[k] && bb.0[k] <= ta.1[k])
                                })
                            };
                            if dj(&t1) && dj(&t2) {
                                flags.push(format!("op{op_i}:freespace_cut"));
                            }
                        }
                    }
                }
                "Revolve" => {
                    op_i += 1;
                    bosses.clear();
                    has_revolve = true;
                    flags.push(format!("op{op_i}:undecidable"));
                }
                _ => {}
            }
        }
        let real: Vec<&String> = flags
            .iter()
            .filter(|f| !f.contains("undecidable"))
            .filter(|f| !(has_revolve && f.contains("freespace_cut")))
            .collect();
        if !real.is_empty() {
            eprintln!("{id}: {real:?}");
            counts.3 += 1;
            for f in &real {
                if f.contains("swallowed_boss") {
                    counts.0 += 1;
                }
                if f.contains("swallows_body") {
                    counts.1 += 1;
                }
                if f.contains("freespace_cut") {
                    counts.2 += 1;
                }
            }
        }
    }
    eprintln!(
        "TOTAL swallowed_boss={} swallows_body={} freespace_cut={} cases={}",
        counts.0, counts.1, counts.2, counts.3
    );
    assert_eq!(
        (counts.0, counts.1, counts.2),
        (0, 0, 0),
        "corpus contains no-op operations — regenerate with the repaired generator"
    );
}
