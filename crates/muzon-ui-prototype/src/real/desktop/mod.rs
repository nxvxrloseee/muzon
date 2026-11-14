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
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
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
