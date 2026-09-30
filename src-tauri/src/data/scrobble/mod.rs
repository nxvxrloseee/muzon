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
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const LISTENBRAINZ_TOKEN: &str = "listenbrainz-token";
const LASTFM_SECRET: &str = "lastfm-secret";
const LASTFM_SESSION: &str = "lastfm-session";

/// How long a failed send waits before the queue is tried again, when no new
/// listen comes along to try it sooner.
const RETRY_INTERVAL: Duration = Duration::from_secs(5 * 60);

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

#[derive(Debug, Default, Serialize, Deserialize)]
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

pub struct Scrobbler {
    config_path: PathBuf,
    queue_path: PathBuf,
    inner: Mutex<Inner>,
    wake: Arc<tokio::sync::Notify>,
    http: reqwest::Client,
    listenbrainz_base: String,
    lastfm_base: String,
    /// `keyring::lookup`, except in tests, which must not touch the real one.
    secret: fn(&str) -> Option<String>,
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(path: &Path, value: &T) {
    let result = serde_json::to_string_pretty(value)
        .map_err(anyhow::Error::from)
        .and_then(|json| Ok(std::fs::write(path, json)?));
    if let Err(e) = result {
        eprintln!(
            "[muzon scrobble] не удалось сохранить {}: {e}",
            path.display()
        );
    }
}

impl Scrobbler {
    pub fn new(config_dir: &Path) -> Self {
        Self::with_bases(config_dir, listenbrainz::API, lastfm::API)
    }

    fn with_bases(config_dir: &Path, listenbrainz_base: &str, lastfm_base: &str) -> Self {
        let config_path = config_dir.join("scrobble.json");
        let queue_path = config_dir.join("scrobble-queue.json");
        Self {
            inner: Mutex::new(Inner {
                config: read_json(&config_path),
                queues: read_json(&queue_path),
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
            secret: keyring::lookup,
        }
    }

    pub fn status(&self) -> ScrobbleStatus {
        let inner = self.inner.lock().unwrap();
        ScrobbleStatus {
            listenbrainz: ServiceStatus {
                user: inner.config.listenbrainz_user.clone(),
                pending: inner.queues.listenbrainz.len() as u32,
                last_error: inner.listenbrainz_error.clone(),
            },
            lastfm: ServiceStatus {
                user: inner.config.lastfm_user.clone(),
                pending: inner.queues.lastfm.len() as u32,
                last_error: inner.lastfm_error.clone(),
            },
        }
    }

    /// Queues a counted listen for every connected service and wakes the
    /// sender. Written to disk first, so a crash can't lose it.
    pub fn listen(&self, listen: Listen) {
        let mut inner = self.inner.lock().unwrap();
        for service in [Service::ListenBrainz, Service::LastFm] {
            if inner.connected(service) {
                let queue = inner.queue(service);
                queue.push_back(listen.clone());
                while queue.len() > QUEUE_LIMIT {
                    queue.pop_front();
                }
            }
        }
        write_json(&self.queue_path, &inner.queues);
        drop(inner);
        self.wake.notify_one();
    }

    fn save_config(&self, inner: &Inner) {
        write_json(&self.config_path, &inner.config);
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
                if !inner.connected(service) {
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
            // Only the ones sent: more may have been queued meanwhile, at the back
            let queue = inner.queue(service);
            let sent = batch.len().min(queue.len());
            queue.drain(..sent);
            write_json(&self.queue_path, &inner.queues);
        }
    }

    async fn flush_all(&self) {
        self.flush(Service::ListenBrainz).await;
        self.flush(Service::LastFm).await;
    }

    async fn now_playing(&self, listen: Listen) {
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
        keyring::store(LISTENBRAINZ_TOKEN, "Muzon ListenBrainz token", token)?;
        let mut inner = self.inner.lock().unwrap();
        inner.config.listenbrainz_user = Some(user);
        inner.listenbrainz_error = None;
        self.save_config(&inner);
        Ok(())
    }

    /// Forgets the account and whatever was still waiting to be sent to it.
    pub fn disconnect_listenbrainz(&self) {
        keyring::clear(LISTENBRAINZ_TOKEN);
        let mut inner = self.inner.lock().unwrap();
        inner.config.listenbrainz_user = None;
        inner.listenbrainz_error = None;
        inner.queues.listenbrainz.clear();
        self.save_config(&inner);
        write_json(&self.queue_path, &inner.queues);
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
        keyring::store(LASTFM_SECRET, "Muzon Last.fm shared secret", &keys.secret)?;
        keyring::store(LASTFM_SESSION, "Muzon Last.fm session", &session)?;
        let mut inner = self.inner.lock().unwrap();
        inner.config.lastfm_user = Some(user);
        inner.config.lastfm_api_key = Some(keys.api_key);
        inner.lastfm_error = None;
        inner.pending_lastfm_auth = None;
        self.save_config(&inner);
        Ok(())
    }

    pub fn disconnect_lastfm(&self) {
        keyring::clear(LASTFM_SECRET);
        keyring::clear(LASTFM_SESSION);
        let mut inner = self.inner.lock().unwrap();
        inner.config.lastfm_user = None;
        inner.config.lastfm_api_key = None;
        inner.lastfm_error = None;
        inner.queues.lastfm.clear();
        self.save_config(&inner);
        write_json(&self.queue_path, &inner.queues);
    }

    pub fn wake(&self) {
        self.wake.notify_one();
    }
}

fn announce(app: &AppHandle) {
    let _ = app.emit(STATUS_EVENT, app.state::<AppState>().scrobbler.status());
}

/// The sender: flushes whenever a listen is queued, and every
/// `RETRY_INTERVAL` regardless, which is what empties a queue built up
/// offline.
pub fn start(app: AppHandle) {
    let wake = app.state::<AppState>().scrobbler.wake.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            app.state::<AppState>().scrobbler.flush_all().await;
            announce(&app);
            let _ = tokio::time::timeout(RETRY_INTERVAL, wake.notified()).await;
        }
    });
}

/// Tells the connected services what just started playing. Fire and forget.
pub fn now_playing(app: &AppHandle, listen: Listen) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        app.state::<AppState>().scrobbler.now_playing(listen).await;
    });
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
        s.secret = fake_secret;
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
}
