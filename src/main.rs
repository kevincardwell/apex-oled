//! Drives the SteelSeries Apex 7 OLED. One thread renders the visible page, wakes on
//! the second boundary or an event, and only writes frames that changed. Helper threads
//! (signals, weather, idle) just post events.
//!
//!   SIGUSR1         next page (clock, weather, system, spectrum); send it with
//!                   `systemctl --user kill --kill-whom=main -s USR1 apex-oled`, since a
//!                   plain `kill` also hits cava, which reloads its config on USR1
//!   SIGTERM/SIGINT  blank the screen and exit
//!
//!   apex-oled --preview clock|weather|system|spectrum [frames] > out.pbm
//!       render without the keyboard; several frames come out as one multi-image PBM
//!       (one a second, or 30 a second for the spectrum)
//!
//! Environment: APEX_OLED_LOCATION="lat,lon", APEX_OLED_UNITS=fahrenheit,
//! APEX_OLED_IDLE=<seconds before blanking, 0 = never>.

mod cava;
mod idle;
mod oled;
mod pages;
mod stats;
mod weather;

use oled::{Frame, Oled};
use stats::Stats;
use std::{
    io::{self, Write},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use weather::Weather;

const NODE: &str = "/dev/apex-oled"; // udev/71-apex-oled.rules
const PAGES: usize = 4;
const SPECTRUM: usize = 3;

pub enum Event {
    Signal(i32),
    Weather(Weather),
    Idle(bool),
    Spectrum([u8; cava::BARS]),
}

/// What the pages draw from, besides the clock.
struct Data {
    stats: Option<Stats>, // created on first view: NVML costs ~24 MB RSS that nvmlShutdown never returns
    forecast: Option<Weather>,
    bars: [u8; cava::BARS],
    bars_at: Option<Instant>,  // last spectrum frame
    sound_at: Option<Instant>, // last frame that wasn't silence
}

impl Data {
    fn new() -> Self {
        Data { stats: None, forecast: None, bars: [0; cava::BARS], bars_at: None, sound_at: None }
    }

    fn heard_within(&self, at: Option<Instant>, secs: u64) -> bool {
        at.is_some_and(|t| t.elapsed() < Duration::from_secs(secs))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("--preview") => {
            preview(args.get(1).map_or("clock", String::as_str), args.get(2).and_then(|n| n.parse().ok()).unwrap_or(1))
        }
        _ => run(),
    };
    if let Err(e) = result {
        eprintln!("apex-oled: {e}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    signals(tx.clone()); // first, so every later thread inherits the blocked mask
    let oled = Oled::open(NODE)?;
    weather::spawn(tx.clone());
    if let Some(timeout) = idle_timeout() {
        idle::spawn(tx.clone(), timeout);
    }

    let mut cava = cava::Cava::new();
    let mut data = Data::new();
    let (mut page, mut idle, mut last) = (0, false, None);
    loop {
        // Idle: no ticking at all, just wait for the next event. The spectrum page keeps
        // its 1 s tick so it can go dark a few seconds after the music stops.
        let event = if idle && page != SPECTRUM { rx.recv().ok() } else { rx.recv_timeout(until_next_second()).ok() };
        match event {
            Some(Event::Signal(libc::SIGUSR1)) => page = (page + 1) % PAGES,
            Some(Event::Signal(_)) => {
                cava.stop();
                return oled.send(&Frame::new()); // don't leave a frame burning in
            }
            Some(Event::Weather(w)) => data.forecast = Some(w),
            Some(Event::Idle(i)) => idle = i,
            Some(Event::Spectrum(bars)) => {
                data.bars = bars;
                data.bars_at = Some(Instant::now());
                if bars.iter().any(|&b| b > 0) {
                    data.sound_at = Some(Instant::now());
                }
            }
            None => {}
        }
        cava.run(page == SPECTRUM, &tx);
        // A playing visualiser is moving, so it may stay lit while you're away from the keyboard.
        let playing = page == SPECTRUM && data.heard_within(data.sound_at, 3);
        let frame = if idle && !playing { Frame::new() } else { render(page, &mut data, true) };
        if last.as_ref() != Some(&frame) {
            oled.send(&frame)?;
            last = Some(frame);
        }
    }
}

fn render(page: usize, data: &mut Data, shift: bool) -> Frame {
    let mut frame = match page {
        0 => pages::clock(&local_now(), data.forecast.as_ref()),
        1 => pages::weather(data.forecast.as_ref()),
        2 => {
            let stats = data.stats.get_or_insert_with(Stats::new);
            stats.sample();
            pages::system(stats)
        }
        // cava sends nothing while asleep, so stale bars fall back to the flat line
        _ => pages::spectrum(if data.heard_within(data.bars_at, 1) { &data.bars } else { &[0; cava::BARS] }),
    };
    if shift {
        // A 2x2 orbit, one step a minute, so no pixel stays lit in exactly one place for long.
        let minute = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() / 60;
        let (right, down) = [(false, false), (true, false), (true, true), (false, true)][(minute % 4) as usize];
        frame.shift(right, down);
    }
    frame
}

fn preview(page: &str, frames: usize) -> io::Result<()> {
    let n = match page {
        "weather" => 1,
        "system" => 2,
        "spectrum" => 3,
        _ => 0,
    };
    let mut data = Data { stats: Some(Stats::new()), forecast: weather::fetch(), ..Data::new() };
    data.stats.as_mut().map(Stats::sample);
    let (tx, rx) = mpsc::channel();
    let mut cava = cava::Cava::new();
    cava.run(n == SPECTRUM, &tx);
    let step = Duration::from_millis(if n == SPECTRUM { 33 } else { 1000 });
    // The first frame waits for a CPU-load delta, or for cava's autosens to settle.
    let mut due = Instant::now() + Duration::from_secs(if n == SPECTRUM { 3 } else { 1 });
    let mut out = io::stdout().lock();
    for _ in 0..frames {
        while let Ok(Event::Spectrum(bars)) = rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
            (data.bars, data.bars_at) = (bars, Some(Instant::now()));
        }
        out.write_all(&render(n, &mut data, false).pbm())?;
        due = Instant::now() + step;
    }
    cava.stop();
    Ok(())
}

/// APEX_OLED_IDLE seconds (0 = never blank), else the Omarchy screensaver delay
/// (shell.json idle.screensaver), else 5 min.
fn idle_timeout() -> Option<Duration> {
    if let Ok(v) = std::env::var("APEX_OLED_IDLE") {
        return v.trim().parse().ok().filter(|&s| s > 0).map(Duration::from_secs);
    }
    let secs = std::env::var("HOME")
        .ok()
        .and_then(|h| std::fs::read_to_string(format!("{h}/.config/omarchy/shell.json")).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["idle"]["screensaver"].as_u64())
        .filter(|&s| s > 0)
        .unwrap_or(300);
    Some(Duration::from_secs(secs))
}

fn signals(tx: mpsc::Sender<Event>) {
    let set = unsafe {
        let mut set = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for s in [libc::SIGUSR1, libc::SIGTERM, libc::SIGINT] {
            libc::sigaddset(&mut set, s);
        }
        // Blocked before any other thread exists, so every thread inherits it and only sigwait sees them.
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        set
    };
    std::thread::spawn(move || {
        loop {
            let mut sig = 0;
            if unsafe { libc::sigwait(&set, &mut sig) } == 0 && tx.send(Event::Signal(sig)).is_err() {
                break;
            }
        }
    });
}

fn until_next_second() -> Duration {
    let into = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos();
    Duration::from_nanos(1_000_000_000 - into as u64) + Duration::from_millis(2)
}

fn local_now() -> libc::tm {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm
    }
}
