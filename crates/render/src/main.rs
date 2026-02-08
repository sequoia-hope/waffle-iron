//! Generate SVG wireframe renders of CAD primitives for the README.

use cad_kernel::boolean::engine::{boolean_op, BoolOp};
use cad_kernel::geometry::point::Point3d;
use cad_kernel::geometry::vector::Vec3;
use cad_kernel::operations::chamfer::chamfer_edge;
use cad_kernel::operations::extrude::{extrude_profile, Profile};
use cad_kernel::operations::feature::{
    BooleanOpType, Feature, FeatureTree, Parameter, SketchConstraint, SketchProfile,
};
use cad_kernel::operations::fillet::fillet_edge;
use cad_kernel::operations::revolve::revolve_profile;
use cad_kernel::topology::brep::EntityStore;
use cad_kernel::topology::primitives::{make_box, make_cylinder, make_sphere};
use cad_tessellation::{mesh_to_obj, mesh_to_stl, tessellate_solid, validate_mesh, TriangleMesh};
use std::fs;

/// Simple isometric projection: 3D -> 2D
fn project(x: f64, y: f64, z: f64) -> (f64, f64) {
    let angle_x: f64 = 0.6;
    let angle_z: f64 = 0.8;
    let rx = x * angle_z.cos() - y * angle_z.sin();
    let ry = x * angle_z.sin() + y * angle_z.cos();
    let rz = z;
    let _py = ry * angle_x.cos() - rz * angle_x.sin();
    let pz = ry * angle_x.sin() + rz * angle_x.cos();
    (rx, -pz)
}

fn mesh_to_svg(mesh: &TriangleMesh, width: f64, height: f64, title: &str) -> String {
    if mesh.indices.is_empty() {
        return format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\">\
             <text x=\"10\" y=\"20\" font-family=\"monospace\" font-size=\"14\" fill=\"#ccc\">{title}</text>\
             </svg>"
        );
    }

    let num_verts = mesh.positions.len() / 3;
    let mut projected: Vec<(f64, f64)> = Vec::with_capacity(num_verts);
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;

    for i in 0..num_verts {
        let x = mesh.positions[i * 3] as f64;
        let y = mesh.positions[i * 3 + 1] as f64;
        let z = mesh.positions[i * 3 + 2] as f64;
        let (px, py) = project(x, y, z);
        projected.push((px, py));
        min_x = min_x.min(px);
        max_x = max_x.max(px);
        min_y = min_y.min(py);
        max_y = max_y.max(py);
    }

    let padding = 40.0;
    let avail_w = width - 2.0 * padding;
    let avail_h = height - 2.0 * padding - 25.0;
    let data_w = (max_x - min_x).max(0.001);
    let data_h = (max_y - min_y).max(0.001);
    let scale = (avail_w / data_w).min(avail_h / data_h);
    let offset_x = padding + (avail_w - data_w * scale) / 2.0;
    let offset_y = padding + 25.0 + (avail_h - data_h * scale) / 2.0;

    let tx = |px: f64| -> f64 { (px - min_x) * scale + offset_x };
    let ty = |py: f64| -> f64 { (py - min_y) * scale + offset_y };

    struct TriInfo {
        i0: usize,
        i1: usize,
        i2: usize,
        depth: f64,
        brightness: f64,
        is_backface: bool,
    }

    let num_tris = mesh.indices.len() / 3;
    let mut tris: Vec<TriInfo> = Vec::with_capacity(num_tris);

    let light_dir = (0.3_f64, -0.5_f64, 0.8_f64);
    let light_len =
        (light_dir.0 * light_dir.0 + light_dir.1 * light_dir.1 + light_dir.2 * light_dir.2)
            .sqrt();

    for t in 0..num_tris {
        let i0 = mesh.indices[t * 3] as usize;
        let i1 = mesh.indices[t * 3 + 1] as usize;
        let i2 = mesh.indices[t * 3 + 2] as usize;

        let z0 = mesh.positions[i0 * 3 + 2] as f64;
        let z1 = mesh.positions[i1 * 3 + 2] as f64;
        let z2 = mesh.positions[i2 * 3 + 2] as f64;
        let depth = (z0 + z1 + z2) / 3.0;

        let ax = mesh.positions[i1 * 3] as f64 - mesh.positions[i0 * 3] as f64;
        let ay = mesh.positions[i1 * 3 + 1] as f64 - mesh.positions[i0 * 3 + 1] as f64;
        let az = mesh.positions[i1 * 3 + 2] as f64 - mesh.positions[i0 * 3 + 2] as f64;
        let bx = mesh.positions[i2 * 3] as f64 - mesh.positions[i0 * 3] as f64;
        let by = mesh.positions[i2 * 3 + 1] as f64 - mesh.positions[i0 * 3 + 1] as f64;
        let bz = mesh.positions[i2 * 3 + 2] as f64 - mesh.positions[i0 * 3 + 2] as f64;
        let nx = ay * bz - az * by;
        let ny = az * bx - ax * bz;
        let nz = ax * by - ay * bx;
        let nlen = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-12);

        let dot =
            (nx * light_dir.0 + ny * light_dir.1 + nz * light_dir.2) / (nlen * light_len);
        let brightness = 0.3 + 0.7 * dot.abs().min(1.0);

        // Backface detection: compute signed area of projected 2D triangle.
        // Negative area = backfacing = inside surface visible (winding error
        // or open mesh).  We colour these red/orange so problems are obvious.
        let (px0, py0) = projected.get(i0).copied().unwrap_or((0.0, 0.0));
        let (px1, py1) = projected.get(i1).copied().unwrap_or((0.0, 0.0));
        let (px2, py2) = projected.get(i2).copied().unwrap_or((0.0, 0.0));
        let signed_area = (px1 - px0) * (py2 - py0) - (px2 - px0) * (py1 - py0);
        let is_backface = signed_area < 0.0;

        tris.push(TriInfo {
            i0,
            i1,
            i2,
            depth,
            brightness,
            is_backface,
        });
    }

    tris.sort_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap());

    // For high-poly meshes, reduce stroke to avoid visual noise
    let stroke_width = if num_tris > 200 { 0.2 } else { 0.5 };
    let stroke_color = if num_tris > 200 { "#222240" } else { "#2a2a4a" };

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" style=\"background:#1a1a2e\">\n\
         <text x=\"{}\" y=\"20\" font-family=\"monospace\" font-size=\"13\" fill=\"#8892b0\" text-anchor=\"middle\">{title}</text>\n",
        width / 2.0
    );

    for tri in &tris {
        let (x0, y0) = (tx(projected[tri.i0].0), ty(projected[tri.i0].1));
        let (x1, y1) = (tx(projected[tri.i1].0), ty(projected[tri.i1].1));
        let (x2, y2) = (tx(projected[tri.i2].0), ty(projected[tri.i2].1));

        let b = tri.brightness;
        // Front faces: blue.  Back faces (inside surface): red/orange.
        let (r, g, bl) = if tri.is_backface {
            ((220.0 * b) as u8, (80.0 * b) as u8, (60.0 * b) as u8)
        } else {
            ((100.0 * b) as u8, (160.0 * b) as u8, (220.0 * b) as u8)
        };

        svg.push_str(&format!(
            "  <polygon points=\"{x0:.1},{y0:.1} {x1:.1},{y1:.1} {x2:.1},{y2:.1}\" \
             fill=\"rgb({r},{g},{bl})\" stroke=\"{stroke_color}\" stroke-width=\"{stroke_width}\"/>\n"
        ));
    }

    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"{}\" font-family=\"monospace\" font-size=\"10\" fill=\"#5a6080\" \
         text-anchor=\"middle\">{} triangles, {} vertices</text>\n",
        width / 2.0,
        height - 8.0,
        num_tris,
        num_verts
    ));

    svg.push_str("</svg>\n");
    svg
}

/// Validate and print mesh quality info.
fn validate_and_report(name: &str, mesh: &TriangleMesh) {
    let val = validate_mesh(mesh);
    let watertight = if val.is_watertight() { "watertight" } else { "open" };
    let printable = if val.is_printable() { "printable" } else { "not printable" };
    println!(
        "  {name}: {tris} tris, {verts} verts, {watertight}, {printable}, vol={vol:.1}",
        tris = mesh.triangle_count(),
        verts = mesh.vertex_count(),
        watertight = watertight,
        printable = printable,
        vol = val.signed_volume,
    );
    if val.boundary_edges > 0 {
        println!("    boundary_edges={}, non_manifold={}", val.boundary_edges, val.non_manifold_edges);
    }
}

fn main() {
    fs::create_dir_all("docs/renders").expect("create docs/renders dir");
    fs::create_dir_all("docs/exports").expect("create docs/exports dir");

    println!("=== Basic Primitives ===");

    // 1. Box
    {
        let mut store = EntityStore::new();
        let solid = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);
        let mesh = tessellate_solid(&store, solid);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Box (10 x 8 x 6)");
        fs::write("docs/renders/box.svg", svg).unwrap();
        validate_and_report("box", &mesh);
        fs::write("docs/exports/box.obj", mesh_to_obj(&mesh)).unwrap();
        fs::write("docs/exports/box.stl", mesh_to_stl(&mesh)).unwrap();
    }

    // 2. High-res cylinder
    {
        let mut store = EntityStore::new();
        let solid = make_cylinder(&mut store, Point3d::ORIGIN, 5.0, 12.0, 96);
        let mesh = tessellate_solid(&store, solid);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Cylinder (r=5, h=12, 96 seg)");
        fs::write("docs/renders/cylinder.svg", svg).unwrap();
        validate_and_report("cylinder", &mesh);
    }

    // 3. High-res sphere
    {
        let mut store = EntityStore::new();
        let solid = make_sphere(&mut store, Point3d::ORIGIN, 6.0, 48, 36);
        let mesh = tessellate_solid(&store, solid);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Sphere (r=6, 48x36)");
        fs::write("docs/renders/sphere.svg", svg).unwrap();
        validate_and_report("sphere", &mesh);
        fs::write("docs/exports/sphere.obj", mesh_to_obj(&mesh)).unwrap();
    }

    // 4. Extruded L-shape
    {
        let mut store = EntityStore::new();
        let profile = Profile::from_points(vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(8.0, 0.0, 0.0),
            Point3d::new(8.0, 3.0, 0.0),
            Point3d::new(3.0, 3.0, 0.0),
            Point3d::new(3.0, 7.0, 0.0),
            Point3d::new(0.0, 7.0, 0.0),
        ]);
        let solid = extrude_profile(&mut store, &profile, Vec3::Z, 4.0).unwrap();
        let mesh = tessellate_solid(&store, solid);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Extruded L-Profile (h=4)");
        fs::write("docs/renders/extrude_l.svg", svg).unwrap();
        validate_and_report("extrude_l", &mesh);
    }

    // 5. High-res revolved vase
    {
        let mut store = EntityStore::new();
        let profile = vec![
            Point3d::new(3.0, 0.0, 0.0),
            Point3d::new(5.0, 0.0, 4.0),
            Point3d::new(3.5, 0.0, 8.0),
            Point3d::new(4.0, 0.0, 12.0),
        ];
        let solid = revolve_profile(
            &mut store,
            &profile,
            Point3d::ORIGIN,
            Vec3::new(0.0, 0.0, 1.0),
            std::f64::consts::TAU,
            96,
        )
        .unwrap();
        let mesh = tessellate_solid(&store, solid);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Revolved Vase (96 seg)");
        fs::write("docs/renders/revolve_vase.svg", svg).unwrap();
        validate_and_report("revolve_vase", &mesh);
        fs::write("docs/exports/vase.obj", mesh_to_obj(&mesh)).unwrap();
    }

    // 6. Chamfered box
    {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);
        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let chamfered = chamfer_edge(&mut store, box_id, v0, v1, 1.5).unwrap();
        let mesh = tessellate_solid(&store, chamfered);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Chamfered Box (d=1.5)");
        fs::write("docs/renders/chamfer_box.svg", svg).unwrap();
        validate_and_report("chamfer_box", &mesh);
    }

    // 7. High-res filleted box
    {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);
        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let filleted = fillet_edge(&mut store, box_id, v0, v1, 1.5, 32).unwrap();
        let mesh = tessellate_solid(&store, filleted);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Filleted Box (r=1.5, 32 seg)");
        fs::write("docs/renders/fillet_box.svg", svg).unwrap();
        validate_and_report("fillet_box", &mesh);
    }

    // 8. Boolean union
    {
        let mut store = EntityStore::new();
        let box_a = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);
        let box_b = make_box(&mut store, 5.0, 3.0, 2.0, 15.0, 11.0, 8.0);
        if let Ok(result) = boolean_op(&mut store, box_a, box_b, BoolOp::Union) {
            let mesh = tessellate_solid(&store, result);
            let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Boolean Union");
            fs::write("docs/renders/boolean_union.svg", svg).unwrap();
            validate_and_report("boolean_union", &mesh);
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Advanced Feature Tree Examples (high-resolution)
    // ═══════════════════════════════════════════════════════════════════════

    println!("\n=== Advanced Feature Tree Examples ===");

    // 9. Constraint-solved bracket: rectangular profile defined entirely
    //    by constraints, then extruded
    {
        let mut tree = FeatureTree::new();
        tree.add_parameter(Parameter::new("width", 12.0));
        tree.add_parameter(Parameter::new("height", 8.0));
        tree.add_parameter(Parameter::new("depth", 5.0));

        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![
                    [0.0, 0.0],   // fixed origin
                    [11.0, 0.5],  // ~12 away, ~horizontal — solver corrects
                    [11.5, 7.5],  // ~8 above p1 — solver corrects
                    [0.5, 8.5],   // ~8 above origin — solver corrects
                ],
                closed: true,
            }],
            constraints: vec![
                SketchConstraint::Fixed { point: 0, x: 0.0, y: 0.0 },
                SketchConstraint::Horizontal { line: 4 },
                SketchConstraint::Vertical { line: 5 },
                SketchConstraint::Horizontal { line: 6 },
                SketchConstraint::Vertical { line: 7 },
                SketchConstraint::Distance { point_a: 0, point_b: 1, value: 12.0 },
                SketchConstraint::Distance { point_a: 1, point_b: 2, value: 8.0 },
            ],
            lines: vec![(0, 1), (1, 2), (2, 3), (3, 0)],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("depth", 5.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("constrained bracket");
        let mesh = tessellate_solid(&store, *solids.last().unwrap());
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Constraint-Solved Bracket");
        fs::write("docs/renders/ft_bracket.svg", svg).unwrap();
        validate_and_report("ft_bracket", &mesh);
        fs::write("docs/exports/ft_bracket.obj", mesh_to_obj(&mesh)).unwrap();
    }

    // 10. Multi-fillet enclosure: box with 4 vertical edges filleted for a
    //     rounded enclosure look (high segment count for smooth curves)
    {
        let mut tree = FeatureTree::new();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![[0.0, 0.0], [14.0, 0.0], [14.0, 10.0], [0.0, 10.0]],
                closed: true,
            }],
            constraints: vec![],
            lines: vec![],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("height", 6.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        // Fillet 4 vertical edges (indices depend on edge collection order)
        // We'll fillet edges one at a time since topology changes after each
        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("enclosure base");
        let base = *solids.last().unwrap();

        // Collect vertical edges (those with same x,y but different z)
        let unique_edges = FeatureTree::collect_unique_edges(&store, base);
        let mut vertical_edges: Vec<(Point3d, Point3d)> = Vec::new();
        for (a, b) in &unique_edges {
            let dx = (a.x - b.x).abs();
            let dy = (a.y - b.y).abs();
            let dz = (a.z - b.z).abs();
            if dz > 1.0 && dx < 0.01 && dy < 0.01 {
                vertical_edges.push((*a, *b));
            }
        }

        let mut current = base;
        for (v0, v1) in &vertical_edges {
            current = fillet_edge(&mut store, current, *v0, *v1, 2.0, 32).unwrap();
        }

        let mesh = tessellate_solid(&store, current);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Rounded Enclosure (4 fillets, 32 seg)");
        fs::write("docs/renders/ft_enclosure.svg", svg).unwrap();
        validate_and_report("ft_enclosure", &mesh);
        fs::write("docs/exports/ft_enclosure.obj", mesh_to_obj(&mesh)).unwrap();
    }

    // 11. High-res wine glass: revolved profile with many control points
    //     for a realistic stem and bowl shape
    {
        let mut tree = FeatureTree::new();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![
                    // Base
                    [3.5, 0.0],
                    // Stem taper
                    [1.0, 0.5],
                    [0.8, 1.0],
                    // Stem
                    [0.6, 3.0],
                    [0.6, 6.0],
                    // Bowl curve
                    [0.8, 7.0],
                    [1.5, 8.0],
                    [3.0, 9.0],
                    [4.5, 10.0],
                    [5.5, 11.0],
                    [5.8, 12.0],
                    // Rim
                    [5.5, 13.0],
                ],
                closed: false,
            }],
            constraints: vec![],
            lines: vec![],
        });

        tree.add_feature(Feature::Revolve {
            sketch_index: 0,
            axis_origin: [0.0, 0.0, 0.0],
            axis_direction: [0.0, 0.0, 1.0],
            angle: Parameter::new("angle", std::f64::consts::TAU),
            segments: 96,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("wine glass");
        let mesh = tessellate_solid(&store, *solids.last().unwrap());
        let svg = mesh_to_svg(&mesh, 400.0, 350.0, "Wine Glass (12-pt profile, 96 seg)");
        fs::write("docs/renders/ft_wine_glass.svg", svg).unwrap();
        validate_and_report("ft_wine_glass", &mesh);
        fs::write("docs/exports/ft_wine_glass.obj", mesh_to_obj(&mesh)).unwrap();
    }

    // 12. T-bracket: extruded T-profile with a chamfered edge
    {
        let mut tree = FeatureTree::new();
        // T-shape profile
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![
                    [0.0, 0.0],
                    [12.0, 0.0],
                    [12.0, 3.0],
                    [8.0, 3.0],
                    [8.0, 10.0],
                    [4.0, 10.0],
                    [4.0, 3.0],
                    [0.0, 3.0],
                ],
                closed: true,
            }],
            constraints: vec![],
            lines: vec![],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("depth", 5.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        tree.add_feature(Feature::Chamfer {
            edge_indices: vec![0],
            distance: Parameter::new("chamfer", 1.5),
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("t-bracket");
        let mesh = tessellate_solid(&store, *solids.last().unwrap());
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "T-Bracket with Chamfer");
        fs::write("docs/renders/ft_t_bracket.svg", svg).unwrap();
        validate_and_report("ft_t_bracket", &mesh);
        fs::write("docs/exports/ft_t_bracket.obj", mesh_to_obj(&mesh)).unwrap();
    }

    // 13. Boolean subtraction: plate with cylindrical hole
    {
        let mut store = EntityStore::new();
        let plate = make_box(&mut store, 0.0, 0.0, 0.0, 16.0, 10.0, 3.0);
        let cylinder = make_cylinder(
            &mut store,
            Point3d::new(8.0, 5.0, -1.0),
            3.0,
            5.0,
            96,
        );

        if let Ok(result) = boolean_op(&mut store, plate, cylinder, BoolOp::Difference) {
            let mesh = tessellate_solid(&store, result);
            let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Plate with Hole (96 seg)");
            fs::write("docs/renders/ft_plate_hole.svg", svg).unwrap();
            validate_and_report("ft_plate_hole", &mesh);
            fs::write("docs/exports/ft_plate_hole.obj", mesh_to_obj(&mesh)).unwrap();
        }
    }

    // 14. Smooth chess pawn: revolved profile with many points
    {
        let mut store = EntityStore::new();
        let profile = vec![
            // Base
            Point3d::new(4.0, 0.0, 0.0),
            Point3d::new(4.2, 0.0, 0.5),
            Point3d::new(3.8, 0.0, 1.0),
            // Lower taper
            Point3d::new(2.5, 0.0, 1.5),
            Point3d::new(1.8, 0.0, 2.0),
            // Neck
            Point3d::new(1.2, 0.0, 3.0),
            Point3d::new(1.0, 0.0, 4.5),
            Point3d::new(1.0, 0.0, 5.5),
            // Collar
            Point3d::new(1.5, 0.0, 6.0),
            Point3d::new(1.8, 0.0, 6.5),
            Point3d::new(1.5, 0.0, 7.0),
            // Head sphere
            Point3d::new(1.2, 0.0, 7.5),
            Point3d::new(2.0, 0.0, 8.0),
            Point3d::new(2.5, 0.0, 9.0),
            Point3d::new(2.5, 0.0, 10.0),
            Point3d::new(2.0, 0.0, 11.0),
            Point3d::new(1.2, 0.0, 11.5),
            // Top tip
            Point3d::new(0.5, 0.0, 12.0),
        ];
        let solid = revolve_profile(
            &mut store,
            &profile,
            Point3d::ORIGIN,
            Vec3::new(0.0, 0.0, 1.0),
            std::f64::consts::TAU,
            96,
        )
        .unwrap();
        let mesh = tessellate_solid(&store, solid);
        let svg = mesh_to_svg(&mesh, 400.0, 350.0, "Chess Pawn (18-pt profile, 96 seg)");
        fs::write("docs/renders/ft_chess_pawn.svg", svg).unwrap();
        validate_and_report("ft_chess_pawn", &mesh);
        fs::write("docs/exports/ft_chess_pawn.obj", mesh_to_obj(&mesh)).unwrap();
    }

    // 15. Stepped shaft: two extrusions boolean-unioned
    {
        let mut tree = FeatureTree::new();

        // Large cylinder base (approximated as 24-sided polygon)
        let n_sides = 48;
        let r1 = 5.0;
        let pts1: Vec<[f64; 2]> = (0..n_sides)
            .map(|i| {
                let theta = std::f64::consts::TAU * i as f64 / n_sides as f64;
                [r1 * theta.cos(), r1 * theta.sin()]
            })
            .collect();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile { points: pts1, closed: true }],
            constraints: vec![],
            lines: vec![],
        });
        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("base_height", 8.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        // Smaller step
        let r2 = 3.0;
        let pts2: Vec<[f64; 2]> = (0..n_sides)
            .map(|i| {
                let theta = std::f64::consts::TAU * i as f64 / n_sides as f64;
                [r2 * theta.cos(), r2 * theta.sin()]
            })
            .collect();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile { points: pts2, closed: true }],
            constraints: vec![],
            lines: vec![],
        });
        tree.add_feature(Feature::Extrude {
            sketch_index: 1,
            distance: Parameter::new("step_height", 16.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        tree.add_feature(Feature::BooleanOp {
            op_type: BooleanOpType::Union,
            tool_feature: 0,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("stepped shaft");
        let mesh = tessellate_solid(&store, *solids.last().unwrap());
        let svg = mesh_to_svg(&mesh, 400.0, 350.0, "Stepped Shaft (48-gon, boolean union)");
        fs::write("docs/renders/ft_stepped_shaft.svg", svg).unwrap();
        validate_and_report("ft_stepped_shaft", &mesh);
        fs::write("docs/exports/ft_stepped_shaft.obj", mesh_to_obj(&mesh)).unwrap();
    }

    println!("\nSVGs written to docs/renders/");
    println!("OBJ/STL files written to docs/exports/");
}
