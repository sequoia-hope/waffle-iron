//! The Rhai interpreter: sandbox limits, the registered API, and one
//! evaluation of `entry(ctx, p)` against a [`Recorder`](super::host::Recorder).
//!
//! Determinism and sandboxing (spec §A1.2): the crate is built with no
//! `time` package (no clock), no runtime-seeded hashing, `eval` disabled,
//! and no I/O functions registered. Limits are set per evaluation so a
//! runaway script fails loud in milliseconds.

use std::rc::Rc;

use rhai::{Array, Dynamic, Engine, EvalAltResult, Map, Scope, AST};

use super::host::{
    dyn_ids, dyn_num, dyn_u32, gear_params_from_map, plane_from_dynamic, points_from_dynamic,
    runtime, Ctx, FeatureRef, PlaneRef, PlaneSpec, Query, Region, Shared, SketchBuilder, SketchRef,
};
use waffle_types::{OutputKey, Role, TopoKind};

/// Interpreter limits (spec §A5).
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_operations: u64,
    pub max_call_levels: usize,
    pub max_array_size: usize,
    pub max_map_size: usize,
    pub max_string_size: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_operations: 5_000_000,
            max_call_levels: 64,
            max_array_size: 200_000,
            max_map_size: 20_000,
            max_string_size: 1 << 20,
        }
    }
}

/// Why an evaluation failed.
#[derive(Debug, Clone)]
pub struct Failure {
    /// `parse`, `runtime`, `limit`, or `fail` (an explicit `ctx.fail`).
    pub stage: &'static str,
    pub reason: String,
}

fn opt_bool(m: &Map, key: &str) -> Result<bool, Box<EvalAltResult>> {
    match m.get(key) {
        None => Ok(false),
        Some(v) => v
            .as_bool()
            .or_else(|_| runtime(format!("{key}: expected true/false"))),
    }
}

/// Build an engine with the API registered.
pub fn build_engine(limits: &Limits) -> Engine {
    let mut engine = Engine::new();
    engine
        .set_max_operations(limits.max_operations)
        .set_max_call_levels(limits.max_call_levels)
        .set_max_array_size(limits.max_array_size)
        .set_max_map_size(limits.max_map_size)
        .set_max_string_size(limits.max_string_size)
        .set_max_expr_depths(128, 64);
    engine.disable_symbol("eval");

    // ── Free functions ──────────────────────────────────────────────────
    engine.register_fn("mm", |x: f64| x * 1e-3);
    engine.register_fn("mm", |x: i64| x as f64 * 1e-3);
    engine.register_fn("inch", |x: f64| x * 0.0254);
    engine.register_fn("inch", |x: i64| x as f64 * 0.0254);
    engine.register_fn(
        "plane",
        |origin: Dynamic, normal: Dynamic| -> Result<PlaneRef, Box<EvalAltResult>> {
            let mut m = Map::new();
            m.insert("origin".into(), origin);
            m.insert("normal".into(), normal);
            Ok(PlaneRef(plane_from_dynamic(&Dynamic::from_map(m))?))
        },
    );

    // ── Ctx ─────────────────────────────────────────────────────────────
    engine.register_type_with_name::<Ctx>("Ctx");
    engine.register_fn("sketch", |ctx: &mut Ctx, plane: Dynamic| ctx.sketch(&plane));
    engine.register_fn(
        "extrude",
        |ctx: &mut Ctx, region: Dynamic, opts: Map| -> Result<Dynamic, Box<EvalAltResult>> {
            if let Some(r) = region.clone().try_cast::<Region>() {
                return Ok(Dynamic::from(ctx.extrude(&r, &opts)?));
            }
            if let Some(arr) = region.clone().try_cast::<Array>() {
                let mut out = Array::new();
                for item in arr {
                    let Some(r) = item.try_cast::<Region>() else {
                        return runtime("extrude: expected a region or an array of regions");
                    };
                    out.push(Dynamic::from(ctx.extrude(&r, &opts)?));
                }
                return Ok(Dynamic::from_array(out));
            }
            runtime(format!(
                "extrude: expected a region (from sketch.finish().regions()), got {}",
                region.type_name()
            ))
        },
    );
    engine.register_fn("revolve", |ctx: &mut Ctx, region: Region, opts: Map| {
        ctx.revolve(&region, &opts)
    });
    engine.register_fn(
        "boolean",
        |ctx: &mut Ctx, op: &str, a: Dynamic, b: Dynamic| ctx.boolean(op, &a, &b),
    );
    engine.register_fn("log", |ctx: &mut Ctx, msg: &str| ctx.log(msg));
    engine.register_fn("fail", |ctx: &mut Ctx, msg: &str| ctx.fail(msg));
    engine.register_fn("param", |ctx: &mut Ctx, name: &str| ctx.param(name));

    // ── SketchBuilder ───────────────────────────────────────────────────
    engine.register_type_with_name::<SketchBuilder>("Sketch");
    engine.register_fn(
        "point",
        |sk: &mut SketchBuilder, x: Dynamic, y: Dynamic| -> Result<i64, Box<EvalAltResult>> {
            sk.point(dyn_num(&x, "point.x")?, dyn_num(&y, "point.y")?, false)
        },
    );
    engine.register_fn(
        "point",
        |sk: &mut SketchBuilder,
         x: Dynamic,
         y: Dynamic,
         opts: Map|
         -> Result<i64, Box<EvalAltResult>> {
            sk.point(
                dyn_num(&x, "point.x")?,
                dyn_num(&y, "point.y")?,
                opt_bool(&opts, "construction")?,
            )
        },
    );
    engine.register_fn(
        "line",
        |sk: &mut SketchBuilder, a: Dynamic, b: Dynamic| -> Result<i64, Box<EvalAltResult>> {
            sk.line(dyn_u32(&a, "line")?, dyn_u32(&b, "line")?, false)
        },
    );
    engine.register_fn(
        "line",
        |sk: &mut SketchBuilder,
         a: Dynamic,
         b: Dynamic,
         opts: Map|
         -> Result<i64, Box<EvalAltResult>> {
            sk.line(
                dyn_u32(&a, "line")?,
                dyn_u32(&b, "line")?,
                opt_bool(&opts, "construction")?,
            )
        },
    );
    engine.register_fn(
        "circle",
        |sk: &mut SketchBuilder, c: Dynamic, r: Dynamic| -> Result<i64, Box<EvalAltResult>> {
            sk.circle(dyn_u32(&c, "circle")?, dyn_num(&r, "circle.radius")?, false)
        },
    );
    engine.register_fn(
        "circle",
        |sk: &mut SketchBuilder,
         c: Dynamic,
         r: Dynamic,
         opts: Map|
         -> Result<i64, Box<EvalAltResult>> {
            sk.circle(
                dyn_u32(&c, "circle")?,
                dyn_num(&r, "circle.radius")?,
                opt_bool(&opts, "construction")?,
            )
        },
    );
    engine.register_fn(
        "arc",
        |sk: &mut SketchBuilder,
         c: Dynamic,
         s: Dynamic,
         e: Dynamic|
         -> Result<i64, Box<EvalAltResult>> {
            sk.arc(
                dyn_u32(&c, "arc")?,
                dyn_u32(&s, "arc")?,
                dyn_u32(&e, "arc")?,
                false,
            )
        },
    );
    engine.register_fn(
        "spline",
        |sk: &mut SketchBuilder, pts: Dynamic| -> Result<i64, Box<EvalAltResult>> {
            sk.spline(dyn_ids(&pts)?, false)
        },
    );
    engine.register_fn(
        "gear",
        |sk: &mut SketchBuilder, params: Map| -> Result<i64, Box<EvalAltResult>> {
            sk.gear(gear_params_from_map(&params)?, false)
        },
    );
    engine.register_fn(
        "polygon",
        |sk: &mut SketchBuilder, pts: Dynamic| -> Result<Array, Box<EvalAltResult>> {
            Ok(sk
                .polygon(&points_from_dynamic(&pts)?)?
                .into_iter()
                .map(Dynamic::from)
                .collect())
        },
    );
    engine.register_fn(
        "rect",
        |sk: &mut SketchBuilder,
         x: Dynamic,
         y: Dynamic,
         w: Dynamic,
         h: Dynamic|
         -> Result<Array, Box<EvalAltResult>> {
            let (x, y, w, h) = (
                dyn_num(&x, "rect.x")?,
                dyn_num(&y, "rect.y")?,
                dyn_num(&w, "rect.w")?,
                dyn_num(&h, "rect.h")?,
            );
            if !(w > 0.0 && h > 0.0) {
                return runtime("rect: width and height must be positive");
            }
            Ok(sk
                .polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])?
                .into_iter()
                .map(Dynamic::from)
                .collect())
        },
    );
    engine.register_fn("finish", |sk: &mut SketchBuilder| sk.finish());

    // ── SketchRef / Region ──────────────────────────────────────────────
    engine.register_type_with_name::<SketchRef>("SketchRef");
    engine.register_fn("regions", |s: &mut SketchRef| -> Array {
        s.regions
            .iter()
            .map(|r| Dynamic::from(Region(r.clone())))
            .collect()
    });
    engine.register_get("id", |s: &mut SketchRef| s.id.to_string());
    engine.register_type_with_name::<Region>("Region");
    engine.register_get("area", |r: &mut Region| r.0.area);
    engine.register_get("is_outer", |r: &mut Region| r.0.is_outer);
    engine.register_get("entity_ids", |r: &mut Region| -> Array {
        r.0.entity_ids
            .iter()
            .map(|&i| Dynamic::from(i as i64))
            .collect()
    });

    // ── FeatureRef / Query ──────────────────────────────────────────────
    engine.register_type_with_name::<FeatureRef>("FeatureRef");
    engine.register_get("id", |f: &mut FeatureRef| f.id.to_string());
    engine.register_type_with_name::<Query>("Query");
    engine.register_type_with_name::<PlaneRef>("Plane");
    fn created_by(f: FeatureRef) -> Query {
        Query {
            feature: f,
            key: OutputKey::Main,
            kind: TopoKind::Solid,
            role: None,
        }
    }
    engine.register_fn("created_by", created_by);
    engine.register_fn("bodies", created_by);
    engine.register_fn(
        "nth",
        |q: &mut Query, i: i64| -> Result<Query, Box<EvalAltResult>> {
            if i < 0 {
                return runtime("nth: index must be ≥ 0");
            }
            let mut out = q.clone();
            out.key = if i == 0 {
                OutputKey::Main
            } else {
                OutputKey::Body { index: i as usize }
            };
            Ok(out)
        },
    );
    engine.register_fn("faces", |q: &mut Query| -> Query {
        let mut out = q.clone();
        out.kind = TopoKind::Face;
        out
    });
    engine.register_fn("edges", |q: &mut Query| -> Query {
        let mut out = q.clone();
        out.kind = TopoKind::Edge;
        out
    });
    engine.register_fn("role", |q: &mut Query, name: &str, index: i64| -> Result<Query, Box<EvalAltResult>> {
        let role: Role = serde_json::from_value(serde_json::json!({ "type": name }))
            .or_else(|_| runtime(format!("role: `{name}` is not a role name (EndCapPositive, EndCapNegative, SideFace, …)")))?;
        if index < 0 {
            return runtime("role: index must be ≥ 0");
        }
        let mut out = q.clone();
        if out.kind == TopoKind::Solid {
            out.kind = TopoKind::Face;
        }
        out.role = Some((role, index as usize));
        Ok(out)
    });
    engine.register_fn(
        "side_face",
        |q: &mut Query, index: i64| -> Result<Query, Box<EvalAltResult>> {
            if index < 0 {
                return runtime("side_face: index must be ≥ 0");
            }
            let mut out = q.clone();
            out.kind = TopoKind::Face;
            out.role = Some((
                Role::SideFace {
                    index: index as usize,
                },
                0,
            ));
            Ok(out)
        },
    );
    engine.register_fn(
        "plane_of",
        |q: &mut Query| -> Result<PlaneRef, Box<EvalAltResult>> {
            if q.kind != TopoKind::Face {
                return runtime("plane_of: the query must select a face");
            }
            Ok(PlaneRef(PlaneSpec::Face(q.to_geom_ref()?)))
        },
    );

    engine
}

/// Compile `text`.
pub fn compile(engine: &Engine, text: &str) -> Result<AST, Failure> {
    engine.compile(text).map_err(|e| Failure {
        stage: "parse",
        reason: e.to_string(),
    })
}

/// Evaluate `entry(ctx, args)` against `rec`, routing `print`/`debug` to
/// the recorder's log. Returns the script's return value.
pub fn run(
    engine: &mut Engine,
    ast: &AST,
    entry: &str,
    args: Map,
    rec: Shared,
) -> Result<Dynamic, Failure> {
    let log_rec = rec.clone();
    engine.on_print(move |s| log_rec.borrow_mut().logs.push(s.to_string()));
    let log_rec = rec.clone();
    engine.on_debug(move |s, _, _| log_rec.borrow_mut().logs.push(s.to_string()));

    if !ast.iter_functions().any(|f| f.name == entry) {
        return Err(Failure {
            stage: "parse",
            reason: format!("the script defines no `fn {entry}(ctx, p)`"),
        });
    }
    let ctx = Ctx {
        rec: rec.clone(),
        args: Rc::new(args.clone()),
    };
    let mut scope = Scope::new();
    let result = engine.call_fn::<Dynamic>(&mut scope, ast, entry, (ctx, args));
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            if let Some(msg) = rec.borrow().failed.clone() {
                return Err(Failure {
                    stage: "fail",
                    reason: msg,
                });
            }
            let stage = match *e {
                EvalAltResult::ErrorTooManyOperations(..)
                | EvalAltResult::ErrorStackOverflow(..)
                | EvalAltResult::ErrorDataTooLarge(..) => "limit",
                _ => "runtime",
            };
            Err(Failure {
                stage,
                reason: e.to_string(),
            })
        }
    }
}
