//! Shell integrations use only a command/snapshot client, never GStreamer.
pub mod control;
pub mod instance;
mod mpris;
use muzon_core::controller::ControllerClient;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender},
        Arc,
    },
    thread::JoinHandle,
};
#[derive(Debug, Clone, Copy)]
pub enum WindowRequest {
    Raise,
    Quit,
}
/// Owns integration workers; stopping these never shuts down the audio owner.
pub struct Desktop {
    instance: Option<instance::InstanceServer>,
    control: Option<control::ControlServer>,
    mpris: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    requests: Receiver<WindowRequest>,
}
impl Desktop {
    pub fn start(
        client: ControllerClient,
        profile: &Path,
        instance: instance::InstanceServer,
        tx: SyncSender<WindowRequest>,
        requests: Receiver<WindowRequest>,
    ) -> Self {
        let socket = std::env::var_os("MUZON_CONTROL_SOCKET")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| profile.join("control.sock"));
        let control = match control::ControlServer::start(client.clone(), socket) {
            Ok(server) => Some(server),
            Err(e) => {
                eprintln!("[muzon control] {e}");
                None
            }
        };
        let stop = Arc::new(AtomicBool::new(false));

        let name = std::env::var("MUZON_MPRIS_NAME")
            .unwrap_or_else(|_| format!("muzon.native.instance{}", std::process::id()));
        let mpris = if name.is_empty() {
            None
        } else {
            let stop = stop.clone();
            let covers = profile.join("covers");
            std::thread::Builder::new()
                .name("muzon-mpris".into())
                .spawn(move || {
                    let runtime = match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(runtime) => runtime,
                        Err(e) => {
                            eprintln!("[muzon mpris] {e}");
                            return;
                        }
                    };
                    if let Err(e) = runtime.block_on(mpris::serve(client, covers, name, tx, stop)) {
                        eprintln!("[muzon mpris] {e}");
                    }
                })
                .map_err(|e| eprintln!("[muzon mpris] {e}"))
                .ok()
        };
        Self {
            instance: Some(instance),
            control,
            mpris,
            stop,
            requests,
        }
    }
    pub fn request(&self) -> Option<WindowRequest> {
        self.requests.try_recv().ok()
    }
    pub fn request_shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(control) = &self.control {
            control.request_shutdown();
        }
    }
    pub fn shutdown(&mut self) {
        self.request_shutdown();
        self.instance.take();
        if let Some(mut server) = self.control.take() {
            server.shutdown();
        }
        if let Some(thread) = self.mpris.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for Desktop {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn window_request(
    tx: &SyncSender<WindowRequest>,
    request: WindowRequest,
) -> mpris_server::zbus::fdo::Result<()> {
    tx.try_send(request)
        .map_err(|e| mpris_server::zbus::fdo::Error::Failed(e.to_string()))
}

#[cfg(test)]
mod tests;

/// Poll cancellation while preserving the same in-flight future across ticks.
/// Used for external D-Bus calls, including connection setup.
pub(super) async fn bounded<T>(
    stop: &AtomicBool,
    limit: std::time::Duration,
    future: impl std::future::Future<Output = T>,
) -> anyhow::Result<T> {
    tokio::time::timeout(limit, async {
        let mut future = std::pin::pin!(future);
        loop {
            anyhow::ensure!(!stop.load(Ordering::Acquire), "desktop integration stopped");
            if let Ok(value) =
                tokio::time::timeout(std::time::Duration::from_millis(20), future.as_mut()).await
            {
                return Ok(value);
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("desktop integration timed out"))?
}

// File reads cannot be cancelled in the kernel. Keep them off runtime blocking
// pools (whose teardown joins them), and cap detached readers process-wide.
const MAX_READERS: usize = 4;
static READERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub(super) fn read_job<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<std::sync::mpsc::Receiver<T>, String> {
    let mut readers = READERS.load(Ordering::Acquire);
    loop {
        if readers >= MAX_READERS {
            return Err("desktop file readers busy".into());
        }
        match READERS.compare_exchange_weak(
            readers,
            readers + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => break,
            Err(current) => readers = current,
        }
    }
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let result = std::thread::Builder::new()
        .name("muzon-desktop-read".into())
        .spawn(move || {
            struct Release;
            impl Drop for Release {
                fn drop(&mut self) {
                    READERS.fetch_sub(1, Ordering::AcqRel);
                }
            }
            let _release = Release;
            let _ = tx.send(work());
        });
    if let Err(error) = result {
        READERS.fetch_sub(1, Ordering::AcqRel);
        return Err(error.to_string());
    }
    Ok(rx)
}
