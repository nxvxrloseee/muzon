//! S3 access keys, kept in the system keyring (see `data::keyring`).

use super::sigv4::Credentials;
use crate::data::keyring::{clear, lookup, store};

const ACCESS: &str = "s3-access-key";
const SECRET: &str = "s3-secret-key";

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

pub fn load_with(vault: &dyn crate::data::keyring::Vault) -> anyhow::Result<Credentials> {
    Ok(Credentials {
        access_key: vault
            .lookup(ACCESS)
            .ok_or_else(|| anyhow::anyhow!("Ключи не заданы или недоступны"))?,
        secret_key: vault
            .lookup(SECRET)
            .ok_or_else(|| anyhow::anyhow!("Ключи не заданы или недоступны"))?,
        session_token: String::new(),
    })
}
pub fn save_with(
    vault: &dyn crate::data::keyring::Vault,
    access: &str,
    secret: &str,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !access.trim().is_empty() && !secret.trim().is_empty(),
        "Ключ и секрет не могут быть пустыми"
    );
    vault.store(ACCESS, "Muzon native S3 access", access.trim())?;
    vault.store(SECRET, "Muzon native S3 secret", secret.trim())?;
    Ok(())
}
pub fn forget_with(vault: &dyn crate::data::keyring::Vault) -> anyhow::Result<()> {
    vault.clear(ACCESS)?;
    vault.clear(SECRET)
}
