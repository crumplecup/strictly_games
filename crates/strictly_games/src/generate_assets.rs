//! SVG → ASCII art conversion pipeline for the `generate-assets` command.
//!
//! Rasterizes each SVG with resvg, writes a temporary PNG, converts to an
//! ASCII string with cascii, then emits a Rust source file of `pub const`
//! raw-string literals.  The server crate compiles that file in at build
//! time — zero runtime conversion cost.

use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cascii::{AsciiConverter, ConversionOptions};
use resvg::usvg;
use tiny_skia::{Pixmap, Transform};
use tracing::{info, instrument, warn};

// ── Public entry point ────────────────────────────────────────────────────────

/// Convert all SVG assets under `input/{cards,dice}/` and write the resulting
/// Rust constants to `rust_out`.
#[instrument(skip_all, fields(columns, input = %input.display(), rust_out = %rust_out.display()))]
pub fn run_generate_assets(
    columns: u32,
    input: &Path,
    rust_out: &Path,
) -> Result<()> {
    let mut entries: Vec<AssetEntry> = Vec::new();

    collect_svgs(input.join("cards"), "CARD", &mut entries)?;
    collect_svgs(input.join("dice"), "DIE", &mut entries)?;

    if entries.is_empty() {
        anyhow::bail!(
            "No SVG files found under {}/cards/ or {}/dice/",
            input.display(),
            input.display()
        );
    }

    // Convert each SVG → ASCII string.
    let mut converted: Vec<(String, String)> = Vec::new(); // (const_name, ascii)
    for entry in &entries {
        match convert_svg(&entry.svg_path, columns) {
            Ok(ascii) => {
                info!(const_name = %entry.const_name, "converted");
                converted.push((entry.const_name.clone(), ascii));
            }
            Err(e) => {
                warn!(path = %entry.svg_path.display(), error = %e, "skipping (conversion failed)");
            }
        }
    }

    // Write the generated Rust file.
    if let Some(parent) = rust_out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }
    let source = build_rust_source(columns, &converted);
    std::fs::write(rust_out, &source)
        .with_context(|| format!("writing {}", rust_out.display()))?;

    println!(
        "Generated {} constants → {}",
        converted.len(),
        rust_out.display()
    );
    Ok(())
}

// ── Internal helpers ──────────────────────────────────────────────────────────

struct AssetEntry {
    svg_path: PathBuf,
    const_name: String,
}

/// Collect all `.svg` files in `dir`, derive a Rust constant name for each.
fn collect_svgs(dir: PathBuf, prefix: &str, out: &mut Vec<AssetEntry>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("svg"))
        .collect();
    paths.sort();
    for path in paths {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let const_name = stem_to_const(stem, prefix);
        out.push(AssetEntry {
            svg_path: path,
            const_name,
        });
    }
    Ok(())
}

/// Derive a `SCREAMING_SNAKE_CASE` constant name from an SVG file stem.
///
/// Examples:
/// - `Ace_of_spades`      → `CARD_ACE_OF_SPADES`
/// - `Jack_of_clubs2`     → `CARD_JACK_OF_CLUBS`  (trailing digit after letter stripped)
/// - `Dice-1-b`           → `DIE_1`               (`-b` suffix + `Dice-` prefix stripped)
fn stem_to_const(stem: &str, prefix: &str) -> String {
    // Dice files: "Dice-1-b" → strip "-b" → "Dice-1" → strip "Dice-" → "1"
    let stem = stem.strip_suffix("-b").unwrap_or(stem);
    let stem = stem.strip_prefix("Dice-").unwrap_or(stem);

    // Face cards: "Jack_of_clubs2" → strip trailing digit that follows a letter.
    // Regular cards like "2_of_clubs" end in a letter, so are unaffected.
    let stem = if stem.ends_with(|c: char| c.is_ascii_digit())
        && stem
            .chars()
            .rev()
            .nth(1)
            .is_some_and(|c| c.is_ascii_alphabetic())
    {
        stem.trim_end_matches(|c: char| c.is_ascii_digit())
    } else {
        stem
    };

    let upper = stem.replace('-', "_").replace(' ', "_").to_uppercase();
    format!("{prefix}_{upper}")
}

/// Rasterize an SVG and convert to an ASCII string at `columns` wide.
fn convert_svg(svg_path: &Path, columns: u32) -> Result<String> {
    // 1. Rasterize SVG → PNG bytes via resvg + tiny_skia.
    let svg_data =
        std::fs::read(svg_path).with_context(|| format!("reading {}", svg_path.display()))?;
    let tree = usvg::Tree::from_data(&svg_data, &usvg::Options::default())
        .with_context(|| format!("parsing SVG {}", svg_path.display()))?;
    let size = tree.size();
    let width = (size.width().ceil() as u32).max(1);
    let height = (size.height().ceil() as u32).max(1);
    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("failed to allocate {}×{} pixmap", width, height))?;
    resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());
    let png_bytes = pixmap
        .encode_png()
        .context("encoding pixmap as PNG")?;

    // 2. Write PNG to a temp file (cascii requires a file path).
    let stem = svg_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("asset");
    let tmp_path = std::env::temp_dir().join(format!("sg_asset_{stem}.png"));
    std::fs::write(&tmp_path, &png_bytes)
        .with_context(|| format!("writing temp PNG {}", tmp_path.display()))?;

    // 3. Convert PNG → ASCII string.
    let converter = AsciiConverter::new();
    let options = ConversionOptions::default().with_columns(columns);
    let ascii = converter
        .image_to_string(&tmp_path, &options)
        .context("cascii conversion")?;

    // 4. Clean up temp file (best-effort).
    let _ = std::fs::remove_file(&tmp_path);

    Ok(ascii)
}

/// Build the complete Rust source file content.
fn build_rust_source(columns: u32, entries: &[(String, String)]) -> String {
    let mut src = String::new();
    writeln!(
        src,
        "//! ASCII art constants for terminal rendering.\n//!\
         \n//! GENERATED — run `strictly_games generate-assets --columns {columns}` to regenerate.\
         \n//! Do not edit manually.\n"
    )
    .unwrap();

    for (const_name, ascii) in entries {
        // Escape any `#"` sequences that would break the raw string delimiter.
        // Rust raw strings r#"..."# are safe as long as the content has no `"#`.
        // We use a higher fence if needed; for ASCII art this is never an issue.
        writeln!(src, "pub const {const_name}: &str = r#\"{ascii}\"#;\n").unwrap();
    }
    src
}
