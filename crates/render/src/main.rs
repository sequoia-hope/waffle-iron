//! Generate SVG wireframe renders of CAD primitives for the README.

use cad_kernel::geometry::point::Point3d;
use cad_kernel::geometry::vector::Vec3;
use cad_kernel::operations::chamfer::chamfer_edge;
use cad_kernel::operations::extrude::{extrude_profile, Profile};
use cad_kernel::operations::fillet::fillet_edge;
use cad_kernel::operations::revolve::revolve_profile;
use cad_kernel::topology::brep::EntityStore;
use cad_kernel::topology::primitives::{make_box, make_cylinder, make_sphere};
use cad_tessellation::{mesh_to_obj, mesh_to_stl, tessellate_solid, TriangleMesh};
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
    }

    let num_tris = mesh.indices.len() / 3;
    let mut tris: Vec<TriInfo> = Vec::with_capacity(num_tris);

    let light_dir = (0.3_f64, -0.5_f64, 0.8_f64);
    let light_len = (light_dir.0 * light_dir.0 + light_dir.1 * light_dir.1 + light_dir.2 * light_dir.2).sqrt();

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

        let dot = (nx * light_dir.0 + ny * light_dir.1 + nz * light_dir.2) / (nlen * light_len);
        let brightness = 0.3 + 0.7 * dot.abs().min(1.0);

        tris.push(TriInfo { i0, i1, i2, depth, brightness });
    }

    tris.sort_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap());

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
        let r = (100.0 * b) as u8;
        let g = (160.0 * b) as u8;
        let bl = (220.0 * b) as u8;

        svg.push_str(&format!(
            "  <polygon points=\"{x0:.1},{y0:.1} {x1:.1},{y1:.1} {x2:.1},{y2:.1}\" \
             fill=\"rgb({r},{g},{bl})\" stroke=\"#2a2a4a\" stroke-width=\"0.5\"/>\n"
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

fn main() {
    fs::create_dir_all("docs/renders").expect("create docs/renders dir");

    // 1. Box
    {
        let mut store = EntityStore::new();
        let solid = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);
        let mesh = tessellate_solid(&store, solid);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Box (10 x 8 x 6)");
        fs::write("docs/renders/box.svg", svg).unwrap();
        println!("box: {} tris, {} verts", mesh.triangle_count(), mesh.vertex_count());
    }

    // 2. Cylinder
    {
        let mut store = EntityStore::new();
        let solid = make_cylinder(&mut store, Point3d::ORIGIN, 5.0, 12.0, 24);
        let mesh = tessellate_solid(&store, solid);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Cylinder (r=5, h=12, 24 segments)");
        fs::write("docs/renders/cylinder.svg", svg).unwrap();
        println!("cylinder: {} tris, {} verts", mesh.triangle_count(), mesh.vertex_count());
    }

    // 3. Sphere
    {
        let mut store = EntityStore::new();
        let solid = make_sphere(&mut store, Point3d::ORIGIN, 6.0, 16, 12);
        let mesh = tessellate_solid(&store, solid);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Sphere (r=6, 16x12)");
        fs::write("docs/renders/sphere.svg", svg).unwrap();
        println!("sphere: {} tris, {} verts", mesh.triangle_count(), mesh.vertex_count());
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
        let solid = extrude_profile(&mut store, &profile, Vec3::Z, 4.0);
        let mesh = tessellate_solid(&store, solid);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Extruded L-Profile (h=4)");
        fs::write("docs/renders/extrude_l.svg", svg).unwrap();
        println!("extrude_l: {} tris, {} verts", mesh.triangle_count(), mesh.vertex_count());
    }

    // 5. Revolved shape (vase-like)
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
            24,
        );
        let mesh = tessellate_solid(&store, solid);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Revolved Profile (vase, 24 seg)");
        fs::write("docs/renders/revolve_vase.svg", svg).unwrap();
        println!("revolve_vase: {} tris, {} verts", mesh.triangle_count(), mesh.vertex_count());
    }

    // 6. Chamfered box
    {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);
        // Chamfer the front-bottom edge
        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let chamfered = chamfer_edge(&mut store, box_id, v0, v1, 1.5);
        let mesh = tessellate_solid(&store, chamfered);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Chamfered Box (d=1.5)");
        fs::write("docs/renders/chamfer_box.svg", svg).unwrap();
        println!("chamfer_box: {} tris, {} verts", mesh.triangle_count(), mesh.vertex_count());
    }

    // 7. Filleted box
    {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);
        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let filleted = fillet_edge(&mut store, box_id, v0, v1, 1.5, 6);
        let mesh = tessellate_solid(&store, filleted);
        let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Filleted Box (r=1.5, 6 seg)");
        fs::write("docs/renders/fillet_box.svg", svg).unwrap();
        println!("fillet_box: {} tris, {} verts", mesh.triangle_count(), mesh.vertex_count());
    }

    // 8. Boolean union of two boxes
    {
        use cad_kernel::boolean::engine::{boolean_op, BoolOp};
        let mut store = EntityStore::new();
        let box_a = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);
        let box_b = make_box(&mut store, 5.0, 3.0, 2.0, 15.0, 11.0, 8.0);
        if let Ok(result) = boolean_op(&mut store, box_a, box_b, BoolOp::Union) {
            let mesh = tessellate_solid(&store, result);
            let svg = mesh_to_svg(&mesh, 400.0, 300.0, "Boolean Union");
            fs::write("docs/renders/boolean_union.svg", svg).unwrap();
            println!("boolean_union: {} tris, {} verts", mesh.triangle_count(), mesh.vertex_count());
        }
    }

    // Export OBJ files for 3D viewing
    fs::create_dir_all("docs/exports").expect("create docs/exports dir");
    {
        let mut store = EntityStore::new();
        let solid = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);
        let mesh = tessellate_solid(&store, solid);
        fs::write("docs/exports/box.obj", mesh_to_obj(&mesh)).unwrap();
        fs::write("docs/exports/box.stl", mesh_to_stl(&mesh)).unwrap();
    }
    {
        let mut store = EntityStore::new();
        let solid = make_sphere(&mut store, Point3d::ORIGIN, 6.0, 16, 12);
        let mesh = tessellate_solid(&store, solid);
        fs::write("docs/exports/sphere.obj", mesh_to_obj(&mesh)).unwrap();
    }
    {
        let mut store = EntityStore::new();
        let profile = vec![
            Point3d::new(3.0, 0.0, 0.0),
            Point3d::new(5.0, 0.0, 4.0),
            Point3d::new(3.5, 0.0, 8.0),
            Point3d::new(4.0, 0.0, 12.0),
        ];
        let solid = revolve_profile(
            &mut store, &profile, Point3d::ORIGIN,
            Vec3::new(0.0, 0.0, 1.0), std::f64::consts::TAU, 24,
        );
        let mesh = tessellate_solid(&store, solid);
        fs::write("docs/exports/vase.obj", mesh_to_obj(&mesh)).unwrap();
    }

    println!("\nSVGs written to docs/renders/");
    println!("OBJ/STL files written to docs/exports/");
}
