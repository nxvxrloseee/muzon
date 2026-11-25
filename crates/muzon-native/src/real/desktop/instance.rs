//! Profile-local activation endpoint. The profile lock remains the authority.
use super::WindowRequest;
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub fn claim(profile: &Path) -> anyhow::Result<Option<File>> {
    fs::create_dir_all(profile)?;
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(profile.join("native.lock"))?;
    match lock.try_lock() {
        Ok(()) => Ok(Some(lock)),
        Err(std::fs::TryLockError::WouldBlock) => {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if activate(&profile.join("instance.sock")).is_ok() {
                    return Ok(None);
                }
                if Instant::now() >= deadline {
                    anyhow::bail!("profile is open, but its window did not acknowledge activation");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        Err(e) => Err(anyhow::anyhow!("cannot lock profile: {e}")),
    }
}
fn activate(path: &Path) -> std::io::Result<()> {
    let mut socket = UnixStream::connect(path)?;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    socket.set_write_timeout(Some(Duration::from_millis(200)))?;
    socket.write_all(b"raise\n")?;
    let mut reply = [0; 3];
    socket.read_exact(&mut reply)?;
    if &reply == b"ok\n" {
        Ok(())
    } else {
        Err(std::io::Error::other("activation busy"))
    }
}

pub struct InstanceServer {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    path: PathBuf,
    identity: (u64, u64),
}
impl InstanceServer {
    /// Caller must retain the exclusive native.lock for this profile.
    pub fn start(profile: &Path, requests: SyncSender<WindowRequest>) -> anyhow::Result<Self> {
        let path = profile.join("instance.sock");
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            anyhow::ensure!(
                metadata.file_type().is_socket(),
                "activation path is not a socket"
            );
            anyhow::ensure!(
                UnixStream::connect(&path).is_err(),
                "activation endpoint already active"
            );
            fs::remove_file(&path)?;
        }
        let listener = UnixListener::bind(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let metadata = fs::symlink_metadata(&path)?;
        let identity = (metadata.dev(), metadata.ino());
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::Builder::new()
            .name("muzon-instance".into())
            .spawn(move || {
                while !stopping.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut socket, _)) => {
                            let _ = socket.set_read_timeout(Some(Duration::from_millis(200)));
                            let _ = socket.set_write_timeout(Some(Duration::from_millis(200)));
                            let mut command = [0; 6];
                            if socket.read_exact(&mut command).is_ok() && &command == b"raise\n" {
                                let reply: &[u8] =
                                    if requests.try_send(WindowRequest::Raise).is_ok() {
                                        b"ok\n"
                                    } else {
                                        b"no\n"
                                    };
                                let _ = socket.write_all(reply);
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(20))
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
            path,
            identity,
        })
    }
}
impl Drop for InstanceServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if fs::symlink_metadata(&self.path).is_ok_and(|m| (m.dev(), m.ino()) == self.identity) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_launch_activates_without_acquiring_profile() {
        let dir = muzon_core::testing::scratch_dir("instance-activation");
        let lock = claim(&dir).unwrap().unwrap();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let server = InstanceServer::start(&dir, tx).unwrap();
        assert!(claim(&dir).unwrap().is_none());
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            WindowRequest::Raise
        ));
        drop(server);
        assert!(!dir.join("instance.sock").exists());
        drop(lock);
        assert!(claim(&dir).unwrap().is_some());
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn full_queue_does_not_acknowledge_activation() {
        let dir = muzon_core::testing::scratch_dir("instance-busy");
        let (tx, _rx) = std::sync::mpsc::sync_channel(1);
        tx.send(WindowRequest::Quit).unwrap();
        let server = InstanceServer::start(&dir, tx).unwrap();
        assert!(activate(&dir.join("instance.sock")).is_err());
        drop(server);
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn shutdown_preserves_replaced_endpoint() {
        let dir = muzon_core::testing::scratch_dir("instance-replaced");
        let (tx, _rx) = std::sync::mpsc::sync_channel(1);
        let server = InstanceServer::start(&dir, tx).unwrap();
        fs::remove_file(dir.join("instance.sock")).unwrap();
        fs::write(dir.join("instance.sock"), "replacement").unwrap();
        drop(server);
        assert_eq!(
            fs::read_to_string(dir.join("instance.sock")).unwrap(),
            "replacement"
        );
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn existing_regular_file_is_preserved() {
        let dir = muzon_core::testing::scratch_dir("instance-file");
        fs::write(dir.join("instance.sock"), "keep").unwrap();
        let (tx, _) = std::sync::mpsc::sync_channel(1);
        assert!(InstanceServer::start(&dir, tx).is_err());
        assert_eq!(
            fs::read_to_string(dir.join("instance.sock")).unwrap(),
            "keep"
        );
        let _ = fs::remove_dir_all(dir);
    }
}
