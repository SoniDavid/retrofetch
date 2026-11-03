//! Optional colour profiles, read from `$XDG_CONFIG_HOME/retrofetch/config.toml`
//! (default `~/.config/retrofetch/config.toml`):
//!
//! ```toml
//! default = "kanagawa-wave"
//!
//! [profiles.kanagawa-wave]
//! name = "#7E9CD8"   # caption: character name (always bold)
//! game = "#DCD7BA"   # caption: game title
//! meta = "#727169"   # caption: company · console · year
//! fastfetch_config = "~/.config/fastfetch/kanagawa-wave.jsonc"
//! box = true         # frame the system info in a rounded box
//! border = "#727169" # box colour
//! ```
//!
//! Every field is optional. Without a config file retrofetch keeps its plain
//! bold/dim caption and fastfetch's own config.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    default: Option<String>,
    #[serde(default)]
    profiles: BTreeMap<String, RawProfile>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    name: Option<String>,
    game: Option<String>,
    meta: Option<String>,
    fastfetch_config: Option<String>,
    #[serde(rename = "box")]
    boxed: Option<bool>,
    border: Option<String>,
}

/// Resolved styles ready to print.
#[derive(Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub game: String,
    pub meta: String,
    pub fastfetch_config: Option<PathBuf>,
    pub boxed: bool,
    pub border: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: BOLD.into(),
            game: DIM.into(),
            meta: DIM.into(),
            fastfetch_config: None,
            boxed: false,
            border: DIM.into(),
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => home()?.join(".config"),
    };
    Some(base.join("retrofetch").join("config.toml"))
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Load the active profile: `--profile` > `$RETROFETCH_PROFILE` > `default`.
pub fn load(cli_choice: Option<&str>) -> Result<Profile> {
    let env_choice = std::env::var("RETROFETCH_PROFILE")
        .ok()
        .filter(|v| !v.is_empty());
    let choice = cli_choice.map(str::to_owned).or(env_choice);
    let Some(path) = config_path() else {
        return resolve(ConfigFile::default(), choice.as_deref(), None);
    };
    let file = match std::fs::read_to_string(&path) {
        Ok(text) => {
            toml::from_str(&text).with_context(|| format!("invalid config {}", path.display()))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ConfigFile::default(),
        Err(e) => return Err(e).with_context(|| format!("can't read {}", path.display())),
    };
    resolve(file, choice.as_deref(), home().as_deref())
}

fn resolve(file: ConfigFile, choice: Option<&str>, home: Option<&Path>) -> Result<Profile> {
    let Some(name) = choice.or(file.default.as_deref()) else {
        return Ok(Profile::default());
    };
    let Some(raw) = file.profiles.get(name) else {
        let known: Vec<_> = file.profiles.keys().map(String::as_str).collect();
        if known.is_empty() {
            bail!("unknown profile `{name}` (no profiles defined in the config)");
        }
        bail!("unknown profile `{name}` (available: {})", known.join(", "));
    };
    let base = Profile::default();
    let style = |hex: &Option<String>, prefix: &str, fallback: String| -> Result<String> {
        match hex {
            Some(h) => {
                let [r, g, b] =
                    parse_hex(h).with_context(|| format!("profile `{name}`: bad colour `{h}`"))?;
                Ok(format!("\x1b[{prefix}38;2;{r};{g};{b}m"))
            }
            None => Ok(fallback),
        }
    };
    Ok(Profile {
        name: style(&raw.name, "1;", base.name)?,
        game: style(&raw.game, "", base.game)?,
        meta: style(&raw.meta, "", base.meta)?,
        fastfetch_config: raw
            .fastfetch_config
            .as_deref()
            .map(|p| expand_tilde(p, home)),
        boxed: raw.boxed.unwrap_or(false),
        border: style(&raw.border, "", base.border)?,
    })
}

fn expand_tilde(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

/// `#rrggbb` (or `rrggbb`) → `[r, g, b]`.
pub fn parse_hex(s: &str) -> Result<[u8; 3]> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("expected #rrggbb");
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("validated");
    Ok([byte(0), byte(2), byte(4)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(text: &str) -> ConfigFile {
        toml::from_str(text).unwrap()
    }

    const SAMPLE: &str = r##"
        default = "wave"
        [profiles.wave]
        name = "#7E9CD8"
        meta = "727169"
        box = true
        border = "#363646"
        fastfetch_config = "~/.config/fastfetch/wave.jsonc"
        [profiles.other]
        game = "#ffffff"
    "##;

    #[test]
    fn parses_hex() {
        assert_eq!(parse_hex("#7E9CD8").unwrap(), [0x7e, 0x9c, 0xd8]);
        assert_eq!(parse_hex("ffa066").unwrap(), [0xff, 0xa0, 0x66]);
        assert!(parse_hex("#fff").is_err());
        assert!(parse_hex("#gggggg").is_err());
    }

    #[test]
    fn no_config_means_plain_defaults() {
        assert_eq!(
            resolve(ConfigFile::default(), None, None).unwrap(),
            Profile::default()
        );
    }

    #[test]
    fn default_profile_is_used() {
        let p = resolve(cfg(SAMPLE), None, Some(Path::new("/home/u"))).unwrap();
        assert_eq!(p.name, "\x1b[1;38;2;126;156;216m");
        assert_eq!(p.game, DIM, "unset fields fall back");
        assert_eq!(p.meta, "\x1b[38;2;114;113;105m");
        assert_eq!(
            p.fastfetch_config,
            Some(PathBuf::from("/home/u/.config/fastfetch/wave.jsonc"))
        );
        assert!(p.boxed);
        assert_eq!(p.border, "\x1b[38;2;54;54;70m");
    }

    #[test]
    fn explicit_choice_wins_over_default() {
        let p = resolve(cfg(SAMPLE), Some("other"), None).unwrap();
        assert_eq!(p.name, BOLD);
        assert_eq!(p.game, "\x1b[38;2;255;255;255m");
        assert_eq!(p.fastfetch_config, None);
        assert!(!p.boxed);
    }

    #[test]
    fn unknown_profile_errors() {
        let err = resolve(cfg(SAMPLE), Some("nope"), None).unwrap_err();
        assert!(err.to_string().contains("available: other, wave"));
    }

    #[test]
    fn bad_colour_errors() {
        let c = cfg("[profiles.x]\nname = \"blue\"");
        assert!(resolve(c, Some("x"), None).is_err());
    }
}
