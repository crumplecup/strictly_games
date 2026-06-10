//! Verify that card SVG/PNG assets can be resolved and loaded from disk.
//!
//! Tests the path-construction logic used by `bridge_image` in `elicit_egui`
//! without requiring a GPU or window.

use std::path::PathBuf;

/// Reproduce the exact URI-construction logic from `bridge_image`.
fn make_file_uri(rel_path: &str) -> String {
    let abs = strictly_server::assets::card_absolute_path(rel_path);
    format!("file://{}", abs.display())
}

/// Extract the filesystem path from a `file://` URI.
fn uri_to_path(uri: &str) -> &str {
    uri.strip_prefix("file://").unwrap_or(uri)
}

#[test]
fn card_asset_paths_resolve_to_existing_files() {
    // Run from the workspace root where `assets/` lives.
    // If the working dir is wrong, all paths will fail together — good signal.
    let cwd = std::env::current_dir().expect("current_dir");
    eprintln!("working dir: {}", cwd.display());

    let samples: &[&str] = &[
        "assets/cards/Ace_of_spades.svg",
        "assets/cards/9_of_diamonds.svg",
        "assets/cards/King_of_hearts2.svg",
        "assets/cards/Jack_of_clubs2.svg",
        "assets/cards/10_of_clubs.svg",
        "assets/cards/USPCC_card_backs_Bicycle_Rider.png",
    ];

    let mut all_ok = true;
    for rel in samples {
        let uri = make_file_uri(rel);
        let path = uri_to_path(&uri);
        let exists = std::path::Path::new(path).exists();
        eprintln!("uri={uri}  path={path}  exists={exists}");
        if !exists {
            all_ok = false;
        }
    }

    assert!(
        all_ok,
        "one or more card assets not found — see stderr for details"
    );
}

#[test]
fn card_asset_bytes_are_readable() {
    let samples: &[(&str, &str)] = &[
        ("assets/cards/9_of_diamonds.svg", "svg"),
        ("assets/cards/USPCC_card_backs_Bicycle_Rider.png", "png"),
    ];

    for (rel, kind) in samples {
        let uri = make_file_uri(rel);
        let path = uri_to_path(&uri);
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
        assert!(!bytes.is_empty(), "{path} is empty");

        match *kind {
            "svg" => {
                let text = std::str::from_utf8(&bytes).expect("SVG should be valid UTF-8");
                assert!(
                    text.contains("<svg") || text.contains("<?xml"),
                    "{path} does not look like SVG (first 100 chars: {})",
                    &text[..text.len().min(100)]
                );
            }
            "png" => {
                assert_eq!(
                    &bytes[0..4],
                    b"\x89PNG",
                    "{path} does not start with PNG magic bytes"
                );
            }
            _ => {}
        }
        eprintln!("ok: {path} ({} bytes)", bytes.len());
    }
}

#[test]
fn uri_format_has_three_slashes_for_absolute_path() {
    // file:// + /absolute/path = file:///absolute/path
    // egui_extras and most URI parsers require the triple slash.
    let uri = make_file_uri("assets/cards/9_of_diamonds.svg");
    eprintln!("uri={uri}");
    assert!(
        uri.starts_with("file:///"),
        "expected file:/// (absolute path) but got: {uri}"
    );
}
