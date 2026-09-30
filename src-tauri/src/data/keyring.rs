//! Secrets in the system keyring: S3 keys, scrobbling tokens.
//!
//! Through `secret-tool` (libsecret) rather than a keyring crate: the session
//! already runs gnome-keyring, the binary is part of the desktop, and this way
//! the package gains no new build dependency. Secrets never touch Muzon's
//! config files, so a backup of ~/.config can't leak them.

use std::io::Write;
use std::process::{Command, Stdio};

const SERVICE: &str = "muzon";

pub fn lookup(attr: &str) -> Option<String> {
    let out = Command::new("secret-tool")
        .args(["lookup", "service", SERVICE, "key", attr])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string();
    (!value.is_empty()).then_some(value)
}

pub fn store(attr: &str, label: &str, value: &str) -> anyhow::Result<()> {
    let mut child = Command::new("secret-tool")
        .args(["store", "--label", label, "service", SERVICE, "key", attr])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("не удалось запустить secret-tool: {e}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("secret-tool не принимает ввод"))?
        .write_all(value.as_bytes())?;
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("secret-tool не сохранил ключ (код {status})");
    }
    Ok(())
}

pub fn clear(attr: &str) {
    let _ = Command::new("secret-tool")
        .args(["clear", "service", SERVICE, "key", attr])
        .status();
}
