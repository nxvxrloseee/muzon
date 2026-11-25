//! Owns network workers. Playback communicates through a durable DB outbox.
use crate::{
    data::{
        db::Db,
        keyring::{Keyring, Vault},
        s3::{
            self,
            sync::{SyncOutcome, SyncProgress},
        },
        s3_config_store::S3ConfigStore,
        scrobble::Scrobbler,
    },
    domain::{
        scrobble::{Listen, ScrobbleStatus},
        S3Config,
    },
};
use muzon_core::controller::ControllerClient;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub use tokio_util::sync::CancellationToken;
pub struct Hub {
    pub scrobbler: Arc<Scrobbler>,
    vault: Arc<dyn Vault>,
    config: Mutex<S3Config>,
    store: S3ConfigStore,
    core: ControllerClient,
    db: Arc<Db>,
    progress: Arc<Mutex<Option<SyncProgress>>>,
    error: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    force: Arc<AtomicBool>,
    cancel: CancellationToken,
    wake: Arc<tokio::sync::Notify>,
    worker: Mutex<Option<JoinHandle<()>>>,
    sync_serial: tokio::sync::Mutex<()>,
}
impl Hub {
    pub fn new(profile: &Path, db: Arc<Db>, core: ControllerClient) -> anyhow::Result<Self> {
        std::fs::create_dir_all(profile.join("config"))?;
        let vault = Arc::new(Keyring::for_profile(profile)?);
        Self::with_vault(profile, db, core, vault)
    }
    pub fn with_vault(
        profile: &Path,
        db: Arc<Db>,
        core: ControllerClient,
        vault: Arc<dyn Vault>,
    ) -> anyhow::Result<Self> {
        let store = S3ConfigStore::new(&profile.join("config"));
        let config = Mutex::new(store.load_or_default());
        let scrobbler = Arc::new(Scrobbler::with_vault(
            &profile.join("config"),
            vault.clone(),
        ));
        let progress = Arc::new(Mutex::new(None));
        let error = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(tokio::sync::Notify::new());
        let force = Arc::new(AtomicBool::new(false));
        let cancel = CancellationToken::new();
        let thread_force = force.clone();
        let stopping_token = cancel.clone();
        let (scrobble, thread_db, thread_core, stopping, stop_wake, thread_error) = (
            scrobbler.clone(),
            db.clone(),
            core.clone(),
            stop.clone(),
            wake.clone(),
            error.clone(),
        );
        let worker=std::thread::Builder::new().name("muzon-services".into()).spawn(move||{
            let runtime=match tokio::runtime::Builder::new_current_thread().enable_all().build(){Ok(runtime)=>runtime,Err(error)=>{*thread_error.lock().unwrap()=Some(error.to_string());return;}};
            runtime.block_on(async move {
                let mut retry=Instant::now();let mut previous=None;
                let mut flusher:Option<tokio::task::JoinHandle<()>>=None;
                let mut now_playing:Option<tokio::task::JoinHandle<()>>=None;
                loop {
                    if stopping.load(Ordering::Acquire){
                        if let Some(task)=flusher.take(){task.abort();let _=task.await;}
                        if let Some(task)=now_playing.take(){task.abort();let _=task.await;}
                        if let Err(error)=drain_all_outbox(&thread_db,&scrobble){*thread_error.lock().unwrap()=Some(error.to_string());}
                        break;
                    }
                    if flusher.as_ref().is_some_and(|task|task.is_finished()){
                        if let Some(task)=flusher.take(){if let Err(error)=task.await{*thread_error.lock().unwrap()=Some(error.to_string());}}
                        retry=Instant::now()+crate::data::scrobble::RETRY_INTERVAL;
                    }
                    if thread_force.swap(false,Ordering::AcqRel){retry=Instant::now();previous=None;if let Err(error)=scrobble.recover(){*thread_error.lock().unwrap()=Some(error.to_string());}else{*thread_error.lock().unwrap()=None;}}
                    match drain_outbox(&thread_db,&scrobble){Ok(true)=>retry=Instant::now(),Ok(false)=>{},Err(error)=>*thread_error.lock().unwrap()=Some(error.to_string())}
                    let snapshot=thread_core.snapshot();
                    let identity=if snapshot.is_playing{snapshot.current_path.clone().map(|path|(path,snapshot.cursor))}else{None};
                    if identity!=previous {
                        if let Some(task)=now_playing.take(){task.abort();let _=task.await;}
                        if let Some(track)=snapshot.current_track().filter(|_|identity.is_some()){
                            let now=SystemTime::now().duration_since(UNIX_EPOCH).map(|d|d.as_secs_f64()).unwrap_or(0.);
                            if let Some(listen)=Listen::from_track(track,(now-snapshot.position_secs/snapshot.tempo.max(0.01)).round()as i64){let copy=scrobble.clone();now_playing=Some(tokio::spawn(async move{copy.now_playing(listen).await;}));}
                        }
                    }
                    previous=identity;
                    if flusher.is_none()&&Instant::now()>=retry{let copy=scrobble.clone();flusher=Some(tokio::spawn(async move{copy.flush_all().await;}));}
                    tokio::select!{_=stopping_token.cancelled()=>{},_=stop_wake.notified()=>{},_=tokio::time::sleep(Duration::from_secs(1))=>{}}
                }
            });
        })?;
        Ok(Self {
            scrobbler,
            vault,
            config,
            store,
            core,
            db,
            progress,
            error,
            stop,
            force,
            cancel,
            wake,
            worker: Mutex::new(Some(worker)),
            sync_serial: tokio::sync::Mutex::new(()),
        })
    }
    pub fn status(&self) -> ScrobbleStatus {
        self.scrobbler.status()
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
    pub fn config(&self) -> S3Config {
        self.config.lock().unwrap().clone()
    }
    pub fn configure(&self, mut cfg: S3Config) -> anyhow::Result<()> {
        cfg.endpoint = cfg.endpoint.trim().to_owned();
        cfg.bucket = cfg.bucket.trim().to_owned();
        cfg.region = cfg.region.trim().to_owned();
        if cfg.region.is_empty() {
            cfg.region = "us-east-1".into();
        }
        cfg.prefix = cfg.prefix.trim().to_owned();
        anyhow::ensure!(
            cfg.region.len() <= 64
                && cfg
                    .region
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "Некорректный регион"
        );
        anyhow::ensure!(
            cfg.bucket.len() <= 255
                && !matches!(cfg.bucket.as_str(), "." | "..")
                && cfg
                    .bucket
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')),
            "Некорректное имя бакета"
        );
        anyhow::ensure!(
            !cfg.prefix.split('/').any(|part| matches!(part, "." | "..")),
            "Префикс не должен содержать . или .."
        );
        let mut validate = cfg.clone();
        if validate.bucket.is_empty() {
            validate.bucket = "validation-only".into();
        }
        let _ = s3::client::S3Client::new(
            validate,
            s3::sigv4::Credentials {
                access_key: String::new(),
                secret_key: String::new(),
                session_token: String::new(),
            },
        )?;
        self.store.save(&cfg)?;
        *self.config.lock().unwrap() = cfg;
        Ok(())
    }
    pub fn save_keys(&self, access: &str, secret: &str) -> anyhow::Result<()> {
        s3::creds::save_with(self.vault.as_ref(), access, secret)
    }
    pub fn forget_keys(&self) -> anyhow::Result<()> {
        s3::creds::forget_with(self.vault.as_ref())
    }
    pub async fn check(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.cancel.is_cancelled(), "Сервисы остановлены");
        anyhow::ensure!(self.config().is_configured(), "Сначала настройте бакет");
        let vault = self.vault.clone();
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => anyhow::bail!("Сервисы остановлены"),
            result = async {
                let credentials = tokio::task::spawn_blocking(move || s3::creds::load_with(vault.as_ref())).await??;
                s3::client::S3Client::new(self.config(), credentials)?.check_access().await
            } => result,
        }
    }

    pub fn progress(&self) -> Option<SyncProgress> {
        self.progress.lock().unwrap().clone()
    }
    pub async fn sync(&self, cancel: CancellationToken) -> anyhow::Result<SyncOutcome> {
        let _guard = self
            .sync_serial
            .try_lock()
            .map_err(|_| anyhow::anyhow!("Синхронизация уже выполняется"))?;
        anyhow::ensure!(!self.cancel.is_cancelled(), "Сервисы остановлены");
        *self.progress.lock().unwrap() = None;
        let cfg = self.config();
        anyhow::ensure!(cfg.is_configured(), "Сначала настройте бакет");
        let vault = self.vault.clone();
        let creds = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => anyhow::bail!("Сервисы остановлены"),
            _ = cancel.cancelled() => anyhow::bail!("Синхронизация отменена"),
            result = tokio::task::spawn_blocking(move || s3::creds::load_with(vault.as_ref())) => result??,
        };
        anyhow::ensure!(!cancel.is_cancelled(), "Синхронизация отменена");
        let progress = self.progress.clone();
        let callback = Arc::new(move |value| *progress.lock().unwrap() = Some(value));
        let core = self.core.clone();
        let shutdown = self.cancel.child_token();
        // Dropping the sync future must also stop an already-started blocking scan waiter.
        let _cancel_scan_on_drop = shutdown.clone().drop_guard();
        let scan_cancel = cancel.clone();
        let scan = Arc::new(move || -> anyhow::Result<()> {
            anyhow::ensure!(
                !shutdown.is_cancelled() && !scan_cancel.is_cancelled(),
                "Сканирование отменено"
            );
            core.rescan();
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                anyhow::ensure!(
                    !shutdown.is_cancelled() && !scan_cancel.is_cancelled(),
                    "Сканирование отменено"
                );
                let snapshot = core.barrier().map_err(anyhow::Error::msg)?;
                if !snapshot.scanning {
                    return Ok(());
                }
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "Сканирование не завершилось за две минуты"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        let result = tokio::select! {biased; _=self.cancel.cancelled()=>Err(anyhow::anyhow!("Сервисы остановлены")), _=cancel.cancelled()=>Err(anyhow::anyhow!("Синхронизация отменена")),result=s3::sync::run(self.db.clone(),cfg,creds,callback,scan)=>result};
        self.core.refresh_library();
        result
    }
    pub fn retry(&self) {
        self.force.store(true, Ordering::Release);
        self.wake.notify_one();
    }
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        self.cancel.cancel();
        self.wake.notify_one();
        if let Some(worker) = self.worker.lock().unwrap().take() {
            if worker.join().is_err() {
                *self.error.lock().unwrap() =
                    Some("Service worker panicked during shutdown".into());
            }
        }
    }
}
impl Drop for Hub {
    fn drop(&mut self) {
        self.shutdown();
    }
}
/// Shutdown must transfer every batch durably before joining. A failed write
/// leaves the unacknowledged suffix in SQLite for the next launch.
fn drain_all_outbox(db: &Db, scrobbler: &Scrobbler) -> anyhow::Result<()> {
    while drain_outbox(db, scrobbler)? {}
    Ok(())
}
fn drain_outbox(db: &Db, scrobbler: &Scrobbler) -> anyhow::Result<bool> {
    let pending = db.pending_listens(100)?;
    let changed = !pending.is_empty();
    for (id, listen) in pending {
        scrobbler.try_listen(listen)?;
        db.acknowledge_listen(id)?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muzon_core::{
        controller::{Controller, CoreConfig},
        data::{playback_settings_store::PlaybackSettingsStore, session_store::SessionStore},
    };
    use std::sync::atomic::AtomicUsize;
    #[derive(Default)]
    struct SpyVault(AtomicUsize);
    impl Vault for SpyVault {
        fn lookup(&self, _: &str) -> Option<String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            None
        }
        fn store(&self, _: &str, _: &str, _: &str) -> anyhow::Result<()> {
            panic!("unexpected credential write")
        }
        fn clear(&self, _: &str) -> anyhow::Result<()> {
            panic!("unexpected credential removal")
        }
    }
    #[test]
    fn disconnected_hub_never_looks_up_keys_and_shutdown_is_idempotent() {
        let _gst = crate::testing::gst_lock();
        let dir = crate::testing::scratch_dir("services-hub-disconnected");
        std::fs::create_dir_all(dir.join("config")).unwrap();
        let db = Arc::new(Db::open(&dir.join("muzon.sqlite3")).unwrap());
        let mut core = Controller::spawn_test(CoreConfig {
            db: db.clone(),
            session_store: SessionStore::new(&dir.join("config")),
            playback_settings_store: PlaybackSettingsStore::new(&dir.join("config")),
            cover_dir: dir.join("covers"),
        })
        .unwrap();
        let vault = Arc::new(SpyVault::default());
        let hub = Hub::with_vault(&dir, db, core.client(), vault.clone()).unwrap();
        hub.retry();
        // A worker barrier would concern the audio thread only. Shutdown joins the
        // service thread, including its initial/final drain and cancelled tasks.
        hub.shutdown();
        hub.shutdown();
        assert_eq!(vault.0.load(Ordering::SeqCst), 0);
        assert!(hub.error().is_none(), "{:?}", hub.error());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(runtime
            .block_on(hub.sync(CancellationToken::new()))
            .is_err());
        assert_eq!(vault.0.load(Ordering::SeqCst), 0);
        core.shutdown();
    }
    #[test]
    fn shutdown_cancels_an_active_s3_request_and_joins_service_worker() {
        // Serialize GStreamer users outside the executor rather than holding
        // a synchronous guard inside an async test future.
        let _gst = crate::testing::gst_lock();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                use std::{
                    io::{Read, Write},
                    sync::mpsc,
                };
                struct TestVault;
                impl Vault for TestVault {
                    fn lookup(&self, _: &str) -> Option<String> {
                        Some("synthetic-test-value".into())
                    }
                    fn store(&self, _: &str, _: &str, _: &str) -> anyhow::Result<()> {
                        unreachable!()
                    }
                    fn clear(&self, _: &str) -> anyhow::Result<()> {
                        unreachable!()
                    }
                }
                let dir = crate::testing::scratch_dir("hub-active-s3-shutdown");
                let root = dir.join("Music");
                std::fs::create_dir_all(&root).unwrap();
                let db = Arc::new(Db::open(&dir.join("muzon.sqlite3")).unwrap());
                db.add_root(&root).unwrap();
                let mut core = Controller::spawn_test(CoreConfig {
                    db: db.clone(),
                    session_store: SessionStore::new(&dir.join("config")),
                    playback_settings_store: PlaybackSettingsStore::new(&dir.join("config")),
                    cover_dir: dir.join("covers"),
                })
                .unwrap();
                let hub = Arc::new(
                    Hub::with_vault(&dir, db, core.client(), Arc::new(TestVault)).unwrap(),
                );
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                hub.configure(S3Config {
                    endpoint: format!("http://{}", listener.local_addr().unwrap()),
                    bucket: "muzon".into(),
                    path_style: true,
                    ..Default::default()
                })
                .unwrap();
                let (seen_tx, seen_rx) = mpsc::channel();
                let (release_tx, release_rx) = mpsc::channel();
                let server = std::thread::spawn(move || {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut buffer = [0; 8192];
                    assert!(stream.read(&mut buffer).unwrap() > 0);
                    seen_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
                });
                let copy = hub.clone();
                let syncing =
                    tokio::spawn(async move { copy.sync(CancellationToken::new()).await });
                tokio::task::spawn_blocking(move || {
                    seen_rx.recv_timeout(Duration::from_secs(5)).unwrap()
                })
                .await
                .unwrap();
                hub.shutdown();
                assert!(hub.worker.lock().unwrap().is_none());
                let result = tokio::time::timeout(Duration::from_secs(2), syncing)
                    .await
                    .unwrap()
                    .unwrap();
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("Сервисы остановлены"));
                assert!(hub.error().is_none());
                release_tx.send(()).unwrap();
                server.join().unwrap();
                core.shutdown();
            });
    }

    #[test]
    fn final_drain_transfers_more_than_one_batch_durably() {
        use muzon_core::{data::db::NewTrack, domain::listen::Listen};
        let dir = crate::testing::scratch_dir("services-final-drain");
        let db = Db::open(&dir.join("muzon.sqlite3")).unwrap();
        db.ensure_listen_outbox().unwrap();
        db.upsert_track(&NewTrack {
            path: "/a.wav".into(),
            title: "A".into(),
            artist: Some("Artist".into()),
            album: None,
            duration_secs: Some(100.),
            track_no: None,
            genre: None,
            year: None,
            mtime: 0,
        })
        .unwrap();
        let track = db.list_tracks().unwrap().remove(0);
        // Connected in a disposable profile, but no credential lookup or HTTP.
        std::fs::write(
            dir.join("scrobble.json"),
            br#"{"listenbrainzUser":"test-user"}"#,
        )
        .unwrap();
        let scrobbler = Scrobbler::with_vault(&dir, Arc::new(SpyVault::default()));
        for i in 0..251 {
            let listen = Listen::from_track(&track, i).unwrap();
            db.record_play_and_queue(track.id, Some(&listen)).unwrap();
        }
        assert_eq!(db.pending_listens(1000).unwrap().len(), 251);
        drain_all_outbox(&db, &scrobbler).unwrap();
        assert!(db.pending_listens(1000).unwrap().is_empty());
        assert_eq!(scrobbler.status().listenbrainz.pending, 251);
        let reopened = Scrobbler::with_vault(&dir, Arc::new(SpyVault::default()));
        assert_eq!(reopened.status().listenbrainz.pending, 251);
        drain_all_outbox(&db, &scrobbler).unwrap();
    }

    #[test]
    fn failed_handoff_keeps_the_durable_outbox_for_recovery() {
        use muzon_core::{data::db::NewTrack, domain::listen::Listen};
        let dir = crate::testing::scratch_dir("services-handoff-corrupt");
        let db = Db::open(&dir.join("muzon.sqlite3")).unwrap();
        db.ensure_listen_outbox().unwrap();
        db.upsert_track(&NewTrack {
            path: "/a.wav".into(),
            title: "A".into(),
            artist: Some("Artist".into()),
            album: None,
            duration_secs: Some(100.),
            track_no: None,
            genre: None,
            year: None,
            mtime: 0,
        })
        .unwrap();
        let track = db.list_tracks().unwrap().remove(0);
        let listen = Listen::from_track(&track, 123).unwrap();
        db.record_play_and_queue(track.id, Some(&listen)).unwrap();
        let broken = dir.join("scrobble-queue.json");
        std::fs::write(&broken, b"broken").unwrap();
        let scrobbler = Scrobbler::with_vault(&dir, Arc::new(SpyVault::default()));
        assert!(drain_outbox(&db, &scrobbler).is_err());
        assert_eq!(db.pending_listens(100).unwrap().len(), 1);
        assert_eq!(std::fs::read(&broken).unwrap(), b"broken");
        std::fs::write(&broken, b"{}").unwrap();
        scrobbler.recover().unwrap();
        assert!(drain_outbox(&db, &scrobbler).unwrap());
        // No connected account: do not replay history into a future account.
        assert!(db.pending_listens(100).unwrap().is_empty());
    }
}
