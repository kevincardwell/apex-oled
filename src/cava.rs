//! Audio spectrum from cava, which only runs while the spectrum page is on screen.
use crate::Event;
use std::{
    fs,
    io::{self, Read},
    process::{Child, Command, Stdio},
    sync::mpsc::Sender,
    thread,
};

pub const BARS: usize = 64;

// Same capture settings as the Spectra bar plugin. Stereo puts the bass in the centre;
// sleep_timer stops all output (and our wakeups) after 3 s of silence.
const CONFIG: &str = "\
[general]
framerate = 30
bars = 64
autosens = 1
sleep_timer = 3
[input]
method = pulse
source = auto
[output]
method = raw
raw_target = /dev/stdout
data_format = binary
bit_format = 8bit
channels = stereo
[smoothing]
noise_reduction = 77
";

pub struct Cava(Option<Child>);

impl Cava {
    pub fn new() -> Self {
        Cava(None)
    }

    /// Starts or stops cava to match `want`. A cava that died on its own (audio server
    /// restart) is reaped here and restarted on the next call.
    // ponytail: a cava that can never start is retried once a second while the page is up; add backoff if that ever matters
    pub fn run(&mut self, want: bool, tx: &Sender<Event>) {
        if let Some(c) = &mut self.0
            && !matches!(c.try_wait(), Ok(None))
        {
            self.0 = None;
        }
        if want && self.0.is_none() {
            match spawn(tx.clone()) {
                Ok(c) => self.0 = Some(c),
                Err(e) => eprintln!("apex-oled: cava: {e}"),
            }
        } else if !want {
            self.stop();
        }
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.0.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn spawn(tx: Sender<Event>) -> io::Result<Child> {
    let dir = std::env::var("XDG_RUNTIME_DIR").map_err(|_| io::Error::other("XDG_RUNTIME_DIR unset"))?;
    let config = format!("{dir}/apex-oled-cava.conf");
    fs::write(&config, CONFIG)?;
    let mut child = Command::new("/usr/bin/cava")
        .args(["-p", &config])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut out = child.stdout.take().expect("stdout is piped");
    thread::spawn(move || {
        let mut bars = [0; BARS];
        while out.read_exact(&mut bars).is_ok() && tx.send(Event::Spectrum(bars)).is_ok() {}
    });
    Ok(child)
}
