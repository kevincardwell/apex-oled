use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use std::{fs::File, io, os::fd::AsRawFd};

pub const W: usize = 128;
pub const H: usize = 40;
const LEN: usize = 2 + W * H / 8;

/// One screen as the keyboard wants it: 0x61 command byte, 128x40 pixels
/// row-major with the leftmost pixel in the MSB, then a 0x00 pad = 642 bytes.
#[derive(Clone, PartialEq)]
pub struct Frame(pub [u8; LEN]);

impl Frame {
    pub fn new() -> Self {
        let mut b = [0; LEN];
        b[0] = 0x61;
        Frame(b)
    }

    fn set(&mut self, x: usize, y: usize, on: bool) {
        let (i, bit) = (1 + y * W / 8 + x / 8, 0x80 >> (x % 8));
        if on { self.0[i] |= bit } else { self.0[i] &= !bit }
    }

    /// Moves the image one pixel right and/or down. Pages keep the last column and row
    /// free so nothing falls off; cycling this spreads wear from static pixels.
    pub fn shift(&mut self, right: bool, down: bool) {
        let px = &mut self.0[1..LEN - 1];
        if down {
            px.copy_within(..(H - 1) * W / 8, W / 8);
            px[..W / 8].fill(0);
        }
        if right {
            for row in px.chunks_mut(W / 8) {
                let mut carry = 0;
                for b in row {
                    (*b, carry) = ((*b >> 1) | (carry << 7), *b & 1);
                }
            }
        }
    }

    /// Binary PBM, inverted so lit pixels come out white like on the keyboard.
    pub fn pbm(&self) -> Vec<u8> {
        let mut out = format!("P4\n{W} {H}\n").into_bytes();
        out.extend(self.0[1..LEN - 1].iter().map(|b| !b));
        out
    }
}

impl OriginDimensions for Frame {
    fn size(&self) -> Size {
        Size::new(W as u32, H as u32)
    }
}

impl DrawTarget for Frame {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<BinaryColor>>>(&mut self, pixels: I) -> Result<(), Self::Error> {
        for Pixel(p, c) in pixels {
            if (0..W as i32).contains(&p.x) && (0..H as i32).contains(&p.y) {
                self.set(p.x as usize, p.y as usize, c.is_on());
            }
        }
        Ok(())
    }
}

/// The keyboard's OLED interface (hidraw, USB interface 1).
pub struct Oled(File);

impl Oled {
    pub fn open(path: &str) -> io::Result<Self> {
        File::options().read(true).write(true).open(path).map(Oled)
    }

    pub fn send(&self, f: &Frame) -> io::Result<()> {
        // HIDIOCSFEATURE(642) = _IOC(_IOC_READ | _IOC_WRITE, 'H', 0x06, 642)
        let req = (3 << 30) | (LEN << 16) | ((b'H' as usize) << 8) | 0x06;
        if unsafe { libc::ioctl(self.0.as_raw_fd(), req as _, f.0.as_ptr()) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_packing() {
        let mut f = Frame::new();
        f.set(0, 0, true);
        f.set(127, 39, true);
        assert_eq!((f.0[0], f.0[1], f.0[640], f.0[641]), (0x61, 0x80, 0x01, 0x00));

        let mut g = Frame::new();
        g.set(7, 0, true); // crosses a byte boundary when shifted right
        g.shift(true, true);
        assert_eq!((g.0[1], g.0[1 + 16], g.0[1 + 17]), (0x00, 0x00, 0x80));
    }
}
