//! The script host: what a script's API calls RECORD.
//!
//! A script does not execute geometry as it runs. Every solid-producing
//! call appends a child [`Feature`] to a private sub-tree owned by the node
//! and returns a handle (`FeatureRef`, `Query`) that lowers to an ordinary
//! `GeomRef`; the engine executes the recorded children afterwards
//! (`super::execute`). Sketches ARE evaluated at `finish()` — profile
//! extraction is pure Rust with no kernel — so the script sees its regions.
//!
//! This split is what keeps the interpreter free of kernel lifetimes: the
//! host holds no `&mut dyn KernelBundle`, only plain data, so it can live in
//! an `Rc<RefCell<_>>` that the `'static` Rhai callbacks share.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rhai::{Array, Dynamic, EvalAltResult, Map, Position};
use uuid::Uuid;
use waffle_types::gear::GearParams;
use waffle_types::sprocket::SprocketParams;
use waffle_types::{
    Anchor, ClosedProfile, GeomRef, OutputKey, ResolvePolicy, Role, Selector, Sketch, SketchEntity,
    SolveStatus, TopoKind,
};

use crate::types::{
    BooleanOp, BooleanParams, CombineMode, DepthMode, ExtrudeParams, Feature, Operation,
    RevolveParams,
};

/// Geometry budget (spec §A5): more child operations than this is a runaway
/// loop, refused loudly rather than run.
pub const MAX_CHILD_OPS: usize = 2000;

/// Where a script sketch lies; resolved to `(origin, normal)` at execution.
// `large_enum_variant`: `Face` carries a `GeomRef`; one per sketch child.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum PlaneSpec {
    OriginNormal {
        origin: [f64; 3],
        normal: [f64; 3],
    },
    /// A datum plane feature id (or one of the three built-in planes).
    Datum(Uuid),
    /// A planar face of an earlier child.
    Face(GeomRef),
}

/// A plane value a script can hold (a `plane` parameter, or built with
/// `plane(origin, normal)`).
#[derive(Debug, Clone)]
pub struct PlaneRef(pub PlaneSpec);

/// One recorded child, in order.
#[derive(Debug, Clone)]
pub struct Child {
    pub feature: Feature,
    /// For a sketch child: its plane, resolved at execution.
    pub plane: Option<PlaneSpec>,
    /// Short label for error messages (`extrude`, `sketch`, …).
    pub label: &'static str,
}

/// The recording host.
#[derive(Debug, Default)]
pub struct Recorder {
    pub children: Vec<Child>,
    pub logs: Vec<String>,
    /// Set by `ctx.fail`.
    pub failed: Option<String>,
}

pub type Shared = Rc<RefCell<Recorder>>;

fn rt<T>(msg: impl Into<String>) -> Result<T, Box<EvalAltResult>> {
    Err(Box::new(EvalAltResult::ErrorRuntime(
        Dynamic::from(msg.into()),
        Position::NONE,
    )))
}

fn num(d: &Dynamic, what: &str) -> Result<f64, Box<EvalAltResult>> {
    if let Ok(f) = d.as_float() {
        Ok(f)
    } else if let Ok(i) = d.as_int() {
        Ok(i as f64)
    } else {
        rt(format!("{what}: expected a number, got {}", d.type_name()))
    }
}

fn int(d: &Dynamic, what: &str) -> Result<i64, Box<EvalAltResult>> {
    if let Ok(i) = d.as_int() {
        Ok(i)
    } else if let Ok(f) = d.as_float() {
        if f.fract() == 0.0 {
            Ok(f as i64)
        } else {
            rt(format!("{what}: expected an integer, got {f}"))
        }
    } else {
        rt(format!(
            "{what}: expected an integer, got {}",
            d.type_name()
        ))
    }
}

fn vec3(d: &Dynamic, what: &str) -> Result<[f64; 3], Box<EvalAltResult>> {
    let arr: Array = match d.clone().try_cast::<Array>() {
        Some(a) => a,
        None => return rt(format!("{what}: expected [x, y, z]")),
    };
    if arr.len() != 3 {
        return rt(format!(
            "{what}: expected [x, y, z], got {} values",
            arr.len()
        ));
    }
    Ok([
        num(&arr[0], what)?,
        num(&arr[1], what)?,
        num(&arr[2], what)?,
    ])
}

fn ids(d: &Dynamic, what: &str) -> Result<Vec<u32>, Box<EvalAltResult>> {
    let arr: Array = match d.clone().try_cast::<Array>() {
        Some(a) => a,
        None => return rt(format!("{what}: expected an array of entity ids")),
    };
    arr.iter()
        .map(|v| {
            let i = int(v, what)?;
            u32::try_from(i).or_else(|_| rt(format!("{what}: bad entity id {i}")))
        })
        .collect()
}

fn map_get<'a>(m: &'a Map, key: &str) -> Option<&'a Dynamic> {
    m.get(key).filter(|v| !v.is_unit())
}

fn map_bool(m: &Map, key: &str, default: bool) -> Result<bool, Box<EvalAltResult>> {
    match map_get(m, key) {
        None => Ok(default),
        Some(v) => v
            .as_bool()
            .or_else(|_| rt(format!("{key}: expected true/false"))),
    }
}

fn map_num(m: &Map, key: &str) -> Result<Option<f64>, Box<EvalAltResult>> {
    map_get(m, key).map(|v| num(v, key)).transpose()
}

/// Parse a plane from any of the shapes `ctx.sketch` accepts.
pub fn plane_from_dynamic(d: &Dynamic) -> Result<PlaneSpec, Box<EvalAltResult>> {
    if let Some(p) = d.clone().try_cast::<PlaneRef>() {
        return Ok(p.0);
    }
    if let Some(q) = d.clone().try_cast::<Query>() {
        if q.kind != TopoKind::Face {
            return rt("sketch plane: a query used as a plane must select a face (use .role(\"EndCapPositive\", 0) or .faces())");
        }
        return Ok(PlaneSpec::Face(q.to_geom_ref()?));
    }
    if let Some(m) = d.clone().try_cast::<Map>() {
        let origin = match map_get(&m, "origin") {
            Some(v) => vec3(v, "plane.origin")?,
            None => [0.0; 3],
        };
        let normal = match map_get(&m, "normal") {
            Some(v) => vec3(v, "plane.normal")?,
            None => return rt("plane: a plane map needs `normal: [x, y, z]`"),
        };
        return Ok(PlaneSpec::OriginNormal { origin, normal });
    }
    if let Some(s) = d.clone().try_cast::<rhai::ImmutableString>() {
        return match Uuid::parse_str(s.as_str()) {
            Ok(id) => Ok(PlaneSpec::Datum(id)),
            Err(_) => rt(format!("plane: `{s}` is not a datum plane id (a uuid)")),
        };
    }
    rt(format!(
        "plane: expected #{{ origin, normal }}, a datum plane id, or a face query; got {}",
        d.type_name()
    ))
}

// ── Handles a script holds ──────────────────────────────────────────────────

/// A recorded solid-producing child (extrude, revolve, boolean).
#[derive(Debug, Clone)]
pub struct FeatureRef {
    pub child: usize,
    pub id: Uuid,
}

/// A geometry query (spec §A6): a VALUE that lowers to one `GeomRef` when
/// consumed. M1 covers `created_by`, `nth` (body) and `role` (face).
#[derive(Debug, Clone)]
pub struct Query {
    pub feature: FeatureRef,
    pub key: OutputKey,
    pub kind: TopoKind,
    pub role: Option<(Role, usize)>,
}

impl Query {
    pub fn to_geom_ref(&self) -> Result<GeomRef, Box<EvalAltResult>> {
        let selector = match (&self.kind, &self.role) {
            (TopoKind::Solid, _) => Selector::Role {
                role: Role::EndCapPositive,
                index: 0,
            },
            (_, Some((role, index))) => Selector::Role {
                role: role.clone(),
                index: *index,
            },
            (kind, None) => {
                return rt(format!(
                    "query: a {kind:?} query needs .role(name, index) to name one entity (query filters are M3)"
                ))
            }
        };
        Ok(GeomRef {
            kind: self.kind,
            anchor: Anchor::FeatureOutput {
                feature_id: self.feature.id,
                output_key: self.key.clone(),
            },
            selector,
            policy: ResolvePolicy::Strict,
            scope: None,
        })
    }
}

/// A solid reference from a `FeatureRef` or `Query` value.
fn body_ref(d: &Dynamic, what: &str) -> Result<GeomRef, Box<EvalAltResult>> {
    if let Some(f) = d.clone().try_cast::<FeatureRef>() {
        return Query {
            feature: f,
            key: OutputKey::Main,
            kind: TopoKind::Solid,
            role: None,
        }
        .to_geom_ref();
    }
    if let Some(q) = d.clone().try_cast::<Query>() {
        if q.kind != TopoKind::Solid {
            return rt(format!(
                "{what}: expected a body (a feature ref or a bodies query)"
            ));
        }
        return q.to_geom_ref();
    }
    rt(format!(
        "{what}: expected a feature ref or a body query, got {}",
        d.type_name()
    ))
}

fn body_refs(d: Option<&Dynamic>, what: &str) -> Result<Vec<GeomRef>, Box<EvalAltResult>> {
    let Some(d) = d else { return Ok(Vec::new()) };
    if let Some(arr) = d.clone().try_cast::<Array>() {
        return arr.iter().map(|v| body_ref(v, what)).collect();
    }
    Ok(vec![body_ref(d, what)?])
}

fn combine_mode(m: &Map) -> Result<CombineMode, Box<EvalAltResult>> {
    match map_get(m, "combine") {
        None => Ok(CombineMode::NewBody),
        Some(v) => match v.clone().try_cast::<rhai::ImmutableString>() {
            Some(s) => match s.as_str() {
                "NewBody" | "new_body" | "new" => Ok(CombineMode::NewBody),
                "Add" | "add" | "union" => Ok(CombineMode::Add),
                "Cut" | "cut" | "subtract" => Ok(CombineMode::Cut),
                "Intersect" | "intersect" => Ok(CombineMode::Intersect),
                other => rt(format!(
                    "combine: `{other}` is not one of NewBody, Add, Cut, Intersect"
                )),
            },
            None => rt("combine: expected a string"),
        },
    }
}

// ── Sketches ────────────────────────────────────────────────────────────────

/// A sketch under construction. Entity ids are allocated from ONE counter
/// across points and curves, starting at 1 — the same scheme the built-in
/// gear generator uses, so a script can reproduce its ids exactly.
#[derive(Debug)]
pub struct SketchDraft {
    pub plane: PlaneSpec,
    pub entities: Vec<SketchEntity>,
    pub next_id: u32,
    pub finished: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SketchBuilder {
    pub rec: Shared,
    pub draft: Rc<RefCell<SketchDraft>>,
}

/// A finished sketch child.
#[derive(Debug, Clone)]
pub struct SketchRef {
    pub child: usize,
    pub id: Uuid,
    pub regions: Rc<Vec<RegionInfo>>,
}

/// One closed region of a finished sketch.
#[derive(Debug, Clone)]
pub struct RegionInfo {
    pub sketch: SketchRef0,
    pub index: usize,
    pub entity_ids: Vec<u32>,
    pub is_outer: bool,
    pub area: f64,
}

/// The identity half of a `SketchRef` (no back-reference to regions).
#[derive(Debug, Clone)]
pub struct SketchRef0 {
    pub child: usize,
    pub id: Uuid,
}

/// A region value handed to `extrude`/`revolve`.
#[derive(Debug, Clone)]
pub struct Region(pub RegionInfo);

impl SketchBuilder {
    fn alloc(&self) -> Result<u32, Box<EvalAltResult>> {
        let mut d = self.draft.borrow_mut();
        if d.finished.is_some() {
            return rt("sketch: already finished");
        }
        let id = d.next_id;
        d.next_id += 1;
        Ok(id)
    }

    fn push(&self, e: SketchEntity) {
        self.draft.borrow_mut().entities.push(e);
    }

    fn need_point(&self, id: u32, what: &str) -> Result<(), Box<EvalAltResult>> {
        let ok = self
            .draft
            .borrow()
            .entities
            .iter()
            .any(|e| matches!(e, SketchEntity::Point { id: pid, .. } if *pid == id));
        if ok {
            Ok(())
        } else {
            rt(format!("{what}: {id} is not a point of this sketch"))
        }
    }

    pub fn point(&mut self, x: f64, y: f64, construction: bool) -> Result<i64, Box<EvalAltResult>> {
        if !(x.is_finite() && y.is_finite()) {
            return rt("point: non-finite coordinate");
        }
        let id = self.alloc()?;
        self.push(SketchEntity::Point {
            id,
            x,
            y,
            construction,
        });
        Ok(id as i64)
    }

    pub fn line(&mut self, a: u32, b: u32, construction: bool) -> Result<i64, Box<EvalAltResult>> {
        self.need_point(a, "line")?;
        self.need_point(b, "line")?;
        if a == b {
            return rt("line: start and end are the same point");
        }
        let id = self.alloc()?;
        self.push(SketchEntity::Line {
            id,
            start_id: a,
            end_id: b,
            construction,
        });
        Ok(id as i64)
    }

    pub fn circle(
        &mut self,
        center: u32,
        radius: f64,
        construction: bool,
    ) -> Result<i64, Box<EvalAltResult>> {
        self.need_point(center, "circle")?;
        if !(radius.is_finite() && radius > 0.0) {
            return rt("circle: radius must be positive");
        }
        let id = self.alloc()?;
        self.push(SketchEntity::Circle {
            id,
            center_id: center,
            radius,
            construction,
        });
        Ok(id as i64)
    }

    pub fn arc(
        &mut self,
        center: u32,
        start: u32,
        end: u32,
        construction: bool,
    ) -> Result<i64, Box<EvalAltResult>> {
        self.need_point(center, "arc")?;
        self.need_point(start, "arc")?;
        self.need_point(end, "arc")?;
        let id = self.alloc()?;
        self.push(SketchEntity::Arc {
            id,
            center_id: center,
            start_id: start,
            end_id: end,
            construction,
        });
        Ok(id as i64)
    }

    pub fn spline(
        &mut self,
        points: Vec<u32>,
        construction: bool,
    ) -> Result<i64, Box<EvalAltResult>> {
        if points.len() < 2 {
            return rt("spline: needs at least 2 points");
        }
        for &p in &points {
            self.need_point(p, "spline")?;
        }
        let id = self.alloc()?;
        self.push(SketchEntity::Spline {
            id,
            point_ids: points,
            construction,
        });
        Ok(id as i64)
    }

    pub fn gear(
        &mut self,
        params: GearParams,
        construction: bool,
    ) -> Result<i64, Box<EvalAltResult>> {
        if params.tooth_count < 3 {
            return rt("gear: tooth_count must be at least 3");
        }
        if !(params.module.is_finite() && params.module > 0.0) {
            return rt("gear: module must be positive");
        }
        let id = self.alloc()?;
        self.push(SketchEntity::Gear {
            id,
            params,
            construction,
        });
        Ok(id as i64)
    }

    pub fn sprocket(
        &mut self,
        params: SprocketParams,
        construction: bool,
    ) -> Result<i64, Box<EvalAltResult>> {
        // Refuse here what the generator would refuse at expansion, so the
        // script line that wrote the parameters is the one that fails.
        if let Err(e) = waffle_types::sprocket_dimensions(&params) {
            return rt(e.to_string());
        }
        let id = self.alloc()?;
        self.push(SketchEntity::Sprocket {
            id,
            params,
            construction,
        });
        Ok(id as i64)
    }

    /// Closed polyline through `pts` (`[[x, y], …]`); returns the line ids.
    pub fn polygon(&mut self, pts: &[(f64, f64)]) -> Result<Vec<i64>, Box<EvalAltResult>> {
        if pts.len() < 3 {
            return rt("polygon: needs at least 3 points");
        }
        let mut pids = Vec::with_capacity(pts.len());
        for &(x, y) in pts {
            pids.push(self.point(x, y, false)? as u32);
        }
        let mut lines = Vec::with_capacity(pts.len());
        for i in 0..pids.len() {
            lines.push(self.line(pids[i], pids[(i + 1) % pids.len()], false)?);
        }
        Ok(lines)
    }

    /// Finish: build the `Sketch`, derive regions (pure Rust), record the
    /// sketch child, and hand back a `SketchRef`.
    pub fn finish(&mut self) -> Result<SketchRef, Box<EvalAltResult>> {
        {
            let d = self.draft.borrow();
            if d.finished.is_some() {
                return rt("sketch: finish() called twice");
            }
            if d.entities.is_empty() {
                return rt("sketch: finish() on an empty sketch");
            }
        }
        let (plane, entities) = {
            let d = self.draft.borrow();
            (d.plane.clone(), d.entities.clone())
        };
        let sketch_id = Uuid::new_v4();
        let mut sketch = Sketch {
            id: sketch_id,
            plane: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::Datum {
                    datum_id: match &plane {
                        PlaneSpec::Datum(id) => *id,
                        _ => Uuid::nil(),
                    },
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::Strict,
                scope: None,
            },
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            entities,
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: HashMap::new(),
            projected: Vec::new(),
            solved_profiles: Vec::new(),
        };
        if let PlaneSpec::OriginNormal { origin, normal } = &plane {
            sketch.plane_origin = *origin;
            sketch.plane_normal = *normal;
        }
        derive_sketch(&mut sketch);

        let regions: Vec<RegionInfo> = sketch
            .solved_profiles
            .iter()
            .enumerate()
            .map(|(index, p)| RegionInfo {
                sketch: SketchRef0 {
                    child: 0, // patched below
                    id: sketch_id,
                },
                index,
                entity_ids: p.entity_ids.clone(),
                is_outer: p.is_outer,
                area: profile_area(p, &sketch.solved_positions),
            })
            .collect();

        let child = {
            let mut rec = self.rec.borrow_mut();
            if rec.children.len() >= MAX_CHILD_OPS {
                return rt(format!(
                    "geometry budget exceeded: more than {MAX_CHILD_OPS} child operations"
                ));
            }
            let idx = rec.children.len();
            rec.children.push(Child {
                feature: Feature {
                    id: sketch_id,
                    name: format!("script sketch {idx}"),
                    operation: Operation::Sketch { sketch },
                    suppressed: false,
                    references: Vec::new(),
                },
                plane: Some(plane),
                label: "sketch",
            });
            idx
        };
        let regions: Vec<RegionInfo> = regions
            .into_iter()
            .map(|mut r| {
                r.sketch.child = child;
                r
            })
            .collect();
        self.draft.borrow_mut().finished = Some(child);
        Ok(SketchRef {
            child,
            id: sketch_id,
            regions: Rc::new(regions),
        })
    }
}

/// Derive positions and profiles the way the engine's own finish does:
/// gear entities expand through the built-in generator (their profiles are
/// authoritative); otherwise loops are extracted and given kernel-ready
/// vertex/arc data (`build_finish_profiles`).
pub fn derive_sketch(sketch: &mut Sketch) {
    for e in &sketch.entities {
        if let SketchEntity::Point { id, x, y, .. } = e {
            sketch.solved_positions.insert(*id, (*x, *y));
        }
    }
    // Generators (gear, sprocket) expand into their own profiles; the plain
    // entities drawn alongside them are finished as any hand-drawn sketch
    // is. A sprocket the builder accepted cannot fail here (`sk.sprocket`
    // validates the same parameters), so the expansion result is not a
    // second failure path.
    let plain: Vec<SketchEntity> = sketch
        .entities
        .iter()
        .filter(|e| !e.is_generator())
        .cloned()
        .collect();
    let had_generators = plain.len() != sketch.entities.len();
    if had_generators {
        let _ = sketch.expand_generators();
        if plain.is_empty() {
            return;
        }
    }
    let extracted = waffle_types::profiles::extract_profiles(&plain, &sketch.solved_positions);
    let fp =
        waffle_types::profiles::build_finish_profiles(&extracted, &plain, &sketch.solved_positions);
    sketch.solved_profiles.extend(fp.profiles);
    sketch.solved_positions = fp.solved_positions;
}

/// Shoelace area over a profile's vertex chain (chords for arcs/splines;
/// informational, for `regions()` ordering and inspection).
fn profile_area(p: &ClosedProfile, positions: &HashMap<u32, (f64, f64)>) -> f64 {
    if let Some(c) = &p.circle {
        return std::f64::consts::PI * c.radius * c.radius;
    }
    let pts: Vec<(f64, f64)> = p
        .vertex_ids
        .iter()
        .filter_map(|id| positions.get(id).copied())
        .collect();
    if pts.len() < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..pts.len() {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % pts.len()];
        a += x0 * y1 - x1 * y0;
    }
    (a / 2.0).abs()
}

// ── Solid ops ───────────────────────────────────────────────────────────────

/// The script context handed to `feature(ctx, p)`.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub rec: Shared,
    pub args: Rc<Map>,
}

impl Ctx {
    fn record(
        &self,
        feature: Feature,
        label: &'static str,
    ) -> Result<FeatureRef, Box<EvalAltResult>> {
        let mut rec = self.rec.borrow_mut();
        if rec.children.len() >= MAX_CHILD_OPS {
            return rt(format!(
                "geometry budget exceeded: more than {MAX_CHILD_OPS} child operations"
            ));
        }
        let child = rec.children.len();
        let id = feature.id;
        rec.children.push(Child {
            feature,
            plane: None,
            label,
        });
        Ok(FeatureRef { child, id })
    }

    pub fn sketch(&mut self, plane: &Dynamic) -> Result<SketchBuilder, Box<EvalAltResult>> {
        let plane = plane_from_dynamic(plane)?;
        Ok(SketchBuilder {
            rec: self.rec.clone(),
            draft: Rc::new(RefCell::new(SketchDraft {
                plane,
                entities: Vec::new(),
                next_id: 1,
                finished: None,
            })),
        })
    }

    pub fn extrude(
        &mut self,
        region: &Region,
        opts: &Map,
    ) -> Result<FeatureRef, Box<EvalAltResult>> {
        let depth = match map_num(opts, "depth")? {
            Some(d) if d.is_finite() && d > 0.0 => d,
            Some(d) => return rt(format!("extrude: depth must be positive, got {d}")),
            None => return rt("extrude: `depth` is required"),
        };
        let combine = combine_mode(opts)?;
        let targets = body_refs(map_get(opts, "targets"), "extrude.targets")?;
        if !matches!(combine, CombineMode::NewBody) && targets.is_empty() {
            return rt(format!(
                "extrude: combine {combine:?} needs `targets: [feature refs]` (a script never auto-targets)"
            ));
        }
        let direction = match map_get(opts, "direction") {
            Some(v) => Some(vec3(v, "extrude.direction")?),
            None => None,
        };
        let symmetric = map_bool(opts, "symmetric", false)?;
        let id = Uuid::new_v4();
        let feature = Feature {
            id,
            name: "script extrude".into(),
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: region.0.sketch.id,
                    profile_index: region.0.index,
                    profile_entity_ids: Some(region.0.entity_ids.clone()),
                    depth,
                    depth_expr: None,
                    direction,
                    symmetric,
                    cut: matches!(combine, CombineMode::Cut),
                    merge: matches!(combine, CombineMode::Add),
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: Vec::new(),
                    combine: Some(combine),
                    targets: Some(targets),
                },
            },
            suppressed: false,
            references: Vec::new(),
        };
        self.record(feature, "extrude")
    }

    pub fn revolve(
        &mut self,
        region: &Region,
        opts: &Map,
    ) -> Result<FeatureRef, Box<EvalAltResult>> {
        let axis = match map_get(opts, "axis") {
            Some(a) => a.clone().try_cast::<Map>(),
            None => None,
        };
        let Some(axis) = axis else {
            return rt("revolve: `axis: #{ origin: [x,y,z], direction: [x,y,z] }` is required");
        };
        let axis_origin = match map_get(&axis, "origin") {
            Some(v) => vec3(v, "revolve.axis.origin")?,
            None => [0.0; 3],
        };
        let axis_direction = match map_get(&axis, "direction") {
            Some(v) => vec3(v, "revolve.axis.direction")?,
            None => return rt("revolve: axis needs `direction`"),
        };
        let angle = map_num(opts, "angle_deg")?.unwrap_or(360.0);
        if !(angle.is_finite() && angle != 0.0) {
            return rt(format!(
                "revolve: angle_deg must be a non-zero angle, got {angle}"
            ));
        }
        let combine = combine_mode(opts)?;
        let targets = body_refs(map_get(opts, "targets"), "revolve.targets")?;
        if !matches!(combine, CombineMode::NewBody) && targets.is_empty() {
            return rt(format!("revolve: combine {combine:?} needs `targets`"));
        }
        let id = Uuid::new_v4();
        let feature = Feature {
            id,
            name: "script revolve".into(),
            operation: Operation::Revolve {
                params: RevolveParams {
                    sketch_id: region.0.sketch.id,
                    profile_index: region.0.index,
                    profile_entity_ids: Some(region.0.entity_ids.clone()),
                    axis_origin,
                    axis_direction,
                    angle,
                    angle_expr: None,
                    cut: matches!(combine, CombineMode::Cut),
                    merge: matches!(combine, CombineMode::Add),
                    combine: Some(combine),
                    targets: Some(targets),
                },
            },
            suppressed: false,
            references: Vec::new(),
        };
        self.record(feature, "revolve")
    }

    pub fn boolean(
        &mut self,
        op: &str,
        a: &Dynamic,
        b: &Dynamic,
    ) -> Result<FeatureRef, Box<EvalAltResult>> {
        let operation = match op {
            "union" | "Union" | "add" | "Add" => BooleanOp::Union,
            "subtract" | "Subtract" | "cut" | "Cut" => BooleanOp::Subtract,
            "intersect" | "Intersect" => BooleanOp::Intersect,
            other => {
                return rt(format!(
                    "boolean: `{other}` is not union, subtract or intersect"
                ))
            }
        };
        let body_a = body_ref(a, "boolean target")?;
        let body_b = body_ref(b, "boolean tool")?;
        let id = Uuid::new_v4();
        let feature = Feature {
            id,
            name: "script boolean".into(),
            operation: Operation::BooleanCombine {
                params: BooleanParams {
                    body_a,
                    body_b,
                    operation,
                },
            },
            suppressed: false,
            references: Vec::new(),
        };
        self.record(feature, "boolean")
    }

    pub fn log(&mut self, msg: &str) {
        self.rec.borrow_mut().logs.push(msg.to_string());
    }

    pub fn fail(&mut self, msg: &str) -> Result<(), Box<EvalAltResult>> {
        self.rec.borrow_mut().failed = Some(msg.to_string());
        rt(format!("fail: {msg}"))
    }

    pub fn param(&mut self, name: &str) -> Result<Dynamic, Box<EvalAltResult>> {
        match self.args.get(name) {
            Some(v) => Ok(v.clone()),
            None => rt(format!("param: no parameter named `{name}`")),
        }
    }
}

/// Convert a Rhai map to `GearParams` (snake_case keys; `tooth_count` and
/// `module` required).
pub fn gear_params_from_map(m: &Map) -> Result<GearParams, Box<EvalAltResult>> {
    let tooth_count = match map_get(m, "tooth_count") {
        Some(v) => int(v, "gear.tooth_count")?,
        None => return rt("gear: `tooth_count` is required"),
    };
    // `module` is a Rhai keyword, so the map key is `module_m` (the spec's
    // spelling); a quoted `"module"` key is accepted too.
    let module = match map_num(m, "module_m")?.or(map_num(m, "module")?) {
        Some(v) => v,
        None => return rt("gear: `module_m` is required"),
    };
    let d = GearParams::default();
    Ok(GearParams {
        tooth_count: u32::try_from(tooth_count)
            .or_else(|_| rt(format!("gear: bad tooth_count {tooth_count}")))?,
        module,
        pressure_angle_deg: map_num(m, "pressure_angle_deg")?.unwrap_or(d.pressure_angle_deg),
        backlash: map_num(m, "backlash")?.unwrap_or(0.0),
        center_x: map_num(m, "center_x")?.unwrap_or(0.0),
        center_y: map_num(m, "center_y")?.unwrap_or(0.0),
        rotation_offset: map_num(m, "rotation_offset")?.unwrap_or(0.0),
        internal: map_bool(m, "internal", false)?,
    })
}

/// Convert a Rhai map to `SprocketParams` (snake_case keys; `tooth_count`,
/// `pitch` and `roller_diameter` required; `seating_radius`, `flank_radius`,
/// `tip_diameter`, `seating_angle_deg` override the ISO 606 mid-range
/// defaults).
pub fn sprocket_params_from_map(m: &Map) -> Result<SprocketParams, Box<EvalAltResult>> {
    let tooth_count = match map_get(m, "tooth_count") {
        Some(v) => int(v, "sprocket.tooth_count")?,
        None => return rt("sprocket: `tooth_count` is required"),
    };
    let pitch = match map_num(m, "pitch")? {
        Some(v) => v,
        None => return rt("sprocket: `pitch` is required"),
    };
    let roller_diameter = match map_num(m, "roller_diameter")? {
        Some(v) => v,
        None => return rt("sprocket: `roller_diameter` is required"),
    };
    Ok(SprocketParams {
        tooth_count: u32::try_from(tooth_count)
            .or_else(|_| rt(format!("sprocket: bad tooth_count {tooth_count}")))?,
        pitch,
        roller_diameter,
        center_x: map_num(m, "center_x")?.unwrap_or(0.0),
        center_y: map_num(m, "center_y")?.unwrap_or(0.0),
        rotation_offset: map_num(m, "rotation_offset")?.unwrap_or(0.0),
        standard: waffle_types::SprocketStandard::Iso606,
        seating_radius: map_num(m, "seating_radius")?,
        flank_radius: map_num(m, "flank_radius")?,
        tip_diameter: map_num(m, "tip_diameter")?,
        seating_angle_deg: map_num(m, "seating_angle_deg")?,
    })
}

/// `Dynamic` → `Vec<(f64, f64)>` for `polygon([[x, y], …])`.
pub fn points_from_dynamic(d: &Dynamic) -> Result<Vec<(f64, f64)>, Box<EvalAltResult>> {
    let Some(arr) = d.clone().try_cast::<Array>() else {
        return rt("polygon: expected [[x, y], …]");
    };
    arr.iter()
        .map(|p| {
            let Some(xy) = p.clone().try_cast::<Array>() else {
                return rt("polygon: expected [x, y] pairs");
            };
            if xy.len() != 2 {
                return rt("polygon: expected [x, y] pairs");
            }
            Ok((num(&xy[0], "polygon")?, num(&xy[1], "polygon")?))
        })
        .collect()
}

pub(crate) fn dyn_ids(d: &Dynamic) -> Result<Vec<u32>, Box<EvalAltResult>> {
    ids(d, "spline")
}

pub(crate) fn dyn_u32(d: &Dynamic, what: &str) -> Result<u32, Box<EvalAltResult>> {
    let i = int(d, what)?;
    u32::try_from(i).or_else(|_| rt(format!("{what}: bad entity id {i}")))
}

pub(crate) fn dyn_num(d: &Dynamic, what: &str) -> Result<f64, Box<EvalAltResult>> {
    num(d, what)
}

pub(crate) fn runtime<T>(msg: impl Into<String>) -> Result<T, Box<EvalAltResult>> {
    rt(msg)
}
