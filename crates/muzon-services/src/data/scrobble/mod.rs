//! Scrobbling to ListenBrainz and Last.fm.
//!
//! A counted listen (see `lib/playThreshold` on the frontend) is queued for
//! every connected service and the queue is written to disk before anything
//! is sent, so a listen made offline, or while a service is down, goes out
//! the next time it can - at the next listen, or on the retry timer.
//! "Now playing" is best effort and never queued: late, it would be wrong.
//!
//! Account names and the Last.fm API key live in `scrobble.json`; tokens,
//! the shared secret and the session key only in the system keyring.

pub mod lastfm;
pub mod listenbrainz;

use crate::data::keyring;
use crate::domain::scrobble::{Listen, ScrobbleStatus, ServiceStatus, LASTFM_BATCH};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const LISTENBRAINZ_TOKEN: &str = "listenbrainz-token";
const LASTFM_SECRET: &str = "lastfm-secret";
const LASTFM_SESSION: &str = "lastfm-session";

/// How long a failed send waits before the queue is tried again, when no new
/// listen comes along to try it sooner.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Months of listening offline; past this the oldest are let go rather than
/// growing the file without bound.
const QUEUE_LIMIT: usize = 10_000;

const LISTENBRAINZ_BATCH: usize = 100;

pub const STATUS_EVENT: &str = "scrobble-status";

/// Why a send failed, and so what to do with what was being sent.
#[derive(Debug)]
pub enum SendError {
    /// The service or the network: keep the listens and try later.
    Retry(String),
    /// The service refused these listens as such: sending them again can't
    /// help, and they would hold up everything behind them.
    Drop(String),
}

impl SendError {
    pub fn message(&self) -> &str {
        match self {
            SendError::Retry(m) | SendError::Drop(m) => m,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Config {
    listenbrainz_user: Option<String>,
    lastfm_user: Option<String>,
    lastfm_api_key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Queues {
    listenbrainz: VecDeque<Listen>,
    lastfm: VecDeque<Listen>,
}

#[derive(Clone, Copy, PartialEq)]
enum Service {
    ListenBrainz,
    LastFm,
}

struct Inner {
    config: Config,
    queues: Queues,
    listenbrainz_error: Option<String>,
    lastfm_error: Option<String>,
    /// Keys and token between `lastfm_begin_auth` and `lastfm_finish_auth`.
    pending_lastfm_auth: Option<(lastfm::AppKeys, String)>,
    storage_error: Option<String>,
}

impl Inner {
    fn queue(&mut self, service: Service) -> &mut VecDeque<Listen> {
        match service {
            Service::ListenBrainz => &mut self.queues.listenbrainz,
            Service::LastFm => &mut self.queues.lastfm,
        }
    }

    fn error(&mut self, service: Service) -> &mut Option<String> {
        match service {
            Service::ListenBrainz => &mut self.listenbrainz_error,
            Service::LastFm => &mut self.lastfm_error,
        }
    }

    fn connected(&self, service: Service) -> bool {
        match service {
            Service::ListenBrainz => self.config.listenbrainz_user.is_some(),
            Service::LastFm => self.config.lastfm_user.is_some(),
        }
    }
}

type SecretLookup = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;
pub struct Scrobbler {
    config_path: PathBuf,
    queue_path: PathBuf,
    inner: Mutex<Inner>,
    wake: Arc<tokio::sync::Notify>,
    http: reqwest::Client,
    listenbrainz_base: String,
    lastfm_base: String,
    /// `keyring::lookup`, except in tests, which must not touch the real one.
    secret: SecretLookup,
    vault: Arc<dyn keyring::Vault>,
}

fn read_json_checked<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> anyhow::Result<T> {
    match std::fs::metadata(path) {
        Ok(meta) => {
            anyhow::ensure!(
                meta.len() <= 64 * 1024 * 1024,
                "Файл очереди или настроек превышает 64 МиБ"
            );
            Ok(serde_json::from_slice(&std::fs::read(path)?)?)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error.into()),
    }
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    let result = (|| -> anyhow::Result<()> {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

impl Scrobbler {
    pub fn with_vault(config_dir: &Path, vault: Arc<dyn keyring::Vault>) -> Self {
        Self::build(config_dir, listenbrainz::API, lastfm::API, vault)
    }
    #[cfg(test)]
    fn with_bases(config_dir: &Path, listenbrainz_base: &str, lastfm_base: &str) -> Self {
        Self::build(
            config_dir,
            listenbrainz_base,
            lastfm_base,
            Arc::new(keyring::MemoryVault::default()),
        )
    }
    fn build(
        config_dir: &Path,
        listenbrainz_base: &str,
        lastfm_base: &str,
        vault: Arc<dyn keyring::Vault>,
    ) -> Self {
        let lookup = vault.clone();
        let config_path = config_dir.join("scrobble.json");
        let queue_path = config_dir.join("scrobble-queue.json");
        let config = read_json_checked(&config_path);
        let queues = read_json_checked(&queue_path);
        let storage_error = config
            .as_ref()
            .err()
            .or_else(|| queues.as_ref().err())
            .map(|e| e.to_string());
        Self {
            inner: Mutex::new(Inner {
                config: config.unwrap_or_default(),
                queues: queues.unwrap_or_default(),
                storage_error,
                listenbrainz_error: None,
                lastfm_error: None,
                pending_lastfm_auth: None,
            }),
            config_path,
            queue_path,
            wake: Arc::new(tokio::sync::Notify::new()),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .user_agent(concat!("Muzon/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
            listenbrainz_base: listenbrainz_base.to_string(),
            lastfm_base: lastfm_base.to_string(),
            secret: Arc::new(move |key| lookup.lookup(key)),
            vault,
        }
    }

    pub fn status(&self) -> ScrobbleStatus {
        let inner = self.inner.lock().unwrap();
        ScrobbleStatus {
            listenbrainz: ServiceStatus {
                user: inner.config.listenbrainz_user.clone(),
                pending: inner.queues.listenbrainz.len() as u32,
                last_error: inner
                    .storage_error
                    .clone()
                    .or_else(|| inner.listenbrainz_error.clone()),
            },
            lastfm: ServiceStatus {
                user: inner.config.lastfm_user.clone(),
                pending: inner.queues.lastfm.len() as u32,
                last_error: inner
                    .storage_error
                    .clone()
                    .or_else(|| inner.lastfm_error.clone()),
            },
        }
    }

    /// Queues a counted listen for every connected service and wakes the
    /// sender. Written to disk first, so a crash can't lose it.
    pub fn listen(&self, listen: Listen) {
        if let Err(error) = self.try_listen(listen) {
            eprintln!("[muzon scrobble] {error}");
        }
    }
    pub fn try_listen(&self, listen: Listen) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.storage_error.is_some() {
            self.recover_storage(&mut inner)?;
        }
        let before = inner.queues.clone();
        let mut changed = false;
        for service in [Service::ListenBrainz, Service::LastFm] {
            if inner.connected(service) {
                let queue = inner.queue(service);
                if !queue.contains(&listen) {
                    queue.push_back(listen.clone());
                    changed = true;
                }
                while queue.len() > QUEUE_LIMIT {
                    queue.pop_front();
                }
            }
        }
        if changed {
            if let Err(error) = write_json_atomic(&self.queue_path, &inner.queues) {
                inner.queues = before;
                return Err(error);
            }
            self.wake.notify_one();
        }
        Ok(())
    }
    pub fn notifier(&self) -> Arc<tokio::sync::Notify> {
        self.wake.clone()
    }

    fn recover_storage(&self, inner: &mut Inner) -> anyhow::Result<()> {
        let config = read_json_checked(&self.config_path)?;
        let queues = read_json_checked(&self.queue_path)?;
        inner.config = config;
        inner.queues = queues;
        inner.storage_error = None;
        Ok(())
    }

    pub fn recover(&self) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.storage_error.is_some() {
            self.recover_storage(&mut inner)?;
        }
        Ok(())
    }
    fn lastfm_keys(&self) -> Option<(lastfm::AppKeys, String)> {
        let api_key = self.inner.lock().unwrap().config.lastfm_api_key.clone()?;
        let secret = (self.secret)(LASTFM_SECRET)?;
        let session = (self.secret)(LASTFM_SESSION)?;
        Some((lastfm::AppKeys { api_key, secret }, session))
    }

    /// Sends as much of `service`'s queue as it will take, oldest first,
    /// stopping at the first failure worth retrying.
    async fn flush(&self, service: Service) {
        loop {
            let batch: Vec<Listen> = {
                let mut inner = self.inner.lock().unwrap();
                if inner.storage_error.is_some() || !inner.connected(service) {
                    return;
                }
                let size = match service {
                    Service::ListenBrainz => LISTENBRAINZ_BATCH,
                    Service::LastFm => LASTFM_BATCH,
                };
                inner.queue(service).iter().take(size).cloned().collect()
            };
            if batch.is_empty() {
                return;
            }

            let result = match service {
                Service::ListenBrainz => match (self.secret)(LISTENBRAINZ_TOKEN) {
                    Some(token) => {
                        listenbrainz::submit(
                            &self.http,
                            &self.listenbrainz_base,
                            &token,
                            &batch,
                            false,
                        )
                        .await
                    }
                    None => Err(SendError::Retry(
                        "токен ListenBrainz не найден в связке ключей".into(),
                    )),
                },
                Service::LastFm => match self.lastfm_keys() {
                    Some((keys, session)) => {
                        lastfm::scrobble(&self.http, &self.lastfm_base, &keys, &session, &batch)
                            .await
                    }
                    None => Err(SendError::Retry(
                        "ключи Last.fm не найдены в связке ключей".into(),
                    )),
                },
            };

            let mut inner = self.inner.lock().unwrap();
            let retry = match result {
                Ok(()) => {
                    *inner.error(service) = None;
                    false
                }
                Err(SendError::Drop(message)) => {
                    *inner.error(service) = Some(message);
                    false
                }
                Err(SendError::Retry(message)) => {
                    *inner.error(service) = Some(message);
                    true
                }
            };
            if retry {
                return;
            }
            // The cap may have evicted a prefix while the request was in flight.
            // Remove acknowledged identities, never a count of unrelated newer entries.
            let before = inner.queues.clone();
            inner
                .queue(service)
                .retain(|listen| !batch.contains(listen));
            if let Err(error) = write_json_atomic(&self.queue_path, &inner.queues) {
                inner.queues = before;
                *inner.error(service) = Some(format!("Не удалось сохранить очередь: {error}"));
                return;
            }
        }
    }

    pub async fn flush_all(&self) {
        self.flush(Service::ListenBrainz).await;
        self.flush(Service::LastFm).await;
    }

    pub async fn now_playing(&self, listen: Listen) {
        let (listenbrainz, lastfm) = {
            let inner = self.inner.lock().unwrap();
            (
                inner.connected(Service::ListenBrainz),
                inner.connected(Service::LastFm),
            )
        };
        if listenbrainz {
            if let Some(token) = (self.secret)(LISTENBRAINZ_TOKEN) {
                let _ = listenbrainz::submit(
                    &self.http,
                    &self.listenbrainz_base,
                    &token,
                    std::slice::from_ref(&listen),
                    true,
                )
                .await;
            }
        }
        if lastfm {
            if let Some((keys, session)) = self.lastfm_keys() {
                let _ =
                    lastfm::now_playing(&self.http, &self.lastfm_base, &keys, &session, &listen)
                        .await;
            }
        }
    }

    pub async fn connect_listenbrainz(&self, token: &str) -> anyhow::Result<()> {
        let token = token.trim();
        if token.is_empty() {
            anyhow::bail!("вставьте токен из настроек профиля ListenBrainz");
        }
        let user = listenbrainz::validate(&self.http, &self.listenbrainz_base, token).await?;
        self.vault
            .store(LISTENBRAINZ_TOKEN, "Muzon ListenBrainz token", token)?;
        let mut inner = self.inner.lock().unwrap();
        if inner.storage_error.is_some() {
            self.recover_storage(&mut inner)?;
        }
        let before = inner.config.clone();
        inner.config.listenbrainz_user = Some(user);
        inner.listenbrainz_error = None;
        if let Err(error) = write_json_atomic(&self.config_path, &inner.config) {
            inner.config = before;
            return Err(error);
        }
        if inner.storage_error.is_some() {
            self.recover_storage(&mut inner)?;
        }
        Ok(())
    }

    /// Forgets the account and whatever was still waiting to be sent to it.
    pub fn disconnect_listenbrainz(&self) -> anyhow::Result<()> {
        self.vault.clear(LISTENBRAINZ_TOKEN)?;
        let mut inner = self.inner.lock().unwrap();
        inner.config.listenbrainz_user = None;
        inner.listenbrainz_error = None;
        inner.queues.listenbrainz.clear();
        write_json_atomic(&self.config_path, &inner.config)?;
        write_json_atomic(&self.queue_path, &inner.queues)?;
        inner.storage_error = None;
        Ok(())
    }

    /// Returns the page where the user approves access.
    pub async fn lastfm_begin_auth(&self, api_key: &str, secret: &str) -> anyhow::Result<String> {
        let keys = lastfm::AppKeys {
            api_key: api_key.trim().to_string(),
            secret: secret.trim().to_string(),
        };
        if keys.api_key.is_empty() || keys.secret.is_empty() {
            anyhow::bail!("нужны API key и shared secret приложения Last.fm");
        }
        let token = lastfm::get_token(&self.http, &self.lastfm_base, &keys).await?;
        let url = lastfm::auth_url(&keys.api_key, &token);
        self.inner.lock().unwrap().pending_lastfm_auth = Some((keys, token));
        Ok(url)
    }

    pub async fn lastfm_finish_auth(&self) -> anyhow::Result<()> {
        let (keys, token) = self
            .inner
            .lock()
            .unwrap()
            .pending_lastfm_auth
            .clone()
            .ok_or_else(|| anyhow::anyhow!("сначала откройте страницу подтверждения Last.fm"))?;
        let (user, session) =
            lastfm::get_session(&self.http, &self.lastfm_base, &keys, &token).await?;
        self.vault
            .store(LASTFM_SECRET, "Muzon Last.fm shared secret", &keys.secret)?;
        self.vault
            .store(LASTFM_SESSION, "Muzon Last.fm session", &session)?;
        let mut inner = self.inner.lock().unwrap();
        if inner.storage_error.is_some() {
            self.recover_storage(&mut inner)?;
        }
        let before = inner.config.clone();
        inner.config.lastfm_user = Some(user);
        inner.config.lastfm_api_key = Some(keys.api_key.clone());
        inner.lastfm_error = None;
        if let Err(error) = write_json_atomic(&self.config_path, &inner.config) {
            inner.config = before;
            return Err(error);
        }
        inner.pending_lastfm_auth = None;
        if inner.storage_error.is_some() {
            self.recover_storage(&mut inner)?;
        }
        Ok(())
    }

    pub fn disconnect_lastfm(&self) -> anyhow::Result<()> {
        self.vault.clear(LASTFM_SECRET)?;
        self.vault.clear(LASTFM_SESSION)?;
        let mut inner = self.inner.lock().unwrap();
        inner.config.lastfm_user = None;
        inner.config.lastfm_api_key = None;
        inner.lastfm_error = None;
        inner.queues.lastfm.clear();
        write_json_atomic(&self.config_path, &inner.config)?;
        write_json_atomic(&self.queue_path, &inner.queues)?;
        inner.storage_error = None;
        Ok(())
    }

    pub fn wake(&self) {
        self.wake.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{fake_http_server, scratch_dir};

    fn listen(track: &str) -> Listen {
        Listen {
            artist: "A".into(),
            track: track.into(),
            album: None,
            duration_secs: None,
            track_no: None,
            listened_at: 1,
        }
    }

    /// A scrobbler that believes it is connected to ListenBrainz as `user`,
    /// without touching the real keyring - `flush` is exercised through
    /// `submit` directly in `listenbrainz`'s tests, so these only look at the
    /// queue's own bookkeeping.
    fn connected(dir: &Path) -> Scrobbler {
        let s = Scrobbler::with_bases(dir, "http://127.0.0.1:1", "http://127.0.0.1:1");
        s.inner.lock().unwrap().config.listenbrainz_user = Some("user".into());
        s
    }

    #[test]
    fn listens_are_queued_only_for_connected_services_and_survive_a_restart() {
        let dir = scratch_dir("scrobble-queue");
        let s = connected(&dir);
        s.listen(listen("one"));
        s.listen(listen("two"));
        let status = s.status();
        assert_eq!(status.listenbrainz.pending, 2);
        assert_eq!(status.lastfm.pending, 0);

        let reopened = Scrobbler::with_bases(&dir, "x", "x");
        assert_eq!(reopened.inner.lock().unwrap().queues.listenbrainz.len(), 2);
    }

    #[test]
    fn the_queue_lets_the_oldest_go_past_its_limit() {
        let dir = scratch_dir("scrobble-limit");
        let s = connected(&dir);
        for i in 0..QUEUE_LIMIT + 3 {
            s.inner
                .lock()
                .unwrap()
                .queues
                .listenbrainz
                .push_back(listen(&i.to_string()));
        }
        s.listen(listen("newest"));
        let inner = s.inner.lock().unwrap();
        assert_eq!(inner.queues.listenbrainz.len(), QUEUE_LIMIT);
        assert_eq!(inner.queues.listenbrainz.back().unwrap().track, "newest");
    }

    fn fake_secret(_: &str) -> Option<String> {
        Some("tok".into())
    }

    fn connected_to(dir: &Path, base: &str) -> Scrobbler {
        let mut s = Scrobbler::with_bases(dir, base, "x");
        s.secret = Arc::new(fake_secret);
        s.inner.lock().unwrap().config.listenbrainz_user = Some("user".into());
        s
    }

    #[tokio::test]
    async fn a_flush_sends_the_queue_and_empties_it() {
        let dir = scratch_dir("scrobble-flush");
        let (base, seen) = fake_http_server(vec![(200, r#"{"status":"ok"}"#.into())]);
        let s = connected_to(&dir, &base);
        s.listen(listen("one"));
        s.listen(listen("two"));

        s.flush(Service::ListenBrainz).await;

        let body: serde_json::Value = serde_json::from_str(&seen.recv().unwrap().body).unwrap();
        assert_eq!(body["listen_type"], "import");
        assert_eq!(body["payload"].as_array().unwrap().len(), 2);
        assert_eq!(s.status().listenbrainz.pending, 0);
        assert!(s.status().listenbrainz.last_error.is_none());
    }

    #[tokio::test]
    async fn a_retryable_failure_keeps_the_queue_and_says_why() {
        let dir = scratch_dir("scrobble-retry");
        let (base, _seen) = fake_http_server(vec![(503, r#"{"error":"down"}"#.into())]);
        let s = connected_to(&dir, &base);
        s.listen(listen("one"));

        s.flush(Service::ListenBrainz).await;

        let status = s.status().listenbrainz;
        assert_eq!(status.pending, 1);
        assert!(status.last_error.unwrap().contains("down"));
    }

    #[tokio::test]
    async fn refused_listens_leave_the_queue_so_the_rest_can_go() {
        let dir = scratch_dir("scrobble-drop");
        let (base, _seen) = fake_http_server(vec![(400, r#"{"error":"bad"}"#.into())]);
        let s = connected_to(&dir, &base);
        s.listen(listen("one"));

        s.flush(Service::ListenBrainz).await;

        assert_eq!(s.status().listenbrainz.pending, 0);
        assert!(s.status().listenbrainz.last_error.is_some());
    }

    #[tokio::test]
    async fn connecting_listenbrainz_needs_a_token_the_service_accepts() {
        let dir = scratch_dir("scrobble-connect");
        let (base, _seen) = fake_http_server(vec![(200, r#"{"valid":false}"#.into())]);
        let s = Scrobbler::with_bases(&dir, &base, "x");
        assert!(s.connect_listenbrainz("bad").await.is_err());
        assert!(s.status().listenbrainz.user.is_none());
        assert!(s.connect_listenbrainz("   ").await.is_err());
    }
    #[test]
    fn corrupt_queue_is_preserved_until_explicit_repair() {
        let dir = scratch_dir("scrobble-corrupt-recovery");
        let path = dir.join("scrobble-queue.json");
        std::fs::write(&path, b"{broken").unwrap();
        let s = connected(&dir);
        assert!(s.try_listen(listen("new")).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
        std::fs::write(&path, b"{}").unwrap();
        s.recover().unwrap();
        s.inner.lock().unwrap().config.listenbrainz_user = Some("user".into());
        s.try_listen(listen("new")).unwrap();
        assert_eq!(s.status().listenbrainz.pending, 1);
    }

    #[test]
    fn failed_enqueue_rolls_back_memory_and_can_be_retried() {
        let dir = scratch_dir("scrobble-enqueue-write-failure");
        let s = connected(&dir);
        let temporary = s.queue_path.with_extension("json.tmp");
        std::fs::create_dir(&temporary).unwrap();
        assert!(s.try_listen(listen("new")).is_err());
        assert_eq!(s.status().listenbrainz.pending, 0);
        std::fs::remove_dir(&temporary).unwrap();
        s.try_listen(listen("new")).unwrap();
        s.try_listen(listen("new")).unwrap();
        assert_eq!(s.status().listenbrainz.pending, 1);
    }

    #[tokio::test]
    async fn acknowledgement_write_failure_retains_pending_records() {
        let dir = scratch_dir("scrobble-ack-write-failure");
        let (base, seen) = fake_http_server(vec![(200, r#"{"status":"ok"}"#.into())]);
        let s = connected_to(&dir, &base);
        s.try_listen(listen("one")).unwrap();
        let before = std::fs::read(&s.queue_path).unwrap();
        std::fs::create_dir(s.queue_path.with_extension("json.tmp")).unwrap();
        s.flush(Service::ListenBrainz).await;
        seen.recv().unwrap();
        assert_eq!(s.status().listenbrainz.pending, 1);
        assert!(s.status().listenbrainz.last_error.is_some());
        assert_eq!(std::fs::read(&s.queue_path).unwrap(), before);
    }
    #[tokio::test]
    async fn cancelling_in_flight_flush_keeps_durable_queue_for_restart() {
        use std::io::{Read, Write};
        use std::sync::mpsc;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (seen_tx, seen_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            // A response is deliberately withheld until the client task has been joined.
            let mut buffer = [0; 8192];
            assert!(stream.read(&mut buffer).unwrap() > 0);
            seen_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
        });
        let dir = scratch_dir("scrobble-cancel-restart");
        let s = Arc::new(connected_to(&dir, &base));
        s.try_listen(listen("pending")).unwrap();
        let before = std::fs::read(&s.queue_path).unwrap();
        let copy = s.clone();
        let task = tokio::spawn(async move { copy.flush_all().await });
        tokio::task::spawn_blocking(move || seen_rx.recv_timeout(Duration::from_secs(5)).unwrap())
            .await
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(std::fs::read(&s.queue_path).unwrap(), before);
        let reopened = Scrobbler::with_bases(&dir, "x", "x");
        assert_eq!(
            reopened.inner.lock().unwrap().queues.listenbrainz.front(),
            Some(&listen("pending"))
        );
        release_tx.send(()).unwrap();
        server.join().unwrap();
    }

    #[tokio::test]
    async fn in_flight_acknowledgement_does_not_remove_newer_eviction_replacement() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::sync::mpsc;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (seen_tx, seen_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
            }
            reader.read_exact(&mut vec![0; length]).unwrap();
            seen_tx.send(()).unwrap();
            release_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            )
            .unwrap();
            // Further sends fail, keeping the newer record pending for retry.
        });
        let dir = scratch_dir("scrobble-inflight-eviction");
        let s = Arc::new(connected_to(&dir, &base));
        s.try_listen(listen("old")).unwrap();
        let sending = s.clone();
        let task = tokio::spawn(async move {
            sending.flush(Service::ListenBrainz).await;
        });
        tokio::task::spawn_blocking(move || {
            seen_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
        })
        .await
        .unwrap();
        // Simulate the prefix eviction that the bounded queue performs at capacity.
        s.inner.lock().unwrap().queues.listenbrainz.pop_front();
        s.try_listen(listen("newer")).unwrap();
        release_tx.send(()).unwrap();
        task.await.unwrap();
        server.join().unwrap();
        assert_eq!(
            s.inner.lock().unwrap().queues.listenbrainz.front(),
            Some(&listen("newer"))
        );
        assert_eq!(s.status().listenbrainz.pending, 1);
    }
}
