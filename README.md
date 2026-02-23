# LIGHTCRAZY

Linux control software for the Pulsar X2 CrazyLight.

This project was created solely for personal use. But I figured I might aswell
release it on the off-chance that someone else finds it useful!

## Features

- DPI: 400 / 800 / 1600 / 3200 / 6400 / 12800
- Polling rate: 125 – 8000 Hz
- Lift-off distance: Low (0.7 mm), Medium (1 mm), High (2 mm)
- Debounce time: 0 – 20 ms
- Toggle: Angle Snap, Ripple Control, Motion Sync, Turbo Mode
- Battery level and charging status via system tray
- Low-battery desktop notifications (configurable threshold & alarm)
- Terminal UI for settings (`--options`)

## Installation

### NixOS

```nix
# flake.nix
inputs.lightcrazy = {
  url = "github:cnsta/lightcrazy";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

```nix
# configuration.nix
hardware.lightcrazy = {
  enable = true;        # installs package + udev rules
  service = {
    enable = true;      # systemd user service
  };
};
```

### Build from source

```bash
cargo build --release
```

Runtime dependencies: `libudev`, `libdbus`. On NixOS these are handled by the
package derivation.

## Usage

```bash
lightcrazy            # start tray service (default)
lightcrazy --options  # open settings panel (starts tray if not running)
```

Settings are stored in `~/.config/lightcrazy/settings.json` and applied to the
device on startup.

## USB permissions

On non-NixOS systems, create `/etc/udev/rules.d/99-lightcrazy.rules`:

```
SUBSYSTEM=="usb",    ATTRS{idVendor}=="3710", ATTRS{idProduct}=="3414", MODE="0666", TAG+="uaccess"
SUBSYSTEM=="usb",    ATTRS{idVendor}=="3710", ATTRS{idProduct}=="5406", MODE="0666", TAG+="uaccess"
KERNEL=="hidraw*",   ATTRS{idVendor}=="3710", ATTRS{idProduct}=="3414", MODE="0666", TAG+="uaccess"
KERNEL=="hidraw*",   ATTRS{idVendor}=="3710", ATTRS{idProduct}=="5406", MODE="0666", TAG+="uaccess"
```

```bash
sudo udevadm control --reload-rules && sudo udevadm trigger
```

## Troubleshooting

Since I exclusively use NixOS and Hyprland, these are the only environments that
have been thoroughly tested. Feel free to open an issue if you encounter bugs.

**Device not found**: check `lsusb | grep 3710`, verify udev rules

**Tray not visible**: requires a StatusNotifier-compatible desktop (KDE, GNOME
with AppIndicator extension, most others). Check
`journalctl --user -u lightcrazy`.

**Settings panel opens in wrong terminal**: set `TERMINAL` or `TERM` in your
environment.

## Credits

- [Elehiggle](https://github.com/Elehiggle/SimplePulsarBatteryNotification) -
  protocol reverse engineering and battery command sequences
- [NotAShelf/tailray](https://github.com/NotAShelf/tailray) - tray architecture
  reference

## Disclaimer

This project is not affiliated with Pulsar Gaming Gears or AplusX Inc.
