//! The embedded sprite table and lookup helpers.

#[derive(Debug, PartialEq)]
pub struct Sprite {
    pub id: &'static str,
    pub name: &'static str,
    pub game: &'static str,
    pub company: &'static str,
    pub console: &'static str,
    pub year: u16,
    pub png: &'static [u8],
}

pub static SPRITES: &[Sprite] = include!(concat!(env!("OUT_DIR"), "/sprites.rs"));

pub const CONSOLES: &[&str] = &["snes", "genesis", "arcade", "neogeo", "pce"];

impl Sprite {
    pub fn console_label(&self) -> &'static str {
        console_label(self.console)
    }
}

pub fn console_label(console: &str) -> &'static str {
    match console {
        "snes" => "SNES",
        "genesis" => "Genesis",
        "arcade" => "Arcade",
        "neogeo" => "Neo Geo",
        "pce" => "PC Engine",
        _ => "?",
    }
}

/// Lowercase and keep only alphanumerics, so "Mega Man X", "megaman-x" and
/// "MEGAMAN X" all compare equal.
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn filter<'a>(
    all: &'a [Sprite],
    console: Option<&str>,
    company: Option<&str>,
) -> Vec<&'a Sprite> {
    let company = company.map(norm);
    all.iter()
        .filter(|s| console.is_none_or(|c| s.console.eq_ignore_ascii_case(c)))
        .filter(|s| {
            company
                .as_deref()
                .is_none_or(|c| norm(s.company).contains(c))
        })
        .collect()
}

#[derive(Debug, PartialEq)]
pub enum Lookup<'a> {
    Found(&'a Sprite),
    NotFound,
    Ambiguous(Vec<&'a Sprite>),
}

/// Exact id/name match wins; otherwise a unique substring match of id or name.
pub fn find<'a>(pool: &[&'a Sprite], query: &str) -> Lookup<'a> {
    let q = norm(query);
    if let Some(s) = pool.iter().find(|s| norm(s.id) == q || norm(s.name) == q) {
        return Lookup::Found(s);
    }
    let hits: Vec<&Sprite> = pool
        .iter()
        .copied()
        .filter(|s| norm(s.id).contains(&q) || norm(s.name).contains(&q))
        .collect();
    match hits.len() {
        0 => Lookup::NotFound,
        1 => Lookup::Found(hits[0]),
        _ => Lookup::Ambiguous(hits),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(
        id: &'static str,
        name: &'static str,
        company: &'static str,
        console: &'static str,
    ) -> Sprite {
        Sprite {
            id,
            name,
            game: "g",
            company,
            console,
            year: 1993,
            png: &[],
        }
    }

    fn roster() -> Vec<Sprite> {
        vec![
            sp("megaman-x", "Mega Man X", "Capcom", "snes"),
            sp("zero", "Zero", "Capcom", "snes"),
            sp("sonic", "Sonic", "Sega", "genesis"),
            sp("mario", "Mario", "Nintendo", "snes"),
            sp("dr-mario", "Dr. Mario", "Nintendo", "snes"),
        ]
    }

    #[test]
    fn filter_by_console_and_company() {
        let r = roster();
        let ids = |v: Vec<&Sprite>| v.iter().map(|s| s.id).collect::<Vec<_>>();
        assert_eq!(ids(filter(&r, Some("GENESIS"), None)), ["sonic"]);
        assert_eq!(ids(filter(&r, None, Some("capcom"))), ["megaman-x", "zero"]);
        assert_eq!(
            ids(filter(&r, Some("snes"), Some("ninten"))),
            ["mario", "dr-mario"]
        );
        assert_eq!(filter(&r, None, None).len(), r.len());
    }

    #[test]
    fn find_exact_beats_substring() {
        let r = roster();
        let pool = filter(&r, None, None);
        // "mario" is a substring of "dr-mario" too, but exact id wins.
        assert_eq!(find(&pool, "Mario"), Lookup::Found(&r[3]));
        assert_eq!(find(&pool, "mega man x"), Lookup::Found(&r[0]));
        assert_eq!(find(&pool, "MEGAMAN"), Lookup::Found(&r[0]));
    }

    #[test]
    fn find_ambiguous_and_missing() {
        let r = roster();
        let pool = filter(&r, None, None);
        assert!(matches!(find(&pool, "o"), Lookup::Ambiguous(v) if v.len() == 4));
        assert_eq!(find(&pool, "link"), Lookup::NotFound);
    }

    #[test]
    fn embedded_sprites_are_valid() {
        assert!(!SPRITES.is_empty());
        for s in SPRITES {
            assert!(CONSOLES.contains(&s.console), "{}: bad console", s.id);
            let img = crate::render::decode(s.png).expect(s.id);
            assert!(
                img.height() <= 64,
                "{} is {}px tall (max 64)",
                s.id,
                img.height()
            );
        }
    }
}
