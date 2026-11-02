mod fetch;
mod render;
mod sprites;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

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

    /// Extra arguments passed through to fastfetch (after `--`)
    #[arg(last = true)]
    fastfetch_args: Vec<String>,
}

const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
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
    let pool = sprites::filter(SPRITES, cli.console.as_deref(), cli.company.as_deref());

    if cli.list {
        print_list(&pool);
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
    let logo = compose(&art, (!cli.no_caption).then(|| caption(sprite)));

    if !cli.no_fetch && fetch::run(&logo, &cli.fastfetch_args)? {
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

/// Caption lines (plain text, styled later): name, game, company · console · year.
fn caption(s: &Sprite) -> Vec<(String, &'static str)> {
    let mut lines = vec![(s.name.to_string(), BOLD)];
    if s.game != s.name {
        lines.push((s.game.to_string(), DIM));
    }
    lines.push((
        format!("{} · {} · {}", s.company, s.console_label(), s.year),
        DIM,
    ));
    lines
}

/// Centre the sprite and caption against each other.
fn compose(art: &render::Art, caption: Option<Vec<(String, &'static str)>>) -> Vec<String> {
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

fn print_list(pool: &[&Sprite]) {
    println!(
        "{BOLD}{:<16} {:<18} {:<30} {:<12} {:<10} YEAR{RESET}",
        "ID", "NAME", "GAME", "COMPANY", "CONSOLE"
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
    println!("{DIM}{} characters{RESET}", pool.len());
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max - 1).chain(['…']).collect()
    }
}
