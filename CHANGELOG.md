# Changelog

## v0.1.0 — 2026-09-25

The first release: the SteelSeries Apex 7's OLED, finally doing something on Linux.

![The four pages cycling on the keyboard's OLED](https://raw.githubusercontent.com/kevincardwell/apex-oled/v0.1.0/docs/demo.gif)

### Four pages

- 🕐 **Clock** — big digits, day and date, the current weather in the corner, and a line that fills across each minute.
- 🌦️ **Weather** — conditions, today's high and low, and the next 24 hours as a temperature line over chance-of-rain bars, from Open-Meteo (no API key).
- 📊 **System** — a bar per CPU thread, CPU and GPU load and temperature, RAM used.
- 🎵 **Spectrum** — 64 mirrored bars from cava at 30 fps, which keep going while the music plays even if you step away.

Step through them with `systemctl --user kill --kill-whom=main -s USR1 apex-oled` on a keybinding.

### Built to be left running

- **Blanks when you're idle**, straight from the compositor (`ext-idle-notify-v1`), and a playing video doesn't hold it on.
- **Burn-in care**: a one-pixel orbit every minute, mostly-black layouts, blank on idle and on exit.
- **Plug and play**: udev starts the service when the keyboard appears and stops it when it goes.
- **Cheap**: under 10 ms of CPU per 30 s on the clock, about 3 MB of RAM, and roughly 1 % of one core with the spectrum running (cava included), measured on a Ryzen 9 5950X.

### Install

Prebuilt (x86_64, glibc 2.34 or newer):

```bash
tar xzf apex-oled-v0.1.0-x86_64-linux.tar.gz
cd apex-oled-v0.1.0-x86_64-linux
./install.sh
```

From source (Rust 1.88+): clone the repository and run `./install.sh`. Either way, `sudo` is used only to install the udev rule, which opens the keyboard's screen interface to your login session and nothing else.

### Compatibility

- Tested on the **Apex 7** (`1038:1612`) under Omarchy / Hyprland.
- The Apex 7 TKL, Apex Pro, Apex Pro TKL and Apex 5 use the same screen protocol and are enabled, but untested. `tools/oled-test.py` checks yours; a report either way is welcome.
- Without Omarchy, set `APEX_OLED_LOCATION` for the weather; see the README's Configuration section.
