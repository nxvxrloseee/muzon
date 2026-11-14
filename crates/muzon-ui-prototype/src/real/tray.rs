//! KSNI tray lives on the UI thread; only its health probe crosses threads.
use mpris_server::zbus;
use muzon_core::controller::Snapshot;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconEvent,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Show,
    Toggle,
    Previous,
    Next,
    Quit,
}

pub struct Tray {
    icon: TrayIcon,
    toggle: MenuItem,
    label: String,
    playing: Option<bool>,
    health: Arc<Mutex<Option<Instant>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Tray {
    pub fn new() -> anyhow::Result<Option<Self>> {
        if std::env::var("MUZON_TRAY").as_deref() == Ok("0") {
            return Ok(None);
        }
        let id = format!("muzon_native_{}", std::process::id());
        let menu = Menu::new();
        let item =
            |suffix: &str, label| MenuItem::with_id(format!("{id}:{suffix}"), label, true, None);
        let show = item("show", "Открыть Muzon");
        let previous = item("previous", "Предыдущий трек");
        let toggle = item("toggle", "Воспроизвести");
        let next = item("next", "Следующий трек");
        let quit = item("quit", "Выйти");
        menu.append_items(&[&show, &previous, &toggle, &next, &quit])?;
        let image = tiny_skia::Pixmap::decode_png(include_bytes!("../../assets/icon.png"))?;
        let rgba: Vec<u8> = image
            .pixels()
            .iter()
            .flat_map(|p| {
                let p = p.demultiply();
                [p.red(), p.green(), p.blue(), p.alpha()]
            })
            .collect();
        let icon = TrayIcon::builder()
            .with_id(id.clone())
            .with_icon(Icon::from_rgba(rgba, image.width(), image.height())?)
            .with_tooltip("Muzon")
            .with_menu(Box::new(menu))
            .build()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let health = Arc::new(Mutex::new(None));
        let report = health.clone();
        let worker = std::thread::Builder::new()
            .name("muzon-tray-health".into())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                runtime.block_on(async move {
                    let connection = zbus::Connection::session().await.ok();
                    while !stopping.load(Ordering::Acquire) {
                        let ready = if let Some(connection) = &connection {
                            tokio::time::timeout(
                                Duration::from_millis(800),
                                registered(connection, &id),
                            )
                            .await
                            .unwrap_or(false)
                        } else {
                            false
                        };
                        if let Ok(mut report) = report.lock() {
                            *report = ready.then(Instant::now);
                        }
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                });
            })?;
        Ok(Some(Self {
            icon,
            toggle,
            label: String::new(),
            playing: None,
            health,
            stop,
            worker: Some(worker),
        }))
    }
    pub fn ready(&self) -> bool {
        self.health
            .lock()
            .ok()
            .and_then(|g| *g)
            .is_some_and(|seen| seen.elapsed() < Duration::from_secs(3))
    }
    pub fn action(&self) -> Option<Action> {
        for event in TrayIconEvent::receiver().try_iter().take(32) {
            if let TrayIconEvent::Click {
                id,
                button,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if id == *self.icon.id() {
                    match button {
                        MouseButton::Left => return Some(Action::Show),
                        MouseButton::Middle => return Some(Action::Toggle),
                        _ => {}
                    }
                }
            }
        }
        for event in MenuEvent::receiver().try_iter().take(32) {
            if let Some(action) = menu_action(self.icon.id().as_ref(), event.id.as_ref()) {
                return Some(action);
            }
        }
        None
    }
    pub fn refresh(&mut self, snapshot: &Snapshot) {
        let label = snapshot
            .current_track()
            .map(|t| t.title.clone())
            .unwrap_or_else(|| "Muzon".into());
        if label != self.label {
            let _ = self.icon.set_tooltip(Some(&label));
            self.label = label;
        }
        if self.playing != Some(snapshot.is_playing) {
            self.toggle.set_text(if snapshot.is_playing {
                "Пауза"
            } else {
                "Воспроизвести"
            });
            self.playing = Some(snapshot.is_playing);
        }
    }
}
fn menu_action(id: &str, menu: &str) -> Option<Action> {
    match menu.strip_prefix(id)?.strip_prefix(':')? {
        "show" => Some(Action::Show),
        "toggle" => Some(Action::Toggle),
        "previous" => Some(Action::Previous),
        "next" => Some(Action::Next),
        "quit" => Some(Action::Quit),
        _ => None,
    }
}
async fn registered(connection: &zbus::Connection, id: &str) -> bool {
    async fn probe(connection: &zbus::Connection, id: &str) -> zbus::Result<bool> {
        let watcher = zbus::proxy::Builder::<zbus::Proxy<'_>>::new(connection)
            .destination("org.kde.StatusNotifierWatcher")?
            .path("/StatusNotifierWatcher")?
            .interface("org.kde.StatusNotifierWatcher")?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?;
        if !watcher
            .get_property::<bool>("IsStatusNotifierHostRegistered")
            .await?
        {
            return Ok(false);
        }
        let items: Vec<String> = watcher
            .get_property("RegisteredStatusNotifierItems")
            .await?;
        for item in items.iter().take(64) {
            let (service, path) = match item.find('/') {
                Some(i) => (&item[..i], &item[i..]),
                None => (item.as_str(), "/StatusNotifierItem"),
            };
            let Ok(proxy) =
                zbus::Proxy::new(connection, service, path, "org.kde.StatusNotifierItem").await
            else {
                continue;
            };
            if proxy
                .get_property::<String>("Id")
                .await
                .is_ok_and(|value| value == id)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    probe(connection, id).await.unwrap_or(false)
}
impl Drop for Tray {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menu_actions_only_accept_own_exact_ids() {
        assert_eq!(menu_action("muzon", "muzon:quit"), Some(Action::Quit));
        assert_eq!(menu_action("muzon", "muzon-other:quit"), None);
        assert_eq!(menu_action("muzon", "muzon:quit:extra"), None);
    }
}
