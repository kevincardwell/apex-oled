//! Pages draw inside x 0..=126, y 0..=38: the last column and row stay free for the burn-in shift.
use crate::{
    oled::{Frame, W},
    stats::Stats,
    weather::{self, Weather},
};
use embedded_graphics::{
    pixelcolor::BinaryColor::{self, On},
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
};
use u8g2_fonts::{
    FontRenderer, fonts,
    types::{FontColor, HorizontalAlignment as Align, VerticalPosition},
};

const RIGHT: i32 = 126;
const BOTTOM: i32 = 38;

const BIG: FontRenderer = FontRenderer::new::<fonts::u8g2_font_logisoso32_tn>();
const TEMP: FontRenderer = FontRenderer::new::<fonts::u8g2_font_logisoso22_tn>();
const MID: FontRenderer = FontRenderer::new::<fonts::u8g2_font_logisoso16_tn>();
const SMALL: FontRenderer = FontRenderer::new::<fonts::u8g2_font_5x7_tf>().with_ignore_unknown_chars(true);
const ICON: FontRenderer = FontRenderer::new::<fonts::u8g2_font_open_iconic_weather_2x_t>();
const ICON_BIG: FontRenderer = FontRenderer::new::<fonts::u8g2_font_open_iconic_weather_4x_t>();

const DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

fn text(f: &mut Frame, font: &FontRenderer, s: std::fmt::Arguments, x: i32, baseline: i32, align: Align) {
    let _ = font.render_aligned(
        s,
        Point::new(x, baseline),
        VerticalPosition::Baseline,
        align,
        FontColor::Transparent(On),
        f,
    );
}

fn icon(f: &mut Frame, font: &FontRenderer, glyph: char, x: i32, top: i32) {
    let _ = font.render(glyph, Point::new(x, top), VerticalPosition::Top, FontColor::Transparent(On), f);
}

fn rect(f: &mut Frame, x: i32, y: i32, w: u32, h: u32, style: PrimitiveStyle<BinaryColor>) {
    let _ = Rectangle::new(Point::new(x, y), Size::new(w, h)).into_styled(style).draw(f);
}

/// Big HH:MM with a line under it that fills across the minute; right column: day and
/// date, weather icon, temperature (day, date and month until the first forecast arrives).
pub fn clock(tm: &libc::tm, w: Option<&Weather>) -> Frame {
    let mut f = Frame::new();
    text(&mut f, &BIG, format_args!("{:02}:{:02}", tm.tm_hour, tm.tm_min), 0, 34, Align::Left);
    match w {
        Some(w) => {
            text(
                &mut f,
                &SMALL,
                format_args!("{} {}", DAYS[tm.tm_wday as usize % 7], tm.tm_mday),
                RIGHT,
                6,
                Align::Right,
            );
            icon(&mut f, &ICON, weather::describe(w.code, w.day).0, RIGHT - 16, 11);
            text(&mut f, &SMALL, format_args!("{:.0}°", w.temp), RIGHT, 36, Align::Right);
        }
        None => {
            text(&mut f, &SMALL, format_args!("{}", DAYS[tm.tm_wday as usize % 7]), RIGHT, 7, Align::Right);
            text(&mut f, &MID, format_args!("{}", tm.tm_mday), RIGHT, 27, Align::Right);
            text(&mut f, &SMALL, format_args!("{}", MONTHS[tm.tm_mon as usize % 12]), RIGHT, 36, Align::Right);
        }
    }
    rect(&mut f, 0, BOTTOM, (tm.tm_sec as u32 + 1) * 90 / 60, 1, PrimitiveStyle::with_fill(On)); // under the digits only
    f
}

/// Icon, temperature, condition and today's range on the left; the next 24 h on the
/// right: temperature line above, chance-of-rain bars below.
pub fn weather(w: Option<&Weather>) -> Frame {
    let mut f = Frame::new();
    let Some(w) = w else {
        text(&mut f, &SMALL, format_args!("waiting for weather"), 63, 22, Align::Center);
        return f;
    };
    let (glyph, label) = weather::describe(w.code, w.day);
    icon(&mut f, &ICON_BIG, glyph, 0, 3);
    // The font's own ° sits above the digits and off the top of the screen, so draw a ring.
    let digits = TEMP.render(
        format_args!("{:.0}", w.temp),
        Point::new(35, 23),
        VerticalPosition::Baseline,
        FontColor::Transparent(On),
        &mut f,
    );
    if let Ok(r) = digits
        && let Some(bb) = r.bounding_box
    {
        let ring = Point::new(bb.top_left.x + bb.size.width as i32 + 2, bb.top_left.y);
        let _ = Circle::new(ring, 6).into_styled(PrimitiveStyle::with_stroke(On, 1)).draw(&mut f);
    }
    text(&mut f, &SMALL, format_args!("{label}"), 35, 31, Align::Left);
    text(&mut f, &SMALL, format_args!("H{:.0} L{:.0}", w.high, w.low), 35, BOTTOM, Align::Left);

    let (x0, top, h) = (78, 1, 18);
    let (lo, hi) = w.hourly.iter().fold((f32::MAX, f32::MIN), |(a, b), &t| (a.min(t), b.max(t)));
    let y = |t: f32| top + h - ((t - lo) / (hi - lo).max(1.0) * h as f32).round() as i32;
    for (i, pair) in w.hourly.windows(2).enumerate() {
        let (a, b) = (Point::new(x0 + 2 * i as i32, y(pair[0])), Point::new(x0 + 2 * i as i32 + 2, y(pair[1])));
        let _ = Line::new(a, b).into_styled(PrimitiveStyle::with_stroke(On, 1)).draw(&mut f);
    }
    for (i, &p) in w.rain.iter().enumerate() {
        let bar = (p.clamp(0.0, 100.0) / 100.0 * 10.0).round() as u32;
        rect(&mut f, x0 + 2 * i as i32, BOTTOM + 1 - bar as i32, 1, bar, PrimitiveStyle::with_fill(On));
    }
    rect(&mut f, x0, BOTTOM, 47, 1, PrimitiveStyle::with_fill(On)); // rain baseline, so 0% still reads as "no rain"
    f
}

/// One bar per CPU thread (4 px each on 32 threads), then CPU/GPU and RAM rows.
pub fn system(s: &Stats) -> Frame {
    let mut f = Frame::new();
    let threads = s.load.get(1..).unwrap_or(&[]);
    if !threads.is_empty() {
        let w = (W / threads.len()).max(1) as i32; // the 1 px gap after the last bar is the free column
        for (i, load) in threads.iter().enumerate() {
            let h = 1 + (load.clamp(0.0, 1.0) * 20.0).round() as i32; // 1 px floor so idle threads still show
            rect(&mut f, i as i32 * w, 21 - h, (w - 1).max(1) as u32, h as u32, PrimitiveStyle::with_fill(On));
        }
    }

    let cpu = s.load.first().copied().unwrap_or(0.0) * 100.0;
    let cpu_temp = s.cpu_temp.map(|t| format!(" {t:.0}°")).unwrap_or_default();
    text(&mut f, &SMALL, format_args!("CPU{cpu:4.0}%{cpu_temp}"), 0, 29, Align::Left);
    if let Some((temp, load)) = s.gpu {
        text(&mut f, &SMALL, format_args!("GPU{load:4}% {temp}°"), RIGHT, 29, Align::Right);
    }

    text(&mut f, &SMALL, format_args!("RAM {:.1}/{:.0}G", s.mem_used_gib, s.mem_total_gib), 0, BOTTOM, Align::Left);
    let used = if s.mem_total_gib > 0.0 { s.mem_used_gib / s.mem_total_gib } else { 0.0 };
    rect(&mut f, RIGHT - 47, 32, 48, 7, PrimitiveStyle::with_stroke(On, 1));
    rect(&mut f, RIGHT - 45, 34, (used.clamp(0.0, 1.0) * 44.0).round() as u32, 3, PrimitiveStyle::with_fill(On));
    f
}

/// 64 one-pixel bars mirrored around the middle row, bass in the centre (cava stereo).
/// Silence leaves a dotted centre line.
pub fn spectrum(bars: &[u8; crate::cava::BARS]) -> Frame {
    let mut f = Frame::new();
    let mid = BOTTOM / 2;
    for (i, &v) in bars.iter().enumerate() {
        let h = v as i32 * mid / 255;
        rect(&mut f, 2 * i as i32, mid - h, 1, 2 * h as u32 + 1, PrimitiveStyle::with_fill(On));
    }
    f
}
