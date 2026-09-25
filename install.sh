#!/bin/bash
# Installs apex-oled for the current user, building it first unless this is a
# release tarball with the binary already next to this script:
#   ~/.local/bin/apex-oled
#   ~/.config/systemd/user/apex-oled.service
#   /etc/udev/rules.d/71-apex-oled.rules   (the only step that needs sudo)
#
#   ./install.sh               install or update
#   ./install.sh --uninstall   blank the screen and remove all three
set -euo pipefail
cd "$(dirname "$0")"

bin="$HOME/.local/bin/apex-oled"
unit="$HOME/.config/systemd/user/apex-oled.service"
rule=/etc/udev/rules.d/71-apex-oled.rules
next_page="systemctl --user kill --kill-whom=main -s USR1 apex-oled"

if [[ $EUID -eq 0 ]]; then
  echo "Run this as your normal user; it asks for sudo only for the udev rule." >&2
  exit 1
fi

if [[ ${1:-} == --uninstall ]]; then
  systemctl --user stop apex-oled 2>/dev/null || true # stopping blanks the screen
  rm -f "$bin" "$unit"
  systemctl --user daemon-reload
  sudo rm -f "$rule"
  sudo udevadm control --reload
  echo "apex-oled removed."
  exit 0
fi

[[ -x /usr/bin/curl ]] || echo "note: /usr/bin/curl not found, so there will be no weather"
[[ -x /usr/bin/cava ]] || echo "note: /usr/bin/cava not found, so the spectrum page will stay flat"

if [[ -f apex-oled && -x apex-oled ]]; then
  built=./apex-oled # release tarball
elif command -v cargo >/dev/null; then
  cargo build --release --locked
  built=target/release/apex-oled
else
  echo "Building needs Rust 1.88 or newer (https://rustup.rs), or use a release tarball." >&2
  exit 1
fi
install -Dm755 "$built" "$bin"
install -Dm644 systemd/apex-oled.service "$unit"
systemctl --user daemon-reload

echo "Installing the udev rule (sudo):"
sudo install -Dm644 udev/71-apex-oled.rules "$rule"
sudo udevadm control --reload
# Re-announce an already-plugged keyboard so it gets the rule, which also starts the service.
sudo udevadm trigger --action=add --subsystem-match=hidraw --property-match=ID_VENDOR_ID=1038
sleep 1

if [[ -e /dev/apex-oled ]]; then
  systemctl --user restart apex-oled # picks up a new binary on updates
  echo
  systemctl --user --no-pager --lines=0 status apex-oled | head -3
else
  echo
  echo "No supported keyboard found. Plug it in and the display starts on its own."
fi

cat <<EOF

Switch pages with:
  $next_page

Bind it to a key, e.g.
  Omarchy (~/.config/hypr/bindings.lua):
    o.bind("SUPER + ALT + O", "Keyboard OLED: next page", "$next_page")
  Hyprland (hyprland.conf):
    bind = SUPER ALT, O, exec, $next_page
  Sway:
    bindsym \$mod+Mod1+o exec $next_page
EOF
