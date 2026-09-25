//! Idle detection straight from the compositor (ext-idle-notify-v1, what hypridle uses).
//! Omarchy's idle handling lives inside its shell, so there is nothing to hook into.
use crate::Event;
use std::{error::Error, os::unix::net::UnixStream, path::PathBuf, sync::mpsc::Sender, thread, time::Duration};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{wl_registry, wl_seat::WlSeat},
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::ExtIdleNotifierV1,
};

struct State {
    tx: Sender<Event>,
    seat: Option<WlSeat>,
    notifier: Option<(ExtIdleNotifierV1, u32)>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        reg: &wl_registry::WlRegistry,
        ev: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = ev {
            match interface.as_str() {
                "wl_seat" if s.seat.is_none() => s.seat = Some(reg.bind(name, 1, qh, ())),
                "ext_idle_notifier_v1" => s.notifier = Some((reg.bind(name, version.min(2), qh, ()), version.min(2))),
                _ => {}
            }
        }
    }
}

impl Dispatch<ExtIdleNotificationV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &ExtIdleNotificationV1,
        ev: ext_idle_notification_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let _ = s.tx.send(Event::Idle(matches!(ev, ext_idle_notification_v1::Event::Idled)));
    }
}

delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ExtIdleNotifierV1);

/// Reconnects every 5 s: the service can start before the compositor (at login, via udev),
/// and the compositor can restart under it. A compositor without the protocol is final.
pub fn spawn(tx: Sender<Event>, timeout: Duration) {
    thread::spawn(move || {
        loop {
            match watch(&tx, timeout) {
                Ok(()) => {
                    eprintln!("apex-oled: compositor has no ext-idle-notify-v1, so the screen won't blank");
                    return;
                }
                Err(e) => eprintln!("apex-oled: idle detection: {e}"),
            }
            if tx.send(Event::Idle(false)).is_err() {
                return;
            }
            thread::sleep(Duration::from_secs(5));
        }
    });
}

/// Only returns Ok if the compositor lacks the protocol; otherwise runs until the connection fails.
fn watch(tx: &Sender<Event>, timeout: Duration) -> Result<(), Box<dyn Error>> {
    let conn = Connection::from_socket(UnixStream::connect(socket()?)?)?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut s = State { tx: tx.clone(), seat: None, notifier: None };
    queue.roundtrip(&mut s)?;
    let (Some(seat), Some((notifier, version))) = (s.seat.clone(), s.notifier.clone()) else {
        return Ok(());
    };
    let ms = timeout.as_millis() as u32;
    // v2's input idle ignores inhibitors: a video keeping the monitor awake is no reason to keep the keyboard lit.
    if version >= 2 {
        notifier.get_input_idle_notification(ms, &seat, &qh, ());
    } else {
        notifier.get_idle_notification(ms, &seat, &qh, ());
    }
    loop {
        queue.blocking_dispatch(&mut s)?;
    }
}

/// $WAYLAND_DISPLAY if the service inherited it, else the first wayland-N socket in the runtime dir.
fn socket() -> Result<PathBuf, Box<dyn Error>> {
    let dir = PathBuf::from(std::env::var("XDG_RUNTIME_DIR")?);
    if let Ok(name) = std::env::var("WAYLAND_DISPLAY") {
        return Ok(dir.join(name));
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("wayland-") && !n.ends_with(".lock"))
        })
        .collect();
    found.sort();
    found.into_iter().next().ok_or_else(|| "no Wayland socket yet".into())
}
