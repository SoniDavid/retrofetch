# retrofetch

[fastfetch](https://github.com/fastfetch-cli/fastfetch) with a random 16-bit era video game character
next to it: Nintendo, Capcom, Sega, Konami, Square, SNK and friends. It's the retro sibling of
[pokefetch](../pokefetch).

Sprites are drawn as truecolor ANSI half blocks (`▀`/`▄`), so it works in any modern terminal.
Every sprite is embedded in the binary: no network, no cache, instant startup.

## Usage

```
retrofetch                        # random character + fastfetch
retrofetch sonic                  # a specific character (id or name, partial match ok)
retrofetch --console snes         # snes | genesis | arcade | neogeo | pce
retrofetch --company capcom
retrofetch --list                 # what's available
retrofetch --no-fetch megaman-x   # just the sprite
retrofetch --no-caption
retrofetch -- --config ~/.config/fastfetch/mini.jsonc   # args after -- go to fastfetch
```

If fastfetch isn't installed, retrofetch prints just the sprite.

## Install

```bash
cargo install --path retrofetch
echo 'retrofetch' >> ~/.bashrc     # or ~/.zshrc
```

## Where the sprites come from

There's no PokeAPI for retro games. The biggest archive of 16-bit rips, [The Spriters Resource](https://www.spriters-resource.com),
has no API, serves whole sprite sheets, and sits behind a Cloudflare bot challenge, so tools can't
download from it. Retrofetch uses two sources instead:

- **Wiki sprite files (automatic).** Super Mario Wiki and Fandom wikis (Mega Man, Sonic, Final Fantasy, Kirby, …)
  host single-character sprites at native resolution behind a public MediaWiki API. Entries with a
  `url` in the manifest are downloaded by `cargo xtask fetch`.
- **Hand-downloaded sheets (manual).** For everything else, save a sheet from your browser into `sheets/`
  and give the entry a `crop` box.

Either way, `cargo xtask crop` turns the source into a small transparent PNG in `sprites/png/`, and
those PNGs are compiled into the binary.

## Adding a character

1. Add (or find) an entry in `sprites/manifest.toml`. `cargo xtask check` lists what's missing.
2. Get the image:
   - **Wiki file:** find the sprite's file page (e.g. `https://megaman.fandom.com/wiki/File:...`) and put
     it in `source`. Put the direct image URL in `url` (for Fandom, append `&format=original`,
     otherwise you get lossy WebP) and name the local file in `sheet` (keep the `.gif`/`.png`
     extension). Then run `cargo xtask fetch`. With no `crop`, the whole image is used.
   - **Sheet:** download it in a browser, save it as `sheets/<id>.png`, then run
     `cargo xtask detect sheets/<id>.png`. Open `sheets/<id>.annotated.png` and copy the frame's
     `x y w h` into `crop`.
     (`--bg '#rrggbb,…'` sets the background colours, `--gap N` merges/splits blobs, `--min N` hides fragments.)
3. Crop and preview:
   ```bash
   cargo xtask crop --id <id>
   cargo run -- --no-fetch <id>
   ```
   - Solid background that also appears inside the sprite (white eyes, etc.): set `bg` and `flood = true`.
   - Image stored at 2×/3× scale: set `downscale = 2` (or 3).
   - Sprites must be ≤ 64 px tall (32 terminal rows).
4. `cargo xtask check && cargo test --workspace`, then commit the PNG + manifest change.

`sheets/` is gitignored; only the cropped sprites are committed.

## Layout

```
sprites/manifest.toml   roster: metadata, source link, sheet file, crop box, bg key
sprites/png/<id>.png    cropped sprites (embedded by retrofetch/build.rs)
sheets/                 raw sheets you downloaded (gitignored)
retrofetch/             the binary: render.rs (ANSI), sprites.rs (lookup), fetch.rs (fastfetch)
xtask/                  dev tools: fetch, detect, crop, check
```

## Credits & disclaimer

All characters and sprites are © their respective owners (Nintendo, Capcom, Sega, Konami,
Square Enix, SNK, Hudson/Konami, …). Sprite rips come from the communities of
[Super Mario Wiki](https://www.mariowiki.com), [WiKirby](https://wikirby.com) and various [Fandom](https://www.fandom.com) game wikis
(and, for manually added sheets, [The Spriters Resource](https://www.spriters-resource.com)); each
entry's `source` in the manifest links to the original file page. This is a non-commercial fan project with no affiliation to any of these companies.
The code is MIT-licensed (see `LICENSE`; it does not cover the sprites).
