mod boxed;
mod fetch;
mod profile;
mod render;
mod sprites;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use profile::Profile;
use sprites::{Lookup, Sprite, CONSOLES, SPRITES};

/// fastfetch with a random 16-bit era video game sprite.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Show a specific character (id or name, partial matches allowed)
    query: Option<String>,

    /// Only pick characters from this console
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(CONSOLES))]
    console: Option<String>,

    /// Only pick characters from this company (e.g. capcom, sega, nintendo)
    #[arg(long)]
    company: Option<String>,

    /// List the available characters and exit
    #[arg(long)]
    list: bool,

    /// Print only the sprite, without running fastfetch
    #[arg(long)]
    no_fetch: bool,

    /// Hide the name/game caption under the sprite
    #[arg(long)]
    no_caption: bool,

    /// Colour profile from ~/.config/retrofetch/config.toml (overrides $RETROFETCH_PROFILE)
    #[arg(long, value_name = "NAME")]
    profile: Option<String>,

    /// Extra arguments passed through to fastfetch (after `--`)
    #[arg(last = true)]
    fastfetch_args: Vec<String>,
}

const RESET: &str = "\x1b[0m";

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("retrofetch: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let profile = profile::load(cli.profile.as_deref())?;
    let pool = sprites::filter(SPRITES, cli.console.as_deref(), cli.company.as_deref());

    if cli.list {
        print_list(&pool, &profile);
        return Ok(ExitCode::SUCCESS);
    }

    let sprite = match &cli.query {
        Some(q) => match sprites::find(&pool, q) {
            Lookup::Found(s) => s,
            Lookup::NotFound => {
                eprintln!("retrofetch: no character matches `{q}` (try --list)");
                return Ok(ExitCode::FAILURE);
            }
            Lookup::Ambiguous(hits) => {
                eprintln!("retrofetch: `{q}` matches several characters:");
                for s in hits {
                    eprintln!("  {:<16} {}", s.id, s.name);
                }
                return Ok(ExitCode::FAILURE);
            }
        },
        None => {
            if pool.is_empty() {
                eprintln!("retrofetch: no characters match those filters (try --list)");
                return Ok(ExitCode::FAILURE);
            }
            pool[fastrand::usize(..pool.len())]
        }
    };

    let art = render::render(&render::decode(sprite.png)?);
    let logo = compose(&art, (!cli.no_caption).then(|| caption(sprite, &profile)));

    if !cli.no_fetch && profile.boxed {
        let config = profile.fastfetch_config.as_deref();
        if let Some(info) = fetch::capture(config, &cli.fastfetch_args)? {
            let info: Vec<String> = info.iter().map(|l| boxed::normalize(l)).collect();
            let style = boxed::Style {
                border: &profile.border,
                title: &profile.name,
            };
            let logo: Vec<String> = logo.iter().map(|l| format!("  {l}")).collect();
            let logo_w = logo
                .iter()
                .map(|l| boxed::visible_width(l))
                .max()
                .unwrap_or(0);
            let title = title();
            let lines = match terminal_cols() {
                // Room beside the sprite (long values get cut with `…` if needed).
                Some(cols) if cols >= logo_w + GAP + MIN_BOX => {
                    let framed = boxed::frame(&info, &title, &style, Some(cols - logo_w - GAP - 1));
                    boxed::side_by_side(&logo, &framed, GAP)
                }
                // Too narrow: box goes under the sprite.
                Some(cols) => {
                    let framed = boxed::frame(&info, &title, &style, Some(cols.saturating_sub(1)));
                    logo.into_iter()
                        .chain([String::new()])
                        .chain(framed)
                        .collect()
                }
                None => boxed::side_by_side(&logo, &boxed::frame(&info, &title, &style, None), GAP),
            };
            for line in lines {
                println!("{line}");
            }
            return Ok(ExitCode::SUCCESS);
        }
    }
    if !cli.no_fetch
        && fetch::run(
            &logo,
            profile.fastfetch_config.as_deref(),
            &cli.fastfetch_args,
        )?
    {
        return Ok(ExitCode::SUCCESS);
    }
    if !cli.no_fetch {
        eprintln!("retrofetch: fastfetch not found, printing sprite only");
    }
    for line in &logo {
        println!("{line}");
    }
    Ok(ExitCode::SUCCESS)
}

/// Spaces between the sprite and the box.
const GAP: usize = 4;
/// Narrowest box worth showing beside the sprite before stacking it below.
const MIN_BOX: usize = 50;

/// Width of the terminal, if stdout is one (or `$COLUMNS` is set).
fn terminal_cols() -> Option<usize> {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .or_else(|| std::env::var("COLUMNS").ok()?.parse().ok())
}

/// `user@host` for the box title.
fn title() -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .map(|h| h.trim().to_string())
        .unwrap_or_else(|_| "localhost".into());
    format!("{user}@{host}")
}

/// Caption lines (plain text, styled later): name, game, company · console · year.
fn caption<'a>(s: &Sprite, p: &'a Profile) -> Vec<(String, &'a str)> {
    let mut lines = vec![(s.name.to_string(), p.name.as_str())];
    if s.game != s.name {
        lines.push((s.game.to_string(), p.game.as_str()));
    }
    lines.push((
        format!("{} · {} · {}", s.company, s.console_label(), s.year),
        p.meta.as_str(),
    ));
    lines
}

/// Centre the sprite and caption against each other.
fn compose(art: &render::Art, caption: Option<Vec<(String, &str)>>) -> Vec<String> {
    let caption = caption.unwrap_or_default();
    let cap_width = caption
        .iter()
        .map(|(t, _)| t.chars().count())
        .max()
        .unwrap_or(0);
    let width = art.width.max(cap_width);

    let pad = |w: usize| " ".repeat((width - w) / 2);
    let mut out: Vec<String> = art
        .lines
        .iter()
        .map(|l| format!("{}{l}", pad(art.width)))
        .collect();
    if !caption.is_empty() {
        out.push(String::new());
        for (text, style) in caption {
            out.push(format!("{}{style}{text}{RESET}", pad(text.chars().count())));
        }
    }
    out
}

fn print_list(pool: &[&Sprite], p: &Profile) {
    println!(
        "{}{:<16} {:<18} {:<30} {:<12} {:<10} YEAR{RESET}",
        p.name, "ID", "NAME", "GAME", "COMPANY", "CONSOLE"
    );
    for s in pool {
        println!(
            "{:<16} {:<18} {:<30} {:<12} {:<10} {}",
            s.id,
            s.name,
            truncate(s.game, 30),
            s.company,
            s.console_label(),
            s.year
        );
    }
    println!("{}{} characters{RESET}", p.meta, pool.len());
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max - 1).chain(['…']).collect()
    }
}
