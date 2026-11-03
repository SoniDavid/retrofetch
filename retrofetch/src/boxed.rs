//! Draw fastfetch's output inside a rounded box next to the sprite.
//!
//! fastfetch can't draw a closed box itself (its lines have different widths),
//! so retrofetch captures its output, measures the visible width of every line
//! and frames it.

use unicode_width::UnicodeWidthChar;

const RESET: &str = "\x1b[0m";

enum Tok<'a> {
    Char(char),
    Esc(&'a str),
}

/// Walk a line, yielding printable chars and whole escape sequences.
fn scan<'a>(line: &'a str, mut f: impl FnMut(Tok<'a>)) {
    let mut chars = line.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if c != '\x1b' {
            f(Tok::Char(c));
            continue;
        }
        // CSI: ESC [ params final-byte(0x40..=0x7e)
        let mut end = start + 1;
        if let Some(&(_, '[')) = chars.peek() {
            chars.next();
            for (i, ch) in chars.by_ref() {
                end = i + ch.len_utf8();
                if ('\x40'..='\x7e').contains(&ch) {
                    break;
                }
            }
        }
        f(Tok::Esc(&line[start..end]));
    }
}

/// Terminal cells taken by a line, ignoring escape sequences.
pub fn visible_width(line: &str) -> usize {
    let mut w = 0;
    scan(line, |t| {
        if let Tok::Char(c) = t {
            w += c.width().unwrap_or(0);
        }
    });
    w
}

/// Make a captured fastfetch line position-independent: absolute cursor moves
/// (`ESC[nG`, used to align values) become spaces, and mode toggles such as
/// `ESC[?7l` (line-wrap off) are dropped.
pub fn normalize(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut col = 0usize;
    scan(line, |t| match t {
        Tok::Char(c) => {
            col += c.width().unwrap_or(0);
            out.push(c);
        }
        Tok::Esc(esc) => {
            let body = esc.strip_prefix("\x1b[").unwrap_or("");
            if let Some(n) = body.strip_suffix('G') {
                let target = n.parse::<usize>().unwrap_or(1).saturating_sub(1);
                while col < target {
                    out.push(' ');
                    col += 1;
                }
            } else if body.starts_with('?') {
                // private mode set/reset: drop
            } else {
                out.push_str(esc);
            }
        }
    });
    out
}

/// Cut a line to at most `max` visible cells, ending in `…` when shortened.
/// Escape sequences are kept so colours stay intact.
pub fn truncate(line: &str, max: usize) -> String {
    if visible_width(line) <= max {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len());
    let mut col = 0usize;
    let mut done = false;
    scan(line, |t| match t {
        Tok::Esc(esc) => out.push_str(esc),
        Tok::Char(_) if done => {}
        Tok::Char(c) => {
            let w = c.width().unwrap_or(0);
            if col + w + 1 > max {
                out.push('…');
                done = true;
            } else {
                col += w;
                out.push(c);
            }
        }
    });
    out.push_str(RESET);
    out
}

pub struct Style<'a> {
    pub border: &'a str,
    pub title: &'a str,
}

/// Frame `lines` in a rounded box with `title` in the top edge and the
/// terminal's 8 palette colours as dots in the bottom edge. With `max_width`
/// (total width including the borders) long lines are cut with `…`.
pub fn frame(
    lines: &[String],
    title: &str,
    style: &Style,
    max_width: Option<usize>,
) -> Vec<String> {
    let b = style.border;
    let dots: String = (1..=7)
        .chain([0])
        .map(|i| format!("\x1b[3{i}m●"))
        .collect::<Vec<_>>()
        .join(" ");
    let dots_w = 8 * 2 - 1;
    let title_w = title.chars().count();

    let content_w = lines.iter().map(|l| visible_width(l)).max().unwrap_or(0);
    // Inner width (between the corner pieces) must fit content + 1 space each side,
    // " title " with some rule, and " dots " with some rule.
    let min_inner = (title_w + 2 + 4).max(dots_w + 2 + 4);
    let mut inner = (content_w + 2).max(min_inner);
    if let Some(max) = max_width {
        inner = inner.min(max.saturating_sub(2)).max(min_inner);
    }

    let edge = |label: String, label_w: usize, l: char, r: char| {
        let rest = inner - label_w - 2;
        let left = rest / 2;
        format!(
            "{b}{l}{}{RESET} {label} {b}{}{r}{RESET}",
            "─".repeat(left),
            "─".repeat(rest - left)
        )
    };

    let mut out = Vec::with_capacity(lines.len() + 2);
    out.push(edge(
        format!("{}{title}{RESET}", style.title),
        title_w,
        '╭',
        '╮',
    ));
    for line in lines {
        let line = &truncate(line, inner - 2);
        let pad = inner - 2 - visible_width(line);
        out.push(format!(
            "{b}│{RESET} {line}{RESET}{} {b}│{RESET}",
            " ".repeat(pad)
        ));
    }
    out.push(edge(format!("{dots}{RESET}"), dots_w, '╰', '╯'));
    out
}

/// Put `left` and `right` next to each other, vertically centred, with `gap` spaces.
pub fn side_by_side(left: &[String], right: &[String], gap: usize) -> Vec<String> {
    let left_w = left.iter().map(|l| visible_width(l)).max().unwrap_or(0);
    let rows = left.len().max(right.len());
    let left_top = (rows - left.len()) / 2;
    let right_top = (rows - right.len()) / 2;
    (0..rows)
        .map(|r| {
            let l = r.checked_sub(left_top).and_then(|i| left.get(i));
            let rt = r.checked_sub(right_top).and_then(|i| right.get(i));
            let l_str = l.map(String::as_str).unwrap_or("");
            match rt {
                Some(rt) => {
                    let pad = left_w - visible_width(l_str) + gap;
                    format!("{l_str}{RESET}{}{rt}", " ".repeat(pad))
                }
                None => l_str.to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(s: &str) -> String {
        let mut out = String::new();
        scan(s, |t| {
            if let Tok::Char(c) = t {
                out.push(c);
            }
        });
        out
    }

    #[test]
    fn width_ignores_escapes() {
        assert_eq!(visible_width("\x1b[1;38;2;1;2;3mab\x1b[0mc"), 3);
        assert_eq!(visible_width("● 󰍛 ▀"), 5);
    }

    #[test]
    fn normalize_expands_cursor_column_and_drops_modes() {
        let line = "\x1b[?7l\x1b[34mOS\x1b[0m\x1b[8GUbuntu";
        assert_eq!(normalize(line), "\x1b[34mOS\x1b[0m     Ubuntu");
    }

    #[test]
    fn frame_lines_all_have_same_width() {
        let lines = vec![
            "short".to_string(),
            "\x1b[31ma much longer line\x1b[0m".to_string(),
        ];
        let boxed = frame(
            &lines,
            "me@host",
            &Style {
                border: "",
                title: "",
            },
            None,
        );
        let widths: Vec<_> = boxed.iter().map(|l| visible_width(l)).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "{widths:?}");
        assert!(plain(&boxed[0]).starts_with('╭') && plain(&boxed[0]).contains(" me@host "));
        assert_eq!(plain(&boxed[1]), "│ short               │");
        assert!(plain(boxed.last().unwrap()).starts_with('╰'));
    }

    #[test]
    fn truncate_keeps_colours_and_adds_ellipsis() {
        let t = truncate("\x1b[31mabcdef\x1b[0m", 4);
        assert_eq!(plain(&t), "abc…");
        assert!(t.starts_with("\x1b[31m"));
        assert_eq!(truncate("abc", 4), "abc");
    }

    #[test]
    fn frame_respects_max_width() {
        let lines = vec!["x".repeat(100)];
        let style = Style {
            border: "",
            title: "",
        };
        let boxed = frame(&lines, "me@host", &style, Some(40));
        assert!(boxed.iter().all(|l| visible_width(l) == 40));
        assert!(plain(&boxed[1]).ends_with("… │"));
    }

    #[test]
    fn side_by_side_centres_shorter_column() {
        let left = vec!["ab".to_string()];
        let right = vec!["1".to_string(), "2".to_string(), "3".to_string()];
        let rows: Vec<_> = side_by_side(&left, &right, 1)
            .iter()
            .map(|l| plain(l))
            .collect();
        assert_eq!(rows, vec!["   1", "ab 2", "   3"]);
    }
}
