//! Dev tools for curating sprites: `cargo xtask detect|crop|check`.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, ensure, Context, Result};
use clap::{Parser, Subcommand};
use image::{imageops, Rgba, RgbaImage};
use serde::Deserialize;

const CONSOLES: &[&str] = &["snes", "genesis", "arcade", "neogeo", "pce"];
const MAX_HEIGHT: u32 = 64;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Find individual sprites on a sheet; prints boxes and writes <sheet>.annotated.png
    Detect {
        sheet: PathBuf,
        /// Background colour(s) to ignore (default: the sheet's top-left pixel)
        #[arg(long, value_delimiter = ',')]
        bg: Vec<String>,
        /// Pixels within this distance are merged into one sprite
        #[arg(long, default_value_t = 2)]
        gap: u32,
        /// Ignore boxes smaller than this in both dimensions
        #[arg(long, default_value_t = 8)]
        min: u32,
    },
    /// Download sheets for entries that have a direct `url` into sheets/
    Fetch {
        /// Re-download even if the sheet already exists
        #[arg(long)]
        force: bool,
    },
    /// Crop manifest entries from sheets/ into sprites/png/
    Crop {
        /// Only crop this sprite id
        #[arg(long)]
        id: Option<String>,
    },
    /// Validate the manifest and report roster status
    Check,
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    sprite: Vec<Entry>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Entry {
    id: String,
    name: String,
    game: String,
    company: String,
    console: String,
    year: u16,
    source: Option<String>,
    /// Direct image URL that `cargo xtask fetch` may download into sheets/
    url: Option<String>,
    sheet: Option<String>,
    crop: Option<[u32; 4]>,
    #[serde(default)]
    bg: Colors,
    /// Only key out background pixels connected to the image border
    #[serde(default)]
    flood: bool,
    downscale: Option<u32>,
}

impl Entry {
    /// Entries cropped by `xtask crop`: an explicit box, or a whole-image download.
    fn croppable(&self) -> bool {
        self.sheet.is_some() && (self.crop.is_some() || self.url.is_some())
    }
}

/// `bg = "#00ff00"` or `bg = ["#00ff00", "#ffffff"]`.
#[derive(Deserialize, Default)]
#[serde(untagged)]
enum Colors {
    #[default]
    None,
    One(String),
    Many(Vec<String>),
}

impl Colors {
    fn list(&self) -> Vec<String> {
        match self {
            Colors::None => vec![],
            Colors::One(c) => vec![c.clone()],
            Colors::Many(v) => v.clone(),
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn load_manifest() -> Result<Vec<Entry>> {
    let path = root().join("sprites/manifest.toml");
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    Ok(toml::from_str::<Manifest>(&text)
        .context("parsing manifest.toml")?
        .sprite)
}

fn parse_hex(s: &str) -> Result<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    ensure!(h.len() == 6, "bad colour `{s}`, expected #rrggbb");
    let v = u32::from_str_radix(h, 16).with_context(|| format!("bad colour `{s}`"))?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// Background test: explicit colours, else the sheet's top-left pixel.
struct Bg(Vec<[u8; 3]>);

impl Bg {
    fn new(sheet: &RgbaImage, colours: &[String]) -> Result<Bg> {
        let mut v = colours
            .iter()
            .map(|c| parse_hex(c))
            .collect::<Result<Vec<_>>>()?;
        if v.is_empty() {
            let [r, g, b, a] = sheet.get_pixel(0, 0).0;
            if a >= 128 {
                v.push([r, g, b]);
            }
        }
        Ok(Bg(v))
    }
    fn is_bg(&self, p: &Rgba<u8>) -> bool {
        let [r, g, b, a] = p.0;
        a < 128 || self.0.contains(&[r, g, b])
    }
}

fn open(path: &Path) -> Result<RgbaImage> {
    Ok(image::open(path)
        .with_context(|| format!("opening {}", path.display()))?
        .to_rgba8())
}

fn main() -> ExitCode {
    let res = match Cli::parse().cmd {
        Cmd::Detect {
            sheet,
            bg,
            gap,
            min,
        } => detect(&sheet, &bg, gap, min),
        Cmd::Fetch { force } => fetch(force),
        Cmd::Crop { id } => crop(id.as_deref()),
        Cmd::Check => check(),
    };
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e:#}");
            ExitCode::FAILURE
        }
    }
}

// ---------------------------------------------------------------- detect

#[derive(Clone, Copy, Debug, PartialEq)]
struct BBox {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Connected components of foreground pixels, where pixels up to `gap` apart
/// (Chebyshev distance) count as connected. Boxes are tight around real pixels.
fn find_boxes(img: &RgbaImage, bg: &Bg, gap: u32) -> Vec<BBox> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let fg: Vec<bool> = img.pixels().map(|p| !bg.is_bg(p)).collect();

    // Dilate by `gap` (separable max filter) so nearby blobs join up.
    let r = gap as usize;
    let mut tmp = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let (a, b) = (x.saturating_sub(r), (x + r).min(w - 1));
            tmp[y * w + x] = (a..=b).any(|xx| fg[y * w + xx]);
        }
    }
    let mut dil = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let (a, b) = (y.saturating_sub(r), (y + r).min(h - 1));
            dil[y * w + x] = (a..=b).any(|yy| tmp[yy * w + x]);
        }
    }

    let mut seen = vec![false; w * h];
    let mut boxes = Vec::new();
    let mut queue = VecDeque::new();
    for start in 0..w * h {
        if !dil[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        queue.push_back(start);
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % w, i / w);
            if fg[i] {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
            let neighbours = [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then(|| i + w),
            ];
            for n in neighbours.into_iter().flatten() {
                if dil[n] && !seen[n] {
                    seen[n] = true;
                    queue.push_back(n);
                }
            }
        }
        if x0 != usize::MAX {
            boxes.push(BBox {
                x: x0 as u32,
                y: y0 as u32,
                w: (x1 - x0 + 1) as u32,
                h: (y1 - y0 + 1) as u32,
            });
        }
    }
    boxes.sort_by_key(|b| (b.y, b.x));
    boxes
}

fn detect(sheet: &Path, bg: &[String], gap: u32, min: u32) -> Result<()> {
    let img = open(sheet)?;
    let bg = Bg::new(&img, bg)?;
    let boxes: Vec<BBox> = find_boxes(&img, &bg, gap)
        .into_iter()
        .filter(|b| b.w >= min || b.h >= min)
        .collect();

    println!(
        "{} sprites on {} ({}x{})",
        boxes.len(),
        sheet.display(),
        img.width(),
        img.height()
    );
    println!("{:>4}  {:>5} {:>5} {:>4} {:>4}", "idx", "x", "y", "w", "h");
    for (i, b) in boxes.iter().enumerate() {
        let flag = if b.h > MAX_HEIGHT { "  (tall)" } else { "" };
        println!("{i:>4}  {:>5} {:>5} {:>4} {:>4}{flag}", b.x, b.y, b.w, b.h);
    }

    let out = annotate(&img, &boxes);
    let out_path = sheet.with_extension("annotated.png");
    out.save(&out_path)?;
    println!("annotated: {}", out_path.display());
    Ok(())
}

/// 3x5 bitmap digits, one u16 per glyph, rows top to bottom, 3 bits each.
const DIGITS: [u16; 10] = [
    0b111_101_101_101_111,
    0b010_110_010_010_111,
    0b111_001_111_100_111,
    0b111_001_111_001_111,
    0b101_101_111_001_001,
    0b111_100_111_001_111,
    0b111_100_111_101_111,
    0b111_001_010_010_010,
    0b111_101_111_101_111,
    0b111_101_111_001_111,
];

/// Upscale 2x, draw a red box around each sprite and its index above it.
fn annotate(img: &RgbaImage, boxes: &[BBox]) -> RgbaImage {
    const S: u32 = 2;
    let mut out = imageops::resize(img, img.width() * S, img.height() * S, imageops::Nearest);
    let red = Rgba([255, 0, 64, 255]);
    let label_bg = Rgba([0, 0, 0, 255]);
    let put = |out: &mut RgbaImage, x: i64, y: i64, c: Rgba<u8>| {
        if x >= 0 && y >= 0 && (x as u32) < out.width() && (y as u32) < out.height() {
            out.put_pixel(x as u32, y as u32, c);
        }
    };
    for (i, b) in boxes.iter().enumerate() {
        let (x0, y0) = (b.x as i64 * S as i64 - 1, b.y as i64 * S as i64 - 1);
        let (x1, y1) = ((b.x + b.w) as i64 * S as i64, (b.y + b.h) as i64 * S as i64);
        for x in x0..=x1 {
            put(&mut out, x, y0, red);
            put(&mut out, x, y1, red);
        }
        for y in y0..=y1 {
            put(&mut out, x0, y, red);
            put(&mut out, x1, y, red);
        }
        // Label: digits at 2x scale, on a black plate just above the box.
        let label = i.to_string();
        // Above the box, or below it if there's no room at the top edge.
        let (lx, ly) = (x0, if y0 >= 12 { y0 - 12 } else { y1 + 1 });
        let plate_w = label.len() as i64 * 8 + 2;
        for dy in 0..12 {
            for dx in 0..plate_w {
                put(&mut out, lx + dx, ly + dy, label_bg);
            }
        }
        for (k, ch) in label.bytes().enumerate() {
            let glyph = DIGITS[(ch - b'0') as usize];
            for row in 0..5 {
                for col in 0..3 {
                    if glyph >> (14 - (row * 3 + col)) & 1 == 1 {
                        for (sx, sy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let px = lx + 2 + k as i64 * 8 + col * 2 + sx;
                            put(
                                &mut out,
                                px,
                                ly + 1 + row * 2 + sy,
                                Rgba([255, 255, 255, 255]),
                            );
                        }
                    }
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------- crop

fn crop_entry(e: &Entry) -> Result<RgbaImage> {
    let sheet_name = e.sheet.as_deref().context("no `sheet`")?;
    let sheet = open(&root().join("sheets").join(sheet_name))?;
    let [x, y, w, h] = e.crop.unwrap_or([0, 0, sheet.width(), sheet.height()]);
    ensure!(
        x + w <= sheet.width() && y + h <= sheet.height(),
        "crop {:?} is outside the {}x{} sheet",
        [x, y, w, h],
        sheet.width(),
        sheet.height()
    );
    let bg = Bg::new(&sheet, &e.bg.list())?;
    let mut img = imageops::crop_imm(&sheet, x, y, w, h).to_image();
    if e.flood {
        flood_clear(&mut img, &bg);
    } else {
        for p in img.pixels_mut() {
            if bg.is_bg(p) {
                *p = Rgba([0, 0, 0, 0]);
            }
        }
    }
    let mut img = trim(&img).context("crop contains no sprite pixels")?;
    if let Some(f) = e.downscale.filter(|&f| f > 1) {
        img = imageops::resize(
            &img,
            img.width().div_ceil(f),
            img.height().div_ceil(f),
            imageops::Nearest,
        );
    }
    Ok(img)
}

/// Clear background pixels reachable from the border, leaving same-coloured
/// pixels inside the sprite (eyes, highlights) intact.
fn flood_clear(img: &mut RgbaImage, bg: &Bg) {
    let (w, h) = img.dimensions();
    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();
    for x in 0..w {
        queue.extend([(x, 0), (x, h - 1)]);
    }
    for y in 0..h {
        queue.extend([(0, y), (w - 1, y)]);
    }
    let clear = Rgba([0, 0, 0, 0]);
    while let Some((x, y)) = queue.pop_front() {
        let p = img.get_pixel(x, y);
        if *p == clear || !bg.is_bg(p) {
            continue;
        }
        img.put_pixel(x, y, clear);
        if x > 0 {
            queue.push_back((x - 1, y));
        }
        if x + 1 < w {
            queue.push_back((x + 1, y));
        }
        if y > 0 {
            queue.push_back((x, y - 1));
        }
        if y + 1 < h {
            queue.push_back((x, y + 1));
        }
    }
}

/// Shrink to the bounding box of non-transparent pixels.
fn trim(img: &RgbaImage) -> Option<RgbaImage> {
    let opaque = |x, y| img.get_pixel(x, y).0[3] > 0;
    let (w, h) = img.dimensions();
    let xs: Vec<u32> = (0..w).filter(|&x| (0..h).any(|y| opaque(x, y))).collect();
    let ys: Vec<u32> = (0..h).filter(|&y| (0..w).any(|x| opaque(x, y))).collect();
    let (x0, x1, y0, y1) = (*xs.first()?, *xs.last()?, *ys.first()?, *ys.last()?);
    Some(imageops::crop_imm(img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image())
}

fn crop(only: Option<&str>) -> Result<()> {
    let entries = load_manifest()?;
    let png_dir = root().join("sprites/png");
    let mut done = 0;
    let mut failed = 0;
    for e in entries.iter().filter(|e| only.is_none_or(|id| e.id == id)) {
        if !e.croppable() {
            if only.is_some() {
                bail!("`{}` has no crop box or url yet", e.id);
            }
            continue;
        }
        match crop_entry(e) {
            Ok(img) => {
                let path = png_dir.join(format!("{}.png", e.id));
                img.save(&path)?;
                let warn = if img.height() > MAX_HEIGHT {
                    "  WARNING: taller than 64px"
                } else {
                    ""
                };
                println!("ok    {:<16} {}x{}{warn}", e.id, img.width(), img.height());
                done += 1;
            }
            Err(err) => {
                println!("FAIL  {:<16} {err:#}", e.id);
                failed += 1;
            }
        }
    }
    if only.is_some() && done + failed == 0 {
        bail!("no sprite with that id in the manifest");
    }
    println!("{done} cropped, {failed} failed");
    ensure!(failed == 0, "some crops failed");
    Ok(())
}

// ---------------------------------------------------------------- fetch

const USER_AGENT: &str = "retrofetch/0.1 (sprite fetch; personal project)";

fn fetch(force: bool) -> Result<()> {
    let sheets = root().join("sheets");
    let (mut got, mut failed) = (0, 0);
    for e in load_manifest()? {
        let (Some(url), Some(sheet)) = (&e.url, &e.sheet) else {
            continue;
        };
        let dest = sheets.join(sheet);
        if dest.is_file() && !force {
            continue;
        }
        let status = std::process::Command::new("curl")
            .args(["-sfL", "--max-time", "30", "-A", USER_AGENT, "-o"])
            .arg(&dest)
            .arg(url)
            .status()
            .context("failed to run curl")?;
        if status.success() {
            println!("ok    {:<16} -> sheets/{sheet}", e.id);
            got += 1;
        } else {
            let _ = fs::remove_file(&dest);
            println!("FAIL  {:<16} {url}", e.id);
            failed += 1;
        }
        // Be polite to the wikis hosting these files.
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    println!("{got} downloaded, {failed} failed");
    ensure!(failed == 0, "some downloads failed");
    Ok(())
}

// ---------------------------------------------------------------- check

fn check() -> Result<()> {
    let entries = load_manifest()?;
    let root = root();
    let mut errors = 0;
    let mut ids = HashSet::new();
    let (mut ready, mut todo_crop, mut todo_sheet) = (vec![], vec![], vec![]);

    for e in &entries {
        let mut err = |msg: String| {
            println!("error  {:<16} {msg}", e.id);
            errors += 1;
        };
        if !ids.insert(e.id.as_str()) {
            err("duplicate id".into());
        }
        if !CONSOLES.contains(&e.console.as_str()) {
            err(format!("unknown console `{}`", e.console));
        }
        let png = root.join("sprites/png").join(format!("{}.png", e.id));
        if png.is_file() {
            match image::open(&png) {
                Ok(img) if img.height() > MAX_HEIGHT => err(format!(
                    "{}px tall (max {MAX_HEIGHT}); set `downscale` or pick a smaller frame",
                    img.height()
                )),
                Ok(_) => ready.push(e.id.as_str()),
                Err(x) => err(format!("unreadable png: {x}")),
            }
            continue;
        }
        if e.crop.is_some() || e.url.is_some() {
            err(
                "has a crop box/url but no png — run `cargo xtask fetch && cargo xtask crop`"
                    .into(),
            );
            continue;
        }
        let sheet_present = e
            .sheet
            .as_ref()
            .is_some_and(|s| root.join("sheets").join(s).is_file());
        if sheet_present {
            todo_crop.push(e.id.as_str());
        } else {
            todo_sheet.push(e);
        }
    }

    println!(
        "\n{} ready, {} need a crop box, {} need a sheet",
        ready.len(),
        todo_crop.len(),
        todo_sheet.len()
    );
    if !todo_crop.is_empty() {
        println!(
            "\nneeds crop box (sheet present):\n  {}",
            todo_crop.join(", ")
        );
    }
    if !todo_sheet.is_empty() {
        println!("\nneeds sheet in sheets/ (download from spriters-resource.com):");
        for e in &todo_sheet {
            let hint = match &e.source {
                Some(url) => url.clone(),
                None => format!("search: {} ({})", e.game, e.console),
            };
            println!(
                "  {:<16} -> sheets/{:<20} {hint}",
                e.id,
                e.sheet.as_deref().unwrap_or("(no `sheet` set)"),
            );
        }
    }
    ensure!(errors == 0, "{errors} error(s)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_merges_close_pixels_and_splits_far_ones() {
        let mut img = RgbaImage::from_pixel(20, 10, Rgba([0, 255, 0, 255]));
        let px = Rgba([255, 0, 0, 255]);
        // Blob A: two pixels 2 apart -> one box. Blob B far away.
        img.put_pixel(1, 1, px);
        img.put_pixel(3, 1, px);
        img.put_pixel(15, 7, px);
        let bg = Bg::new(&img, &[]).unwrap();
        let boxes = find_boxes(&img, &bg, 2);
        assert_eq!(
            boxes,
            vec![
                BBox {
                    x: 1,
                    y: 1,
                    w: 3,
                    h: 1
                },
                BBox {
                    x: 15,
                    y: 7,
                    w: 1,
                    h: 1
                }
            ]
        );
    }

    #[test]
    fn trim_shrinks_to_content() {
        let mut img = RgbaImage::new(5, 5);
        img.put_pixel(2, 1, Rgba([1, 2, 3, 255]));
        img.put_pixel(3, 3, Rgba([1, 2, 3, 255]));
        let t = trim(&img).unwrap();
        assert_eq!(t.dimensions(), (2, 3));
        assert!(trim(&RgbaImage::new(3, 3)).is_none());
    }

    #[test]
    fn flood_keeps_enclosed_background_colour() {
        let w = Rgba([255, 255, 255, 255]);
        let k = Rgba([0, 0, 0, 255]);
        // white border, black ring, white "eye" inside the ring
        let mut img = RgbaImage::from_pixel(5, 5, w);
        for (x, y) in [
            (1, 1),
            (2, 1),
            (3, 1),
            (1, 2),
            (3, 2),
            (1, 3),
            (2, 3),
            (3, 3),
        ] {
            img.put_pixel(x, y, k);
        }
        let bg = Bg::new(&img, &[]).unwrap();
        flood_clear(&mut img, &bg);
        assert_eq!(img.get_pixel(0, 0).0[3], 0);
        assert_eq!(*img.get_pixel(2, 2), w);
    }

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex("#00ff80").unwrap(), [0, 255, 128]);
        assert!(parse_hex("nope").is_err());
    }
}
