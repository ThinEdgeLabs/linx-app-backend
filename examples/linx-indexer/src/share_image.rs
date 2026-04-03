use std::sync::LazyLock;

use anyhow::{Result, anyhow};
use tiny_skia::Pixmap;
use usvg::fontdb;

const PORTRAIT_SCALE: f32 = 2.0;
const LANDSCAPE_SCALE: f32 = 1.0;
const DEFAULT_PORTRAIT_PATH: &str = "assets/share_background.svg";
const DEFAULT_LANDSCAPE_PATH: &str = "assets/share_background_landscape.svg";

#[derive(Debug, Clone, Copy)]
pub enum ImageFormat {
    Portrait,
    Landscape,
}

fn pre_render_background(env_var: &str, default_path: &str, scale: f32) -> Result<Pixmap, String> {
    let path = std::env::var(env_var).unwrap_or_else(|_| default_path.to_string());
    let svg_str = std::fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {}", path, e))?;
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_str(&svg_str, &options).map_err(|e| format!("Failed to parse SVG: {}", e))?;
    let size = tree.size();
    let width = (size.width() * scale) as u32;
    let height = (size.height() * scale) as u32;
    let mut pixmap = Pixmap::new(width, height).ok_or("Failed to create pixmap")?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Ok(pixmap)
}

static BACKGROUND_PORTRAIT: LazyLock<Result<Pixmap, String>> =
    LazyLock::new(|| pre_render_background("SHARE_TEMPLATE_PATH", DEFAULT_PORTRAIT_PATH, PORTRAIT_SCALE));

static BACKGROUND_LANDSCAPE: LazyLock<Result<Pixmap, String>> =
    LazyLock::new(|| pre_render_background("SHARE_TEMPLATE_LANDSCAPE_PATH", DEFAULT_LANDSCAPE_PATH, LANDSCAPE_SCALE));

static FONTDB: LazyLock<fontdb::Database> = LazyLock::new(|| {
    let mut db = fontdb::Database::new();
    // Try loading fonts from common Linux paths first (for Docker containers),
    // then fall back to system fonts (for macOS/local dev)
    let font_dirs = ["/usr/share/fonts", "/usr/local/share/fonts"];
    for dir in &font_dirs {
        if std::path::Path::new(dir).exists() {
            db.load_fonts_dir(dir);
        }
    }
    if db.is_empty() {
        db.load_system_fonts();
    }
    tracing::info!("Loaded {} font faces", db.len());
    db
});

pub fn generate_share_image(points: i32, referral_code: &str, format: ImageFormat) -> Result<Vec<u8>> {
    let (background, scale, text_svg) = match format {
        ImageFormat::Portrait => {
            let bg = BACKGROUND_PORTRAIT.as_ref().map_err(|e| anyhow!("Portrait background not available: {}", e))?;
            let formatted_points = format_with_commas(points);
            let svg = format!(
                r#"<svg width="382" height="516" xmlns="http://www.w3.org/2000/svg">
  <text x="191" y="155" text-anchor="middle" fill="white" font-size="40" font-weight="bold" font-family="Arial, Liberation Sans, Helvetica, sans-serif">{formatted_points}</text>
  <text x="191" y="487" text-anchor="middle" fill="white" font-size="18" font-weight="bold" font-family="Arial, Liberation Sans, Helvetica, sans-serif">{referral_code}</text>
</svg>"#
            );
            (bg, PORTRAIT_SCALE, svg)
        }
        ImageFormat::Landscape => {
            let bg = BACKGROUND_LANDSCAPE.as_ref().map_err(|e| anyhow!("Landscape background not available: {}", e))?;
            let formatted_points = format_with_commas(points);
            let svg = format!(
                r#"<svg width="1200" height="630" xmlns="http://www.w3.org/2000/svg">
  <text x="325" y="313" text-anchor="middle" fill="white" font-size="60" font-weight="bold" font-family="Arial, Liberation Sans, Helvetica, sans-serif">{formatted_points}</text>
  <text x="325" y="528" text-anchor="middle" fill="white" font-size="24" font-weight="bold" font-family="Arial, Liberation Sans, Helvetica, sans-serif">{referral_code}</text>
</svg>"#
            );
            (bg, LANDSCAPE_SCALE, svg)
        }
    };

    let mut pixmap = background.clone();

    let mut options = usvg::Options::default();
    *options.fontdb_mut() = FONTDB.clone();

    let text_tree = usvg::Tree::from_str(&text_svg, &options)?;
    resvg::render(&text_tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());

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
    #[ignore = "requires share_background.svg asset"]
    fn test_generate_share_image() {
        unsafe {
            std::env::set_var(
                "SHARE_TEMPLATE_PATH",
                concat!(env!("CARGO_MANIFEST_DIR"), "/assets/share_background.svg"),
            );
        }
        let result = generate_share_image(65992, "CLEAN-RAVEN-730", ImageFormat::Portrait);
        assert!(result.is_ok());
        let png_bytes = result.unwrap();
        // Verify PNG magic bytes
        assert_eq!(&png_bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }
}
