//! Run fastfetch with the rendered sprite as a raw logo, vertically centred.

use std::io::{self, Write};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// Returns `Ok(false)` if fastfetch isn't installed (caller falls back).
pub fn run(logo: &[String], extra_args: &[String]) -> Result<bool> {
    let probe = Command::new("fastfetch")
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
        .args(["--file-raw", "-"])
        .args(["--logo-padding-top", &pad_top.to_string()])
        .args(["--logo-padding-left", "2", "--logo-padding-right", "3"])
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
