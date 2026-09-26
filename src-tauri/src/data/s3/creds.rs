//! Access keys in the system keyring.
//!
//! Through `secret-tool` (libsecret) rather than a keyring crate: the session
//! already runs gnome-keyring, the binary is part of the desktop, and this way
//! the package gains no new build dependency. The key never touches Muzon's
//! config files, so a backup of ~/.config can't leak it.

use super::sigv4::Credentials;
use std::io::Write;
use std::process::{Command, Stdio};

const SERVICE: &str = "muzon";
const ACCESS: &str = "s3-access-key";
const SECRET: &str = "s3-secret-key";

fn lookup(attr: &str) -> Option<String> {
    let out = Command::new("secret-tool")
        .args(["lookup", "service", SERVICE, "key", attr])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string();
    (!value.is_empty()).then_some(value)
}

fn store(attr: &str, label: &str, value: &str) -> anyhow::Result<()> {
    let mut child = Command::new("secret-tool")
        .args([
            "store", "--label", label, "service", SERVICE, "key", attr,
        ])
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

fn clear(attr: &str) {
    let _ = Command::new("secret-tool")
        .args(["clear", "service", SERVICE, "key", attr])
        .status();
}

pub fn load() -> anyhow::Result<Credentials> {
    let access_key = lookup(ACCESS)
        .ok_or_else(|| anyhow::anyhow!("ключи доступа не заданы: сохраните их в настройках"))?;
    let secret_key = lookup(SECRET)
        .ok_or_else(|| anyhow::anyhow!("ключи доступа не заданы: сохраните их в настройках"))?;
    Ok(Credentials {
        access_key,
        secret_key,
        session_token: String::new(),
    })
}

pub fn save(access_key: &str, secret_key: &str) -> anyhow::Result<()> {
    if access_key.trim().is_empty() || secret_key.trim().is_empty() {
        anyhow::bail!("ключ и секрет не могут быть пустыми");
    }
    store(ACCESS, "Muzon S3 access key", access_key.trim())?;
    store(SECRET, "Muzon S3 secret key", secret_key.trim())?;
    Ok(())
}

pub fn forget() {
    clear(ACCESS);
    clear(SECRET);
}

pub fn are_set() -> bool {
    lookup(ACCESS).is_some() && lookup(SECRET).is_some()
}
