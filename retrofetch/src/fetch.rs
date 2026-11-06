//! Run fastfetch with the rendered sprite as a raw logo, vertically centred.

use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// Returns `Ok(false)` if fastfetch isn't installed (caller falls back).
/// The profile's fastfetch config, unless the user passed their own after `--`
/// (fastfetch refuses to load two).
fn config_args<'a>(config: Option<&'a Path>, extra_args: &[String]) -> Vec<&'a OsStr> {
    let user_config = extra_args
        .iter()
        .any(|a| a == "-c" || a == "--config" || a.starts_with("--config="));
    match config {
        Some(path) if !user_config => vec!["--config".as_ref(), path.as_os_str()],
        _ => Vec::new(),
    }
}

/// Run fastfetch without a logo and return its (coloured) output lines,
/// or `None` if fastfetch isn't installed.
pub fn capture(config: Option<&Path>, extra_args: &[String]) -> Result<Option<Vec<String>>> {
    let out = Command::new("fastfetch")
        .args(config_args(config, extra_args))
        .args(["--logo", "none", "--pipe", "false"])
        .args(extra_args)
        .stderr(Stdio::inherit())
        .output();
    let out = match out {
        Ok(out) => out,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).context("failed to run fastfetch"),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    while lines
        .last()
        .is_some_and(|l| crate::boxed::visible_width(l) == 0)
    {
        lines.pop();
    }
    Ok(Some(lines))
}

pub fn run(logo: &[String], config: Option<&Path>, extra_args: &[String]) -> Result<bool> {
    let config_args = config_args(config, extra_args);
    let probe = Command::new("fastfetch")
        .args(&config_args)
        .args(["--logo", "none"])
        .args(extra_args)
        .stderr(Stdio::null())
        .output();
    let probe = match probe {
        Ok(out) => out,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).context("failed to run fastfetch"),
    };

    let fetch_height = probe.stdout.iter().filter(|&&b| b == b'\n').count();
    let pad_top = fetch_height.saturating_sub(logo.len()) / 2;

    let mut child = Command::new("fastfetch")
        .args(&config_args)
        .args(["--file-raw", "-"])
        .args(["--logo-padding-top", &pad_top.to_string()])
        .args(["--logo-padding-left", "2", "--logo-padding-right", "3"])
        // fastfetch >= 2.6x no longer disables line wrap by default; a wrapped
        // long value would spill onto the next row and draw over the sprite.
        .args(["--disable-linewrap", "true"])
        .args(extra_args)
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to spawn fastfetch")?;

    {
        let mut stdin = child.stdin.take().expect("piped stdin");
        let mut text = logo.join("\n");
        text.push('\n');
        stdin.write_all(text.as_bytes())?;
    } // drop closes stdin so fastfetch sees EOF

    child.wait().context("fastfetch failed")?;
    Ok(true)
}
