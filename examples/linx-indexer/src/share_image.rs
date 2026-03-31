use std::sync::LazyLock;

use anyhow::Result;
use tiny_skia::Pixmap;
use usvg::fontdb;

const SCALE: f32 = 3.0;

/// Pre-rendered background pixmap (expensive, done once on first use)
static BACKGROUND: LazyLock<Pixmap> = LazyLock::new(|| {
    let svg_str = include_str!("../assets/share_background.svg");
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg_str, &options).expect("Failed to parse background SVG");
    let size = tree.size();
    let width = (size.width() * SCALE) as u32;
    let height = (size.height() * SCALE) as u32;
    let mut pixmap = Pixmap::new(width, height).expect("Failed to create background pixmap");
    resvg::render(&tree, tiny_skia::Transform::from_scale(SCALE, SCALE), &mut pixmap.as_mut());
    pixmap
});

static FONTDB: LazyLock<fontdb::Database> = LazyLock::new(|| {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    db
});

pub fn generate_share_image(points: i32, referral_code: &str) -> Result<Vec<u8>> {
    // Clone the pre-rendered background
    let mut pixmap = BACKGROUND.clone();

    // Render a small SVG with just the dynamic text
    let formatted_points = format_with_commas(points);
    let text_svg = format!(
        r#"<svg width="382" height="516" xmlns="http://www.w3.org/2000/svg">
  <text x="191" y="155" text-anchor="middle" fill="white" font-size="40" font-weight="bold" font-family="Arial, Helvetica, sans-serif">{formatted_points}</text>
  <text x="191" y="487" text-anchor="middle" fill="white" font-size="18" font-weight="bold" font-family="Arial, Helvetica, sans-serif">{referral_code}</text>
</svg>"#
    );

    let mut options = usvg::Options::default();
    *options.fontdb_mut() = FONTDB.clone();

    let text_tree = usvg::Tree::from_str(&text_svg, &options)?;
    resvg::render(&text_tree, tiny_skia::Transform::from_scale(SCALE, SCALE), &mut pixmap.as_mut());

    let png_bytes = pixmap.encode_png()?;
    Ok(png_bytes)
}

fn format_with_commas(n: i32) -> String {
    if n < 0 {
        return format!("-{}", format_with_commas_u64(-(n as i64) as u64));
    }
    format_with_commas_u64(n as u64)
}

fn format_with_commas_u64(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_with_commas() {
        assert_eq!(format_with_commas(0), "0");
        assert_eq!(format_with_commas(999), "999");
        assert_eq!(format_with_commas(1000), "1,000");
        assert_eq!(format_with_commas(65992), "65,992");
        assert_eq!(format_with_commas(1234567), "1,234,567");
        assert_eq!(format_with_commas(-1), "-1");
        assert_eq!(format_with_commas(-1000), "-1,000");
        assert_eq!(format_with_commas(i32::MIN), "-2,147,483,648");
    }

    #[test]
    fn test_generate_share_image() {
        let result = generate_share_image(65992, "CLEAN-RAVEN-730");
        assert!(result.is_ok());
        let png_bytes = result.unwrap();
        // Verify PNG magic bytes
        assert_eq!(&png_bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }
}
