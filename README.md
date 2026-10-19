# Budlink

Control your Google Pixel Buds from the Linux desktop: switch noise cancelling modes and check the
battery of each bud and the case. Built with GTK 4 and libadwaita.

> **Status:** early development. Not affiliated with or endorsed by Google.

## Features

- Noise control: Off, Noise Cancelling, Transparency and Adaptive (modes a model rejects are
  greyed out)
- Battery level and charging state for the left bud, right bud and case
- Follows changes made on the buds themselves (touch-and-hold gesture)
- Reconnects on its own when the buds come back
- Panel (tray) icon with battery and noise control, so the window can stay closed. Needs a
  desktop with StatusNotifierItem support: KDE, or GNOME with the AppIndicator extension
  (enabled by default on Ubuntu). Without it, closing the window quits the app.
- English and Spanish, selectable in Preferences (follows the system language by default)

## Supported devices

Budlink talks to the buds over Google's Maestro protocol, using
[`libmaestro`](https://github.com/qzed/pbpctrl) by Maximilian Luz.

| Device | Status |
|---|---|
| Pixel Buds 2a | Tested (no Adaptive mode: the buds reject it) |
| Pixel Buds Pro | Expected to work (supported by `libmaestro`), not tested yet |
| Pixel Buds Pro 2 | Untested |

If you try it with another model, please open an issue with the result.

## Building

Dependencies on Debian/Ubuntu:

```sh
sudo apt install cargo libgtk-4-dev libadwaita-1-dev libdbus-1-dev pkg-config protobuf-compiler
```

Requires Rust 1.92+, GTK 4 and libadwaita 1.7+. Then:

```sh
cargo build --release
./target/release/budlink
```

To install it for your user, with its menu entry and icon (in `~/.local`):

```sh
./install.sh
```

`./install.sh --uninstall` removes it again.

## Usage

1. Pair and connect the buds from the system Bluetooth settings.
2. Open Budlink. It finds the connected buds on its own.

Set `RUST_LOG=budlink=debug` for verbose logs.

## Known limitations

- **One controller at a time.** The buds accept a single control connection. If the Pixel Buds
  app (or the Pixel Buds settings) is open on a phone connected through multipoint, it takes the
  link away from Budlink. Budlink then shows a notice and backs off before retrying (up to one
  minute between attempts), so the two don't keep stealing the connection from each other. Close
  the phone app, or press *Try Again*, to reconnect right away.

## Credits

- [pbpctrl / libmaestro](https://github.com/qzed/pbpctrl) by Maximilian Luz: the reverse-engineered
  Maestro protocol that makes this possible.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
