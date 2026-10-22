# attack-shark

A Linux configuration tool for the **Attack Shark X11** mouse, written in Rust
with GTK 4 and libadwaita. It replaces the vendor's Windows-only
"Attack Shark Software". The protocol was recovered from that software; see
[PROTOCOL.md](PROTOCOL.md).

It talks to the 2.4 GHz receiver (`1d57:fa60`) or the wired connection
(`1d57:fa65`) over hidraw. No kernel driver is needed.

## Features

- DPI: up to 6 stages from 50 to 22,000 DPI, with per-stage colour, enable
  switch and active stage
- Polling rate (125–1000 Hz), ripple control, angle snapping, button debounce
- Button remapping for left, right, middle, forward and back: mouse
  functions, DPI cycling, media and browser keys, keyboard shortcuts, macros
- Macro editor: record from the keyboard, insert mouse clicks, edit delays,
  repeat N times / until a key press / while held
- Lighting: effect, colour, brightness, speed
- Power: light-off and deep-sleep timers, live battery level
- Profiles: import/export as JSON, reset to factory defaults
- CLI: `--read` prints the mouse's settings, `--apply profile.json` writes a profile

Settings live on the mouse, so they keep working on other computers.

## Install

Build dependencies: Rust, GTK ≥ 4.12, libadwaita ≥ 1.6 (on Arch:
`pacman -S gtk4 libadwaita`).

```sh
cargo build --release
install -Dm755 target/release/attack-shark ~/.local/bin/attack-shark
install -Dm644 data/io.github.AttackSharkGtk.desktop ~/.local/share/applications/io.github.AttackSharkGtk.desktop
```

hidraw nodes are root-only by default. Install the udev rule so the logged-in
user can open the receiver:

```sh
sudo install -m644 udev/70-attack-shark.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=hidraw
```

## Usage

Start `attack-shark`. It reads the mouse's settings on connect. Change what
you like, then press **Apply**; only the sections that changed are written.

The mouse doesn't answer while it's asleep. Move it, then press the refresh
button.

The mouse can't report macros back, so the app keeps the last applied profile
in `~/.config/attack-shark/current.json` and restores macros from it.

```sh
attack-shark --read > my-profile.json
attack-shark --apply my-profile.json
```

## Other Attack Shark models

The vendor software supports 18 models on the same protocol. This tool
recognises them all by their model byte, but only the X11's limits have been
verified. For other models it reads settings and refuses to write. To add a
model, fill in its entry in `Model::from_id` (`src/protocol.rs`) with values
from the vendor app's `res/MS/MS_n/ms_n_page.xml`.

## Not supported

- Motion sync and lift-off distance. The vendor app has these settings for
  the X11, but hides them and never sends them to the mouse.
- Bluetooth mode. The vendor app talks to Bluetooth mice through Windows'
  BLE APIs; this tool only handles the receiver and the cable.
