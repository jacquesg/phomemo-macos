# phomemo-macos

Native macOS CUPS driver for Phomemo thermal label printers.
Print from any application via the system print dialog — no
vendor software required.

## Supported Printers

| Model   | Print Head     | Connection |
|---------|----------------|------------|
| M02     | 48mm @ 203 dpi | USB, BLE   |
| M02 Pro | 53mm @ 300 dpi | USB, BLE   |
| M02S    | 53mm @ 300 dpi | USB, BLE   |
| T02     | 48mm @ 203 dpi | USB, BLE   |
| D30     | 40mm @ 203 dpi | USB, BLE   |
| M110    | 48mm @ 203 dpi | USB, BLE   |
| M120    | 48mm @ 203 dpi | USB, BLE   |
| M220    | 75mm @ 203 dpi | USB, BLE   |
| M421    | 70mm @ 203 dpi | USB, BLE   |

## Requirements

- macOS 12 (Monterey) or later
- [Rust toolchain](https://rustup.rs/) — only needed for
  `make install` or `make pkg` (Homebrew installs Rust
  automatically)

## Install

### Homebrew (recommended)

```sh
brew install jacquesg/phomemo/phomemo-macos
sudo phomemo-cups-setup
```

Builds from source, compiles PPDs, and installs a setup script.
The second command copies the driver files to the CUPS
directories and restarts CUPS. Then add your printer manually
(see [Adding a Printer](#adding-a-printer)).

### Installer package

Download `phomemo-macos-1.0.0.pkg` from the
[latest release](https://github.com/jacquesg/phomemo-macos/releases/latest).
The package is **not signed or notarised**. To install, remove
the quarantine attribute first:

```sh
xattr -d com.apple.quarantine phomemo-macos-1.0.0.pkg
sudo installer -pkg phomemo-macos-1.0.0.pkg -target /
```

The installer restarts CUPS. Then add your printer manually
(see [Adding a Printer](#adding-a-printer)).

### Development install

```sh
make install
```

Installs directly to the CUPS directories without building a
`.pkg`.

## Adding a Printer

USB printers appear automatically in **System Settings >
Printers & Scanners**. BLE printers must be added manually
with `lpadmin` — macOS restricts Bluetooth access from
background services, so CUPS cannot discover BLE devices
reliably.

### PPD files

Each printer model has its own PPD. Use the one matching your
printer:

| Model   | PPD file             |
|---------|----------------------|
| M02     | `Phomemo-M02.ppd`   |
| M02 Pro | `Phomemo-M02Pro.ppd` |
| M02S    | `Phomemo-M02S.ppd`  |
| T02     | `Phomemo-T02.ppd`   |
| D30     | `Phomemo-D30.ppd`   |
| M110    | `Phomemo-M110.ppd`  |
| M120    | `Phomemo-M120.ppd`  |
| M220    | `Phomemo-M220.ppd`  |
| M421    | `Phomemo-M421.ppd`  |

PPDs are installed to
`/Library/Printers/PPDs/Contents/Resources/`.

### USB

Phomemo printers appear as `/dev/cu.usbmodem*` serial devices.
List connected devices to find the path:

```sh
ls /dev/cu.usbmodem*
```

Example — adding an M110 at `/dev/cu.usbmodem14201`:

```sh
sudo lpadmin -p Phomemo-M110 -E \
  -v 'phomemo-serial:/dev/cu.usbmodem14201' \
  -P /Library/Printers/PPDs/Contents/Resources/Phomemo-M110.ppd
```

Replace the device path, printer name, and PPD with those
matching your setup.

### Bluetooth

The BLE backend connects directly to the printer by its
advertised name — no system-level pairing is required. The
name is printed on the label on the back of the printer
(e.g. `Q002E0CP0670069`).

Example — adding a D30 named `Q002E0CP0670069`:

```sh
sudo lpadmin -p Phomemo-D30 -E \
  -v 'phomemo-ble://Q002E0CP0670069' \
  -P /Library/Printers/PPDs/Contents/Resources/Phomemo-D30.ppd
```

Replace the BLE name, printer name, and PPD with those matching
your setup.

## Connection Types

### USB

Phomemo printers present as USB CDC serial devices at
`/dev/cu.usbmodemXXXX`. No additional USB driver is needed —
macOS includes CDC ACM support natively.

### Bluetooth Low Energy

The driver communicates directly via BLE GATT (service
`0xFF00`, characteristic `0xFF02`). The CUPS backend scans for
the printer by its advertised name and connects without
requiring system-level pairing.

The BLE backend handles macOS Bluetooth TCC permissions
transparently — it runs the BLE helper in the logged-in user's
context via `launchctl asuser`.

## Label Sizes

Select the label size in the print dialog under **Paper Size**.
Each PPD lists only the sizes available for that printer model.

### M02 / T02 / M02 Pro / M02S

50mm wide continuous roll. Heights: 10, 20, 25, 30, 40, 50, 60,
70, 75, 80, 90, 100, 110, 120, 125, 130, 140, 150mm.

### D30

The D30 prints sideways (90° rotation). Sizes are listed as
physical label dimensions (W x H):

| Size    | Description    |
|---------|----------------|
| 12x40mm | Standard label |
| 14x30mm | Narrow label   |

### M110 / M120

| Category          | Sizes (W x H mm)                              |
|-------------------|------------------------------------------------|
| Rectangular       | 20x10, 25x10, 25x15, 25x67, 30x15, 30x20,    |
|                   | 30x40, 35x15, 40x15, 40x20, 40x30, 40x60,    |
|                   | 40x70, 40x80, 45x15, 50x20, 50x30, 50x50,    |
|                   | 50x70, 50x80, 20x100                          |
| Round             | 20, 30, 40, 50mm                               |
| Jewellery / Cable | 25x30, 25x38 (+40mm tail), 30x25 (+45mm tail) |

### M220

All M110/M120 sizes, plus: 60x40, 60x60, 60x80, 60x86, 70x40,
70x70, 70x80mm.

### M421

| Width | Sizes (H mm)                |
|-------|-----------------------------|
| 40mm  | 15, 20, 30, 40, 60, 70, 80 |
| 45mm  | 15, 20, 60, 80             |
| 50mm  | 15, 20, 30, 50, 70, 80     |
| 60mm  | 40, 60, 80, 86             |
| 62mm  | 100                         |
| 70mm  | 40, 70, 80                 |
| 4x6"  | Imperial (default)          |

## Print Options

### Media Type

Available on **M110, M120, M220, and M421** only. Selectable
under **Media Type** in the print dialog:

- **Label With Gaps** (default) — pre-cut labels. The printer
  uses the gap sensor to detect label boundaries.
- **Continuous** — uncut roll. The printer feeds exactly the
  page height.
- **Label With Marks** — labels with black marks on the backing
  for alignment.

### Feed Lines

Available on **M02, T02, M02 Pro, M02S, and D30**. Controls how
many blank lines the printer feeds after each page for tearing.
Adjustable under **Feed Lines for Tearing** in the print dialog
(0-20, default varies by model).

## Architecture

```
CUPS Pipeline
─────────────

Application  (PDF, image, etc.)
    │
    ▼
┌──────────┐     ┌─────────────┐     ┌──────────────────┐
│ CUPS     │────▶│ Filter      │────▶│ Backend          │
│ rasteri- │     │ rastertopm* │     │ phomemo-serial   │
│ sation   │     │ rastertopd* │     │ phomemo-ble      │
└──────────┘     └─────────────┘     └──────────────────┘
  (RaS3)           (ESC/POS)                │
                                            ▼
                                     ┌──────────────┐
                                     │ Printer      │
                                     │ (USB / BLE)  │
                                     └──────────────┘
```

**Filters** convert CUPS raster data (RaS3) to the printer's
ESC/POS binary protocol. Three filters cover all models:

| Filter          | Models                   | Notes         |
|-----------------|--------------------------|---------------|
| `rastertopm110` | M110, M120, M220, M421   |               |
| `rastertopm02`  | M02, M02 Pro, M02S, T02  | Inverted bits |
| `rastertopd30`  | D30                      | 90° rotation  |

**Backends** handle device communication:

| Backend          | Transport | Helper         |
|------------------|-----------|----------------|
| `phomemo-serial` | USB CDC   | `phomemo-send` |
| `phomemo-ble`    | BLE GATT  | `phomemo-ble`  |

## Building

The build produces universal binaries (ARM64 + x86_64) that run
on both Apple Silicon and Intel Macs.

```sh
# Install both Rust targets (requires rustup)
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# Build everything
make all

# Build installer package
make pkg
```

## Testing

```sh
cargo test
```

The test suite includes:

- **Unit tests** — RaS3 parser, 1-bit packing, nearest-neighbour
  resize, 90° rotation
- **Integration tests** — each filter binary is fed synthetic
  RaS3 raster data and the ESC/POS output is verified
  byte-by-byte, covering:
  - Header structure and command sequences
  - Raster dimensions and left-padding alignment
  - Bit polarity (M110 normal, M02 inverted)
  - LineFeed byte substitution (M02)
  - Block chunking at 256-line boundaries (M02)
  - 90° rotation and dimension swap (D30)
  - Feed line encoding and footer structure
  - Golden-file regression (exact byte comparison)

## Uninstall

### Homebrew

```sh
sudo phomemo-cups-setup --uninstall
brew uninstall phomemo-macos
```

### Installer package / development install

```sh
make uninstall
```

Removes all filters, backends, PPDs, and registered printers.

## Troubleshooting

**Printer not discovered automatically:**

Check that the device exists:

```sh
ls /dev/cu.usbmodem*   # USB
```

If found, add manually with `lpadmin` (see above).

**Print job stuck in queue:**

```sh
cancel -a Phomemo-M110
sudo launchctl kickstart -k system/org.cups.cupsd
```

**Enable debug logging:**

```sh
sudo cupsctl LogLevel=debug
tail -f /var/log/cups/error_log
# After debugging, reset:
sudo cupsctl LogLevel=warn
```

**Bluetooth printer not found:**

Ensure the printer is powered on and within range. BLE
discovery requires an active user session (not just a locked
screen). Check that Bluetooth is enabled in System Settings.

## Acknowledgements

This project builds on protocol knowledge from the open-source
community:

- **[phomemo-tools](https://github.com/vivier/phomemo-tools)**
  by Laurent Vivier — original reverse engineering of the
  Phomemo printer protocols, Linux CUPS driver, and PPD
  definitions
- **Yury Chuyko** — M421 printer support and label size
  definitions
- **[phomemo-d30](https://github.com/crabdancing/phomemo-d30)**
  by crabdancing — independent D30 protocol documentation

## Licence

[MIT](LICENSE)

Copyright (c) 2026 Jacques Germishuys
