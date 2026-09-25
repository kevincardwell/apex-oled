#!/usr/bin/env python3
"""Apex 7 OLED hardware check: times a burst of writes, then leaves a test card up.

    oled-test.py          animate + time, then show the test card
    oled-test.py clear    blank the screen
"""
import fcntl
import os
import sys
import time

W, H = 128, 40
NODE = "/dev/apex-oled"  # symlink from udev/71-apex-oled.rules


def hidiocsfeature(n):  # _IOC(_IOC_READ|_IOC_WRITE, 'H', 0x06, n)
    return (3 << 30) | (n << 16) | (ord("H") << 8) | 0x06


def frame(pixels):
    # 0x61 command byte + 128x40 row-major, MSB = leftmost pixel + 0x00 pad = 642 bytes
    buf = bytearray(2 + W * H // 8)
    buf[0] = 0x61
    for x, y in pixels:
        buf[1 + y * (W // 8) + x // 8] |= 0x80 >> (x % 8)
    return bytes(buf)


def send(fd, f):
    fcntl.ioctl(fd, hidiocsfeature(len(f)), f)


def test_card():
    px = {(x, y) for x in range(W) for y in (0, H - 1)}
    px |= {(x, y) for y in range(H) for x in (0, W - 1)}
    px |= {(x, y) for x in range(2, 10) for y in range(2, 10)}  # solid square = top-left
    px |= {(x, x * (H - 1) // (W - 1)) for x in range(W)}  # diagonal top-left -> bottom-right
    px |= {(x, y) for x in range(96, 124) for y in range(4, 36) if (x // 2 + y // 2) % 2}  # checkerboard
    return frame(px)


def bouncing_box(i):
    x = abs((i * 3) % (2 * (W - 12)) - (W - 12))
    y = abs((i * 2) % (2 * (H - 12)) - (H - 12))
    return frame({(x + dx, y + dy) for dx in range(12) for dy in range(12)})


def main():
    fd = os.open(NODE, os.O_RDWR)
    if sys.argv[1:] == ["clear"]:
        send(fd, frame(()))
        return
    frames = [bouncing_box(i) for i in range(200)]
    times = []
    for f in frames:
        t = time.perf_counter()
        send(fd, f)
        times.append(time.perf_counter() - t)
    times.sort()
    print(f"{len(times)} writes: median {times[len(times) // 2] * 1e3:.2f} ms, "
          f"p95 {times[int(len(times) * 0.95)] * 1e3:.2f} ms, max {times[-1] * 1e3:.2f} ms "
          f"-> ~{1 / times[len(times) // 2]:.0f} fps ceiling")
    send(fd, test_card())
    print("test card up: border, solid square top-left, diagonal, checkerboard on the right")


if __name__ == "__main__":
    assert len(frame(())) == 642 and frame({(0, 0)})[1] == 0x80 and frame({(127, 39)})[640] == 0x01
    main()
