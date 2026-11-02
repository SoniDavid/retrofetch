//! Reads `sprites/manifest.toml` and generates `$OUT_DIR/sprites.rs`, a static
//! table of every sprite whose PNG exists in `sprites/png/`, embedded with
//! `include_bytes!`.

use std::collections::HashSet;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const CONSOLES: &[&str] = &["snes", "genesis", "arcade", "neogeo", "pce"];

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    sprite: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    id: String,
    name: String,
    game: String,
    company: String,
    console: String,
    year: u16,
    #[serde(default)]
    crop: Option<[u32; 4]>,
    #[serde(default)]
    url: Option<String>,
}

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let sprites_dir = crate_dir.join("../sprites");
    let manifest_path = sprites_dir.join("manifest.toml");
    let png_dir = sprites_dir.join("png");

    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed={}", png_dir.display());

    let text = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", manifest_path.display()));
    let manifest: Manifest =
        toml::from_str(&text).unwrap_or_else(|e| panic!("invalid manifest.toml: {e}"));

    let mut seen = HashSet::new();
    let mut code = String::from("&[\n");
    let mut included = 0usize;

    for e in &manifest.sprite {
        assert!(seen.insert(e.id.as_str()), "duplicate sprite id `{}`", e.id);
        assert!(
            CONSOLES.contains(&e.console.as_str()),
            "sprite `{}`: unknown console `{}` (expected one of {CONSOLES:?})",
            e.id,
            e.console
        );

        let png = png_dir.join(format!("{}.png", e.id));
        println!("cargo:rerun-if-changed={}", png.display());
        if !png.is_file() {
            // Entries with a crop box must have been cropped; entries without one
            // are roster placeholders still waiting for a sheet.
            assert!(
                e.crop.is_none() && e.url.is_none(),
                "sprite `{}` has a crop box/url but {} is missing — run `cargo xtask fetch && cargo xtask crop`",
                e.id,
                png.display()
            );
            continue;
        }

        included += 1;
        writeln!(
            code,
            "    Sprite {{ id: {:?}, name: {:?}, game: {:?}, company: {:?}, console: {:?}, year: {}, png: include_bytes!({:?}) }},",
            e.id, e.name, e.game, e.company, e.console, e.year, abs(&png)
        )
        .unwrap();
    }
    code.push_str("]\n");

    assert!(
        included > 0,
        "no sprites with PNGs found in {}",
        png_dir.display()
    );

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("sprites.rs");
    fs::write(out, code).unwrap();
}

fn abs(p: &Path) -> String {
    p.canonicalize().unwrap().display().to_string()
}
