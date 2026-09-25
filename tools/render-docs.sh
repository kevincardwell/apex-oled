#!/bin/bash
# Renders the README images from the real pages via `apex-oled --preview`:
# docs/{clock,weather,system,spectrum}.png and docs/demo.gif.
# Needs ImageMagick 7, a weather location, and audio playing for the spectrum.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --quiet
bin=target/release/apex-oled
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Each OLED pixel becomes a 4x4 dot with a 1 px gap, lit slightly cool, with a soft glow,
# centred on a dark rounded bezel with transparent corners.
magick -size 5x5 xc:black -fill white -draw 'rectangle 0,0 3,3' "$tmp/cell.png"
magick -size 640x200 tile:"$tmp/cell.png" "$tmp/grid.png"
magick -size 704x264 xc:none +antialias -fill '#0c0c0e' -stroke '#34343a' -strokewidth 2 \
  -draw 'roundrectangle 1,1 702,262 18,18' "$tmp/bezel.png"

style() { # in.png out.png
  magick "$1" -filter point -resize 500% "$tmp/grid.png" -compose multiply -composite \
    -fill '#eef4ff' -opaque white \( +clone -blur 0x3 -evaluate multiply 0.7 \) -compose screen -composite \
    "$tmp/bezel.png" +swap -gravity center -compose over -composite "$2"
}

frames() { # page count -> $tmp/<page>-NN.png, styled
  "$bin" --preview "$1" "$2" > "$tmp/$1.pbm"
  magick "$tmp/$1.pbm" "$tmp/$1-raw-%02d.png"
  for f in "$tmp/$1"-raw-*.png; do style "$f" "${f/-raw/}"; done
}

frames clock 3
frames weather 1
frames system 4
frames spectrum 90

mkdir -p docs
cp "$tmp/clock-00.png" docs/clock.png
cp "$tmp/weather-00.png" docs/weather.png
cp "$tmp/system-03.png" docs/system.png
cp "$tmp/spectrum-45.png" docs/spectrum.png

# The page cycle as Super+Alt+O shows it: 1 s per clock/system frame, 30 fps spectrum.
magick -dispose background \
  -delay 100 "$tmp"/clock-0?.png \
  -delay 250 "$tmp/weather-00.png" \
  -delay 100 "$tmp"/system-0?.png \
  -delay 3 "$tmp"/spectrum-??.png \
  -loop 0 -layers Optimize docs/demo.gif
ls -l docs
