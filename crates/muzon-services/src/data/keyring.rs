//! Profile-scoped secret-tool adapter. Values only travel over stdin/stdout.
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub trait Vault: Send + Sync {
    fn lookup(&self, key: &str) -> Option<String>;
    fn store(&self, key: &str, label: &str, value: &str) -> anyhow::Result<()>;
    fn clear(&self, key: &str) -> anyhow::Result<()>;
}
pub struct Keyring {
    service: String,
}
impl Keyring {
    pub fn for_profile(profile: &Path) -> anyhow::Result<Self> {
        let path = profile.canonicalize()?;
        Ok(Self {
            service: format!(
                "muzon-native-{}",
                hex::encode(Sha256::digest(path.as_os_str().as_encoded_bytes()))
            ),
        })
    }
    pub(crate) fn legacy() -> Self {
        Self {
            service: "muzon".into(),
        }
    }
    fn run(
        &self,
        action: &str,
        key: &str,
        label: Option<&str>,
        input: Option<&str>,
    ) -> anyhow::Result<Vec<u8>> {
        anyhow::ensure!(
            input.is_none_or(|value| value.len() <= 4096),
            "Слишком длинный ключ"
        );
        let mut command = Command::new("secret-tool");
        command.arg(action);
        if let Some(label) = label {
            command.args(["--label", label]);
        }
        command
            .args(["service", &self.service, "key", key])
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|e| anyhow::anyhow!("Не удалось запустить secret-tool: {e}"))?;
        let result = (|| -> anyhow::Result<Vec<u8>> {
            use std::io::Read;
            if let Some(value) = input {
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(value.as_bytes())?;
                }
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(status) = child.try_wait()? {
                    let mut output = Vec::new();
                    if let Some(mut stdout) = child.stdout.take() {
                        stdout.read_to_end(&mut output)?;
                    }
                    let mut errors = Vec::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        stderr.read_to_end(&mut errors)?;
                    }
                    if !status.success() {
                        use zeroize::Zeroize;
                        output.zeroize();
                        if action == "clear" && status.code() == Some(1) && errors.is_empty() {
                            return Ok(Vec::new());
                        }
                        anyhow::bail!("Системная связка ключей не выполнила операцию");
                    }
                    return Ok(output);
                }
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "Связка ключей не ответила за 10 секунд"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        })();
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        result
    }
}
impl Vault for Keyring {
    fn lookup(&self, key: &str) -> Option<String> {
        let bytes = self.run("lookup", key, None, None).ok()?;
        let mut value = String::from_utf8(bytes).ok()?;
        while value.ends_with('\n') {
            value.pop();
        }
        (!value.is_empty()).then_some(value)
    }
    fn store(&self, key: &str, label: &str, value: &str) -> anyhow::Result<()> {
        anyhow::ensure!(value.len() <= 4096, "Слишком длинный ключ");
        self.run("store", key, Some(label), Some(value)).map(|_| ())
    }
    fn clear(&self, key: &str) -> anyhow::Result<()> {
        self.run("clear", key, None, None).map(|_| ())
    }
}
// Compatibility helpers for the ported modules; the native hub always injects
// an explicit profile vault instead of using these default service names.
pub fn lookup(key: &str) -> Option<String> {
    Keyring::legacy().lookup(key)
}
pub fn store(key: &str, label: &str, value: &str) -> anyhow::Result<()> {
    Keyring::legacy().store(key, label, value)
}
pub fn clear(key: &str) {
    let _ = Keyring::legacy().clear(key);
}
#[cfg(test)]
pub struct MemoryVault(pub std::sync::Mutex<std::collections::HashMap<String, String>>);
#[cfg(test)]
impl Default for MemoryVault {
    fn default() -> Self {
        Self(std::sync::Mutex::new(std::collections::HashMap::new()))
    }
}
#[cfg(test)]
impl Vault for MemoryVault {
    fn lookup(&self, key: &str) -> Option<String> {
        self.0.lock().unwrap().get(key).cloned()
    }
    fn store(&self, key: &str, _: &str, value: &str) -> anyhow::Result<()> {
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn clear(&self, key: &str) -> anyhow::Result<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_namespaces_are_stable_and_distinct_without_accessing_keyring() {
        let a = crate::testing::scratch_dir("vault-profile-a");
        let b = crate::testing::scratch_dir("vault-profile-b");
        let first = Keyring::for_profile(&a).unwrap();
        assert_eq!(
            first.service,
            Keyring::for_profile(&a.join(".")).unwrap().service
        );
        assert_ne!(first.service, Keyring::for_profile(&b).unwrap().service);
        assert_ne!(first.service, Keyring::legacy().service);
        assert_eq!(
            format!("{:?}", crate::Secret("hidden-token".into())),
            "[redacted]"
        );
    }
}
