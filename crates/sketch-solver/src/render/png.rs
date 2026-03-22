//! PNG rasterization of SVG content via resvg.

use std::sync::Arc;

/// Render an SVG string to PNG bytes at the given dimensions.
///
/// Uses resvg + usvg + tiny-skia for rasterization. Loads system fonts
/// so that text elements (constraint badges, status labels) render correctly.
///
/// Note: PNG output may vary across machines depending on available fonts.
/// SVG output is the canonical golden format; PNGs are for convenience only.
pub fn render_sketch_png(svg_content: &str, width: u32, height: u32) -> Vec<u8> {
    let mut fontdb = usvg::fontdb::Database::new();
    fontdb.load_system_fonts();
    fontdb.set_sans_serif_family("DejaVu Sans");

    let options = usvg::Options {
        fontdb: Arc::new(fontdb),
        ..usvg::Options::default()
    };

    let tree = usvg::Tree::from_str(svg_content, &options).expect("Failed to parse SVG");

    let mut pixmap = tiny_skia::Pixmap::new(width, height).expect("Failed to create pixmap");
    pixmap.fill(tiny_skia::Color::WHITE);

    let tree_size = tree.size();
    let sx = width as f32 / tree_size.width();
    let sy = height as f32 / tree_size.height();
    let scale = sx.min(sy);

    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    pixmap.encode_png().expect("Failed to encode PNG")
}
